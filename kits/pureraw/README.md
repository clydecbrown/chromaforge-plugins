# PureRAW test kit

A self-contained protocol for checking how a version of **DxO PureRAW** takes photos from a plug-in and hands its
results back, on macOS. It's written so that an agent (or a person) who has never seen this project can run it on a
fresh machine and send back a report. It's for a new PureRAW version (6, say), a new macOS, or a user's error report.

The plug-in it serves, **ccb: Edit in DxO PureRAW** for Chromaforge, starts PureRAW the way PureRAW's own Lightroom
Classic plug-in does. This kit checks whether that still works, and records every difference.

Contents: `README.md` (this file) and `probe.py` (Python 3, standard library only). Nothing else is needed: no
Chromaforge, no Lightroom.

## Rules (read first; they're not optional)

1. **One case per command, with a person at the screen.** PureRAW's window needs a human: they pick settings,
   press Process or Cancel, or close it, as each case says. Never send synthetic input (no `osascript` keystrokes or
   clicks, no accessibility automation, no `cliclick`). `probe.py` only shows a notification banner.
2. **Lightroom Classic must be closed** (`probe.py` refuses to run otherwise, except the optional `lr` case).
   - PureRAW can start Lightroom Classic by itself. With an installed `--lr-version`, PureRAW 2 started a working
     Lightroom that opened its last catalog. `probe.py` always passes `--lr-version 99.0` (a version no one has),
     which has stopped that so far.
   - If a run still starts Lightroom, `probe.py` says so. **Ask the person to quit it with ⌘Q. Never kill it**:
     it may have a catalog open, and killing it can damage the catalog.
3. **Never open, import into or change a real Lightroom catalog**, and never put files in Lightroom's own temp
   folder. `probe.py` gives PureRAW a private `TMPDIR` for each run, so Lightroom's DxO importer never sees anything.
4. **Copies only.** `probe.py` copies the samples into each run's folder. PureRAW writes its outputs next to those
   copies. The originals are only read.
5. **Observe, don't take apart.**
   - From PureRAW's app bundle, read only `Info.plist` and file names. That's what `probe.py inspect` shows.
   - Don't read anything else inside PureRAW's app bundle or its Lightroom plug-ins, and don't decompile,
     disassemble or run `strings` on binaries. What PureRAW does is learnt from its runs.
   - Never read anything inside an Adobe app bundle either; file names only.
6. **PureRAW's logs are private.** They hold licence and activation data. The full copies stay in each run's
   `logs/` and are **never shared**. Share only `log-excerpt.txt`, after reading it to check it holds nothing
   private (it filters licence lines out, but check).
7. **Disk:** each output DNG is about 3–4 times its input. A 12-photo case needs a few hundred MB.

## What's already known (PureRAW 2.6 and 3.7, macOS 14)

**Start:**

    open -n -b com.dxo-labs.PureRawv<N>.standalone --env TMPDIR=<private folder>/ \
        --args --as-lightroom-plugin --lr-version <version> --batch-file <file>

- The batch file has one absolute path per line, each ending in `\n`, with no quoting. PureRAW reads it and deletes
  it, whatever happens next.
- Without `--lr-version`, PureRAW refuses to start ("An error occurred while launching the DxO PureRAW plug-in").
- PureRAW goes straight to its "Process N photos" dialog, without its usual file list.

**Results**, in PureRAW's `$TMPDIR`: `dxo-imported-files-PureRawv<N>`, then an empty `dxo-trigger-PureRawv<N>`, then
PureRAW exits.

PureRAW 3 writes it once, at the end:

    DxO PureRAW 3
    END_OF_COLLECTION_SET_SECTION
    2026-10-24 10:13END_OF_COLLECTION_NAME_SECTION
    + selectCollection
    - <original A>          <- the photos processed, sorted by path (not in batch order)
    - <original B>
    0 <output of A>         <- "<index> <output>": an index into the "-" list above
    1 <output of B>

PureRAW 2 has no `END_OF_…` header, just a name line. It **adds a `- <original>` / `0 <output>` pair after every
photo**, and writes the trigger again each time. At the end it writes the same `+` / `-` / indexed block as
PureRAW 3. One reading fits both: an index points into the most recent run of `-` lines.

**Known behaviour, the same in both:**
- Only finished photos are listed: failed and cancelled ones are absent.
- Closing without processing, or a refused batch, leaves no results file and no trigger.
- A duplicate path is processed twice.
- A send to a PureRAW already open in plug-in mode is silently dropped.
- Outputs go to a `DxO/` folder next to each original by default, named `<name>-<ext>_DxO_<method>.dng`.

**Logs:** `~/Library/DxO_Labs/DxO PureRAW <N>/Logs/DxO PureRAW <N>.txt` (2 and 3), renamed to `.1`, `.2`, … at each
start. `probe.py` also looks in `~/Documents/LrClassicLogs/`, and keeps only the log that names the run's batch
file. The report should say where a new version logs.

## Setup

1. macOS on which the PureRAW version runs (PureRAW 6 needs macOS 15). A VM is fine.
2. PureRAW installed in `/Applications` and activated or on its trial. Run it once by hand first, to get past
   its first-run screens. Then quit it.
3. Lightroom Classic closed, or not installed.
4. A folder of **at least 4 raw files** from a camera PureRAW supports (DNG, CR2, CR3, NEF, ARW, RAF, …), on a disk
   with a few GB free. They're copied, never changed.
5. `python3 --version` (3.8 or later), and this folder copied somewhere writable. Runs go to `./runs/`.

## Steps

Run each command, tell the person what to do in PureRAW's window (the command prints it), and wait for the command
to finish. It finishes when PureRAW exits, so if PureRAW stays open, they quit it.

| # | Command | The person… | Answers |
|---|---|---|---|
| 0 | `python3 probe.py list` | — | which versions are installed, their bundle ids, the macOS they need |
| 1 | `python3 probe.py inspect <N>` | — | the bundle id, the version, the minimum macOS and the names of the binaries |
| 2 | `python3 probe.py run <N> order --samples <dir>` | processes all 4 | does the handoff work at all; results format; sorting; timing of the trigger(s) |
| 3 | `python3 probe.py run <N> cancel --samples <dir>` | presses Cancel during the 2nd photo | which photos the results list after a cancel |
| 4 | `python3 probe.py run <N> close --samples <dir>` | closes the window without processing | results file or not |

If step 2 is refused (PureRAW says an error occurred when launching, or never shows its window):
- stop and report what `inspect` shows and the error PureRAW gave;
- don't try other arguments by guessing.

Optional, if the core steps pass (each a separate run):

| Case | The person… | Answers |
|---|---|---|
| `names` | processes all | odd file names (spaces, quotes, Unicode, NFD, `$`, `#`, leading `-` or `+`) survive |
| `fail` | processes all, notes errors | unreadable, truncated and unwritable-folder photos are left out of the results |
| `bad` | processes what's offered | missing paths, blank lines, a relative path, CRLF, a `.txt`, a duplicate |
| `empty` | — | an empty batch |
| `nolr` | — | `--lr-version` still required? |
| `busy` | first opens PureRAW normally, then processes in the new window | a second instance, or dropped? |
| `lr` | first opens Lightroom Classic (with a scratch catalog only, never a real one), then processes; checks Lightroom imported nothing | needs `--lr-version <the open Lightroom's version>` |

After every run, look at the console output. It shows the events (each handoff file appearing, with its time),
the outputs, the results file, and whether Lightroom started.

## Report

Write `REPORT.md` next to `probe.py`:

```markdown
# PureRAW <N> handoff report

- Date; who ran it (person/agent); machine (Mac model or VM), macOS version
- PureRAW: version and bundle id (from `list`), minimum macOS
- Lightroom Classic installed? (version) Running during runs? (should be no)

## inspect
- The bundle id, version and minimum macOS, and the binaries' names

## Cases
For each case run: the run folder name; what the person did; seconds; the events line; outputs (count);
the results file (paste it, with the run folder's path shortened to <run>); whether Lightroom started;
"Return with result" from log-excerpt.txt.

## Verdict
One of: same as PureRAW 3 / same as PureRAW 2 / different, with a list of each difference.
Anything that surprised you.
```

**Send back:** `REPORT.md`, and for each run folder only:
- `report.json`, `batch-as-sent` and `tmp/` (the handoff files), after checking them for paths. They hold full paths:
  the samples, this kit's folder and the run folder. In them, replace your home folder (`/Users/<name>`) with
  `<home>`, and any other folder that names you or your machine (for example `/Volumes/<disk>/<folder>`) with
  `<folder>`;
- `log-excerpt.txt`, after checking it.

**Never send `logs/` or `in/`:** `logs/` is private and `in/` is large.

## Clean up

The runs are large (`in/` holds the DNGs). Delete `runs/` once the report has been received. If Lightroom was
started during a run, make sure the person quit it.
