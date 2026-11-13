# Changelog

## [0.2.0] - unreleased

- Finds which versions of DxO PureRAW are installed. Tested with PureRAW 3 and 2; PureRAW 4 to 9 run as
  experimental, with PureRAW 3's handoff. With one installed, Send Selected Photos starts it; with several, it asks
  which to use; with none, it says which to install.
- Updating from 0.1.0 asks for DxO PureRAW again: the plug-in now names more versions of it, so Chromaforge
  unticks it until you tick it again in File ▸ Plug-ins ▸ Plug-in Manager….
- Needs the `settings` host feature too (Chromaforge v26.44.2 or later, as before).

## [0.1.0] - unreleased

- First version: File ▸ Plug-ins ▸ ccb: Edit in DxO PureRAW ▸ Send Selected Photos sends raw photos to DxO PureRAW 3
  on macOS, and stacks each DNG it makes on the photo it came from.
