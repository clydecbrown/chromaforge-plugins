# Changelog

## [Unreleased]

- **DxO PureRAW 6 is offered**, and tested. Its bundle id is `com.dxo-labs.PureRAWv6.standalone`, with a capital RAW,
  which is how DxO spells it and what Chromaforge looks it up by. It is started with PureRAW 3's arguments and writes its
  results list as `dxo-imported-files-PureRAWv6`. Checked on PureRAW 6.9 with runs of the order, names, fail, bad, empty,
  close, cancel and busy cases (instant and last-settings modes). Not yet run: the no `--lr-version`, bad `--lr-version`
  and installed-Lightroom-version cases.
- **No experimental versions.** A version is offered only once it's tested. Automatic starts the newest version installed,
  and the plug-in no longer marks any version as experimental or asks you to report one.
- **PureRAW 4, 5, 7, 8 and 9 are no longer declared.** This plug-in hasn't seen them, so it doesn't offer them.
- **Allow DxO PureRAW again after updating:** the PureRAW entry changes, so Chromaforge asks for it again. Until you
  allow it in Plug-in Manager ▸ ccb: Edit in DxO PureRAW ▸ Permissions, Send Selected Photos says it isn't allowed.

## [0.2.1] - 2026-11-24

- **The log is the whole record.** Every reply Send Selected Photos shows you (nothing selected, DxO PureRAW not
  installed or not allowed, too many photos, nothing it could send, and the send itself with the album reason and
  the photos left out) is also a line in the plug-in's log (Plug-in Manager ▸ Log), with the same words. A send's
  line starts with its run id. The log names photos by file name, never a path. When Chromaforge asks the plug-in
  twice before a send, a refusal can appear twice.

## [0.2.0] - 2026-11-19

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
- **Updates:** the plug-in names its update feed. Turn on Check for updates in the Plug-in Manager and new versions
  come through it (needs a Chromaforge with plug-in update checks).
  An update from the Plug-in Manager keeps the permissions you allowed. One that adds a PureRAW version, or an install
  by command with different code, asks for DxO PureRAW again: allow it in Plug-in Manager ▸ the plug-in ▸ Permissions.
