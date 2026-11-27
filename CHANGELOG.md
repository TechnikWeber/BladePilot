# Changelog

## Unreleased

- **Desktop app** (Tauri 2, Svelte): a cutting mat with grid, rulers and cutting area; designs
  in their colours to move, turn, resize, mirror and duplicate, with snapping and undo; one
  pass per colour with tool, force, speed, depth and repeats; skip a colour, drag the passes
  into your own order; an animated preview of what the virtual plotter cuts; a send dialog
  with progress, pause, stop and tool changes. Light and dark, English and German, following
  the system.
- The planner takes a manual pass order and colours to leave out, and warns when a manual
  order cuts a piece loose before its inside is done.
- Jobs pause at the next lifted move, stop with the tool lifted and the mat moved out, and
  wait for the tool change between a pen and a blade pass. The command line asks for the
  tool change too.
- Release builds include the desktop app: AppImage, deb, rpm, Windows installer and macOS
  disk image.

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
