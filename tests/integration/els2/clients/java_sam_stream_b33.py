#!/usr/bin/env python3
"""Plan 412 stock Java SAM STREAM requester for a blinded destination.

The helper receives the address and listener endpoints only through its
environment, writes only categorical stage results and integrity hashes, and
never persists a SAM reply or private destination material.
"""

from __future__ import annotations

import hashlib
import json
import os
import socket
import sys
import time


def required(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        raise RuntimeError(f"missing required environment variable: {name}")
    return value


def read_line(stream: socket.socket, deadline: float) -> str:
    data = bytearray()
    while time.monotonic() < deadline:
        chunk = stream.recv(1)
        if not chunk:
            raise RuntimeError("sam-closed")
        if chunk == b"\n":
            return data.decode("ascii", errors="strict").rstrip("\r")
        if len(data) >= 4096:
            raise RuntimeError("sam-line-over-limit")
        data.extend(chunk)
    raise TimeoutError("sam-read-deadline")


def command(port: int, line: str, timeout_s: float) -> tuple[str, socket.socket]:
    sock = socket.create_connection(("127.0.0.1", port), timeout=timeout_s)
    sock.settimeout(timeout_s)
    sock.sendall(line.encode("ascii") + b"\n")
    reply = read_line(sock, time.monotonic() + timeout_s)
    return reply, sock


def result(stage: str, outcome: str, payload: bytes = b"") -> None:
    print(json.dumps({
        "schema": "i2pr-java-els2-noauth-requester-v1",
        "stage": stage,
        "outcome": outcome,
        "payload_bytes": len(payload),
        "payload_sha256": hashlib.sha256(payload).hexdigest() if payload else None,
        "request_sha256": hashlib.sha256(b"plan412-request\n").hexdigest(),
    }, sort_keys=True))


def main() -> int:
    sam_port = int(required("I2PR_JAVA_SAM_PORT"))
    destination = required("I2PR_JAVA_SAM_DESTINATION")
    destination_port = int(required("I2PR_JAVA_SAM_DESTINATION_PORT"))
    session_id = required("I2PR_JAVA_SAM_SESSION_ID")
    expected = required("I2PR_JAVA_SAM_EXPECTED_PAYLOAD").encode("ascii")
    if not destination.endswith(".b32.i2p"):
        raise RuntimeError("destination-must-use-stock-java-b32-spelling")
    if not session_id.isascii() or any(c.isspace() for c in session_id):
        raise RuntimeError("invalid-session-id")

    stage = "hello"
    session: socket.socket | None = None
    stream: socket.socket | None = None
    try:
        hello, session = command(sam_port, "HELLO VERSION MIN=3.1 MAX=3.1", 30)
        if "RESULT=OK" not in hello:
            result(stage, "sam-error")
            return 2
        stage = "session-create"
        session.sendall(
            f"SESSION CREATE STYLE=STREAM ID={session_id} "
            "DESTINATION=TRANSIENT SIGNATURE_TYPE=7\n".encode("ascii")
        )
        create = read_line(session, time.monotonic() + 240)
        if "RESULT=OK" not in create:
            result(stage, "sam-error")
            return 2
        session.close()
        session = None

        stage = "stream-connect"
        connect_reply, stream = command(
            sam_port,
            "HELLO VERSION MIN=3.1 MAX=3.1",
            30,
        )
        if "RESULT=OK" not in connect_reply:
            result(stage, "sam-error")
            return 2
        stream.sendall(
            f"STREAM CONNECT ID={session_id} DESTINATION={destination} PORT={destination_port}\n".encode("ascii")
        )
        connect = read_line(stream, time.monotonic() + 300)
        if "RESULT=OK" not in connect:
            result(stage, "lookup-or-connect-failed")
            return 2

        stage = "payload"
        stream.settimeout(120)
        stream.sendall(b"plan412-java-requester\n")
        received = bytearray()
        deadline = time.monotonic() + 120
        while expected not in received and time.monotonic() < deadline:
            block = stream.recv(4096)
            if not block:
                break
            received.extend(block)
            if len(received) > 65536:
                raise RuntimeError("payload-over-limit")
        if expected not in received:
            result(stage, "payload-mismatch", bytes(received))
            return 3
        result(stage, "payload-ok", bytes(received))
        return 0
    except (OSError, TimeoutError, RuntimeError, UnicodeError, ValueError) as exc:
        # Exception text can contain endpoint or peer data; report only a
        # stable class token so raw protocol details never enter evidence.
        result(stage, type(exc).__name__.lower())
        return 4
    finally:
        if stream is not None:
            stream.close()
        if session is not None:
            session.close()


if __name__ == "__main__":
    sys.exit(main())
