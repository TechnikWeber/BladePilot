# The machine protocol, as BladePilot understands it

Silhouette machines speak a dialect of Graphtec's GP-GL. This page describes it in BladePilot's
own words; the virtual plotter is written from it. Facts come from the sources at the end and
from the first [hardware test day](HARDWARE-DAY.md) (2026-11-27, a Portrait 4 with firmware
V1.05, observed over USB and Bluetooth). Everything marked *to verify* is still open.

## Connection

- USB printer class, one bulk endpoint out and one in. Vendor id `0x0b4d` (Graphtec); the Cameo
  5 Alpha reports `0x3844`. Product ids are in `crates/machines`.
- On Linux the kernel's `usblp` driver claims the machine first; a program that talks to it
  directly detaches the driver and needs write access to the device node (udev rule).
- Portrait 4 (observed): USB 2.0 full speed, interface class 7 (printer), subclass 1, protocol 2
  (bidirectional), bulk endpoint 0x01 out and 0x82 in with 64-byte packets, self-powered. It
  has a serial number string but no manufacturer and no product string. Windows binds it to
  its own USB printing support (`usbprint`); no vendor driver is involved.

### Bluetooth

- The Portrait 4 is dual mode. It announces itself as `Portrait 4-<serial>` (the USB serial
  number) both over Bluetooth Classic and Bluetooth Low Energy.
- The cutting connection is **Bluetooth Classic**: one RFCOMM service, on channel 1 (an SDP
  search for any RFCOMM service finds it). It carries **exactly the same bytes as USB**, with
  the same replies, and no framing of its own. Replies come back in 25–60 ms (USB: 100–160 ms).
- Pairing: the machine has no input or output and asks for no bonding, so Secure Simple Pairing
  runs as "Just Works" while connecting. There is no PIN, nothing to confirm and nothing stays
  paired; no serial port is created. A program opens an RFCOMM socket and talks.
- Closing is slow: the machine acknowledges the RFCOMM disconnect only after 15–20 s per
  channel, and a new connection made in the meantime waits or fails. Expect up to about 40 s
  after a disconnect before the machine accepts the next connection; keep one connection open.
- The first reply after connecting can be a stale one from the previous session (an unread
  answer to the last query): read and drop what is pending before the first query.
- The blue Bluetooth light: blinking while ready, steady while connected, off when Bluetooth is
  switched off (the Bluetooth button). The machine is not found while it is off.
- The Low Energy side offers the standard Generic Access, Generic Attribute and Device
  Information services and one vendor service (`e2088282-4fde-42f9-bb22-6ec3c7ed8f91`, two
  characteristics with write and indicate, one with write only). Silhouette Studio on Windows
  does not use it; its purpose (perhaps the mobile app) is *to verify*.

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
| `FG` ETX | Firmware version | Text such as `CAMEO 4 PRO V1.02`; the Portrait 4: `Portrait 4 V1.05` and four spaces |
| `ESC DC1` (0x11) | Identification | The same text as `FG` |
| `ESC SYN` (0x16) | *Unknown*; Silhouette Studio polls it with the status | `   0` |
| `TI` ETX | *Unknown* | Empty (only ETX) on the Portrait 4 |
| `TB71` ETX, `FA` ETX | *Unknown* (offsets or calibration?) | `    0,    0`: two numbers, right-aligned in five characters |

Tool codes: 0 empty, 1 ratchet blade, 2 AutoBlade, 3 deep-cut blade, 4 kraft blade, 5 rotary
blade, 7 pen, 255 error.

Observed on the Portrait 4: the status is `2` without the mat, `0` once it is loaded, `1` while
the head moves (not while the mat is drawn in). `ESC NAK` answers ` 2, 0` with the AutoBlade and
` 7, 0` with the sketch pen: the machine tells a pen from a blade, and reports a second, empty
holder although it has only one. Silhouette Studio polls status, tools and `ESC SYN` about every
half second while connected; it never sends `ESC EOT`.

## Commands

| Command | Meaning |
|---|---|
| `M<d>,<r>` | Move to a point with the tool lifted |
| `D<d>,<r>[,<d>,<r>…]` | Cut or draw to one or more points |
| `\<d>,<r>` / `Z<d>,<r>` | Upper-left and lower-right corner of the cutting area |
| `J<h>` | Select tool holder `h` (machines with two holders) |
| `FX<f>[,<h>]` | Force |
| `!<s>[,<h>]` | Speed |
| `TF<d>,<h>` | Blade depth for a blade the machine sets itself. The Portrait 4 then moves the head to its setting point and turns the AutoBlade (status `1` for about 3 s); it skips that when the depth is already set |
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
| `BZ1,<d0>,<r0>,<d1>,<r1>,<d2>,<r2>,<d3>,<r3>,0` | A cubic Bézier curve with the tool down: start, two control points, end. The Portrait 4 cuts curves sent this way; BladePilot sends short lines instead |
| `TR…`, `BE…` | Seen around and inside jobs of the vendor software; *meaning unknown*, not needed by BladePilot |

## Generations

| Generation | Machines | Differences |
|---|---|---|
| Legacy | Cameo 1 and 2, Portrait 1 and 2, Craft Robo, SD | Media preset `FW`; force, speed and offset without a holder |
| Cameo 3 | Cameo 3 | Commands carry the holder; mat select; blade depth |
| Cameo 4 | Cameo 4 and 5, Portrait 3 and 4 | Force and blade offset are sent twice; no `J` on one-holder machines |

The holder argument on the Portrait 4 (one holder): it acts on BladePilot's `FX<f>,1`, `!<s>,1`,
`TF<d>,1` and `FC<x>,1,1` (force, speed and depth all measurably took effect).

The machine keeps what it is sent while it sets the AutoBlade: BladePilot sends the settings and
the paths in one go and the cut starts once the blade is set. A job ends with the mat still
loaded; the machine only moves it out on its own button.

### How the vendor software seems to do it (assumptions)

Only a rough picture, as a hint for others; BladePilot does not depend on it. Silhouette Studio
appears to frame every job with a few set-up and closing commands of its own, to limit the cutting
area to the page with a small margin, to send force, speed and blade offset partly without the
holder argument, to wait until the blade is set before it sends the paths, to send curves as
Bézier segments and straight lines in a compact form of its own, and to move the head back to the
origin at the end. It polls the status and the tools about twice a second while connected.

## Registration marks (print and cut)

`TB99` starts the set-up, `TB52,2` selects the mark type, `TB51,400` the mark length (20 mm),
`TB53,10` the line width, `TB54,0,0` the offset and `TB55,1`. Then `TB123,<h>,<w>,<top>,<left>`
searches for three marks (`TB124` for four) around the expected position, or `TB23,<h>,<w>`
positions manually. The sender waits until the status reads ready before it sends the cut.
*To verify:* the reply when the marks are not found.

## Open questions

- Status codes besides 0, 1 and 2; the reply when the registration marks are not found.
- The Portrait 3's version string and tool reply.
- Force and speed ranges of the Portrait 3 and 4.
- How much the machine buffers and how it signals that it is full; what stops a running job
  (a job sent in one piece keeps running when the sender stops sending).
- The meaning of `TR`, `TI`, `TB71`, `FA` and `ESC SYN`, and the `BE` records.
- What the Low Energy vendor service is for.

Answered on the first test day: the Portrait 4's version string and tool reply, and what the
machine offers over Bluetooth (the same GP-GL over RFCOMM).

## Reproducing this without the private material

BladePilot's own captures of the Portrait 4 are in the repository:
[`crates/capture/tests/fixtures`](../crates/capture/tests/fixtures) (USB, read them with
`bladepilot replay`) and
[`crates/device/tests/fixtures`](../crates/device/tests/fixtures) (a Bluetooth session as a
text transcript). Tests replay them, so what is written here stays checked.

## Sources

- [`craft-plotter`](https://github.com/storytold/craft-libs/tree/main/crates/plotter) documentation
  and source (MIT OR Apache-2.0), which collects the community's facts
- [py_silhouette](https://github.com/mossblaser/py_silhouette): registration-mark sequence
- Graphtec's GP-GL command reference for the CraftRobo Pro and CE5000
- Device notes of [inkscape-silhouette](https://github.com/fablabnbg/inkscape-silhouette)
  (read for facts only, see [CLEAN-ROOM.md](CLEAN-ROOM.md))
