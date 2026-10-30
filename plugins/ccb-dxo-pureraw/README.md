# CCB: Edit in DxO PureRAW

A [Chromaforge](https://github.com/emmertex/chromaforge) plug-in that sends raw photos to **DxO PureRAW 3** and brings
back what it makes. Each DNG PureRAW makes is added to the library, stacked on the photo it came from, as the
round trip from Lightroom Classic does.

Not affiliated with, endorsed by or supported by DxO. DxO and DxO PureRAW are trademarks of DxO Labs. You need your
own copy of DxO PureRAW 3.

Releases: [`ccb-dxo-pureraw` releases](https://github.com/clydecbrown/chromaforge-plugins/releases?q=ccb-dxo-pureraw-v&expanded=true).
It needs Chromaforge v26.44.2 or later (plug-in programs and `library.add`); an older Chromaforge refuses to install it.

## Use

1. Select raw photos in the Library.
2. **File ▸ Plug-ins ▸ CCB: Edit in DxO PureRAW ▸ Send Selected Photos.** The plug-in leaves out what PureRAW can't
   take and names each one: photos without a file (demo photos), photos that aren't raw files, photos in Recently
   Deleted, and files whose path holds a line break.
3. PureRAW opens its *Process N photos* window. Choose the settings there and process the photos, or close it.
4. When PureRAW quits, each DNG it made is added to the library, stacked on its original. The plug-in's log
   (Plug-in Manager ▸ Log) has a summary: how many DNGs were added, and which photos weren't processed.

Good to know:

- **Where the DNGs go is PureRAW's choice**, set in its Preferences. Keep its default, a `DxO` folder next to each
  original: Chromaforge only adds files in the original's folder or below it, and names the ones it couldn't add.
- **Sending a photo again** makes PureRAW write over its earlier DNG. The library keeps the one stacked version and
  reads the new file again, as *Reload from Disk* does: new previews, size and camera fields the photo lacked.
  Metadata already set on that version stays.
- **Develop settings aren't copied** to the DNG: it's a new starting point.
- **Virtual copies:** a virtual copy shares its master's file, so the file is sent once and its DNG is stacked on the
  master photo, whichever of them you selected. (If the master is in Recently Deleted, the DNG goes on the copy.)
- **One send at a time.** While PureRAW is open from a send, the next one waits. ✕ on the run's row in the activity
  list stops waiting: PureRAW keeps running, but what it makes from that send isn't added.
- PureRAW drops a new batch while it's already open from another app (Lightroom Classic's DxO plug-in, say). The
  summary says so when that seems to have happened: close that window and send again.
- macOS only for now. The plug-in passes PureRAW a Lightroom version that doesn't exist, which keeps PureRAW from
  starting Lightroom Classic when it's done.

## Permissions

Chromaforge shows each one unticked when you install the plug-in. It needs all three.

| Permission | What for |
|---|---|
| **Read the catalog** (`catalog`) | To find each selected photo's file, and to tell raw files from others. It reads nothing else. |
| **Add files made from photos to the library** (`libraryAdd`) | To add each DNG PureRAW makes, stacked on its original. Chromaforge allows only files in the original's folder or below it. |
| **Start the program DxO PureRAW** (`programs`, bundle id `com.dxo-labs.PureRawv3.standalone`) | To open PureRAW with the photos you send. The plug-in passes it the list of files and the arguments its plug-in mode takes, nothing else. |

It asks for no network access and no other folders.
