# The machine protocol, as BladePilot understands it

Silhouette machines speak a dialect of Graphtec's GP-GL. This page describes it in BladePilot's
own words; the virtual plotter is written from it. Facts come from the sources at the end.
Everything marked *to verify* waits for the [hardware test day](HARDWARE-DAY.md).

## Connection

- USB printer class, one bulk endpoint out and one in. Vendor id `0x0b4d` (Graphtec); the Cameo
  5 Alpha reports `0x3844`. Product ids are in `crates/machines`.
- On Linux the kernel's `usblp` driver claims the machine first; a program that talks to it
  directly detaches the driver and needs write access to the device node (udev rule).

## Framing

- Commands are ASCII text, each followed by ETX (`0x03`).
- Queries and resets are escape sequences: ESC (`0x1b`) and one byte, with no terminator.
- Replies are ASCII text followed by ETX.

## Units and coordinates

- One device unit is 0.05 mm: 20 units per mm, 508 per inch.
- Coordinates are written feed axis first: `M<down>,<right>`. The origin is the top-left
  corner of the media as it enters the machine.

## Queries

| Bytes | Meaning | Reply |
|---|---|---|
| `ESC EOT` | Initialise | none |
| `ESC ENQ` | Status | `0` ready, `1` moving, `2` no media; *to verify:* other codes |
| `ESC NAK` | Tools in the holders (Cameo 4 generation) | Codes per holder, comma-separated |
| `FG` ETX | Firmware version | Text such as `CAMEO 4 PRO V1.02`; *to verify:* Portrait 3/4 |

Tool codes: 0 empty, 1 ratchet blade, 2 AutoBlade, 3 deep-cut blade, 4 kraft blade, 5 rotary
blade, 7 pen, 255 error.

## Commands

| Command | Meaning |
|---|---|
| `M<d>,<r>` | Move to a point with the tool lifted |
| `D<d>,<r>[,<d>,<r>…]` | Cut or draw to one or more points |
| `\<d>,<r>` / `Z<d>,<r>` | Upper-left and lower-right corner of the cutting area |
| `J<h>` | Select tool holder `h` (machines with two holders) |
| `FX<f>[,<h>]` | Force |
| `!<s>[,<h>]` | Speed |
| `TF<d>,<h>` | Blade depth for a blade the machine sets itself |
| `FC<x>[,<y>,<h>]` | Blade swivel offset in device units |
| `FE<n>,<h>` | Lift the tool between segments when `n` is 1 |
| `FF<s>,<e>,<h>` | Corner handling range in tenths of a mm |
| `TJ<a>` | Acceleration |
| `TG<n>` | Mat: 0 none, 1 12×12 in, 2 12×24 in, 3 Portrait 8×12 in, 8 15×15 in, 9 24×24 in, 11 8.5×12 in |
| `TB50,<n>` | Feed direction: 0 portrait, 1 landscape |
| `FN0` | Soft reset of the settings |
| `FY<n>` | Roller track enhancing: 0 on |
| `FW<id>` | Media preset (older machines only) |
| `L0` | Pen-up marker between sections |
| `SO0` | Park, used after the last move |
| `H` | Home |

## Generations

| Generation | Machines | Differences |
|---|---|---|
| Legacy | Cameo 1 and 2, Portrait 1 and 2, Craft Robo, SD | Media preset `FW`; force, speed and offset without a holder |
| Cameo 3 | Cameo 3 | Commands carry the holder; mat select; blade depth |
| Cameo 4 | Cameo 4 and 5, Portrait 3 and 4 | Force and blade offset are sent twice; no `J` on one-holder machines |

## Registration marks (print and cut)

`TB99` starts the set-up, `TB52,2` selects the mark type, `TB51,400` the mark length (20 mm),
`TB53,10` the line width, `TB54,0,0` the offset and `TB55,1`. Then `TB123,<h>,<w>,<top>,<left>`
searches for three marks (`TB124` for four) around the expected position, or `TB23,<h>,<w>`
positions manually. The sender waits until the status reads ready before it sends the cut.
*To verify:* the reply when the marks are not found.

## Open questions for the hardware test day

- Status codes besides 0, 1 and 2; the reply when the registration marks are not found.
- The exact version strings of the Portrait 3 and 4, and whether both answer `ESC NAK`.
- Force and speed ranges of the Portrait 3 and 4.
- How much the machine buffers and how it signals that it is full.
- What Silhouette Studio sends that BladePilot does not, for the same job, and for its test
  cut.
- What the machine offers over Bluetooth (a Classic serial port, BLE services), and whether it
  speaks the same GP-GL there.

## Sources

- [`craft-plotter`](https://github.com/storytold/craft-libs/tree/main/crates/plotter) documentation
  and source (MIT OR Apache-2.0), which collects the community's facts
- [py_silhouette](https://github.com/mossblaser/py_silhouette): registration-mark sequence
- Graphtec's GP-GL command reference for the CraftRobo Pro and CE5000
- Device notes of [inkscape-silhouette](https://github.com/fablabnbg/inkscape-silhouette)
  (read for facts only, see [CLEAN-ROOM.md](CLEAN-ROOM.md))
