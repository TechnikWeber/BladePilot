# How BladePilot works

```text
 file ──► design ──► job ──────────────► program ──► device ──► transport
 (SVG,    (shapes in   (flatten, place,   (GP-GL,     (Session:   ├─ virtual plotter
 .studio3) mm, colours) group, order,      bytes)      init, wait, └─ USB (nusb)
                        check)                         send, wait)
```

## Crates

| Crate | Job |
|---|---|
| `bladepilot-machines` | What each machine is: USB ids, protocol generation, holders, cutting width, longest feed, mats. |
| `bladepilot-design` | Reads SVG with `usvg` and Silhouette Studio documents with `craft-silhouette-studio` into one model: Bézier paths in mm, `(x right, y down)` from the page's top-left corner, with outline and fill colours. Flattens curves into polylines. |
| `bladepilot-job` | Turns a design into a plan: flatten within a tolerance (0.02 mm by default), move onto the mat, group into passes, order, check against the cutting area. Encodes a plan with `craft-plotter`. |
| `bladepilot-sim` | The virtual plotter. Parses the byte stream like a machine, keeps the head position, answers queries, records cuts and lifted moves, warns, draws SVG. |
| `bladepilot-device` | The `Transport` trait and the `Session` that runs a job: initialise, wait until ready, stream, wait for marks, wait until done. Transports: the virtual plotter; USB through nusb on Linux and macOS; on Windows the USB printer class driver (`usbprint.sys`), which Silhouette Studio uses too, so no driver has to be replaced. |
| `bladepilot-capture` | Reads USB captures (pcap, pcapng) from USBPcap and usbmon, finds the machine and splits its traffic into commands and replies, for `bladepilot replay`. |
| `bladepilot-cli` | The `bladepilot` command line. |

## Decisions

- **The encoder does no I/O.** `craft-plotter` turns a job into commands and bytes; opening
  the device, writing and waiting belong to `bladepilot-device`. Every program can be checked
  as text, and the golden tests pin exactly what a machine receives.
- **The virtual plotter is a second implementation.** It is written from
  [PROTOCOL.md](PROTOCOL.md), not from the encoder, so a misunderstanding on one side shows up
  as a difference. The round-trip tests plan a design, run it through the session and the
  virtual plotter, and require the recorded cuts to match the plan to the machine's 0.05 mm
  step.
- **Order of cuts.** Pen passes come before blade passes, so nothing is drawn on material that
  is already cut loose. Shapes inside other shapes come first, so a piece is never cut free
  before its holes: between passes by the deepest nesting a pass contains, within a pass path
  by path. Within one nesting level the head goes to the nearest next start; open paths may be
  cut backwards when their end is nearer.
- **Only one place with `unsafe`.** The Windows transport calls Win32 directly; the rest of the
  workspace denies `unsafe` code.
- **Units.** Millimetres everywhere in the model, `(x right, y down)`. The machine counts in
  0.05 mm steps with the feed axis first; only the encoder and the virtual plotter know that.
- **Safety first on hardware.** Sending to a real machine needs `--experimental-usb` until the
  USB path has passed a hardware test day.

## Dependencies and licences

| Crate | Licence | Used for |
|---|---|---|
| craft-plotter, craft-silhouette-studio | MIT OR Apache-2.0 | GP-GL program, Studio documents |
| usvg | Apache-2.0 OR MIT | SVG import |
| kurbo | Apache-2.0 OR MIT | Curves, flattening |
| nusb | Apache-2.0 OR MIT | USB without libusb |
| clap, anyhow, thiserror | Apache-2.0 OR MIT | Command line, errors |
