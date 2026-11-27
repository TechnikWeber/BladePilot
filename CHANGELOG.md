# Changelog

## 0.1.0 – 2026-11-27

First preview, for trying BladePilot without a machine. Cutting on a real machine has not been
tested yet: use `--simulate` and `--dry-run`, and `--experimental-usb` only on a hardware test
day, with a pen on paper first.

- Opens SVG and Silhouette Studio `.studio` / `.studio3` files (outlines, line colours, page
  size); protected store designs are refused.
- Plans jobs: one pass or one per colour, pens before blades, holes before the shape around
  them, nearest next start, check against the mat.
- Writes the machine program for the Portrait 3 and 4 and the other Silhouette machines.
- Virtual plotter that answers like a machine and draws the job as SVG.
- `bladepilot replay` reads USB captures from USBPcap (Windows) and usbmon (Linux).
- USB: Linux and macOS through nusb, Windows through the printer class driver Silhouette Studio
  uses; not yet tested on hardware.
