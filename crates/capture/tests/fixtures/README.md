# Captures of a real machine

BladePilot's own traffic with a Silhouette Portrait 4 (firmware V1.05) on the first hardware
test day, 2026-11-27, on Windows 11 with USBPcap (link type 249, the machine alone). They contain
no serial numbers or addresses. Captures of the vendor software are not published (see
[CLEAN-ROOM.md](../../../../docs/CLEAN-ROOM.md)); what was learned from them is described in
[PROTOCOL.md](../../../../docs/PROTOCOL.md).

| File | Job | What to see |
|---|---|---|
| `portrait4-usb-pen-square.pcap` | `cut hardware-tests/T1-square.svg --pen` | init, version, status `0`, the whole program in one write, status `1` for about 2.5 s, `0` |
| `portrait4-usb-autoblade-depth2.pcap` | the same square cut with `--depth 2 --force 15 --speed 10 --offset 120,0`, the blade at depth 3 before | the machine sets the AutoBlade after `TF2,1`: busy about 9.5 s |
| `portrait4-usb-circle-in-square.pcap` | `cut hardware-tests/T3-label.svg --depth 2 --force 15 --speed 10 --offset 60,0` | a circle as short lines, cut before the square around it |

Read them with `bladepilot replay FILE --machine portrait4` (add `--svg out.svg` for a drawing).
The Bluetooth counterpart is a text transcript in
[`crates/device/tests/fixtures`](../../../device/tests/fixtures).
