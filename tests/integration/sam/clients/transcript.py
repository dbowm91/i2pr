#!/usr/bin/env python3
"""Supporting raw SAM 3.1 transcript checks for Plan 150.

This is not counted as an independent client.  It covers the protocol
surfaces that the two external library runners do not expose conveniently:
destination generation for an import test, NAMING LOOKUP, and the negative
compatibility matrix.  All secret-bearing values remain in temporary files
or process memory and are never printed.
"""

from __future__ import annotations

import argparse
import base64
import os
import socket
import sys
import threading
import time
from pathlib import Path


class BufferedSocket:
    """Preserve bytes read after a line terminator for raw transitions."""

    def __init__(self, sock: socket.socket) -> None:
        self.sock = sock
        self.buffer = bytearray()

    def close(self) -> None:
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()

    def send_line(self, line: str) -> None:
        self.sock.sendall(line.encode("ascii") + b"\n")

    def recv_line(self, timeout_s: float = 5.0) -> str:
        deadline = time.monotonic() + timeout_s
        while True:
            newline = self.buffer.find(b"\n")
            if newline >= 0:
                line = bytes(self.buffer[: newline + 1])
                del self.buffer[: newline + 1]
                return line.decode("utf-8", errors="replace").rstrip("\r\n")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("SAM line deadline exceeded")
            self.sock.settimeout(max(0.05, remaining))
            chunk = self.sock.recv(4096)
            if not chunk:
                raise EOFError("SAM socket closed while reading a line")
            self.buffer.extend(chunk)


def open_session(host: str, port: int) -> BufferedSocket:
    sam = BufferedSocket(socket.create_connection((host, port), timeout=10.0))
    sam.send_line("HELLO VERSION MIN=3.1 MAX=3.1")
    if not sam.recv_line().startswith("HELLO REPLY RESULT=OK"):
        sam.close()
        raise RuntimeError("HELLO VERSION 3.1 rejected")
    return sam


def parse_field(reply: str, name: str) -> str:
    prefix = f"{name}="
    for token in reply.split():
        if token.startswith(prefix):
            return token[len(prefix) :].strip('"')
    raise RuntimeError(f"SAM reply omitted {name}")


def public_from_private(private_destination: str) -> str:
    private_bytes = base64.b64decode(
        private_destination.encode("ascii"), altchars=b"-~", validate=True
    )
    if len(private_bytes) < 391:
        raise RuntimeError("private destination is shorter than its public part")
    return base64.b64encode(private_bytes[:391], altchars=b"-~").decode("ascii")


def write_secret(path: Path, value: str) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC
    fd = os.open(path, flags, 0o600)
    with os.fdopen(fd, "w", encoding="ascii") as output:
        output.write(value)


def cmd_generate(args: argparse.Namespace) -> int:
    sam = open_session(args.host, args.port)
    try:
        sam.send_line("DEST GENERATE SIGNATURE_TYPE=7")
        reply = sam.recv_line()
        if not reply.startswith("DEST REPLY RESULT=OK"):
            return 2
        write_secret(Path(args.private_output), parse_field(reply, "PRIV"))
        Path(args.public_output).write_text(
            parse_field(reply, "PUB"), encoding="ascii"
        )
        return 0
    finally:
        sam.close()


def cmd_naming(args: argparse.Namespace) -> int:
    sam = open_session(args.host, args.port)
    try:
        sam.send_line(
            "SESSION CREATE STYLE=STREAM ID=plan150-naming DESTINATION=TRANSIENT"
        )
        session_reply = sam.recv_line()
        if not session_reply.startswith("SESSION STATUS RESULT=OK"):
            print("naming: session create failed", file=sys.stderr)
            return 2
        private = parse_field(session_reply, "DESTINATION")
        public = public_from_private(private)
        sam.send_line("NAMING LOOKUP NAME=ME")
        me_reply = sam.recv_line()
        if not me_reply.startswith("NAMING REPLY RESULT=OK"):
            print("naming: NAME=ME failed", file=sys.stderr)
            return 3
        named_public = parse_field(me_reply, "VALUE")
        if len(public) < 524 or named_public != public[:522] + "==":
            print(
                f"naming: public mismatch status_len={len(public)} named_len={len(named_public)}",
                file=sys.stderr,
            )
            return 4

        sam.send_line(f"NAMING LOOKUP NAME={named_public}")
        if not sam.recv_line().startswith("NAMING REPLY RESULT=OK"):
            print("naming: full public lookup failed", file=sys.stderr)
            return 5
        sam.send_line("NAMING LOOKUP NAME=not-a-valid-base64!!!")
        if "RESULT=INVALID_KEY" not in sam.recv_line():
            print("naming: malformed public lookup failed", file=sys.stderr)
            return 6
        sam.send_line("NAMING LOOKUP NAME=unknown-plan150-name.i2p")
        if "RESULT=KEY_NOT_FOUND" not in sam.recv_line():
            print("naming: unknown .i2p lookup failed", file=sys.stderr)
            return 7
        return 0
    finally:
        sam.close()


def expect_reply(host: str, port: int, command: str, expected: str) -> bool:
    sam = open_session(host, port)
    try:
        sam.send_line(command)
        return expected in sam.recv_line(5.0)
    finally:
        sam.close()


def cmd_negative(args: argparse.Namespace) -> int:
    checks = [
        (
            "HELLO VERSION MIN=3.2 MAX=3.3",
            "HELLO REPLY RESULT=NOVERSION",
            False,
        ),
        ("SESSION CREATE STYLE=DATAGRAM ID=bad", "NOT_IMPLEMENTED", True),
        ("SESSION CREATE STYLE=RAW ID=bad", "NOT_IMPLEMENTED", True),
        (
            "STREAM CONNECT ID=bad DESTINATION=invalid FROM_PORT=1",
            "NOT_IMPLEMENTED",
            True,
        ),
        (
            "STREAM CONNECT ID=bad DESTINATION=invalid TO_PORT=1",
            "NOT_IMPLEMENTED",
            True,
        ),
        (
            "STREAM FORWARD ID=bad PORT=1 HOST=127.0.0.1 SSL=true",
            "NOT_IMPLEMENTED",
            True,
        ),
        ("NAMING LOOKUP NAME=ME OPTIONS=true", "NOT_IMPLEMENTED", True),
        ("FROBNICATE X=1", "RESULT=I2P_ERROR", True),
        (
            "SESSION CREATE STYLE=STREAM ID=bad DESTINATION=not-base64!!!",
            "RESULT=INVALID_KEY",
            True,
        ),
        (
            "SESSION CREATE STYLE=STREAM ID=bad ID=duplicate DESTINATION=TRANSIENT",
            "RESULT=I2P_ERROR",
            True,
        ),
    ]
    for command, expected, hello_first in checks:
        if hello_first:
            if not expect_reply(args.host, args.port, command, expected):
                return 2
        else:
            sam = BufferedSocket(
                socket.create_connection((args.host, args.port), timeout=10.0)
            )
            try:
                sam.send_line(command)
                if expected not in sam.recv_line(5.0):
                    return 2
            finally:
                sam.close()
    return 0


def raw_transfer(
    sam: BufferedSocket, send_data: bytes, expected_data: bytes
) -> bool:
    send_error: list[BaseException] = []

    def writer() -> None:
        try:
            for offset in range(0, len(send_data), 4096):
                sam.sock.sendall(send_data[offset : offset + 4096])
        except BaseException as error:
            send_error.append(error)

    thread = threading.Thread(target=writer)
    thread.start()
    try:
        received = bytearray()
        deadline = time.monotonic() + 30.0
        while len(received) < len(expected_data):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return False
            sam.sock.settimeout(remaining)
            chunk = sam.sock.recv(len(expected_data) - len(received))
            if not chunk:
                return False
            received.extend(chunk)
    except (OSError, TimeoutError):
        return False
    finally:
        thread.join(timeout=30.0)
    return not thread.is_alive() and not send_error and bytes(received) == expected_data


def cmd_silent(args: argparse.Namespace) -> int:
    """Exercise both SILENT=true raw transitions in one supporting transcript."""

    accept = open_session(args.host, args.port)
    connect = open_session(args.host, args.port)
    try:
        accept_public_value = create_session(accept, "silent-accept")
        create_session(connect, "silent-connect")
        accept.send_line("STREAM ACCEPT ID=silent-accept SILENT=true")
        connect.send_line(
            f"STREAM CONNECT ID=silent-connect DESTINATION={accept_public_value} SILENT=true"
        )
        payload_a = b"\x00SILENT-A\xff\x10" * 256
        payload_b = b"\x80SILENT-B\x00\xfe" * 256
        # The request sockets are deliberately not read as SAM lines: raw
        # bytes must be the first bytes after each SILENT command.
        outcomes: list[bool] = [False, False]
        accept_thread = threading.Thread(
            target=lambda: outcomes.__setitem__(
                0, raw_transfer(accept, payload_a, payload_b)
            )
        )
        connect_thread = threading.Thread(
            target=lambda: outcomes.__setitem__(
                1, raw_transfer(connect, payload_b, payload_a)
            )
        )
        accept_thread.start()
        connect_thread.start()
        accept_thread.join(timeout=35.0)
        connect_thread.join(timeout=35.0)
        accept_ok, connect_ok = outcomes
        if not accept_ok or not connect_ok:
            print(
                f"silent: raw byte transition failed accept={accept_ok} "
                f"connect={connect_ok} accept_alive={accept_thread.is_alive()} "
                f"connect_alive={connect_thread.is_alive()}",
                file=sys.stderr,
            )
            return 2
        return 0
    finally:
        accept.close()
        connect.close()


def create_session(sam: BufferedSocket, session_id: str) -> str:
    sam.send_line(f"SESSION CREATE STYLE=STREAM ID={session_id} DESTINATION=TRANSIENT")
    reply = sam.recv_line()
    if not reply.startswith("SESSION STATUS RESULT=OK"):
        raise RuntimeError("silent: session create failed")
    return public_from_private(parse_field(reply, "DESTINATION"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    generate = sub.add_parser("generate")
    generate.add_argument("--host", required=True)
    generate.add_argument("--port", type=int, required=True)
    generate.add_argument("--private-output", required=True)
    generate.add_argument("--public-output", required=True)

    naming = sub.add_parser("naming")
    naming.add_argument("--host", required=True)
    naming.add_argument("--port", type=int, required=True)

    negative = sub.add_parser("negative")
    negative.add_argument("--host", required=True)
    negative.add_argument("--port", type=int, required=True)

    silent = sub.add_parser("silent")
    silent.add_argument("--host", required=True)
    silent.add_argument("--port", type=int, required=True)

    args = parser.parse_args()
    if args.command == "generate":
        return cmd_generate(args)
    if args.command == "naming":
        return cmd_naming(args)
    if args.command == "silent":
        return cmd_silent(args)
    return cmd_negative(args)


if __name__ == "__main__":
    raise SystemExit(main())
