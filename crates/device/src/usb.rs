//! USB access to real machines (experimental: not run on hardware yet).
//!
//! The Silhouette machines are USB printer-class devices with one bulk endpoint in each
//! direction. On Linux the kernel's `usblp` driver usually claims them first; opening detaches
//! it. The user needs write access to the device node, which the udev rule in
//! `packaging/linux/70-bladepilot.rules` grants.

use std::io::{Read, Write};
use std::time::Duration;

use bladepilot_machines::{Machine, VENDOR_IDS, by_usb};
use nusb::MaybeFuture;
use nusb::descriptors::TransferType;
use nusb::transfer::{Bulk, Direction, In, Out};

use crate::{Error, Transport};

/// A machine found on the USB bus.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// The model, when BladePilot knows the product id.
    pub machine: Option<&'static Machine>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub bus: String,
    pub address: u8,
}

fn usb_error(e: impl std::fmt::Display) -> Error {
    Error::Usb(e.to_string())
}

/// Lists the cutting machines on the USB bus.
pub fn scan() -> Result<Vec<Found>, Error> {
    let devices = nusb::list_devices().wait().map_err(usb_error)?;
    Ok(devices
        .filter(|d| VENDOR_IDS.contains(&d.vendor_id()))
        .map(|d| Found {
            machine: by_usb(d.vendor_id(), d.product_id()),
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            product: d.product_string().map(str::to_string),
            serial: d.serial_number().map(str::to_string),
            bus: d.bus_id().to_string(),
            address: d.device_address(),
        })
        .collect())
}

/// Opens a machine found by [`scan`]. On Windows the printer class driver usually holds the
/// machine, so that route is tried first; everywhere else, and as a fallback, the machine is
/// claimed directly.
pub fn open(found: &Found) -> Result<Box<dyn Transport + Send>, Error> {
    #[cfg(windows)]
    {
        let ports = crate::usbprint::ports();
        if let Some(port) = ports.iter().find(|p| p.vendor_id == found.vendor_id && p.product_id == found.product_id) {
            return Ok(Box::new(crate::usbprint::UsbPrint::open(&port.path)?));
        }
    }
    Ok(Box::new(UsbTransport::open(found)?))
}

/// Reads `vid_xxxx` and `pid_xxxx` from a Windows device path such as
/// `\\?\usb#vid_0b4d&pid_113a#…#{28d78fad-…}`.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_printer_path_ids(path: &str) -> Option<(u16, u16)> {
    let lower = path.to_ascii_lowercase();
    let hex_after = |tag: &str| {
        let at = lower.find(tag)? + tag.len();
        lower.get(at..at + 4).and_then(|h| u16::from_str_radix(h, 16).ok())
    };
    Some((hex_after("vid_")?, hex_after("pid_")?))
}

/// A machine claimed directly over USB.
pub struct UsbTransport {
    writer: nusb::io::EndpointWrite<Bulk>,
    reader: nusb::io::EndpointRead<Bulk>,
}

impl std::fmt::Debug for UsbTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsbTransport").finish_non_exhaustive()
    }
}

impl UsbTransport {
    /// Opens a machine found by [`scan`].
    pub fn open(found: &Found) -> Result<UsbTransport, Error> {
        let info = nusb::list_devices()
            .wait()
            .map_err(usb_error)?
            .find(|d| d.bus_id() == found.bus && d.device_address() == found.address)
            .ok_or_else(|| Error::Usb("the machine is no longer connected".to_string()))?;
        let device = info.open().wait().map_err(usb_error)?;
        let config = device.active_configuration().map_err(usb_error)?;
        let mut chosen = None;
        'search: for interface in config.interfaces() {
            for alt in interface.alt_settings() {
                let (mut ep_out, mut ep_in) = (None, None);
                for ep in alt.endpoints().filter(|ep| ep.transfer_type() == TransferType::Bulk) {
                    match ep.direction() {
                        Direction::Out => ep_out = ep_out.or(Some(ep.address())),
                        Direction::In => ep_in = ep_in.or(Some(ep.address())),
                    }
                }
                if let (Some(o), Some(i)) = (ep_out, ep_in) {
                    chosen = Some((interface.interface_number(), o, i));
                    break 'search;
                }
            }
        }
        let (number, ep_out, ep_in) = chosen.ok_or_else(|| Error::Usb("the device has no bulk endpoints".to_string()))?;
        let interface = device.detach_and_claim_interface(number).wait().map_err(usb_error)?;
        let mut writer = interface.endpoint::<Bulk, Out>(ep_out).map_err(usb_error)?.writer(4096);
        writer.set_write_timeout(Duration::from_secs(60));
        let reader = interface.endpoint::<Bulk, In>(ep_in).map_err(usb_error)?.reader(512);
        Ok(UsbTransport { writer, reader })
    }
}

impl Transport for UsbTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.writer.write_all(bytes).map_err(usb_error)?;
        self.writer.flush().map_err(usb_error)
    }

    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, Error> {
        self.reader.set_read_timeout(timeout.max(Duration::from_millis(1)));
        let mut buf = [0u8; 512];
        match self.reader.read(&mut buf) {
            Ok(n) => Ok(buf[..n].to_vec()),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(Vec::new()),
            Err(e) => Err(usb_error(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printer_paths_give_their_usb_ids() {
        let path = r"\\?\usb#vid_0b4d&pid_113a#7&1a2b3c4d&0&2#{28d78fad-5a12-11d1-ae5b-0000f803a8c2}";
        assert_eq!(parse_printer_path_ids(path), Some((0x0b4d, 0x113a)));
        assert_eq!(parse_printer_path_ids(&path.to_uppercase()), Some((0x0b4d, 0x113a)));
        assert_eq!(parse_printer_path_ids(r"\\?\usb#vid_zz#x"), None);
    }
}
