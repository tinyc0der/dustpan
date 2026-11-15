#!/usr/bin/env python3
"""Time from spawn to the first full frame, read from the raw bytes a program writes to its pty.

Usage: first-frame.py [--size COLSxROWS] [--runs N] --marker TEXT --quit KEY -- CMD...

The first frame counts as full when MARKER (text from the last band of the screen) has arrived.
Prints one millisecond value per run and the median. The program gets KEY to exit, then SIGKILL after 2 s.
"""
import argparse
import fcntl
import os
import pty
import select
import signal
import statistics
import struct
import termios
import time


def reap(pid, fd):
    kill_at = time.time() + 2
    killed = False
    while True:
        done, _ = os.waitpid(pid, os.WNOHANG)
        if done:
            return
        if not killed and time.time() > kill_at:
            os.kill(pid, signal.SIGKILL)
            killed = True
        ready, _, _ = select.select([fd], [], [], 0.02)
        if ready:
            try:
                os.read(fd, 65536)
            except OSError:
                pass


def run_once(cmd, cols, rows, marker, quit_key):
    start = time.perf_counter()
    pid, fd = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        os.environ["TERM"] = "xterm-256color"
        os.execvp(cmd[0], cmd)
    seen = b""
    elapsed = None
    deadline = start + 10
    while time.perf_counter() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.05)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        seen += chunk
        if marker.encode() in seen:
            elapsed = (time.perf_counter() - start) * 1000
            break
    try:
        os.write(fd, quit_key.encode())
    except OSError:
        pass
    reap(pid, fd)
    os.close(fd)
    return elapsed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--size", default="120x40")
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--marker", required=True)
    parser.add_argument("--quit", default="q", dest="quit_key")
    parser.add_argument("cmd", nargs="+")
    args = parser.parse_args()
    cols, rows = (int(n) for n in args.size.split("x"))
    samples = []
    for _ in range(args.runs):
        ms = run_once(args.cmd, cols, rows, args.marker, args.quit_key)
        if ms is None:
            raise SystemExit("marker never arrived within 10 s")
        samples.append(ms)
        print(f"{ms:.1f}")
    print(f"median {statistics.median(samples):.1f} ms over {len(samples)} runs")


if __name__ == "__main__":
    main()
