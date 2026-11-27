# BladePilot: notes for Claude

Linux cutting software for Silhouette cutters, Portrait 3 and 4 first. Rust, MIT, public at
github.com/TechnikWeber/BladePilot.

If `re/README.md` exists, read it first: it holds the private, machine-local notes (tools,
paths, the reverse-engineering workspace). It is git-ignored and exists only on the
maintainer's computers.

## Working rules

- Talk to the maintainer in German. Everything in the repository is English.
- Commit and push after every finished change, so GitHub always matches the local tree.
- Commit messages are plain: no AI co-author lines, no session links.
- Before each commit: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
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
| `docs/` | Roadmap, architecture, protocol, clean-room rules, hardware test day |

## Status

Progress and next steps are the checkboxes in [docs/ROADMAP.md](docs/ROADMAP.md); keep them up
to date. No hardware is at hand during development: the maintainer owns a Portrait 3 or 4 (which
one is still open). Everything is built and tested against the virtual plotter first.

The hardware test day happens on the maintainer's Windows PC, in a clone of this repository,
with Claude Code running the captures: follow [docs/HARDWARE-DAY.md](docs/HARDWARE-DAY.md)
step by step there. Private material on that PC goes into `re\` as well.

## Next: the desktop app (phase 3)

The plan, decided with the maintainer; adjust details, not the direction:

- Tauri 2 in `apps/desktop`: `src-tauri/` is the Rust crate `bladepilot-desktop`, a thin layer
  of commands over the workspace crates; `ui/` is the web front end (Vite, TypeScript,
  Svelte 5, own CSS, no UI kit). Plain `cargo build` must keep working for the rest of the
  workspace.
- It has to look good: a mat that looks like a real cutting mat (grid, rulers, cutting area),
  the design drawn in its colours, drag to place, rotate and scale; a colour list with tool,
  force, speed, depth and repeats per colour; a machine panel with status; a send dialog with
  progress; an animated preview of the cutting order from the virtual plotter. Light and dark
  theme from the system, English and German from the system language.
- Long jobs run off the UI thread; the UI never freezes while cutting.
- The front end has a mock backend so it runs in a plain browser: that is how screenshots for
  the README are made (headless Chrome), and maybe later a web demo.
- Releases: extend `release.yml` with the Tauri bundles (AppImage, deb, rpm, msi/exe, dmg).
- Then phase 2 (`.studio3` beyond outlines, test documents from Silhouette Studio under Wine)
  and phase 4 (print and cut).

## Releases

Tag `vX.Y.Z` (matching `version` in `Cargo.toml`, with a `CHANGELOG.md` section) and push the
tag: `.github/workflows/release.yml` builds Linux (AppImage, deb, rpm, tar.gz for x86_64 and
aarch64), Windows (exe, zip for x86_64 and arm64) and macOS (universal tar.gz) and publishes
the release; 0.x versions are marked as pre-releases. Started by hand, the workflow only builds.

## Commands

```sh
cargo test --workspace
cargo run -p bladepilot-cli -- cut crates/design/tests/fixtures/nested.svg --by-color --simulate out/x.preview.svg
cargo run -p bladepilot-cli -- cut crates/design/tests/fixtures/square.svg --dry-run
```
