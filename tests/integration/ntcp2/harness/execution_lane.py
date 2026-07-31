"""Plan 077 constrained-host lane contracts and capability probe.

This module owns only capability inspection, lane ordering, and the two small
JSON contracts used by the constrained-host lane.  It never installs a
package, starts a container or VM, changes host networking, invokes sudo, or
launches a router.  A capability is not a qualification: a full-runtime lane
is qualified only by a later, bounded execution record.
"""

from __future__ import annotations

import argparse
import ctypes
import datetime as dt
import hashlib
import json
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Final


PROBE_SCHEMA: Final[str] = "i2pr-ntcp2-constrained-host-lane-probe-v1"
QUALIFICATION_SCHEMA: Final[str] = "i2pr-ntcp2-execution-lane-qualification-v1"
MANIFEST_SCHEMA: Final[str] = "i2pr-ntcp2-execution-manifest-v1"
SCHEMA_VERSION: Final[int] = 1

LANE_DOCKER: Final[str] = "docker-network-none"
LANE_QEMU: Final[str] = "qemu-tcg-no-nic"
LANE_INHERITED: Final[str] = "inherited-descriptors-seccomp"
LANE_REMOTE: Final[str] = "remote-manual"
LANE_NONE: Final[str] = "none"
FULL_RUNTIME_LANES: Final[frozenset[str]] = frozenset({LANE_DOCKER, LANE_QEMU, LANE_REMOTE})
ALL_LANES: Final[frozenset[str]] = FULL_RUNTIME_LANES | frozenset({LANE_INHERITED, LANE_NONE})

SAFE_ID_RE: Final[re.Pattern[str]] = re.compile(r"^[a-z0-9][a-z0-9-]{7,47}$")
SHA256_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")
REVISION_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}(?:[0-9a-f]{24})?$")
SOURCE_COMMIT_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}$")
REASON_RE: Final[re.Pattern[str]] = re.compile(r"^[a-z0-9][a-z0-9_-]{1,79}$")

EXPECTED_MANIFEST_FIELDS: Final[frozenset[str]] = frozenset({
    "schema",
    "schema_version",
    "source_commit",
    "reference_revision",
    "i2pr_binary_sha256",
    "i2pd_binary_sha256",
    "reference_build_manifest_sha256",
    "direction",
    "run_id",
    "result_output",
    "execution_timeout_seconds",
})
EXPECTED_QUALIFICATION_FIELDS: Final[frozenset[str]] = frozenset({
    "schema",
    "schema_version",
    "selected_lane",
    "scope",
    "host_or_image_metadata",
    "artifact_digests",
    "loopback_only_proven",
    "no_public_interface_proven",
    "control_connection_passed",
    "result_export_passed",
    "cleanup_passed",
    "qualified",
    "reason_code",
    "reason_codes",
    "full_runtime_lane",
    "reduced_scope_lane",
    "recorded_utc",
    "record_sha256",
})

ALLOWED_DIRECTIONS: Final[frozenset[str]] = frozenset({
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
})


class ExecutionLaneError(ValueError):
    """Raised when a Plan 077 contract is invalid."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ExecutionLaneError(message)


def _strict_bool(value: Any, field: str) -> None:
    _require(isinstance(value, bool), f"{field} must be boolean")


def canonical_digest(record: dict[str, Any], *, digest_field: str = "record_sha256") -> str:
    """Return the SHA-256 of a canonical JSON record without its digest field."""

    payload = {key: value for key, value in record.items() if key != digest_field}
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def _validate_digest(value: Any, field: str) -> None:
    _require(isinstance(value, str) and SHA256_RE.fullmatch(value) is not None, f"{field} invalid")


def validate_execution_manifest(value: Any) -> dict[str, Any]:
    """Validate the bounded manifest shared by future lane entrypoints."""

    _require(isinstance(value, dict), "execution manifest must be an object")
    record = dict(value)
    _require(set(record) == EXPECTED_MANIFEST_FIELDS, "execution manifest fields are not exact")
    _require(record["schema"] == MANIFEST_SCHEMA, "execution manifest schema mismatch")
    _require(record["schema_version"] == SCHEMA_VERSION, "execution manifest version mismatch")
    _require(
        isinstance(record["source_commit"], str)
        and SOURCE_COMMIT_RE.fullmatch(record["source_commit"]) is not None,
        "source_commit must be 40 lowercase hex characters",
    )
    _require(
        isinstance(record["reference_revision"], str)
        and REVISION_RE.fullmatch(record["reference_revision"]) is not None,
        "reference_revision must be a full lowercase revision",
    )
    for field in ("i2pr_binary_sha256", "i2pd_binary_sha256", "reference_build_manifest_sha256"):
        _validate_digest(record[field], field)
    _require(record["direction"] in ALLOWED_DIRECTIONS, "direction is not allowlisted")
    _require(isinstance(record["run_id"], str) and SAFE_ID_RE.fullmatch(record["run_id"]) is not None,
             "run_id must be a safe bounded ID")
    _require(
        isinstance(record["result_output"], str)
        and record["result_output"]
        and not record["result_output"].startswith("/")
        and ".." not in Path(record["result_output"]).parts,
        "result_output must be a relative bounded path",
    )
    _require(
        isinstance(record["execution_timeout_seconds"], int)
        and not isinstance(record["execution_timeout_seconds"], bool)
        and 1 <= record["execution_timeout_seconds"] <= 3600,
        "execution_timeout_seconds out of range",
    )
    return record


def validate_qualification_record(value: Any) -> dict[str, Any]:
    """Validate a sanitized lane prequalification or no-lane record."""

    _require(isinstance(value, dict), "qualification record must be an object")
    record = dict(value)
    _require(set(record) <= EXPECTED_QUALIFICATION_FIELDS, "qualification record has unknown fields")
    required = EXPECTED_QUALIFICATION_FIELDS - {"record_sha256", "recorded_utc"}
    _require(required <= set(record), "qualification record is missing fields")
    _require(record["schema"] == QUALIFICATION_SCHEMA, "qualification schema mismatch")
    _require(record["schema_version"] == SCHEMA_VERSION, "qualification version mismatch")
    _require(record["selected_lane"] in ALL_LANES, "qualification selected_lane invalid")
    _require(record["scope"] in {"full-runtime", "reduced-scope-diagnostic", "unavailable"},
             "qualification scope invalid")
    _require(isinstance(record["host_or_image_metadata"], dict), "host_or_image_metadata must be object")
    _require(isinstance(record["artifact_digests"], dict), "artifact_digests must be object")
    for name, digest in record["artifact_digests"].items():
        _require(isinstance(name, str) and name and "/" not in name and ".." not in name,
                 "artifact digest name invalid")
        _validate_digest(digest, f"artifact_digests.{name}")
    for field in (
        "loopback_only_proven",
        "no_public_interface_proven",
        "control_connection_passed",
        "result_export_passed",
        "cleanup_passed",
        "qualified",
    ):
        _strict_bool(record[field], field)
    _require(isinstance(record["reason_code"], str) and REASON_RE.fullmatch(record["reason_code"]) is not None,
             "reason_code invalid")
    _require(
        isinstance(record["reason_codes"], list)
        and all(isinstance(item, str) and REASON_RE.fullmatch(item) is not None for item in record["reason_codes"]),
        "reason_codes invalid",
    )
    _require(record["full_runtime_lane"] in {"available", "unavailable"}, "full_runtime_lane invalid")
    _require(record["reduced_scope_lane"] in {"available", "unavailable"}, "reduced_scope_lane invalid")
    if "recorded_utc" in record:
        _require(isinstance(record["recorded_utc"], str) and record["recorded_utc"].endswith("Z"),
                 "recorded_utc invalid")
    if "record_sha256" in record:
        _validate_digest(record["record_sha256"], "record_sha256")
        _require(record["record_sha256"] == canonical_digest(record), "record_sha256 mismatch")
    _require(not record["qualified"] or record["scope"] == "full-runtime",
             "only a full-runtime record may be qualified")
    _require(not record["qualified"] or record["full_runtime_lane"] == "available",
             "qualified record requires a full-runtime lane")
    return record


def select_lane(capabilities: dict[str, bool]) -> str:
    """Select the first lane in Plan 077 order without guessing."""

    for field in (
        "docker_daemon_accessible",
        "qemu_tcg_usable",
        "seccomp_no_new_privs_supported",
        "remote_workflow_present",
    ):
        _strict_bool(capabilities.get(field), field)
    if capabilities["docker_daemon_accessible"]:
        return LANE_DOCKER
    if capabilities["qemu_tcg_usable"]:
        return LANE_QEMU
    if capabilities["seccomp_no_new_privs_supported"]:
        return LANE_INHERITED
    if capabilities["remote_workflow_present"]:
        return LANE_REMOTE
    return LANE_NONE


def _probe_docker() -> tuple[bool, bool, str]:
    docker = shutil.which("docker")
    if docker is None:
        return False, False, "docker-cli-absent"
    try:
        result = subprocess.run(
            [docker, "info", "--format", "{{.ServerVersion}}"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
            timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        return True, False, "docker-daemon-ambiguous"
    return True, result.returncode == 0, (
        "docker-daemon-accessible" if result.returncode == 0 else "docker-daemon-inaccessible"
    )


def _qemu_path() -> str | None:
    candidates = [f"qemu-system-{platform.machine()}"]
    if platform.machine() == "x86_64":
        candidates.append("qemu-system-i386")
    for candidate in candidates:
        path = shutil.which(candidate)
        if path:
            return path
    return None


def _probe_qemu() -> tuple[bool, bool, str]:
    qemu = _qemu_path()
    if qemu is None:
        return False, False, "qemu-system-absent"
    command = [
        qemu,
        "-accel",
        "tcg",
        "-machine",
        "none",
        "-nodefaults",
        "-nographic",
        "-display",
        "none",
        "-monitor",
        "none",
        "-serial",
        "none",
        "-nic",
        "none",
        "-S",
    ]
    try:
        result = subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1)
    except subprocess.TimeoutExpired:
        return True, True, "qemu-tcg-usable"
    except OSError:
        return True, False, "qemu-tcg-ambiguous"
    return True, False, "qemu-tcg-unusable" if result.returncode != 0 else "qemu-tcg-exited"


def _probe_no_new_privs() -> tuple[bool, str]:
    try:
        libc = ctypes.CDLL(None, use_errno=True)
        prctl = libc.prctl
        prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
        prctl.restype = ctypes.c_int
        if prctl(38, 1, 0, 0, 0) != 0:  # PR_SET_NO_NEW_PRIVS, scoped to this probe process.
            return False, "seccomp-no-new-privs-unsupported"
    except (AttributeError, OSError):
        return False, "seccomp-no-new-privs-ambiguous"
    return True, "seccomp-no-new-privs-supported"


def _remote_workflow_present(repo_root: Path) -> bool:
    workflow = repo_root / ".github/workflows/ntcp2-interop-ubuntu.yml"
    if not workflow.is_file():
        return False
    text = workflow.read_text(encoding="utf-8")
    return "workflow_dispatch:" in text and "pull_request:" not in text and "run-gate.sh" in text


def probe_environment(repo_root: Path) -> dict[str, Any]:
    """Inspect local capabilities and return a sanitized probe record."""

    docker_cli_present, docker_daemon_accessible, docker_reason = _probe_docker()
    qemu_system_present, qemu_tcg_usable, qemu_reason = _probe_qemu()
    no_new_privs, seccomp_reason = _probe_no_new_privs()
    remote_present = _remote_workflow_present(repo_root)
    selected = select_lane({
        "docker_daemon_accessible": docker_daemon_accessible,
        "qemu_tcg_usable": qemu_tcg_usable,
        "seccomp_no_new_privs_supported": no_new_privs,
        "remote_workflow_present": remote_present,
    })
    reasons = [docker_reason, qemu_reason, seccomp_reason]
    reasons.append("remote-workflow-present" if remote_present else "remote-workflow-missing")
    if selected == LANE_INHERITED and remote_present:
        reasons.append("remote-workflow-present-not-selected-after-reduced-scope")
    if selected == LANE_NONE:
        reasons.append("full-runtime-lane-unavailable")
    return {
        "schema": PROBE_SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "docker_cli_present": docker_cli_present,
        "docker_daemon_accessible": docker_daemon_accessible,
        "qemu_system_present": qemu_system_present,
        "qemu_tcg_usable": qemu_tcg_usable,
        "seccomp_no_new_privs_supported": no_new_privs,
        "remote_workflow_present": remote_present,
        "selected_lane": selected,
        "reason_codes": reasons,
        "host_architecture": platform.machine(),
    }


def no_lane_qualification(probe: dict[str, Any]) -> dict[str, Any]:
    """Create the truthful prequalification record when no full lane ran."""

    selected = probe["selected_lane"]
    reduced_available = selected == LANE_INHERITED
    record: dict[str, Any] = {
        "schema": QUALIFICATION_SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "selected_lane": selected,
        "scope": "reduced-scope-diagnostic" if reduced_available else "unavailable",
        "host_or_image_metadata": {
            "architecture": probe["host_architecture"],
            "docker_cli_present": probe["docker_cli_present"],
            "docker_daemon_accessible": probe["docker_daemon_accessible"],
            "qemu_system_present": probe["qemu_system_present"],
            "qemu_tcg_usable": probe["qemu_tcg_usable"],
            "remote_workflow_present": probe["remote_workflow_present"],
        },
        "artifact_digests": {},
        "loopback_only_proven": False,
        "no_public_interface_proven": False,
        "control_connection_passed": False,
        "result_export_passed": False,
        "cleanup_passed": False,
        "qualified": False,
        "reason_code": "full_runtime_lane_unavailable",
        "reason_codes": list(probe["reason_codes"]),
        "full_runtime_lane": "unavailable",
        "reduced_scope_lane": "available" if reduced_available else "unavailable",
        "recorded_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    }
    record["record_sha256"] = canonical_digest(record)
    return validate_qualification_record(record)


def _write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)


def _main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[4])
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--qualification-output", type=Path, default=None)
    args = parser.parse_args(argv)
    probe = probe_environment(args.repo_root.resolve())
    if args.output is not None:
        _write_json(args.output.resolve(), probe)
    qualification = no_lane_qualification(probe)
    if args.qualification_output is not None:
        _write_json(args.qualification_output.resolve(), qualification)
    print(json.dumps(probe, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(_main(sys.argv[1:]))
