# Changelog

## 0.5.1 – 2026-11-27

- The command-line downloads are called `bladepilot-cli-…` now (AppImage, Windows `.exe` and
  `.zip`, macOS and Linux archives), so they are not mistaken for the desktop app
  (`BladePilot-Desktop-…`). Its AppImage shows up as "BladePilot (command line)".
- The README says what is tested: cutting on a Portrait 4 over USB and Bluetooth.

## 0.5.0 – 2026-11-27

- On Linux the desktop app checks at start whether a udev rule gives you USB access to the
  machine. Without one (AppImage, build from source) it offers to set it up, asking for your
  password through pkexec, or shows the command to paste into a terminal, with a copy button.
  The machine panel keeps a reminder until the rule is there.
- BladePilot's own captures of the Portrait 4 are published as test data (USB as pcap, Bluetooth
  as a text transcript), with tests that replay them; PROTOCOL.md describes the vendor software
  only roughly.

## 0.4.0 – 2026-11-27

After the first hardware test day with a Portrait 4: sending over USB works, and Bluetooth is new.
Sending to a machine is still marked experimental.

- **Bluetooth** on Windows and Linux: the machine speaks the same protocol over Bluetooth
  Classic and needs no pairing. `bladepilot devices --bluetooth` searches and prints the command
  to use; `--experimental-bluetooth [ADDRESS]` sends (without an address it searches first). The
  desktop app has a Bluetooth search next to the USB one and keeps one connection open for status
  and jobs, because the machine takes up to about 40 s after a disconnect before it accepts the
  next one.
- **Tool check:** before the first pass and after a tool change BladePilot asks the machine which
  tool is in the holder and waits once when a pen pass finds a blade or the other way round.
- **Fixed:** a pen pass of a two-colour job sent the blade's depth and force; the machine would
  have run its AutoBlade setting with the pen in the holder. Pens never get a depth now, and
  `--pen-force` sets their force (default: the machine's own).
- **Fixed:** a job counted as done when the machine answered "ready" before it had started (over
  Bluetooth replies come back within 20 ms), and the connection was closed too early.
- Late replies are dropped before each query, and a query without a reply is asked once more.
- [PROTOCOL.md](docs/PROTOCOL.md) has what the Portrait 4 answered and what Silhouette Studio
  sends; [HARDWARE-DAY.md](docs/HARDWARE-DAY.md) the results and how to capture the machine alone.

## 0.3.0 – 2026-11-27

- **Test cut:** a 12 mm square with a triangle inside, cut with one pass's settings to try
  force and depth before the real job. In the desktop app every pass has a **Test cut** button;
  the test goes to the next free spot of the mat, which the mat shows, and the next test lands
  beside it. On the command line: `bladepilot test-cut --at X,Y` with the usual settings.
- The README says what BladePilot does differently from the related projects, and the roadmap
  what Silhouette Studio does that BladePilot does not do yet.
- The hardware test day checks the test cut and what the machine offers over Bluetooth.

## 0.2.0 – 2026-11-27

The desktop app. Cutting on a real machine has still not been tested: send to the virtual
plotter, and to a machine only on a hardware test day, with a pen on paper first.

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
- The desktop app opens design files named on its command line; on Linux it offers itself for
  SVG files in the file manager and installs the udev rule for USB access.
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
