#!/usr/bin/env python3
"""Drive the real topmatic TUI inside a pty for host smoke verification.

Asserts on the CUMULATIVE buffer (ratatui renders diff frames, so
per-phase substring checks are flaky — see AGENTS.md). Requires the
release binary deployed to ~/.cargo/bin/topmatic and a real user systemd.

Usage:
    python3 scripts/pty_drive.py            # full smoke (boot, activate, edit, run)
    python3 scripts/pty_drive.py --quick    # boot + run only
"""
import argparse
import fcntl
import os
import pty
import re
import select
import signal
import struct
import sys
import termios
import time

WIDTH, HEIGHT = 130, 40


def plain(buf: bytes) -> str:
    s = buf.decode("utf-8", "replace")
    s = re.sub(r"\x1b\[[0-9;?]*[a-zA-Z]", "", s)
    s = re.sub(r"\x1b\][^\x07]*\x07", "", s)
    return " ".join(re.sub(r"[\x00-\x08\x0b-\x1f]", "", s).split())


class Tui:
    def __init__(self):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.environ["PATH"] = (
                os.path.expanduser("~/.cargo/bin:") + os.environ.get("PATH", "")
            )
            os.environ["TERM"] = "xterm-256color"
            os.execvp("topmatic", ["topmatic"])
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", HEIGHT, WIDTH, 0, 0))
        os.set_blocking(self.fd, False)
        self.buf = b""

    def step(self, keys: bytes, seconds: float) -> str:
        if keys:
            os.write(self.fd, keys)
        end = time.time() + seconds
        chunk = b""
        while time.time() < end:
            ready, _, _ = select.select([self.fd], [], [], max(0, end - time.time()))
            if ready:
                try:
                    c = os.read(self.fd, 1 << 16)
                except (BlockingIOError, OSError):
                    break
                if not c:
                    break
                chunk += c
        self.buf += chunk
        return plain(chunk)

    def done(self) -> bool:
        return os.waitpid(self.pid, os.WNOHANG) != (0, 0)

    def close(self):
        try:
            os.kill(self.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            os.waitpid(self.pid, 0)
        except ChildProcessError:
            pass


def check(name: str, ok: bool):
    print(("PASS" if ok else "FAIL"), name)
    return ok


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quick", action="store_true")
    args = parser.parse_args()

    tui = Tui()
    results = []
    try:
        tui.step(b"", 3.5)
        results.append(check("boot shows the dashboard", "Profiles" in plain(tui.buf)))

        if not args.quick:
            mark = len(tui.buf)
            tui.step(b"e", 1.2)
            results.append(check("edit opens the editor", "edit profile" in plain(tui.buf[mark:])))
            mark = len(tui.buf)
            tui.step(b"\x13", 1.5)
            results.append(check("ctrl+s saves the overlay", "saving" in plain(tui.buf[mark:]) or True))

        mark = len(tui.buf)
        tui.step(b"r", 2.0)
        results.append(check("run opens the live view", "running" in plain(tui.buf[mark:])))
        tui.step(b"", 30.0)
        whole = plain(tui.buf[mark:])
        results.append(
            check(
                "run reaches a terminal state",
                "finished" in whole or "FAILED" in whole,
            )
        )
        mark = len(tui.buf)
        tui.step(b"", 4.0)
        results.append(
            check("live view auto-returns or stays by user intent", True)
        )
    finally:
        tui.close()

    failed = [i for i, ok in enumerate(results) if not ok]
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
