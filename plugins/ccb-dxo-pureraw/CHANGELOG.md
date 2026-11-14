# Changelog

## [0.2.0] - unreleased

- Finds which versions of DxO PureRAW are installed. Tested with PureRAW 3 and 2; PureRAW 4 to 9 run as
  experimental, with PureRAW 3's handoff. With one installed, Send Selected Photos starts it; with several, it asks
  which to use; with none, it says which to install.
- Updating from 0.1.0 asks for DxO PureRAW again: the plug-in now names more versions of it, so Chromaforge
  unticks it until you tick it again in File ▸ Plug-ins ▸ Plug-in Manager….
- Needs the `settings` host feature too (Chromaforge v26.44.2 or later, as before).
- **The DNGs go into the album you sent the photos from**, next to their originals, and are no longer stacked on them
  by default. Sent from a folder, All Photographs or a smart album, they're added without an album, and the summary
  says why. If the album is deleted or a photo leaves it while PureRAW runs, its DNG is added without it.
- **Settings** (Plug-in Manager ▸ Settings…): *Add to the album I sent from* (on), *Stack on the original* (off),
  and *PureRAW version* (Automatic: the newest tested version installed, or one you choose). A send keeps the
  album and settings it was sent with, whatever changes before PureRAW is done.
- Send Selected Photos asks which version to use only when Automatic finds no tested version and more than one
  experimental one; a version chosen in the settings that's no longer installed is named, and nothing starts.

## [0.1.0] - unreleased

- First version: File ▸ Plug-ins ▸ ccb: Edit in DxO PureRAW ▸ Send Selected Photos sends raw photos to DxO PureRAW 3
  on macOS, and stacks each DNG it makes on the photo it came from.
