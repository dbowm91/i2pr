#!/usr/bin/env python3
"""Plan 181 §5 loopback HTTP fixture (stdlib only).

Serves the M10 HTTP/SOCKS round-trip rows through the i2pr
generic-server target port. One connection at a time is enough:
every counted row opens a fresh connection.

Protocol:
  * read a bounded HTTP/1.1 head (<=64 KiB, ends in CRLFCRLF);
  * if the method is CONNECT, answer `200 Connection Established`
    and read one more HEAD as the opaque-tunneled request;
  * route: `/large` -> 65536 deterministic bytes, `/post` ->
    200 with the posted length, anything else -> fixed body;
  * POST reads exactly Content-Length bytes and records the
    SHA-256 of the raw body.

Facts are appended as one JSON object per line to --facts. Only
digests/lengths/headers-policy facts are recorded, never user
bodies.
"""

import argparse
import hashlib
import json
import socket
import sys

SMALL_BODY = b"hello-from-loopback-fixture"


def large_body() -> bytes:
    return (bytes(range(256)) * 256)[:65536]


def read_head(stream):
    """Reads one HEAD section, returning (head, surplus).

    `surplus` carries bytes that arrived in the same read past the
    terminator (e.g. a pipelined POST body); callers must consume
    them before reading the socket again.
    """
    head = b""
    while b"\r\n\r\n" not in head and len(head) <= 65536:
        chunk = stream.recv(4096)
        if not chunk:
            break
        head += chunk
    if b"\r\n\r\n" in head:
        head, _, surplus = head.partition(b"\r\n\r\n")
        return head + b"\r\n\r\n", surplus
    return head, b""


def serve_connection(stream, record) -> None:
    head, surplus = read_head(stream)
    if b"\r\n\r\n" not in head:
        return
    request_line = head.split(b"\r\n", 1)[0].decode("latin-1", "replace")
    parts = request_line.split(" ")
    method = parts[0] if parts else ""
    target = parts[1] if len(parts) > 1 else ""
    if method == "CONNECT":
        stream.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        head, surplus = read_head(stream)
        if b"\r\n\r\n" not in head:
            return
        request_line = head.split(b"\r\n", 1)[0].decode("latin-1", "replace")
        parts = request_line.split(" ")
        method = parts[0] if parts else ""
        target = parts[1] if len(parts) > 1 else ""
        tunneled = True
    else:
        tunneled = False
    header_text = head.decode("latin-1", "replace")
    user_agent = ""
    has_referer = False
    has_from = False
    content_length = 0
    for line in header_text.split("\r\n")[1:]:
        lower = line.lower()
        if lower.startswith("user-agent:"):
            user_agent = line.split(":", 1)[1].strip()
        elif lower.startswith("referer:"):
            has_referer = True
        elif lower.startswith("from:"):
            has_from = True
        elif lower.startswith("content-length:"):
            try:
                content_length = int(line.split(":", 1)[1].strip())
            except ValueError:
                content_length = 0
    body = b""
    if method == "POST" and content_length > 0 and content_length <= 1_048_576:
        chunks = [surplus] if surplus else []
        remaining = content_length - len(surplus)
        while remaining > 0:
            chunk = stream.recv(min(4096, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        body = b"".join(chunks)[:content_length]
    path = target.split("?", 1)[0]
    if path.endswith("/large"):
        response_body = large_body()
    elif method == "POST":
        response_body = f"posted={len(body)}".encode("ascii")
    else:
        response_body = SMALL_BODY
    header = (
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n"
        f"Content-Length: {len(response_body)}\r\nConnection: close\r\n\r\n"
    ).encode("ascii")
    stream.sendall(header + response_body)
    record(
        {
            "method": method,
            "target": target,
            "tunneled": tunneled,
            "body_sha256": hashlib.sha256(body).hexdigest() if body else "",
            "body_len": len(body),
            "user_agent": user_agent,
            "has_referer": has_referer,
            "has_from": has_from,
        }
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--facts", required=True)
    parser.add_argument("--max-connections", type=int, default=16)
    args = parser.parse_args()
    # Facts append per request (flushed) so the harness can read
    # them mid-run; a SIGTERM shutdown must not lose served rows.
    facts_log = open(args.facts, "w", encoding="utf-8")
    facts = []

    def record(fact) -> None:
        facts.append(fact)
        facts_log.write(json.dumps(fact, sort_keys=True) + "\n")
        facts_log.flush()

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", args.port))
    listener.listen(4)
    listener.settimeout(90)
    print(f"PORT={listener.getsockname()[1]}", flush=True)
    served = 0
    try:
        while served < args.max_connections:
            try:
                stream, _ = listener.accept()
            except socket.timeout:
                break
            served += 1
            try:
                with stream:
                    stream.settimeout(15)
                    serve_connection(stream, record)
            except (OSError, ValueError):
                pass
    finally:
        facts_log.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
