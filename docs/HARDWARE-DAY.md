# Hardware test day

BladePilot is built against the virtual plotter. On a hardware test day the real machine
answers the open questions in [PROTOCOL.md](PROTOCOL.md): the same test jobs are recorded once
from Silhouette Studio and once from BladePilot, and the two USB captures are compared.

## Bring

- The machine (Portrait 3 or 4), its USB cable, a mat, plain paper, light card
- A Silhouette sketch pen, or a pen in a pen adapter, for the first runs
- The Windows PC with Silhouette Studio
- A USB stick to carry the captures to the Linux computer

## Prepare the Windows PC

1. Install Silhouette Studio and connect the machine once, so Studio offers its firmware
   update now and not in the middle of a test. Note Studio's version and the firmware version
   it shows.
2. Install [Wireshark](https://www.wireshark.org/download.html) and tick **USBPcap** in the
   installer. Restart Windows afterwards.
3. Optional, to have Claude Code on that PC: install Git and Claude Code, then
   `git clone https://github.com/TechnikWeber/BladePilot`, start `claude` in that folder and
   say: "Hardware test day: read CLAUDE.md and docs/HARDWARE-DAY.md and guide me through the
   captures."

## The test jobs

Draw every shape in Studio by its numbers (Transform panel), so it matches the SVG BladePilot
cuts. Studio's Basic Edition cannot import SVG files; drawing by numbers works in every
edition. Save each Studio document as `.studio3`: those files are test material for the
importer as well.

| Job | Shape | Studio | BladePilot |
|---|---|---|---|
| T0 | none | Connect, load and unload the mat, wait 30 s | `bladepilot devices` |
| T1 | 20 × 20 mm square at X 10, Y 10 | Sketch pen, default settings | `cut hardware-tests/T1-square.svg --pen` |
| T2 | as T1 | AutoBlade on paper: note depth, force, speed | `cut hardware-tests/T1-square.svg --depth D --force F --speed S` |
| T3 | 40 × 40 mm square at X 5, Y 5, 20 mm circle in its centre | AutoBlade on paper | `cut hardware-tests/T3-label.svg` (with T2's settings) |
| T4 | as T3, circle red, square black | Cut by line colour: red sketch pen, black AutoBlade | `cut hardware-tests/T4-two-colours.svg --by-color --pen-color "#ff0000"` |
| T5 | 30 mm circle with registration marks | Print and cut: print the sheet, load it, cut | not yet: Studio only |
| T6 | as T2 | Start, then cancel after about a second | Ctrl+C after about a second |

Add `--machine portrait4` when the machine is a Portrait 4, and `--experimental-usb` to send to
the machine. The versions Studio shows also settle which Portrait it is.

## Capture on Windows

1. Start Wireshark and pick the **USBPcap** interface the machine hangs on. If unsure, start a
   capture on each one, switch the machine off and on, and keep the one that shows traffic.
2. Start the capture, run the job in Studio, wait until the machine stops, stop the capture.
3. Save as `T1-studio.pcapng`, `T2-studio.pcapng`, … and note the settings Studio used.

## Capture on Linux

1. Once: install the udev rule from `packaging/linux/70-bladepilot.rules` (instructions in the
   file) and Wireshark's command-line tools (`sudo dnf install wireshark-cli`).
2. `sudo modprobe usbmon`, then `lsusb` shows the bus the machine is on, e.g. `Bus 003`.
3. `sudo tshark -i usbmon3 -w re/captures/T1-bladepilot.pcapng`, run the BladePilot job in a
   second terminal, stop with Ctrl+C when the machine is done.

Captures go into `re/captures/`, which git ignores: captures of vendor software are not
published.

## Safety

- Pen on paper first. The first blade job runs on paper with low force.
- Keep a hand near the machine's power button for the first BladePilot jobs.
- Stop at the first unexpected movement and note what happened before trying again.

## Afterwards

`bladepilot replay` (see the [roadmap](ROADMAP.md)) draws each capture with the virtual plotter
and lists its commands, so Studio's and BladePilot's runs of the same job can be compared line
by line. The differences answer the open questions in [PROTOCOL.md](PROTOCOL.md) and turn into
fixes and new golden tests.
