#!/usr/bin/env python3
"""Plan 181 §5 loopback raw echo fixture (stdlib only).

Echoes every received chunk back verbatim until EOF on each
accepted connection. Used by the generic byte-stream rows where
the application protocol is intentionally opaque.
"""

import argparse
import socket
import sys
import threading

STOP = False


def serve(stream) -> None:
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
                try:
                    stream.sendall(chunk)
                except OSError:
                    break
    except OSError:
        pass


def main() -> int:
    global STOP
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--max-connections", type=int, default=8)
    args = parser.parse_args()
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
            served += 1
            worker = threading.Thread(target=serve, args=(stream,), daemon=True)
            worker.start()
            threads.append(worker)
    finally:
        for worker in threads:
            worker.join(timeout=20)
    return 0


if __name__ == "__main__":
    sys.exit(main())
