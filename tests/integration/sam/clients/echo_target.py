#!/usr/bin/env python3
"""Bounded localhost target for the Plan 150 STREAM FORWARD lane.

The target accepts one connection, recognizes i2pr's non-silent
``DESTINATION=...`` metadata line when present, then echoes application bytes
as they arrive. It records only those application bytes. Echoing incrementally
keeps the lane live when the SAM connector waits for a response before it
closes its stream.
"""

from __future__ import annotations

import argparse
import socket
import sys
import time
from pathlib import Path

_DESTINATION_PREFIX = b"DESTINATION="
_MAX_DESTINATION_LINE = 600
_MAX_RECORDED_BYTES = 8 * 1024 * 1024
_TRANSFER_BARRIER = b"\x00PLAN150-TRANSFER-DONE\x00"
_TRANSFER_ACK = b"\x00PLAN150-TRANSFER-ACK\x00"


def _read_application_prefix(conn: socket.socket, deadline: float) -> bytearray:
    """Return bytes already read, excluding a valid SAM metadata line."""

    probe = bytearray()
    while len(probe) < len(_DESTINATION_PREFIX):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("target metadata/payload deadline exceeded")
        conn.settimeout(remaining)
        chunk = conn.recv(1)
        if not chunk:
            return probe
        probe.extend(chunk)
        if not _DESTINATION_PREFIX.startswith(probe):
            return probe

    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("target metadata line deadline exceeded")
        conn.settimeout(remaining)
        chunk = conn.recv(1)
        if not chunk:
            return probe
        probe.extend(chunk)
        if len(probe) > _MAX_DESTINATION_LINE:
            raise ValueError("SAM destination metadata line is overlong")
        if chunk == b"\n":
            if not bytes(probe).startswith(_DESTINATION_PREFIX):
                return probe
            return bytearray()


def main() -> int:
    parser = argparse.ArgumentParser(description="Plan 150 localhost forward target")
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--received-file", required=True)
    parser.add_argument("--deadline-seconds", type=float, default=20.0)
    args = parser.parse_args()

    listener = socket.create_server((args.bind, args.port), backlog=1)
    print(f"target listening on {args.bind}:{args.port}", file=sys.stderr, flush=True)
    if args.port == 0:
        print(listener.getsockname()[1], flush=True)
    listener.settimeout(args.deadline_seconds)
    try:
        conn, _address = listener.accept()
    except socket.timeout:
        print("target accept timed out", file=sys.stderr, flush=True)
        listener.close()
        return 7
    print("target accepted connection", file=sys.stderr, flush=True)

    deadline = time.monotonic() + args.deadline_seconds
    received = bytearray()
    try:
        application_prefix = _read_application_prefix(conn, deadline)
        if application_prefix:
            if len(application_prefix) > _MAX_RECORDED_BYTES:
                return 8
            received.extend(application_prefix)
            conn.sendall(application_prefix)

        pending = bytearray()
        barrier_seen = False

        def record_and_echo(application: bytes) -> None:
            if len(received) + len(application) > _MAX_RECORDED_BYTES:
                raise ValueError("application payload exceeds recording ceiling")
            received.extend(application)
            if application:
                conn.sendall(application)

        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return 7
            conn.settimeout(remaining)
            chunk = conn.recv(16 * 1024)
            if not chunk:
                break
            pending.extend(chunk)
            if not barrier_seen:
                barrier_at = pending.find(_TRANSFER_BARRIER)
                if barrier_at < 0:
                    safe_length = max(0, len(pending) - len(_TRANSFER_BARRIER) + 1)
                    record_and_echo(bytes(pending[:safe_length]))
                    del pending[:safe_length]
                    continue
                record_and_echo(bytes(pending[:barrier_at]))
                conn.sendall(_TRANSFER_BARRIER)
                del pending[: barrier_at + len(_TRANSFER_BARRIER)]
                barrier_seen = True
            if barrier_seen and pending:
                if not pending.startswith(_TRANSFER_ACK):
                    raise ValueError("forward target received unexpected post-barrier bytes")
                conn.sendall(_TRANSFER_ACK)
                del pending[: len(_TRANSFER_ACK)]

        if not barrier_seen:
            record_and_echo(bytes(pending))
    except (OSError, TimeoutError, ValueError) as error:
        print(f"target stopped before transfer completion: {type(error).__name__}", file=sys.stderr, flush=True)
        return 7
    finally:
        Path(args.received_file).write_bytes(received)
        try:
            conn.close()
        finally:
            listener.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
