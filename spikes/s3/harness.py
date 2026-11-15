"""Drive s3priv in a pseudo-terminal through the unprivileged path.

Usage: python3 harness.py S3PRIV_BINARY OUTDIR

A stand-in for `sudo -v` reads a password from /dev/tty with echo off, like sudo
does. Unlike sudo, it does not flush type-ahead when it restores the terminal, so
any type-ahead reaches the TUI unless the TUI drains it.
"""

import fcntl
import os
import pty
import select
import struct
import sys
import tempfile
import termios
import time
from pathlib import Path

TYPE_AHEAD = "xyz"

AUTH_STANDIN = (
    "i=0; while [ $i -lt 3 ]; do stty -echo < /dev/tty; printf 'Password:' > /dev/tty; "
    "read -r pw < /dev/tty; stty echo < /dev/tty; printf '\\n' > /dev/tty; "
    "[ \"$pw\" = hunter2-right ] && exit 0; echo 'Sorry, try again.' > /dev/tty; i=$((i+1)); done; exit 1"
)


def drive(binary, args, script, timeout=20.0):
    pid, fd = pty.fork()
    if pid == 0:
        os.execv(binary, [binary] + args)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    transcript = b""
    steps = list(script)
    deadline = time.time() + timeout
    while time.time() < deadline:
        if steps and steps[0][0].encode() in transcript:
            marker, keys = steps.pop(0)
            transcript = transcript.replace(marker.encode(), marker.lower().encode(), 1)
            time.sleep(0.1)
            os.write(fd, keys.encode())
        ready, _, _ = select.select([fd], [], [], 0.05)
        if ready:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            transcript += chunk
    _, status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(status), transcript, steps


def case(binary, out, name, extra_args, script, target_in_fixture=True):
    fixture = Path(tempfile.mkdtemp(prefix="s3-")) / "dustpan-s3"
    fixture.mkdir()
    target = fixture / "user-owned.txt" if target_in_fixture else fixture.parent / "outside.txt"
    target.write_text("fixture\n")
    report = out / f"{name}.report.tsv"
    args = ["--auto", "unpriv", "--target", str(target), "--report", str(report)] + extra_args
    code, transcript, unsent = drive(binary, args, script)
    lines = [
        f"case\t{name}",
        f"exit\t{code}",
        f"unsent_steps\t{len(unsent)}",
        f"alt_screen_enters\t{transcript.count(b'\x1b[?1049h')}",
        f"alt_screen_leaves\t{transcript.count(b'\x1b[?1049l')}",
        f"password_echoed\t{b'hunter2' in transcript}",
        f"stray_echoed\t{TYPE_AHEAD.encode() in transcript}",
        f"target_exists_now\t{target.exists()}",
    ]
    (out / f"{name}.harness.tsv").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    print(report.read_text() if report.exists() else "report\tmissing")
    print()


def main():
    binary, out = sys.argv[1], Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    typed = [("Password:", "hunter2-wrong\n"), ("Sorry, try again.", ""), ("Password:", "hunter2-right\n" + TYPE_AHEAD)]
    case(binary, out, "drain", ["--auth-cmd", AUTH_STANDIN], typed)
    case(binary, out, "no-drain-control", ["--auth-cmd", AUTH_STANDIN, "--no-drain"], typed)
    case(binary, out, "refuse-outside-fixture", ["--auth-cmd", AUTH_STANDIN], typed, target_in_fixture=False)


main()
