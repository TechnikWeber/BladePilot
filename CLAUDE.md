# BladePilot: notes for Claude

Linux cutting software for Silhouette cutters, Portrait 3 and 4 first. Rust, MIT, public at
github.com/TechnikWeber/BladePilot.

If `re/README.md` exists, read it first: it holds the private, machine-local notes (tools,
paths, the reverse-engineering workspace). It is git-ignored and exists only on the
maintainer's computers.

## Working rules

- Talk to the maintainer in German. Everything in the repository is English.
- Commit and push after every finished change, so GitHub always matches the local tree.
- Release whenever more than a bug fix has landed: bump the version in `Cargo.toml` and
  `apps/desktop/ui/package.json` (0.x: minor for features), add the `CHANGELOG.md` section,
  run `npm run screenshots`, then tag and push (see Releases) and check the published files.
- Commit messages are plain: no AI co-author lines, no session links.
- Before each commit: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`; after front-end changes also `npm run check`, `npm test` and
  `npm run build` in `apps/desktop/ui`.
- Golden programs in `crates/job/tests/golden/` are what reaches a machine. Review every diff by
  hand; regenerate with `BLESS=1 cargo test -p bladepilot-job` only after an intended change.

## Clean room

Read [docs/CLEAN-ROOM.md](docs/CLEAN-ROOM.md) before touching protocol or file-format code.

- Never copy code from GPL projects (inkscape-silhouette, cuthulhu, robocut, …). Facts only.
- No vendor code, icons, fonts, material tables or firmware in the repository.
- Reverse-engineering material lives in `re/` (git-ignored): extracted vendor software,
  analysis projects, notes, USB captures of vendor software. Never commit it, never quote it in
  issues or pull requests.
- Never bypass licence checks or copy protection. Protected store designs are refused.
- Facts sent upstream to craft-libs must come from public documentation or observed device
  behaviour (USB captures), never from decompiled code.

## Layout

| Path | What |
|---|---|
| `crates/machines` | Model table: USB ids, protocol generation, cutting width, mats |
| `crates/design` | SVG and `.studio`/`.studio3` import into one model (mm, y down) |
| `crates/job` | Flatten, place, group into passes, order, check, encode via craft-plotter |
| `crates/sim` | Virtual plotter: parses the byte stream, answers queries, draws SVG |
| `crates/device` | Transports (virtual; USB via nusb; Windows printer class driver) and the job protocol (`Session`) |
| `crates/capture` | Reads USB captures (pcap/pcapng, USBPcap and usbmon) for `bladepilot replay` |
| `apps/cli` | `bladepilot` command line |
| `apps/desktop/src-tauri` | Desktop app `bladepilot-desktop`: Tauri commands over the crates (`service.rs` holds the work, `model.rs` the JSON types) |
| `apps/desktop/ui` | Its web front end: Vite, TypeScript, Svelte 5, own CSS; `src/lib/api/mock` is the browser stand-in for the Rust side |
| `docs/` | Roadmap, architecture, protocol, clean-room rules, hardware test day |

## Status

Progress and next steps are the checkboxes in [docs/ROADMAP.md](docs/ROADMAP.md); keep them up
to date. No hardware is at hand during development: the maintainer owns a Portrait 3 or 4 (which
one is still open). Everything is built and tested against the virtual plotter first.

The hardware test day happens on the maintainer's Windows PC, in a clone of this repository,
with Claude Code running the captures: follow [docs/HARDWARE-DAY.md](docs/HARDWARE-DAY.md)
step by step there. Private material on that PC goes into `re\` as well.

## The desktop app (phase 3, built)

Decided with the maintainer; adjust details, not the direction:

- Tauri 2 in `apps/desktop`: `src-tauri/` is the Rust crate `bladepilot-desktop`, a thin layer
  of commands over the workspace crates (`service.rs` does the work and is tested without a
  window); `ui/` is the web front end (Vite, TypeScript, Svelte 5, own CSS, no UI kit). A plain
  `cargo build` leaves the desktop crate out (`default-members`).
- It has to look good: the mat looks like a real cutting mat, designs in their colours, light
  and dark from the system, English and German from the system language. Check visual changes
  with `npm run screenshots` and look at the images before committing.
- Long jobs run off the UI thread; the UI never freezes while cutting.
- The mock backend (`ui/src/lib/api/mock`) keeps the front end running in a plain browser.
- Testing the real window without the maintainer's desktop: run it in a virtual KWin session
  (`dbus-run-session`, `kwin_wayland --virtual --socket NAME`, `spectacle -b -n -f -o FILE`), see
  the private notes.

## Next

- Phase 2: `.studio3` beyond outlines, test documents from Silhouette Studio under Wine.
- Phase 4: print and cut.
- Desktop: file associations on Windows and macOS, remember the last session, material
  profiles per colour, a web demo with the planner in WebAssembly.

## Releases

Tag `vX.Y.Z` (matching `version` in `Cargo.toml`, with a `CHANGELOG.md` section) and push the
tag: `.github/workflows/release.yml` builds the command line for Linux (AppImage, deb, rpm,
tar.gz for x86_64 and aarch64), Windows (exe, zip for x86_64 and arm64) and macOS (universal
tar.gz), and the desktop app (`BladePilot-Desktop-…` AppImage, `bladepilot-desktop` deb and rpm
for x86_64 and aarch64, Windows setup exe and msi, universal dmg), then publishes the release;
0.x versions are marked as pre-releases. Started by hand, the workflow only builds.

## Commands

```sh
cargo test --workspace
cargo run -p bladepilot-cli -- cut crates/design/tests/fixtures/nested.svg --by-color --simulate out/x.preview.svg
cargo run -p bladepilot-cli -- cut crates/design/tests/fixtures/square.svg --dry-run

# Desktop app (in apps/desktop/ui, after npm ci)
npm run tauri dev             # window with hot reload; `npm run dev` alone = browser with the mock backend
npm run tauri build           # bundles for this system (`npm run tauri -- build --bundles deb`)
npm run screenshots           # README images into docs/images (headless Chrome, mock backend)
BLESS=1 cargo test -p bladepilot-desktop   # after changing machines or the demo design: mock JSON
```

`?demo`, `?select`, `?preview=0.6`, `?send=change|start`, `?lang=de` and `?theme=dark` put the
front end into fixed states for screenshots; in `npm run tauri dev` the variable `VITE_STATE` does the
same inside the window (e.g. `VITE_STATE='demo&send=start'`).
