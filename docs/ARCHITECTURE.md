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
| `bladepilot-design` | Reads SVG with `usvg` and Silhouette Studio documents with `craft-silhouette-studio` into one model: Bézier paths in mm, `(x right, y down)` from the page's top-left corner, with outline and fill colours, and pictures with their traced edges. Flattens curves into polylines; works out contours at a distance and the outermost lines. |
| `bladepilot-job` | Turns a design into a plan: flatten within a tolerance (0.02 mm by default), move onto the mat, group into passes, order, check against the cutting area. Encodes a plan with `craft-plotter`. |
| `bladepilot-sim` | The virtual plotter. Parses the byte stream like a machine, keeps the head position, answers queries, records cuts and lifted moves, warns, draws SVG. |
| `bladepilot-device` | The `Transport` trait and the `Session` that runs a job: initialise, wait until ready, stream, wait for marks, wait until done. A `JobControl` pauses (at the next lifted move), resumes and stops (lift, park) a job from another thread, and the session pauses by itself before a pass that needs another tool. Transports: the virtual plotter; USB through nusb on Linux and macOS; on Windows the USB printer class driver (`usbprint.sys`), which Silhouette Studio uses too, so no driver has to be replaced. |
| `bladepilot-trace` | Outlines from pixels: a picture's transparency or darkness cut at a threshold, specks and pinholes removed, holes filled; marching squares with sub-pixel precision for the edges, a Euclidean distance field for contours at a distance. |
| `bladepilot-print` | Print and cut: the registration-mark layout (a square at the top left, brackets at the top right and bottom left), the keep-out areas around the marks, and the sheet as a PDF with the pictures and the marks. |
| `bladepilot-capture` | Reads USB captures (pcap, pcapng) from USBPcap and usbmon, finds the machine and splits its traffic into commands and replies, for `bladepilot replay`. |
| `bladepilot-cli` | The `bladepilot` command line. |
| `bladepilot-desktop` | The desktop app (`apps/desktop/src-tauri`): Tauri commands over the crates above. `service.rs` does the work without Tauri (imports, plans, the virtual plotter run behind the preview, jobs on their own thread), `model.rs` holds the JSON types the front end mirrors. |

## Desktop app

```text
 ui/ (Svelte 5, TypeScript) ──invoke──► src-tauri/lib.rs ──► service.rs ──► job, sim, device
   │  state.svelte.ts          ◄─channel── job thread: progress, pauses, end
   └─ api/mock: the same calls answered in the browser (screenshots, web demo)
```

- **The front end owns the placement, Rust owns the geometry.** Imported designs stay in the
  Rust process; the front end gets their outlines as path data, a convex hull and the colours,
  and describes a job as items with a transform each plus the colour list. Every plan,
  preview and job is computed by the same crates the command line uses.
- **Nothing slow on the main thread.** Imports, plans and simulations run on Tauri's blocking
  pool; a job runs on a thread of its own and reports through a channel. Pause and stop reach
  it through a `JobControl`.
- **A mock backend in the browser.** The same build runs without Tauri: a small TypeScript
  planner stands in for Rust. Its machine table and demo design are written by the desktop
  crate's tests, so they cannot drift from what the app sends.

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
  cut backwards when their end is nearer. A manual pass order overrides the rules between
  passes, and the planner names every pass that would cut a piece loose too early.
- **Tool changes.** Every pass goes to tool holder 1. Between a pen pass and a blade pass the
  job stops with the tool lifted until the user has swapped it; the only command the session
  adds to the program is that lift (a move to where the tool is).
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
| tauri, tauri-plugin-dialog | Apache-2.0 OR MIT | Desktop window, file dialogs |
| serde, serde_json, sys-locale | Apache-2.0 OR MIT | JSON for the front end, system language |
| Svelte (built with Vite and TypeScript) | MIT | Desktop front end |
| image | MIT OR Apache-2.0 | Reading pictures |
| tiny-skia | BSD-3-Clause | Drawing designs for contours |
| resvg | Apache-2.0 OR MIT | Drawing SVG artwork for the print sheet |
| pdf-writer, miniz_oxide | MIT OR Apache-2.0 (miniz_oxide also Zlib) | The print sheet |
