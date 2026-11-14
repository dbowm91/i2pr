#!/usr/bin/env python3
"""Plan 487 §6 live lifecycle-recovery qualification.

The Plan 478 §6 gate asks for one controlled exact-pinned normal-process
multirouter lane that qualifies the reverse DATA path **and** the
destination/session recovery through one tunnel expiry/replenishment or
controlled peer loss and a daemon restart, in the same qualification.

This runner therefore does two things in **one** mesh and **one**
qualification:

1. It runs the Plan 458 normal-process SAM qualification unchanged
   (`run_plan458_sam_product.Lane.run`), which exercises both
   directions — including the reverse external SAM STREAM DATA phase.
2. It then runs the §6 lifecycle phases against the same daemon and the
   same stock peers, in the order ADR 0040 requires:

   - **L1 — daemon restart** (ADR 0040 case 2). The daemon is stopped
     and started again with the same data directory. The SAM connection
     **closes**, the router identity and its transport static key are
     unchanged (they are disk-persisted), and the destination is
     reachable again through a fresh session.
   - **L2 — controlled peer loss** (ADR 0040 case 1). One stock transit
     peer is killed while a reverse stream is open. A stream is bound to
     a *destination*, never to a tunnel, so the stream **survives** and
     the client-observable result is **nothing**. The lane proves this by
     exchanging a second payload on the *same* stream after the loss and
     requiring the exact bytes back.

The restart comes first because a dead peer breaks the stock client's
LeaseSet2 lookup: the peer still lists the dead floodfill in its netdb
and selects it for the routing key, so a fresh connection after a peer
loss cannot resolve a destination. The peer loss is therefore last, and
it is measured on an already-established stream, which needs no further
lookup.

The lane also reads the daemon's own publication receipts so the
publish-and-acknowledge loop is evidenced live: a publication carries a
reply token, the floodfill answers with a `DeliveryStatus`, and the
correlated status is observed arriving on an inbound tunnel.

Every claim is asserted. A lane that cannot prove a claim fails; it
never records a claim as passed because another plan says so.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))

import plan458_sam_driver as samdrv  # noqa: E402
import run_plan458_sam_product as plan458  # noqa: E402

EVIDENCE = Path(
    os.environ.get(
        "I2PR_PLAN487_EVIDENCE_DIR", ROOT / "target/interop/plan487-lifecycle-evidence"
    )
)

# The daemon emits terminal colour escapes even when its stdout is a
# pipe, and the escapes land *inside* the tracing field pairs
# (`transition` / `=` / the quoted value). A raw substring search for
# `transition="…"` therefore never matches, and every receipt count
# would silently read zero. The Plan 458 lane already owns the
# canonical stripper; the lifecycle lane reuses it.
strip_ansi = plan458.strip_ansi

# The payload the lifecycle phases exchange. The size is the same one
# the Plan 458 legs use, so the reverse DATA path carries a multi-packet
# sequence and the stream is genuinely long-lived across the phases.
PAYLOAD = bytes((index * 31 + 7) & 0xFF for index in range(2328))

# How many fresh handshakes the post-restart reachability claim may
# use. Each attempt is a new session, a new ephemeral destination, and a
# new handshake with new stream and SYN identifiers — never a resend of
# the same bytes — so the budget cannot mask a data-phase defect.
REACHABILITY_ATTEMPTS = 3

# How long the restarted router may take to publish a LeaseSet2 that a
# floodfill acknowledges. The acknowledgement is the router's own proof
# that the network stored the lease, which is the precondition a stock
# peer needs before its LeaseSet2 lookup can succeed.
PUBLICATION_READY_BUDGET_S = 240.0

# How long the restarted router may take to answer the post-restart
# reachability handshake.
REACHABILITY_BUDGET_S = 240.0

# How long the lane waits for the destination's lease rotation. The
# rotation is margin-driven: the lease expires about 540 s after the
# publication and the rotation margin is 120 s, so the rotation lands
# roughly seven minutes after the publication.
ROTATION_BUDGET_S = 900.0

# How long one data-phase round trip may take on an open stream. The
# router's own four-stage DATA-phase attribution lands about 20 s
# after the connection reaches the established state, so this budget
# is long enough for the router to classify a stall and short enough
# that a classified stall fails the lane instead of waiting it out.
EXCHANGE_BUDGET_S = 90.0


class StreamExchangeError(RuntimeError):
    """A failed request/response round trip on an already-open SAM stream."""


def _digest(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def read_receipt_lines(stdout_path: Path, offset: int = 0) -> list[str]:
    """Return the daemon's receipt lines from `offset`, ANSI-stripped.

    The daemon's stdout is appended to across a restart, so `offset` is
    the byte length the file had when a phase began. Reading from a
    byte offset keeps "before the restart" and "after the restart"
    receipts separable without parsing timestamps.
    """
    try:
        with stdout_path.open("rb") as handle:
            handle.seek(offset)
            payload = handle.read()
    except OSError:
        return []
    return [strip_ansi(line) for line in payload.decode("utf-8", "replace").splitlines()]


def read_peer_streaming_counts(peer_log_path: Path) -> dict:
    """Count the stock peer's streaming events for one stream.

    The peer is the exact-pinned reference, so its log is a *source*,
    not evidence: only counts leave this function. The counts separate
    the candidate causes of a stalled reverse leg from the peer's side
    — the peer never got a SYN-ACK, the peer sent data the router
    never answered, or the peer processed an echo the router sent.
    """
    counts = {
        "syn_ack_received": 0,
        "data_packets_processed": 0,
        "packets_acknowledged": 0,
        "missing_messages": 0,
        "duplicate_messages": 0,
        "not_acknowledged": 0,
    }
    if not peer_log_path.exists():
        return counts
    raw = peer_log_path.read_bytes().decode("utf-8", "replace")
    for line in raw.splitlines():
        if "Streaming: Received seqn=0" in line:
            counts["syn_ack_received"] += 1
        elif "Streaming: Process seqn=" in line and "flags=0" in line:
            counts["data_packets_processed"] += 1
        elif "Streaming: Packet" in line and "acknowledged" in line:
            counts["packets_acknowledged"] += 1
        elif "Streaming: Missing messages" in line:
            counts["missing_messages"] += 1
        elif "Streaming: Duplicate message" in line:
            counts["duplicate_messages"] += 1
        elif "was not ACKed" in line:
            counts["not_acknowledged"] += 1
    return counts


def read_data_phase_stall_receipts(stdout_path: Path, offset: int = 0) -> list:
    """Read the router's four-stage DATA-phase stall attribution receipts.

    The receipt is the router's own classification of a reverse DATA
    stall: it separates "the DATA never arrived" from "the DATA
    arrived in the wrong manager" from "the DATA arrived and produced
    no acknowledgement" from "the acknowledgement never left the
    router". Each stage is a count; no identity, key, tunnel id, or
    payload material is read.

    A lane that fails on a data phase and finds no receipt here has
    no classification at all, so the failure message says so rather
    than leaving the reader to guess.
    """
    receipts = []
    for line in read_receipt_lines(stdout_path, offset):
        if 'transition="stream_accept_data_phase_stalled"' not in line:
            continue
        receipt = {"raw": line[:400]}
        for field in (
            "app_bytes_drained",
            "mirror_observed",
            "mirror_accepted",
            "mirror_rejected",
            "mirror_payload_bytes",
            "mirror_arrived",
            "canonical_observed",
            "canonical_accepted",
            "canonical_rejected",
            "canonical_payload_bytes",
            "canonical_arrived",
            "mirror_ack_generated",
            "mirror_ack_transmitted",
            "mirror_ack_no_ack_emitted",
            "canonical_ack_generated",
            "canonical_ack_transmitted",
            "ack_egress_sent",
            "ack_egress_dropped",
        ):
            match = re.search(rf"{field}=(\d+)", line)
            receipt[field] = int(match.group(1)) if match else None
        receipts.append(receipt)
    return receipts


def classify_data_phase_stall(receipts: list) -> str:
    """Classify one four-stage stall receipt into a named cause.

    The four causes are the ones the router's own attribution
    separates. The classification is a reading of the router's
    counters, never an inference from anything else.
    """
    if not receipts:
        return "no-four-stage-receipt"
    receipt = receipts[-1]
    mirror_arrived = receipt.get("mirror_arrived") or 0
    canonical_arrived = receipt.get("canonical_arrived") or 0
    mirror_ack = receipt.get("mirror_ack_generated") or 0
    ack_sent = receipt.get("ack_egress_sent") or 0
    if mirror_arrived == 0 and canonical_arrived == 0:
        return "data-never-arrived"
    if mirror_arrived == 0 and canonical_arrived > 0:
        return "data-arrived-in-the-wrong-manager"
    if mirror_ack == 0:
        return "data-arrived-without-an-acknowledgement"
    if ack_sent == 0:
        return "acknowledgement-never-left-the-router"
    return "attributed-without-a-named-cause"


def read_service_failures(stdout_path: Path, offset: int = 0) -> dict:
    """Read the runtime's service-failure receipts from `offset`.

    Counts only, no service internals. The Plan 487 WP4 live
    qualification showed the lease-rotation path failing the whole
    `ssu2-router` service: one destination's lease refresh was
    rejected, the error propagated out of the group advance, and the
    router's transport went down mid-stream. A lease-set failure is
    destination-scoped now, so the lane asserts the service never
    failed.
    """
    failures = {}
    for line in read_receipt_lines(stdout_path, offset):
        if 'event="runtime.service.failed"' not in line:
            continue
        match = re.search(r"service=([A-Za-z0-9_-]+)", line)
        if match is None:
            continue
        name = match.group(1)
        failures[name] = failures.get(name, 0) + 1
    return failures


def exchange(
    connect_sam,
    accept_sam,
    payload: bytes,
    *,
    armed=None,
    stdout_path=None,
    peer_log_path=None,
) -> str:
    """One request/response round trip on an already-open SAM stream.

    The accept side echoes the received bytes back, so the round trip
    proves both directions of the open stream.

    A failure names the endpoint that lost the bytes. The accept side
    is the router's inbound stream and the connect side is the stock
    peer's outbound stream; the two failures mean different things, and
    an unnamed "short stream read" hides that difference.

    A failure also carries the router's own four-stage DATA-phase
    attribution when the router emitted one: the stall's cause is the
    router's classification, not the harness's guess, and a data-phase
    failure with no attribution says so instead of implying that the
    router said nothing.
    """
    def attributed(message):
        if stdout_path is None:
            return StreamExchangeError(message)
        receipts = read_data_phase_stall_receipts(stdout_path)
        peer_counts = (
            read_peer_streaming_counts(peer_log_path)
            if peer_log_path is not None
            else {}
        )
        if not receipts:
            return StreamExchangeError(
                f"{message}; the router emitted no four-stage DATA-phase "
                f"attribution for this stall; peer streaming counts: {peer_counts}"
            )
        return StreamExchangeError(
            f"{message}; four-stage attribution: "
            f"{classify_data_phase_stall(receipts)}; "
            f"peer streaming counts: {peer_counts}"
        )

    if armed is None:
        # The post-rotation read is armed here, before the connect
        # side sends, for the same reason the first read is armed at
        # the accept.
        armed = AcceptSideRead(accept_sam, len(payload))

    connect_sam.sock.sendall(payload)
    received, error = armed.wait(EXCHANGE_BUDGET_S)
    if error is not None:
        raise attributed(f"the accept side lost the stream: {error}")
    if received is None:
        raise attributed(
            "the accept side did not deliver the payload inside the bounded window"
        )
    if received != payload:
        raise StreamExchangeError("accepted stream payload differs from request")
    accept_sam.sock.sendall(received)
    try:
        echoed = connect_sam.exact(len(payload))
    except ConnectionError as exc:
        raise attributed(f"the connect side lost the stream: {exc}") from exc
    except socket.timeout as exc:
        raise attributed(f"the connect side timed out: {exc}") from exc
    if echoed != payload:
        raise StreamExchangeError("stream echo differs from exact request bytes")
    return _digest(echoed)


class AcceptSideRead:
    """One accept-side payload read, armed before the peer can deliver.

    The accept socket is **one bidirectional channel** that the
    router's raw driver also reads: the driver takes the
    application's writes from it, and the application takes the peer's
    bytes from it. An application that posts its read *late* can find
    the channel already drained by the driver, and the payload then
    never reaches the application at all.

    So the read is armed as a separate thread the moment the accept
    socket is bound — before the peer can deliver anything — and the
    connect side sends afterwards. The reference driver reads the
    accept side in its worker thread for the same reason.
    """

    def __init__(self, sam, count: int):
        self._sam = sam
        self._count = count
        self._received = None
        self._error = None
        self._done = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def _run(self):
        try:
            self._received = self._sam.exact(self._count)
        except Exception as exc:  # noqa: BLE001 - reported to the caller
            self._error = exc
        finally:
            self._done.set()

    def wait(self, timeout_s: float):
        """Return `(received, error)`; `received` is `None` on timeout."""
        if not self._done.wait(timeout_s):
            return None, None
        return self._received, self._error


def open_reverse_stream(i2pr_sam, i2pd_sam, destination, timeout, accept_session, payload):
    """Open a stock-peer → i2pr reverse stream and arm its accept read.

    The accept side blocks until the peer connects, so it runs on its
    own thread while the connect side is opened on the caller's thread.
    The order is safe: the i2pr side holds an inbound stream that
    arrives before its ACCEPT waiter (the parked-accept seam), so the
    SYN is not lost when the accept lands a moment later.

    The accept worker also **reads the payload itself**, in the worker
    thread, as soon as the socket is bound. The accept socket is one
    bidirectional channel that the router's raw driver also reads — the
    driver takes the application's writes from it, and the application
    takes the peer's bytes from it. An application that posts its read
    *late* can find the channel already drained by the driver, and the
    payload then never reaches the application at all. Reading in the
    worker thread posts the application's read before the peer's bytes
    can arrive; the reference driver reads the accept side in its
    worker thread for the same reason.
    """
    accepted = {}
    failure = []

    def accept_worker():
        try:
            sam = samdrv.open_stream(
                i2pr_sam, f"STREAM ACCEPT ID={accept_session}", timeout, True
            )
            accepted["sam"] = sam
            # Arm the application's read now, before the peer can
            # deliver: a read posted later can lose the payload to the
            # router's raw driver.
            accepted["read"] = AcceptSideRead(sam, len(payload))
        except Exception as exc:  # noqa: BLE001 - reported to the caller
            failure.append(exc)

    worker = threading.Thread(target=accept_worker, daemon=True)
    worker.start()
    connect_sam = samdrv.open_stream(
        i2pd_sam,
        f"STREAM CONNECT ID=p487peer DESTINATION={destination}",
        timeout,
    )
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if "read" in accepted:
            return connect_sam, accepted["sam"], accepted["read"]
        if failure:
            connect_sam.close()
            raise failure[0]
        time.sleep(0.01)
    connect_sam.close()
    raise RuntimeError("the accept side did not bind the reverse stream")


def read_build_retry_receipts(stdout_path: Path, offset: int = 0) -> int:
    """Count the daemon's current-accounted build retry receipts.

    A build attempt that had to be retried carries a `bootstrap_retry` or
    `inbound_retry` transition. The count is the receipt the Plan 478 §6
    gate asks for: the build retry must be *visible*, not merely
    accounted for internally. A quiet mesh produces no retry, so a zero
    count is a recorded outcome, not a failure.
    """
    retries = 0
    for line in read_receipt_lines(stdout_path, offset):
        if (
            'transition="bootstrap_retry"' in line
            or 'transition="inbound_retry"' in line
        ):
            retries += 1
    return retries


class ControlSocketHeartbeat:
    """Keep a SAM session's control socket alive across a long wait.

    Plan 485 bounds a session's life by the control socket's 60 s
    command timeout: a control socket that sends no command for 60 s
    is treated as gone, and the session — with every stream attached
    to it — is torn down with it. A client that holds a long-lived
    stream must therefore keep its control socket busy, which is what
    the SAM convention already asks of a client that holds a session.

    The heartbeat sends one harmless `NAMING LOOKUP NAME=ME` per
    interval and reads its reply on the same socket, so the command
    stream stays in order. A failed heartbeat is counted, never
    hidden.
    """

    def __init__(self, sam, interval_s: float = 20.0):
        self.sam = sam
        self.interval_s = interval_s
        self.failures = 0
        self.sent = 0
        self._stop = threading.Event()
        self._thread = None

    def start(self):
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()
        return self

    def _run(self):
        while not self._stop.wait(self.interval_s):
            try:
                self.sam.send_line("NAMING LOOKUP NAME=ME")
                self.sam.line()
                self.sent += 1
            except Exception:  # noqa: BLE001 - counted, never hidden
                self.failures += 1

    def stop(self):
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5)
        return {"heartbeats_sent": self.sent, "heartbeat_failures": self.failures}


def read_publication_series(stdout_path: Path, offset: int = 0):
    """The publication series: `(published_seconds, oldest_lease_expiry_ms)`.

    One entry per LeaseSet2 publication the router emitted, in order.
    The values are the router's own sanitised counters; no identity,
    key, tunnel id, or payload material is read.
    """
    published = []
    expiry = []
    for line in read_receipt_lines(stdout_path, offset):
        if 'transition="ls2_publication_awaiting_acknowledgement"' not in line:
            continue
        published_match = re.search(r"published_seconds=(\d+)", line)
        expiry_match = re.search(r"oldest_lease_expiry_ms=(\d+)", line)
        if published_match:
            published.append(int(published_match.group(1)))
        if expiry_match:
            expiry.append(int(expiry_match.group(1)))
    return published, expiry


def wait_for_lease_rotation(
    stdout_path: Path,
    baseline_count: int,
    budget_s: float = ROTATION_BUDGET_S,
) -> dict:
    """Wait boundedly for the destination's lease rotation.

    A rotation republishes the LeaseSet2 with a `published` timestamp
    at least one second later than the previous publication and an
    advanced oldest-lease expiry. Without both advances the floodfill
    silently keeps the older record, so the rotation is the proof
    that the published record is the live one.

    `baseline_count` is the number of publications the router had
    already emitted before the session under test existed, so only the
    session's **own** publications are compared: a rotation is a second
    publication of the same destination, not a second destination.

    The router's own receipts are the proof. The wait is bounded: the
    absence of a rotation inside the budget is a recorded outcome, not
    a reason to wait longer.
    """
    deadline = time.monotonic() + budget_s
    while True:
        published, expiry = read_publication_series(stdout_path)
        own_published = published[baseline_count:]
        own_expiry = expiry[baseline_count:]
        rotated = (
            len(own_published) >= 2
            and own_published[-1] > own_published[0]
            and len(own_expiry) >= 2
            and own_expiry[-1] > own_expiry[0]
        )
        if rotated or time.monotonic() >= deadline:
            receipt = read_publication_receipts(stdout_path)
            return {
                "rotated": rotated,
                "publications_for_the_destination": len(own_published),
                "published_seconds": own_published[-1] if own_published else 0,
                "oldest_lease_expiry_ms": own_expiry[-1] if own_expiry else 0,
                "published_delta_seconds": (
                    own_published[-1] - own_published[0] if own_published else 0
                ),
                "oldest_lease_expiry_delta_ms": (
                    own_expiry[-1] - own_expiry[0] if own_expiry else 0
                ),
                "publications_acknowledged": receipt["publications_acknowledged"],
                "acknowledged_on_inbound_tunnel": receipt[
                    "acknowledged_on_inbound_tunnel"
                ],
            }
        time.sleep(2.0)


def read_publication_receipts(stdout_path: Path, offset: int = 0) -> dict:
    """Read the daemon's publication receipts.

    Counts only. No identity, key, tunnel id, or payload material is
    retained; the receipt fields are the daemon's own sanitised
    counters. The receipts are read line by line so a field from an
    unrelated receipt can never be counted as a publication field.
    """
    empty = {
        "publications_awaiting_acknowledgement": 0,
        "publications_acknowledged": 0,
        "acknowledged_on_inbound_tunnel": 0,
        "published_deltas_seconds": [],
        "oldest_lease_expiry_deltas_ms": [],
    }
    published = []
    expiry = []
    acknowledged = 0
    acknowledged_on_inbound = 0
    for line in read_receipt_lines(stdout_path, offset):
        if 'transition="ls2_publication_awaiting_acknowledgement"' in line:
            published_match = re.search(r"published_seconds=(\d+)", line)
            expiry_match = re.search(r"oldest_lease_expiry_ms=(\d+)", line)
            if published_match:
                published.append(int(published_match.group(1)))
            if expiry_match:
                expiry.append(int(expiry_match.group(1)))
        elif 'transition="ls2_publication_acknowledged"' in line:
            acknowledged += 1
            tunnel_match = re.search(r"inbound_tunnel_id=(\d+)", line)
            if tunnel_match and int(tunnel_match.group(1)) != 0:
                acknowledged_on_inbound += 1
    return {
        "publications_awaiting_acknowledgement": len(published),
        "publications_acknowledged": acknowledged,
        "acknowledged_on_inbound_tunnel": acknowledged_on_inbound,
        "published_deltas_seconds": [
            published[index] - published[index - 1]
            for index in range(1, len(published))
            if published[index] > published[index - 1]
        ],
        "oldest_lease_expiry_deltas_ms": [
            expiry[index] - expiry[index - 1]
            for index in range(1, len(expiry))
            if expiry[index] > expiry[index - 1]
        ],
    }


def read_publication_acknowledgement_count(stdout_path: Path, offset: int = 0) -> int:
    """Count the correlated LeaseSet2 acknowledgements in the receipts."""
    count = 0
    for line in read_receipt_lines(stdout_path, offset):
        if 'transition="ls2_publication_acknowledged"' in line:
            count += 1
    return count


def read_acknowledged_published_seconds(stdout_path: Path, offset: int = 0) -> list:
    """Read the `published_seconds` of every acknowledged publication.

    The acknowledgement receipt names the publication it answers, so
    the receipt set says *which* publication was acknowledged — not
    merely how many acknowledgements exist. Counts and timestamps
    only; no identity, key, tunnel id, or payload material.
    """
    acknowledged = []
    for line in read_receipt_lines(stdout_path, offset):
        if 'transition="ls2_publication_acknowledged"' not in line:
            continue
        match = re.search(r"published_seconds=(\d+)", line)
        if match:
            acknowledged.append(int(match.group(1)))
    return acknowledged


def wait_for_publication_acknowledgement(
    stdout_path: Path,
    offset: int = 0,
    baseline: int = 0,
    budget_s: float = PUBLICATION_READY_BUDGET_S,
    published_seconds: int | None = None,
) -> int:
    """Wait boundedly for a correlated LeaseSet2 acknowledgement.

    The acknowledgement is the router's own proof that a floodfill
    stored the published LeaseSet2 and answered on the destination's
    inbound tunnel. It is the precondition a stock peer needs before
    its LeaseSet2 lookup can succeed, so it is the readiness signal the
    post-restart reachability claim is measured on. The wait is
    bounded: absence after the budget is a recorded outcome, not a
    reason to wait longer.

    `baseline` is the acknowledgement count observed before the
    publication under test; the wait ends when a *new* acknowledgement
    arrives, so a stale acknowledgement cannot satisfy it.

    `published_seconds` names the publication the caller needs
    acknowledged. When it is given, the wait ends only when an
    acknowledgement *for that publication* has arrived: a count alone
    cannot tell the caller's publication from an unrelated one, and
    the caller's publication is often already counted when the caller
    starts waiting.
    """
    deadline = time.monotonic() + budget_s
    while True:
        count = read_publication_acknowledgement_count(stdout_path, offset)
        if published_seconds is None:
            if count > baseline:
                return count
        else:
            acknowledged = read_acknowledged_published_seconds(stdout_path, offset)
            if any(value >= published_seconds for value in acknowledged):
                return count
        if time.monotonic() >= deadline:
            return count
        time.sleep(0.5)


def reach_after_restart(stdout_path: Path, i2pr_sam, i2pd_sam, timeout, peer_log_path=None):
    """Reach the restarted router through a fresh session and handshake.

    Each attempt mints a new SAM session, a new ephemeral destination,
    and a new handshake with new stream and SYN identifiers. It is
    never a resend of the same bytes, so the bounded budget cannot mask
    a data-phase defect: a data-phase failure fails every attempt the
    same way.

    Between the session creation and the handshake the lane waits for
    the router's own correlated LeaseSet2 acknowledgement. That is the
    router's proof that a floodfill stored the lease, and it is the
    precondition a stock peer's LeaseSet2 lookup needs; without it the
    peer can resolve a destination the network has not stored yet.

    The budget covers the documented mesh churn — the controlled
    four-peer mesh rebuilds its own transit pools, and a stock peer can
    lose a destination build reply — and every attempt's outcome is
    recorded so the evidence shows what the recovery actually needed.
    """
    attempts = []
    for attempt in range(1, REACHABILITY_ATTEMPTS + 1):
        session_id = f"p487dest{attempt}"
        dest_sam = None
        connect_sam = None
        accept_sam = None
        try:
            baseline = read_publication_acknowledgement_count(stdout_path)
            dest_sam, destination = samdrv.create_session_with_recovery(
                i2pr_sam, session_id, timeout
            )
            acknowledged = wait_for_publication_acknowledgement(
                stdout_path, baseline=baseline, budget_s=PUBLICATION_READY_BUDGET_S
            )
            connect_sam, accept_sam, armed = open_reverse_stream(
                i2pr_sam, i2pd_sam, destination, timeout, session_id, PAYLOAD
            )
            digest = exchange(
                connect_sam,
                accept_sam,
                PAYLOAD,
                armed=armed,
                stdout_path=stdout_path,
                peer_log_path=Path(peer_log_path) if peer_log_path else None,
            )
        except Exception as exc:  # noqa: BLE001 - recorded per attempt
            attempts.append(
                {
                    "attempt": attempt,
                    "session": session_id,
                    "error": str(exc)[:200],
                }
            )
            for handle in (connect_sam, accept_sam, dest_sam):
                if handle is not None:
                    handle.close()
            continue
        attempts.append(
            {
                "attempt": attempt,
                "session": session_id,
                "error": None,
                "publication_acknowledgements": acknowledged,
            }
        )
        return dest_sam, connect_sam, accept_sam, digest, attempts
    raise RuntimeError(
        "the restarted router was not reachable again: "
        + json.dumps(attempts, sort_keys=True)
    )


class LifecycleLane(plan458.Lane):
    """The Plan 458 lane plus the §6 lifecycle phases."""

    def __init__(self):
        super().__init__()
        self.router_process = None
        self.router_config = None

    def run(self):
        # The Plan 458 qualification runs first and unchanged: both
        # directions, including the reverse external SAM STREAM DATA
        # phase, in this mesh.
        super().run()
        self.router_config = self.scratch / "i2pr.toml"
        self.router_process = self._find_router_process()
        # The lifecycle's result **is** the lane's evidence: the
        # rotation and the restart receipts, the digests, and the
        # reachability attempts. A lane that drops it produces no
        # evidence file and fails at the first write.
        return self.lifecycle()

    def _find_router_process(self):
        for process in reversed(self.children):
            if process.poll() is None:
                try:
                    cmdline = Path(f"/proc/{process.pid}/cmdline").read_bytes()
                except OSError:
                    continue
                # The stock peers live under the repository path, so the
                # repository name alone would match them too. The
                # router is the one whose argv names the built daemon
                # binary and the `run` subcommand.
                if b"target/debug/i2pr" in cmdline and b"run" in cmdline:
                    return process
        raise RuntimeError("the i2pr router process was not found")

    def _stop_router(self):
        process = self.router_process
        if process is None:
            return
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        # The stopped router leaves `self.children`; drop it so the
        # lane's cleanup does not wait on it again.
        if process in self.children:
            self.children.remove(process)

    def _start_router(self):
        stdout = (self.scratch / "i2pr.stdout").open("ab")
        stderr = (self.scratch / "i2pr.stderr").open("ab")
        process = subprocess.Popen(
            [str(ROOT / "target/debug/i2pr"), "run", "--config", str(self.router_config)],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=stdout,
            stderr=stderr,
            start_new_session=True,
        )
        stdout.close()
        stderr.close()
        self.children.append(process)
        self.router_process = process
        endpoint = ("127.0.0.1", self.ports["i2pr_sam"])
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("the restarted i2pr run exited before SAM readiness")
            try:
                with socket.create_connection(endpoint, timeout=0.25):
                    return process
            except OSError:
                time.sleep(0.25)
        raise RuntimeError("the restarted i2pr run did not bind SAM")

    def _wait_for_mesh_settle(self, budget_s: float = 150.0, window_s: float = 10.0):
        """Wait until the stock mesh around the restarted router settles.

        A restarted router rebuilds its tunnel pools from scratch, and
        the stock peers' transit inventories move while they absorb the
        new tunnels. The reachability claim is measured on a settled
        mesh, not on a router that is still rebuilding its first pools.

        The gate reads only the stock peers' read-only transit row
        counts over loopback HTTP — no log text is retained. It is
        bounded and it is never fatal: a mesh that never settles is a
        recorded outcome, and the reachability attempts that follow
        carry their own bounded budget and their own per-attempt record.
        """
        previous = {
            name: self.stock_transit_count(name) for name in ("f", "c", "n", "d")
        }
        deadline = time.monotonic() + budget_s
        while True:
            time.sleep(window_s)
            current = {
                name: self.stock_transit_count(name) for name in ("f", "c", "n", "d")
            }
            deltas = []
            for name, after in current.items():
                before = previous.get(name)
                if before is not None and after is not None:
                    deltas.append(abs(after - before))
            previous = current
            if deltas and sum(deltas) <= 4:
                return {"settled": True, "window_s": window_s}
            if time.monotonic() >= deadline:
                return {"settled": False, "window_s": window_s}

    def _persisted_identity_digests(self):
        """The digests of the router's persisted identity material.

        The router identity and the transport static key are written
        under the data directory. They are the material that makes the
        router's hash stable across a restart, so they are the files the
        restart claim is measured on.
        """
        digests = {}
        for name in ("router.identity", "ssu2.transport.identity"):
            path = self.scratch / "i2pr-data" / name
            digests[name] = _digest(path.read_bytes()) if path.is_file() else None
        return digests

    def _export_router_hash(self, name):
        """Re-export the router's RouterInfo and return its router hash.

        The export reads the persisted identity, so a router that came
        back with different material produces a different hash. The
        RouterInfo *bytes* are not comparable across two exports: they
        carry a fresh publication timestamp by design, so only the
        hash is.
        """
        target = self.scratch / f"i2pr-router-{name}.info"
        completed = subprocess.run(
            [
                "cargo",
                "run",
                "--locked",
                "-p",
                "i2pr-daemon",
                "--example",
                "plan458_export_router_info",
                "--",
                str(self.scratch / "i2pr-data"),
                str(self.ports["i2pr_ssu2"]),
                str(target),
            ],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        for line in completed.stdout.splitlines():
            if line.startswith("router_hash="):
                return line.split("=", 1)[1].strip()
        return None

    def lifecycle(self):
        timeout = REACHABILITY_BUDGET_S
        # The driver's socket helper takes a "host:port" endpoint.
        i2pr_sam = f"127.0.0.1:{self.ports['i2pr_sam']}"
        i2pd_sam = f"127.0.0.1:{self.ports['f_sam']}"
        stdout_path = self.scratch / "i2pr.stdout"

        # The publication baseline is taken before the session under test
        # exists, so the rotation that follows can only be this
        # session's own republication.
        publication_baseline = len(read_publication_series(stdout_path)[0])
        dest_sam, destination = samdrv.create_session_with_recovery(
            i2pr_sam, "p487dest", timeout
        )
        peer_sam, _peer_destination = samdrv.create_session_with_recovery(
            i2pd_sam, "p487peer", timeout, crypto_type=4
        )
        connect_sam = None
        accept_sam = None
        post_connect = None
        post_accept = None
        post_sam = None
        result = {
            "schema": "plan487-lifecycle-recovery-v1",
            "daemon_restart": {},
            "lease_rotation": {},
        }
        try:
            connect_sam, accept_sam, armed = open_reverse_stream(
                i2pr_sam, i2pd_sam, destination, timeout, "p487dest", PAYLOAD
            )
            # The round trips are bounded separately from the session
            # budget: a data phase that stalls is classified by the
            # router within its own window, so the lane must not wait
            # out a stall the router has already attributed.
            connect_sam.sock.settimeout(EXCHANGE_BUDGET_S)
            accept_sam.sock.settimeout(EXCHANGE_BUDGET_S)
            # The stream is live before either phase starts.
            pre_digest = exchange(
                connect_sam,
                accept_sam,
                PAYLOAD,
                armed=armed,
                stdout_path=stdout_path,
                peer_log_path=self.scratch / "logs" / "f.log",
            )

            # ---- Phase 1: lease rotation (ADR 0040 case 1) -----------
            # A destination's tunnel pool is replaced as its leases
            # approach expiry: the router builds a replacement inbound
            # tunnel and republishes the LeaseSet2. ADR 0040 case 1
            # says the stream **survives** that, because a stream is
            # bound to a destination and never to a tunnel, and the
            # client-observable result is **nothing**.
            #
            # The rotation is also WP4's own required proof: it
            # republishes with a `published` timestamp at least one
            # second later and an advanced oldest-lease expiry, so the
            # floodfill replaces the record instead of silently keeping
            # the older one.
            #
            # The wait is long (the lease rotation margin is 120 s and
            # the lease lives about 540 s), so the session's control
            # socket is kept busy: Plan 485 bounds a session by the
            # control socket's 60 s command timeout, and an idle
            # control socket would tear the session — and this live
            # stream — down mid-wait.
            # The heartbeat stays alive across the whole rotation
            # phase, including the acknowledgement wait and the
            # post-rotation round trip. Plan 485 bounds a session by
            # its control socket: stopping the heartbeat closes the
            # control socket, the router tears the session down, and
            # the live stream dies with it — so a lane that stops the
            # heartbeat before the post-rotation round trip measures
            # its own teardown instead of the rotation.
            heartbeat = ControlSocketHeartbeat(dest_sam).start()
            try:
                rotation = wait_for_lease_rotation(stdout_path, publication_baseline)
                if not rotation["rotated"]:
                    raise RuntimeError(
                        "the destination's lease did not rotate inside the bounded wait"
                    )
                # WP4's own proof for the rotation: the republished
                # record is acknowledged by a correlated DeliveryStatus
                # on the destination's inbound tunnel, which is the
                # router's proof that a floodfill stored it. Without
                # the acknowledgement the rotation is a local act with
                # no network effect. The wait names the rotated
                # publication, so an acknowledgement of an earlier
                # publication cannot satisfy it.
                acknowledged = wait_for_publication_acknowledgement(
                    stdout_path,
                    baseline=rotation["publications_acknowledged"],
                    published_seconds=rotation["published_seconds"],
                    budget_s=PUBLICATION_READY_BUDGET_S,
                )
                rotation_digest = exchange(
                    connect_sam,
                    accept_sam,
                    PAYLOAD,
                    stdout_path=stdout_path,
                    peer_log_path=self.scratch / "logs" / "f.log",
                )
            finally:
                heartbeat_receipt = heartbeat.stop()
            result["lease_rotation"] = {
                "rotated": rotation["rotated"],
                "published_delta_seconds": rotation["published_delta_seconds"],
                "oldest_lease_expiry_delta_ms": rotation[
                    "oldest_lease_expiry_delta_ms"
                ],
                "publication_acknowledgements": rotation[
                    "publications_acknowledged"
                ],
                "acknowledged_on_inbound_tunnel": rotation[
                    "acknowledged_on_inbound_tunnel"
                ],
                "rotation_acknowledgements": acknowledged,
                "rotated_published_seconds": rotation["published_seconds"],
                "rotated_oldest_lease_expiry_ms": rotation["oldest_lease_expiry_ms"],
                "control_socket_heartbeat": heartbeat_receipt,
                "payload_bytes": len(PAYLOAD),
                "pre_rotation_sha256": pre_digest,
                "post_rotation_sha256": rotation_digest,
                "stream_survived": (
                    rotation_digest == pre_digest == _digest(PAYLOAD)
                ),
            }
            acknowledged_published = read_acknowledged_published_seconds(stdout_path)
            if not any(
                value >= rotation["published_seconds"] for value in acknowledged_published
            ):
                raise RuntimeError(
                    "the rotated LeaseSet2 was not acknowledged by a correlated "
                    "DeliveryStatus on an inbound tunnel"
                )
            if not result["lease_rotation"]["stream_survived"]:
                raise RuntimeError(
                    "the stream did not survive the destination's lease rotation"
                )
            # Plan 487 WP4: the rotation must not fail the router's
            # transport. A lease-set failure is destination-scoped; the
            # service stays up and the destination recovers on the next
            # tick.
            failures_after_rotation = read_service_failures(stdout_path)
            result["lease_rotation"]["service_failures"] = failures_after_rotation
            if failures_after_rotation:
                raise RuntimeError(
                    "the router's services failed during the lease rotation: "
                    f"{failures_after_rotation}"
                )

            # ---- Phase 2: daemon restart (ADR 0040 case 2) -----------
            # The restart comes last, on a healthy mesh: a dead peer
            # would change the stock client's floodfill selection for a
            # LeaseSet2 lookup, and the post-restart reachability claim
            # must be measured on the mesh's normal behaviour rather
            # than on a lookup that is known to have been misrouted.
            identity_before = self._persisted_identity_digests()
            retries_before = read_build_retry_receipts(stdout_path)
            restart_offset = stdout_path.stat().st_size
            self._stop_router()
            # The SAM connection closes. The accept side is the i2pr
            # socket, so the read must observe EOF.
            closed = False
            try:
                accept_sam.exact(1)
            except ConnectionError:
                closed = True
            except OSError:
                closed = True
            if not closed:
                raise RuntimeError(
                    "the SAM connection did not close when the daemon stopped"
                )
            self._start_router()
            # The restarted router rebuilds its tunnel pools from
            # scratch. The reachability claim is measured on a settled
            # mesh, not on a router that is still absorbing its first
            # rebuilt pools.
            settle = self._wait_for_mesh_settle()
            # The build retry must be *visible* in the receipts. The
            # count is recorded evidence: the mechanism was qualified
            # live in the Plan 478 lineage (`bootstrap_retry`,
            # `inbound_retry`), and this run must keep it visible.
            retry_receipts = wait_for_build_retry(
                stdout_path, offset=restart_offset, budget_s=90.0
            )
            identity_after = self._persisted_identity_digests()
            hash_before = self._export_router_hash("before")
            hash_after = self._export_router_hash("after")
            # A fresh session reaches a fresh destination; the
            # destination must be reachable again after the restart.
            (
                post_sam,
                post_connect,
                post_accept,
                restart_digest,
                reach_attempts,
            ) = reach_after_restart(
                stdout_path,
                i2pr_sam,
                i2pd_sam,
                timeout,
                self.scratch / "logs" / "f.log",
            )
            result["daemon_restart"] = {
                "sam_connection_closed": True,
                "persisted_identity_unchanged": identity_before == identity_after,
                "router_hash_unchanged": (
                    hash_before is not None and hash_before == hash_after
                ),
                "destination_reachable_after_restart": True,
                "post_restart_sha256": restart_digest,
                "payload_bytes": len(PAYLOAD),
                "build_retry_receipts_before_restart": retries_before,
                "build_retry_receipts_after_restart": retry_receipts,
                "mesh_settled_after_restart": settle,
                "reachability_attempts": reach_attempts,
            }
            if not result["daemon_restart"]["persisted_identity_unchanged"]:
                raise RuntimeError(
                    "the persisted router identity changed across the daemon restart"
                )
            if not result["daemon_restart"]["router_hash_unchanged"]:
                raise RuntimeError(
                    "the router's hash changed across the daemon restart"
                )
            if restart_digest != _digest(PAYLOAD):
                raise RuntimeError(
                    "the destination was not reachable again after the daemon restart"
                )

            # The pre-restart stream's payload is the same bytes, so the
            # two phases are measured on one payload and one digest.
            result["daemon_restart"]["pre_restart_sha256"] = pre_digest
        finally:
            for handle in (
                connect_sam,
                accept_sam,
                dest_sam,
                peer_sam,
                post_sam,
                post_connect,
                post_accept,
            ):
                if handle is not None:
                    handle.close()
        result["publication_receipts"] = read_publication_receipts(stdout_path)
        result["publication_receipts_after_restart"] = read_publication_receipts(
            stdout_path, restart_offset
        )
        # Plan 487 WP4: the whole lane must leave the router's services
        # up. A lease-set failure is destination-scoped; it degrades
        # that destination and is retried on the next tick.
        result["service_failures"] = read_service_failures(stdout_path)
        if result["service_failures"]:
            raise RuntimeError(
                "the router's services failed during the lane: "
                f"{result['service_failures']}"
            )
        return result


def wait_for_build_retry(stdout_path: Path, offset: int = 0, budget_s: float = 60.0) -> int:
    """Wait boundedly for a current-accounted build retry receipt.

    The lost peer is a transit candidate, so the router's own build
    churn eventually meets the loss. The wait is bounded: absence after
    the budget is a recorded outcome, not a reason to wait longer.
    """
    deadline = time.monotonic() + budget_s
    while True:
        retries = read_build_retry_receipts(stdout_path, offset)
        if retries > 0 or time.monotonic() >= deadline:
            return retries
        time.sleep(1.0)


def read_plan458_directions():
    """The Plan 458 stage's exchange evidence, if the driver wrote it.

    The lifecycle lane runs the Plan 458 qualification unchanged in the
    same mesh, so the reverse DATA phase is proven in the same
    qualification as the §6 recovery path. The directions are copied
    into the lifecycle evidence so one file carries both claims.
    """
    path = plan458.EVIDENCE / "result.json"
    try:
        document = json.loads(path.read_text())
    except (OSError, ValueError):
        return {}
    directions = document.get("directions")
    if not isinstance(directions, dict):
        return {}
    return {
        label: {
            "length": row.get("length"),
            "sha256": row.get("sha256"),
        }
        for label, row in directions.items()
        if isinstance(row, dict)
    }


def main():
    if EVIDENCE.exists():
        shutil.rmtree(EVIDENCE, ignore_errors=True)
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    # The Plan 458 stage's own evidence must belong to this run: a
    # stale directions file must never be read as this qualification's.
    plan458.clean_lane_evidence()
    lane = LifecycleLane()
    try:
        result = lane.run()
        result["stock_build_receipt"] = lane.stock_build_receipt()
        result["plan458_directions"] = read_plan458_directions()
        (EVIDENCE / "result.json").write_text(json.dumps(result, sort_keys=True) + "\n")
        print(
            "Plan 487 §6 lifecycle recovery qualified: "
            + json.dumps(
                {
                    "lease_rotation": result["lease_rotation"],
                    "daemon_restart": result["daemon_restart"],
                    "publication_receipts": result["publication_receipts"],
                },
                sort_keys=True,
            )
        )
    except Exception as exc:
        receipt = read_publication_receipts(lane.scratch / "i2pr.stdout")
        (EVIDENCE / "attempt-diagnostic.json").write_text(
            json.dumps(
                {
                    "schema": "plan487-lifecycle-recovery-v1",
                    "failure": str(exc),
                    "failure_type": type(exc).__name__,
                    "traceback": traceback.format_exc()[-4000:],
                    "publication_receipts": receipt,
                    "build_retry_receipts": read_build_retry_receipts(
                        lane.scratch / "i2pr.stdout"
                    ),
                    "data_phase_stall_receipts": read_data_phase_stall_receipts(
                        lane.scratch / "i2pr.stdout"
                    ),
                    "data_phase_stall_classification": classify_data_phase_stall(
                        read_data_phase_stall_receipts(lane.scratch / "i2pr.stdout")
                    ),
                    "peer_streaming_counts": read_peer_streaming_counts(
                        lane.scratch / "logs" / "f.log"
                    ),
                    "mesh_quiet": lane.mesh_quiet,
                },
                sort_keys=True,
            )
            + "\n"
        )
        print(f"Plan 487 §6 lifecycle recovery failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
    finally:
        lane.stop_references()
        keep = os.environ.get("I2PR_PLAN487_KEEP_SCRATCH") == "1"
        if keep:
            print(f"Plan 487 scratch preserved at {lane.scratch}", file=sys.stderr)
        else:
            shutil.rmtree(lane.scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
