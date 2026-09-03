#!/usr/bin/env python3
"""Plan 150 §6 — i2plib.sam-based runner (libsam3 substitute).

libsam3 (snapshot 7d6e658798baec31394c5685f9583343cc00900b) is
out-of-spec for SAM 3.1: its `sam3CreateSession` rejects any
SESSION STATUS reply whose DESTINATION value is shorter than
`SAM3_PRIVKEY_MIN_SIZE` (884 chars). The canonical Java I2P / i2pd
Ed25519 private destination is exactly 608 chars, so libsam3 cannot
interop with any i2pr-emitted SAM bridge. Plan 150 §6 explicitly
permits substituting a third independent client when a client's
public API structurally cannot express a direction.

This runner satisfies the substitute constraint using the independent
i2plib.sam message/Base64 surface from
`https://github.com/l-n-s/i2plib` snapshot
`6edf51cd5d21cc745aa7e23cb98c582144884fa8`. It is a thin harness that
owns the socket lifecycle and delegates message construction and
Base64 parsing to i2plib.sam. The evidence label is exactly
`i2plib-sam-substitute` per Plan 150 §2.3.

The runner publishes its public destination on stdout line 1 (after
SESSION CREATE, before STREAM CONNECT/ACCEPT) so the orchestrator can
coordinate a cross-client exchange. The published pub has the canonical
two trailing `=` padding chars expected by i2pr's
`decode_destination_triple`.

Usage:
    i2plib_runner connect <sam_host> <sam_port> <peer_pub>
                        <send_payload_file> <expect_payload_file>
                        <silent:true|false>
    i2plib_runner accept  <sam_host> <sam_port>
                        <send_payload_file> <expect_payload_file>
                        <silent:true|false>
"""

from __future__ import annotations

import argparse
import os
import socket
import sys
import time
from pathlib import Path

# Add the i2plib checkout to the Python path so the harness picks up
# the exact pinned snapshot rather than any system-installed version.
_DEFAULT_I2PLIB_ROOT = (
    Path(__file__).resolve().parents[4]
    / "target/interop/cache/sam/i2plib/6edf51cd5d21cc745aa7e23cb98c582144884fa8"
)
_I2PLIB_ROOT = os.environ.get("I2PLIB_ROOT", str(_DEFAULT_I2PLIB_ROOT))
if os.path.isdir(_I2PLIB_ROOT) and _I2PLIB_ROOT not in sys.path:
    sys.path.insert(0, _I2PLIB_ROOT)

import i2plib.sam as i2psam  # noqa: E402

SAM_PUB_CHARS = 524  # 391 raw bytes -> 524 Base64 chars with `==` padding.


def _connect(host: str, port: int) -> socket.socket:
    s = socket.create_connection((host, port), timeout=10.0)
    s.settimeout(15.0)
    return s


def _round_trip(sock: socket.socket, payload: bytes, deadline_s: float = 10.0) -> None:
    sock.sendall(i2psam.hello("3.1", "3.1"))
    line = _recv_line(sock, deadline_s)
    if "REPLY RESULT=OK" not in line:
        raise SystemExit(f"hello failed: {line!r}")


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


def _recv_raw_n(sock: socket.socket, n: int, deadline_s: float) -> bytes:
    out = bytearray()
    deadline = time.monotonic() + deadline_s
    while len(out) < n:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(f"short read got={len(out)} want={n}")
        sock.settimeout(max(0.05, remaining))
        try:
            chunk = sock.recv(4096)
        except socket.timeout:
            continue
        if not chunk:
            raise EOFError(f"socket closed got={len(out)} want={n}")
        out.extend(chunk)
    return bytes(out)


def _open_session(host: str, port: int, session_id: str) -> tuple[socket.socket, str]:
    """Open a STREAM session, return (socket, public_destination_524_chars).

    The published pub is the canonical I2P Base64 public portion of the
    i2pr-issued SESSION STATUS DESTINATION reply, with two trailing
    `=` padding chars appended.
    """
    s = _connect(host, port)
    s.sendall(i2psam.hello("3.1", "3.1"))
    line = _recv_line(s, 5.0)
    if "REPLY RESULT=OK" not in line:
        s.close()
        raise SystemExit(f"hello failed: {line!r}")
    s.sendall(
        i2psam.session_create("STREAM", session_id, i2psam.TRANSIENT_DESTINATION)
    )
    line = _recv_line(s, 5.0)
    msg = i2psam.Message(line)
    if not msg.ok:
        s.close()
        raise SystemExit("session create failed")
    raw_dest = msg["DESTINATION"]
    # The reply's DESTINATION is the full private destination (608 chars
    # with one trailing `=` for Ed25519). Slice the first 522 chars and
    # append `==` to produce the canonical 524-char public destination.
    pub = raw_dest[:SAM_PUB_CHARS - 2] + "=="
    return s, pub


def run_connect(
    host: str,
    port: int,
    peer_pub: str,
    send_path: Path,
    expect_path: Path,
    silent: bool,
) -> int:
    send_buf = send_path.read_bytes()
    expect_buf = expect_path.read_bytes()
    session_id = f"i2plib-runner-connect-{os.getpid()}-{int(time.time() * 1_000_000)}"
    s, my_pub = _open_session(host, port, session_id)
    # Plan 150 §6: emit our pub on stdout line 1.
    sys.stdout.write(my_pub + "\n")
    sys.stdout.flush()
    silent_str = "true" if silent else "false"
    s.sendall(i2psam.stream_connect(session_id, peer_pub, silent_str))
    line = _recv_line(s, 10.0)
    msg = i2psam.Message(line)
    if not msg.ok:
        sys.stderr.write(
            f"i2plib_runner: STREAM CONNECT failed: {line!r}\n"
        )
        s.close()
        return 4
    if silent:
        # No STATUS line; everything in the stream is raw payload.
        pass
    # Send payload and read echo. The peer should send exactly `expect_buf`.
    s.sendall(send_buf)
    try:
        got = _recv_raw_n(s, len(expect_buf), 10.0)
    except (TimeoutError, EOFError) as exc:
        sys.stderr.write(f"i2plib_runner: short read {exc}\n")
        s.close()
        return 7
    s.close()
    if got != expect_buf:
        sys.stderr.write(
            f"i2plib_runner: payload mismatch got_len={len(got)} want_len={len(expect_buf)}\n"
        )
        return 8
    return 0


def run_accept(
    host: str,
    port: int,
    send_path: Path,
    expect_path: Path,
    silent: bool,
) -> int:
    send_buf = send_path.read_bytes()
    expect_buf = expect_path.read_bytes()
    session_id = f"i2plib-runner-accept-{os.getpid()}-{int(time.time() * 1_000_000)}"
    s, my_pub = _open_session(host, port, session_id)
    sys.stdout.write(my_pub + "\n")
    sys.stdout.flush()
    silent_str = "true" if silent else "false"
    s.sendall(i2psam.stream_accept(session_id, silent_str))
    line = _recv_line(s, 25.0)
    msg = i2psam.Message(line)
    if not msg.ok:
        sys.stderr.write(
            f"i2plib_runner: STREAM ACCEPT failed: {line!r}\n"
        )
        s.close()
        return 4
    # Per SAM 3.1 non-silent ACCEPT, the next line is the peer's pub.
    dest_line = _recv_line(s, 5.0)
    if not dest_line.startswith("DESTINATION="):
        sys.stderr.write(
            f"i2plib_runner: expected DESTINATION= line, got {dest_line!r}\n"
        )
        s.close()
        return 10
    got = _recv_raw_n(s, len(expect_buf), 10.0)
    if got != expect_buf:
        sys.stderr.write(
            f"i2plib_runner: payload mismatch got_len={len(got)} want_len={len(expect_buf)}\n"
        )
        s.close()
        return 8
    s.sendall(send_buf)
    s.close()
    return 0


def main(argv: list[str]) -> int:
    if len(argv) < 2 or argv[0] not in ("connect", "accept"):
        raise SystemExit(
            "usage: i2plib_runner connect <host> <port> <peer_pub> <send_file> <expect_file> <silent>\n"
            "       i2plib_runner accept  <host> <port> <send_file> <expect_file> <silent>"
        )
    role = argv[0]
    if role == "connect":
        if len(argv) != 7:
            raise SystemExit(
                "usage: i2plib_runner connect <host> <port> <peer_pub> <send_file> <expect_file> <silent>"
            )
        host, port_str, peer_pub, send_file, expect_file, silent = argv[1:]
        return run_connect(
            host,
            int(port_str),
            peer_pub,
            Path(send_file),
            Path(expect_file),
            _parse_silent(silent),
        )
    if len(argv) != 6:
        raise SystemExit(
            "usage: i2plib_runner accept <host> <port> <send_file> <expect_file> <silent>"
        )
    host, port_str, send_file, expect_file, silent = argv[1:]
    return run_accept(
        host,
        int(port_str),
        Path(send_file),
        Path(expect_file),
        _parse_silent(silent),
    )


def _parse_silent(s: str) -> bool:
    return s.lower() in ("true", "1", "yes")


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
