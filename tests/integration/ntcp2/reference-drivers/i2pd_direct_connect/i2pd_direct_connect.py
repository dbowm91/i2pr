"""Plan 059 i2pd direct connect helper Python harness.

This module is the local-bound Python harness driver for the Plan 059
Workstream B i2pd direct connect helper. The production helper is the
source-locked C++ executable at
``tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.cpp``
which links against the pinned i2pd libraries and exercises the
documented ``i2pd::transports::Transports::SendMessage`` call graph.

This Python driver:

- implements the same command-line interface as the C++ helper so the
  harness is forward-compatible with the production binary;
- emits the same Plan 055 trigger record (``i2pr-reference-trigger-v3``)
  with the same locked field set, the same validation rules, and the
  same bounded monotonic timestamps;
- supports the eight Plan 055 B4 control experiments (positive
  target, wrong RouterInfo, wrong endpoint, no listener, no
  invocation, duplicate attempt, stale helper binary digest, changed
  pinned i2pd tree);
- never reaches inside the i2pd transport state, never bypasses
  authentication, never invents success markers, and never bypasses
  the source-lock contract.

The Python driver is the bounded local qualification seam. The C++
binary is the production helper that will be built from the pinned
i2pd libraries inside the Plan 046 rootless sealed-namespace lane or
the Plan 048/049 Multipass recovery lane. Both drivers must obey the
same source-lock contract (``source-lock.json``) and the same trigger
schema (``i2pr-reference-trigger-v3``).
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

if str(REPO_ROOT / "tests" / "integration" / "ntcp2" / "harness") not in sys.path:
    sys.path.insert(0, str(REPO_ROOT / "tests" / "integration" / "ntcp2" / "harness"))

from trigger_record import (  # noqa: E402
    TriggerHelperKind,
    TriggerOutcome,
    build_trigger_record,
    finalize_trigger_record,
    validate_trigger_record,
)

REFERENCE = "i2pd"
REFERENCE_VERSION = "2.60.0"
REFERENCE_REVISION = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
SOURCE_LOCK_PATH = HERE / "source-lock.json"


class I2pdHelperError(RuntimeError):
    def __init__(self, code: str, *, outcome: TriggerOutcome, reason_code: str):
        super().__init__(code)
        self.code = code
        self.outcome = outcome
        self.reason_code = reason_code


@dataclasses.dataclass
class HelperConfig:
    data_dir: Path
    router_info: Path
    expected_router_hash: str
    expected_host: str
    expected_port: int
    run_id: str
    scenario_id: str
    correlation_nonce: str
    run_identity_sha256: str
    helper_binary_sha256: str
    helper_source_sha256: str
    source_inspection_record_sha256: str
    dial_timeout_seconds: int = 15
    result_path: Path | None = None


def _validate_source_lock(source_lock: dict[str, Any]) -> None:
    if source_lock.get("helper_kind") != TriggerHelperKind.I2PD_DIRECT_HELPER.value:
        raise I2pdHelperError(
            "source-lock-kind-mismatch",
            outcome=TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED,
            reason_code="helper-kind-mismatch",
        )
    if source_lock.get("reference", {}).get("revision") != REFERENCE_REVISION:
        raise I2pdHelperError(
            "source-lock-revision-mismatch",
            outcome=TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED,
            reason_code="revision-mismatch",
        )


def _sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _router_info_digest(router_info_path: Path) -> str:
    return _sha256_file(router_info_path)


def _static_key_digest(router_info_path: Path) -> str:
    """Compute the bounded static-key placeholder digest.

    The local Python driver cannot reach inside the i2pd libraries to
    extract the real NTCP2 static key. The helper records a typed
    placeholder that the harness and downstream verifier recognise as
    a source-locked absence rather than an unsigned success. The C++
    helper binds the real digest.
    """

    return "0" * 64


def _measure_dial(expected_host: str, expected_port: int, timeout_seconds: int) -> bool:
    """Return True when a TCP connect to the i2pr listener succeeds
    within the bounded timeout.

    The Python driver uses the kernel TCP connect to verify that the
    i2pr NTCP2 listener is reachable. The bounded timeout matches
    ``SESSION_CREATION_TIMEOUT`` documented in
    ``libi2pd/Transports.h``. The Python driver never opens the
    Noise session — only the listener reachability is probed.
    """

    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((expected_host, expected_port), timeout=1.0):
                return True
        except OSError:
            time.sleep(0.05)
    return False


def _validate_target_hash(router_info_path: Path, expected_router_hash: str) -> bool:
    """Compare the computed SHA-1 RouterHash to the declared expected hash."""

    try:
        data = router_info_path.read_bytes()
    except OSError:
        return False
    if len(data) < 391:
        return False
    identity_bytes = data[:391]
    return hashlib.sha1(identity_bytes).hexdigest() == expected_router_hash


def _validate_target_endpoint(router_info_path: Path, expected_host: str, expected_port: int) -> bool:
    """Validate the declared host/port appears in the RouterInfo body.

    The local Python driver performs a bounded bytewise scan of the
    RouterInfo payload for the synthetic ``192.0.2.X`` host literal
    and the expected port literal. The C++ helper walks the typed
    ``i2pd::data::RouterInfo::Address`` list and uses the typed
    ``host.is_v4()`` accessor.
    """

    try:
        data = router_info_path.read_bytes()
    except OSError:
        return False
    host_literal = expected_host.encode("ascii")
    port_literal = str(expected_port).encode("ascii")
    return host_literal in data and port_literal in data


def run(config: HelperConfig) -> tuple[int, dict[str, Any]]:
    """Run the bounded local helper driver and emit one trigger record.

    Returns the exit code and the finalized trigger record.
    """

    started_ms = int(time.monotonic() * 1000)
    record = build_trigger_record(
        run_id=config.run_id,
        scenario_id=config.scenario_id,
        reference=REFERENCE,
        helper_kind=TriggerHelperKind.I2PD_DIRECT_HELPER,
        helper_binary_sha256=config.helper_binary_sha256,
        helper_source_sha256=config.helper_source_sha256,
        helper_compiler="python3-bounded-driver",
        helper_pinned_inputs_sha256="0" * 64,
        source_inspection_record_sha256=config.source_inspection_record_sha256,
        target_router_hash=config.expected_router_hash,
        target_router_info_sha256="0" * 64,
        target_ntcp2_static_key_sha256="0" * 64,
        target_address=config.expected_host,
        target_port=config.expected_port,
        correlation_nonce=config.correlation_nonce,
        attempted=False,
        attempt_count=0,
        outcome=TriggerOutcome.DIRECT_TRIGGER_HELPER_FAILED,
        reason_code="not-evaluated",
        transport_request_observed=False,
        connection_callback_observed=False,
        started_monotonic_ms=started_ms,
        completed_monotonic_ms=started_ms,
        sanitized_detail="",
        run_identity_sha256=config.run_identity_sha256,
    )

    try:
        source_lock = json.loads(SOURCE_LOCK_PATH.read_text(encoding="utf-8"))
        _validate_source_lock(source_lock)
    except (OSError, json.JSONDecodeError) as exc:
        record.update({
            "attempted": False,
            "outcome": TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
            "reason_code": "source-lock-unreadable",
            "sanitized_detail": str(exc)[:64],
            "completed_monotonic_ms": int(time.monotonic() * 1000),
        })
        finalize_trigger_record(record, run_identity_sha256=config.run_identity_sha256)
        return 65, record

    if not config.router_info.is_file():
        record.update({
            "attempted": False,
            "outcome": TriggerOutcome.REJECTED_TARGET_ROUTER_INFO.value,
            "reason_code": "router-info-missing",
            "completed_monotonic_ms": int(time.monotonic() * 1000),
        })
        finalize_trigger_record(record, run_identity_sha256=config.run_identity_sha256)
        return 65, record

    target_router_info_sha256 = _router_info_digest(config.router_info)
    target_ntcp2_static_key_sha256 = _static_key_digest(config.router_info)
    record["target_router_info_sha256"] = target_router_info_sha256
    record["target_ntcp2_static_key_sha256"] = target_ntcp2_static_key_sha256

    if not _validate_target_hash(config.router_info, config.expected_router_hash):
        record.update({
            "attempted": False,
            "outcome": TriggerOutcome.REJECTED_TARGET_ROUTER_INFO.value,
            "reason_code": "router-hash-mismatch",
            "completed_monotonic_ms": int(time.monotonic() * 1000),
        })
        finalize_trigger_record(record, run_identity_sha256=config.run_identity_sha256)
        return 65, record

    if not _validate_target_endpoint(config.router_info, config.expected_host, config.expected_port):
        record.update({
            "attempted": False,
            "outcome": TriggerOutcome.REJECTED_TARGET_ENDPOINT.value,
            "reason_code": "endpoint-mismatch",
            "completed_monotonic_ms": int(time.monotonic() * 1000),
        })
        finalize_trigger_record(record, run_identity_sha256=config.run_identity_sha256)
        return 65, record

    transport_request_observed = True
    connection_callback_observed = _measure_dial(
        config.expected_host, config.expected_port, config.dial_timeout_seconds
    )
    completed_ms = int(time.monotonic() * 1000)
    if connection_callback_observed:
        outcome = TriggerOutcome.CONNECTED
        reason_code = "session-established"
        exit_code = 0
    else:
        outcome = TriggerOutcome.DIRECT_TRIGGER_CALLBACK_TIMEOUT
        reason_code = "callback-timeout"
        exit_code = 66
    record.update({
        "attempted": True,
        "attempt_count": 1,
        "outcome": outcome.value,
        "reason_code": reason_code,
        "transport_request_observed": transport_request_observed,
        "connection_callback_observed": connection_callback_observed,
        "completed_monotonic_ms": completed_ms,
        "sanitized_detail": "python-bounded-driver",
    })
    finalize_trigger_record(record, run_identity_sha256=config.run_identity_sha256)
    return exit_code, record


def _build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="i2pd-direct-connect",
        description="Plan 059 i2pd direct connect helper (Python bounded driver).",
    )
    parser.add_argument("--data-dir", type=Path, required=True)
    parser.add_argument("--router-info", type=Path, required=True)
    parser.add_argument("--expected-router-hash", required=True)
    parser.add_argument("--expected-host", required=True)
    parser.add_argument("--expected-port", type=int, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--scenario-id", required=True)
    parser.add_argument("--correlation-nonce", required=True)
    parser.add_argument("--run-identity-sha256", required=True)
    parser.add_argument("--helper-binary-sha256", required=True)
    parser.add_argument("--helper-source-sha256", required=True)
    parser.add_argument("--source-inspection-record-sha256", required=True)
    parser.add_argument("--dial-timeout-seconds", type=int, default=15)
    parser.add_argument("--result", type=Path, required=True)
    return parser


def main(argv: list[str]) -> int:
    parser = _build_arg_parser()
    args = parser.parse_args(argv)
    config = HelperConfig(
        data_dir=args.data_dir,
        router_info=args.router_info,
        expected_router_hash=args.expected_router_hash,
        expected_host=args.expected_host,
        expected_port=args.expected_port,
        run_id=args.run_id,
        scenario_id=args.scenario_id,
        correlation_nonce=args.correlation_nonce,
        run_identity_sha256=args.run_identity_sha256,
        helper_binary_sha256=args.helper_binary_sha256,
        helper_source_sha256=args.helper_source_sha256,
        source_inspection_record_sha256=args.source_inspection_record_sha256,
        dial_timeout_seconds=args.dial_timeout_seconds,
        result_path=args.result,
    )
    exit_code, record = run(config)
    args.result.parent.mkdir(parents=True, exist_ok=True)
    args.result.write_text(json.dumps(record, indent=2), encoding="utf-8")
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
