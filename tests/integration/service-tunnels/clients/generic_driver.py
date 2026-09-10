#!/usr/bin/env python3
"""Plan 181 §4.3 generic TCP byte-stream driver (stdlib only).

Generic tunnels intentionally carry no application profile, so a
minimal stdlib socket client is the honest tool here. It exchanges
opaque pattern bytes only and must never be reused as counted
evidence for the profiled tunnels; the static checker rejects
profiled-tunnel framing tokens anywhere in this file.

Modes:
  small      send 25 bytes, read 25 back, digest-compare;
  large      send 98304 pattern bytes, read all back, digest-compare;
  halfclose  send, shutdown(SHUT_WR), read to EOF, digest-compare;
  siblings   two concurrent small exchanges with distinct payloads.

Emits KEY=value facts on stdout.
"""

import argparse
import hashlib
import socket
import sys
import threading

SMALL_A = b"plan181-generic-small-001"
LARGE = bytes((i % 251 for i in range(98304)))
SIB_A = b"sibling-a-payload-001-xxxx"
SIB_B = b"sibling-b-payload-002-yyyy"


def exchange(host, port, payload, half_close, timeout=20):
    sock = socket.create_connection((host, port), timeout=timeout)
    try:
        sock.settimeout(timeout)
        sock.sendall(payload)
        if half_close:
            sock.shutdown(socket.SHUT_WR)
        received = b""
        while len(received) < len(payload):
            chunk = sock.recv(4096)
            if not chunk:
                break
            received += chunk
        eof = False
        if half_close:
            try:
                tail = sock.recv(64)
                eof = tail == b""
            except socket.timeout:
                eof = False
        return received, eof
    finally:
        sock.close()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--mode", required=True,
                        choices=["small", "large", "halfclose", "siblings"])
    args = parser.parse_args()

    if args.mode == "small":
        received, _ = exchange(args.host, args.port, SMALL_A, False)
        print(f"SMALL_DIGEST_MATCH={int(hashlib.sha256(received).hexdigest() == hashlib.sha256(SMALL_A).hexdigest())}")
        print(f"SMALL_LEN={len(received)}")
    elif args.mode == "large":
        received, _ = exchange(args.host, args.port, LARGE, False)
        print(f"LARGE_DIGEST_MATCH={int(hashlib.sha256(received).hexdigest() == hashlib.sha256(LARGE).hexdigest())}")
        print(f"LARGE_LEN={len(received)}")
    elif args.mode == "halfclose":
        received, eof = exchange(args.host, args.port, SMALL_A, True)
        print(f"HALFCLOSE_DIGEST_MATCH={int(hashlib.sha256(received).hexdigest() == hashlib.sha256(SMALL_A).hexdigest())}")
        print(f"HALFCLOSE_EOF={int(eof)}")
    elif args.mode == "siblings":
        results = {}

        def run(name, payload):
            received, _ = exchange(args.host, args.port, payload, False)
            results[name] = (
                hashlib.sha256(received).hexdigest() == hashlib.sha256(payload).hexdigest()
            )

        first = threading.Thread(target=run, args=("A", SIB_A))
        second = threading.Thread(target=run, args=("B", SIB_B))
        first.start()
        second.start()
        first.join(timeout=30)
        second.join(timeout=30)
        print(f"SIBLING_A_MATCH={int(results.get('A', False))}")
        print(f"SIBLING_B_MATCH={int(results.get('B', False))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
