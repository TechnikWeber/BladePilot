# Roadmap

BladePilot is developed without the machine at hand: everything is built and tested against
the virtual plotter first, then checked against the real machine on a
[hardware test day](HARDWARE-DAY.md).

## Phase 0: foundations

- [x] Workspace, CI, MIT license, clean-room rules
- [x] Machine table: USB ids, protocol generation, cutting width, mats
- [x] Import SVG (usvg) and Silhouette Studio `.studio` / `.studio3` (craft-silhouette-studio)
- [x] Planner: passes by colour, pens before blades, holes before outlines, nearest next start,
      bounds check against the mat
- [x] Machine program via craft-plotter, golden tests for Portrait 3 and 4
- [x] Virtual plotter: status, version and tool replies, cut recording, warnings, SVG drawing
- [x] Job protocol over any transport, with registration-mark wait
- [x] Command line: `machines`, `devices`, `inspect`, `cut --dry-run`, `cut --simulate`
- [x] USB transport: nusb on Linux and macOS, the USB printer class driver on Windows (works
      next to Silhouette Studio without changing drivers); run on a Portrait 4 on Windows

## Phase 1: check everything that can be checked without hardware

- [ ] Second opinion: feed the same SVG through inkscape-silhouette's dry run and compare its
      command stream with BladePilot's (a test harness outside the repository's code)
- [x] `bladepilot replay CAPTURE`: reads pcap/pcapng from USBPcap (Windows) and usbmon (Linux),
      finds the machine, lists the conversation with repeated status polls folded, runs it
      through the virtual plotter and draws it; `--commands-only` for comparing two captures
- [x] udev rule for USB access on Linux (`packaging/linux/70-bladepilot.rules`)
- [x] Release builds for Linux, Windows and macOS on every tag
- [ ] Review the USB transport against the protocol notes; time-outs, unplugging, errors
- [x] Robustness tests: random and damaged input for the virtual plotter, the capture reader
      and the importers (fixed seed, runs in CI)

## Phase 2: Silhouette Studio files beyond outlines

- [ ] Test documents from Silhouette Studio (running under Wine is enough; no machine needed)
- [ ] Layers, fills, line styles and the cut settings stored in a document
- [ ] Text objects, groups and compound paths
- [ ] Contribute what is found back to `craft-silhouette-studio`

## Phase 3: desktop app

- [x] Tauri 2 app: Rust core, web front end (Svelte), long jobs off the UI thread
- [x] Mat view: place, move, rotate, scale, mirror, duplicate; snap to the mat; undo
- [x] Colour list: tool and settings per colour, skip a colour, pass order by drag, pauses
      between passes and for tool changes
- [x] Animated preview of the cutting order from the virtual plotter
- [x] Test cut: a 12 mm square with a triangle inside, with one pass's settings, on the next
      free spot of the mat; also `bladepilot test-cut`
- [x] Send dialog with machine status, progress, pause, stop and tool changes
- [x] English and German, light and dark, following the system
- [x] Mock backend: the front end runs in a browser, for screenshots and a later web demo
- [x] Bundles on GitHub Actions for every tag: AppImage, deb and rpm (x86_64, aarch64), Windows
      installer and msi, universal dmg
- [x] Open design files named on the command line; "Open with" for SVG on Linux
- [x] Pictures (PNG, JPEG, WebP, BMP, GIF): traced from transparency or darkness, with a
      threshold and specks ignored
- [x] Per design: its own lines, only the outermost lines, a contour at a distance, or nothing;
      single lines clicked away on the mat
- [x] PDF (vector paths of the first page) and DXF (lines, arcs, splines, blocks) import
- [x] Text in the system's fonts and simple shapes (rectangle, ellipse, polygon, star, heart),
      changeable after they are placed
- [x] Weeding box and weeding lines per design, a pass of their own cut last
- [x] Copies into free spots; arrange all designs onto little material (rows, first fit)
- [x] Mirror everything for heat transfer vinyl
- [x] Material profiles per colour: built-in starting values (BladePilot's own) and the user's
      own, saved under a name; heat transfer vinyl switches the mirror on
- [ ] File associations on Windows and macOS; remember the last session
- [ ] Web demo on GitHub Pages: the front end with the planner compiled to WebAssembly

## Phase 4: print and cut

- [x] Registration-mark layout and a printable PDF with the design and the marks (desktop app)
- [x] Contour at a chosen distance around printed stickers, holes filled
- [ ] Mark search with three marks: written, waits for the hardware test (T5); four marks and
      manual positioning later
- [ ] Print and cut on the command line
- [ ] Calibration sheet and stored offsets per machine

## Phase 5: hardware test days

- [x] First day (2026-11-27, Portrait 4): captures of Silhouette Studio and BladePilot for T0
      to T3 and Bluetooth; the facts are in [PROTOCOL.md](PROTOCOL.md)
- [x] Fixed: pen passes no longer send the blade depth; the tool is checked before passes;
      a job only counts as done once the machine was seen moving
- [x] Linux (2026-11-28): the desktop app over USB and Bluetooth on Fedora; jobs on a mat no
      longer shift the origin; Bluetooth search from the AppImage fixed
- [ ] Second day: tool change (T4), stopping a running job (T6), the desktop app (T7) over USB
      and Bluetooth, a long job to learn the machine's buffer, Studio's test cut (T8)
- [ ] First release that is no longer marked experimental

## Later

- [ ] Per-machine corrections for material profiles; profiles for more tools
- [ ] Long jobs on roll media without a mat, tiled when longer than the machine allows
- [ ] Inkscape extension "Send to BladePilot"
- [x] Bluetooth on Windows and Linux: the same GP-GL over Bluetooth Classic (RFCOMM channel 1),
      no pairing; a search in the command line (`devices --bluetooth`) and the desktop app
- [ ] Bluetooth on macOS (IOBluetooth); until then the app says to use USB
- [ ] Bluetooth Low Energy: find out what the Portrait 4 offers over GATT; if it carries the
      same bytes, a second transport (also a way to Bluetooth on macOS)
- [ ] Tools per pass instead of pens hidden in the colour list: action (cut, draw) and the
      machine's tools (AutoBlade, ratchet, deep-cut, kraft, rotary blade, sketch pen), material
      profiles per tool, a warning when the machine reports another tool
- [ ] Signed Windows builds: Smart App Control blocks a fresh unsigned release, above all a
      browser download (seen with 0.4.0's command line; the desktop installer got through).
      Free for open source through the SignPath Foundation, signing in the release workflow
- [ ] Move the head with the arrow keys before a job or a test cut
- [ ] Text in the PDF import (glyph outlines); DXF text and hatches
- [ ] Text: weld overlapping letters of script fonts into one outline
- [ ] Cameo-only features as experimental: second tool holder, electrostatic mat

## Open points in other projects BladePilot means to solve

Issues of [inkscape-silhouette](https://github.com/fablabnbg/inkscape-silhouette/issues) and
[cuthulhu](https://github.com/vcolombo/cuthulhu/issues) that matter for the Portrait machines or
for everyone:

| Topic | Issues elsewhere | Where in BladePilot |
|---|---|---|
| Registration marks not found, cuts shifted | inkscape-silhouette #378, #329, #32, #124, #206 | Phase 4 |
| Margins for printing the marks | inkscape-silhouette #265 | Phase 4 |
| Systematic offsets, homography correction | inkscape-silhouette #309, #166 | Phase 4 |
| Long cuts fail | inkscape-silhouette #295, #216 | Later, roll media |
| Tool settings stored with the design | inkscape-silhouette #323 | Phase 2 and 3 |
| Blade offset for other blades | inkscape-silhouette #285 | done: `--blade-offset` |
| Cut in a chosen order, pause between passes | inkscape-silhouette #283, #328 | done: manual pass order, pauses |
| UI freezes while cutting | inkscape-silhouette #111 | done: the job runs on its own thread |
| Installing the USB backend | inkscape-silhouette #349, #353, #30 | nusb needs no libusb; AppImage/RPM |
| Open `.studio` / `.studio3` | cuthulhu #347 | done for outlines; Phase 2 |
| Material-first profiles, test cut | cuthulhu #310 | done: material profiles per colour, test cut |
| Pause, resume, retry a pass | cuthulhu #306 | done: pause and resume; a pass is retried by leaving the others out |

## What Silhouette Studio does that BladePilot does not (yet)

From the vendor's public product information. BladePilot is cutting software: designs come
from Inkscape or any other editor, or from Silhouette Studio files; text and simple shapes for
the everyday job are made in the app.

| Silhouette Studio | BladePilot |
|---|---|
| Test cut | done |
| Cut by line colour; by fill colour; by layer | line colour, else fill colour: done; layers: phase 2 |
| Material list with blade depth, force, speed and passes per material | done: own starting values and own profiles, checked with the test cut (no vendor tables) |
| Print and cut with registration marks, PixScan | registration marks: written, to be tried on a machine; PixScan: not planned |
| Trace pictures, offset (contour) | trace from transparency or darkness, contour at a distance: done |
| Move the blade to a start point, feed and unload from the software | later; load and unload stay on the machine's buttons |
| Roller track enhancing, double cut, overcut | repeats and overcut: done; track enhancing: later |
| Tools: ratchet, kraft, rotary, deep-cut and punch blades, sketch pens, embossing (Curio), heat pen | blade and pen; tool-specific settings later |
| Two tool holders, electrostatic mat (Cameo) | later, experimental |
| Long cuts from the roll, tiling | later |
| Weeding lines and boxes, replicate and nesting to save material | done: weeding box and lines, copies, arrange in rows (no free-form nesting yet) |
| Mirror for heat transfer vinyl | done |
| Open PDF and DXF | done (vector paths; text in PDFs must be converted to paths) |
| Bluetooth | Windows and Linux: done, experimental; macOS later |
| Design tools: shapes, text, weld, knife, sketch fills | text and simple shapes: done; weld, knife, sketch fills: not planned, use Inkscape |
| Design store, library, cloud | out of scope |
| Firmware updates | out of scope |

## Out of scope

- Firmware updates: they stay with the vendor software.
- The vendor's design store and anything protected by it.
- Licence features of the vendor software.
