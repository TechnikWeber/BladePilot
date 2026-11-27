//! Windows: the machine through the USB printer class driver (experimental, not run on hardware
//! yet).
//!
//! Windows binds Silhouette machines to its USB printer class driver (`usbprint.sys`). Talking
//! to the driver's device interface, as the vendor software does, needs no driver change, so
//! BladePilot and Silhouette Studio can use the machine in turn.
//!
//! This is the only place in BladePilot with `unsafe`: the Win32 calls. Every buffer handed to
//! an overlapped read or write stays alive until that operation has completed or been cancelled.

#![allow(unsafe_code)]

use std::time::Duration;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows_sys::core::GUID;

use crate::{Error, Transport};

/// Device interface class of USB printers.
const GUID_DEVINTERFACE_USBPRINT: GUID = GUID::from_u128(0x28d78fad_5a12_11d1_ae5b_0000f803a8c2);
/// Longest single write; the driver splits it into USB packets.
const WRITE_CHUNK: usize = 4096;
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// A USB printer interface: its device path and the USB ids in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
}

/// Every USB printer interface Windows currently has.
pub fn ports() -> Vec<Port> {
    let mut found = Vec::new();
    // SAFETY: plain SetupAPI enumeration. The detail buffer is allocated with the size the first
    // call reports, aligned for the struct, and read only within that size.
    unsafe {
        let set = SetupDiGetClassDevsW(&GUID_DEVINTERFACE_USBPRINT, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        if set == INVALID_HANDLE_VALUE as isize {
            return found;
        }
        let mut index = 0;
        loop {
            let mut data: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            if SetupDiEnumDeviceInterfaces(set, std::ptr::null(), &GUID_DEVINTERFACE_USBPRINT, index, &mut data) == 0 {
                break;
            }
            index += 1;
            let mut required = 0u32;
            SetupDiGetDeviceInterfaceDetailW(set, &data, std::ptr::null_mut(), 0, &mut required, std::ptr::null_mut());
            let path_offset = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
            if (required as usize) <= path_offset {
                continue;
            }
            let mut buffer = vec![0u64; (required as usize).div_ceil(8)];
            let detail = buffer.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(set, &data, detail, required, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                continue;
            }
            let chars = (required as usize - path_offset) / 2;
            let text = std::slice::from_raw_parts(std::ptr::addr_of!((*detail).DevicePath).cast::<u16>(), chars);
            let len = text.iter().position(|&c| c == 0).unwrap_or(chars);
            let path = String::from_utf16_lossy(&text[..len]);
            if let Some((vendor_id, product_id)) = crate::usb::parse_printer_path_ids(&path) {
                found.push(Port { path, vendor_id, product_id });
            }
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    found
}

/// An open printer interface.
pub struct UsbPrint {
    handle: HANDLE,
    event: HANDLE,
}

// SAFETY: the handles are owned by this value and Win32 handles may be used from any thread.
unsafe impl Send for UsbPrint {}

impl std::fmt::Debug for UsbPrint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsbPrint").finish_non_exhaustive()
    }
}

fn last_error(what: &str) -> Error {
    // SAFETY: reads the calling thread's last-error value.
    Error::Usb(format!("{what} failed (Windows error {})", unsafe { GetLastError() }))
}

enum Done {
    Bytes(usize),
    TimedOut,
}

impl UsbPrint {
    /// Opens the printer interface at `path` for overlapped reading and writing.
    pub fn open(path: &str) -> Result<UsbPrint, Error> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(last_error("opening the machine (is Silhouette Studio still connected to it?)"));
        }
        // SAFETY: an unnamed manual-reset event; closed in Drop.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            let error = last_error("creating an event");
            // SAFETY: `handle` is valid and owned here.
            unsafe { CloseHandle(handle) };
            return Err(error);
        }
        Ok(UsbPrint { handle, event })
    }

    /// One overlapped read or write that waits at most `timeout`.
    fn transfer(&mut self, buffer: &mut [u8], write: bool, timeout: Duration) -> Result<Done, Error> {
        let len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: `overlapped` and `buffer` stay alive until the operation has completed: either
        // it finishes within the wait, or it is cancelled and its completion awaited below.
        unsafe {
            let mut overlapped: OVERLAPPED = std::mem::zeroed();
            overlapped.hEvent = self.event;
            let started = if write {
                WriteFile(self.handle, buffer.as_ptr(), len, std::ptr::null_mut(), &mut overlapped)
            } else {
                ReadFile(self.handle, buffer.as_mut_ptr(), len, std::ptr::null_mut(), &mut overlapped)
            };
            if started == 0 && GetLastError() != ERROR_IO_PENDING {
                return Err(last_error(if write { "writing to the machine" } else { "reading from the machine" }));
            }
            let waited = WaitForSingleObject(self.event, ms);
            let mut transferred = 0u32;
            if waited == WAIT_TIMEOUT {
                CancelIoEx(self.handle, &overlapped);
                GetOverlappedResult(self.handle, &overlapped, &mut transferred, 1);
                return Ok(if transferred > 0 { Done::Bytes(transferred as usize) } else { Done::TimedOut });
            }
            if waited != WAIT_OBJECT_0 || GetOverlappedResult(self.handle, &overlapped, &mut transferred, 0) == 0 {
                return Err(last_error("waiting for the machine"));
            }
            Ok(Done::Bytes(transferred as usize))
        }
    }
}

impl Drop for UsbPrint {
    fn drop(&mut self) {
        // SAFETY: both handles were created in `open` and are closed exactly once.
        unsafe {
            CancelIoEx(self.handle, std::ptr::null());
            CloseHandle(self.handle);
            CloseHandle(self.event);
        }
    }
}

impl Transport for UsbPrint {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
        for chunk in bytes.chunks(WRITE_CHUNK) {
            let mut rest = chunk.to_vec();
            while !rest.is_empty() {
                match self.transfer(&mut rest, true, WRITE_TIMEOUT)? {
                    Done::Bytes(0) | Done::TimedOut => return Err(Error::Timeout),
                    Done::Bytes(n) => {
                        rest.drain(..n.min(rest.len()));
                    }
                }
            }
        }
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, Error> {
        let mut buffer = vec![0u8; 512];
        match self.transfer(&mut buffer, false, timeout.max(Duration::from_millis(1)))? {
            Done::Bytes(n) => {
                buffer.truncate(n);
                Ok(buffer)
            }
            Done::TimedOut => Ok(Vec::new()),
        }
    }
}
