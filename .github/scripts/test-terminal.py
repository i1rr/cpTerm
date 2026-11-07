"""Open the real binary in a disposable terminal and verify clean keyboard exit."""

import fcntl
import os
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time

master, slave = pty.openpty()
try:
    before = termios.tcgetattr(slave)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    with tempfile.TemporaryDirectory() as work:
        env = dict(os.environ, HOME=work, XDG_CONFIG_HOME=work, TERM="xterm-256color",
                   TOKIO_WORKER_THREADS="1")
        with subprocess.Popen(["target/debug/cpt", work, work], stdin=slave,
                              stdout=slave, stderr=slave, env=env) as proc:
            try:
                output = bytearray()
                deadline = time.monotonic() + 10
                while b"\x1b[?1049h" not in output:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([master], [], [], remaining)[0]:
                        raise RuntimeError("cpTerm did not enter the terminal screen")
                    output.extend(os.read(master, 65536))
                os.write(master, b"\x11")  # Ctrl+Q
                if proc.wait(timeout=10) != 0:
                    raise RuntimeError("cpTerm did not exit cleanly")
            finally:
                if proc.poll() is None:
                    proc.kill()
    if termios.tcgetattr(slave) != before:
        raise RuntimeError("cpTerm left the terminal in raw mode")
finally:
    os.close(master)
    os.close(slave)
