"""Plan 150 — minimal localhost TCP echo target.

The FORWARD lane (Plan 150 §11) needs a deterministic localhost
target that does two things:

1. record every byte it receives (so the harness can verify that
   the SAM `STREAM FORWARD` target actually received the bytes
   the connecting client sent);
2. echo the bytes back so the connecting client can read a known
   reply through SAM.

The script is intentionally tiny: it accepts a single TCP
connection, drains it into `received.bin`, then writes the
contents of `echo.bin` back and exits. The harness owns all
file paths.
"""

from __future__ import annotations

import argparse
import socket
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Plan 150 echo target")
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--received-file", required=True)
    parser.add_argument("--echo-file", required=True)
    parser.add_argument("--deadline-seconds", type=float, default=15.0)
    args = parser.parse_args()

    listener = socket.create_server((args.bind, args.port), backlog=1)
    if args.port == 0:
        actual_port = listener.getsockname()[1]
        print(actual_port)
        sys.stdout.flush()

    listener.settimeout(args.deadline_seconds)
    try:
        conn, _addr = listener.accept()
    except socket.timeout:
        listener.close()
        return 7

    conn.settimeout(args.deadline_seconds)
    received = bytearray()
    try:
        while True:
            chunk = conn.recv(4096)
            if not chunk:
                break
            received.extend(chunk)
    except socket.timeout:
        pass

    Path(args.received_file).write_bytes(received)
    echo = Path(args.echo_file).read_bytes()
    try:
        conn.sendall(echo)
    finally:
        conn.close()
        listener.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
