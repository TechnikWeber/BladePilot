# Hardware test day

BladePilot is built against the virtual plotter. On a hardware test day the real machine
answers the open questions in [PROTOCOL.md](PROTOCOL.md): the same test jobs run once from
Silhouette Studio and once from BladePilot, both are captured over USB, and the captures are
compared with `bladepilot replay`.

Everything happens on one Windows PC: Studio and BladePilot use the machine in turn through the
same Windows driver, Wireshark's USBPcap captures both, and Claude Code can run the whole session
from a clone of this repository.

## Results of the first test day (2026-11-27)

A Portrait 4, firmware V1.05, Silhouette Studio 5.0.556, BladePilot 0.3.0, on Windows 11.

- **T0 to T3 done; BladePilot works over USB.** The pen square lands on Studio's, the AutoBlade
  cuts copy paper cleanly, circles and the inside-first order are right. BladePilot's depth,
  force and speed all take effect (depth 2 → the machine sets the AutoBlade; force 1 and speed
  1 → a slow, barely-through cut). Depth 2, force 15, speed 10 cuts copy paper without scoring
  the mat's film; Studio's profile (depth 3) scores it.
- Found and fixed afterwards: a pen pass of a two-colour job inherited the blade's depth and
  force, so the machine would have run its AutoBlade setting with the pen in the holder.
- **Bluetooth works**: the same bytes over Bluetooth Classic (RFCOMM channel 1), no pairing, no
  PIN. BladePilot 0.4 sends over Bluetooth on Windows and Linux.
- T4 (tool change), T5 (print and cut), T6 (cancel), T7 (desktop app), T8 (Studio's test cut):
  not done; the tool change and stopping are built to the best of what is known.
- All facts are in [PROTOCOL.md](PROTOCOL.md).

Next time: T4, T6 and T7 with the desktop app (USB and Bluetooth), a long job to see how much
the machine buffers, and how Studio stops a running job.

## Bring

- The machine (Portrait 3 or 4), its USB cable, a mat, plain paper, light card
- A Silhouette sketch pen, or a pen in a pen adapter, for the first runs
- The Windows PC

## Prepare the Windows PC (once)

1. Install Silhouette Studio and connect the machine once, so Studio offers its firmware update
   now and not in the middle of a test. Note Studio's version and the firmware version it shows;
   they also settle whether the machine is a Portrait 3 or 4.
2. Install [Wireshark](https://www.wireshark.org/download.html) and tick **USBPcap** in the
   installer. The MSI package (what `winget install WiresharkFoundation.Wireshark` installs)
   has no USBPcap: then install it on its own (`winget install desowin.USBPcap`) and copy
   `C:\Program Files\USBPcap\USBPcapCMD.exe` into `C:\Program Files\Wireshark\extcap\`.
   Plug in a USB Bluetooth adapter now if you want to capture Bluetooth: USBPcap only sees
   devices that were there at start-up. Restart Windows.
3. Install [Git for Windows](https://git-scm.com/download/win) and Claude Code.
4. Clone the repository and fetch the Windows build of BladePilot into the git-ignored `re`
   folder:

   ```powershell
   git clone https://github.com/TechnikWeber/BladePilot
   cd BladePilot
   $tag = gh release list --repo TechnikWeber/BladePilot --limit 1 --json tagName --jq ".[0].tagName"
   gh release download $tag --repo TechnikWeber/BladePilot --pattern "bladepilot-cli-*-windows-x86_64.exe" --dir re\bin
   Get-ChildItem re\bin\bladepilot-cli-*-windows-x86_64.exe | Rename-Item -NewName bladepilot.exe
   ```

   Without the GitHub CLI, download the `.exe` from the releases page into `re\bin` instead.
   For T7 also install the desktop app: `BladePilot-Desktop-…-windows-x64-setup.exe` from the
   same release.

## Start the session

USBPcap needs administrator rights to capture. Open Windows Terminal with **Run as
administrator**, then:

```powershell
cd BladePilot
git pull
claude
```

and say: *"Hardware test day: read CLAUDE.md and docs/HARDWARE-DAY.md and run the captures with
me."* Claude starts and stops the captures, you run the jobs in Studio, and Claude reads every
capture with `bladepilot replay`.

## The test jobs

Draw every shape in Studio by its numbers (Transform panel), so it matches the SVG BladePilot
cuts. Studio's Basic Edition cannot import SVG; drawing by numbers works in every edition. Save
each Studio document as `.studio3` into `re\studio\`: those files are test material for the
importer.

| Job | Shape | In Studio | In BladePilot (`re\bin\bladepilot.exe …`) |
|---|---|---|---|
| T0 | none | Connect, load and unload the mat, wait 30 s | `devices` |
| T1 | 20 × 20 mm square at X 10, Y 10 | Sketch pen, default settings | `cut hardware-tests\T1-square.svg --pen` |
| T2 | as T1 | AutoBlade on paper: note depth, force, speed | `cut hardware-tests\T1-square.svg --depth D --force F --speed S`; to try settings first: `test-cut --at 150,10 --depth D --force F --speed S` |
| T3 | 40 × 40 mm square at X 5, Y 5, 20 mm circle in its centre | AutoBlade on paper | `cut hardware-tests\T3-label.svg` with T2's settings |
| T4 | as T3, circle red, square black | Cut by line colour: red sketch pen, black AutoBlade | `cut hardware-tests\T4-two-colours.svg --by-color --pen-color "#ff0000"`; it draws the circle, lifts the pen and waits: put the AutoBlade in, press Enter |
| T5 | 30 mm circle with registration marks | Print and cut: print the sheet, load it, cut | not yet: Studio only |
| T6 | as T2 | Start, cancel after about a second | Ctrl+C after about a second |
| T7 | as T4 | not needed | Optional, with the desktop app (`BladePilot-Desktop-…-windows-x64-setup.exe` from the releases): open `T4-two-colours.svg`, set red to Pen, choose the machine under Connection, Send; pause once, then Stop in a second run |
| T8 | a test cut | Send panel, **Test Cut** with T2's settings | `test-cut --depth D --force F --speed S` |

Add `--machine portrait4` for a Portrait 4, and `--experimental-usb` to send to the machine.
Close Silhouette Studio's connection (or Studio itself) before BladePilot sends, and the other
way round: only one program can talk to the machine at a time.

## Bluetooth

Answered on the first test day: the machine speaks the same GP-GL over Bluetooth Classic
(RFCOMM channel 1) and needs no pairing. To try BladePilot over Bluetooth, unplug the USB cable,
make sure the machine's blue Bluetooth light blinks (press the Bluetooth button if it is off)
and close Silhouette Studio, which holds the connection while it runs:

```powershell
re\bin\bladepilot.exe devices --bluetooth
re\bin\bladepilot.exe cut hardware-tests\T1-square.svg --pen --machine portrait4 --experimental-bluetooth
```

Without an address `--experimental-bluetooth` searches for the machine first. Connecting can
take up to a minute right after another program was connected.

To capture Bluetooth, the adapter must be a USB one that was plugged in before Windows started
(see the preparation): then USBPcap captures the adapter like any USB device, and Wireshark
decodes the Bluetooth traffic in it (HCI, L2CAP, RFCOMM).

## Capturing

Capture the machine alone when a keyboard or mouse shares its USB hub: otherwise every keystroke
ends up in the file, and a gaming mouse (8000 reports a second) can flood the capture buffer.
`USBPcapCMD.exe --extcap-interface \\.\USBPcap1 --extcap-config` lists the devices of a hub
with their addresses (the machine is the "USB printing support" entry); then capture only it:

```powershell
& "C:\Program Files\USBPcap\USBPcapCMD.exe" -d \\.\USBPcap2 --devices 10 --capture-from-new-devices --inject-descriptors -o re\captures\T1-studio.pcap
```

Stop it by closing its window (or ending the process): it writes every block straight to the
file, so nothing is lost. `bladepilot replay` reads these `.pcap` files like Wireshark's.

Otherwise list the capture interfaces and capture on all USBPcap hubs at once; `bladepilot
replay` finds the machine among the devices by the way it talks:

```powershell
& "C:\Program Files\Wireshark\tshark.exe" -D
& "C:\Program Files\Wireshark\tshark.exe" -i USBPcap1 -i USBPcap2 -w re\captures\T1-studio.pcapng
```

Stop with Ctrl+C when the machine is done. Name the files `T<n>-studio.pcapng` and
`T<n>-bladepilot.pcapng`. Then:

```powershell
re\bin\bladepilot.exe replay re\captures\T1-studio.pcapng --svg re\captures\T1-studio.svg
re\bin\bladepilot.exe replay re\captures\T1-studio.pcapng --commands-only > re\captures\T1-studio.txt
re\bin\bladepilot.exe replay re\captures\T1-bladepilot.pcapng --commands-only > re\captures\T1-bladepilot.txt
git diff --no-index re\captures\T1-studio.txt re\captures\T1-bladepilot.txt
```

`replay` shows the whole conversation with repeated status polls folded, including the
machine's version reply; `--svg` draws what the machine was told to cut.

Captures and Studio documents stay in `re\`, which git ignores: traffic of the vendor software
is not published. Copy them somewhere private to keep them.

## On Linux instead

The same works with usbmon: install the udev rule from `packaging/linux/70-bladepilot.rules`,
then `sudo modprobe usbmon`, look up the machine's bus with `lsusb` and capture with
`sudo tshark -i usbmon<bus> -w re/captures/T1-bladepilot.pcapng`.

## Safety

- Pen on paper first. The first blade job runs on paper with low force.
- Keep a hand near the machine's power button for the first BladePilot jobs.
- Stop at the first unexpected movement and note what happened before trying again.

## Afterwards

Code changes made on the Windows PC are committed and pushed as usual. The private material
(`re\captures`, `re\studio`, `re\notes`) goes to the maintainer's laptop by USB stick or the
NAS, into `~/BladePilot/re/`, never through git.

The differences between the captures answer the open questions in [PROTOCOL.md](PROTOCOL.md)
and turn into fixes and new golden tests. Facts learned from captures may also go upstream to
craft-libs (see [CLEAN-ROOM.md](CLEAN-ROOM.md)).
