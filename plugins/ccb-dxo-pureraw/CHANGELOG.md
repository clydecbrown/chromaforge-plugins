# Changelog

## [0.2.0] - unreleased

First public release, for macOS only. **File ▸ Plug-ins ▸ ccb: Edit in DxO PureRAW ▸ Send Selected Photos** sends raw
photos to DxO PureRAW, and adds each DNG it makes to the library.

- **The DNGs go into the album you sent the photos from**, next to their originals. Stacking each DNG on its original
  is off by default and can be turned on. Sent from a folder, All Photographs or a smart album, they're added without
  an album, and the summary says why.
- **Settings** (Plug-in Manager ▸ Settings…): *Add to the album I sent from* (on), *Stack on the original* (off), and
  *PureRAW version* (Automatic: the newest tested version installed, or one you choose).
- **Versions:** tested with DxO PureRAW 3 and 2. PureRAW 4 to 9 run as experimental, started the way PureRAW 3 is,
  and the log asks you to report how they go. With none installed, Send Selected Photos says which to install.
- Needs Chromaforge v26.44.2 or later (plug-in programs, settings and `library.add`); an older Chromaforge refuses to
  install it.
