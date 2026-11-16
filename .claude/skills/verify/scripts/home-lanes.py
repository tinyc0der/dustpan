#!/usr/bin/env python3
"""Run the live text lanes for the Home screen against a release binary and save one capture per lane.

Usage: home-lanes.py OUTDIR [--bin PATH] [--only 1,2,...]

Lanes 0 to 13 follow docs/M0-checklist.md "Draw the TUI frame, Home and help overlay (M0-6)". Lanes 14 to 23 cover the
About pages, History, Check and the mouse, with fixture homes under OUTDIR/fixtures. Nothing here runs Mole. Each lane prints PASS or FAIL
with the reason and writes OUTDIR/<slug>.txt. The exit status is 1 when any lane fails.
"""
import argparse
import os
import re
import shutil
import subprocess
import sys
import time
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
DRIVE = os.path.join(HERE, "pty-drive.sh")
SESSION = "dustpan-lane"


def drive(*args, check=True):
    done = subprocess.run([DRIVE, *args], capture_output=True, text=True)
    if check and done.returncode != 0:
        raise RuntimeError(f"pty-drive {' '.join(args)} failed: {done.stderr}")
    return done.stdout


ENTRIES = ["Clean", "Uninstall", "Analyze", "Optimize", "Status", "Purge", "Installers", "Slim", "History", "Check a path"]


def cells(text):
    return sum(2 if unicodedata.east_asian_width(c) in "WF" else 0 if unicodedata.combining(c) else 1 for c in text)


HINTS = ["↑↓ Move", "Enter Open", "0-9 Jump", "Tab Detail", ", Settings", "? Help", "q Quit", "Ctrl+C Quit now"]
ENTRY_ROW = r"\d  {} +\S.* (soon|ready)$"


def wrapped(screen, expected):
    """Return why a screen shows a wrap, or None. Each expected text must appear whole on exactly one line, and no line
    may start with the tail of an expected text that sits whole elsewhere or not at all."""
    lines = screen.split("\n")
    for text in expected:
        pattern = re.compile(text if text.startswith("\\d") else re.escape(text))
        whole = [line for line in lines if pattern.search(line)]
        if len(whole) != 1:
            return f"{text!r} appears whole on {len(whole)} lines"
    for text in (t for t in expected if not t.startswith("\\d")):
        words = text.split(" ")
        for skip in range(1, len(words)):
            tail = " ".join(words[skip:])
            for line in lines:
                if line.lstrip().startswith(tail) and text not in line:
                    return f"line {line!r} starts with the tail {tail!r} of {text!r}"
    return None


def result_paths(screen):
    """The paths a Check screen judged, newest first, each put back together from its wrapped lines."""
    lines = screen.split("\n")
    rules = [i for i, line in enumerate(lines) if line and set(line) <= {"─", "-"}]
    paths, chunks = [], []
    for line in lines[rules[1] + 1 : rules[-1]]:
        if re.match(r"^ {3}(Kept|Eligible)  ", line):
            paths.append("".join(chunks))
            chunks = []
        elif line.startswith(" ") and not line.startswith("   "):
            chunks.append(line[1:])
    return paths


def same_path(shown, expected):
    """A wrapped line loses a trailing space to the screen capture, so compare without spaces."""
    return shown.replace(" ", "") == expected.replace(" ", "")


def start(binary, cols, rows, *extra, env=None):
    argv = ["env", *(f"{k}={v}" for k, v in (env or {}).items()), binary, *extra]
    drive("start", SESSION, str(cols), str(rows), "--", *argv)
    drive("wait", SESSION, "q Quit", "5")


def box_borders_align(screen):
    lines = screen.split("\n")
    for top, line in enumerate(lines):
        left_corner, right_corner = ("┌", "┐") if "┌" in line else ("+", "+") if line.count("+") == 2 else (None, None)
        if not left_corner:
            continue
        left = cells(line[: line.index(left_corner)])
        right = cells(line[: line.rindex(right_corner)])
        wall = "│" if left_corner == "┌" else "|"
        for below in lines[top + 1 :]:
            probe = below.ljust(right + 1)
            prefix_cells = 0
            left_ok = right_ok = False
            for char in probe:
                if prefix_cells == left and char == wall:
                    left_ok = True
                if prefix_cells == right and char == wall:
                    right_ok = True
                prefix_cells += cells(char)
            if char_is_bottom(below, left_corner):
                return True
            if not (left_ok and right_ok):
                return f"border off its corners on row: {below!r}"
        return "no bottom corners found"
    return "no box on screen"


def char_is_bottom(line, left_corner):
    return ("└" if left_corner == "┌" else "+") in line


class Lanes:
    def __init__(self, out, binary):
        self.out = out
        self.binary = binary
        self.failed = False

    def save(self, slug, text):
        with open(os.path.join(self.out, slug), "w") as handle:
            handle.write(text)

    def report(self, lane, ok, why):
        self.failed |= ok is not True
        print(f"lane {lane}: {'PASS' if ok is True else 'FAIL'} {why if ok is True else ok or why}")

    def lane1(self):
        start(self.binary, 120, 40)
        screen = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("home.txt", f"=== dustpan @ 120x40 ===\n{screen}")
        missing = [e for e in ("Clean", "Uninstall", "Optimize", "Analyze", "Status") if e not in screen]
        header = screen.split("\n")[0]
        if missing:
            return self.report(1, f"missing Mole entries {missing}", "")
        if not re.search(r"\d+(\.\d)? (GB|TB|MB) free", header):
            return self.report(1, f"header lacks free space: {header!r}", "")
        self.report(1, True, f"every Mole menu entry present; header {header.strip()!r}")

    def lane2(self):
        start(self.binary, 120, 40)
        drive("keys", SESSION, "Down", "Down", "Down", "Down", "k")
        time.sleep(0.2)
        screen = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("nav.txt", screen)
        marked = [line for line in screen.split("\n") if "➤" in line]
        ok = len(marked) == 1 and re.search(r"➤ 4\s+Optimize", marked[0])
        self.report(2, True if ok else f"cursor rows: {marked}", "cursor on entry 4 (Optimize)")

    def lane3(self):
        start(self.binary, 120, 40)
        drive("keys", SESSION, "?")
        time.sleep(0.2)
        screen = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("help.txt", screen)
        needles = ["↑ ↓ j k", "Enter → l", "0..9", "Settings", "Show or close this help", "Quit now", "Wheel, click", "9 History", "0 Check a path"]
        missing = [n for n in needles if n not in screen]
        aligned = box_borders_align(screen)
        self.report(3, True if not missing and aligned is True else missing or aligned, "help lists every Home key, box borders aligned")

    def lane4(self):
        start(self.binary, 120, 40)
        drive("resize", SESSION, "70", "20")
        time.sleep(0.3)
        screen = drive("screen", SESSION)
        drive("keys", SESSION, "?")
        time.sleep(0.2)
        help_screen = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("narrow.txt", f"=== Home @ 70x20 ===\n{screen}\n=== Help @ 70x20 ===\n{help_screen}")
        lines = screen.rstrip("\n").split("\n")
        footer = lines[-1]
        shown = [hint for hint in HINTS if hint in footer]
        expected = ["dustpan › Home", "free", *shown]
        expected += [ENTRY_ROW.format(label) for label in ENTRIES]
        bad_wrap = wrapped(screen, expected)
        ok = (
            bad_wrap is None
            and "q Quit" in footer
            and "? Help" in footer
            and "Tab Detail" in footer
            and "Ctrl+C" not in footer
            and "Wheel" not in footer
            and "soon" in screen
        )
        aligned = box_borders_align(help_screen)
        self.report(4, True if ok and aligned is True else (bad_wrap or footer if not ok else aligned), f"footer {footer.strip()!r}; help borders aligned")

    def lane12(self):
        start(self.binary, 80, 24)
        before = drive("screen", SESSION)
        drive("keys", SESSION, "Down", "Down", "Tab")
        time.sleep(0.2)
        detail = drive("screen", SESSION)
        drive("keys", SESSION, "?")
        time.sleep(0.2)
        help_screen = drive("screen", SESSION)
        drive("keys", SESSION, "?", "Tab")
        time.sleep(0.2)
        back = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("tab.txt", f"=== list @ 80x24 ===\n{before}\n=== after Down Down Tab ===\n{detail}\n=== help ===\n{help_screen}\n=== Tab again ===\n{back}")
        footer = before.rstrip("\n").split("\n")[-1]
        ok = (
            "Tab Detail" in footer
            and "Arrives in M1" in detail
            and "Uninstall" not in detail
            and "Tab" in help_screen
            and "Uninstall" in back
            and wrapped(detail, ["dustpan › Home", "Status  Arrives in M1"]) is None
        )
        self.report(12, True if ok else f"Tab did not swap panes at 80x24: {detail!r}", "Tab swaps list and detail at 80x24, hint and help entry present")

    def lane13(self):
        probe = os.path.join(HERE, "hangup-without-sighup.py")
        done = subprocess.run([sys.executable, probe, self.binary], capture_output=True, text=True, timeout=30)
        self.save("hangup-no-sighup.txt", done.stdout + done.stderr)
        self.report(13, True if done.returncode == 0 else done.stdout.strip().splitlines()[-1:] or "probe failed", "a terminal that closes without SIGHUP still ends the process")

    def lane0(self):
        shown = ["dustpan › Home", "free", "q Quit", "? Help"] + [ENTRY_ROW.format(label) for label in ENTRIES]
        start(self.binary, 70, 20)
        screen = drive("screen", SESSION)
        drive("stop", SESSION)
        clean = wrapped(screen, shown)
        chopped = "\n".join(piece for line in screen.split("\n") for piece in (line[:22], line[22:]))
        wrapped_bad = wrapped(chopped, shown)
        ok = clean is None and wrapped_bad is not None
        self.report(0, True if ok else f"wrap check: clean screen {clean!r}, wrapped screen {wrapped_bad!r}", "the wrap check passes the real capture and fails the same capture wrapped at 22 cells")

    def lane5(self):
        start(self.binary, 120, 40)
        drive("resize", SESSION, "50", "12")
        time.sleep(0.3)
        small = drive("screen", SESSION)
        drive("resize", SESSION, "120", "40")
        time.sleep(0.3)
        back = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("too-small.txt", f"=== 50x12 ===\n{small}\n=== back at 120x40 ===\n{back}")
        ok = "needs 60 by 16" in small and "Resize to continue" in small and "Clean" in back and "soon" in back
        self.report(5, True if ok else "no message at 50x12 or no Home after resize", "minimum-size message, then Home returns")

    def shell_session(self):
        drive("start", SESSION, "120", "40")
        drive("text", SESSION, f"{self.binary}")
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "q Quit", "5")
        return drive("pid", SESSION).strip()

    def stty_facts(self):
        drive("text", SESSION, "echo exit=$?; stty -a")
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "speed", "5")
        time.sleep(0.2)
        screen = drive("screen", SESSION)
        info = drive("info", SESSION).strip()
        return screen, info

    def judge_restore(self, screen, info, expect_exit):
        flags = " ".join(re.findall(r"[-\w]+", screen.split("exit=")[-1]))
        cooked = " icanon " in f" {flags} " and " echo " in f" {flags} " and " isig " in f" {flags} "
        ok = cooked and "cursor_visible=1" in info and "alternate_screen=0" in info and f"exit={expect_exit}" in screen
        return ok, info

    def lane6(self):
        self.shell_session()
        drive("keys", SESSION, "C-c")
        drive("wait", SESSION, "$ ", "3", check=False)
        time.sleep(0.3)
        screen, info = self.stty_facts()
        drive("stop", SESSION)
        ok, info = self.judge_restore(screen, info, 130)
        self.save("restore.txt", f"after Ctrl+C\n{info}\n{screen}")
        self.report(6, True if ok else f"not restored: {info}", f"cooked mode, cursor shown, main screen, exit 130 ({info})")

    def lane7(self):
        shell_pid = self.shell_session()
        child = subprocess.run(["pgrep", "-P", shell_pid], capture_output=True, text=True).stdout.split()[0]
        os.kill(int(child), 15)
        time.sleep(0.5)
        screen, info = self.stty_facts()
        drive("stop", SESSION)
        ok, info = self.judge_restore(screen, info, 130)
        self.save("sigterm.txt", f"after SIGTERM to pid {child}\n{info}\n{screen}")
        self.report(7, True if ok else f"not restored: {info}", f"SIGTERM restored the terminal, exit 130 ({info})")

    def lane8(self):
        start(self.binary, 120, 40, env={"NO_COLOR": "1"})
        drive("keys", SESSION, "Down", "Down")
        plain_text = drive("screen", SESSION)
        raw = subprocess.run(["tmux", "-L", "dustpan-verify", "-f", "/dev/null", "capture-pane", "-p", "-e", "-t", SESSION], capture_output=True, text=True).stdout
        drive("keys", SESSION, "Enter")
        time.sleep(0.2)
        toast = drive("screen", SESSION)
        drive("stop", SESSION)
        colors = re.findall(r"\x1b\[[0-9;]*m", raw)
        colored = [c for c in colors if re.search(r"(^|;)(3[0-7]|4[0-7]|9[0-7]|10[0-7]|38|48)(;|m)", c[2:])]
        start(self.binary, 120, 40, "--ascii")
        drive("keys", SESSION, "?")
        time.sleep(0.2)
        ascii_help = drive("screen", SESSION)
        drive("keys", SESSION, "?")
        drive("keys", SESSION, "Enter")
        time.sleep(0.2)
        ascii_home = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save(
            "plain.txt",
            f"=== NO_COLOR=1 (escape codes seen: {sorted(set(colors))}) ===\n{plain_text}\n=== NO_COLOR=1 after Enter ===\n{toast}\n"
            f"=== --ascii home ===\n{ascii_home}\n=== --ascii help ===\n{ascii_help}",
        )
        non_ascii = sorted({c for c in ascii_help + ascii_home if ord(c) > 127})
        words = "➤" in plain_text and "Arrives in milestone M" in toast
        ok = not colored and not non_ascii and words
        self.report(8, True if ok else f"color escapes {colored}, non-ASCII under --ascii {non_ascii}, cursor mark/words {words}", "NO_COLOR emits no color, --ascii emits only ASCII, states keep symbol and word")

    def lane11(self):
        start(self.binary, 120, 40)
        pid = drive("pid", SESSION).strip()
        drive("stop", SESSION)
        time.sleep(1.0)
        alive = subprocess.run(["ps", "-p", pid, "-o", "pid="], capture_output=True, text=True).stdout.strip()
        self.save("hangup.txt", f"pid {pid} after the terminal closed: {'still running' if alive else 'gone'}\n")
        if alive:
            os.kill(int(pid), 9)
        self.report(11, True if not alive else f"pid {pid} outlived its terminal", "closing the terminal ends the process")

    def lane10(self):
        out_path = os.path.join(self.out, "no-tty.txt")
        with open(out_path, "w") as sink:
            done = subprocess.run([self.binary], stdin=subprocess.DEVNULL, stdout=sink, timeout=10)
        text = open(out_path).read()
        ok = done.returncode == 0 and "Usage:" in text and "\x1b" not in text
        self.report(10, True if ok else f"exit {done.returncode}, output {text[:80]!r}", "prints help, no escape codes, exit 0")

    def fixture_home(self, name, log=None, dirs=()):
        """A folder to use as HOME. It never holds anything the run did not put there."""
        home = os.path.realpath(os.path.join(self.out, "fixtures", name))
        shutil.rmtree(home, ignore_errors=True)
        os.makedirs(home)
        for sub in dirs:
            os.makedirs(os.path.join(home, sub))
        if log is not None:
            folder = os.path.join(home, "Library", "Logs", "dustpan")
            os.makedirs(folder)
            with open(os.path.join(folder, "operations.log"), "w") as handle:
                handle.write(log)
        return home

    def start_in(self, home, cols, rows, *extra):
        env = {"HOME": home, "XDG_CONFIG_HOME": "", "XDG_STATE_HOME": ""}
        start(self.binary, cols, rows, *extra, env=env)

    def listing(self, home):
        found = []
        for base, folders, files in os.walk(home):
            found += [os.path.relpath(os.path.join(base, name), home) for name in folders + files]
        return sorted(found)

    def lane14(self):
        start(self.binary, 120, 40)
        drive("keys", SESSION, "Down", "Down", "Enter")
        drive("wait", SESSION, "What it will do", "3")
        page = drive("screen", SESSION)
        drive("keys", SESSION, "Escape")
        time.sleep(0.2)
        back = drive("screen", SESSION)
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "What it will do", "3")
        drive("keys", SESSION, "Left")
        time.sleep(0.2)
        by_left = drive("screen", SESSION)
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "What it will do", "3")
        drive("keys", SESSION, "BSpace")
        time.sleep(0.2)
        by_backspace = drive("screen", SESSION)
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "What it will do", "3")
        drive("click", SESSION, "12", "1")
        time.sleep(0.2)
        by_click = drive("screen", SESSION)
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "What it will do", "3")
        drive("keys", SESSION, "0")
        time.sleep(0.2)
        to_check = drive("screen", SESSION)
        drive("stop", SESSION)
        start(self.binary, 60, 16)
        drive("keys", SESSION, "7", "Enter")
        drive("wait", SESSION, "What it will do", "3")
        small = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("about.txt", f"=== Analyze @ 120x40 ===\n{page}\n=== Esc ===\n{back}\n=== Left ===\n{by_left}\n=== Backspace ===\n{by_backspace}\n=== click Home crumb ===\n{by_click}\n=== key 0 ===\n{to_check}\n=== Installers @ 60x16 ===\n{small}")

        def on_analyze(screen):
            marked = [line for line in screen.split("\n") if "➤" in line]
            return len(marked) == 1 and "Analyze" in marked[0]

        about_ok = all(
            text in page
            for text in ["dustpan › Home › Analyze", "What it will do", "Arrives in milestone M1.", "Until then: Check a path (0) or History (9)", "Esc Back"]
        )
        returns = all(on_analyze(screen) for screen in (back, by_left, by_backspace, by_click))
        small_ok = (
            "Arrives in milestone M6." in small
            and "Until then:" in small
            and "q Quit" in small.rstrip("\n").split("\n")[-1]
            and wrapped(small, ["Arrives in milestone M6.", "Until then: Check a path (0) or History (9)"]) is None
        )
        ok = about_ok and returns and "dustpan › Home › Check a path" in to_check and small_ok
        self.report(14, True if ok else f"about page {about_ok}, returns {returns}, small {small_ok}", "About page: text, Esc/Left/Backspace/click return to the same row, 0 opens Check, fits 60x16")

    LOG_HEAD = "\n# ========== clean run=20261116094500-42-9f3a session started at 2026-11-16 09:45:00 +0700 ==========\n"
    LOG_TAIL = "# ========== clean run=20261116094500-42-9f3a session ended at 2026-11-16 09:45:02, 25 items, 1.5 MB ==========\n"

    def history_log(self, rows=25):
        body = "".join(
            f"[2026-11-16 09:45:01] [clean run=20261116094500-42-9f3a] REMOVED /home/a/.cache/item-{i:02d} (sink=delete size=1500000)\n"
            for i in range(rows)
        )
        return self.LOG_HEAD + body + self.LOG_TAIL

    def lane15(self):
        home = self.fixture_home("history-log", log=self.history_log())
        before = self.listing(home)
        self.start_in(home, 120, 24)
        drive("keys", SESSION, "9", "Enter")
        drive("wait", SESSION, "Recent sessions", "3")
        top = drive("screen", SESSION)
        for _ in range(12):
            drive("wheel", SESSION, "down", "10", "8")
        time.sleep(0.3)
        scrolled = drive("screen", SESSION)
        for _ in range(12):
            drive("wheel", SESSION, "up", "10", "8")
        time.sleep(0.3)
        top_again = drive("screen", SESSION)
        drive("keys", SESSION, "Escape")
        time.sleep(0.2)
        home_screen = drive("screen", SESSION)
        drive("stop", SESSION)
        self.start_in(home, 60, 16)
        drive("keys", SESSION, "9", "Enter")
        drive("wait", SESSION, "Recent sessions", "3")
        small = drive("screen", SESSION)
        drive("keys", SESSION, "Down", "Down", "j")
        time.sleep(0.2)
        small_scrolled = drive("screen", SESSION)
        drive("stop", SESSION)
        after = self.listing(home)
        self.save("history.txt", f"=== top @ 120x24 ===\n{top}\n=== wheel down x12 ===\n{scrolled}\n=== wheel up x12 ===\n{top_again}\n=== Esc ===\n{home_screen}\n=== @ 60x16 ===\n{small}\n=== Down Down j ===\n{small_scrolled}\n=== fixture before ===\n{before}\n=== fixture after ===\n{after}\n")
        marked = [line for line in home_screen.split("\n") if "➤" in line]
        ok = (
            "dustpan › Home › History" in top
            and re.search(r"2026-11-16 09:45 +clean +25 +1\.5 MB +done", top)
            and "Deletion audit" in top
            and "Recent sessions" not in scrolled
            and "Log: " in scrolled
            and "Recent sessions" in top_again
            and len(marked) == 1
            and "History" in marked[0]
            and "Recent sessions" in small
            and small_scrolled != small
            and before == after
        )
        self.report(15, True if ok else "history screen did not match; see history.txt", "History over a fixture log: session row, audit, wheel and key scroll, Esc to the same row, fixture untouched")

    def lane16(self):
        empty = self.fixture_home("history-empty")
        self.start_in(empty, 80, 24)
        drive("keys", SESSION, "9", "Enter")
        drive("wait", SESSION, "Nothing has been cleaned yet.", "3")
        nothing = drive("screen", SESSION)
        drive("stop", SESSION)
        made = self.listing(empty)
        broken = self.fixture_home("history-unreadable", dirs=["Library/Logs/dustpan/operations.log"])
        self.start_in(broken, 80, 24)
        drive("keys", SESSION, "9", "Enter")
        drive("wait", SESSION, "could not be read", "3")
        error = drive("screen", SESSION)
        drive("keys", SESSION, "Escape")
        time.sleep(0.2)
        after = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("history-empty.txt", f"=== empty home ===\n{nothing}\n=== folders in the empty home after ===\n{made}\n=== unreadable log ===\n{error}\n=== Esc ===\n{after}")
        ok = (
            "Every removal dustpan makes will show up here." in nothing
            and made == []
            and "cannot read" in error
            and "not a regular file" in error
            and "Welcome." in after
        )
        self.report(16, True if ok else "empty or unreadable history wrong; see history-empty.txt", "History: plain empty state without creating the log folder; an unreadable log shows its error and Esc still works")

    def lane17(self):
        home = self.fixture_home("check", dirs=["scratch", "My Folder (1)"])
        self.start_in(home, 120, 40)
        drive("keys", SESSION, "0", "Enter")
        drive("wait", SESSION, "Type a path", "3")
        empty = drive("screen", SESSION)
        drive("keys", SESSION, "q", "j", "1")
        time.sleep(0.2)
        typed = drive("screen", SESSION)
        alive = drive("info", SESSION)
        drive("keys", SESSION, "C-u")
        time.sleep(0.2)
        cleared = drive("screen", SESSION)
        drive("text", SESSION, "/System")
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "Kept", "3")
        kept = drive("screen", SESSION)
        drive("text", SESSION, "~/scratch")
        drive("keys", SESSION, "Enter")
        drive("wait", SESSION, "Eligible", "3")
        two = drive("screen", SESSION)
        drive("paste", SESSION, home + "/My\\ Folder\\ \\(1\\) ")
        time.sleep(0.4)
        pasted = drive("screen", SESSION)
        pasted_field = pasted.split("\n")[2]
        drive("keys", SESSION, "Escape")
        time.sleep(0.2)
        back = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("check.txt", f"=== open ===\n{empty}\n=== q j 1 typed ===\n{typed}\n{alive}\n=== Ctrl+U ===\n{cleared}\n=== /System Enter ===\n{kept}\n=== ~/scratch Enter ===\n{two}\n=== pasted escaped path, no Enter ===\n{pasted}\n=== Esc ===\n{back}")
        lines = two.split("\n")
        kept_at = next((i for i, line in enumerate(lines) if "Kept" in line), -1)
        eligible_at = next((i for i, line in enumerate(lines) if "Eligible" in line), -1)
        ok = (
            "Path  qj1" in typed
            and "dead=0" in alive
            and "Type a path, e.g. ~/Library/Caches" in cleared
            and re.search(r"Kept +\S", kept)
            and 0 <= eligible_at < kept_at
            and "dustpan may clean this when Clean arrives" in two
            and same_path(result_paths(two)[0], home + "/scratch")
            and "Type a path, e.g." in pasted_field
            and "My\\ Folder" not in pasted
            and same_path(result_paths(pasted)[0], home + "/My Folder (1)")
            and "\\" not in result_paths(pasted)[0]
            and "Welcome." in back
        )
        self.report(17, True if ok else "check screen did not match; see check.txt", "Check: q j 1 are text, Ctrl+U clears, Kept and Eligible verdicts newest first, ~ expands, a pasted escaped path is cleaned up")

    def lane18(self):
        start(self.binary, 120, 40)
        drive("click", SESSION, "10", "6")
        time.sleep(0.2)
        selected = drive("screen", SESSION)
        drive("click", SESSION, "10", "6")
        drive("wait", SESSION, "What it will do", "3")
        opened = drive("screen", SESSION)
        drive("click", SESSION, "12", "1")
        time.sleep(0.2)
        home = drive("screen", SESSION)
        drive("wheel", SESSION, "down", "10", "8")
        drive("wheel", SESSION, "down", "10", "8")
        time.sleep(0.2)
        wheeled = drive("screen", SESSION)
        drive("stop", SESSION)
        start(self.binary, 60, 16)
        drive("click", SESSION, "10", "13")
        time.sleep(0.2)
        small_selected = drive("screen", SESSION)
        drive("click", SESSION, "10", "13")
        drive("wait", SESSION, "Type a path", "3")
        small_opened = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("mouse.txt", f"=== click row 6 ===\n{selected}\n=== click again ===\n{opened}\n=== click Home crumb ===\n{home}\n=== wheel down x2 ===\n{wheeled}\n=== 60x16 click row 13 ===\n{small_selected}\n=== click again ===\n{small_opened}")

        def marked(screen):
            rows = [line for line in screen.split("\n") if "➤" in line]
            return rows[0] if len(rows) == 1 else None

        ok = (
            marked(selected)
            and "Uninstall" in marked(selected)
            and "What it will do" not in selected
            and "dustpan › Home › Uninstall" in opened
            and marked(home)
            and "Uninstall" in marked(home)
            and marked(wheeled)
            and "Optimize" in marked(wheeled)
            and marked(small_selected)
            and "Check a path" in marked(small_selected)
            and "dustpan › Home › Check a path" in small_opened
        )
        self.report(18, True if ok else "mouse did not match; see mouse.txt", "click selects, a second click opens, the Home crumb returns, the wheel moves the selection, 60x16 click opens Check")


    def open_check(self, home, cols, rows):
        self.start_in(home, cols, rows)
        drive("keys", SESSION, "0", "Enter")
        drive("wait", SESSION, "Type a path", "3")

    def submit(self):
        drive("keys", SESSION, "Enter")
        time.sleep(0.4)
        return drive("screen", SESSION)

    @staticmethod
    def write_whitelist(home, body, mode=0o644):
        folder = os.path.join(home, ".config", "dustpan")
        os.makedirs(folder, exist_ok=True)
        path = os.path.join(folder, "whitelist")
        if os.path.exists(path):
            os.chmod(path, 0o644)
        with open(path, "w") as handle:
            handle.write(body)
        os.chmod(path, mode)

    def lane19(self):
        home = self.fixture_home("check-many", dirs=["scratch", "keepme", "My Folder (1)", "with space", "a "])
        self.write_whitelist(home, "~/keepme\n")
        before = self.listing(home)
        scratch, keepme, folder = f"{home}/scratch", f"{home}/keepme", f"{home}/My Folder (1)"
        escaped = f"{home}/My\\ Folder\\ \\(1\\)"
        cases = {
            "two lines": ([f"{scratch}\n{keepme}"], [scratch, keepme], None),
            "terminal two-file drag": ([f"{escaped} {keepme} "], [folder, keepme], None),
            "single-quoted pair": ([f"'{scratch}' '{keepme}' "], [scratch, keepme], None),
            "double-quoted pair": ([f'"{scratch}" "{keepme}"'], [scratch, keepme], None),
            "escapes then a quote": ([f"{escaped} '{keepme}'"], [folder, keepme], None),
            "two drags before Enter": ([scratch, keepme], [keepme, scratch], None),
            "plain space, existing": ([f"{home}/with space"], [f"{home}/with space"], None),
            "plain space, missing": ([f"{home}/Backup ~2024"], [], "This looks like several paths, or one path with spaces. Quote it, or paste one path at a time."),
            "child of the folder a-space": ([f"{home}/a /c"], [], "This looks like several paths, or one path with spaces."),
            "unmatched quote": ([f"'{scratch}"], [], "This paste has an unmatched quote. Paste one path at a time, or type it."),
        }
        captures = {}
        for name, (pastes, _, _) in cases.items():
            self.open_check(home, 120, 40)
            for text in pastes:
                drive("paste", SESSION, text)
                time.sleep(0.3)
            captures[name] = drive("screen", SESSION)
            drive("stop", SESSION)
        after = self.listing(home)
        out = "".join(f"=== {name} ===\n{screen}\n" for name, screen in captures.items())
        self.save("check-many.txt", out + f"=== fixture before ===\n{before}\n=== fixture after ===\n{after}\n")

        def flat(screen):
            return " ".join(screen.replace("│", " ").split())

        problems = []
        for name, (_, expected, message) in cases.items():
            screen = captures[name]
            shown = result_paths(screen)
            if len(shown) != len(expected) or not all(same_path(a, b) for a, b in zip(shown, expected)):
                problems.append(f"{name}: judged {shown}")
            if "Type a path, e.g." not in screen.split("\n")[2]:
                problems.append(f"{name}: the field is not empty")
            if message and message not in flat(screen):
                problems.append(f"{name}: no message")
            if not message and "unmatched" in screen:
                problems.append(f"{name}: a message was shown")
        pair = captures["single-quoted pair"]
        lines = pair.split("\n")
        eligible_at = next((i for i, line in enumerate(lines) if "Eligible" in line), -1)
        kept_at = next((i for i, line in enumerate(lines) if re.search(r"Kept +protected by your whitelist", line)), -1)
        if not 0 <= eligible_at < kept_at:
            problems.append("the quoted pair is not scratch Eligible then keepme Kept")
        if before != after:
            problems.append("the fixture changed")
        self.report(19, True if not problems else f"paste cases did not match: {problems}; see check-many.txt", "Check: each paste is checked at once by the shell word rules (quotes, escapes, lines, two drags), nothing is glued or guessed, and the field stays empty")

    def lane20(self):
        home = self.fixture_home("check-whitelist", dirs=["keepme"])
        self.open_check(home, 120, 40)
        first = self.type_and_submit(f"{home}/keepme")
        self.write_whitelist(home, "~/keepme\n")
        second = self.type_and_submit(f"{home}/keepme")
        self.write_whitelist(home, "\n")
        third = self.type_and_submit(f"{home}/keepme")
        drive("stop", SESSION)
        self.save("check-whitelist.txt", f"=== no whitelist ===\n{first}\n=== whitelist now names keepme ===\n{second}\n=== whitelist emptied ===\n{third}")

        def newest(screen):
            row = next((line for line in screen.split("\n") if re.match(r"^ {3}(Kept|Eligible)  ", line)), "")
            return row.strip()

        ok = (
            newest(first).startswith("Eligible")
            and newest(second).startswith("Kept")
            and "whitelist" in newest(second)
            and newest(third).startswith("Eligible")
        )
        self.report(20, True if ok else f"whitelist edit not seen: {newest(first)!r} {newest(second)!r} {newest(third)!r}", "Check: an edit to the whitelist is seen by the next Enter on the same screen")

    def type_and_submit(self, text):
        drive("text", SESSION, text)
        return self.submit()

    def lane21(self):
        home = self.fixture_home("check-unreadable", dirs=["keepme"])
        self.write_whitelist(home, "~/keepme\n", mode=0)
        shots = {}
        for cols in (60, 80, 100):
            self.open_check(home, cols, 24)
            shots[cols] = self.type_and_submit("/usr")
            drive("stop", SESSION)
        self.write_whitelist(home, "\n")
        self.save("check-error.txt", "".join(f"=== {cols} columns ===\n{shot}\n" for cols, shot in shots.items()))

        def flat(screen):
            return " ".join(screen.split())

        ok = all("nothing is cleaned until it can be read" in flat(shot) and "Permission denied" in flat(shot) for shot in shots.values())
        self.report(21, True if ok else "the cause is cut off; see check-error.txt", "Check: an unreadable whitelist's error wraps over several lines at 60, 80 and 100 columns")

    def lane22(self):
        home = self.fixture_home("check-small", dirs=["scratch"])
        wanted = {
            "home": (
                ["?"],
                ["Move the cursor", "Open the entry", "Jump to an entry", "Show or close this help", "Quit now", "Wheel moves, click selects or opens", "1 Clean", "9 History", "0 Check a path", "? or Esc closes"],
            ),
            "about": (["3", "Enter", "?"], ["Back to Home", "Open Check a path or History now", "Quit now", "? or Esc closes"]),
            "history": (["9", "Enter", "?"], ["Scroll", "Back to Home", "Quit now", "? or Esc closes"]),
            "check": (["0", "Enter", "F1"], ["Check the path", "Back to Home", "Erase a letter", "Clear the field", "Scroll the results", "Quit now", "Drag a file in, or paste a path, to check it at once", "? or Esc closes"]),
        }
        shots, missing = {}, []
        for name, (keys, needles) in wanted.items():
            self.start_in(home, 60, 16)
            for key in keys:
                drive("keys", SESSION, key)
                time.sleep(0.25)
            shots[name] = drive("screen", SESSION)
            flat = " ".join(shots[name].replace("│", " ").split())
            missing += [f"{name}: {n}" for n in needles if n not in flat]
            drive("stop", SESSION)
        self.start_in(home, 60, 16)
        drive("keys", SESSION, "0", "Enter")
        drive("wait", SESSION, "Type a path", "3")
        shots["check empty"] = drive("screen", SESSION)
        flat = " ".join(shots["check empty"].split())
        if "Drag a file in, or paste a path, to check it at once." not in flat:
            missing.append("check empty: hint")
        drive("stop", SESSION)
        self.save("small-60x16.txt", "".join(f"=== {name} ===\n{shot}\n" for name, shot in shots.items()))
        self.report(22, True if not missing else f"cut off at 60x16: {missing}", "Every help overlay and the Check empty hint fit whole at 60x16")

    def lane23(self):
        home = self.fixture_home("check-backslash", dirs=["back"])
        typed = f"{home}/back\\slash"
        self.open_check(home, 120, 40)
        by_typing = self.type_and_submit(typed)
        drive("paste", SESSION, typed)
        time.sleep(0.4)
        by_pasting = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("check-backslash.txt", f"=== typed ===\n{by_typing}\n=== pasted ===\n{by_pasting}")
        ok = same_path(result_paths(by_typing)[0], typed) and "\\" in result_paths(by_typing)[0] and same_path(result_paths(by_pasting)[0], f"{home}/backslash") and len(result_paths(by_pasting)) == 2
        self.report(23, True if ok else "typed backslash dropped or pasted one kept; see check-backslash.txt", "Check: a typed backslash is part of the path, a pasted one is an escape")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out")
    parser.add_argument("--bin", default=os.path.join(HERE, "../../../../target/release/dustpan"))
    parser.add_argument("--only", default="0,1,2,3,4,5,6,7,8,10,11,12,13,14,15,16,17,18,19,20,21,22,23")
    args = parser.parse_args()
    os.makedirs(args.out, exist_ok=True)
    lanes = Lanes(args.out, os.path.abspath(args.bin))
    try:
        for lane in args.only.split(","):
            getattr(lanes, f"lane{lane}")()
    finally:
        drive("stop", SESSION, check=False)
    sys.exit(1 if lanes.failed else 0)


if __name__ == "__main__":
    main()
