#!/usr/bin/env python3
"""Probe how a DxO PureRAW version takes photos from a plug-in and hands results back (macOS).

See README.md before running anything. Usage (one case per command; a person operates PureRAW's window):
  probe.py list                                 the PureRAW apps installed in /Applications
  probe.py inspect <N>                          what DxO PureRAW <N>.app declares (no launch)
  probe.py run <N> <case> --samples <dir>       one case; see CASES
Options for run: --lr-version V (only for the "lr" case; every other case passes 99.0, a Lightroom no one has),
                 --app <path to DxO PureRAW N.app> (if it isn't in /Applications).
Each run gets runs/<stamp>-v<N>-<case>/ with: batch-as-sent, in/ (copies of the samples and PureRAW's outputs),
tmp/ (PureRAW's private TMPDIR, with the handoff files), report.json, log-excerpt.txt, and logs/ (PureRAW's full
logs: private, they hold licence data; never share logs/).
"""
import json, os, plistlib, re, shutil, subprocess, sys, time, unicodedata
from pathlib import Path

HERE = Path(__file__).resolve().parent
RAW_EXT = {".dng", ".cr2", ".cr3", ".nef", ".arw", ".raf", ".orf", ".rw2", ".pef", ".srw", ".3fr", ".iiq"}
LR_MAIN = "Adobe Lightroom Classic.app/Contents/MacOS/"
TIMEOUT = 45 * 60
# Log lines worth sharing; anything about licences or activation is never copied into the excerpt.
KEEP = re.compile(r"parseCommandLine|LrC with|Lightroom|lightroomExec|createLockFile|triggerPlugin|saved to DNG|"
                  r"Cancel|Return with result|Files list|could not|couldn't|error occurred|Processing file|processImage|"
                  r"batch|trigger|imported", re.I)
PRIVATE = re.compile(r"licen|activat|hostid|serial|e-?mail|i_Email|OutputLicense|SOAP|xmlns|token|account", re.I)


def die(msg):
    sys.exit(f"probe: {msg}")


def pids(pattern):
    r = subprocess.run(["pgrep", "-f", pattern], capture_output=True, text=True)
    return sorted(int(p) for p in r.stdout.split())


def ere(s):
    """A literal string as a pgrep (extended regex) pattern."""
    return re.sub(r"([.^$*+?()\[\]{}|\\])", r"\\\1", s)


def notify(title, text):
    # A notification banner only: no input is sent to any app.
    subprocess.run(["osascript", "-e", f"display notification {json.dumps(text)} with title {json.dumps(title)}"],
                   capture_output=True)


def apps():
    return sorted(Path("/Applications").glob("DxO PureRAW *.app"))


def app_info(app):
    with open(app / "Contents" / "Info.plist", "rb") as f:
        p = plistlib.load(f)
    return {k: p.get(k) for k in ("CFBundleIdentifier", "CFBundleShortVersionString", "CFBundleVersion",
                                  "CFBundleExecutable", "LSMinimumSystemVersion")}


def find_app(n, override):
    app = Path(override) if override else Path(f"/Applications/DxO PureRAW {n}.app")
    if not (app / "Contents" / "Info.plist").exists():
        die(f"{app} not found (installed: {[a.name for a in apps()]})")
    return app


def cmd_list():
    for a in apps():
        i = app_info(a)
        print(f"{a.name}: {i['CFBundleIdentifier']} {i['CFBundleShortVersionString']} (needs macOS "
              f"{i['LSMinimumSystemVersion']})")


def cmd_inspect(app):
    """Static facts only: Info.plist and the file names in Contents/MacOS. Nothing inside the app's own plug-ins."""
    print(json.dumps(app_info(app), indent=2))
    print("Contents/MacOS:", sorted(p.name for p in (app / "Contents/MacOS").iterdir()))


# ---- cases: each returns (batch lines, what the person at PureRAW's window should do) ----

def samples(d):
    if not Path(d).is_dir():
        die(f"--samples {d}: not a folder")
    s = sorted(p for p in Path(d).iterdir() if p.suffix.lower() in RAW_EXT and p.is_file())
    if len(s) < 2:
        die(f"--samples {d}: need at least 2 raw files ({', '.join(sorted(RAW_EXT))}); 4 or more is better")
    return s


def clone(src, dst):
    dst.parent.mkdir(parents=True, exist_ok=True)
    if subprocess.run(["cp", "-c", str(src), str(dst)], capture_output=True).returncode != 0:
        shutil.copy2(src, dst)          # another volume: a plain copy
    return dst


def make(S, d, n, name=lambda i, s: s.name, sub=lambda i: ""):
    return [clone(S[i % len(S)], d / "in" / sub(i) / name(i, S[i % len(S)])) for i in range(n)]


def c_order(S, d):
    a = make(S, d, 4, sub=lambda i: "ab"[i % 2])
    return [str(p) for p in (a[3], a[0], a[1], a[2])], "Process all 4 as they are."


NAMES = ["space name", 'quote"d', "apos'trophe", "back\\slash", "ünïcödé 日本", unicodedata.normalize("NFD", "nfd-é"),
         "dollar$HOME;semi&amp", "#hash%41", " leading-space", "trailing-space ", "-dash-first", "+plus-first",
         "0 digit-first"]


def c_names(S, d):
    p = make(S, d, len(NAMES), name=lambda i, s: NAMES[i] + s.suffix, sub=lambda i: "dir with spaces/é ü")
    return [str(x) for x in p], f"Process all ({len(NAMES)})."


def c_fail(S, d):
    unreadable = clone(S[0], d / "in" / ("1-unreadable" + S[0].suffix))
    good = clone(S[1 % len(S)], d / "in" / ("2-good" + S[1 % len(S)].suffix))
    cut = d / "in" / ("3-truncated" + S[0].suffix)
    cut.write_bytes(S[0].read_bytes()[:65536])
    ro = clone(S[1 % len(S)], d / "in" / "readonly" / ("4-readonly-folder" + S[1 % len(S)].suffix))
    os.chmod(unreadable, 0)
    os.chmod(ro.parent, 0o555)
    return [str(x) for x in (unreadable, good, cut, ro)], "Process all; note every error PureRAW shows."


def restore_modes(d):
    """c_fail makes a file unreadable and a folder read-only, for PureRAW to fail on. Put the modes back once the
    run is over, so the run folder can be deleted without sudo."""
    for p in [d, *d.rglob("*")]:
        if not p.is_symlink():
            os.chmod(p, 0o755 if p.is_dir() else 0o644)


def c_two(S, d):
    return [str(x) for x in make(S, d, 2)]


def c_cancel(S, d):
    p = make(S, d, 12, name=lambda i, s: f"{i:02}-{s.name}")
    return [str(x) for x in p], "Start processing, press Cancel during the 2nd photo, then quit PureRAW if it stays open."


def c_close(S, d):
    return c_two(S, d), "Don't process: close the window (quit PureRAW if it stays open)."


def c_nolr(S, d):
    return c_two(S, d), "No --lr-version: note any message; process both if it lets you."


def c_bad(S, d):
    a, b = clone(S[0], d / "in" / ("a" + S[0].suffix)), clone(S[1 % len(S)], d / "in" / ("b" + S[1 % len(S)].suffix))
    t = d / "in" / "notes.txt"
    t.write_text("not an image\n")
    lines = [str(a), str(d / "in" / ("missing" + S[0].suffix)), str(a), "", str(t), "in/b" + S[0].suffix, str(b) + "\r"]
    return lines, "Process whatever PureRAW offers; note every warning it shows."


def c_empty(S, d):
    return [], "Nothing to process: PureRAW should complain or exit. Quit it if it stays open."


def c_lr(S, d):
    return c_two(S, d), "Lightroom Classic is open: process both, then check that Lightroom imported nothing."


def c_busy(S, d):
    return c_two(S, d), "A normal PureRAW window is already open: process both in the new window, if one appears."


CASES = {"order": c_order, "cancel": c_cancel, "close": c_close, "names": c_names, "fail": c_fail, "bad": c_bad,
         "empty": c_empty, "nolr": c_nolr, "lr": c_lr, "busy": c_busy}


def log_files(n):
    """PureRAW's logs: ~/Library/DxO_Labs/DxO PureRAW <n>/Logs/ (2 and 3), plus ~/Documents/LrClassicLogs in case a
    version logs there. Each file once, even when the two folders are links to the same place."""
    home, seen, out = Path.home(), set(), []
    for p in [*(home / f"Library/DxO_Labs/DxO PureRAW {n}/Logs").glob("*"), *(home / "Documents/LrClassicLogs").glob("*PureRAW*")]:
        key = p.resolve()
        if key not in seen:
            seen.add(key)
            out.append(p)
    return out


def collect_logs(d, n, batch, logs_before):
    """Copies the logs of this run only: PureRAW 3 renames its old logs at every start, so a changed file isn't
    enough; the run's own log is the one that names its batch file (a path unique to the run)."""
    (d / "logs").mkdir(exist_ok=True)
    excerpt = []
    for p in log_files(n):
        if not p.is_file() or logs_before.get(str(p)) == p.stat().st_mtime:
            continue
        text = p.read_text(errors="replace")
        if str(batch) not in text:
            continue
        shutil.copy(p, d / "logs" / p.name)
        excerpt += [f"{p.name}: {line}" for line in text.splitlines() if KEEP.search(line) and not PRIVATE.search(line)]
    if not excerpt:
        excerpt = ["(no PureRAW log named this run's batch file: look for where this version logs, and say so in the report)"]
    (d / "log-excerpt.txt").write_text("\n".join(excerpt) + "\n")


def cmd_run(n, case, app, sample_dir, lr_version):
    info = app_info(app)
    bundle, exe = info["CFBundleIdentifier"], info["CFBundleExecutable"]
    pr_main = ere(f"{app.name}/Contents/MacOS/{exe}") + "( |$)"
    S = samples(sample_dir)
    lr_running, pr_before = bool(pids(LR_MAIN)), pids(pr_main)
    if case == "lr":
        if not lr_running:
            die("the lr case needs Lightroom Classic already open")
        if not lr_version:
            die("the lr case needs --lr-version <the open Lightroom's version, e.g. 15.3>")
    elif lr_running:
        die("Lightroom Classic is running: quit it first (only the lr case runs with it)")
    if case == "busy" and not pr_before:
        die("the busy case needs a normal PureRAW window already open")
    if case != "busy" and pr_before:
        die(f"PureRAW {n} is already running (pids {pr_before}): quit it first")

    d = HERE / "runs" / f"{time.strftime('%Y%m%d-%H%M%S')}-v{n}-{case}"
    (d / "tmp").mkdir(parents=True)
    try:
        lines, how = CASES[case](S, d)
        batch = d / "batch"
        batch.write_text("".join(x + "\n" for x in lines))
        shutil.copy(batch, d / "batch-as-sent")
        lr_arg = [] if case == "nolr" else ["--lr-version", lr_version if case == "lr" else "99.0"]
        logs_before = {str(p): p.stat().st_mtime for p in log_files(n)}

        print(f"run dir: {d}\n=== {case}: {how}", flush=True)
        notify(f"PureRAW probe: {case}", how)
        t0 = time.time()
        subprocess.run(["open", "-n", "-b", bundle, "--env", f"TMPDIR={d / 'tmp'}/", "--args", "--as-lightroom-plugin",
                        *lr_arg, "--batch-file", str(batch)], check=True)
        events, seen, listing = [], False, set()
        while time.time() - t0 < 20 and not seen:        # wait for the new PureRAW, so the loop below watches it
            seen = bool(set(pids(pr_main)) - set(pr_before))
            time.sleep(0.2)
        events.append(f"new PureRAW process seen: {seen}")
        while time.time() - t0 < TIMEOUT:
            now = {p.name: p.stat().st_mtime for p in (d / "tmp").iterdir()}
            for name in sorted(set(now) - listing):
                events.append(f"+{name} at {time.time() - t0:.2f}s")
            for name in sorted(listing - set(now)):
                events.append(f"-{name} at {time.time() - t0:.2f}s")
            listing = set(now)
            if not set(pids(pr_main)) - set(pr_before if case == "busy" else []):
                break
            time.sleep(0.1)
        else:
            events.append("TIMEOUT: PureRAW still running")
        seconds = round(time.time() - t0, 1)
        time.sleep(5)
        lr_started = []
        if not lr_running:
            for pid in pids(LR_MAIN):
                st = subprocess.run(["ps", "-o", "stat=,time=", "-p", str(pid)], capture_output=True, text=True).stdout.split()
                lr_started.append({"pid": pid, "state": st})

    finally:
        restore_modes(d)

    collect_logs(d, n, batch, logs_before)
    handoff = {p.name: p.read_text(errors="replace") for p in (d / "tmp").iterdir() if p.is_file()}
    outputs = sorted(str(p.relative_to(d)) for p in (d / "in").rglob("*") if p.is_file() and "DxO" in p.parts)
    report = {"case": case, "pureraw": info, "instructions": how, "seconds": seconds, "events": events,
              "batch_lines": lines, "batch_deleted": not batch.exists(), "handoff_files": handoff,
              "outputs": outputs, "lightroom_started": lr_started, "lr_args": lr_arg}
    (d / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False))
    print(json.dumps({k: report[k] for k in ("seconds", "events", "batch_deleted", "outputs")}, ensure_ascii=False, indent=1))
    for k, v in handoff.items():
        print(f"--- {k} ({len(v)} chars):\n{v[:4000]}")
    if lr_started:
        print(f"\n!!! Lightroom Classic was started during the run: {lr_started}. Ask the person to quit it with "
              "Cmd-Q. Never kill it: it may have a catalog open.")


def main():
    a = sys.argv[1:]
    opt = lambda name: a[a.index(name) + 1] if name in a and a.index(name) + 1 < len(a) else None
    if a[:1] == ["list"]:
        return cmd_list()
    if len(a) >= 2 and a[0] == "inspect":
        return cmd_inspect(find_app(a[1], opt("--app")))
    if len(a) >= 3 and a[0] == "run":
        if a[2] not in CASES:
            die(f"unknown case {a[2]!r}; cases: {', '.join(CASES)}")
        if not opt("--samples"):
            die("run needs --samples <folder with raw files>")
        return cmd_run(a[1], a[2], find_app(a[1], opt("--app")), opt("--samples"), opt("--lr-version"))
    print(__doc__)
    sys.exit(2)


if __name__ == "__main__":
    main()
