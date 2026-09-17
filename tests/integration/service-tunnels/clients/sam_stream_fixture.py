#!/usr/bin/env python3
"""Plan 213 — harness-only i2pd SAM STREAM fixture (stdlib only).

The helper is NOT product code. It speaks only i2pd's public SAM
3.1 interface on loopback and carries no tunnel/Streaming/Garlic
implementation of its own.

`server` mode (Direction A target):
  1. HELLO + DEST GENERATE + SESSION CREATE STYLE=STREAM on one
     session socket (kept open for the session lifetime);
  2. writes the PUBLIC destination to --pub-file (scratch only,
     never evidence) and prints READY=1 on stdout;
  3. accepts --expected-connections sequential inbound streams,
     each over a FRESH SAM socket (exact-pinned i2pd 2.61.0
     rejects ACCEPT on the bound session socket with
     `Socket already in use`, cf. Plan 193);
  4. echoes every received chunk back verbatim until EOF;
  5. records per-connection lengths/digests (never payload bytes,
     never private material) to --facts.

`connect` mode (Direction B initiator):
  1. HELLO + DEST GENERATE + SESSION CREATE STYLE=STREAM on one
     session socket (own transient destination);
  2. opens one STREAM connection per payload over a FRESH SAM
     socket (`STREAM CONNECT ID=<sid> DESTINATION=<server PUB>`,
     same fresh-socket rule as ACCEPT);
  3. waits for `STREAM STATUS RESULT=OK`, sends deterministic
     small then large payloads (separate connections so the
     target-side fixture attributes one digest per connection),
     verifies exact echo equality;
  4. exits 0 only on STATUS OK + both digest matches; any
     timeout, lookup failure, connect failure, short read,
     digest mismatch, or unexpected close exits nonzero.

Usage:
  sam_stream_fixture.py --mode server --sam 127.0.0.1:PORT \\
      --session-id plan213-a --pub-file PUB --facts FACTS \\
      --expected-connections 2
  sam_stream_fixture.py --mode connect --sam 127.0.0.1:PORT \\
      --session-id plan213-b --destination-pub B64 --facts FACTS
"""

import argparse
import base64
import hashlib
import socket
import sys
import time

HELLO = b"HELLO VERSION MIN=3.1 MAX=3.1\n"
SMALL_PAYLOAD = b"plan213-direction-b-small"
LARGE_LEN = 8192
MAX_CONN_BYTES = 1 << 20


def log(message):
    print(message, flush=True)


def read_line(sock, deadline, what):
    buf = bytearray()
    while time.monotonic() < deadline:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        sock.settimeout(min(remaining, 5.0))
        try:
            chunk = sock.recv(1)
        except socket.timeout:
            continue
        if not chunk:
            raise ConnectionError(f"{what}: EOF while reading line")
        buf += chunk
        if buf.endswith(b"\n"):
            return bytes(buf).decode("latin-1", "replace")
    raise TimeoutError(f"{what}: timed out waiting for line")


def transact(sock, command, deadline, what):
    sock.sendall(command)
    return read_line(sock, deadline, what)


def read_exact(sock, count, deadline, what):
    out = bytearray()
    while len(out) < count:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(
                f"{what}: short read ({len(out)}/{count})"
            )
        sock.settimeout(min(remaining, 5.0))
        try:
            chunk = sock.recv(min(count - len(out), 65536))
        except socket.timeout:
            continue
        if not chunk:
            raise ConnectionError(
                f"{what}: EOF after {len(out)}/{count} bytes"
            )
        out += chunk
    return bytes(out)


def i2p_b64decode(text):
    std = text.strip().replace("-", "+").replace("~", "/")
    pad = (-len(std)) % 4
    std += "=" * pad
    return base64.b64decode(std, validate=False)


def dest_facts(pub_b64):
    pub_bytes = i2p_b64decode(pub_b64)
    digest = hashlib.sha256(pub_bytes).digest()
    b32 = base64.b32encode(digest).decode("ascii").rstrip("=").lower()
    return {
        "pub_len": str(len(pub_bytes)),
        "dest_hash": digest.hex(),
        "dest_b32": f"{b32}.b32.i2p",
    }


def open_sam(sam_host, sam_port, timeout=15.0):
    sock = socket.create_connection((sam_host, sam_port), timeout=timeout)
    return sock


def sam_param(line, name):
    token = f" {name}="
    if token not in line:
        return None
    return line.split(token, 1)[1].split(" ", 1)[0].strip()


def create_stream_session(sock, session_id, deadline):
    hello = transact(sock, HELLO, deadline, "hello")
    if "RESULT=OK" not in hello:
        raise RuntimeError(f"SAM hello failed: {hello[:120]}")
    generated = transact(
        sock, b"DEST GENERATE SIGNATURE_TYPE=7\n", deadline, "dest-generate"
    )
    if not generated.startswith("DEST REPLY") or " PUB=" not in generated:
        raise RuntimeError(f"SAM DEST GENERATE failed: {generated[:120]}")
    pub = sam_param(generated, "PUB")
    priv = sam_param(generated, "PRIV")
    if not pub or len(pub) < 512 or not priv:
        raise RuntimeError("SAM DEST GENERATE yielded no usable PUB/PRIV")
    create = transact(
        sock,
        (
            f"SESSION CREATE STYLE=STREAM ID={session_id} "
            f"DESTINATION={priv} SIGNATURE_TYPE=7 "
            f"inbound.length=0 outbound.length=0\n"
        ).encode("ascii"),
        deadline,
        "session-create",
    )
    if "RESULT=OK" not in create:
        raise RuntimeError(f"SAM SESSION CREATE failed: {create[:160]}")
    return pub


def run_server(args):
    deadline = time.monotonic() + args.setup_timeout
    session = open_sam(args.sam_host, args.sam_port)
    try:
        pub = create_stream_session(session, args.session_id, deadline)
    except Exception as exc:
        log(f"SERVER_SETUP_FAILED={exc}")
        return 2
    facts = dest_facts(pub)
    with open(args.pub_file, "w", encoding="ascii") as handle:
        handle.write(pub + "\n")
    with open(args.facts, "w", encoding="utf-8") as handle:
        handle.write(f"dest_hash={facts['dest_hash']}\n")
        handle.write(f"dest_b32={facts['dest_b32']}\n")
        handle.write(f"pub_len={facts['pub_len']}\n")
    # The session socket stays open for the session lifetime; the
    # public facts above are the only material the runner consumes.
    log(
        f"READY=1 dest_hash={facts['dest_hash']} "
        f"dest_b32={facts['dest_b32']} pub_len={facts['pub_len']}"
    )
    digest_all = hashlib.sha256()
    total = 0
    for index in range(args.expected_connections):
        # Plan 213 / Plan 193 accept ordering: when --accept-trigger
        # is set, each ACCEPT waits for the driver's trigger file,
        # which the driver writes only after the SYN cells were
        # composed and delivered. The reference then picks up the
        # buffered inbound stream — the accept-after-data-in-flight
        # ordering the proven Plan 193 lane uses. A pending ACCEPT
        # registered long before (or racing) the SYN never matches
        # on exact-pinned i2pd 2.61.0.
        if args.accept_trigger:
            trigger_deadline = time.monotonic() + args.accept_timeout
            trigger_path = f"{args.accept_trigger}.{index}"
            while time.monotonic() < trigger_deadline:
                try:
                    with open(trigger_path, "rb"):
                        break
                except OSError:
                    time.sleep(0.2)
            else:
                log(f"ACCEPT_TRIGGER_TIMEOUT conn={index}")
                return 2
            log(f"TRIGGER_SEEN={index}")
        accept_deadline = time.monotonic() + args.accept_timeout
        acceptor = open_sam(args.sam_host, args.sam_port)
        try:
            hello = transact(acceptor, HELLO, accept_deadline, "accept-hello")
            if "RESULT=OK" not in hello:
                raise RuntimeError(f"ACCEPT hello failed: {hello[:80]}")
            reply = transact(
                acceptor,
                f"STREAM ACCEPT ID={args.session_id}\n".encode("ascii"),
                accept_deadline,
                "accept",
            )
            if "RESULT=OK" not in reply:
                raise RuntimeError(f"STREAM ACCEPT failed: {reply[:160]}")
            # After RESULT=OK the bridge sends the peer destination
            # line, then raw stream bytes follow on this socket.
            try:
                peer_line = read_line(acceptor, accept_deadline, "peer-line")
            except (TimeoutError, ConnectionError):
                peer_line = ""
            received = bytearray()
            conn_deadline = time.monotonic() + args.conn_timeout
            acceptor.settimeout(5.0)
            while time.monotonic() < conn_deadline:
                try:
                    chunk = acceptor.recv(65536)
                except socket.timeout:
                    if received:
                        break
                    continue
                if not chunk:
                    break
                received += chunk
                digest_all.update(chunk)
                total += len(chunk)
                try:
                    acceptor.sendall(chunk)
                except OSError:
                    break
                if len(received) >= MAX_CONN_BYTES:
                    break
            conn_sha = hashlib.sha256(received).hexdigest()
            with open(args.facts, "a", encoding="utf-8") as handle:
                handle.write(f"conn{index}_len={len(received)}\n")
                handle.write(f"conn{index}_sha={conn_sha}\n")
                handle.write(f"conn{index}_peer_line_len={len(peer_line)}\n")
            log(f"CONN={index} len={len(received)} sha={conn_sha}")
        finally:
            try:
                acceptor.close()
            except OSError:
                pass
    with open(args.facts, "a", encoding="utf-8") as handle:
        handle.write(f"connection_count={args.expected_connections}\n")
        handle.write(f"total_len={total}\n")
        handle.write(f"total_sha={digest_all.hexdigest()}\n")
    log(f"DONE=1 connection_count={args.expected_connections} total_len={total}")
    return 0


def run_connect(args):
    deadline = time.monotonic() + args.setup_timeout
    session = open_sam(args.sam_host, args.sam_port)
    try:
        create_stream_session(session, args.session_id, deadline)
    except Exception as exc:
        log(f"CONNECT_SETUP_FAILED={exc}")
        return 2
    payloads = [SMALL_PAYLOAD, bytes((i % 251) for i in range(LARGE_LEN))]
    results = []
    for index, payload in enumerate(payloads):
        conn_deadline = time.monotonic() + args.conn_timeout
        stream = open_sam(args.sam_host, args.sam_port)
        try:
            hello = transact(stream, HELLO, conn_deadline, "connect-hello")
            if "RESULT=OK" not in hello:
                raise RuntimeError(f"CONNECT hello failed: {hello[:80]}")
            stream.sendall(
                (
                    f"STREAM CONNECT ID={args.session_id} "
                    f"DESTINATION={args.destination_pub}\n"
                ).encode("ascii")
            )
            status = None
            while time.monotonic() < conn_deadline and status is None:
                try:
                    line = read_line(stream, conn_deadline, "connect-status")
                except (TimeoutError, ConnectionError):
                    break
                if line.startswith("STREAM STATUS"):
                    status = line
            if status is None or "RESULT=OK" not in status:
                raise RuntimeError(
                    f"STREAM CONNECT never returned OK: {(status or 'none')[:160]}"
                )
            stream.sendall(payload)
            echoed = read_exact(stream, len(payload), conn_deadline, "echo")
            digest = hashlib.sha256(echoed).hexdigest()
            expected = hashlib.sha256(payload).hexdigest()
            match = echoed == payload
            results.append((len(payload), digest, match))
            log(
                f"PAYLOAD={index} len={len(payload)} "
                f"sha={digest} match={int(match)}"
            )
            try:
                stream.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        finally:
            try:
                stream.close()
            except OSError:
                pass
    small_ok = len(results) > 0 and results[0][2]
    large_ok = len(results) > 1 and results[1][2]
    with open(args.facts, "w", encoding="utf-8") as handle:
        handle.write("connect_status_ok=1\n")
        if results:
            handle.write(f"small_len={results[0][0]}\n")
            handle.write(f"small_sha={results[0][1]}\n")
            handle.write(f"small_match={int(small_ok)}\n")
        if len(results) > 1:
            handle.write(f"large_len={results[1][0]}\n")
            handle.write(f"large_sha={results[1][1]}\n")
            handle.write(f"large_match={int(large_ok)}\n")
    log(f"DONE=1 small_match={int(small_ok)} large_match={int(large_ok)}")
    return 0 if (small_ok and large_ok) else 1


def parse_host_port(value):
    host, _, port = value.rpartition(":")
    return host or "127.0.0.1", int(port)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("server", "connect"), required=True)
    parser.add_argument("--sam", default="127.0.0.1:7656")
    parser.add_argument("--session-id", default="plan213-fixture")
    parser.add_argument("--pub-file", default="")
    parser.add_argument("--accept-trigger", default="")
    parser.add_argument("--destination-pub", default="")
    parser.add_argument("--facts", default="")
    parser.add_argument("--expected-connections", type=int, default=2)
    parser.add_argument("--setup-timeout", type=float, default=60.0)
    parser.add_argument("--accept-timeout", type=float, default=150.0)
    parser.add_argument("--conn-timeout", type=float, default=150.0)
    args = parser.parse_args()
    args.sam_host, args.sam_port = parse_host_port(args.sam)
    if args.mode == "server" and not args.pub_file:
        parser.error("--pub-file is required in server mode")
    if args.mode == "connect" and not args.destination_pub:
        parser.error("--destination-pub is required in connect mode")
    if not args.facts:
        parser.error("--facts is required")
    try:
        if args.mode == "server":
            return run_server(args)
        return run_connect(args)
    except Exception as exc:  # fail closed, never hang the lane
        log(f"FATAL={exc}")
        return 3


if __name__ == "__main__":
    sys.exit(main())
