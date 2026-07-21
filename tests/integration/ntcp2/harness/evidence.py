"""Sanitized, typed evidence records for the Plan 038/041 harness.

This module deliberately has no router, socket, or network dependencies. Raw
logs and secret-bearing run state remain outside the record schema and are
deleted by the runner before a scenario is considered complete.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import tempfile
from pathlib import Path
from typing import Any

RECORD_FIELDS = (
    "schema",
    "scenario_id",
    "date_utc",
    "i2pr_commit",
    "reference",
    "reference_version",
    "reference_revision",
    "artifact_sha256",
    "installed_tree_sha256",
    "configuration_sha256",
    "namespace_topology_sha256",
    "direction",
    "address_family",
    "deterministic_parameters",
    "expected",
    "actual_typed_result",
    "resource_counters",
    "process_counters",
    "cleanup_result",
    "evidence_sha256",
    "known_deviation",
    "reproduction",
    # Plan 045 D8: typed RouterInfo digests and directional data-phase
    # selectors. Records that pre-date Plan 045 omit the trailing four
    # fields; new records carry real SHA-256 digests of the live
    # i2pr RouterInfo and the live reference RouterInfo.
    "i2pr_router_info_sha256",
    "reference_router_info_sha256",
    "data_phase_mode",
    "expected_observation",
    # Plan 046: bind each mixed-router record to the sandbox attestation
    # that authorized its execution.
    "topology_kind",
    "privilege_model",
    "sandbox_attestation_sha256",
    "parent_network_state_unchanged",
)

# Plan 049 Multipass attribution is an opt-in suffix so existing reference
# and local harness records remain schema-compatible.  Multipass direction
# records must carry the complete suffix before they can enter a VM bundle.
MULTIPASS_RECORD_FIELDS = (
    "environment_id",
    "run_id",
    "instance_generation",
    "environment_evidence_sha256",
    "instance_name_digest",
    "lifecycle_schema_version",
    "ownership_record_sha256",
    "environment_manifest_sha256",
    "cloud_init_sha256",
    "host_baseline_probe_outcome",
    "guest_rootless_probe_outcome",
    "adoption_mode",
)

REFERENCE_PAIR_RECORD_FIELDS = (
    "schema",
    "scenario_id",
    "date_utc",
    "i2pr_commit",
    "java_reference",
    "java_version",
    "java_revision",
    "java_artifact_sha256",
    "java_installed_tree_sha256",
    "java_configuration_sha256",
    "i2pd_reference",
    "i2pd_version",
    "i2pd_revision",
    "i2pd_artifact_sha256",
    "i2pd_installed_tree_sha256",
    "i2pd_configuration_sha256",
    "namespace_topology_sha256",
    "private_network_id",
    "direction_policy",
    "router_info_validation",
    "authenticated_link_observations",
    "connection_counters",
    "process_counters",
    "expected_authenticated_link_count",
    "actual_typed_result",
    "cleanup_result",
    "evidence_sha256",
    "known_deviation",
    "reproduction",
)

_FORBIDDEN = re.compile(
    r"(?:-----BEGIN[^\n]*PRIVATE KEY-----|router\.identity|ntcp2\.static\.key|"
    r"\.pcap(?:ng)?\b|RouterInfo|I2NP|/home/|/root/|[A-Fa-f0-9]{80,})",
    re.IGNORECASE,
)
_ENDPOINT = re.compile(r"(?:\d{1,3}\.){3}\d{1,3}:\d+|\[[0-9a-f:]+\]:\d+", re.IGNORECASE)
_HEX40 = re.compile(r"^[0-9a-f]{40};(?:clean|dirty)$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_REPRODUCTION_TEMPLATE = re.compile(
    r"^bash scripts/interop/run-scenario\.sh --scenario [a-z0-9-]+ --reference (?:java_i2p|i2pd)$"
)
_ALLOWED_EXPECTED = {
    "authenticated-handshake-and-bounded-i2np-exchange",
    "authenticated-handshake-and-bounded-i2np-exchange-or-explicit-environment-skip",
    "authenticated-handshake-and-directional-data-phase",
    "typed-rejection-with-bounded-cleanup",
    "deterministic-winner-and-loser-drain",
    "bounded-result",
}
_ALLOWED_KNOWN_DEVIATION = {
    "driver-absent",
    "driver-absent-with-cleanup",
    "cleanup-verification-failed",
    "environment-smoke-only",
    "reference-only-control",
    "host-contract-blocked",
    "no-valid-data-phase-oracle",
    "evidence-finalization-failed",
    "ipv6-capability-unavailable",
    "typed-harness-operation-failed",
    "not-started",
    "dual-authenticated-reference-observation",
    "authenticated-link-observation-missing",
    "typed-reference-pair-operation-failed",
    "run-root-delete-failed",
    "negative-scenario-before-primary-directions",
    "data-oracle-not-available",
}


class EvidenceError(ValueError):
    """Raised when a result would cross the sanitized evidence boundary."""


def _scan(value: Any, *, field: str | None = None) -> None:
    if isinstance(value, str):
        if field in {"expected", "actual_typed_result"}:
            return
        if _FORBIDDEN.search(value) or _ENDPOINT.search(value):
            raise EvidenceError("record contains forbidden secret, payload, path, or endpoint text")
    elif isinstance(value, dict):
        for child in value.values():
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


_ALLOWED_TOPOLOGY_KIND = {
    "rootless-sealed-single-netns",
    "privileged-dual-netns-veth",
}
_ALLOWED_PRIVILEGE_MODEL = {
    "unprivileged-userns",
    "host-capabilities",
}


def validate_record(record: dict[str, Any]) -> None:
    """Validate the exact typed-record shape and sanitation rules."""

    if record.get("schema") == 2:
        _validate_reference_pair_record(record)
        return
    multipass_record = tuple(record) == RECORD_FIELDS + MULTIPASS_RECORD_FIELDS
    if tuple(record) not in {RECORD_FIELDS, RECORD_FIELDS + MULTIPASS_RECORD_FIELDS}:
        raise EvidenceError("record fields do not match the locked schema")
    if record["schema"] != 1:
        raise EvidenceError("unsupported evidence schema")
    fields = RECORD_FIELDS + (MULTIPASS_RECORD_FIELDS if multipass_record else ())
    for field in fields:
        if field not in record:
            raise EvidenceError(f"missing evidence field: {field}")
    for field in fields:
        if field not in {"expected", "actual_typed_result"}:
            _scan(record[field], field=field)
    if record["expected"] not in _ALLOWED_EXPECTED:
        raise EvidenceError("unknown expected result class")
    if record["actual_typed_result"] not in {
        "passed",
        "rejected",
        "blocked",
        "skipped_ipv6",
        "blocked_host_contract",
        "failed_cleanup",
    }:
        raise EvidenceError("unknown typed result")
    if record["known_deviation"] not in _ALLOWED_KNOWN_DEVIATION:
        raise EvidenceError("unknown known-deviation reason code")
    if not _REPRODUCTION_TEMPLATE.fullmatch(record["reproduction"]):
        raise EvidenceError("reproduction does not match the fixed template")
    if record["cleanup_result"] not in {"clean", "forced", "failed", "not-started"}:
        raise EvidenceError("unknown cleanup result")
    if record["reference"] not in {"java_i2p", "i2pd"}:
        raise EvidenceError("non-canonical reference identifier")
    expected_version = "2.12.0" if record["reference"] == "java_i2p" else "2.60.0"
    if record["reference_version"] != expected_version:
        raise EvidenceError("reference version does not match identifier")
    if not _HEX40.fullmatch(str(record["i2pr_commit"])):
        raise EvidenceError("i2pr commit is not an exact commit plus disposition")
    if not re.fullmatch(r"[0-9a-f]{40}", str(record["reference_revision"])):
        raise EvidenceError("reference revision is not a full object ID")
    for field in (
        "artifact_sha256",
        "installed_tree_sha256",
        "configuration_sha256",
        "namespace_topology_sha256",
        "i2pr_router_info_sha256",
        "reference_router_info_sha256",
        "sandbox_attestation_sha256",
    ):
        value = str(record[field])
        if not _HEX64.fullmatch(value):
            raise EvidenceError(f"{field} is not a SHA-256 digest")
        if record["actual_typed_result"] == "passed" and value == "0" * 64:
            raise EvidenceError(f"passed record contains a zero-filled {field}")
    if record["topology_kind"] not in _ALLOWED_TOPOLOGY_KIND:
        raise EvidenceError("topology_kind is not a typed selector")
    if record["privilege_model"] not in _ALLOWED_PRIVILEGE_MODEL:
        raise EvidenceError("privilege_model is not a typed selector")
    if not isinstance(record["parent_network_state_unchanged"], bool):
        raise EvidenceError("parent_network_state_unchanged is not a boolean")
    if record["actual_typed_result"] == "passed":
        if record["cleanup_result"] not in {"clean", "forced"}:
            raise EvidenceError("passed record did not clean up")
        if record["i2pr_commit"] == "record-at-execution":
            raise EvidenceError("passed record contains an execution placeholder")
        if record["topology_kind"] == "rootless-sealed-single-netns":
            if record["privilege_model"] != "unprivileged-userns":
                raise EvidenceError("rootless record requires unprivileged-userns")
            if record["sandbox_attestation_sha256"] == "0" * 64:
                raise EvidenceError("passed rootless record requires a non-zero sandbox attestation")
            if not record["parent_network_state_unchanged"]:
                raise EvidenceError("passed rootless record requires parent state unchanged")
    if record["evidence_sha256"] and not _HEX64.fullmatch(str(record["evidence_sha256"])):
        raise EvidenceError("evidence digest is not a SHA-256 digest")
    if record["data_phase_mode"] not in {
        "handshake-only",
        "initiator-data-only",
        "responder-data-only",
        "round-trip-delivery-status",
    }:
        raise EvidenceError("data_phase_mode is not a typed selector")
    if record["expected_observation"] not in {
        "i2pr-sent-and-acknowledged",
        "i2pr-received-from-peer",
        "i2pr-sent-only",
        "i2pr-received-only",
        "no-data-phase-required",
    }:
        raise EvidenceError("expected_observation is not a typed selector")
    if multipass_record:
        if not re.fullmatch(r"[a-z0-9-]{8,48}", str(record["run_id"])):
            raise EvidenceError("Multipass run ID is not safe")
        if not re.fullmatch(r"[a-z0-9-]+", str(record["environment_id"])):
            raise EvidenceError("Multipass environment ID is not safe")
        if not isinstance(record["instance_generation"], int) or record["instance_generation"] < 1:
            raise EvidenceError("Multipass generation is invalid")
        for field in (
            "environment_evidence_sha256",
            "instance_name_digest",
            "ownership_record_sha256",
            "environment_manifest_sha256",
            "cloud_init_sha256",
        ):
            if not _HEX64.fullmatch(str(record[field])):
                raise EvidenceError(f"Multipass {field} is not a SHA-256 digest")
        if record["lifecycle_schema_version"] != 1:
            raise EvidenceError("unsupported Multipass lifecycle schema")
        if record["guest_rootless_probe_outcome"] != "rootless_sandbox_available":
            raise EvidenceError("Multipass guest probe did not pass")
        if record["adoption_mode"] not in {"fresh", "adopted", "resumed", "recreated"}:
            raise EvidenceError("unknown Multipass adoption mode")
        if record["actual_typed_result"] == "passed" and record["host_baseline_probe_outcome"] == "rootless_sandbox_available":
            # A permissive host baseline is allowed, but it is never the
            # guest execution proof; the guest field above remains mandatory.
            pass


def _require_sha256(record: dict[str, Any], field: str, *, nonzero: bool) -> None:
    value = str(record[field])
    if not _HEX64.fullmatch(value):
        raise EvidenceError(f"{field} is not a SHA-256 digest")
    if nonzero and value == "0" * 64:
        raise EvidenceError(f"passed record contains a zero-filled {field}")


def _validate_reference_pair_record(record: dict[str, Any]) -> None:
    if tuple(record) != REFERENCE_PAIR_RECORD_FIELDS:
        raise EvidenceError("reference-pair record fields do not match the locked schema")
    _scan(record)
    if record["schema"] != 2:
        raise EvidenceError("unsupported reference-pair evidence schema")
    if record["java_reference"] != "java_i2p" or record["i2pd_reference"] != "i2pd":
        raise EvidenceError("non-canonical reference-pair identifiers")
    if record["java_version"] != "2.12.0" or record["i2pd_version"] != "2.60.0":
        raise EvidenceError("reference-pair version does not match identifier")
    if record["java_revision"] != "2800040deee9bb376567b671ef2e9c34cf3e30b6":
        raise EvidenceError("Java reference revision does not match the lock")
    if record["i2pd_revision"] != "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e":
        raise EvidenceError("i2pd reference revision does not match the lock")
    if not re.fullmatch(r"[0-9a-f]{40}", str(record["i2pr_commit"])):
        raise EvidenceError("reference-pair commit is not exact")
    for field in (
        "java_artifact_sha256",
        "java_installed_tree_sha256",
        "java_configuration_sha256",
        "i2pd_artifact_sha256",
        "i2pd_installed_tree_sha256",
        "i2pd_configuration_sha256",
        "namespace_topology_sha256",
    ):
        _require_sha256(record, field, nonzero=record["actual_typed_result"] == "passed")
    if record["private_network_id"] != "explicit-non-public":
        raise EvidenceError("reference-pair network ID is not classified as explicit and non-public")
    if record["direction_policy"] not in {"java_i2p", "i2pd"}:
        raise EvidenceError("unknown reference-pair direction policy")
    if record["expected_authenticated_link_count"] != 1:
        raise EvidenceError("reference-pair expected-link count is not one")
    if not isinstance(record["router_info_validation"], dict) or set(record["router_info_validation"]) != {"java_i2p", "i2pd"}:
        raise EvidenceError("reference-pair RouterInfo validation is not dual-sided")
    if any(value not in {"validated-and-bound", "not-run", "rejected"} for value in record["router_info_validation"].values()):
        raise EvidenceError("reference-pair RouterInfo validation was not complete")
    if record["actual_typed_result"] == "passed" and any(value != "validated-and-bound" for value in record["router_info_validation"].values()):
        raise EvidenceError("passed reference-pair record lacks dual RouterInfo validation")
    if not isinstance(record["authenticated_link_observations"], dict) or set(record["authenticated_link_observations"]) != {"java_i2p", "i2pd"}:
        raise EvidenceError("reference-pair observations are not dual-sided")
    if any(value not in {"authenticated", "not-observed", "not-run"} for value in record["authenticated_link_observations"].values()):
        raise EvidenceError("reference-pair authentication observation is not typed")
    if any(value != "authenticated" for value in record["authenticated_link_observations"].values()) and record["actual_typed_result"] == "passed":
        raise EvidenceError("passed reference-pair record lacks dual authentication observations")
    if not isinstance(record["connection_counters"], dict) or set(record["connection_counters"]) != {"java_i2p", "i2pd"}:
        raise EvidenceError("reference-pair connection counters are not dual-sided")
    if not isinstance(record["process_counters"], dict) or set(record["process_counters"]) != {"java_i2p", "i2pd"}:
        raise EvidenceError("reference-pair process counters are not dual-sided")
    for value in record["connection_counters"].values():
        if not isinstance(value, dict) or any(key not in value for key in ("attempts", "authenticated")):
            raise EvidenceError("reference-pair connection counter shape is invalid")
    for value in record["process_counters"].values():
        if not isinstance(value, dict) or any(key not in value for key in ("started", "exited", "forced")):
            raise EvidenceError("reference-pair process counter shape is invalid")
    if record["actual_typed_result"] not in {"passed", "rejected", "blocked", "blocked_host_contract", "failed_cleanup"}:
        raise EvidenceError("unknown reference-pair typed result")
    if record["cleanup_result"] not in {"clean", "forced", "failed", "not-started"}:
        raise EvidenceError("unknown reference-pair cleanup result")
    if record["actual_typed_result"] == "passed" and record["cleanup_result"] not in {"clean", "forced"}:
        raise EvidenceError("passed reference-pair record did not clean up")
    if record["evidence_sha256"] and not _HEX64.fullmatch(str(record["evidence_sha256"])):
        raise EvidenceError("reference-pair evidence digest is not a SHA-256 digest")


def write_record(path: Path, record: dict[str, Any]) -> str:
    """Validate and write one canonical JSON record, returning its hash."""

    validate_record(record)
    unsigned = dict(record)
    unsigned["evidence_sha256"] = ""
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(canonical).hexdigest()
    record["evidence_sha256"] = digest
    validate_record(record)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(json.dumps(record, sort_keys=False, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return digest


def validate_file(path: Path) -> None:
    """Validate one JSON evidence record from disk."""

    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise EvidenceError("evidence file is not valid UTF-8 JSON") from exc
    if not isinstance(value, dict):
        raise EvidenceError("evidence record must be a JSON object")
    validate_record(value)
    if not value["evidence_sha256"]:
        raise EvidenceError("evidence record is not finalized")
    unsigned = dict(value)
    expected = unsigned["evidence_sha256"]
    unsigned["evidence_sha256"] = ""
    actual = hashlib.sha256(json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if actual != expected:
        raise EvidenceError("evidence digest mismatch")
