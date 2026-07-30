"""Plan 059 reference-side helper and qualification validator.

This module provides the source-locked validation helpers used by the
Plan 059 test matrix and the Plan 060 candidate freeze:

- :func:`load_source_lock` — load and validate the i2pd direct
  helper source-lock record;
- :func:`load_observation_qualification_receipt` — load and validate
  the per-reference observation qualification receipt;
- :func:`load_qualification_summary` — load the typed-absence
  summary record that tracks the receiver observation qualification
  status;
- :func:`i2pd_helper_invocation` — invoke the i2pd direct connect
  helper through its Python bounded driver and return the finalized
  trigger record plus the typed exit code;
- :func:`assert_plan059_typed_blocker` — assert that Plan 059 closes
  with the required typed blocker because the Plan 058 ADR 0021
  rejection forbids the Java support topology.

The helpers under :mod:`trigger_record` remain the authoritative
schema for the trigger payload; this module binds the source-lock
record, the qualification receipts, and the canonical Plan 055
trigger schema into a single typed surface.

Plan 059 closes with the typed blocker
``blocked_java_support_topology_rejected`` because ADR 0021
(`docs/adr/0021-minimal-java-support-topology.md`) was Rejected by
the Plan 058 repository maintainer decision. The Java
reference-initiated direction remains blocked; this module exposes
the contract that Plan 060 cannot consume a candidate without a
fresh ADR or a different pinned Java revision.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

I2PD_DIRECT_CONNECT_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd_direct_connect"
OBSERVATION_QUALIFICATION_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-observation-qualification"
SOURCE_LOCK_PATH = I2PD_DIRECT_CONNECT_DIR / "source-lock.json"
HELPER_DRIVER_PATH = I2PD_DIRECT_CONNECT_DIR / "i2pd_direct_connect.py"
CATALOG_PATH = REPO_ROOT / "tests/integration/ntcp2/reference-observation-catalog.toml"
QUALIFICATION_SUMMARY_PATH = OBSERVATION_QUALIFICATION_DIR / "summary.json"
ADR_0021_PATH = REPO_ROOT / "docs/adr/0021-minimal-java-support-topology.md"

HELPER_SOURCE_LOCK_SCHEMA = "i2pr-i2pd-helper-source-lock-v1"
QUALIFICATION_RECEIPT_SCHEMA = "i2pr-reference-observation-qualification-v1"
QUALIFICATION_SUMMARY_SCHEMA = "i2pr-reference-observation-qualification-summary-v1"
HELPER_KIND = "i2pd-direct-helper"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")


class Plan059Error(ValueError):
    """Raised when a Plan 059 helper or qualification validator fails."""


@dataclass(frozen=True)
class HelperSourceLock:
    schema: str
    schema_version: int
    helper_kind: str
    reference_name: str
    reference_version: str
    reference_revision: str
    source_lock_path: Path

    @property
    def source_lock_sha256(self) -> str:
        return hashlib.sha256(self.source_lock_path.read_bytes()).hexdigest()


@dataclass(frozen=True)
class QualificationReceipt:
    schema: str
    schema_version: int
    reference: str
    reference_version: str
    reference_revision: str
    qualification_status: str
    qualification_blocker: str
    qualified_marker_count: int
    total_marker_count: int
    receipt_path: Path

    @property
    def receipt_sha256(self) -> str:
        return hashlib.sha256(self.receipt_path.read_bytes()).hexdigest()


def _load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise Plan059Error(f"json-unreadable:{path.name}:{exc}") from exc


def load_source_lock(path: Path = SOURCE_LOCK_PATH) -> HelperSourceLock:
    payload = _load_json(path)
    schema = payload.get("schema") or payload.get("$schema")
    if schema != HELPER_SOURCE_LOCK_SCHEMA:
        raise Plan059Error("helper-source-lock-schema-mismatch")
    if payload.get("helper_kind") != HELPER_KIND:
        raise Plan059Error("helper-source-lock-kind-mismatch")
    reference = payload.get("reference", {})
    if not HEX40.fullmatch(reference.get("revision", "")):
        raise Plan059Error("helper-source-lock-revision-not-hex")
    if not reference.get("source_revision_locked"):
        raise Plan059Error("helper-source-lock-not-revision-locked")
    return HelperSourceLock(
        schema=schema,
        schema_version=payload.get("schema_version", 0),
        helper_kind=payload["helper_kind"],
        reference_name=reference["name"],
        reference_version=reference["version"],
        reference_revision=reference["revision"],
        source_lock_path=path,
    )


def load_observation_qualification_receipt(reference: str) -> QualificationReceipt:
    if reference not in {"java_i2p", "i2pd"}:
        raise Plan059Error("qualification-receipt-reference-unknown")
    if reference == "i2pd":
        path = OBSERVATION_QUALIFICATION_DIR / "i2pd-2.60.0.json"
    else:
        path = OBSERVATION_QUALIFICATION_DIR / "java_i2p-2.12.0.json"
    payload = _load_json(path)
    if payload.get("schema") != QUALIFICATION_RECEIPT_SCHEMA:
        raise Plan059Error("qualification-receipt-schema-mismatch")
    qualifications = payload.get("qualifications", [])
    if not isinstance(qualifications, list):
        raise Plan059Error("qualification-receipt-missing-qualifications")
    return QualificationReceipt(
        schema=payload["schema"],
        schema_version=payload.get("schema_version", 0),
        reference=payload["reference"],
        reference_version=payload["reference_version"],
        reference_revision=payload["reference_revision"],
        qualification_status=payload.get("qualification_status", ""),
        qualification_blocker=payload.get("qualification_blocker", ""),
        qualified_marker_count=sum(1 for q in qualifications if q.get("qualified")),
        total_marker_count=len(qualifications),
        receipt_path=path,
    )


def load_qualification_summary() -> dict[str, Any]:
    if not QUALIFICATION_SUMMARY_PATH.is_file():
        raise Plan059Error("qualification-summary-missing")
    payload = _load_json(QUALIFICATION_SUMMARY_PATH)
    if payload.get("schema") != QUALIFICATION_SUMMARY_SCHEMA:
        raise Plan059Error("qualification-summary-schema-mismatch")
    return payload


def observation_catalog_digest() -> str:
    return hashlib.sha256(CATALOG_PATH.read_bytes()).hexdigest()


def helper_source_digest() -> str:
    """Return the SHA-256 of the C++ helper source file."""

    cpp_path = I2PD_DIRECT_CONNECT_DIR / "i2pd_direct_connect.cpp"
    return hashlib.sha256(cpp_path.read_bytes()).hexdigest()


def helper_python_driver_digest() -> str:
    """Return the SHA-256 of the bounded Python helper driver."""

    return hashlib.sha256(HELPER_DRIVER_PATH.read_bytes()).hexdigest()


def i2pd_helper_invocation(
    *,
    config: dict[str, Any],
    helper_binary_sha256: str = "0" * 64,
    helper_source_sha256: str = "0" * 64,
) -> tuple[int, dict[str, Any]]:
    """Invoke the bounded Python helper driver and return the
    (exit_code, finalized_trigger_record) tuple.
    """

    config = dict(config)
    required = (
        "data_dir",
        "router_info",
        "expected_router_hash",
        "expected_host",
        "expected_port",
        "run_id",
        "scenario_id",
        "correlation_nonce",
        "run_identity_sha256",
        "source_inspection_record_sha256",
        "result_path",
    )
    for field in required:
        if field not in config:
            raise Plan059Error(f"helper-config-missing:{field}")
    import importlib

    driver_dir = str(I2PD_DIRECT_CONNECT_DIR)
    if driver_dir not in sys.path:
        sys.path.insert(0, driver_dir)
    driver = importlib.import_module("i2pd_direct_connect")
    exit_code, record = driver.run(driver.HelperConfig(
        data_dir=Path(config["data_dir"]),
        router_info=Path(config["router_info"]),
        expected_router_hash=config["expected_router_hash"],
        expected_host=config["expected_host"],
        expected_port=int(config["expected_port"]),
        run_id=config["run_id"],
        scenario_id=config["scenario_id"],
        correlation_nonce=config["correlation_nonce"],
        run_identity_sha256=config["run_identity_sha256"],
        helper_binary_sha256=helper_binary_sha256,
        helper_source_sha256=helper_source_sha256,
        source_inspection_record_sha256=config["source_inspection_record_sha256"],
        dial_timeout_seconds=int(config.get("dial_timeout_seconds", 1)),
        result_path=Path(config["result_path"]),
    ))
    return exit_code, record


def adr_0021_decision(path: Path = ADR_0021_PATH) -> str:
    """Return the explicit decision for ADR 0021."""

    text = path.read_text(encoding="utf-8")
    match = re.search(r"^- Status:\s*(\w+)", text, flags=re.MULTILINE)
    if match is None:
        return "Unknown"
    return match.group(1)


def assert_plan059_typed_blocker(decision: str | None = None) -> None:
    """Assert that Plan 059 closes with the required typed blocker.

    Plan 058 rejected ADR 0021; Plan 059 must therefore close with
    the typed blocker ``blocked_java_support_topology_rejected``.
    """

    if decision is None:
        decision = adr_0021_decision()
    if decision != "Rejected":
        raise Plan059Error(
            "adr-0021-not-rejected; Plan 059 cannot close with "
            "blocked_java_support_topology_rejected"
        )


def plan059_typed_blocker() -> str:
    """Return the typed blocker that closes Plan 059."""

    assert_plan059_typed_blocker()
    return "blocked_java_support_topology_rejected"


def qualification_requirements_locked() -> dict[str, bool]:
    """Return the locked Plan 059 requirement matrix for the static
    boundary check.
    """

    return {
        "source_lock_committed": SOURCE_LOCK_PATH.is_file(),
        "cmake_contract_committed": (I2PD_DIRECT_CONNECT_DIR / "CMakeLists.txt").is_file(),
        "cpp_helper_committed": (I2PD_DIRECT_CONNECT_DIR / "i2pd_direct_connect.cpp").is_file(),
        "python_driver_committed": HELPER_DRIVER_PATH.is_file(),
        "qualification_receipt_i2pd_committed": (OBSERVATION_QUALIFICATION_DIR / "i2pd-2.60.0.json").is_file(),
        "qualification_receipt_java_committed": (OBSERVATION_QUALIFICATION_DIR / "java_i2p-2.12.0.json").is_file(),
        "qualification_summary_committed": QUALIFICATION_SUMMARY_PATH.is_file(),
        "adr_0021_present": ADR_0021_PATH.is_file(),
        "adr_0021_rejected": adr_0021_decision() == "Rejected",
    }
