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
- [x] USB transport written, not yet run on hardware: nusb on Linux and macOS, the USB printer
      class driver on Windows (works next to Silhouette Studio without changing drivers)

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

- [ ] Tauri 2 app: Rust core, web front end
- [ ] Mat view: place, move, rotate, scale, mirror, duplicate; snap to the mat
- [ ] Colour list: material, tool and settings per colour, skip a colour, pass order by drag
- [ ] Send dialog with live machine status, progress, pause and cancel
- [ ] English and German, following the system language
- [ ] AppImage and RPM built on GitHub Actions for tagged releases

## Phase 4: print and cut

- [ ] Registration-mark layout and a printable PDF with the design and the marks
- [ ] Mark search with three or four marks, manual positioning as fallback
- [ ] Calibration sheet and stored offsets per machine

## Phase 5: hardware test day

- [ ] Captures of Silhouette Studio and BladePilot for the test jobs in
      [HARDWARE-DAY.md](HARDWARE-DAY.md)
- [ ] Fix the differences, record the open protocol questions in [PROTOCOL.md](PROTOCOL.md)
- [ ] First release

## Later

- [ ] Material profiles with a test cut and per-machine corrections
- [ ] Long jobs on roll media without a mat, tiled when longer than the machine allows
- [ ] Inkscape extension "Send to BladePilot"
- [ ] Bluetooth
- [ ] DXF and PDF import
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
| Cut in a chosen order, pause between passes | inkscape-silhouette #283, #328 | Phase 3 |
| UI freezes while cutting | inkscape-silhouette #111 | Phase 3: cutting runs off the UI thread |
| Installing the USB backend | inkscape-silhouette #349, #353, #30 | nusb needs no libusb; AppImage/RPM |
| Open `.studio` / `.studio3` | cuthulhu #347 | done for outlines; Phase 2 |
| Material-first profiles, test cut | cuthulhu #310 | Later |
| Pause, resume, retry a pass | cuthulhu #306 | Phase 3 |

## Out of scope

- Firmware updates: they stay with the vendor software.
- The vendor's design store and anything protected by it.
- Licence features of the vendor software.
