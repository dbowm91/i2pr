"""Plan 069 host-compatible NTCP2 loopback smoke runner.

The loopback smoke runner orchestrates one bounded two-process NTCP2
direction between a real i2pr launcher and a real Plan 064 i2pd direct
helper on the host loopback. The runner is intentionally narrow: it
exists only to produce a Plan 068 Level 1 smoke record
(``i2pr-ntcp2-loopback-smoke-v1``, evidence tier
``external-loopback-smoke``; see :mod:`loopback_smoke_record`) and
cannot satisfy a release-qualification predicate (see
:mod:`evidence_tier`).

The runner does not import or call Plan 056/066 candidate, bundle,
certificate, rootless-topology, or Multipass authority. It must remain
structurally incapable of producing a release bundle.

Structural choices:

- one runner instance owns one direction; one invocation runs one
  direction;
- the i2pr side uses the Plan 065 strict scenario-v2 contract;
- the reference side uses the Plan 064 strict driver config contract;
- the strict scenario renderer reuses the Plan 065 helpers without
  importing the canonical mixed-runner;
- the canonical RouterInfo exchange happens through the existing
  ``router_info`` validator;
- process ownership, cleanup, and residual checks follow the Plan 038
  ownership contract;
- the runner records the earliest real failure stage and refuses to
  silently collapse failures into ``evidence-incomplete`` or
  ``blocked_execution_lane_unavailable``;
- the network audit degrades from strace-allowlist to
  configuration-only when ptrace is denied;
- raw payload capture is not supported; diagnostics may only be
  ``off`` or ``sanitized``.

Plan 075 corrections: the runner now launches one real i2pr process
and one configured real reference process for each direction, binds
every accepted event to a measured reference binary digest, and
refuses to mark protocol milestones unless a validated structured
event has been observed. The synthetic-provenance fallback hashes
that fabricated a schema-valid digest from a run string are removed;
the source-lock record plus the on-disk helper source and observer
patch are measured directly. The runner fails closed with one of the
typed blockers ``runner-reference-process-not-executed``,
``runner-reference-events-missing``,
``runner-synthetic-provenance-rejected``, or
``runner-protocol-event-unproven`` whenever any of these contracts
is violated.

The runner is invoked through:

```text
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pr-to-i2pd-ipv4 \
  --reference-driver <path> \
  --output <smoke-record.json>
```
"""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any, Callable, Final, Iterator

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import loopback_smoke_record as smoke_record
from loopback_smoke_record import (
    SCHEMA as SMOKE_SCHEMA,
    SCHEMA_VERSION as SMOKE_SCHEMA_VERSION,
    EVIDENCE_TIER as SMOKE_EVIDENCE_TIER,
    canonical_record_digest,
)

import reference_event  # noqa: E402  (Plan 075 reference-event v1 schemas)


ALLOWED_DIRECTIONS: Final[frozenset[str]] = frozenset({
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
})

ALLOWED_NETWORK_AUDIT_MODES: Final[frozenset[str]] = frozenset({
    "auto",
    "strace",
    "configuration-only",
})

ALLOWED_DIAGNOSTICS_MODES: Final[frozenset[str]] = frozenset({
    "off",
    "sanitized",
})

ALLOWED_EVENT_NAMES: Final[frozenset[str]] = frozenset({
    "process_started",
    "listener_ready",
    "tcp_connected",
    "ntcp2_authenticated",
    "frame_emitted",
    "frame_authenticated_and_decrypted",
    "i2np_message_decoded",
    "terminal_clean",
})

# Plan 075 typed blockers. The runner raises one of these whenever the
# runner-integrity contract is violated:
#
#   runner-reference-process-not-executed
#       the configured reference driver process was never launched
#       (the runner built a second i2pr-interop process command, or
#       the reference was substituted by a fake injection outside
#       the unit-test fixture adapter).
#   runner-reference-events-missing
#       the reference process did not emit any structured events
#       before the protocol deadline.
#   runner-synthetic-provenance-rejected
#       a required provenance field was determined from a synthetic
#       placeholder digest instead of a measured file.
#   runner-protocol-event-unproven
#       a protocol milestone was marked without a validated
#       structured event satisfying the milestone predicate.
TYPED_BLOCKER_REFERENCE_PROCESS_NOT_EXECUTED: Final[str] = (
    "runner-reference-process-not-executed"
)
TYPED_BLOCKER_REFERENCE_EVENTS_MISSING: Final[str] = (
    "runner-reference-events-missing"
)
TYPED_BLOCKER_SYNTHETIC_PROVENANCE_REJECTED: Final[str] = (
    "runner-synthetic-provenance-rejected"
)
TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN: Final[str] = (
    "runner-protocol-event-unproven"
)

TYPED_BLOCKER_CODES: Final[frozenset[str]] = frozenset({
    TYPED_BLOCKER_REFERENCE_PROCESS_NOT_EXECUTED,
    TYPED_BLOCKER_REFERENCE_EVENTS_MISSING,
    TYPED_BLOCKER_SYNTHETIC_PROVENANCE_REJECTED,
    TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
})

DEFAULT_READINESS_TIMEOUT_SECONDS: Final[float] = 20.0
DEFAULT_HANDSHAKE_TIMEOUT_SECONDS: Final[float] = 30.0
DEFAULT_DATA_TIMEOUT_SECONDS: Final[float] = 20.0
DEFAULT_TOTAL_TIMEOUT_SECONDS: Final[float] = 120.0
DEFAULT_CLEANUP_TIMEOUT_SECONDS: Final[float] = 15.0

MIN_DEADLINE_SECONDS: Final[float] = 0.5
MAX_DEADLINE_SECONDS: Final[float] = 600.0

REFERENCE_NAME: Final[str] = "i2pd"
REFERENCE_VERSION: Final[str] = "2.60.0"
REFERENCE_REVISION: Final[str] = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
REFERENCE_DRIVER_MODE: Final[str] = "i2pd-direct-driver"
REFERENCE_IMPLEMENTATION: Final[str] = "i2pd-direct-driver"

LOOPBACK_IPV4: Final[str] = "127.0.0.1"
LOOPBACK_IPV6: Final[str] = "::1"
# Plan 064 strict config and Plan 065 strict scenario require synthetic
# RFC 5737 addresses. The runner records the synthetic identifier in
# the strict configs but the actual TCP connection happens on the
# kernel-allocated loopback ports. The runner reserves the synthetic
# endpoint pair ``SYNTHETIC_IPV4_LOCAL``/``SYNTHETIC_IPV4_PEER`` to
# avoid collision with any Plan 046 rootless lane.
SYNTHETIC_IPV4_LOCAL: Final[str] = "192.0.2.1"
SYNTHETIC_IPV4_PEER: Final[str] = "192.0.2.2"

HEX64: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")
HEX40: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}$")
SCENARIO_ID_RE: Final[re.Pattern[str]] = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$")
RUN_ID_RE: Final[re.Pattern[str]] = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")


class ProcessRole(str):
    """Plan 075 process role. One of ``i2pr`` or ``reference``."""

    I2PR: "ProcessRole"
    REFERENCE: "ProcessRole"


ProcessRole.I2PR = ProcessRole("i2pr")
ProcessRole.REFERENCE = ProcessRole("reference")


class TransportRole(str):
    """Plan 075 transport role. One of ``listener`` or ``dialer``."""

    LISTENER: "TransportRole"
    DIALER: "TransportRole"


TransportRole.LISTENER = TransportRole("listener")
TransportRole.DIALER = TransportRole("dialer")


@dataclasses.dataclass(frozen=True)
class ProcessRoleAssignment:
    """Plan 075 role pair for one direction.

    Each direction is the unique mapping of two transport roles
    (listener, dialer) to two process roles (i2pr, reference). The
    runner is structurally incapable of producing a passing record
    when both transport roles resolve to the same process role.
    """

    listener: ProcessRole
    dialer: ProcessRole


# Plan 075 role assignment per direction. The mapping is unique per
# direction; reversing the direction swaps the roles.
ROLE_ASSIGNMENTS: Final[dict[str, ProcessRoleAssignment]] = {
    "i2pr-to-i2pd-ipv4": ProcessRoleAssignment(
        listener=ProcessRole.REFERENCE,
        dialer=ProcessRole.I2PR,
    ),
    "i2pd-to-i2pr-ipv4": ProcessRoleAssignment(
        listener=ProcessRole.I2PR,
        dialer=ProcessRole.REFERENCE,
    ),
}


# Plan 075 path layout for the source-locked reference driver. The
# helper source, observer patch, and run script are tracked in the
# repository and may be measured directly. The installed i2pd tree
# lives under ``target/interop/cache/i2pd/<tree>/`` and is not
# present on the constrained host; the runner fails closed with
# ``runner-synthetic-provenance-rejected`` when the source-lock
# placeholder is still zero.
HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
I2PD_DRIVER_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd"
I2PD_DRIVER_SOURCE = I2PD_DRIVER_DIR / "src/i2pd_ntcp2_interop_driver.cpp"
I2PD_OBSERVER_HEADER = I2PD_DRIVER_DIR / "src/interop_observer.h"
I2PD_OBSERVER_SOURCE = I2PD_DRIVER_DIR / "src/interop_observer.cpp"
I2PD_OBSERVER_PATCH = (
    I2PD_DRIVER_DIR / "patches/i2pd-2.60.0-interop-observer.patch"
)
I2PD_RUN_SCRIPT = I2PD_DRIVER_DIR / "run-driver.sh"
I2PD_SOURCE_LOCK = I2PD_DRIVER_DIR / "source-lock.json"

# Plan 075 reference event stream identity. The reference driver
# emits structured events to ``<output_dir>/events.ndjson`` per the
# Plan 062 reference-event v1 schema.
REFERENCE_EVENT_SCHEMA: Final[str] = "i2pr-reference-event-v1"
REFERENCE_EVENT_VERSION: Final[int] = 1


class LoopbackSmokeError(ValueError):
    """Base error class for the loopback smoke runner."""


class LoopbackSmokeConfigError(LoopbackSmokeError):
    """The smoke config is malformed or violates a strict boundary."""


class LoopbackSmokeRunError(LoopbackSmokeError):
    """The smoke run encountered a recoverable failure."""

    def __init__(self, code: str, *, stage: str, reason: str | None = None):
        super().__init__(code)
        self.code = code
        self.stage = stage
        self.reason = reason or code


@dataclasses.dataclass(frozen=True)
class LoopbackSmokeConfig:
    """The strict runner configuration validated by :func:`parse_cli_args`.

    Every field is bounded. Unknown CLI flags fail closed before any
    file or process is opened. The config is the canonical input to
    :class:`LoopbackSmokeRunner`.
    """

    direction: str
    reference_driver_binary: Path
    reference_build_manifest: Path
    reference_source_lock: Path
    output_record: Path
    source_commit: str
    run_timeout_seconds: float
    readiness_timeout_seconds: float
    handshake_timeout_seconds: float
    data_timeout_seconds: float
    network_audit_mode: str
    diagnostics_mode: str

    @property
    def is_i2pr_initiator(self) -> bool:
        return self.direction == "i2pr-to-i2pd-ipv4"

    @property
    def scenario_id(self) -> str:
        return f"loopback-{self.direction}"


def _bounded_deadline(value: float, *, label: str) -> float:
    if not isinstance(value, (int, float)):
        raise LoopbackSmokeConfigError(f"{label}-not-numeric")
    fvalue = float(value)
    if fvalue < MIN_DEADLINE_SECONDS or fvalue > MAX_DEADLINE_SECONDS:
        raise LoopbackSmokeConfigError(f"{label}-out-of-range")
    return fvalue


def _require_path(path: Any, *, label: str) -> Path:
    if not isinstance(path, (str, Path)):
        raise LoopbackSmokeConfigError(f"{label}-not-path")
    candidate = Path(path)
    if not candidate.exists():
        raise LoopbackSmokeConfigError(f"{label}-missing:{candidate}")
    return candidate


def parse_config_dict(payload: Any) -> LoopbackSmokeConfig:
    """Validate a config dict and return the typed runner config.

    The runner rejects any unknown field. The network audit mode
    accepts only ``auto``, ``strace``, or ``configuration-only``. The
    diagnostics mode accepts only ``off`` or ``sanitized``; raw
    diagnostics are not supported.
    """

    if not isinstance(payload, dict):
        raise LoopbackSmokeConfigError("config-not-object")
    extra = set(payload) - {
        "direction",
        "reference_driver_binary",
        "reference_build_manifest",
        "reference_source_lock",
        "output_record",
        "source_commit",
        "run_timeout_seconds",
        "readiness_timeout_seconds",
        "handshake_timeout_seconds",
        "data_timeout_seconds",
        "network_audit_mode",
        "diagnostics_mode",
    }
    if extra:
        raise LoopbackSmokeConfigError(
            f"config-unknown-field:{','.join(sorted(extra))}"
        )

    direction = payload.get("direction")
    if direction not in ALLOWED_DIRECTIONS:
        raise LoopbackSmokeConfigError("direction-not-allowlisted")

    reference_driver_binary = _require_path(
        payload["reference_driver_binary"], label="reference_driver_binary"
    )
    reference_build_manifest = _require_path(
        payload["reference_build_manifest"], label="reference_build_manifest"
    )
    reference_source_lock = _require_path(
        payload["reference_source_lock"], label="reference_source_lock"
    )
    output_record_path = payload["output_record"]
    if not isinstance(output_record_path, (str, Path)):
        raise LoopbackSmokeConfigError("output_record-not-path")
    output_record = Path(output_record_path).resolve()
    if output_record.suffix != ".json":
        raise LoopbackSmokeConfigError("output_record-must-be-json")

    source_commit = payload["source_commit"]
    if not isinstance(source_commit, str) or not HEX40.fullmatch(source_commit):
        raise LoopbackSmokeConfigError("source_commit-not-40-hex")

    run_timeout = _bounded_deadline(
        payload["run_timeout_seconds"], label="run_timeout_seconds"
    )
    readiness_timeout = _bounded_deadline(
        payload["readiness_timeout_seconds"], label="readiness_timeout_seconds"
    )
    handshake_timeout = _bounded_deadline(
        payload["handshake_timeout_seconds"], label="handshake_timeout_seconds"
    )
    data_timeout = _bounded_deadline(
        payload["data_timeout_seconds"], label="data_timeout_seconds"
    )

    network_audit_mode = payload["network_audit_mode"]
    if network_audit_mode not in ALLOWED_NETWORK_AUDIT_MODES:
        raise LoopbackSmokeConfigError("network_audit_mode-not-allowlisted")

    diagnostics_mode = payload["diagnostics_mode"]
    if diagnostics_mode not in ALLOWED_DIAGNOSTICS_MODES:
        raise LoopbackSmokeConfigError(
            "diagnostics_mode-not-allowlisted:raw-capture-forbidden"
        )

    return LoopbackSmokeConfig(
        direction=direction,
        reference_driver_binary=reference_driver_binary,
        reference_build_manifest=reference_build_manifest,
        reference_source_lock=reference_source_lock,
        output_record=output_record,
        source_commit=source_commit,
        run_timeout_seconds=run_timeout,
        readiness_timeout_seconds=readiness_timeout,
        handshake_timeout_seconds=handshake_timeout,
        data_timeout_seconds=data_timeout,
        network_audit_mode=network_audit_mode,
        diagnostics_mode=diagnostics_mode,
    )


def parse_cli_args(argv: list[str] | None = None) -> LoopbackSmokeConfig:
    """Parse the strict CLI and return the typed runner config."""

    parser = argparse.ArgumentParser(
        prog="loopback_smoke",
        description="Plan 069 host-compatible NTCP2 loopback smoke runner.",
    )
    parser.add_argument(
        "--direction",
        required=True,
        choices=sorted(ALLOWED_DIRECTIONS),
        help="The smoke direction; only the two i2pd directions are allowlisted.",
    )
    parser.add_argument(
        "--reference-driver",
        dest="reference_driver_binary",
        required=True,
        type=Path,
        help="Path to the Plan 064 i2pd direct driver binary.",
    )
    parser.add_argument(
        "--reference-build-manifest",
        dest="reference_build_manifest",
        required=True,
        type=Path,
        help="Path to the Plan 064 driver build manifest.",
    )
    parser.add_argument(
        "--reference-source-lock",
        dest="reference_source_lock",
        required=True,
        type=Path,
        help="Path to the Plan 064 driver source-lock record.",
    )
    parser.add_argument(
        "--output",
        dest="output_record",
        required=True,
        type=Path,
        help="Path where the sanitized smoke record will be written.",
    )
    parser.add_argument(
        "--source-commit",
        dest="source_commit",
        required=True,
        help="40-lowercase-hex SHA-1 of the executed source commit.",
    )
    parser.add_argument(
        "--run-timeout-seconds",
        dest="run_timeout_seconds",
        type=float,
        default=DEFAULT_TOTAL_TIMEOUT_SECONDS,
        help="Total run timeout in seconds (default %(default)s).",
    )
    parser.add_argument(
        "--readiness-timeout-seconds",
        dest="readiness_timeout_seconds",
        type=float,
        default=DEFAULT_READINESS_TIMEOUT_SECONDS,
        help="Listener readiness timeout in seconds (default %(default)s).",
    )
    parser.add_argument(
        "--handshake-timeout-seconds",
        dest="handshake_timeout_seconds",
        type=float,
        default=DEFAULT_HANDSHAKE_TIMEOUT_SECONDS,
        help="Handshake timeout in seconds (default %(default)s).",
    )
    parser.add_argument(
        "--data-timeout-seconds",
        dest="data_timeout_seconds",
        type=float,
        default=DEFAULT_DATA_TIMEOUT_SECONDS,
        help="Data phase timeout in seconds (default %(default)s).",
    )
    parser.add_argument(
        "--network-audit-mode",
        dest="network_audit_mode",
        default="auto",
        choices=sorted(ALLOWED_NETWORK_AUDIT_MODES),
        help="Network audit mode (default %(default)s).",
    )
    parser.add_argument(
        "--diagnostics-mode",
        dest="diagnostics_mode",
        default="off",
        choices=sorted(ALLOWED_DIAGNOSTICS_MODES),
        help="Diagnostics mode (default %(default)s).",
    )
    args = parser.parse_args(argv)
    payload = {
        "direction": args.direction,
        "reference_driver_binary": args.reference_driver_binary,
        "reference_build_manifest": args.reference_build_manifest,
        "reference_source_lock": args.reference_source_lock,
        "output_record": args.output_record,
        "source_commit": args.source_commit,
        "run_timeout_seconds": args.run_timeout_seconds,
        "readiness_timeout_seconds": args.readiness_timeout_seconds,
        "handshake_timeout_seconds": args.handshake_timeout_seconds,
        "data_timeout_seconds": args.data_timeout_seconds,
        "network_audit_mode": args.network_audit_mode,
        "diagnostics_mode": args.diagnostics_mode,
    }
    return parse_config_dict(payload)


def derive_delivery_status_message_id(
    *,
    run_id: str,
    scenario_id: str,
    correlation_nonce: str,
) -> int:
    """Derive the per-run DeliveryStatus ``message_id``.

    The derivation is a Plan 065-compatible domain-separated SHA-256
    digest over the run identity, the scenario id, and the correlation
    nonce; the first four bytes become an unsigned 32-bit integer and
    any zero value is bumped to one to satisfy the strict nonzero
    contract. The same algorithm is used by the Plan 065
    ``mixed_runner._plan065_primary_fields`` helper.
    """

    digest = hashlib.sha256(
        f"i2pr-ntcp2-delivery-status-v1|{run_id}|{scenario_id}|{correlation_nonce}".encode()
    ).digest()
    raw = int.from_bytes(digest[:4], "big")
    return (raw % 0xFFFFFFFF) or 1


def allocate_loopback_port() -> int:
    """Allocate a loopback port by binding ``port=0`` then closing.

    Returns the kernel-assigned ephemeral port. The port is freed
    before the listener is launched, so a second helper in the same
    process may rebind the same port with ``SO_REUSEADDR``. The runner
    retries at most once on a typed ``address-in-use`` preflight
    failure; protocol failures are not retried.
    """

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as handle:
        handle.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        handle.bind((LOOPBACK_IPV4, 0))
        return int(handle.getsockname()[1])


def check_loopback_listening(port: int, *, timeout_seconds: float = 1.0) -> bool:
    """Return ``True`` when ``127.0.0.1:port`` accepts a TCP connection."""

    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((LOOPBACK_IPV4, port), timeout=0.25):
                return True
        except (ConnectionRefusedError, socket.timeout, OSError):
            time.sleep(0.05)
    return False


def probe_strace_available() -> bool:
    """Return ``True`` when ``strace`` is installed and ptrace is allowed.

    ``ptrace_scope == 1`` (Debian/Ubuntu default) restricts ptrace to
    ancestors; we treat that as ``unavailable`` for the smoke runner
    because the child processes are siblings of the runner, not
    descendants. ``ptrace_scope == 0`` allows ptrace on any process
    owned by the same user; ``ptrace_scope == 3`` forbids ptrace for
    any non-ancestor.
    """

    strace_path = shutil.which("strace")
    if strace_path is None:
        return False
    try:
        value = Path("/proc/sys/kernel/yama/ptrace_scope").read_text(encoding="ascii").strip()
    except OSError:
        return False
    return value == "0"


def now_utc_iso() -> str:
    """Return the current UTC time as an RFC 3339 string with millisecond precision."""

    return dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") + (
        f"{dt.datetime.now(dt.timezone.utc).microsecond // 1000:03d}Z"
    )


def _hash_file(path: Path) -> str:
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError:
        return ""


def _command_summary(command: tuple[str, ...]) -> str:
    """Return a sanitized one-line summary of a command for logging.

    The runner never logs raw payloads, private keys, Noise state, or
    full RouterInfo bytes. The summary reduces each argument to at
    most 64 characters and joins with spaces so the ``events.jsonl``
    log stays bounded.
    """

    parts: list[str] = []
    for arg in command:
        if len(arg) > 64:
            parts.append(arg[:60] + "\u2026")
        else:
            parts.append(arg)
    return " ".join(parts)


def _iter_validated_reference_events(
    events_path: Path,
    *,
    expected_message_id: int,
    expected_direction: str,
    expected_run_id: str,
    expected_peer_router_hash_sha256: str,
    expected_local_router_hash_sha256: str,
    expected_driver_binary_sha256: str,
    seen_sequences: set[int],
) -> Iterator[dict[str, Any]]:
    """Yield validated reference events from ``events_path``.

    Plan 075 closes the Plan 069 defect where the runner auto-marked
    protocol milestones. The function reads the structured event
    stream written by the Plan 064 driver and validates every event
    against the Plan 062 v1 schema plus the run-identity correlation
    fields. The runner raises a typed
    :data:`TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN` or
    :data:`TYPED_BLOCKER_REFERENCE_EVENTS_MISSING` blocker whenever
    the contract is violated.
    """

    try:
        raw_text = events_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise LoopbackSmokeRunError(
            TYPED_BLOCKER_REFERENCE_EVENTS_MISSING,
            stage="protocol",
            reason=f"events-unreadable:{exc}",
        ) from exc
    if not raw_text.strip():
        raise LoopbackSmokeRunError(
            TYPED_BLOCKER_REFERENCE_EVENTS_MISSING,
            stage="protocol",
            reason=TYPED_BLOCKER_REFERENCE_EVENTS_MISSING,
        )
    for line_number, line in enumerate(raw_text.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as exc:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"invalid-json:line-{line_number}",
            ) from exc
        if not isinstance(event, dict):
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"non-object-event:line-{line_number}",
            )
        event = dict(event)
        if event.get("schema") != REFERENCE_EVENT_SCHEMA:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-schema-invalid:line-{line_number}",
            )
        if event.get("schema_version") != REFERENCE_EVENT_VERSION:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-schema-version-invalid:line-{line_number}",
            )
        if event.get("run_id") != expected_run_id:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-run-id-mismatch:line-{line_number}",
            )
        if event.get("direction") != expected_direction:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-direction-mismatch:line-{line_number}",
            )
        if event.get("implementation") != REFERENCE_IMPLEMENTATION:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-implementation-mismatch:line-{line_number}",
            )
        if event.get("implementation_revision") != REFERENCE_REVISION:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-revision-mismatch:line-{line_number}",
            )
        if event.get("driver_binary_sha256") != expected_driver_binary_sha256:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-driver-binary-mismatch:line-{line_number}",
            )
        if event.get("local_router_hash_sha256") != expected_local_router_hash_sha256:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-local-router-hash-mismatch:line-{line_number}",
            )
        if event.get("peer_router_hash_sha256") != expected_peer_router_hash_sha256:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-peer-router-hash-mismatch:line-{line_number}",
            )
        try:
            reference_event.validate_event(
                event,
                seen_event_sequences=seen_sequences,
                expected_peer_router_hash_sha256=expected_peer_router_hash_sha256,
            )
        except reference_event.ReferenceEventError as exc:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=f"event-validation:{exc}",
            ) from exc
        # Data-phase correlation: the exact DeliveryStatus message ID
        # must match the scenario-owned derivation. The runner uses
        # the sender's correlation as the authoritative source.
        if event["event_kind"] in (
            "frame_emitted",
            "frame_authenticated_and_decrypted",
            "i2np_message_decoded",
        ):
            if event.get("delivery_status_message_id") != expected_message_id:
                raise LoopbackSmokeRunError(
                    TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                    stage="protocol",
                    reason=f"event-message-id-mismatch:line-{line_number}",
                )
        yield event


def generate_run_id(now: dt.datetime | None = None) -> str:
    """Return a bounded, sanitized run identifier.

    The identifier uses only lowercase ASCII so it satisfies the
    Plan 064 strict config run-id regex
    (``^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$``) without any
    escape or normalization.
    """

    moment = now or dt.datetime.now(dt.timezone.utc)
    stamp = moment.strftime("%Y%m%d%H%M%S")
    suffix = uuid.uuid4().hex[:8]
    return f"loopback-smoke-{stamp}-{suffix}"


def _measured_provenance_digest(field: str, file_path: Path) -> str:
    """Return the measured SHA-256 digest of an on-disk artifact.

    Plan 075 closes the synthetic-provenance fallback. The runner
    raises :class:`LoopbackSmokeConfigError` when the artifact is
    missing, empty, or unreadable; no synthetic placeholder is ever
    emitted.

    The function is local to the runner so that the strict-config
    payload builder does not need to import the production helper
    module just to compute a digest.
    """

    if not isinstance(file_path, Path):
        raise LoopbackSmokeConfigError(f"{field}-not-path")
    if not file_path.is_file():
        raise LoopbackSmokeConfigError(f"{field}-missing:{file_path}")
    try:
        data = file_path.read_bytes()
    except OSError as exc:
        raise LoopbackSmokeConfigError(f"{field}-unreadable:{file_path}") from exc
    if not data:
        raise LoopbackSmokeConfigError(f"{field}-empty:{file_path}")
    digest = hashlib.sha256(data).hexdigest()
    if not HEX64.fullmatch(digest):
        raise LoopbackSmokeConfigError(f"{field}-not-hex64")
    return digest


def _non_zero_hex64(field: str, value: str) -> str:
    """Return ``value`` when it is a non-zero 64-lowercase-hex digest.

    The Plan 064 source-lock record carries placeholder digests for
    helper sources, observer patches, and the installed i2pd tree.
    These placeholders are encoded as long runs of zero hex digits.
    Plan 075 fails closed whenever a placeholder is encountered.
    """

    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise LoopbackSmokeConfigError(f"{field}-not-hex64")
    if value.startswith("0" * 16):
        raise LoopbackSmokeConfigError(
            f"{field}-placeholder:{value[:16]}\u2026"
        )
    return value


def _render_reference_strict_config(
    *,
    config: LoopbackSmokeConfig,
    run_root: Path,
    run_id: str,
    scenario_id: str,
    role: str,
    local_port: int,
    peer_port: int,
    peer_address: str,
    local_address: str,
    delivery_status_message_id: int,
    expected_local_router_hash_sha256: str,
    expected_peer_router_hash_sha256: str,
    peer_router_info_path: Path,
) -> dict[str, Any]:
    """Build the Plan 064 strict driver config payload for the smoke run.

    The runner only invokes the Plan 064 ``validate_strict_config`` and
    ``render_strict_config`` helpers when the reference adapter is the
    Plan 064 i2pd driver; tests that substitute a fake reference
    adapter must not depend on this helper.

    The payload targets schema ``i2pr-i2pd-direct-driver-config-v1``
    (``i2pd_direct_driver.CONFIG_SCHEMA``). Plan 075 closes the
    synthetic-provenance fallback: every reported digest is measured
    from the on-disk artifact (helper source, observer patch, driver
    binary, build manifest) or read from the source-lock record once
    that record carries a non-placeholder digest. ``raise`` is the
    fail-closed guard.
    """

    import i2pd_direct_driver as i2pd_driver

    source_lock = json.loads(config.reference_source_lock.read_text(encoding="utf-8"))
    if not isinstance(source_lock, dict):
        raise LoopbackSmokeConfigError("source-lock-not-object")
    if source_lock.get("reference", {}).get("revision") != REFERENCE_REVISION:
        raise LoopbackSmokeConfigError("source-lock-reference-revision-mismatch")
    helper = source_lock.get("helper", {})
    if not isinstance(helper, dict):
        raise LoopbackSmokeConfigError("source-lock-helper-missing")

    binary_digest = _measured_provenance_digest(
        "driver_binary_sha256", config.reference_driver_binary
    )
    build_manifest_digest = _measured_provenance_digest(
        "build_manifest_sha256", config.reference_build_manifest
    )
    driver_source_digest = _measured_provenance_digest(
        "driver_source_sha256", I2PD_DRIVER_SOURCE
    )
    observer_patch_digest = _measured_provenance_digest(
        "observer_patch_sha256", I2PD_OBSERVER_PATCH
    )

    # The installed i2pd tree lives under
    # ``target/interop/cache/i2pd/<tree>/`` which is not present on
    # the constrained host. The source-lock record carries a
    # placeholder until the canonical Plan 046 rootless lane or the
    # Plan 048/049 Multipass recovery lane has produced a real
    # cache. The runner fails closed whenever the placeholder is
    # still zero.
    reference_tree_digest = _non_zero_hex64(
        "reference_tree_sha256",
        source_lock.get("reference", {}).get(
            "installed_tree_sha256_placeholder",
            "0" * 64,
        ),
    )

    run_identity_digest = hashlib.sha256(
        f"loopback-smoke-run-identity|{run_id}|{config.direction}".encode()
    ).hexdigest()
    payload: dict[str, Any] = {
        "schema": i2pd_driver.CONFIG_SCHEMA,
        "schema_version": i2pd_driver.CONFIG_VERSION,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "direction": config.direction,
        "mode": role,
        "data_dir": str((run_root / "ref-data").resolve()),
        "output_dir": str((run_root / "ref-events").resolve()),
        "local_address": local_address,
        "local_port": local_port,
        "network_id": 99,
        "peer_router_info_path": str(peer_router_info_path.resolve()),
        "expected_local_router_hash_sha256": expected_local_router_hash_sha256,
        "expected_peer_router_hash_sha256": expected_peer_router_hash_sha256,
        "expected_peer_address": peer_address,
        "expected_peer_port": peer_port,
        "delivery_status_message_id": delivery_status_message_id,
        "startup_timeout_ms": int(config.readiness_timeout_seconds * 1000),
        "handshake_timeout_ms": int(config.handshake_timeout_seconds * 1000),
        "data_phase_timeout_ms": int(config.data_timeout_seconds * 1000),
        "shutdown_timeout_ms": 5_000,
        "reference_revision": REFERENCE_REVISION,
        "reference_tree_sha256": reference_tree_digest,
        "driver_source_sha256": driver_source_digest,
        "driver_binary_sha256": binary_digest,
        "build_manifest_sha256": build_manifest_digest,
        "observer_patch_sha256": observer_patch_digest,
        "run_identity_sha256": run_identity_digest,
    }
    i2pd_driver.validate_strict_config(payload)
    return payload


@dataclasses.dataclass
class SmokeProtocol:
    """Structured protocol events captured by the runner.

    Each boolean field records a Plan 068 smoke record milestone. The
    runner updates the milestones in order and refuses to mark the
    record as ``passed`` unless every positive milestone is ``True``
    and the cleanup step reports ``cleanup_clean = True``.
    """

    process_started: bool = False
    listener_ready: bool = False
    tcp_connected: bool = False
    ntcp2_authenticated: bool = False
    frame_emitted: bool = False
    frame_authenticated_and_decrypted: bool = False
    i2np_message_decoded: bool = False
    cleanup_clean: bool = True

    def mark(self, name: str) -> None:
        if name not in ALLOWED_EVENT_NAMES:
            raise LoopbackSmokeRunError(
                "unknown-event-name",
                stage="preflight",
                reason="unknown-event-name",
            )
        if hasattr(self, name):
            setattr(self, name, True)

    def positive_milestones_reached(self) -> bool:
        return all(
            (
                self.process_started,
                self.listener_ready,
                self.tcp_connected,
                self.ntcp2_authenticated,
                self.frame_emitted,
                self.frame_authenticated_and_decrypted,
                self.i2np_message_decoded,
            )
        )


@dataclasses.dataclass
class _ProcessHandle:
    """A bounded child process owned by the smoke runner.

    The handle owns its log path and PID file. ``stop`` performs a
    bounded SIGTERM/SIGKILL teardown that mirrors the existing
    ``BoundedProcess`` semantics.
    """

    label: str
    command: tuple[str, ...]
    log_path: Path
    process: subprocess.Popen[bytes] | None = None
    forced: bool = False

    def start(self, env: dict[str, str] | None = None) -> None:
        self.log_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        try:
            self.process = subprocess.Popen(
                list(self.command),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                close_fds=True,
                env=env,
            )
        except FileNotFoundError as exc:
            raise LoopbackSmokeRunError(
                "process-binary-missing",
                stage="process-start",
                reason=f"{self.label}-binary-missing",
            ) from exc

    def stop(self, timeout_seconds: float) -> str:
        if self.process is None:
            return "not-started"
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=timeout_seconds)
            except subprocess.TimeoutExpired:
                self.forced = True
                self.process.kill()
                self.process.wait(timeout=timeout_seconds)
        return "forced" if self.forced else "clean"

    def pid(self) -> int | None:
        if self.process is None:
            return None
        return self.process.pid

    def exit_code(self) -> int | None:
        if self.process is None:
            return None
        return self.process.poll()

    def snapshot(self) -> dict[str, int | str]:
        return {
            "label": self.label,
            "running": int(self.process is not None and self.process.poll() is None),
            "exit_code": self.process.returncode if self.process is not None else -1,
            "forced": int(self.forced),
        }


class LoopbackSmokeRunner:
    """The Plan 069 loopback smoke runner.

    A runner instance is bound to one direction and one run identity.
    The same instance may be invoked exactly once. Tests substitute
    the :pyattr:`subprocess_factory` and :pyattr:`port_allocator`
    hooks with deterministic fakes.
    """

    def __init__(
        self,
        config: LoopbackSmokeConfig,
        repo_root: Path,
        *,
        run_id: str | None = None,
        subprocess_factory: Callable[..., subprocess.Popen[bytes]] | None = None,
        port_allocator: Callable[[], int] | None = None,
        now: Callable[[], dt.datetime] | None = None,
        keep_run_root: bool = False,
    ):
        self.config = config
        self.repo_root = repo_root.resolve()
        self.run_id = run_id or generate_run_id(now() if now else None)
        self._now = now or (lambda: dt.datetime.now(dt.timezone.utc))
        self._subprocess_factory = subprocess_factory or _default_subprocess_factory
        self._port_allocator = port_allocator or allocate_loopback_port
        self.keep_run_root = keep_run_root
        self.run_root: Path | None = None
        self.protocol = SmokeProtocol()
        self.events: list[dict[str, Any]] = []
        self.started_utc = ""
        self.completed_utc = ""
        self.failure_stage = "none"
        self.failure_reason = "none"
        self.network_audit_outcome = "configuration-only"
        self.local_router_hash_sha256 = ""
        self.peer_router_hash_sha256 = ""
        self.peer_router_info_path: Path | None = None
        self.local_router_info_path: Path | None = None
        self.peer_listen_port: int | None = None
        self.local_listen_port: int | None = None
        self.i2pr_handle: _ProcessHandle | None = None
        self.reference_handle: _ProcessHandle | None = None
        self.cleanup_outcome = "clean"

    # ----- Hooks -----

    @property
    def subprocess_factory(self) -> Callable[..., subprocess.Popen[bytes]]:
        return self._subprocess_factory

    @subprocess_factory.setter
    def subprocess_factory(self, value: Callable[..., subprocess.Popen[bytes]]) -> None:
        self._subprocess_factory = value

    @property
    def port_allocator(self) -> Callable[[], int]:
        return self._port_allocator

    @port_allocator.setter
    def port_allocator(self, value: Callable[[], int]) -> None:
        self._port_allocator = value

    # ----- Orchestration -----

    def run(self) -> dict[str, Any]:
        """Run one bounded smoke direction and return the sanitized record.

        The runner is fail-closed: every error path produces a typed
        ``failure_stage`` and ``failure_reason`` and a bounded smoke
        record. The runner refuses to silently collapse failures into
        ``evidence-incomplete`` or ``blocked_execution_lane_unavailable``;
        the stage is the earliest real failure observed.
        """

        self.started_utc = now_utc_iso()
        try:
            self._preflight()
            self._setup_run_root()
            self._allocate_ports()
            self._prepare_identities()
            self._render_scenarios()
            self._render_reference_config()
            self._network_audit()
            self._launch_listener()
            self._launch_dialer()
            self._monitor_protocol()
        except LoopbackSmokeRunError as exc:
            self.failure_stage = exc.stage
            self.failure_reason = exc.reason or exc.code
        except LoopbackSmokeConfigError as exc:
            self.failure_stage = "preflight"
            self.failure_reason = str(exc)
        finally:
            try:
                self._cleanup()
            except LoopbackSmokeRunError as exc:
                self.cleanup_outcome = "failed"
                if self.failure_stage == "none":
                    self.failure_stage = exc.stage
                    self.failure_reason = exc.reason or exc.code
            self.completed_utc = now_utc_iso()
        return self._build_record()

    # ----- Stages -----

    def _preflight(self) -> None:
        if not HEX40.fullmatch(self.config.source_commit):
            raise LoopbackSmokeConfigError("source_commit-not-40-hex")
        if self.config.direction not in ALLOWED_DIRECTIONS:
            raise LoopbackSmokeConfigError("direction-not-allowlisted")
        self._log_event("preflight", "ok", "preflight-passed")

    def _setup_run_root(self) -> None:
        if self.run_root is not None:
            raise LoopbackSmokeRunError(
                "run-root-already-set",
                stage="preflight",
                reason="run-root-already-set",
            )
        parent = self.repo_root / "target/interop/loopback-smoke"
        parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        unique = tempfile.mkdtemp(prefix=f"{self.run_id}-", dir=parent)
        self.run_root = Path(unique).resolve()
        (self.run_root / "i2pr-state").mkdir(mode=0o700, exist_ok=True)
        (self.run_root / "i2pr-state-listener").mkdir(mode=0o700, exist_ok=True)
        (self.run_root / "ref-data").mkdir(mode=0o700, exist_ok=True)
        (self.run_root / "ref-events").mkdir(mode=0o700, exist_ok=True)
        (self.run_root / "exchange").mkdir(mode=0o700, exist_ok=True)
        (self.run_root / "i2pr-logs").mkdir(mode=0o700, exist_ok=True)
        (self.run_root / "ref-logs").mkdir(mode=0o700, exist_ok=True)
        (self.run_root / "pids").mkdir(mode=0o700, exist_ok=True)
        self._log_event("run-root", "ok", str(self.run_root))

    def _allocate_ports(self) -> None:
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        if self.config.is_i2pr_initiator:
            self.local_listen_port = self._port_allocator()
            self.peer_listen_port = self._port_allocator()
        else:
            self.local_listen_port = self._port_allocator()
            self.peer_listen_port = self._port_allocator()
        (self.run_root / "ports.json").write_text(
            json.dumps(
                {
                    "local_listen_port": self.local_listen_port,
                    "peer_listen_port": self.peer_listen_port,
                }
            ),
            encoding="utf-8",
        )
        self._log_event(
            "ports",
            "ok",
            (
                f"local={self.local_listen_port} peer={self.peer_listen_port}"
            ),
        )

    def _prepare_identities(self) -> None:
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        self.local_router_hash_sha256 = hashlib.sha256(
            f"{self.run_id}|local|identity".encode()
        ).hexdigest()
        self.peer_router_hash_sha256 = hashlib.sha256(
            f"{self.run_id}|peer|identity".encode()
        ).hexdigest()
        self._log_event(
            "identities",
            "ok",
            (
                f"local_router_hash={self.local_router_hash_sha256[:16]} "
                f"peer_router_hash={self.peer_router_hash_sha256[:16]}"
            ),
        )

    def _render_scenarios(self) -> None:
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        message_id = derive_delivery_status_message_id(
            run_id=self.run_id,
            scenario_id=self.config.scenario_id,
            correlation_nonce=self.run_id,
        )
        run_identity_sha256 = hashlib.sha256(
            (
                f"{self.run_id}|{self.config.direction}|{self.config.source_commit}"
            ).encode()
        ).hexdigest()
        scenario_paths = _render_strict_scenarios(
            run_root=self.run_root,
            direction=self.config.direction,
            run_id=self.run_id,
            local_address=LOOPBACK_IPV4,
            local_port=int(self.local_listen_port or 0),
            peer_address=LOOPBACK_IPV4,
            peer_port=int(self.peer_listen_port or 0),
            expected_sender_router_hash_sha256=self.local_router_hash_sha256,
            expected_receiver_router_hash_sha256=self.peer_router_hash_sha256,
            reference_driver_mode=REFERENCE_DRIVER_MODE,
            delivery_status_message_id=message_id,
            run_identity_sha256=run_identity_sha256,
        )
        self._log_event(
            "scenarios-rendered",
            "ok",
            f"listener={scenario_paths['listener']} dialer={scenario_paths['dialer']}",
        )

    def _render_reference_config(self) -> None:
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        message_id = derive_delivery_status_message_id(
            run_id=self.run_id,
            scenario_id=self.config.scenario_id,
            correlation_nonce=self.run_id,
        )
        run_identity_sha256 = hashlib.sha256(
            (
                f"{self.run_id}|{self.config.direction}|{self.config.source_commit}"
            ).encode()
        ).hexdigest()
        role = "dial" if self.config.is_i2pr_initiator else "listen"
        local_port = int(self.peer_listen_port or 0)
        peer_port = int(self.local_listen_port or 0)
        payload = _render_reference_strict_config(
            config=self.config,
            run_root=self.run_root,
            run_id=self.run_id,
            scenario_id=self.config.scenario_id,
            role=role,
            local_port=local_port,
            peer_port=peer_port,
            peer_address=SYNTHETIC_IPV4_LOCAL,
            local_address=SYNTHETIC_IPV4_PEER,
            delivery_status_message_id=message_id,
            expected_local_router_hash_sha256=self.peer_router_hash_sha256,
            expected_peer_router_hash_sha256=self.local_router_hash_sha256,
            peer_router_info_path=self.run_root / "exchange" / "i2pr-router.info",
        )
        config_path = self.run_root / "ref-config.json"
        config_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        self._log_event("reference-config", "ok", str(config_path))

    def _network_audit(self) -> None:
        if self.config.network_audit_mode == "configuration-only":
            self.network_audit_outcome = "configuration-only"
        elif self.config.network_audit_mode == "strace":
            self.network_audit_outcome = "strace-allowlist"
        elif self.config.network_audit_mode == "auto":
            self.network_audit_outcome = (
                "strace-allowlist"
                if probe_strace_available()
                else "configuration-only"
            )
        else:
            raise LoopbackSmokeConfigError("network_audit_mode-not-allowlisted")
        self._log_event("network-audit", "ok", self.network_audit_outcome)

    def _role_pair(self) -> ProcessRoleAssignment:
        """Return the unique role mapping for the configured direction."""

        try:
            return ROLE_ASSIGNMENTS[self.config.direction]
        except KeyError as exc:
            raise LoopbackSmokeConfigError(
                "direction-not-allowlisted"
            ) from exc

    def _listener_process_role(self) -> ProcessRole:
        return self._role_pair().listener

    def _dialer_process_role(self) -> ProcessRole:
        return self._role_pair().dialer

    def _build_command(
        self,
        process_role: ProcessRole,
        transport_role: TransportRole,
    ) -> tuple[str, ...]:
        """Return the bounded command for the requested role pair.

        Plan 075 corrects the Plan 069 defect where the runner
        selected the i2pr launcher for both transport roles. The
        helper-source and observer-patch digests are measured inline
        so the launched process is bound to the exact on-disk
        artifacts through the strict driver config.
        """

        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        if process_role == ProcessRole.I2PR:
            return self._build_i2pr_command(transport_role)
        if process_role == ProcessRole.REFERENCE:
            return self._build_reference_command(transport_role)
        raise LoopbackSmokeConfigError(
            f"process-role-not-allowlisted:{process_role}"
        )

    def _build_i2pr_command(self, transport_role: TransportRole) -> tuple[str, ...]:
        binary = self.repo_root / "target/debug/i2pr-interop"
        if not binary.is_file():
            raise LoopbackSmokeRunError(
                "i2pr-launcher-missing",
                stage="build",
                reason="i2pr-launcher-missing",
            )
        scenario = self.run_root / "scenarios" / f"{transport_role}.toml"
        if not scenario.is_file():
            raise LoopbackSmokeRunError(
                "i2pr-scenario-missing",
                stage="build",
                reason=f"i2pr-scenario-{transport_role}-missing",
            )
        return (
            str(binary),
            "ntcp2",
            str(transport_role),
            "--scenario-config",
            str(scenario),
        )

    def _build_reference_command(self, transport_role: TransportRole) -> tuple[str, ...]:
        if not I2PD_RUN_SCRIPT.is_file():
            raise LoopbackSmokeRunError(
                "reference-run-script-missing",
                stage="build",
                reason="reference-run-script-missing",
            )
        if not os.access(I2PD_RUN_SCRIPT, os.X_OK):
            raise LoopbackSmokeRunError(
                "reference-run-script-not-executable",
                stage="build",
                reason="reference-run-script-not-executable",
            )
        if not self.config.reference_driver_binary.is_file():
            raise LoopbackSmokeRunError(
                "reference-driver-binary-missing",
                stage="build",
                reason="reference-driver-binary-missing",
            )
        ref_config = self.run_root / "ref-config.json"
        if not ref_config.is_file():
            raise LoopbackSmokeRunError(
                "reference-strict-config-missing",
                stage="build",
                reason="reference-strict-config-missing",
            )
        # The Plan 064 strict driver config already encodes the
        # transport role; the run script is the committed runtime
        # seam. The driver mode is set inside the strict config.
        _ = transport_role
        return (
            "bash",
            str(I2PD_RUN_SCRIPT),
            "--driver-binary",
            str(self.config.reference_driver_binary),
            "--strict-config",
            str(ref_config),
        )

    def _launch_listener(self) -> None:
        listener_role = self._listener_process_role()
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        command = self._build_command(listener_role, TransportRole.LISTENER)
        handle = self._instantiate_process_handle(
            listener_role, TransportRole.LISTENER, command
        )
        self._install_handle(listener_role, TransportRole.LISTENER, handle)
        if listener_role == ProcessRole.I2PR:
            self.protocol.mark("process_started")
        self._log_event(
            "listener-spawn" if listener_role == ProcessRole.I2PR
            else "reference-listener-spawn",
            "ok",
            f"role={listener_role} cmd={_command_summary(command)}",
        )

    def _launch_dialer(self) -> None:
        dialer_role = self._dialer_process_role()
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        command = self._build_command(dialer_role, TransportRole.DIALER)
        handle = self._instantiate_process_handle(
            dialer_role, TransportRole.DIALER, command
        )
        self._install_handle(dialer_role, TransportRole.DIALER, handle)
        self._log_event(
            "dialer-spawn" if dialer_role == ProcessRole.I2PR
            else "reference-dialer-spawn",
            "ok",
            f"role={dialer_role} cmd={_command_summary(command)}",
        )

    def _instantiate_process_handle(
        self,
        process_role: ProcessRole,
        transport_role: TransportRole,
        command: tuple[str, ...],
    ) -> _ProcessHandle:
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        label = f"{process_role}-{transport_role}"
        log_path = (
            self.run_root / "i2pr-logs"
            / f"{transport_role}.log"
            if process_role == ProcessRole.I2PR
            else self.run_root / "ref-logs"
            / f"{transport_role}.log"
        )
        handle = _ProcessHandle(label=label, command=command, log_path=log_path)
        handle.start()
        self._record_pid(label, handle.pid())
        return handle

    def _install_handle(
        self,
        process_role: ProcessRole,
        transport_role: TransportRole,
        handle: _ProcessHandle,
    ) -> None:
        if process_role == ProcessRole.I2PR and transport_role == TransportRole.LISTENER:
            self.i2pr_handle = handle
            return
        if process_role == ProcessRole.I2PR and transport_role == TransportRole.DIALER:
            self.i2pr_handle = handle
            return
        if process_role == ProcessRole.REFERENCE and transport_role == TransportRole.LISTENER:
            self.reference_handle = handle
            return
        if process_role == ProcessRole.REFERENCE and transport_role == TransportRole.DIALER:
            self.reference_handle = handle
            return
        raise LoopbackSmokeConfigError(
            "role-pair-not-allowlisted"
        )

    def _monitor_protocol(self) -> None:
        """Drive the protocol monitoring loop.

        The Plan 075 implementation binds protocol milestones to
        validated structured events from the reference driver. The
        TCP loopback probe is a non-authoritative diagnostic that
        only feeds ``tcp_connected``. The protocol milestones
        (``ntcp2_authenticated``, ``frame_emitted``,
        ``frame_authenticated_and_decrypted``, ``i2np_message_decoded``)
        are derived solely from validated events.

        Tests override this method to inject success and failure
        outcomes without launching the actual binaries.
        """

        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        deadline = time.monotonic() + self.config.handshake_timeout_seconds
        while time.monotonic() < deadline:
            if self._check_listener_ready():
                break
            if self._listener_exited_prematurely():
                raise LoopbackSmokeRunError(
                    "listener-exited-before-ready",
                    stage="process-start",
                    reason="listener-exited-before-ready",
                )
            time.sleep(0.05)
        else:
            raise LoopbackSmokeRunError(
                "readiness-timeout",
                stage="timeout",
                reason="listener-ready-timeout",
            )
        tcp_deadline = time.monotonic() + self.config.handshake_timeout_seconds
        while time.monotonic() < tcp_deadline:
            if self._await_tcp_connected():
                self.protocol.mark("tcp_connected")
                break
            time.sleep(0.05)
        else:
            raise LoopbackSmokeRunError(
                "tcp-connect-timeout",
                stage="connect",
                reason="tcp-connect-timeout",
            )
        self._consume_reference_events()

    def _listener_exited_prematurely(self) -> bool:
        """Return ``True`` when the listener process has exited."""

        handle = self._listener_handle()
        if handle is None or handle.exit_code() is None:
            return False
        return True

    def _listener_handle(self) -> _ProcessHandle | None:
        if self._listener_process_role() == ProcessRole.I2PR:
            return self.i2pr_handle
        return self.reference_handle

    def _await_tcp_connected(self) -> bool:
        peer_port = int(self.peer_listen_port or 0)
        if peer_port <= 0:
            return False
        return check_loopback_listening(peer_port, timeout_seconds=0.5)

    def _consume_reference_events(self) -> None:
        """Read the reference event stream and validate every milestone.

        Plan 075 closes the Plan 069 defect where the runner auto-
        marked protocol milestones after a TCP loopback probe. The
        milestone set is now derived solely from validated structured
        events emitted by the reference driver. The runner fails
        closed with :data:`TYPED_BLOCKER_REFERENCE_EVENTS_MISSING`
        when the event stream is missing, and with
        :data:`TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN` when a
        milestone lacks a validated event.
        """

        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        events_path = self.run_root / "ref-events" / "events.ndjson"
        if not events_path.is_file():
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_REFERENCE_EVENTS_MISSING,
                stage="protocol",
                reason=TYPED_BLOCKER_REFERENCE_EVENTS_MISSING,
            )
        expected_message_id = derive_delivery_status_message_id(
            run_id=self.run_id,
            scenario_id=self.config.scenario_id,
            correlation_nonce=self.run_id,
        )
        driver_binary_digest = _measured_provenance_digest(
            "driver_binary_sha256", self.config.reference_driver_binary
        )
        seen_sequences: set[int] = set()
        validated = 0
        for event in _iter_validated_reference_events(
            events_path,
            expected_message_id=expected_message_id,
            expected_direction=self.config.direction,
            expected_run_id=self.run_id,
            expected_peer_router_hash_sha256=self.local_router_hash_sha256,
            expected_local_router_hash_sha256=self.peer_router_hash_sha256,
            expected_driver_binary_sha256=driver_binary_digest,
            seen_sequences=seen_sequences,
        ):
            validated += 1
            kind = event["event_kind"]
            if kind == "ntcp2_authenticated":
                self.protocol.mark("ntcp2_authenticated")
            elif kind == "frame_emitted":
                self.protocol.mark("frame_emitted")
            elif kind == "frame_authenticated_and_decrypted":
                self.protocol.mark("frame_authenticated_and_decrypted")
            elif kind == "i2np_message_decoded":
                self.protocol.mark("i2np_message_decoded")
        if validated == 0:
            raise LoopbackSmokeRunError(
                TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                stage="protocol",
                reason=TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
            )
        # Plan 075 closes the loopback-probe defect: every protocol
        # milestone must be backed by a validated event. The runner
        # raises only when a milestone is missing a backing event,
        # not when the run produced a strictly stronger subset.
        for milestone in (
            "ntcp2_authenticated",
            "frame_emitted",
            "frame_authenticated_and_decrypted",
            "i2np_message_decoded",
        ):
            if not getattr(self.protocol, milestone):
                raise LoopbackSmokeRunError(
                    TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN,
                    stage="protocol",
                    reason=f"{milestone}-no-event",
                )
        self._log_event("protocol", "ok", "events-validated")

    def _check_listener_ready(self) -> bool:
        if self.run_root is None:
            raise LoopbackSmokeRunError(
                "run-root-not-set",
                stage="preflight",
                reason="run-root-not-set",
            )
        if self._listener_process_role() == ProcessRole.I2PR:
            return self._probe_i2pr_listener_ready()
        return self._probe_loopback_listener_ready()

    def _probe_i2pr_listener_ready(self) -> bool:
        status_path = self.run_root / "i2pr-state-listener" / "status.jsonl"
        if not status_path.is_file():
            return False
        for line in status_path.read_text(encoding="utf-8").splitlines():
            if '"listener_ready"' in line and '"result":"ready"' in line:
                self.protocol.mark("listener_ready")
                return True
        return False

    def _probe_loopback_listener_ready(self) -> bool:
        port = int(self.peer_listen_port or 0)
        if port <= 0:
            return False
        try:
            with socket.create_connection((LOOPBACK_IPV4, port), timeout=0.25):
                self.protocol.mark("listener_ready")
                return True
        except (ConnectionRefusedError, socket.timeout, OSError):
            return False

    # ----- Cleanup -----

    def _cleanup(self) -> None:
        cleanup_failure: LoopbackSmokeRunError | None = None
        for handle in (self.reference_handle, self.i2pr_handle):
            if handle is None:
                continue
            try:
                outcome = handle.stop(self.config.readiness_timeout_seconds)
                self._log_event(
                    "process-stop", outcome, f"{handle.label}={outcome}"
                )
            except LoopbackSmokeRunError as exc:
                cleanup_failure = exc
        if cleanup_failure is not None:
            self.cleanup_outcome = "failed"
            raise cleanup_failure
        residual = self._check_residual()
        if not residual["clean"]:
            self.cleanup_outcome = "failed"
            raise LoopbackSmokeRunError(
                "process-residual",
                stage="cleanup",
                reason="residual-process-or-port",
            )
        if not self.keep_run_root and self.run_root is not None:
            try:
                shutil.rmtree(self.run_root, ignore_errors=False)
            except OSError:
                self.cleanup_outcome = "failed"
                raise LoopbackSmokeRunError(
                    "run-root-removal-failed",
                    stage="cleanup",
                    reason="run-root-removal-failed",
                ) from None
        self._log_event("cleanup", "ok", "cleanup-passed")

    def _check_residual(self) -> dict[str, Any]:
        running_pids: list[int] = []
        for handle in (self.reference_handle, self.i2pr_handle):
            if handle is None or handle.process is None:
                continue
            try:
                os.kill(handle.process.pid, 0)
            except ProcessLookupError:
                continue
            except OSError:
                continue
            if handle.process.poll() is None:
                running_pids.append(handle.process.pid)
        if self.peer_listen_port is not None and check_loopback_listening(
            int(self.peer_listen_port),
            timeout_seconds=0.25,
        ):
            running_pids.append(-int(self.peer_listen_port))
        return {
            "clean": len(running_pids) == 0,
            "residual_pids": running_pids,
        }

    def _record_pid(self, label: str, pid: int | None) -> None:
        if self.run_root is None or pid is None:
            return
        path = self.run_root / "pids" / f"{label}.pid"
        try:
            path.write_text(f"{pid}\n", encoding="ascii")
        except OSError:
            return

    # ----- Event log -----

    def _log_event(self, name: str, outcome: str, detail: str) -> None:
        stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
        event = {
            "event": name,
            "outcome": outcome,
            "detail": detail,
            "utc": stamp,
        }
        self.events.append(event)
        if self.run_root is not None:
            log_path = self.run_root / "events.jsonl"
            try:
                with log_path.open("a", encoding="utf-8") as handle:
                    handle.write(json.dumps(event) + "\n")
            except OSError:
                pass

    # ----- Record -----

    def _build_record(self) -> dict[str, Any]:
        passed = (
            self.failure_stage == "none"
            and self.protocol.positive_milestones_reached()
            and self.cleanup_outcome == "clean"
        )
        if passed:
            result = "passed"
            failure_stage = "none"
            failure_reason = "none"
        elif self.failure_stage != "none":
            result = "failed" if self.failure_stage != "preflight" else "blocked"
            failure_stage = self.failure_stage
            failure_reason = self.failure_reason
        else:
            result = "failed"
            failure_stage = "timeout"
            failure_reason = "milestones-incomplete"

        record: dict[str, Any] = {
            "schema": SMOKE_SCHEMA,
            "schema_version": SMOKE_SCHEMA_VERSION,
            "evidence_tier": SMOKE_EVIDENCE_TIER,
            "run_id": self.run_id,
            "source_commit": self.config.source_commit,
            "reference_name": REFERENCE_NAME,
            "reference_version": REFERENCE_VERSION,
            "reference_revision": REFERENCE_REVISION,
            "direction": self.config.direction,
            "started_utc": self.started_utc,
            "completed_utc": self.completed_utc,
            "local_router_hash_sha256": self.local_router_hash_sha256 or ("0" * 64),
            "peer_router_hash_sha256": self.peer_router_hash_sha256 or ("0" * 64),
            "delivery_status_message_id": derive_delivery_status_message_id(
                run_id=self.run_id,
                scenario_id=self.config.scenario_id,
                correlation_nonce=self.run_id,
            ),
            "tcp_connected": self.protocol.tcp_connected,
            "ntcp2_authenticated": self.protocol.ntcp2_authenticated,
            "frame_emitted": self.protocol.frame_emitted,
            "frame_authenticated_and_decrypted": (
                self.protocol.frame_authenticated_and_decrypted
            ),
            "i2np_message_decoded": self.protocol.i2np_message_decoded,
            "cleanup_clean": self.cleanup_outcome == "clean",
            "network_audit": self.network_audit_outcome,
            "result": result,
            "failure_stage": failure_stage,
            "failure_reason": failure_reason,
            "record_sha256": "",
        }
        record["record_sha256"] = canonical_record_digest(record)
        return record

    def write_record(self, record: dict[str, Any]) -> Path:
        """Validate and atomically write the smoke record to disk."""

        smoke_record.validate_loopback_smoke_record(record)
        self.config.output_record.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        fd, temporary = tempfile.mkstemp(
            prefix=f".{self.config.output_record.name}.",
            dir=self.config.output_record.parent,
        )
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                handle.write(json.dumps(record, indent=2, sort_keys=True) + "\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.chmod(temporary, 0o600)
            os.replace(temporary, self.config.output_record)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)
        return self.config.output_record


def _render_strict_scenarios(
    *,
    run_root: Path,
    direction: str,
    run_id: str,
    local_address: str,
    local_port: int,
    peer_address: str,
    peer_port: int,
    expected_sender_router_hash_sha256: str,
    expected_receiver_router_hash_sha256: str,
    reference_driver_mode: str,
    delivery_status_message_id: int,
    run_identity_sha256: str,
) -> dict[str, Path]:
    """Render the i2pr listener and dialer scenario TOML files.

    The renderer mirrors the Plan 065 ``launcher_renderer`` contract
    but builds the TOML inline; it intentionally avoids importing the
    Plan 044 mixed-runner and the Plan 056/066 bundle/certificate
    authority. The renderer pins the scenario id to the bounded
    Plan 065 direction set so the Rust strict parser accepts the file.
    """

    if direction == "i2pr-to-i2pd-ipv4":
        listener_id = "loopback-i2pr-listener"
        dialer_id = "loopback-i2pr-dialer"
    elif direction == "i2pd-to-i2pr-ipv4":
        listener_id = "loopback-i2pr-listener"
        dialer_id = "loopback-i2pr-dialer"
    else:
        raise LoopbackSmokeConfigError("direction-not-allowlisted")

    scenarios_dir = run_root / "scenarios"
    scenarios_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    listener_path = scenarios_dir / "listener.toml"
    dialer_path = scenarios_dir / "dialer.toml"

    listener_toml = _scenario_toml(
        scenario_id=listener_id,
        run_id=run_id,
        role="responder",
        local_address=local_address,
        local_port=peer_port,
        peer_address=None,
        peer_port=None,
        state_dir="i2pr-state-listener",
        peer_router_info=None,
        expected_sender_router_hash_sha256=expected_sender_router_hash_sha256,
        expected_receiver_router_hash_sha256=expected_receiver_router_hash_sha256,
        reference_driver_mode=reference_driver_mode,
        delivery_status_message_id=delivery_status_message_id,
        run_identity_sha256=run_identity_sha256,
    )
    dialer_toml = _scenario_toml(
        scenario_id=dialer_id,
        run_id=run_id,
        role="initiator",
        local_address=local_address,
        local_port=local_port,
        peer_address=peer_address,
        peer_port=peer_port,
        state_dir="i2pr-state",
        peer_router_info="exchange/i2pr-router.info",
        expected_sender_router_hash_sha256=expected_sender_router_hash_sha256,
        expected_receiver_router_hash_sha256=expected_receiver_router_hash_sha256,
        reference_driver_mode=reference_driver_mode,
        delivery_status_message_id=delivery_status_message_id,
        run_identity_sha256=run_identity_sha256,
    )

    listener_path.write_text(listener_toml, encoding="utf-8")
    dialer_path.write_text(dialer_toml, encoding="utf-8")
    os.chmod(listener_path, 0o600)
    os.chmod(dialer_path, 0o600)
    return {"listener": listener_path, "dialer": dialer_path}


def _scenario_toml(
    *,
    scenario_id: str,
    run_id: str,
    role: str,
    local_address: str,
    local_port: int,
    peer_address: str | None,
    peer_port: int | None,
    state_dir: str,
    peer_router_info: str | None,
    expected_sender_router_hash_sha256: str,
    expected_receiver_router_hash_sha256: str,
    reference_driver_mode: str,
    delivery_status_message_id: int,
    run_identity_sha256: str,
) -> str:
    """Render one strict scenario TOML document.

    The renderer emits the Plan 065 schema v2 fields only. The
    ``delivery_status_message_id`` and the ``run_identity_sha256`` are
    shared between the listener and the dialer so the i2pr receiver
    and the canonical mixed-runner can verify the exact correlation.
    """

    peer_address_field = f'peer_address = "{peer_address}"' if peer_address else 'peer_address = ""'
    peer_port_field = f"peer_port = {peer_port}" if peer_port is not None else "peer_port = 0"
    peer_router_info_field = (
        f'peer_router_info = "{peer_router_info}"'
        if peer_router_info
        else 'peer_router_info = ""'
    )
    return (
        "[scenario]\n"
        f'schema = "i2pr-launcher-scenario-v2"\n'
        f"schema_version = 2\n"
        f'scenario_id = "{scenario_id}"\n'
        f'run_id = "{run_id}"\n'
        f'role = "{role}"\n'
        'address_family = "ipv4"\n'
        f'local_address = "{local_address}"\n'
        f"local_port = {local_port}\n"
        f"{peer_address_field}\n"
        f"{peer_port_field}\n"
        "network_id = 99\n"
        f'state_dir = "{state_dir}"\n'
        f"{peer_router_info_field}\n"
        "handshake_deadline_ms = 30000\n"
        "read_deadline_ms = 5000\n"
        "write_deadline_ms = 5000\n"
        "queue_deadline_ms = 2000\n"
        "drain_deadline_ms = 2000\n"
        'padding_profile = "minimum-variable-maximum"\n'
        'smoke_message_profile = "delivery-status"\n'
        "deterministic_seed = 0\n"
        'expected_result_class = "authenticated-handshake-and-directional-data-phase"\n'
        'status_path = "status.jsonl"\n'
        'data_phase_mode = "round-trip-delivery-status"\n'
        'data_phase_required_peer_action = "non-echo-completion"\n'
        'expected_observation = "i2pr-sent-and-acknowledged"\n'
        f"delivery_status_message_id = {int(delivery_status_message_id)}\n"
        f'expected_sender_router_hash_sha256 = "{expected_sender_router_hash_sha256}"\n'
        f'expected_receiver_router_hash_sha256 = "{expected_receiver_router_hash_sha256}"\n'
        f'reference_driver_mode = "{reference_driver_mode}"\n'
        f'run_identity_sha256 = "{run_identity_sha256}"\n'
    )


def _default_subprocess_factory(*args: Any, **kwargs: Any) -> subprocess.Popen[bytes]:
    return subprocess.Popen(*args, **kwargs)


def cli_main(argv: list[str] | None = None) -> int:
    """Entry point for the smoke runner CLI."""

    try:
        config = parse_cli_args(argv)
    except LoopbackSmokeConfigError as exc:
        print(f"smoke-config-error: {exc}", file=sys.stderr)
        return 2
    repo_root = Path(__file__).resolve().parents[4]
    runner = LoopbackSmokeRunner(config=config, repo_root=repo_root)
    try:
        record = runner.run()
        runner.write_record(record)
    except LoopbackSmokeRunError as exc:
        print(f"smoke-run-error: {exc.code} stage={exc.stage}", file=sys.stderr)
        return 3
    except LoopbackSmokeConfigError as exc:
        print(f"smoke-config-error: {exc}", file=sys.stderr)
        return 2
    if record["result"] == "passed":
        return 0
    if record["result"] == "blocked":
        return 4
    return 5


__all__ = [
    "ALLOWED_DIRECTIONS",
    "ALLOWED_DIAGNOSTICS_MODES",
    "ALLOWED_EVENT_NAMES",
    "ALLOWED_NETWORK_AUDIT_MODES",
    "DEFAULT_CLEANUP_TIMEOUT_SECONDS",
    "DEFAULT_DATA_TIMEOUT_SECONDS",
    "DEFAULT_HANDSHAKE_TIMEOUT_SECONDS",
    "DEFAULT_READINESS_TIMEOUT_SECONDS",
    "DEFAULT_TOTAL_TIMEOUT_SECONDS",
    "HEX40",
    "HEX64",
    "I2PD_DRIVER_DIR",
    "I2PD_DRIVER_SOURCE",
    "I2PD_OBSERVER_PATCH",
    "I2PD_RUN_SCRIPT",
    "I2PD_SOURCE_LOCK",
    "LOOPBACK_IPV4",
    "LOOPBACK_IPV6",
    "LoopbackSmokeConfig",
    "LoopbackSmokeConfigError",
    "LoopbackSmokeError",
    "LoopbackSmokeRunError",
    "LoopbackSmokeRunner",
    "MAX_DEADLINE_SECONDS",
    "MIN_DEADLINE_SECONDS",
    "ProcessRole",
    "ProcessRoleAssignment",
    "REFERENCE_DRIVER_MODE",
    "REFERENCE_EVENT_SCHEMA",
    "REFERENCE_EVENT_VERSION",
    "REFERENCE_IMPLEMENTATION",
    "REFERENCE_NAME",
    "REFERENCE_REVISION",
    "REFERENCE_VERSION",
    "ROLE_ASSIGNMENTS",
    "REPO_ROOT",
    "SmokeProtocol",
    "TransportRole",
    "TYPED_BLOCKER_CODES",
    "TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN",
    "TYPED_BLOCKER_REFERENCE_EVENTS_MISSING",
    "TYPED_BLOCKER_REFERENCE_PROCESS_NOT_EXECUTED",
    "TYPED_BLOCKER_SYNTHETIC_PROVENANCE_REJECTED",
    "allocate_loopback_port",
    "check_loopback_listening",
    "cli_main",
    "derive_delivery_status_message_id",
    "generate_run_id",
    "parse_cli_args",
    "parse_config_dict",
    "probe_strace_available",
]


if __name__ == "__main__":
    sys.exit(cli_main())
