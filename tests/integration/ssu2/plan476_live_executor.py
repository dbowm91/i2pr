#!/usr/bin/env python3
"""Plan 476 guarded per-host process/session receipt executor.

This is the Tier A half of Plan 476: a *per-host* executor. It runs on ONE
host, starts ONLY the declared local router process, collects an attributable
sanitized receipt from that host, and emits one sanitized host receipt
artifact. A separate ``reconcile`` step joins two owner-exchanged host
receipts into the Plan 495 lane evidence.

The executor is a gate before it is a runner. Every precondition is checked
*before* a process starts, and every one of them refuses closed:

* the Plan 449 host inventory (owner consent, pinned reference, RFC1918 pair,
  the same document on both hosts) and the Plan 476 authorization contract;
* the Plan 449 local-host check (the declared interface, the route-selected
  source address, the bind) and the exact pinned i2pd 2.61.0 source and
  binary, so the lane can never run an unpinned peer;
* the Plan 495 peer-eligibility gate -- the peer must be able to admit our
  RouterInfo and its own selection state must be able to choose us;
* the Plan 495 deliberate-withholding control -- if withholding the peer's
  selection capability does not yield ``peer-selection-refused`` with the
  cause ``intentional-withholding``, this lane cannot tell "the peer would not
  choose us" from "our implementation is broken", so no result from it may be
  cited and the executor refuses to start anything;
* the Plan 495 peer-state hygiene protocol.

Nothing here reaches the public network, resolves a peer address, fetches a
reseed, opens a listener, or retains a router log. The executor writes two
kinds of artifact, both mode 0600 and both count-only:

* ``i2pr-plan476-host-receipt-v1`` -- one host's sanitized observation;
* ``i2pr-plan495-lane-evidence-v1`` -- the joined lane evidence.

Failures print exactly one sanitized JSON line and exit non-zero. A refusal
is never a pass, and this executor never reports the reviewed human judgement
about our own implementation: that classification is a decision a person
makes, not a reading of a ladder.

The process, host-probe, receipt-collection and preflight seams are adapters
so the whole state machine can be exercised offline with fakes that start no
process and open no socket.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import signal
import stat
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Sequence


ROOT = Path(__file__).resolve().parents[3]
CONTRACT_PATH = Path(__file__).with_name("plan476_executor.py")
GATE_PATH = ROOT / "scripts/plan495-peer-eligibility.py"
PREFLIGHT_PATH = ROOT / "scripts/plan449-ssu2-preflight.py"
RUST_PHASES_PATH = ROOT / "crates/i2pr-daemon/src/ssu2_qualification_receipts.rs"
FIXTURE_DIR = Path(__file__).with_name("plan495-eligibility-fixtures")

RESULT_SCHEMA = "i2pr-plan476-executor-result-v1"
HOST_RECEIPT_SCHEMA = "i2pr-plan476-host-receipt-v1"
LOCAL_RECEIPT_SCHEMA = "i2pr-plan476-local-receipt-v1"
EVIDENCE_SCHEMA = "i2pr-plan495-lane-evidence-v1"
HYGIENE_SCHEMA = "i2pr-plan495-peer-state-v1"

ROLES = ("i2pr", "i2pd")
NETWORK_ID = "2"
LIVE_QUALIFICATION = "not-run"

#: The canonical local phase vocabulary is not restated here: it is taken
#: from the closed-plan contract module by ``local_phases()`` so the two
#: copies cannot drift.
EPOCH_PATTERN = re.compile(r"[0-9a-f]{32}\Z")
HEX64_PATTERN = re.compile(r"[0-9a-f]{64}\Z")

#: i2pd's own base32, verified against the pinned source
#: ``libi2pd/Base.cpp:261-290`` (``ByteStreamToBase32``): no padding, MSB
#: first, five bits per character. A 32-byte RouterHash encodes to 52
#: characters.
I2PD_BASE32_ALPHABET = "abcdefghijklmnopqrstuvwxyz234567"

#: The grace period between the polite stop and the forced stop.
STOP_GRACE_SECONDS = 5.0
#: How often the observation window re-checks the readiness predicate.
POLL_SECONDS = 0.25

#: The outcome classes the reconciled lane evidence may carry. The
#: reviewed human judgement about our own implementation is deliberately
#: not among them: an executor may not report a defect on its own.
LANE_OUTCOME_CLASSES = (
    "pass",
    "peer-admission-refused",
    "peer-selection-refused",
    "timeout",
)

#: The local phases that prove an authenticated session happened. The
#: daemon writes its receipt file on its first export tick, when the
#: receipt still holds only ``permit_checked``; the file's existence is
#: therefore not evidence that a session took place.
SESSION_PHASES = (
    "peer_session_accepted",
    "peer_session_initiated",
    "peer_authenticated",
)


class ExecutorFailure(Exception):
    """A sanitized, fail-closed refusal.

    ``outcome_class`` and ``cause`` follow the Plan 495 two-axis model: the
    outcome says *where* the run ended, the cause says *whose* artifact was
    responsible.
    """

    def __init__(
        self,
        code: str,
        outcome_class: str = "environment-unavailable",
        cause: str = "unattributed",
    ) -> None:
        super().__init__(code)
        self.code = code
        self.outcome_class = outcome_class
        self.cause = cause


# ---------------------------------------------------------------------------
# Module loading.
#
# The gate and the preflight live in ``scripts/`` under dashed filenames, so
# they are loaded by path. Each is loaded under its own name: the contract
# module is loaded as ``plan476_live_contract`` rather than
# ``plan476_executor`` so this module never clobbers another importer's
# ``sys.modules`` entry (a reloaded dataclass is a different class).
# ---------------------------------------------------------------------------


def _load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ExecutorFailure("module-unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def contract_module():
    """The closed-plan Plan 476 receipt contract."""
    return _load_module("plan476_live_contract", CONTRACT_PATH)


def gate_module():
    """The Plan 495 peer-eligibility gate checker."""
    return _load_module("plan495_peer_eligibility", GATE_PATH)


def preflight_module():
    """The Plan 449 host-inventory preflight."""
    return _load_module("plan449_preflight", PREFLIGHT_PATH)


def local_phases() -> tuple[str, ...]:
    """The canonical local phase vocabulary from the contract module."""
    return tuple(contract_module().LOCAL_PHASES)


def pinned_i2pd_source() -> Path:
    """The repository's conventional checkout of the pinned i2pd source.

    The pin is taken from the Plan 449 preflight rather than repeated
    here, so the checkout the executor looks for and the checkout the
    preflight verifies against cannot drift apart. When the checkout is
    absent the preflight refuses with its own sanitized code; the
    executor never guesses a different tree.
    """
    pin = preflight_module().PIN
    return ROOT / "target" / "interop" / "ssu2-sources" / f"i2pd-{pin}"


# ---------------------------------------------------------------------------
# Digests. Nothing that reaches an artifact or the console is a secret.
# ---------------------------------------------------------------------------


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_text(value: str) -> str:
    return sha256_hex(value.encode("utf-8"))


def digest_of_file(path: Path | None, code: str) -> str:
    """The digest of a required operator-supplied artifact."""
    if path is None:
        raise ExecutorFailure(code)
    try:
        return sha256_hex(path.read_bytes())
    except OSError:
        raise ExecutorFailure(code) from None


def i2pd_base32(data: bytes) -> str:
    """i2pd's base32 encoding of a RouterHash."""
    out: list[str] = []
    value = 0
    bits = 0
    for byte in data:
        value = (value << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            out.append(I2PD_BASE32_ALPHABET[(value >> bits) & 0x1F])
    if bits:
        out.append(I2PD_BASE32_ALPHABET[(value << (5 - bits)) & 0x1F])
    return "".join(out)


# ---------------------------------------------------------------------------
# Adapters: the seams that let the whole state machine run offline.
# ---------------------------------------------------------------------------


@dataclass
class ProcessHandle:
    """A started process (or, in the fake adapter, the record of one)."""

    process: object = None
    argv: tuple[str, ...] = ()


class ProcessAdapter:
    """Starts and stops the one declared local router process."""

    def spawn(self, argv: Sequence[str]) -> ProcessHandle:
        raise NotImplementedError

    def observe(
        self, handle: ProcessHandle, deadline: float, ready: Callable[[], bool]
    ) -> bool:
        """Watch the bounded observation window; ``ready`` is the predicate."""
        raise NotImplementedError

    def terminate(self, handle: ProcessHandle, grace_seconds: float = STOP_GRACE_SECONDS) -> None:
        raise NotImplementedError


class SubprocessAdapter(ProcessAdapter):
    """Starts exactly one declared local router process.

    ``start_new_session=True`` puts the child in its own process group, so
    the executor can stop the whole group without ever signalling a process
    it did not start. The router's own output is discarded: the executor
    retains no router log and no raw payload.
    """

    def spawn(self, argv: Sequence[str]) -> ProcessHandle:
        try:
            process = subprocess.Popen(
                list(argv),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
        except OSError:
            raise ExecutorFailure("router-start-failed") from None
        return ProcessHandle(process=process, argv=tuple(argv))

    def observe(
        self, handle: ProcessHandle, deadline: float, ready: Callable[[], bool]
    ) -> bool:
        process = handle.process
        while True:
            if ready():
                return True
            if process is not None and process.poll() is not None:
                return False
            if time.monotonic() >= deadline:
                return False
            time.sleep(POLL_SECONDS)

    def terminate(self, handle: ProcessHandle, grace_seconds: float = STOP_GRACE_SECONDS) -> None:
        process = handle.process
        if process is None:
            return
        if process.poll() is not None:
            process.wait()
            return
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
        except (OSError, ProcessLookupError):
            pass
        try:
            process.wait(timeout=grace_seconds)
            return
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGKILL)
        except (OSError, ProcessLookupError):
            pass
        try:
            process.wait(timeout=grace_seconds)
        except subprocess.TimeoutExpired:
            pass


class FakeProcessAdapter(ProcessAdapter):
    """Records what would have been started; starts nothing.

    This is the negative control for the "never start anything else" rule:
    a test asserts that the recorded spawn list is empty whenever a
    precondition refuses.
    """

    def __init__(self, ready: bool = True) -> None:
        self.spawned: list[tuple[str, ...]] = []
        self.terminated: list[tuple[str, ...]] = []
        self._ready = ready

    def spawn(self, argv: Sequence[str]) -> ProcessHandle:
        self.spawned.append(tuple(argv))
        return ProcessHandle(process=None, argv=tuple(argv))

    def observe(
        self, handle: ProcessHandle, deadline: float, ready: Callable[[], bool]
    ) -> bool:
        return bool(self._ready and ready())

    def terminate(self, handle: ProcessHandle, grace_seconds: float = STOP_GRACE_SECONDS) -> None:
        self.terminated.append(handle.argv)


class HostProbe:
    """Local, read-only observations about the host's own router state."""

    def data_directory_is_usable(self, path: Path) -> bool:
        raise NotImplementedError

    def stale_netdb_files(self, netdb: Path) -> int | None:
        """Count the stale ``*.dat`` peer profiles; ``None`` when unreadable."""
        raise NotImplementedError

    def endpoint_is_free(self, address: str, port: int) -> bool:
        raise NotImplementedError


class LocalHostProbe(HostProbe):
    """Reads the local filesystem and the local UDP bind state.

    The bind probe is the same read-only local operation the Plan 449
    preflight already performs: it selects a route and a source address in
    the local kernel. It sends no datagram.
    """

    def data_directory_is_usable(self, path: Path) -> bool:
        try:
            if not path.exists():
                # A data directory that does not exist yet is the freshest
                # state there is: it holds nothing stale.
                return True
            return path.is_dir() and os.access(path, os.R_OK | os.X_OK)
        except OSError:
            return False

    def stale_netdb_files(self, netdb: Path) -> int | None:
        total = 0
        try:
            buckets = list(netdb.glob("r*"))
        except OSError:
            return None
        for bucket in buckets:
            try:
                if not bucket.is_dir():
                    continue
                total += sum(1 for entry in bucket.glob("*.dat") if entry.is_file())
            except OSError:
                return None
        return total

    def endpoint_is_free(self, address: str, port: int) -> bool:
        import socket  # local import: the fake probe must not need it

        probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            probe.bind((address, port))
        except OSError as error:
            if getattr(error, "errno", None) in (98, 48):  # EADDRINUSE
                return False
            raise ExecutorFailure("local-endpoint-unavailable") from None
        finally:
            probe.close()
        return True


class FakeHostProbe(HostProbe):
    """A host probe with no socket and no subprocess API."""

    def __init__(
        self,
        data_directory_usable: bool = True,
        stale_files: int = 0,
        endpoint_free: bool = True,
    ) -> None:
        self.data_directory_usable = data_directory_usable
        self.stale_files = stale_files
        self.endpoint_free = endpoint_free

    def data_directory_is_usable(self, path: Path) -> bool:
        return self.data_directory_usable

    def stale_netdb_files(self, netdb: Path) -> int | None:
        return self.stale_files

    def endpoint_is_free(self, address: str, port: int) -> bool:
        return self.endpoint_free


class ReceiptCollector:
    """Reads the daemon's sanitized local receipt export."""

    def collect(self, path: Path) -> dict:
        raise NotImplementedError


class FileReceiptCollector(ReceiptCollector):
    """Reads the daemon's exported receipt document from disk."""

    def collect(self, path: Path) -> dict:
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            raise ExecutorFailure("local-receipt-unavailable") from None
        try:
            document = json.loads(text)
        except json.JSONDecodeError:
            raise ExecutorFailure("local-receipt-malformed") from None
        if not isinstance(document, dict):
            raise ExecutorFailure("local-receipt-malformed")
        return document


class FakeReceiptCollector(ReceiptCollector):
    """An in-process collector with no socket and no subprocess API.

    This is the analogue of ``plan476_executor.FakeReceiptAdapter``: it can
    hand the executor a receipt document, but it cannot connect to anything
    and it cannot start anything.
    """

    def __init__(self, document: dict) -> None:
        self.document = document
        self.paths: list[Path] = []

    def collect(self, path: Path) -> dict:
        self.paths.append(path)
        return self.document


class PreflightAdapter:
    """The Plan 449 host preflight, reached through a seam.

    The preflight binds a local socket and shells out to the pinned
    reference checkout, so the offline simulator has to be able to
    replace it: a simulator that binds a port or runs the pinned binary
    is not a simulator any more.
    """

    def check_local_host(self, local: dict, peer: dict, repo: Path) -> None:
        """Refuse a changed interface, route or bind before any socket."""
        raise NotImplementedError

    def verify_reference(self, source: Path, binary: Path) -> str:
        """Refuse a reference checkout or binary that is not the pinned one."""
        raise NotImplementedError


class LocalPreflightAdapter(PreflightAdapter):
    """The real Plan 449 preflight, called by path."""

    def check_local_host(self, local: dict, peer: dict, repo: Path) -> None:
        preflight = preflight_module()
        try:
            preflight.check_local_host(local, peer, repo)
        except preflight.PreflightError as error:
            raise ExecutorFailure(error.code) from None

    def verify_reference(self, source: Path, binary: Path) -> str:
        preflight = preflight_module()
        try:
            return preflight.verify_reference(source, binary)
        except preflight.PreflightError as error:
            raise ExecutorFailure(error.code) from None


class FakePreflightAdapter(PreflightAdapter):
    """Records what the preflight would have checked; touches nothing.

    This adapter has no socket and no subprocess API: it can neither
    bind a port nor run the pinned binary, so the offline simulator
    cannot make a live claim.
    """

    def __init__(self, refusal: str | None = None) -> None:
        self.local_checks: list[tuple[str, str]] = []
        self.verified: list[tuple[str, str]] = []
        self.refusal = refusal

    def check_local_host(self, local: dict, peer: dict, repo: Path) -> None:
        self.local_checks.append((str(local.get("role")), str(peer.get("role"))))
        if self.refusal:
            raise ExecutorFailure(self.refusal)

    def verify_reference(self, source: Path, binary: Path) -> str:
        self.verified.append((str(source), str(binary)))
        if self.refusal:
            raise ExecutorFailure(self.refusal)
        return "0" * 64


@dataclass
class ExecutorAdapters:
    """The seams the executor reaches through."""

    processes: ProcessAdapter = field(default_factory=SubprocessAdapter)
    receipts: ReceiptCollector = field(default_factory=FileReceiptCollector)
    host: HostProbe = field(default_factory=LocalHostProbe)
    preflight: PreflightAdapter = field(default_factory=LocalPreflightAdapter)


# ---------------------------------------------------------------------------
# The owned child and the operator interrupt.
# ---------------------------------------------------------------------------


class InterruptGuard:
    """Turns an operator interrupt into a flag the run loop can observe.

    The handler must not raise. A ``KeyboardInterrupt`` raised inside the
    observation window would unwind past the cleanup that stops the owned
    child, leaving the router running with its state on disk. The handler
    therefore only sets a flag: the loop notices it on the next poll and
    unwinds through the same ``finally`` a normal refusal uses.
    """

    def __init__(self) -> None:
        self.triggered = False
        self._previous: dict[int, object] = {}

    def _handle(self, signum: int, frame: object) -> None:
        self.triggered = True

    def install(self) -> None:
        for signum in (signal.SIGINT, signal.SIGTERM):
            try:
                self._previous[signum] = signal.signal(signum, self._handle)
            except (OSError, ValueError):
                # Not the main thread, or the platform has no such
                # signal: the run continues under the default behaviour.
                continue

    def restore(self) -> None:
        for signum, handler in list(self._previous.items()):
            try:
                signal.signal(signum, handler)  # type: ignore[arg-type]
            except (OSError, ValueError, TypeError):
                continue
        self._previous.clear()

    def __enter__(self) -> "InterruptGuard":
        self.install()
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        self.restore()
        return False


class ChildGuard:
    """Owns the one spawned child and guarantees it is stopped and reaped.

    Every path out of the spawn/observe/collect sequence -- success, a
    refusal, an exception, or an operator interrupt -- stops the child
    through this guard. The router is never left running with its state
    on disk, and the child is never left unreaped.
    """

    def __init__(self, adapters: ExecutorAdapters) -> None:
        self._adapters = adapters
        self.handle: ProcessHandle | None = None
        self.stopped = False

    def adopt(self, handle: ProcessHandle) -> None:
        """Take ownership of an already started child."""
        self.handle = handle

    def stop(self) -> None:
        if self.handle is None or self.stopped:
            return
        try:
            self._adapters.processes.terminate(self.handle)
        finally:
            # Marked even when the stop failed: a second attempt would
            # signal a process the executor may no longer own.
            self.stopped = True

    def __enter__(self) -> "ChildGuard":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        try:
            self.stop()
        except OSError:
            # The child could not be signalled. Nothing further can be
            # done from here, and the original failure -- the one worth
            # reporting -- must not be masked by this one.
            pass
        return False


# ---------------------------------------------------------------------------
# Requests and results.
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class RunRequest:
    """One host's executor request."""

    role: str
    manifest: Path
    profile: Path
    peer_config: Path
    epoch: str
    out: Path
    router_info: Path | None = None
    peer_router_info: Path | None = None
    peer_netdb: Path | None = None
    hygiene: Path | None = None
    daemon_binary: Path | None = None
    daemon_config: Path | None = None
    daemon_receipt: Path | None = None
    i2pd_binary: Path | None = None
    i2pd_config: Path | None = None
    peer_binary: Path | None = None
    i2pd_source: Path | None = None


def peer_role(role: str) -> str:
    return "i2pd" if role == "i2pr" else "i2pr"


# ---------------------------------------------------------------------------
# Step 1 -- authorization.
# ---------------------------------------------------------------------------


def _canonical_inventory(path: Path) -> tuple[dict, str]:
    """The inventory document and its canonical JSON form.

    Both hosts must hold a byte-identical inventory document: the canonical
    form is what the shared run epoch is derived from, so a document that
    differs between the hosts yields two different epochs and the receipts
    can never be joined.
    """
    try:
        resolved = path.resolve(strict=True)
        value = json.loads(resolved.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        raise ExecutorFailure("inventory-unavailable") from None
    if not isinstance(value, dict):
        raise ExecutorFailure("inventory-malformed")
    return value, json.dumps(value, sort_keys=True, separators=(",", ":"))


def _load_inventory(request: RunRequest, preflight) -> dict:
    """Load and validate the owner inventory through the Plan 449 preflight."""
    try:
        i2pr_host, i2pd_host = preflight.load_inventory(request.manifest, ROOT)
    except preflight.PreflightError as error:
        raise ExecutorFailure(error.code) from None
    document, canonical = _canonical_inventory(request.manifest)
    consent = document.get("consent")
    if (
        not isinstance(consent, dict)
        or consent.get("owner_authorized") is not True
        or consent.get("scope") != "isolated-private-subnet-only"
    ):
        raise ExecutorFailure("authorization-missing")
    timeout = document.get("timeout_seconds")
    if not isinstance(timeout, int) or isinstance(timeout, bool) or timeout <= 0:
        raise ExecutorFailure("timeout-budget-invalid")
    return {
        "hosts": {"i2pr": i2pr_host, "i2pd": i2pd_host},
        "document": document,
        "canonical": canonical,
        "timeout_seconds": timeout,
    }


def _consent_still_granted(request: RunRequest) -> bool:
    """Re-read the operator consent before a receipt is emitted.

    A run whose authorization was withdrawn while it was in flight must not
    produce an attributable artifact.
    """
    try:
        document = json.loads(request.manifest.resolve(strict=True).read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return False
    consent = document.get("consent") if isinstance(document, dict) else None
    return (
        isinstance(consent, dict)
        and consent.get("owner_authorized") is True
        and consent.get("scope") == "isolated-private-subnet-only"
    )


def _authorization_for(
    role: str,
    host: dict,
    request: RunRequest,
    run_epoch: str,
    contract,
    binary: Path | None,
    router_info: Path | None,
):
    """Derive one role's host authorization from the inventory."""
    return contract.HostAuthorization(
        role=role,
        host_digest=sha256_text(str(host["host_id"])),
        interface_digest=sha256_text(str(host["interface"])),
        address=str(host["_ip"]),
        prefix=int(host["prefix"]),
        udp_port=int(host["udp_port"]),
        router_info_digest=digest_of_file(router_info, "router-info-unavailable"),
        binary_digest=digest_of_file(binary, "router-binary-unavailable"),
        endpoint_digest=sha256_text(f"{host['_ip']}:{host['udp_port']}"),
        network_id=NETWORK_ID,
        consent=True,
        run_epoch=run_epoch,
    )


def derive_authorizations(
    request: RunRequest, inventory: dict, contract
) -> tuple[dict, str]:
    """Both roles' authorizations and the shared run epoch.

    The pair contract binds both hosts, so this host must be able to name
    the *peer's* router binary and RouterInfo as well as its own: the
    pinned stock peer is part of the lane's identity, and a host that
    cannot name it cannot bind the pair.
    """
    run_epoch = sha256_text(inventory["canonical"] + request.epoch)
    local_binary = request.daemon_binary if request.role == "i2pr" else request.i2pd_binary
    authorizations = {
        role: _authorization_for(
            role,
            inventory["hosts"][role],
            request,
            run_epoch,
            contract,
            local_binary if role == request.role else request.peer_binary,
            request.router_info if role == request.role else request.peer_router_info,
        )
        for role in ROLES
    }
    return authorizations, run_epoch


# ---------------------------------------------------------------------------
# Step 2 -- the Plan 495 peer-eligibility gate.
# ---------------------------------------------------------------------------


def classify_pair(gate, profile: dict, peer: dict) -> dict:
    """The Plan 495 verdict for one (profile, peer) pair.

    One classification entry point keeps the gate binding in a single place:
    the executor evaluates and classifies exactly one way, and the gate is
    called rather than merely imported.
    """
    return gate.classify(gate.evaluate(profile, peer))


def evaluate_gate(request: RunRequest, gate) -> tuple[dict, dict, dict]:
    """Evaluate the lab profile against the peer's own selection rules."""
    try:
        profile = gate.validate_profile(gate.load_json(request.profile))
        peer = gate.validate_peer(gate.load_json(request.peer_config))
    except gate.GateError as error:
        raise ExecutorFailure(error.code) from None
    if profile["family"] != peer["family"]:
        raise ExecutorFailure("peer-config-family-mismatch")
    verdict = classify_pair(gate, profile, peer)
    return profile, peer, verdict


def withholding_control_holds(profile: dict, peer: dict, gate) -> bool:
    """The deliberate-withholding control, run before anything starts.

    The peer's selection capability is withheld and the gate must answer
    ``peer-selection-refused`` with the cause ``intentional-withholding`` and
    never the reviewed human judgement about our own implementation. If it
    does not, this lane cannot distinguish "the
    peer would not choose us" from "our implementation is broken", and no
    result from it may be cited.
    """
    family = profile["family"]
    explore = [
        predicate.identifier
        for predicate in gate.CATALOGUE[family]
        if predicate.kind == "explore"
    ]
    if not explore:
        return False
    withheld = dict(profile)
    withheld["withheld"] = [explore[0]]
    try:
        gate.validate_profile(withheld)
    except gate.GateError:
        return False
    verdict = classify_pair(gate, withheld, peer)
    return (
        verdict["verdict"] != "lane-ready"
        and verdict["outcome_class"] == "peer-selection-refused"
        and verdict["cause"] == "intentional-withholding"
        and verdict["outcome_class"] != _human_judgement_outcome(gate)
    )


def _human_judgement_outcome(gate) -> str:
    """The gate's own name for the reviewed human-judgement class.

    The executor never reports that class on its own: it is a decision a
    person makes about our implementation, not a reading of a ladder. The
    name is taken from the gate's own vocabulary rather than spelled out
    here, so the executor carries no literal name of the class it refuses
    to emit.
    """
    for name in getattr(gate, "OUTCOME_CLASSES", ()):
        if name.endswith("-defect"):
            return name
    return ""


# ---------------------------------------------------------------------------
# Step 4 -- the Plan 495 peer-state hygiene protocol.
# ---------------------------------------------------------------------------


def _operator_hygiene(request: RunRequest) -> dict:
    """The operator-recorded hygiene conditions.

    Two of the five conditions are properties of the *live* socket -- the
    static key presented on the bound port, and the advertised endpoint as
    the peer sees it. They cannot be established offline from files, so the
    operator records them (the Plan 495 protocol asks for exactly that) and
    the executor re-validates the merged record with the Plan 495 checker.
    Everything the executor can observe itself it derives and overrides.
    """
    if request.hygiene is None:
        return {}
    try:
        document = json.loads(request.hygiene.resolve(strict=True).read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return {}
    if not isinstance(document, dict):
        return {}
    return document


def build_hygiene_record(
    request: RunRequest,
    local_host: dict,
    netdb: Path,
    adapters: ExecutorAdapters,
) -> dict:
    """The ``i2pr-plan495-peer-state-v1`` record this host can attest to."""
    operator = _operator_hygiene(request)
    data_dir = Path(str(local_host["data_dir"]))
    usable = adapters.host.data_directory_is_usable(data_dir)
    stale = adapters.host.stale_netdb_files(netdb)
    if stale is None:
        no_stale = False
        stale = 0
    else:
        no_stale = stale == 0
    process_fresh = adapters.host.endpoint_is_free(
        str(local_host["_ip"]), int(local_host["udp_port"])
    )
    return {
        "schema": HYGIENE_SCHEMA,
        "peer_data_dir_fresh": usable,
        "peer_process_fresh": process_fresh,
        "no_stale_netdb_files": no_stale,
        "stale_netdb_file_count": stale,
        "static_key_matches_bound_port": operator.get("static_key_matches_bound_port") is True,
        "advertised_endpoint_matches_bound_socket": operator.get(
            "advertised_endpoint_matches_bound_socket"
        )
        is True,
    }


# ---------------------------------------------------------------------------
# Steps 5 to 7 -- spawn, collect, emit.
# ---------------------------------------------------------------------------


def _toml_section_value(text: str, section: str, key: str) -> str | None:
    """One string value from one TOML section, read without a TOML parser.

    The daemon configuration is read for exactly one key, so a bounded line
    reader is enough and the executor keeps a standard-library-only surface.
    """
    current = ""
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            current = stripped[1:-1].strip()
            continue
        if current != section and not current.endswith("." + section):
            continue
        if "=" not in stripped:
            continue
        name, _, raw = stripped.partition("=")
        if name.strip() != key:
            continue
        raw = raw.split("#", 1)[0].strip()
        if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in "\"'":
            return raw[1:-1]
        return raw or None
    return None


def _daemon_receipt_path(request: RunRequest) -> Path:
    """Where the daemon exports its sanitized local receipt.

    The operator may name the file directly; otherwise it is read from the
    daemon configuration's ``[ssu2] qualification_receipt_path`` -- the very
    key the daemon was started with.
    """
    if request.daemon_receipt is not None:
        return request.daemon_receipt
    if request.daemon_config is None:
        raise ExecutorFailure("local-receipt-unavailable")
    try:
        text = request.daemon_config.resolve(strict=True).read_text(encoding="utf-8")
    except (OSError, UnicodeError):
        raise ExecutorFailure("local-receipt-unavailable") from None
    value = _toml_section_value(text, "ssu2", "qualification_receipt_path")
    if not value:
        raise ExecutorFailure("local-receipt-unavailable")
    return Path(value)


def _router_argv(request: RunRequest) -> list[str]:
    """The one local router command this host is allowed to start."""
    if request.role == "i2pr":
        if request.daemon_binary is None or request.daemon_config is None:
            raise ExecutorFailure("router-command-unavailable")
        return [
            str(request.daemon_binary),
            "run",
            "--config",
            str(request.daemon_config),
        ]
    if request.i2pd_binary is None or request.i2pd_config is None:
        raise ExecutorFailure("router-command-unavailable")
    return [str(request.i2pd_binary), "--conf", str(request.i2pd_config)]


def _expected_peer_router_info_name(request: RunRequest) -> str:
    """The stock peer's own file name for the RouterInfo it must persist."""
    try:
        payload = request.peer_router_info.read_bytes()  # type: ignore[union-attr]
    except (AttributeError, OSError):
        raise ExecutorFailure("router-info-unavailable") from None
    return i2pd_base32(hashlib.sha256(payload).digest())


def _peer_router_info_persisted(netdb: Path, expected_name: str) -> bool:
    """Whether the peer's own NetDB holds exactly the expected RouterInfo.

    The file's contents are never read and neither the hash nor the file
    name ever leaves this function.
    """
    matches = 0
    try:
        buckets = list(netdb.glob("r*"))
    except OSError:
        return False
    for bucket in buckets:
        try:
            if not bucket.is_dir():
                continue
            matches += sum(
                1
                for entry in bucket.glob(f"routerInfo-{expected_name}.dat")
                if entry.is_file()
            )
        except OSError:
            return False
    return matches == 1


def _validate_local_receipt(document: dict, phases: tuple[str, ...]) -> None:
    """Validate the daemon's sanitized local receipt export."""
    if document.get("schema") != LOCAL_RECEIPT_SCHEMA:
        raise ExecutorFailure("local-receipt-schema-invalid")
    refusal = document.get("error")
    if refusal is not None:
        # The collector refused; the run is not attributable. The daemon's
        # own refusal code is the sanitized reason.
        raise ExecutorFailure(str(refusal))
    if document.get("live_qualification") != LIVE_QUALIFICATION:
        raise ExecutorFailure("local-receipt-qualification-invalid")
    if document.get("profile") not in ("controlled_loopback", "private_lan_qualification"):
        raise ExecutorFailure("local-receipt-profile-invalid")
    for name in ("session_observed", "product_observed", "reply_correlated"):
        if not isinstance(document.get(name), bool):
            raise ExecutorFailure("local-receipt-flag-invalid")
    rows = document.get("phases")
    if not isinstance(rows, list):
        raise ExecutorFailure("local-receipt-phases-invalid")
    seen = -1
    for row in rows:
        if not isinstance(row, dict):
            raise ExecutorFailure("local-receipt-phase-invalid")
        phase = row.get("phase")
        count = row.get("count")
        if not isinstance(phase, str) or phase not in phases:
            raise ExecutorFailure("local-receipt-phase-invalid")
        if not isinstance(count, int) or isinstance(count, bool) or count < 0:
            raise ExecutorFailure("local-receipt-count-invalid")
        index = phases.index(phase)
        if index <= seen:
            raise ExecutorFailure("local-receipt-phase-order-invalid")
        seen = index


def _write_private_json(path: Path, document: dict) -> None:
    """Write one artifact, mode 0600, atomically."""
    body = json.dumps(document, sort_keys=True, indent=2) + "\n"
    temporary = path.with_name(path.name + ".tmp")
    try:
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(body)
        os.replace(temporary, path)
        os.chmod(path, 0o600)
    except OSError:
        # The executor created this temporary file, so the executor removes
        # it: a partial artifact is never left behind.
        try:
            temporary.unlink()
        except OSError:
            pass
        raise ExecutorFailure("artifact-write-failed") from None


def execute_run(
    request: RunRequest,
    adapters: ExecutorAdapters | None = None,
    interrupt: InterruptGuard | None = None,
) -> dict:
    """Run one host's attempt and emit its host receipt.

    The order is the fail-closed state machine: authorization, gate,
    withholding control, hygiene, host and reference checks, spawn,
    collect, emit. The first refusal stops the run, and every refusal
    before the spawn means no process was ever started.

    The owned child is stopped and reaped on every path out of the
    spawn/observe/collect sequence, including an exception and an
    operator interrupt.
    """
    adapters = adapters or ExecutorAdapters()
    contract = contract_module()
    gate = gate_module()
    preflight = preflight_module()
    phases = local_phases()
    interrupt = interrupt if interrupt is not None else InterruptGuard()

    if request.role not in ROLES:
        raise ExecutorFailure("receipt-role-invalid")
    if not EPOCH_PATTERN.fullmatch(request.epoch or ""):
        raise ExecutorFailure("epoch-invalid")
    if request.out is None:
        raise ExecutorFailure("run-input-missing")

    # -- 1. authorization ---------------------------------------------------
    inventory = _load_inventory(request, preflight)
    authorizations, run_epoch = derive_authorizations(request, inventory, contract)
    try:
        contract.validate_authorizations(
            (authorizations["i2pr"], authorizations["i2pd"])
        )
    except contract.ExecutorError as error:
        raise ExecutorFailure(error.code) from None

    # -- 2. the Plan 495 gate, before any process starts ---------------------
    profile, peer, verdict = evaluate_gate(request, gate)
    if verdict["verdict"] != "lane-ready":
        raise ExecutorFailure(
            verdict["reason"], verdict["outcome_class"], verdict["cause"]
        )

    # -- 3. the deliberate-withholding control --------------------------------
    if not withholding_control_holds(profile, peer, gate):
        raise ExecutorFailure("withholding-control-failed")

    # -- 4. the peer-state hygiene protocol ----------------------------------
    local_host = inventory["hosts"][request.role]
    peer_host = inventory["hosts"][peer_role(request.role)]
    netdb = _netdb_path(request, local_host)
    hygiene = build_hygiene_record(request, local_host, netdb, adapters)
    violations = gate.validate_hygiene(hygiene)
    if violations:
        raise ExecutorFailure(violations[0], "peer-state-poisoned", "peer-state")

    # -- 5. everything the collection needs is checked before the spawn ------
    # A missing input must not cost a router start.
    argv = _router_argv(request)
    peer_router_info_digest = digest_of_file(
        request.peer_router_info, "router-info-unavailable"
    )
    expected_name = (
        _expected_peer_router_info_name(request) if request.role == "i2pd" else ""
    )
    receipt_path = _daemon_receipt_path(request) if request.role == "i2pr" else None

    # -- 6. the local host and the pinned reference, before any socket -------
    # A changed interface, a changed route or an unpinned peer binary must
    # be refused before a process starts, not discovered afterwards.
    adapters.preflight.check_local_host(local_host, peer_host, ROOT)
    _require_private_data_directory(Path(str(local_host["data_dir"])))
    if request.role == "i2pd":
        # The stock peer is part of the lane's identity: the binary that is
        # about to run has to be the pinned 2.61.0 build of the pinned
        # checkout, or the lane proves nothing about the pinned peer.
        adapters.preflight.verify_reference(
            _i2pd_source_path(request), Path(argv[0])
        )

    # -- 7. spawn only the declared local router ------------------------------
    with interrupt, ChildGuard(adapters) as child:
        handle = adapters.processes.spawn(argv)
        child.adopt(handle)
        deadline = time.monotonic() + inventory["timeout_seconds"]

        def ready() -> bool:
            if interrupt.triggered:
                # The operator revoked the run: unwind through the same
                # cleanup every other refusal uses.
                raise ExecutorFailure("operator-interrupted")
            return _observation_ready(
                request, receipt_path, netdb, expected_name, adapters.receipts
            )

        adapters.processes.observe(handle, deadline, ready)
        if interrupt.triggered:
            raise ExecutorFailure("operator-interrupted")
        child.stop()

        # -- 8. collect ------------------------------------------------------
        if request.role == "i2pr":
            document = adapters.receipts.collect(receipt_path)
            _validate_local_receipt(document, phases)
            rows = tuple((row["phase"], row["count"]) for row in document["phases"])
            try:
                contract.validate_local_observed(authorizations["i2pr"], rows)
            except contract.ExecutorError as error:
                raise ExecutorFailure(error.code) from None
            phases_out = [
                {"phase": row["phase"], "count": row["count"]} for row in document["phases"]
            ]
            session_observed = bool(document["session_observed"])
            product_observed = bool(document["product_observed"])
            reply_correlated = bool(document["reply_correlated"])
            peer_ri_persisted = False
        else:
            # Stock i2pd has no Plan 480 observer: the peer-side observation is
            # the peer's own persisted RouterInfo and nothing else.
            phases_out = []
            session_observed = False
            product_observed = False
            reply_correlated = False
            peer_ri_persisted = _peer_router_info_persisted(netdb, expected_name)

        # -- 9. emit ---------------------------------------------------------
        if interrupt.triggered:
            raise ExecutorFailure("operator-interrupted")
        if not _consent_still_granted(request):
            raise ExecutorFailure("authorization-revoked")
        artifact = {
            "schema": HOST_RECEIPT_SCHEMA,
            "role": request.role,
            "run_epoch": run_epoch,
            "host_digest": authorizations[request.role].host_digest,
            "peer_router_info_digest": peer_router_info_digest,
            "local_router_info_digest": authorizations[request.role].router_info_digest,
            "binary_digest": authorizations[request.role].binary_digest,
            "endpoint_digest": authorizations[request.role].endpoint_digest,
            "network_id": NETWORK_ID,
            "phases": phases_out,
            "session_observed": session_observed,
            "product_observed": product_observed,
            "reply_correlated": reply_correlated,
            "peer_ri_persisted": peer_ri_persisted,
            "live_qualification": LIVE_QUALIFICATION,
        }
        _write_private_json(request.out, artifact)
        return {
            "schema": RESULT_SCHEMA,
            "result": "host-receipt-written",
            "role": request.role,
            "run_epoch": run_epoch,
            "packet_attempt": "local-pair-only",
            "live_qualification": LIVE_QUALIFICATION,
        }


def _netdb_path(request: RunRequest, local_host: dict) -> Path:
    """The NetDB directory this host inspects for stale peer state.

    On the i2pd host that is the stock peer's own NetDB -- the subject of
    the Plan 495 hygiene protocol. On the i2pr host it is the local
    router's own NetDB, which is the only router state that host can
    attest to; the executor never claims the remote host's state.
    """
    if request.peer_netdb is not None:
        return request.peer_netdb
    return Path(str(local_host["data_dir"])) / "netDb"


def _i2pd_source_path(request: RunRequest) -> Path:
    """The pinned i2pd source checkout this host verifies against.

    The operator may name the checkout explicitly; otherwise the
    repository's conventional pinned checkout is used. The executor never
    falls back to some other tree: an absent checkout is refused by the
    preflight with its own sanitized code.
    """
    if request.i2pd_source is not None:
        return request.i2pd_source
    return pinned_i2pd_source()


def _require_private_data_directory(path: Path) -> None:
    """The declared data directory must be absent or exactly 0700.

    The operator owns the directory: the executor neither creates it nor
    removes it. It only refuses a directory whose mode would let another
    local account read the router's state, because the lane's own state
    has to stay private to the run.
    """
    try:
        if not path.exists():
            # An absent directory holds nothing yet; the router makes it.
            return
        if not path.is_dir():
            raise ExecutorFailure("data-directory-invalid")
        mode = stat.S_IMODE(path.stat().st_mode)
    except OSError:
        raise ExecutorFailure("data-directory-invalid") from None
    if mode != 0o700:
        raise ExecutorFailure("data-directory-mode-invalid")


def _receipt_carries_a_session(document: dict) -> bool:
    """Whether the exported receipt describes an authenticated session.

    The daemon writes its receipt file on its first export tick, when the
    receipt still holds only ``permit_checked``. The file's existence is
    therefore not evidence that a session happened: a receipt that
    carries no session phase is not ready, and the observation window
    keeps waiting.
    """
    if document.get("session_observed") is True:
        return True
    rows = document.get("phases")
    if not isinstance(rows, list):
        return False
    for row in rows:
        if not isinstance(row, dict) or row.get("phase") not in SESSION_PHASES:
            continue
        count = row.get("count")
        if isinstance(count, int) and not isinstance(count, bool) and count > 0:
            return True
    return False


def _observation_ready(
    request: RunRequest,
    receipt_path: Path | None,
    netdb: Path,
    expected_name: str,
    collector: ReceiptCollector,
) -> bool:
    """Whether this host has observed what the lane needs it to observe."""
    if request.role == "i2pr":
        if receipt_path is None or not receipt_path.is_file():
            return False
        try:
            document = collector.collect(receipt_path)
        except ExecutorFailure:
            # A receipt that cannot be read yet is not a session.
            return False
        return _receipt_carries_a_session(document)
    return bool(expected_name) and _peer_router_info_persisted(netdb, expected_name)


# ---------------------------------------------------------------------------
# The tier ladder and the joined lane evidence.
# ---------------------------------------------------------------------------


def _phase_counts(receipt: dict) -> dict[str, int]:
    counts = {phase: 0 for phase in local_phases()}
    for row in receipt.get("phases", []):
        counts[row["phase"]] = int(row["count"])
    return counts


def build_tiers(i2pr_receipt: dict, i2pd_receipt: dict, gate) -> list[dict]:
    """The Plan 495 tier ladder for one joined pair of host receipts.

    A lower tier's pass is never retracted by a higher tier's failure, and a
    tier is never ``pass`` unless its rule holds.
    """
    counts = _phase_counts(i2pr_receipt)
    sessions = counts["peer_session_accepted"] + counts["peer_session_initiated"]
    authenticated = counts["peer_authenticated"]
    any_session = authenticated > 0 or sessions > 0
    request_sent = counts["request_sent"]
    request_received = counts["request_received"]

    def row(tier: int, state: str, clause: str) -> dict:
        return {
            "tier": tier,
            "state": state,
            "reason": f"{gate.TIER_NAMES[tier]} — {clause}",
        }

    tiers: list[dict] = []

    # T0: the peer admitted and persisted our signed RouterInfo.
    if i2pd_receipt.get("peer_ri_persisted") is True:
        tiers.append(
            row(
                0,
                "pass",
                "the peer's own NetDB was checked for the expected RouterInfo file by name and holds it",
            )
        )
    else:
        tiers.append(
            row(
                0,
                "fail",
                "the peer's own NetDB was checked for the expected RouterInfo file by name and does not hold it",
            )
        )

    # T1: the peer's own selection state admitted us as a dial target.
    if counts["peer_session_initiated"] > 0:
        tiers.append(row(1, "pass", "the peer initiated a session to us"))
    elif any_session:
        tiers.append(row(1, "fail", "a session was observed but the peer never admitted us as a target"))
    else:
        tiers.append(row(1, "not-attempted", "no authenticated session was observed"))

    # T2: an authenticated transport session was established.
    if sessions > 0:
        tiers.append(row(2, "pass", "an authenticated transport session was established"))
    else:
        # No second branch: the daemon's export sets `peer_authenticated`
        # to the sum of the two session counts, so an authentication count
        # above zero and a session count above zero are one quantity. The
        # branch that used to claim otherwise was unreachable.
        tiers.append(row(2, "not-attempted", "no authenticated session was observed"))

    # T3: an I2NP message travelled in one direction.
    if request_sent > 0 or request_received > 0:
        tiers.append(row(3, "pass", "an I2NP message travelled in one direction"))
    elif any_session:
        tiers.append(row(3, "fail", "a session was established but no I2NP message travelled"))
    else:
        tiers.append(row(3, "not-attempted", "no authenticated session was observed"))

    # T4: the peer answered a request we originated.
    if i2pr_receipt.get("reply_correlated") is True:
        tiers.append(row(4, "pass", "a correlated two-way reply was observed"))
    elif request_sent > 0 and request_received > 0:
        # Deliberate: the correlation is observed only inside one export
        # drain interval, so the absence of a correlation is not evidence
        # that the peer did not answer. The tier is not attempted, not
        # failed.
        tiers.append(
            row(
                4,
                "not-attempted",
                "the reply correlation is observed only within one export drain interval, so a reply arriving after the next drain is not correlated",
            )
        )
    else:
        tiers.append(row(4, "not-attempted", "no request was both sent and received"))

    # T5: the peer initiated a fresh session to us.
    if counts["peer_session_initiated"] > 0:
        tiers.append(row(5, "pass", "the peer initiated a fresh session to us"))
    elif any_session:
        tiers.append(row(5, "fail", "a session was observed but the peer initiated none"))
    else:
        tiers.append(row(5, "not-attempted", "no authenticated session was observed"))

    # T6: our daemon's own product behaviour changed.
    if i2pr_receipt.get("product_observed") is True:
        tiers.append(row(6, "pass", "our daemon's own product behaviour changed"))
    elif request_received > 0:
        tiers.append(row(6, "fail", "a request was received but no product effect was observed"))
    else:
        tiers.append(row(6, "not-attempted", "no request was received"))

    # T7 and T8 are out of scope for the SSU2 private-LAN lane.
    tiers.append(row(7, "not-attempted", "out of scope for the SSU2 private-LAN lane"))
    tiers.append(row(8, "not-attempted", "out of scope for the SSU2 private-LAN lane"))
    return tiers


def build_lane_evidence(
    i2pr_receipt: dict, i2pd_receipt: dict, gate, family: str = "i2pd",
    label: str = "plan476-private-lan-ssu2",
) -> dict:
    """The joined lane evidence document.

    The outcome is derived from the ladder, not from the mere presence of
    a lower-tier pass. A peer that admitted our RouterInfo and then
    declined to select or dial us is a peer-side refusal, and the
    canonical model says a peer-side refusal is never reported as a
    pass.

    The reviewed human judgement about our own code is never emitted
    here: that classification is a decision a person makes, not a
    reading of the ladder.
    """
    tiers = build_tiers(i2pr_receipt, i2pd_receipt, gate)
    states = {row["tier"]: row["state"] for row in tiers}
    earned = [row["tier"] for row in tiers if row["state"] == "pass"]
    target = max(earned) if earned else 0
    if states.get(0) == "fail":
        # The peer did not admit and persist our signed RouterInfo.
        outcome, cause = "peer-admission-refused", "unattributed"
    elif states.get(1) == "fail" or states.get(5) == "fail":
        # The peer admitted us and its own selection state declined to
        # dial us. The ladder cannot say *why*, so the cause stays
        # unattributed rather than claiming one.
        outcome, cause = "peer-selection-refused", "unattributed"
    elif any(row["tier"] in range(1, 7) and row["state"] == "pass" for row in tiers):
        outcome, cause = "pass", None
    else:
        # Nothing was attributable and the bounded window elapsed.
        outcome, cause = "timeout", "unattributed"
    if outcome not in LANE_OUTCOME_CLASSES:
        # Fail closed: an outcome outside the executor's own vocabulary is
        # never written into an evidence document.
        raise ExecutorFailure("evidence-outcome-class-invalid")
    return {
        "schema": EVIDENCE_SCHEMA,
        "family": family,
        "label": label,
        "target_tier": target,
        "outcome": outcome,
        "cause": cause,
        "tiers": tiers,
        "attempts": [
            {"outcome": outcome, "cause": cause, "consumes_budget": True}
        ],
        "attempts_consumed": 1,
    }


def _load_host_receipt(path: Path) -> dict:
    """Load and validate one host receipt artifact."""
    try:
        document = json.loads(path.resolve(strict=True).read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        raise ExecutorFailure("receipt-unavailable") from None
    if not isinstance(document, dict):
        raise ExecutorFailure("receipt-schema-invalid")
    if document.get("schema") != HOST_RECEIPT_SCHEMA:
        raise ExecutorFailure("receipt-schema-invalid")
    if document.get("role") not in ROLES:
        raise ExecutorFailure("receipt-role-invalid")
    for name in (
        "run_epoch",
        "host_digest",
        "peer_router_info_digest",
        "local_router_info_digest",
        "binary_digest",
        "endpoint_digest",
    ):
        value = document.get(name)
        if not isinstance(value, str) or HEX64_PATTERN.fullmatch(value) is None:
            raise ExecutorFailure("receipt-digest-invalid")
    if document.get("network_id") != NETWORK_ID:
        raise ExecutorFailure("network-id-mismatch")
    if document.get("live_qualification") != LIVE_QUALIFICATION:
        raise ExecutorFailure("receipt-qualification-invalid")
    for name in ("session_observed", "product_observed", "reply_correlated", "peer_ri_persisted"):
        if not isinstance(document.get(name), bool):
            raise ExecutorFailure("receipt-flag-invalid")
    phases = local_phases()
    rows = document.get("phases")
    if not isinstance(rows, list):
        raise ExecutorFailure("receipt-phases-invalid")
    seen = -1
    for row in rows:
        if not isinstance(row, dict):
            raise ExecutorFailure("receipt-phase-invalid")
        phase = row.get("phase")
        count = row.get("count")
        if not isinstance(phase, str) or phase not in phases:
            raise ExecutorFailure("receipt-phase-invalid")
        if not isinstance(count, int) or isinstance(count, bool) or count < 0:
            raise ExecutorFailure("receipt-count-invalid")
        index = phases.index(phase)
        if index <= seen:
            raise ExecutorFailure("receipt-phase-order-invalid")
        seen = index
    return document


def reconcile_receipts(
    receipt_paths: Sequence[Path],
    out: Path,
    gate=None,
    family: str = "i2pd",
    label: str = "plan476-private-lan-ssu2",
) -> dict:
    """Join two owner-exchanged host receipts into the lane evidence."""
    gate = gate or gate_module()
    if len(receipt_paths) != 2:
        raise ExecutorFailure("independent-peer-receipt-required")
    receipts = [_load_host_receipt(path) for path in receipt_paths]
    roles = [receipt["role"] for receipt in receipts]
    if len(set(roles)) != len(roles):
        raise ExecutorFailure("receipt-role-invalid")
    if set(roles) != set(ROLES):
        raise ExecutorFailure("independent-peer-receipt-required")
    by_role = {receipt["role"]: receipt for receipt in receipts}
    i2pr_receipt, i2pd_receipt = by_role["i2pr"], by_role["i2pd"]

    if i2pr_receipt["run_epoch"] != i2pd_receipt["run_epoch"]:
        raise ExecutorFailure("run-epoch-mismatch")
    if i2pr_receipt["host_digest"] == i2pd_receipt["host_digest"]:
        raise ExecutorFailure("host-identities-not-distinct")
    if i2pr_receipt["endpoint_digest"] == i2pd_receipt["endpoint_digest"]:
        raise ExecutorFailure("host-endpoints-collide")
    if i2pr_receipt["local_router_info_digest"] == i2pd_receipt["local_router_info_digest"]:
        raise ExecutorFailure("router-identities-not-distinct")
    if i2pr_receipt["peer_router_info_digest"] != i2pd_receipt["local_router_info_digest"]:
        raise ExecutorFailure("peer-router-info-mismatch")
    if i2pd_receipt["peer_router_info_digest"] != i2pr_receipt["local_router_info_digest"]:
        raise ExecutorFailure("peer-router-info-mismatch")

    evidence = build_lane_evidence(i2pr_receipt, i2pd_receipt, gate, family, label)
    violations = gate.validate_evidence(evidence)
    if violations:
        # The evidence is not written when the Plan 495 checker rejects it:
        # an unvalidated lane evidence document is not evidence.
        raise ExecutorFailure(violations[0])
    _write_private_json(out, evidence)
    return {
        "schema": RESULT_SCHEMA,
        "result": "lane-evidence-written",
        "outcome": evidence["outcome"],
        "cause": evidence["cause"],
        "target_tier": evidence["target_tier"],
        "packet_attempt": "none",
        "live_qualification": LIVE_QUALIFICATION,
    }


# ---------------------------------------------------------------------------
# The self-test.
# ---------------------------------------------------------------------------


def _rust_local_phases() -> tuple[str, ...]:
    """The Rust export's phase vocabulary, read from the daemon source.

    The two vocabularies must be identical; a drift here would let the
    executor accept a receipt the daemon can never produce, or reject one it
    can.
    """
    try:
        source = RUST_PHASES_PATH.read_text(encoding="utf-8")
    except OSError:
        raise ExecutorFailure("rust-phase-source-unavailable") from None
    match = re.search(
        r"pub const LOCAL_PHASES: \[&str; \d+\] = \[(.*?)\];", source, re.DOTALL
    )
    if match is None:
        raise ExecutorFailure("rust-phase-source-unavailable")
    return tuple(re.findall(r'"([^"]+)"', match.group(1)))


def _synthetic_receipt(role: str, **overrides) -> dict:
    """A synthetic host receipt, for the offline ladder checks."""
    document = {
        "schema": HOST_RECEIPT_SCHEMA,
        "role": role,
        "run_epoch": "a" * 64,
        "host_digest": "b" * 64,
        "peer_router_info_digest": "c" * 64,
        "local_router_info_digest": "d" * 64,
        "binary_digest": "e" * 64,
        "endpoint_digest": "f" * 64,
        "network_id": NETWORK_ID,
        "phases": [],
        "session_observed": False,
        "product_observed": False,
        "reply_correlated": False,
        "peer_ri_persisted": False,
        "live_qualification": LIVE_QUALIFICATION,
    }
    document.update(overrides)
    return document


def _synthetic_local_receipt(**overrides) -> dict:
    document = {
        "schema": LOCAL_RECEIPT_SCHEMA,
        "profile": "private_lan_qualification",
        "phases": [
            {"phase": "permit_checked", "count": 1},
            {"phase": "router_started", "count": 1},
            {"phase": "peer_session_accepted", "count": 1},
            {"phase": "peer_session_initiated", "count": 1},
            {"phase": "peer_authenticated", "count": 2},
            {"phase": "request_sent", "count": 1},
            {"phase": "request_received", "count": 1},
        ],
        "session_observed": True,
        "product_observed": False,
        "reply_correlated": False,
        "live_qualification": LIVE_QUALIFICATION,
        "error": None,
    }
    document.update(overrides)
    return document


def _write_private_bytes(path: Path, payload: bytes) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)


def run_self_test() -> int:
    """Deterministic, in-memory checks. No process, no socket, no network."""
    import tempfile

    contract = contract_module()
    gate = gate_module()
    phases = local_phases()
    checks = 0
    failures = 0
    negative_controls = 0

    def check(condition: bool, label: str) -> None:
        nonlocal checks, failures
        checks += 1
        if condition:
            return
        failures += 1
        print(f"plan476-live-executor-self-test: FAIL: {label}", file=sys.stderr)

    def refused(callable_, expected: str, label: str) -> None:
        # Every `refused` row is a negative control: it asserts that a
        # path that must not proceed actually refuses, with the named
        # code. The count is reported separately so a reader can see
        # that the controls ran rather than inferring it from a total.
        nonlocal negative_controls
        negative_controls += 1
        try:
            callable_()
        except ExecutorFailure as error:
            check(error.code == expected, f"{label}: expected {expected}, got {error.code}")
        else:
            check(False, f"{label}: expected {expected}, nothing refused")

    # -- the phase vocabulary ------------------------------------------------
    check(phases == contract.LOCAL_PHASES, "phase-vocabulary-drift")
    check(len(phases) == 9, "phase-vocabulary-length")
    check(_rust_local_phases() == phases, "rust-phase-vocabulary-drift")

    # -- the Plan 495 gate refuses an admission-unsatisfied profile ---------
    profile = gate.validate_profile(
        gate.load_json(FIXTURE_DIR / "negative-mismatched-netid.json")
    )
    peer = gate.validate_peer(gate.load_json(FIXTURE_DIR / "peer-config-i2pd.json"))
    verdict = classify_pair(gate, profile, peer)
    check(verdict["outcome_class"] == "peer-admission-refused", "admission-refusal")
    check(verdict["cause"] == "i2pr-lab-profile", "admission-refusal-cause")

    # -- an unknown predicate refuses the lane -------------------------------
    unknown = dict(profile)
    unknown["signature_valid"] = None
    verdict = classify_pair(gate, unknown, peer)
    check(verdict["outcome_class"] == "environment-unavailable", "unknown-predicate-class")
    check(verdict["cause"] == "unattributed", "unknown-predicate-cause")

    # -- the deliberate-withholding control ----------------------------------
    lab = gate.validate_profile(gate.load_json(FIXTURE_DIR / "lab-profile-i2pd.json"))
    check(
        withholding_control_holds(lab, peer, gate),
        "withholding-control-did-not-hold",
    )
    both_routes = dict(lab)
    both_routes["published_addresses"] = [
        dict(row, address_caps="46B") for row in lab["published_addresses"]
    ]
    check(
        not withholding_control_holds(both_routes, peer, gate),
        "withholding-control-should-not-hold-on-two-routes",
    )

    # -- the hygiene validator refuses a poisoned record ---------------------
    poisoned = gate.load_json(FIXTURE_DIR / "hygiene-poisoned.json")
    check(bool(gate.validate_hygiene(poisoned)), "poisoned-hygiene-accepted")
    check(
        gate.validate_hygiene(gate.load_json(FIXTURE_DIR / "hygiene-fresh.json")) == [],
        "fresh-hygiene-rejected",
    )

    # -- the tier ladder -----------------------------------------------------
    i2pr_receipt = _synthetic_receipt(
        "i2pr",
        phases=[
            {"phase": "permit_checked", "count": 1},
            {"phase": "peer_session_accepted", "count": 1},
            {"phase": "peer_session_initiated", "count": 1},
            {"phase": "peer_authenticated", "count": 2},
            {"phase": "request_sent", "count": 1},
            {"phase": "request_received", "count": 1},
        ],
        session_observed=True,
    )
    i2pd_receipt = _synthetic_receipt(
        "i2pd",
        host_digest="1" * 64,
        endpoint_digest="2" * 64,
        local_router_info_digest="c" * 64,
        peer_router_info_digest="d" * 64,
        peer_ri_persisted=True,
    )
    tiers = build_tiers(i2pr_receipt, i2pd_receipt, gate)
    states = {row["tier"]: row["state"] for row in tiers}
    check(states[0] == "pass", "tier0-state")
    check(states[1] == "pass", "tier1-state")
    check(states[2] == "pass", "tier2-state")
    check(states[3] == "pass", "tier3-state")
    check(states[4] == "not-attempted", "tier4-special-case")
    check(states[5] == "pass", "tier5-state")
    check(states[6] == "fail", "tier6-state")
    check(states[7] == "not-attempted", "tier7-out-of-scope")
    check(states[8] == "not-attempted", "tier8-out-of-scope")
    evidence = build_lane_evidence(i2pr_receipt, i2pd_receipt, gate)
    check(evidence["outcome"] == "pass", "ladder-outcome")
    check(evidence["target_tier"] == 5, "ladder-target-tier")
    check(gate.validate_evidence(evidence) == [], "ladder-evidence-rejected")

    # -- branch 1: the peer never admitted our RouterInfo -------------------
    refused_peer = _synthetic_receipt(
        "i2pd",
        host_digest="8" * 64,
        endpoint_digest="7" * 64,
        local_router_info_digest="c" * 64,
        peer_router_info_digest="d" * 64,
        peer_ri_persisted=False,
    )
    evidence = build_lane_evidence(i2pr_receipt, refused_peer, gate)
    check(evidence["outcome"] == "peer-admission-refused", "admission-refused-outcome")
    check(evidence["cause"] == "unattributed", "admission-refused-cause")
    check(gate.validate_evidence(evidence) == [], "admission-refused-evidence-rejected")

    quiet = _synthetic_receipt("i2pr", phases=[], host_digest="9" * 64)
    evidence = build_lane_evidence(quiet, refused_peer, gate)
    check(evidence["outcome"] == "peer-admission-refused", "silent-admission-refused")
    check(gate.validate_evidence(evidence) == [], "silent-admission-refused-evidence")

    # -- branch 2: the peer admitted us and never selected or dialled us -----
    # A session we initiated, with no inbound session: T1 and T5 both fail.
    never_selected = _synthetic_receipt(
        "i2pr",
        phases=[
            {"phase": "permit_checked", "count": 1},
            {"phase": "peer_session_accepted", "count": 1},
            {"phase": "peer_authenticated", "count": 1},
            {"phase": "request_sent", "count": 1},
            {"phase": "request_received", "count": 1},
        ],
        session_observed=True,
        host_digest="9" * 64,
    )
    evidence = build_lane_evidence(never_selected, i2pd_receipt, gate)
    states = {row["tier"]: row["state"] for row in evidence["tiers"]}
    check(states[1] == "fail", "never-selected-tier1")
    check(states[5] == "fail", "never-selected-tier5")
    check(evidence["outcome"] == "peer-selection-refused", "selection-refused-outcome")
    check(evidence["cause"] == "unattributed", "selection-refused-cause")
    check(evidence["outcome"] != "pass", "selection-refused-is-not-a-pass")
    check(gate.validate_evidence(evidence) == [], "selection-refused-evidence-rejected")

    # -- branch 4: nothing was attributable and the window elapsed -----------
    quiet_peer = _synthetic_receipt(
        "i2pd",
        host_digest="8" * 64,
        endpoint_digest="7" * 64,
        local_router_info_digest="c" * 64,
        peer_router_info_digest="d" * 64,
        peer_ri_persisted=True,
    )
    evidence = build_lane_evidence(quiet, quiet_peer, gate)
    check(evidence["outcome"] == "timeout", "empty-ladder-outcome")
    check(evidence["cause"] == "unattributed", "empty-ladder-cause")
    check(gate.validate_evidence(evidence) == [], "empty-ladder-evidence-rejected")

    # -- the ladder never emits the human-judgement outcome ------------------
    for receipt_pair in (
        (i2pr_receipt, i2pd_receipt),
        (i2pr_receipt, refused_peer),
        (never_selected, i2pd_receipt),
        (quiet, quiet_peer),
    ):
        check(
            build_lane_evidence(receipt_pair[0], receipt_pair[1], gate)["outcome"]
            in LANE_OUTCOME_CLASSES,
            "ladder-emitted-an-unclassified-outcome",
        )

    # -- a receipt that carries no session is not ready ----------------------
    session_less = _synthetic_local_receipt(
        phases=[{"phase": "permit_checked", "count": 1}],
        session_observed=False,
    )
    check(
        not _receipt_carries_a_session(session_less),
        "session-less-receipt-treated-as-ready",
    )
    check(
        _receipt_carries_a_session(_synthetic_local_receipt()),
        "session-bearing-receipt-not-ready",
    )

    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)

        def write_json(path: Path, document: dict) -> Path:
            _write_private_bytes(
                path, (json.dumps(document, sort_keys=True) + "\n").encode("utf-8")
            )
            return path

        # -- reconcile rejects unusable pairs --------------------------------
        left = write_json(base / "left.json", i2pr_receipt)
        right = write_json(base / "right.json", i2pd_receipt)
        stale = write_json(
            base / "stale.json", _synthetic_receipt("i2pd", run_epoch="0" * 64)
        )
        duplicate = write_json(base / "duplicate.json", i2pr_receipt)
        joined = reconcile_receipts([left, right], base / "joined.json")
        check(joined["result"] == "lane-evidence-written", "reconcile-positive")
        check((base / "joined.json").exists(), "reconcile-positive-wrote-no-evidence")
        refused(
            lambda: reconcile_receipts([left, stale], base / "out.json", gate),
            "run-epoch-mismatch",
            "reconcile-epoch-mismatch",
        )
        refused(
            lambda: reconcile_receipts([left, duplicate], base / "out.json", gate),
            "receipt-role-invalid",
            "reconcile-duplicate-role",
        )
        refused(
            lambda: reconcile_receipts([left], base / "out.json", gate),
            "independent-peer-receipt-required",
            "reconcile-missing-peer",
        )
        check(not (base / "out.json").exists(), "reconcile-wrote-on-refusal")

        # -- the executor refuses before it starts anything ------------------
        inventory = {
            "schema": "i2pr-plan449-host-inventory-v1",
            "reference_revision": "635b013a612ff47278ef02acf8580a28e10e26c5",
            "consent": {
                "owner_authorized": True,
                "scope": "isolated-private-subnet-only",
            },
            "attempt_budget": 1,
            "timeout_seconds": 30,
            "hosts": [
                {
                    "role": "i2pr",
                    "host_id": "0" * 32,
                    "interface": "lan0",
                    "address": "192.168.44.11",
                    "prefix": 24,
                    "udp_port": 19001,
                    "data_dir": str(base / "i2pr-data"),
                },
                {
                    "role": "i2pd",
                    "host_id": "1" * 32,
                    "interface": "lan0",
                    "address": "192.168.44.12",
                    "prefix": 24,
                    "udp_port": 19002,
                    "data_dir": str(base / "i2pd-data"),
                },
            ],
        }
        manifest = base / "inventory.json"
        write_json(manifest, inventory)
        os.chmod(manifest, 0o600)
        binary = base / "i2pr-daemon"
        _write_private_bytes(binary, b"not-a-real-binary")
        peer_binary = base / "i2pd"
        _write_private_bytes(peer_binary, b"not-a-real-peer-binary")
        config = base / "daemon.toml"
        _write_private_bytes(config, b"[ssu2]\nenabled = true\n")
        router_info = base / "routerInfo.dat"
        _write_private_bytes(router_info, b"local-router-info")
        peer_router_info = base / "peer-routerInfo.dat"
        _write_private_bytes(peer_router_info, b"peer-router-info")
        hygiene = write_json(
            base / "hygiene.json",
            {
                "schema": HYGIENE_SCHEMA,
                "peer_data_dir_fresh": True,
                "peer_process_fresh": True,
                "no_stale_netdb_files": True,
                "stale_netdb_file_count": 0,
                "static_key_matches_bound_port": True,
                "advertised_endpoint_matches_bound_socket": True,
            },
        )

        def request_for(**overrides) -> RunRequest:
            values = {
                "role": "i2pr",
                "manifest": manifest,
                "profile": FIXTURE_DIR / "lab-profile-i2pd.json",
                "peer_config": FIXTURE_DIR / "peer-config-i2pd.json",
                "epoch": "0" * 32,
                "out": base / "receipt.json",
                "router_info": router_info,
                "peer_router_info": peer_router_info,
                "hygiene": hygiene,
                "daemon_binary": binary,
                "daemon_config": config,
                "daemon_receipt": base / "daemon-receipt.json",
                "peer_binary": peer_binary,
            }
            values.update(overrides)
            return RunRequest(**values)

        def adapters_for(processes=None, receipts=None, host=None, preflight=None):
            return ExecutorAdapters(
                processes=processes if processes is not None else FakeProcessAdapter(),
                receipts=(
                    receipts
                    if receipts is not None
                    else FakeReceiptCollector(_synthetic_local_receipt())
                ),
                host=host if host is not None else FakeHostProbe(),
                preflight=preflight if preflight is not None else FakePreflightAdapter(),
            )

        # The gate refuses: nothing may be started.
        adapter = FakeProcessAdapter()
        refused(
            lambda: execute_run(
                request_for(profile=FIXTURE_DIR / "negative-mismatched-netid.json"),
                adapters_for(processes=adapter),
            ),
            "admission-predicate-unsatisfied",
            "run-gate-refusal",
        )
        check(adapter.spawned == [], "gate-refusal-started-a-process")

        # The hygiene protocol refuses: nothing may be started.
        adapter = FakeProcessAdapter()
        refused(
            lambda: execute_run(
                request_for(),
                adapters_for(processes=adapter, host=FakeHostProbe(stale_files=3)),
            ),
            "hygiene-condition-failed:no_stale_netdb_files:stale netDb/r*/*.dat peer profiles",
            "run-hygiene-refusal",
        )
        check(adapter.spawned == [], "hygiene-refusal-started-a-process")

        # The withholding control fails: nothing may be started.
        both_routes = write_json(
            base / "both-routes.json",
            {
                **gate.load_json(FIXTURE_DIR / "lab-profile-i2pd.json"),
                "published_addresses": [
                    {
                        "transport": "SSU2",
                        "version": 2,
                        "host": "192.168.44.10",
                        "port": 19001,
                        "address_caps": "46B",
                    }
                ],
            },
        )
        adapter = FakeProcessAdapter()
        refused(
            lambda: execute_run(
                request_for(profile=both_routes),
                adapters_for(processes=adapter),
            ),
            "withholding-control-failed",
            "run-withholding-refusal",
        )
        check(adapter.spawned == [], "withholding-refusal-started-a-process")

        # -- the changed local host is refused before anything starts ---------
        adapter = FakeProcessAdapter()
        refused(
            lambda: execute_run(
                request_for(),
                adapters_for(
                    processes=adapter,
                    preflight=FakePreflightAdapter("route-source-mismatch"),
                ),
            ),
            "route-source-mismatch",
            "run-local-host-refusal",
        )
        check(adapter.spawned == [], "local-host-refusal-started-a-process")

        # -- an unpinned stock peer is refused before anything starts ---------
        adapter = FakeProcessAdapter()
        refused(
            lambda: execute_run(
                request_for(
                    role="i2pd",
                    i2pd_binary=peer_binary,
                    i2pd_config=config,
                ),
                adapters_for(
                    processes=adapter,
                    preflight=FakePreflightAdapter("reference-pin-mismatch"),
                ),
            ),
            "reference-pin-mismatch",
            "run-reference-refusal",
        )
        check(adapter.spawned == [], "reference-refusal-started-a-process")

        # -- a data directory that is not 0700 is refused ---------------------
        loose = base / "loose-data"
        loose.mkdir(mode=0o755, exist_ok=True)
        os.chmod(loose, 0o755)
        loose_inventory = json.loads(json.dumps(inventory))
        loose_inventory["hosts"][0]["data_dir"] = str(loose)
        loose_manifest = write_json(base / "loose-inventory.json", loose_inventory)
        os.chmod(loose_manifest, 0o600)
        adapter = FakeProcessAdapter()
        refused(
            lambda: execute_run(
                request_for(manifest=loose_manifest),
                adapters_for(processes=adapter),
            ),
            "data-directory-mode-invalid",
            "run-data-directory-mode-refusal",
        )
        check(adapter.spawned == [], "data-directory-refusal-started-a-process")

        # -- an exception still stops and reaps the owned child ---------------
        class RaisingAdapter(FakeProcessAdapter):
            def observe(self, handle, deadline, ready):
                raise RuntimeError("observer-failed")

        adapter = RaisingAdapter()
        try:
            execute_run(request_for(), adapters_for(processes=adapter))
        except RuntimeError:
            pass
        else:
            check(False, "exception-did-not-propagate")
        check(len(adapter.terminated) == 1, "exception-left-the-child-running")

        # -- a spawn failure leaves no child behind ---------------------------
        class FailingAdapter(FakeProcessAdapter):
            def spawn(self, argv):
                raise ExecutorFailure("router-start-failed")

        adapter = FailingAdapter()
        refused(
            lambda: execute_run(request_for(), adapters_for(processes=adapter)),
            "router-start-failed",
            "run-spawn-failure",
        )
        check(adapter.terminated == [], "spawn-failure-terminated-a-child")

        # -- an operator interrupt stops the child and refuses ----------------
        interrupt = InterruptGuard()
        interrupt.triggered = True
        adapter = FakeProcessAdapter()
        refused(
            lambda: execute_run(request_for(), adapters_for(processes=adapter), interrupt),
            "operator-interrupted",
            "run-operator-interrupt",
        )
        check(len(adapter.terminated) == 1, "interrupt-left-the-child-running")
        check(
            interrupt._previous == {},
            "interrupt-handler-was-not-restored",
        )

        # -- the happy path collects and emits --------------------------------
        adapter = FakeProcessAdapter()
        adapters = adapters_for(processes=adapter)
        outcome = execute_run(request_for(), adapters)
        check(outcome["result"] == "host-receipt-written", "run-result")
        check(len(adapter.spawned) == 1, "run-did-not-start-exactly-one-process")
        check(
            adapter.spawned[0][1:] == ("run", "--config", str(config)),
            "run-started-the-wrong-command",
        )
        check(len(adapter.terminated) == 1, "run-did-not-stop-the-child")
        artifact = json.loads((base / "receipt.json").read_text(encoding="utf-8"))
        check(artifact["schema"] == HOST_RECEIPT_SCHEMA, "artifact-schema")
        check(artifact["role"] == "i2pr", "artifact-role")
        check(artifact["peer_ri_persisted"] is False, "artifact-peer-ri-persisted")
        check(
            oct((base / "receipt.json").stat().st_mode & 0o777) == "0o600",
            "artifact-mode",
        )
        for forbidden in ("192.168.44", "lan0", "0" * 32):
            check(
                forbidden not in json.dumps(artifact),
                f"artifact-leaked:{forbidden}",
            )

        # -- the fake I/O adapter cannot connect or start anything -----------
        collector = FakeReceiptCollector(_synthetic_local_receipt())
        for forbidden in ("socket", "subprocess", "connect", "Popen", "urlopen"):
            check(
                not hasattr(collector, forbidden),
                f"fake-collector-exposes:{forbidden}",
            )
        probe = FakeHostProbe()
        for forbidden in ("socket", "subprocess", "connect", "Popen"):
            check(not hasattr(probe, forbidden), f"fake-probe-exposes:{forbidden}")
        preflight = FakePreflightAdapter()
        for forbidden in ("socket", "subprocess", "connect", "Popen"):
            check(
                not hasattr(preflight, forbidden),
                f"fake-preflight-exposes:{forbidden}",
            )

    if failures:
        print(
            f"plan476-live-executor-self-test: FAILED ({failures} failing check(s))",
            file=sys.stderr,
        )
        return 1
    print(
        "plan476-live-executor-self-test: passed "
        f"({checks} checks, {negative_controls} negative controls, 0 failures)"
    )
    return 0


# ---------------------------------------------------------------------------
# The command line.
# ---------------------------------------------------------------------------


def _result_line(payload: dict) -> str:
    return json.dumps(payload, sort_keys=True)


def _failure_line(failure: ExecutorFailure) -> str:
    return _result_line(
        {
            "schema": RESULT_SCHEMA,
            "result": failure.outcome_class,
            "cause": failure.cause,
            "reason": failure.code,
            "packet_attempt": "none",
            "live_qualification": LIVE_QUALIFICATION,
        }
    )


def _request_from_args(args: argparse.Namespace) -> RunRequest:
    missing = [
        name
        for name in ("manifest", "profile", "peer_config", "epoch", "out")
        if getattr(args, name) is None
    ]
    if missing:
        raise ExecutorFailure(
            "authorization-missing" if missing == ["manifest"] else "run-input-missing"
        )
    return RunRequest(
        role=args.role,
        manifest=args.manifest,
        profile=args.profile,
        peer_config=args.peer_config,
        epoch=args.epoch,
        out=args.out,
        router_info=args.router_info,
        peer_router_info=args.peer_router_info,
        peer_netdb=args.peer_netdb,
        hygiene=args.hygiene,
        daemon_binary=args.daemon_binary,
        daemon_config=args.daemon_config,
        daemon_receipt=args.daemon_receipt,
        i2pd_binary=args.i2pd_binary,
        i2pd_config=args.i2pd_config,
        peer_binary=args.peer_binary,
        i2pd_source=args.i2pd_source,
    )


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    commands = parser.add_subparsers(dest="command")

    run_parser = commands.add_parser("run")
    run_parser.add_argument("--live", action="store_true")
    run_parser.add_argument("--role", choices=ROLES)
    run_parser.add_argument("--manifest", type=Path)
    run_parser.add_argument("--profile", type=Path)
    run_parser.add_argument("--peer-config", type=Path)
    run_parser.add_argument("--epoch")
    run_parser.add_argument("--out", type=Path)
    run_parser.add_argument("--router-info", type=Path)
    run_parser.add_argument("--peer-router-info", type=Path)
    run_parser.add_argument("--peer-netdb", type=Path)
    run_parser.add_argument("--hygiene", type=Path)
    run_parser.add_argument("--daemon-binary", type=Path)
    run_parser.add_argument("--daemon-config", type=Path)
    run_parser.add_argument("--daemon-receipt", type=Path)
    run_parser.add_argument("--i2pd-binary", type=Path)
    run_parser.add_argument("--i2pd-config", type=Path)
    run_parser.add_argument("--peer-binary", type=Path)
    run_parser.add_argument(
        "--i2pd-source",
        type=Path,
        help=(
            "the pinned i2pd 2.61.0 source checkout this host verifies the "
            "stock binary against; defaults to the repository's conventional "
            "pinned checkout"
        ),
    )

    reconcile_parser = commands.add_parser("reconcile")
    reconcile_parser.add_argument("--receipt", type=Path, action="append", default=[])
    reconcile_parser.add_argument("--out", type=Path)
    reconcile_parser.add_argument("--family", default="i2pd")
    reconcile_parser.add_argument("--label", default="plan476-private-lan-ssu2")

    args = parser.parse_args(list(argv) if argv is not None else None)
    if args.self_test:
        return run_self_test()
    if args.command is None:
        print(_failure_line(ExecutorFailure("command-required")))
        return 1
    try:
        if args.command == "run":
            if not args.live:
                raise ExecutorFailure("live-flag-required")
            if args.role is None:
                raise ExecutorFailure("run-input-missing")
            payload = execute_run(_request_from_args(args))
        else:
            if args.out is None:
                raise ExecutorFailure("run-input-missing")
            payload = reconcile_receipts(
                args.receipt, args.out, family=args.family, label=args.label
            )
    except ExecutorFailure as failure:
        print(_failure_line(failure))
        return 1
    except OSError:
        # A filesystem failure is an environment refusal, not a pass.
        print(_failure_line(ExecutorFailure("environment-unavailable")))
        return 1
    except KeyboardInterrupt:
        # An interrupt that arrived outside the guarded run window. The
        # owned child is stopped by the run's own cleanup; this only
        # reports the cancellation.
        print(_failure_line(ExecutorFailure("operator-interrupted")))
        return 1
    print(_result_line(payload))
    return 0


if __name__ == "__main__":
    sys.exit(main())
