# Clean-room rules

BladePilot exists so that people can use the cutting machines and design files they own on an
operating system the vendor does not support. It is built for interoperability, and it keeps
its sources clean so that it can stay MIT-licensed and public.

## Where facts may come from

1. Public documentation: manuals, command references, product pages.
2. Community notes and the documented behaviour of other drivers, read for facts. Code from
   GPL projects (inkscape-silhouette, cuthulhu, robocut, …) is never copied, translated or
   adapted.
3. Observation of our own machines and files: USB captures, files saved with different
   settings and compared byte by byte, the machine's replies. In the EU, observing, studying
   and testing a program you are entitled to use is allowed (Directive 2009/24/EC, Art. 5(3);
   in Germany § 69d (3) UrhG).
4. Decompilation of vendor software only where it is indispensable for interoperability and
   the information is not available otherwise (Art. 6; § 69e UrhG). Its results stay out of
   this repository and are never contributed upstream.

## What never enters the repository

- Vendor code, decompiled or not; vendor icons, images, fonts, sounds, material tables,
  firmware or design-library content.
- Captures of vendor software, analysis projects and notes. They live in `re/`, which git
  ignores.
- Anything that bypasses a licence check, an activation or copy protection. Protected designs
  from the vendor's store are refused, not decrypted.

## Contributing upstream

`craft-libs` accepts facts from public documentation and observed device behaviour only.
Findings from USB captures and saved files can go there; findings from decompilation cannot.

## Names

"Silhouette", "Cameo", "Portrait" and the vendor's product names are used only to say which
machines and files BladePilot works with. The project's name, icon and look are its own.
