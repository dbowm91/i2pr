#!/usr/bin/env python3
"""Black-box SAM driver for Plan 458's ordinary i2pr process lane."""

import argparse
import hashlib
import json
import re
import socket
import threading
import time


HELLO = b"HELLO VERSION MIN=3.1 MAX=3.1\n"
PAYLOADS = {
    "i2pr-to-i2pd": b"plan458-i2pr-request\x00\xff\xc3\x28" + bytes(range(256)) * 7,
    "i2pd-to-i2pr": b"plan458-i2pd-request\x00\x80\xfe\x00" + bytes(reversed(range(256))) * 9,
}


class SamSocket:
    def __init__(self, endpoint, timeout):
        host, port = endpoint.rsplit(":", 1)
        self.sock = socket.create_connection((host, int(port)), timeout=timeout)
        self.sock.settimeout(timeout)
        self.buffer = bytearray()

    def line(self):
        while True:
            pos = self.buffer.find(b"\n")
            if pos >= 0:
                value = bytes(self.buffer[: pos + 1])
                del self.buffer[: pos + 1]
                return value.decode("ascii", "replace").strip()
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("SAM closed while reading a line")
            self.buffer.extend(chunk)

    def exact(self, count):
        out = bytearray()
        if self.buffer:
            take = min(count, len(self.buffer))
            out.extend(self.buffer[:take])
            del self.buffer[:take]
        while len(out) < count:
            chunk = self.sock.recv(count - len(out))
            if not chunk:
                raise ConnectionError(f"short stream read ({len(out)}/{count})")
            out.extend(chunk)
        return bytes(out)

    def send_line(self, value):
        self.sock.sendall(value.encode("ascii") + b"\n")

    def hello(self):
        self.send_line("HELLO VERSION MIN=3.1 MAX=3.1")
        reply = self.line()
        if not reply.startswith("HELLO REPLY RESULT=OK"):
            raise RuntimeError("SAM HELLO was rejected")

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


def field(line, key):
    needle = f" {key}="
    if needle not in line:
        return None
    return line.split(needle, 1)[1].split(" ", 1)[0]


def session_create_failure_category(message):
    match = re.search(
        r"router-backed SAM destination provisioning failed at ([a-z0-9-]+)",
        message,
    )
    if match:
        return match.group(1)
    static_prefixes = (
        ("destination runtime construction failed:", "destination-runtime-construction"),
        ("destination registry capacity ", "destination-registry-capacity"),
        ("destination registry command queue full", "destination-registry-queue-full"),
        ("destination registry mutex poisoned", "destination-registry-poisoned"),
        ("duplicate destination", "duplicate-destination"),
        ("I2P error", "i2p-error"),
    )
    for prefix, category in static_prefixes:
        if message.startswith(prefix):
            return category
    destination_match = re.match(r"destination ([a-z0-9]+)", message)
    if destination_match:
        destination_categories = {
            "identity",
            "pool",
            "registry",
            "runtime",
            "session",
            "tunnel",
        }
        second = destination_match.group(1)
        if second in destination_categories:
            return f"destination-{second}"
        return "destination-error"
    return "unclassified"


def create_session(endpoint, session_id, timeout, crypto_type=None):
    sam = SamSocket(endpoint, timeout)
    sam.hello()
    # Plan 478: the stock peer must use an ECIES-X25519 destination
    # (CRYPTO_TYPE=4). The i2pr router product is ECIES-only by
    # declared scope (ElGamal fails closed); stock defaults to
    # ElGamal. Our own session keeps its existing parameters.
    generate = "DEST GENERATE SIGNATURE_TYPE=7"
    if crypto_type is not None:
        generate += f" CRYPTO_TYPE={crypto_type}"
    sam.send_line(generate)
    generated = sam.line()
    private = field(generated, "PRIV")
    if not generated.startswith("DEST REPLY") or not private:
        sam.close()
        raise RuntimeError("SAM DEST GENERATE failed")
    sam.send_line(
        f"SESSION CREATE STYLE=STREAM ID={session_id} DESTINATION={private} "
        "SIGNATURE_TYPE=7 inbound.length=0 outbound.length=0"
    )
    created = sam.line()
    if not created.startswith("SESSION STATUS RESULT=OK"):
        sam.close()
        message = re.search(r'MESSAGE="([^"\\]*)', created)
        category = session_create_failure_category(message.group(1) if message else "")
        raise RuntimeError(
            "SAM SESSION CREATE failed: status=I2P_ERROR "
            f"failure_category={category}"
        )
    sam.send_line("NAMING LOOKUP NAME=ME")
    named = sam.line()
    public = field(named, "VALUE")
    if not named.startswith("NAMING REPLY RESULT=OK") or not public:
        sam.close()
        raise RuntimeError("SAM local destination lookup failed")
    return sam, public


def open_stream(endpoint, command, timeout, wait_for_peer_line=False):
    sam = SamSocket(endpoint, timeout)
    sam.hello()
    sam.send_line(command)
    status = sam.line()
    if "RESULT=OK" not in status:
        sam.close()
        # Full status line (truncated): endpoint attribution matters
        # because both routers emit STREAM STATUS lines.
        raise RuntimeError(
            f"SAM stream command failed on {endpoint}: {command.split(' ', 3)[:3]} "
            f"status={status[:160]!r}"
        )
    if wait_for_peer_line:
        # STREAM ACCEPT emits the peer destination line before raw stream bytes.
        sam.line()
    return sam


def echo_accept(endpoint, session_id, payload, timeout, result):
    sam = open_stream(endpoint, f"STREAM ACCEPT ID={session_id}", timeout, True)
    try:
        received = sam.exact(len(payload))
        if received != payload:
            raise RuntimeError("accepted stream payload differs from request")
        sam.sock.sendall(received)
        result["length"] = len(received)
        result["sha256"] = hashlib.sha256(received).hexdigest()
    finally:
        sam.close()


def connect_exchange(endpoint, session_id, destination, payload, timeout):
    sam = open_stream(
        endpoint,
        f"STREAM CONNECT ID={session_id} DESTINATION={destination}",
        timeout,
    )
    try:
        sam.sock.sendall(payload)
        echoed = sam.exact(len(payload))
        if echoed != payload:
            raise RuntimeError("stream echo differs from exact request bytes")
        return {"length": len(echoed), "sha256": hashlib.sha256(echoed).hexdigest()}
    finally:
        sam.close()


def run_leg(sender, receiver, sid, rid, remote, payload, timeout):
    """One complete STREAM leg: accept worker plus CONNECT exchange."""
    accepted = {}
    failure = []

    def accept():
        try:
            echo_accept(receiver, rid, payload, timeout, accepted)
        except Exception as exc:
            failure.append(str(exc))

    worker = threading.Thread(target=accept, daemon=True)
    worker.start()
    try:
        sent = connect_exchange(sender, sid, remote, payload, timeout)
    except Exception:
        # A failed CONNECT leaves its accept worker parked on the
        # socket recv; it is daemon-owned and dies on socket timeout.
        # Never block recovery on it.
        raise
    worker.join(timeout)
    if worker.is_alive():
        raise TimeoutError("accepting SAM stream did not finish")
    if failure:
        raise RuntimeError(f"accept failed: {failure[0]}")
    if accepted != sent:
        raise RuntimeError("sender/receiver digest mismatch")
    return sent


def run_forward_leg_with_recovery(sender, receiver, sid, rid, remote, payload, timeout,
                                 sleep_fn=time.sleep):
    """Forward STREAM leg with at most one fresh-handshake recovery.

    Returns `(sent, recovery)`. A fresh CONNECT after a failed first
    handshake is a new handshake with new stream/SYN ids — not a retry
    of the same send — and is only attempted on the forward leg, where
    prior runs show stock terminating its stream after a single
    SYN-ACK attempt that never arrives.
    """
    recovery = {}
    try:
        sent = run_leg(sender, receiver, sid, rid, remote, payload, timeout)
    except Exception as first_exc:
        recovery["forward_first_error"] = str(first_exc)[:200]
        sleep_fn(5)
        sent = run_leg(sender, receiver, sid, rid, remote, payload, timeout)
        recovery["forward_recovered"] = True
    return sent, recovery


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--i2pr-sam", required=True)
    parser.add_argument("--i2pd-sam", required=True)
    parser.add_argument("--timeout", type=float, default=240.0)
    parser.add_argument("--evidence", required=True)
    parser.add_argument("--stock-control", action="store_true")
    args = parser.parse_args()

    i2pr_session, i2pr_public = create_session(args.i2pr_sam, "p458router", args.timeout)
    i2pd_session, i2pd_public = create_session(
        args.i2pd_sam, "p458peer", args.timeout, crypto_type="4"
    )
    # Peer identities are public (published in NetDb); record shapes
    # (lengths only) so lane forensics can match LS2 store keys and
    # destination decodes without persisting key material.
    print(
        f"peer destinations: i2pr_public_len={len(i2pr_public)} "
        f"i2pd_public_len={len(i2pd_public)}",
        flush=True,
    )
    try:
        with open(args.evidence + ".peers.json", "w", encoding="utf-8") as peers_handle:
            json.dump(
                {"schema": "plan458-sam-peers-v1", "i2pr_public": i2pr_public,
                 "i2pd_public": i2pd_public},
                peers_handle,
            )
            peers_handle.write("\n")
    except OSError as exc:
        print(f"peer evidence unavailable: {exc}", flush=True)
    outcomes = {}
    recovery = {}
    try:
        for label, sender, receiver, sid, rid in (
            ("i2pr-to-i2pd", args.i2pr_sam, args.i2pd_sam, "p458router", "p458peer"),
            ("i2pd-to-i2pr", args.i2pd_sam, args.i2pr_sam, "p458peer", "p458router"),
        ):
            payload = PAYLOADS[label]
            remote = i2pd_public if label == "i2pr-to-i2pd" else i2pr_public
            if label == "i2pr-to-i2pd":
                # Plan 478 fresh-handshake recovery, forward leg only,
                # at most once (see run_forward_leg_with_recovery).
                # Reverse leg keeps single-attempt shape.
                sent, leg_recovery = run_forward_leg_with_recovery(
                    sender, receiver, sid, rid, remote, payload, args.timeout
                )
                recovery.update(leg_recovery)
            else:
                sent = run_leg(sender, receiver, sid, rid, remote, payload, args.timeout)
            evidence_label = (
                {
                    "i2pr-to-i2pd": "stock-f-to-stock-c",
                    "i2pd-to-i2pr": "stock-c-to-stock-f",
                }[label]
                if args.stock_control
                else label
            )
            outcomes[evidence_label] = sent
    finally:
        i2pr_session.close()
        i2pd_session.close()

    with open(args.evidence, "w", encoding="utf-8") as handle:
        json.dump(
            {"schema": "plan458-sam-exchange-v1", "directions": outcomes,
             "recovery": recovery},
            handle,
        )
        handle.write("\n")
    for label, row in outcomes.items():
        print(f"{label}=passed bytes={row['length']} sha256={row['sha256']}")


if __name__ == "__main__":
    main()
