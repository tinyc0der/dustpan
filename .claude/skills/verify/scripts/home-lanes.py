#!/usr/bin/env python3
"""Run the live text lanes for the Home screen against a release binary and save one capture per lane.

Usage: home-lanes.py OUTDIR [--bin PATH] [--only 1,2,...]

Lanes follow docs/M0-checklist.md "Draw the TUI frame, Home and help overlay (M0-6)". Each lane prints PASS or FAIL
with the reason and writes OUTDIR/<slug>.txt. The exit status is 1 when any lane fails.
"""
import argparse
import os
import re
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


ENTRIES = ["Clean", "Uninstall", "Analyze", "Optimize", "Status", "Purge", "Installers", "Slim", "History"]


def cells(text):
    return sum(2 if unicodedata.east_asian_width(c) in "WF" else 0 if unicodedata.combining(c) else 1 for c in text)


HINTS = ["↑↓ Move", "Enter Open", "1-9 Jump", "Tab Detail", ", Settings", "? Help", "q Quit", "Ctrl+C Quit now"]
ENTRY_ROW = r"\d  {} +Coming in M\d"


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
        mole = subprocess.run(["which", "mo"], capture_output=True, text=True).stdout.strip()
        drive("start", SESSION, "120", "40", "--", mole)
        drive("wait", SESSION, "Q Quit", "8")
        mole_screen = drive("screen", SESSION)
        drive("stop", SESSION)
        start(self.binary, 120, 40)
        screen = drive("screen", SESSION)
        drive("stop", SESSION)
        self.save("home-vs-mole.txt", f"=== mo @ 120x40 ===\n{mole_screen}\n=== dustpan @ 120x40 ===\n{screen}")
        missing = [e for e in ("Clean", "Uninstall", "Optimize", "Analyze", "Status") if e not in screen]
        header = screen.split("\n")[0]
        if missing:
            return self.report(1, f"missing Mole entries {missing}", "")
        if not re.search(r"\d+(\.\d)? (GB|TB|MB) free", header):
            return self.report(1, f"header lacks free space: {header!r}", "")
        self.report(1, True, f"every Mole entry present; header {header.strip()!r}")

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
        needles = ["↑ ↓ j k", "Enter → l", "1..9", "Settings", "Show or close this help", "Quit now", "Wheel, click", "9 History"]
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
            and "Coming in M3" in screen
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
            and "Explore disk usage" in detail
            and "Uninstall" not in detail
            and "Tab" in help_screen
            and "Uninstall" in back
            and wrapped(detail, ["dustpan › Home", "Status  Coming in M1"]) is None
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
        ok = "needs 60 by 16" in small and "Resize to continue" in small and "Clean" in back and "Coming in M3" in back
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
            f"=== NO_COLOR=1 (escape codes seen: {sorted(set(colors))}) ===\n{plain_text}\n{toast.splitlines()[-3]}\n"
            f"=== --ascii home ===\n{ascii_home}\n=== --ascii help ===\n{ascii_help}",
        )
        non_ascii = sorted({c for c in ascii_help + ascii_home if ord(c) > 127})
        words = "➤" in plain_text and "Coming in M" in toast.splitlines()[-3]
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out")
    parser.add_argument("--bin", default=os.path.join(HERE, "../../../../target/release/dustpan"))
    parser.add_argument("--only", default="0,1,2,3,4,5,6,7,8,10,11,12,13")
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
