#!/usr/bin/env python3
"""SIGTERM while the main thread is redrawing. For RUNS runs: spawn BIN in a pty, wait for the footer, spam j/k for a random
0-150 ms, send SIGNAL, drain until exit. Classify the bytes written after the first ESC[?1049l (leave alt screen):
any ESC[?25l (hide cursor) after the last ESC[?25h, or any cell output after leaving the alternate screen."""
import fcntl, os, pty, random, re, select, signal, struct, sys, termios, time
runs = int(sys.argv[1]); signame = sys.argv[2]; cmd = sys.argv[3:]
hidden = garbage = stuck = raw_left = 0
examples = []
for i in range(runs):
    pid, fd = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
        os.environ["TERM"] = "xterm-256color"
        os.execvp(cmd[0], cmd)
    buf = b""
    def drain(t=0.0):
        global buf
        while True:
            r, _, _ = select.select([fd], [], [], t)
            if not r:
                return True
            try:
                c = os.read(fd, 65536)
            except OSError:
                return False
            if not c:
                return False
            buf += c
    while b"Quit" not in buf:
        drain(0.05)
    end = time.time() + random.uniform(0, 0.15)
    while time.time() < end:
        try:
            os.write(fd, b"jkjkjk")
        except OSError:
            break
        drain(0)
    os.kill(pid, getattr(signal, signame))
    status = None
    t_end = time.time() + 3
    while time.time() < t_end:
        drain(0.01)
        done, st = os.waitpid(pid, os.WNOHANG)
        if done:
            status = st
            break
    drain(0.1)
    try:
        lflag = termios.tcgetattr(fd)[3]
        if not (lflag & termios.ICANON):
            raw_left += 1
    except termios.error:
        pass
    os.close(fd)
    if status is None:
        stuck += 1
        os.kill(pid, 9); os.waitpid(pid, 0)
        continue
    idx = buf.find(b"\x1b[?1049l")
    if idx < 0:
        garbage += 1; examples.append((i, "no leave", buf[-200:])); continue
    after = buf[idx + len(b"\x1b[?1049l"):]
    last_show = after.rfind(b"\x1b[?25h")
    if b"\x1b[?25l" in after[last_show:] or last_show < 0:
        hidden += 1; examples.append((i, "cursor hidden at exit", after[-160:]))
    cleaned = re.sub(rb"\x1b\[\?[0-9]+[hl]", b"", after)
    if cleaned.strip():
        garbage += 1; examples.append((i, "output after leaving alt screen", after[:200]))
print(f"runs={runs} cursor_left_hidden={hidden} output_after_leave={garbage} not_exited={stuck} termios_raw_after_exit={raw_left}")
for e in examples[:4]:
    print(e)
sys.exit(1 if hidden or garbage or stuck or raw_left else 0)
