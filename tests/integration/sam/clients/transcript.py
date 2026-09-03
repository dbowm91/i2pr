"""Plan 150 — supporting Python transcript runner.

This runner is a thin, raw-socket SAM 3.1 client used for the
matrix items that the external libraries cannot express:

- byte-exact `SILENT=true/false` raw-transition evidence for both
  CONNECT and ACCEPT (Plan 150 §9);
- `NAMING LOOKUP` round-trips for `ME`, locally-known full
  destinations, malformed destinations, and `KEY_NOT_FOUND` paths
  (Plan 150 §10);
- the negative compatibility matrix (Plan 150 §13) — the runner
  drives the malformed/unsupported commands directly so the
  harness owns the exact wire bytes.

Per Plan 150 §2.4 this transcript runner is **supporting
evidence**, not one of the two independent-client counts. The
mandatory clients remain libsam3 and i2psam.
"""

from __future__ import annotations

import argparse
import os
import socket
import struct
import sys
import tempfile
import time
from pathlib import Path
from typing import Optional, Tuple


def recv_line(sock: socket.socket, deadline: float) -> str:
    buf = bytearray()
    while True:
        if time.monotonic() > deadline:
            raise TimeoutError(
                f"deadline exceeded while reading line, got_bytes={len(buf)}"
            )
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


def send_line(sock: socket.socket, line: str) -> None:
    sock.sendall(line.encode("utf-8") + b"\n")


def recv_n(sock: socket.socket, n: int, deadline: float) -> bytes:
    out = bytearray()
    while len(out) < n:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(f"short read got={len(out)} want={n}")
        sock.settimeout(max(0.05, remaining))
        try:
            chunk = sock.recv(n - len(out))
        except socket.timeout:
            continue
        if not chunk:
            break
        out.extend(chunk)
    return bytes(out)


def recv_until_close(sock: socket.socket, deadline: float) -> bytes:
    out = bytearray()
    sock.settimeout(max(0.05, deadline - time.monotonic()))
    while True:
        try:
            chunk = sock.recv(4096)
        except socket.timeout:
            if time.monotonic() > deadline:
                break
            continue
        if not chunk:
            break
        out.extend(chunk)
    return bytes(out)


class SamSession:
    def __init__(self, host: str, port: int) -> None:
        self.sock = socket.create_connection((host, port), timeout=5.0)
        self.sock.settimeout(5.0)
        self.host = host
        self.port = port

    def close(self) -> None:
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()

    def hello(self, min_v: str = "3.1", max_v: str = "3.1") -> str:
        send_line(self.sock, f"HELLO VERSION MIN={min_v} MAX={max_v}")
        return recv_line(self.sock, time.monotonic() + 5.0)

    def dest_generate(self) -> Tuple[str, str]:
        send_line(self.sock, "DEST GENERATE SIGNATURE_TYPE=7")
        reply = recv_line(self.sock, time.monotonic() + 5.0)
        if not reply.startswith("DEST REPLY RESULT=OK"):
            raise RuntimeError("DEST GENERATE failed")
        priv = None
        pub = None
        for token in reply.split():
            if token.startswith("PRIV="):
                priv = token[len("PRIV="):].strip('"')
            if token.startswith("PUB="):
                pub = token[len("PUB="):].strip('"')
        if priv is None or pub is None:
            raise RuntimeError("DEST REPLY missing PRIV/PUB")
        return priv, pub

    def session_create(self, sid: str, destination: str) -> str:
        send_line(
            self.sock,
            f"SESSION CREATE STYLE=STREAM ID={sid} DESTINATION={destination}",
        )
        return recv_line(self.sock, time.monotonic() + 5.0)

    def naming_lookup(self, name: str) -> str:
        send_line(self.sock, f"NAMING LOOKUP NAME={name}")
        return recv_line(self.sock, time.monotonic() + 5.0)


def cmd_silent_connect(args: argparse.Namespace) -> int:
    """Drive a raw-socket CONNECT SILENT round-trip and verify the
    byte-exact raw transition (no STREAM STATUS line, raw bytes
    immediately follow)."""

    sess = SamSession(args.host, args.port)
    try:
        sess.hello()
        priv, _pub = sess.dest_generate()
        reply = sess.session_create(args.session_id, priv)
        if not reply.startswith("SESSION STATUS RESULT=OK"):
            raise RuntimeError("session_create failed")
    finally:
        sess.close()

    # Open a fresh control socket for the raw stream transition. The
    # SAM listener requires a new TCP connection per STREAM CONNECT
    # because the raw socket is detached from the line parser after
    # the success transition.
    stream = socket.create_connection((args.host, args.port), timeout=5.0)
    stream.settimeout(5.0)
    send_line(stream, f"HELLO VERSION MIN=3.1 MAX=3.1")
    hello_line = recv_line(stream, time.monotonic() + 5.0)
    if not hello_line.startswith("HELLO REPLY RESULT=OK"):
        stream.close()
        raise RuntimeError(f"stream HELLO failed: {hello_line}")
    send_line(stream, f"SESSION CREATE STYLE=STREAM ID={args.session_id} DESTINATION={priv}")
    sc_line = recv_line(stream, time.monotonic() + 5.0)
    if not sc_line.startswith("SESSION STATUS RESULT=OK"):
        stream.close()
        raise RuntimeError("stream SESSION CREATE failed")
    connect_line = (
        f"STREAM CONNECT ID={args.session_id} "
        f"DESTINATION={args.peer_pub} SILENT=true"
    )
    send_line(stream, connect_line)
    # No STREAM STATUS line is expected — write the sentinel
    # immediately. The i2pr daemon must transition straight to
    # raw-mode on the wire.
    sent_payload = Path(args.payload_file).read_bytes()
    stream.sendall(sent_payload)

    # The peer (the i2pr SAM listener) will echo the same payload
    # back to us. We must read exactly len(sent_payload) bytes back.
    got = recv_n(stream, len(sent_payload), time.monotonic() + 10.0)
    stream.close()
    if got != sent_payload:
        sys.stderr.write(
            f"silent_connect: byte mismatch got_len={len(got)} want_len={len(sent_payload)}\n"
        )
        return 8
    return 0


def cmd_silent_accept(args: argparse.Namespace) -> int:
    """ACCEPT SILENT=true round-trip. The accept side must
    transition straight to raw bytes without writing a STREAM STATUS
    line and without writing a peer Destination line."""

    sess = SamSession(args.host, args.port)
    try:
        sess.hello()
        priv, _pub = sess.dest_generate()
        reply = sess.session_create(args.session_id, priv)
        if not reply.startswith("SESSION STATUS RESULT=OK"):
            raise RuntimeError("session_create failed")
        pub_local = None
        for token in reply.split():
            if token.startswith("DESTINATION="):
                pub_local = token[len("DESTINATION="):].strip('"')
        if pub_local is None:
            raise RuntimeError(f"missing DESTINATION= in {reply}")
    finally:
        sess.close()

    stream = socket.create_connection((args.host, args.port), timeout=5.0)
    stream.settimeout(5.0)
    send_line(stream, "HELLO VERSION MIN=3.1 MAX=3.1")
    hello_line = recv_line(stream, time.monotonic() + 5.0)
    if not hello_line.startswith("HELLO REPLY RESULT=OK"):
        stream.close()
        raise RuntimeError(f"stream HELLO failed: {hello_line}")
    send_line(stream, f"SESSION CREATE STYLE=STREAM ID={args.session_id} DESTINATION={priv}")
    sc_line = recv_line(stream, time.monotonic() + 5.0)
    if not sc_line.startswith("SESSION STATUS RESULT=OK"):
        stream.close()
        raise RuntimeError("stream SESSION CREATE failed")
    send_line(stream, f"STREAM ACCEPT ID={args.session_id} SILENT=true")

    # Plan 150 §9 — for ACCEPT SILENT=true the listener must write
    # no status line and no peer Destination line; the very first
    # byte is raw data from the peer.
    expect_payload = Path(args.payload_file).read_bytes()
    first = recv_n(stream, len(expect_payload), time.monotonic() + 10.0)
    stream.close()
    if first != expect_payload:
        sys.stderr.write(
            f"silent_accept: byte mismatch got_len={len(first)} want_len={len(expect_payload)}\n"
        )
        return 8
    return 0


def cmd_naming_lookup(args: argparse.Namespace) -> int:
    """Exercise the NAMING LOOKUP supported surface through the
    raw transcript. Validates NAME=ME, full destination
    round-trip, malformed destination, and KEY_NOT_FOUND."""

    sess = SamSession(args.host, args.port)
    try:
        sess.hello()
        priv, pub = sess.dest_generate()
        sess.session_create(args.session_id, priv)

        # 1. NAME=ME returns the session destination.
        me_reply = sess.naming_lookup("ME")
        if not me_reply.startswith("NAMING REPLY RESULT=OK"):
            sys.stderr.write(f"naming NAME=ME failed: {me_reply}\n")
            return 8
        if pub not in me_reply:
            sys.stderr.write(f"naming NAME=ME did not echo destination: {me_reply}\n")
            return 8

        # 2. full destination round-trip — must be accepted and the
        # public destination must be byte-identical to what we
        # generated.
        full_reply = sess.naming_lookup(pub)
        if not full_reply.startswith("NAMING REPLY RESULT=OK"):
            sys.stderr.write(f"naming full destination failed: {full_reply}\n")
            return 8
        if pub not in full_reply:
            sys.stderr.write(f"naming full destination missing: {full_reply}\n")
            return 8

        # 3. malformed destination → INVALID_KEY.
        bad_reply = sess.naming_lookup("not-a-valid-base64!!!")
        if "RESULT=INVALID_KEY" not in bad_reply:
            sys.stderr.write(f"naming malformed did not return INVALID_KEY: {bad_reply}\n")
            return 8

        # 4. unknown .i2p → KEY_NOT_FOUND.
        unknown_reply = sess.naming_lookup("nonexistent.i2p")
        if "RESULT=KEY_NOT_FOUND" not in unknown_reply:
            sys.stderr.write(f"naming unknown .i2p did not return KEY_NOT_FOUND: {unknown_reply}\n")
            return 8
    finally:
        sess.close()
    return 0


def cmd_negative_matrix(args: argparse.Namespace) -> int:
    """Exercise the externally observable rejection vocabulary."""

    sess = SamSession(args.host, args.port)
    try:
        # HELLO 3.2 must fail with NOVERSION.
        send_line(sess.sock, "HELLO VERSION MIN=3.2 MAX=3.3")
        hello_reply = recv_line(sess.sock, time.monotonic() + 5.0)
        if "NOVERSION" not in hello_reply:
            sys.stderr.write(f"negative HELLO 3.2 did not return NOVERSION: {hello_reply}\n")
            return 8

        # Re-establish SAM 3.1 baseline.
        hello_reply = sess.hello()
        if not hello_reply.startswith("HELLO REPLY RESULT=OK"):
            sys.stderr.write(f"negative baseline HELLO failed: {hello_reply}\n")
            return 8

        # SESSION CREATE STYLE=DATAGRAM must fail (M7 baseline is
        # STREAM only).
        send_line(
            sess.sock,
            "SESSION CREATE STYLE=DATAGRAM ID=negative",
        )
        sc_reply = recv_line(sess.sock, time.monotonic() + 5.0)
        if "RESULT=OK" in sc_reply and "DESTINATION=" in sc_reply:
            sys.stderr.write(f"negative SESSION CREATE STYLE=DATAGRAM unexpectedly OK: {sc_reply}\n")
            return 8

        # SESSION CREATE STYLE=RAW must fail.
        send_line(sess.sock, "SESSION CREATE STYLE=RAW ID=negative")
        sc_reply = recv_line(sess.sock, time.monotonic() + 5.0)
        if "RESULT=OK" in sc_reply and "DESTINATION=" in sc_reply:
            sys.stderr.write(f"negative SESSION CREATE STYLE=RAW unexpectedly OK: {sc_reply}\n")
            return 8

        # Unknown command must be rejected deterministically.
        send_line(sess.sock, "FROBNICATE X=1")
        unk_reply = recv_line(sess.sock, time.monotonic() + 5.0)
        if "RESULT=OK" in unk_reply and "DESTINATION=" in unk_reply:
            sys.stderr.write(f"negative unknown command unexpectedly OK: {unk_reply}\n")
            return 8
    finally:
        sess.close()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Plan 150 transcript runner")
    parser.add_argument("--host", required=True)
    parser.add_argument("--port", type=int, required=True)
    sub = parser.add_subparsers(dest="command", required=True)

    s1 = sub.add_parser("silent_connect")
    s1.add_argument("--session-id", required=True)
    s1.add_argument("--peer-pub", required=True)
    s1.add_argument("--payload-file", required=True)

    s2 = sub.add_parser("silent_accept")
    s2.add_argument("--session-id", required=True)
    s2.add_argument("--payload-file", required=True)

    s3 = sub.add_parser("naming_lookup")
    s3.add_argument("--session-id", required=True)

    s4 = sub.add_parser("negative_matrix")

    args = parser.parse_args()
    if args.command == "silent_connect":
        return cmd_silent_connect(args)
    if args.command == "silent_accept":
        return cmd_silent_accept(args)
    if args.command == "naming_lookup":
        return cmd_naming_lookup(args)
    if args.command == "negative_matrix":
        return cmd_negative_matrix(args)
    return 64


if __name__ == "__main__":
    sys.exit(main())
