#!/usr/bin/env python3
"""Close the pty master under a session leader that ignores SIGHUP, so the kernel's SIGHUP never reaches dustpan,
then check the dustpan child exits within 3 seconds instead of spinning. Usage: hangup-without-sighup.py BINARY"""
import fcntl, os, pty, select, struct, subprocess, sys, termios, time

binary = sys.argv[1]
pid, fd = pty.fork()
if pid == 0:
    fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    os.execvp("bash", ["bash", "-c", f"trap '' HUP; {binary}; true"])
buf = b""
deadline = time.time() + 5
while b"Quit" not in buf and time.time() < deadline:
    ready, _, _ = select.select([fd], [], [], 0.1)
    if ready:
        buf += os.read(fd, 65536)
children = subprocess.run(["pgrep", "-P", str(pid)], capture_output=True, text=True).stdout.split()
if not children:
    print("FAIL no dustpan child under the shell")
    sys.exit(1)
child = children[0]
os.close(fd)
alive = True
for _ in range(30):
    time.sleep(0.1)
    alive = bool(subprocess.run(["ps", "-o", "pid=", "-p", child], capture_output=True, text=True).stdout.strip())
    if not alive:
        break
cpu = subprocess.run(["ps", "-o", "%cpu=", "-p", child], capture_output=True, text=True).stdout.strip()
subprocess.run(["kill", "-9", child, str(pid)], capture_output=True)
print(f"dustpan pid {child} {'still running at ' + cpu + '% cpu' if alive else 'gone'}")
sys.exit(1 if alive else 0)
