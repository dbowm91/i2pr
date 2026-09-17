#!/usr/bin/env python3
"""Plan 181 §5 loopback raw echo fixture (stdlib only).

Echoes every received chunk back verbatim until EOF on each
accepted connection. Used by the generic byte-stream rows where
the application protocol is intentionally opaque.

Plan 213 §D — with `--facts <file>`, records sanitized
target-side facts sufficient to prove remote bytes reached the
target: connection-count plus per-connection request/response
lengths and SHA-256 digests (never payload bytes).
"""

import argparse
import hashlib
import socket
import sys
import threading

STOP = False
FACTS_LOCK = threading.Lock()


def record_fact(path, key, value):
    if not path:
        return
    with FACTS_LOCK:
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(f"{key}={value}\n")


def serve(stream, facts_path, index) -> None:
    received = bytearray()
    try:
        with stream:
            stream.settimeout(20)
            while True:
                try:
                    chunk = stream.recv(4096)
                except socket.timeout:
                    break
                if not chunk:
                    break
                received += chunk
                try:
                    stream.sendall(chunk)
                except OSError:
                    break
    except OSError:
        pass
    finally:
        if facts_path:
            digest = hashlib.sha256(received).hexdigest()
            record_fact(facts_path, f"conn{index}_request_len", str(len(received)))
            record_fact(facts_path, f"conn{index}_request_sha", digest)
            record_fact(facts_path, f"conn{index}_response_len", str(len(received)))
            record_fact(facts_path, f"conn{index}_response_sha", digest)


def main() -> int:
    global STOP
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--max-connections", type=int, default=8)
    parser.add_argument("--facts", type=str, default="")
    args = parser.parse_args()
    if args.facts:
        with open(args.facts, "w", encoding="utf-8") as handle:
            handle.write("fixture=echo\n")
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", args.port))
    listener.listen(8)
    listener.settimeout(60)
    print(f"PORT={listener.getsockname()[1]}", flush=True)
    threads = []
    served = 0
    try:
        while served < args.max_connections and not STOP:
            try:
                stream, _ = listener.accept()
            except socket.timeout:
                break
            worker = threading.Thread(
                target=serve, args=(stream, args.facts, served), daemon=True
            )
            worker.start()
            threads.append(worker)
            served += 1
    finally:
        for worker in threads:
            worker.join(timeout=20)
        if args.facts:
            record_fact(args.facts, "connection_count", str(served))
    return 0


if __name__ == "__main__":
    sys.exit(main())
