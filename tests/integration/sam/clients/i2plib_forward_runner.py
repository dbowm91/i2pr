#!/usr/bin/env python3
"""Plan 150 §11 — i2plib.sam-based STREAM FORWARD registerer.

Drives i2pr's SAM 3.1 listener with a STREAM FORWARD register and
prints the registerer's public destination on stdout line 1 so the
orchestrator can route a connector's STREAM CONNECT to it. This
runner is the i2plib.sam substitute for the libsam3 STREAM FORWARD
runner.

Usage:
    i2plib_forward_runner.py <sam_host> <sam_port> <forward_host> <forward_port>
"""

from __future__ import annotations

import os
import socket
import sys
import time
from pathlib import Path

_DEFAULT_I2PLIB_ROOT = (
    Path(__file__).resolve().parents[4]
    / "target/interop/cache/sam/i2plib/6edf51cd5d21cc745aa7e23cb98c582144884fa8"
)
_I2PLIB_ROOT = os.environ.get("I2PLIB_ROOT", str(_DEFAULT_I2PLIB_ROOT))
if os.path.isdir(_I2PLIB_ROOT) and _I2PLIB_ROOT not in sys.path:
    sys.path.insert(0, _I2PLIB_ROOT)

import i2plib.sam as i2psam  # noqa: E402


def _recv_line(sock: socket.socket, deadline_s: float) -> str:
    buf = bytearray()
    deadline = time.monotonic() + deadline_s
    while time.monotonic() < deadline:
        sock.settimeout(max(0.05, deadline - time.monotonic()))
        try:
            chunk = sock.recv(4096)
        except socket.timeout:
            continue
        if not chunk:
            break
        buf.extend(chunk)
        if buf.endswith(b"\n"):
            break
    return buf.decode("utf-8", errors="replace").rstrip("\r\n")


def main(argv: list[str]) -> int:
    if len(argv) != 5:
        raise SystemExit(
            "usage: i2plib_forward_runner.py <host> <port> <fwd_host> <fwd_port>"
        )
    host, port_str, fwd_host, fwd_port = argv[1:]
    port = int(port_str)
    session_id = "i2plib-forward-runner"
    s = socket.create_connection((host, port), timeout=10.0)
    s.settimeout(15.0)
    s.sendall(i2psam.hello("3.1", "3.1"))
    line = _recv_line(s, 5.0)
    if "REPLY RESULT=OK" not in line:
        sys.stderr.write(f"hello failed: {line!r}\n")
        return 2
    s.sendall(
        i2psam.session_create("STREAM", session_id, i2psam.TRANSIENT_DESTINATION)
    )
    line = _recv_line(s, 5.0)
    msg = i2psam.Message(line)
    if not msg.ok:
        sys.stderr.write("session create failed\n")
        return 3
    raw_dest = msg["DESTINATION"]
    pub = raw_dest[:522] + "=="
    # Publish pub on stdout line 1.
    sys.stdout.write(pub + "\n")
    sys.stdout.flush()
    # Now STREAM FORWARD.
    fwd_cmd = f"STREAM FORWARD ID={session_id} PORT={fwd_port} HOST={fwd_host}\n"
    s.sendall(fwd_cmd.encode())
    line = _recv_line(s, 5.0)
    msg = i2psam.Message(line)
    if not msg.ok:
        sys.stderr.write(f"STREAM FORWARD failed: {line!r}\n")
        return 4
    # Block until the orchestrator terminates us (keep socket open).
    try:
        while True:
            data = s.recv(4096)
            if not data:
                break
    except (socket.timeout, ConnectionResetError, OSError):
        pass
    s.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
