# ccb: Edit in DxO PureRAW

A [Chromaforge](https://github.com/emmertex/chromaforge) plug-in that sends raw photos to **DxO PureRAW** and brings
back what it makes. Each DNG PureRAW makes is added to the library, in the album you sent the photos from, next to
its original (or stacked on it, if you prefer: see [Settings](#settings)).

Not affiliated with, endorsed by or supported by DxO. DxO and DxO PureRAW are trademarks of DxO Labs. You need your
own copy of DxO PureRAW: version 3 or 2 (see [Versions](#versions)).

Releases: [`ccb-dxo-pureraw` releases](https://github.com/clydecbrown/chromaforge-plugins/releases?q=ccb-dxo-pureraw-v&expanded=true).
It needs Chromaforge v26.44.2 or later (plug-in programs, settings and `library.add`); an older Chromaforge refuses to
install it.

## Updates

The plug-in names its update feed, [`index.json`](https://raw.githubusercontent.com/clydecbrown/chromaforge-plugins/index/index.json)
on this repository's `index` branch. Chromaforge checks it only when you turn checks on: **File ▸ Plug-ins ▸ Plug-in
Manager…**, select this plug-in, then **Check for updates** (off by default). That needs a Chromaforge with plug-in
update checks, and a plug-in installed from a signed release: a file installed by hand without the signature gets no
checks. An older Chromaforge ignores the feed and still runs the plug-in; update it by installing the new release by
hand.

## Use

1. Select raw photos in the Library.
2. **File ▸ Plug-ins ▸ ccb: Edit in DxO PureRAW ▸ Send Selected Photos.** The plug-in leaves out what PureRAW can't
   take and names each one: photos without a file (demo photos), photos that aren't raw files, photos in Recently
   Deleted, and files whose path holds a line break. It starts the version of PureRAW the settings choose.
3. PureRAW opens its *Process N photos* window. Choose the settings there and process the photos, or close it.
4. When PureRAW quits, each DNG it made is added to the library: in the album you sent the photos from, and stacked
   on its original if you turned that on. The plug-in's log (Plug-in Manager ▸ Log) has a summary: how many DNGs
   were added and where, and which photos weren't processed.

## Settings

File ▸ Plug-ins ▸ Plug-in Manager…, then **Settings…** on this plug-in's row:

| Setting | Default | |
|---|---|---|
| **Add to the album I sent from** | on | Each DNG goes into the album you sent its photo from, next to it. |
| **Stack on the original** | off | Each DNG is stacked on the photo it came from, also when it goes into an album. |
| **PureRAW version** | Automatic | Which PureRAW to start (see [Versions](#versions)). The list shows the versions installed and allowed, experimental ones marked. |

- **The album is the one you sent the photos from**: the album, or the Quick Collection, shown in the Library
  when you chose Send Selected Photos. Other albums the photos are in don't count. Sent from a folder, All
  Photographs, a smart album or another view (Picks, a search over All Photographs, …), the DNGs are added without
  an album, as smart albums choose their photos by their rules; the message and the summary say so. With both
  settings off, the DNGs are only added to the library, in their folder.
- **A send keeps what you chose when you sent it.** You can move to another view or change the settings while
  PureRAW is open: the DNGs from that send still go where they would have gone when you sent the photos. The next
  send uses the new settings.
- **If the album is deleted, or a photo is taken out of it, while PureRAW runs**, the DNGs are still added, without
  the album, and the summary names the album and the DNGs it couldn't take.
- Settings are kept for the whole app, not per library.

Good to know:

- **Where the DNGs go is PureRAW's choice**, set in its Preferences. Keep its default, a `DxO` folder next to each
  original: Chromaforge only adds files in the original's folder or below it, and names the ones it couldn't add.
- **Sending a photo again** makes PureRAW write over its earlier DNG. The library keeps that one DNG, in its album
  and stack once each, and reads the new file again, as *Reload from Disk* does: new previews, size and camera
  fields the photo lacked. Metadata already set on that version stays.
- **Develop settings aren't copied** to the DNG: it's a new starting point.
- **Virtual copies:** a virtual copy shares its master's file, so the file is sent once. Sent from an album, its DNG
  goes in next to the copy you selected (stacked on it, with stacking on). Otherwise it's stacked on the master
  photo, whichever of them you selected. (If the master is in Recently Deleted, the DNG goes on the copy.)
- **Up to 250 raw photos per send.** Photos that are left out (JPEGs, videos, Recently Deleted) don't count, up to 2000 selected. For
  more, send them in parts, each once PureRAW has closed. Every DNG is added when PureRAW is done, and a plug-in's
  call has a time limit, so a bigger send could stop partway.
- **One send at a time.** While PureRAW is open from a send, the next one waits. ✕ on the run's row in the activity
  list stops waiting: PureRAW keeps running, but what it makes from that send isn't added.
- PureRAW drops a new batch while it's already open from another app (Lightroom Classic's DxO plug-in, say). The
  summary says so when that seems to have happened: close that window and send again.
- macOS only for now. The plug-in passes PureRAW a Lightroom version that doesn't exist, which keeps PureRAW from
  starting Lightroom Classic when it's done.

## Versions

Each major version of PureRAW installs as an app of its own (`DxO PureRAW 2.app`, `DxO PureRAW 3.app`, …), and you
may have several. Send Selected Photos looks for the ones below that are installed and starts the one the
**PureRAW version** setting chooses:

- **Automatic** (the default): the newest tested version installed. With no tested version, the only one installed,
  or, with several experimental ones, it asks which.
- **A version you chose**: that one, whatever else is installed. If it's no longer installed, Send Selected Photos
  says so and starts nothing: choose another, or Automatic, in the settings.
- With none installed, it says which to install.

| Version | |
|---|---|
| **DxO PureRAW 3** | tested |
| **DxO PureRAW 2** | tested |
| DxO PureRAW 4 to 9 | **experimental**: started with PureRAW 3's handoff, untested |

- **Experimental** versions are started the way PureRAW 3 is, and their results read the way PureRAW 3's are. That
  may not fit: a later version could write its list of results somewhere else, or differently. The messages and
  the summary in its log say "experimental", and when something in the results isn't as expected (or nothing comes
  back after you processed the photos) the log asks you to report it.
- **A version becomes tested** once its handoff has been watched in three runs (processed, cancelled during
  processing, and closed without processing) and the stand-in that CI runs the round trip against
  (`tools/fake-pureraw`) does what that version does.
- **To report how a version went**, open a
  [PureRAW version report](https://github.com/clydecbrown/chromaforge-plugins/issues/new?template=pureraw-version-report.yml):
  the PureRAW version (PureRAW ▸ About), what you did, and the plug-in's log lines from that send (Plug-in
  Manager ▸ Log). For a full check, [the test kit](../../kits/pureraw/README.md) has the steps and a report to paste.
  Don't attach PureRAW's own logs: they hold your licence details.
- PureRAW 10 and later aren't declared, so Chromaforge can't let the plug-in start them; they need a plug-in update.

## Permissions

Chromaforge shows each one unticked when you install the plug-in. It needs all three.

| Permission | What for |
|---|---|
| **Read the catalog** (`catalog`) | To find each selected photo's file, and to tell raw files from others. It reads nothing else. |
| **Add files made from photos to the library** (`libraryAdd`) | To add each DNG PureRAW makes, in the album you sent from or stacked on its original, as the settings say. Chromaforge allows only files in the original's folder or below it, and only an album that holds the original. |
| **Start the program DxO PureRAW** (`programs`, bundle ids `com.dxo-labs.PureRawv9.standalone` down to `com.dxo-labs.PureRawv2.standalone`) | To open the PureRAW you choose with the photos you send. The plug-in passes it the list of files and the arguments its plug-in mode takes, nothing else. Each bundle id is one version of PureRAW. |

It asks for no network access and no other folders.
