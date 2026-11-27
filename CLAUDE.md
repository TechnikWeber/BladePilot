# BladePilot: notes for Claude

Linux cutting software for Silhouette cutters, Portrait 3 and 4 first. Rust, MIT, public at
github.com/TechnikWeber/BladePilot.

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
one is still open) and has a Windows PC with Silhouette Studio for captures later. Everything is
built and tested against the virtual plotter first; [docs/HARDWARE-DAY.md](docs/HARDWARE-DAY.md)
is the plan for the first session with the real machine.

## Commands

```sh
cargo test --workspace
cargo run -p bladepilot-cli -- cut crates/design/tests/fixtures/nested.svg --by-color --simulate out/x.preview.svg
cargo run -p bladepilot-cli -- cut crates/design/tests/fixtures/square.svg --dry-run
```
