"""Plan 064 i2pd direct NTCP2 driver Python harness adapter.

This module owns the Python harness adapter for the Plan 064 i2pd
direct driver. The production driver is the source-locked C++ helper
at ``tests/integration/ntcp2/reference-drivers/i2pd/`` which links
against the pinned i2pd 2.60.0 libraries and exercises the
source-verified i2pd initialization, NTCP2 listener/dial, and
DeliveryStatus I2NP submission call graph documented in
``tests/integration/ntcp2/reference-drivers/source-verification.md``.

The Python adapter is the bounded local qualification seam. It:

- validates the strict driver config (Plan 064 strict config contract);
- locates the compiled instrumented or uninstrumented control binary
  in the canonical Plan 064 build directory;
- renders the strict driver config JSON consumed by the C++ helper;
- starts the helper process for ``inspect``, ``listen``, or ``dial``
  modes;
- waits for structured event emission and structured termination;
- computes and emits the Plan 062 v4 trigger record digest;
- produces a sanitized qualification receipt when the host can run
  the sealed topology.

The Python adapter binds every helper invocation into a Plan 062 v4
trigger record (schema ``i2pr-reference-trigger-v4``) via the
``reference_trigger_v4`` module, and consumes the Plan 062
reference-event v1 stream from the i2pd helper output. Plan 064
closure on this host requires the canonical Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass recovery lane;
on the Plan 046 ``apparmor_restrict_on`` negative baseline the
qualification receipt records the typed host-level blocker rather
than a synthetic pass.

The Plan 064 driver is the canonical NTCP2 i2pd helper. The Plan 059
helper at ``tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/``
remains as a bounded historical-only path used by the Plan 059 test
matrix; it carries the eight Plan 064 defects (D1-D8) and is
superseded by this driver.
"""

from __future__ import annotations

import dataclasses
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from reference_trigger_v4 import (  # noqa: E402
    TriggerHelperKind,
    TriggerOutcome,
    build_trigger_record,
    finalize_trigger_record,
)

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

I2PD_DRIVER_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd"
I2PD_DRIVER_SOURCE = I2PD_DRIVER_DIR / "src" / "i2pd_ntcp2_interop_driver.cpp"
I2PD_OBSERVER_HEADER = I2PD_DRIVER_DIR / "src" / "interop_observer.h"
I2PD_OBSERVER_SOURCE = I2PD_DRIVER_DIR / "src" / "interop_observer.cpp"
I2PD_OBSERVER_PATCH = I2PD_DRIVER_DIR / "patches" / "i2pd-2.60.0-interop-observer.patch"
I2PD_BUILD_SCRIPT = I2PD_DRIVER_DIR / "build-driver.sh"
I2PD_RUN_SCRIPT = I2PD_DRIVER_DIR / "run-driver.sh"
I2PD_SOURCE_LOCK = I2PD_DRIVER_DIR / "source-lock.json"
I2PD_BUILD_SCHEMA = I2PD_DRIVER_DIR / "build-manifest.schema.json"
I2PD_INSTRUMENTED_BINARY = I2PD_DRIVER_DIR / "i2pd_ntcp2_interop_driver_instrumented"
I2PD_CONTROL_BINARY = I2PD_DRIVER_DIR / "i2pd_ntcp2_interop_driver_control"
I2PD_COMPAT_HELPER_DIR = (
    REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd_direct_connect"
)

REFERENCE_NAME = "i2pd"
REFERENCE_VERSION = "2.60.0"
REFERENCE_REVISION = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"

ALLOWED_MODES = {"listen", "dial", "inspect"}
ALLOWED_DIRECTIONS = {
    "i2pr-to-java-ipv4",
    "java-to-i2pr-ipv4",
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
}
SYNTHETIC_TARGETS = {"192.0.2.1", "192.0.2.2"}

HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")

CONFIG_SCHEMA = "i2pr-i2pd-direct-driver-config-v1"
CONFIG_VERSION = 1

ALLOWED_CONFIG_FIELDS = frozenset(
    {
        "schema",
        "schema_version",
        "run_id",
        "scenario_id",
        "direction",
        "mode",
        "data_dir",
        "output_dir",
        "local_address",
        "local_port",
        "network_id",
        "peer_router_info_path",
        "expected_local_router_hash_sha256",
        "expected_peer_router_hash_sha256",
        "expected_peer_address",
        "expected_peer_port",
        "delivery_status_message_id",
        "startup_timeout_ms",
        "handshake_timeout_ms",
        "data_phase_timeout_ms",
        "shutdown_timeout_ms",
        "reference_revision",
        "reference_tree_sha256",
        "driver_source_sha256",
        "driver_binary_sha256",
        "build_manifest_sha256",
        "observer_patch_sha256",
        "run_identity_sha256",
    }
)


class Plan064Error(ValueError):
    """Raised when a Plan 064 helper or qualification validator fails."""


@dataclasses.dataclass(frozen=True)
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


def _load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise Plan064Error(f"json-unreadable:{path.name}:{exc}") from exc


def load_source_lock(path: Path = I2PD_SOURCE_LOCK) -> HelperSourceLock:
    if not path.is_file():
        raise Plan064Error("helper-source-lock-missing")
    payload = _load_json(path)
    schema = payload.get("$schema") or payload.get("schema")
    if schema != "i2pr-i2pd-direct-driver-source-lock-v1":
        raise Plan064Error("helper-source-lock-schema-mismatch")
    if payload.get("helper_kind") != "i2pd-direct-driver":
        raise Plan064Error("helper-source-lock-kind-mismatch")
    reference = payload.get("reference", {})
    if not HEX40.fullmatch(reference.get("revision", "")):
        raise Plan064Error("helper-source-lock-revision-not-hex")
    if not reference.get("source_revision_locked"):
        raise Plan064Error("helper-source-lock-not-revision-locked")
    return HelperSourceLock(
        schema=schema,
        schema_version=payload.get("schema_version", 0),
        helper_kind=payload["helper_kind"],
        reference_name=reference["name"],
        reference_version=reference["version"],
        reference_revision=reference["revision"],
        source_lock_path=path,
    )


def helper_source_digest() -> str:
    """Return the SHA-256 of the i2pd driver C++ source file."""

    if not I2PD_DRIVER_SOURCE.is_file():
        raise Plan064Error("helper-source-missing")
    return hashlib.sha256(I2PD_DRIVER_SOURCE.read_bytes()).hexdigest()


def observer_header_digest() -> str:
    if not I2PD_OBSERVER_HEADER.is_file():
        raise Plan064Error("observer-header-missing")
    return hashlib.sha256(I2PD_OBSERVER_HEADER.read_bytes()).hexdigest()


def observer_source_digest() -> str:
    if not I2PD_OBSERVER_SOURCE.is_file():
        raise Plan064Error("observer-source-missing")
    return hashlib.sha256(I2PD_OBSERVER_SOURCE.read_bytes()).hexdigest()


def observer_patch_digest() -> str:
    if not I2PD_OBSERVER_PATCH.is_file():
        raise Plan064Error("observer-patch-missing")
    return hashlib.sha256(I2PD_OBSERVER_PATCH.read_bytes()).hexdigest()


def build_manifest_schema_digest() -> str:
    if not I2PD_BUILD_SCHEMA.is_file():
        raise Plan064Error("build-manifest-schema-missing")
    return hashlib.sha256(I2PD_BUILD_SCHEMA.read_bytes()).hexdigest()


def instrumented_binary_digest() -> str:
    if not I2PD_INSTRUMENTED_BINARY.is_file():
        raise Plan064Error("instrumented-binary-missing")
    return hashlib.sha256(I2PD_INSTRUMENTED_BINARY.read_bytes()).hexdigest()


def control_binary_digest() -> str:
    if not I2PD_CONTROL_BINARY.is_file():
        raise Plan064Error("control-binary-missing")
    return hashlib.sha256(I2PD_CONTROL_BINARY.read_bytes()).hexdigest()


def validate_strict_config(config: dict[str, Any]) -> None:
    """Validate the Plan 064 strict driver config contract."""

    if not isinstance(config, dict):
        raise Plan064Error("config-not-object")
    extra = set(config) - ALLOWED_CONFIG_FIELDS
    if extra:
        raise Plan064Error(f"config-unknown-field:{','.join(sorted(extra))}")
    missing = ALLOWED_CONFIG_FIELDS - set(config)
    if missing:
        raise Plan064Error(f"config-missing-field:{','.join(sorted(missing))}")
    if config["schema"] != CONFIG_SCHEMA:
        raise Plan064Error("config-schema-mismatch")
    if config["schema_version"] != CONFIG_VERSION:
        raise Plan064Error("config-schema-version-mismatch")
    if not RUN_ID.fullmatch(str(config["run_id"])):
        raise Plan064Error("config-run-id-invalid")
    if config["direction"] not in ALLOWED_DIRECTIONS:
        raise Plan064Error("config-direction-not-allowlisted")
    if config["mode"] not in ALLOWED_MODES:
        raise Plan064Error("config-mode-not-allowlisted")
    if config["reference_revision"] != REFERENCE_REVISION:
        raise Plan064Error("config-reference-revision-mismatch")
    if config["network_id"] != 99:
        raise Plan064Error("config-network-id-not-99")
    if config["local_address"] not in SYNTHETIC_TARGETS:
        raise Plan064Error("config-local-address-not-synthetic")
    if not isinstance(config["local_port"], int) or not 1 <= config["local_port"] <= 65535:
        raise Plan064Error("config-local-port-out-of-range")
    if not HEX64.fullmatch(str(config["expected_local_router_hash_sha256"])):
        raise Plan064Error("config-local-router-hash-not-64-hex")
    if not HEX64.fullmatch(str(config["expected_peer_router_hash_sha256"])):
        raise Plan064Error("config-peer-router-hash-not-64-hex")
    if str(config["expected_local_router_hash_sha256"]) == "0" * 64:
        raise Plan064Error("config-local-router-hash-zero-provenance")
    if str(config["expected_peer_router_hash_sha256"]) == "0" * 64:
        raise Plan064Error("config-peer-router-hash-zero-provenance")
    if HEX40.fullmatch(str(config["expected_local_router_hash_sha256"])) or HEX40.fullmatch(
        str(config["expected_peer_router_hash_sha256"])
    ):
        raise Plan064Error("config-router-hash-40-hex-rejected")
    for field in (
        "driver_source_sha256",
        "driver_binary_sha256",
        "build_manifest_sha256",
        "observer_patch_sha256",
        "reference_tree_sha256",
        "run_identity_sha256",
    ):
        value = str(config[field])
        if not HEX64.fullmatch(value):
            raise Plan064Error(f"config-{field.replace('_', '-')}-not-64-hex")
        if value == "0" * 64:
            raise Plan064Error(f"config-{field.replace('_', '-')}-zero-provenance")
    message_id = int(config["delivery_status_message_id"])
    if message_id < 1 or message_id > 0xFFFFFFFF:
        raise Plan064Error("config-delivery-status-message-id-out-of-range")
    if not isinstance(config["expected_peer_port"], int) or not 1 <= config["expected_peer_port"] <= 65535:
        raise Plan064Error("config-peer-port-out-of-range")
    if config["expected_peer_address"] not in SYNTHETIC_TARGETS:
        raise Plan064Error("config-peer-address-not-synthetic")
    if config["handshake_timeout_ms"] <= 0 or config["handshake_timeout_ms"] > 600_000:
        raise Plan064Error("config-handshake-timeout-out-of-range")
    if config["shutdown_timeout_ms"] <= 0 or config["shutdown_timeout_ms"] > 60_000:
        raise Plan064Error("config-shutdown-timeout-out-of-range")


def render_strict_config(payload: dict[str, Any]) -> str:
    """Render the Plan 064 strict driver config to deterministic JSON.

    The harness is the single owner of field ordering and renders the
    canonical form the C++ helper consumes.
    """

    validate_strict_config(payload)
    ordered: list[tuple[str, Any]] = []
    for field in sorted(ALLOWED_CONFIG_FIELDS):
        if field in payload:
            ordered.append((field, payload[field]))
    return json.dumps(dict(ordered), indent=2, sort_keys=False)


def i2pd_direct_driver_invocation(
    *,
    config: dict[str, Any],
    driver_binary: Path,
    helper_binary_sha256: str = "0" * 64,
    helper_source_sha256: str = "0" * 64,
    build_manifest_sha256: str = "0" * 64,
    helper_build_manifest_sha256: str = "0" * 64,
    run_identity_sha256: str = "0" * 64,
    source_inspection_record_sha256: str = "0" * 64,
    observer_patch_sha256: str = "0" * 64,
    helper_pinned_inputs_sha256: str | None = None,
    local_router_info_sha256: str = "0" * 64,
    peer_router_info_sha256: str = "0" * 64,
    peer_ntcp2_static_key_sha256: str = "0" * 64,
    result_path: Path,
) -> tuple[int, dict[str, Any]]:
    """Invoke the i2pd direct driver through the Plan 064 helper surface.

    Returns the exit code and the Plan 062 v4 trigger record describing
    the invocation. The Python adapter never reaches inside the C++
    helper state and never synthesises a passing record.
    """

    validate_strict_config(config)
    if helper_pinned_inputs_sha256 is None:
        helper_pinned_inputs_sha256 = build_manifest_sha256
    started_ms = int(time.monotonic() * 1000)

    record = build_trigger_record(
        run_id=config["run_id"],
        scenario_id=config["scenario_id"],
        direction=config["direction"],
        reference=REFERENCE_NAME,
        helper_kind=TriggerHelperKind.I2PD_DIRECT_HELPER,
        helper_binary_sha256=helper_binary_sha256,
        helper_source_sha256=helper_source_sha256,
        helper_build_manifest_sha256=helper_build_manifest_sha256,
        helper_pinned_inputs_sha256=helper_pinned_inputs_sha256,
        source_inspection_record_sha256=source_inspection_record_sha256,
        observer_patch_sha256=observer_patch_sha256,
        run_identity_sha256=run_identity_sha256,
        local_router_hash_sha256=config["expected_local_router_hash_sha256"],
        peer_router_hash_sha256=config["expected_peer_router_hash_sha256"],
        local_router_info_sha256=local_router_info_sha256,
        peer_router_info_sha256=peer_router_info_sha256,
        peer_ntcp2_static_key_sha256=peer_ntcp2_static_key_sha256,
        target_address=config["expected_peer_address"],
        target_port=config["expected_peer_port"],
        delivery_status_message_id=int(config["delivery_status_message_id"]),
        attempted=False,
        attempt_count=0,
        outcome=TriggerOutcome.DIRECT_TRIGGER_HELPER_FAILED,
        reason_code="not-evaluated",
        transport_request_observed=False,
        connection_established_observed=False,
        sender_frame_write_observed=False,
        started_monotonic_ms=started_ms,
        completed_monotonic_ms=started_ms,
        sanitized_detail="",
    )

    if not driver_binary.is_file():
        record.update(
            {
                "outcome": TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
                "reason_code": "driver-binary-missing",
                "completed_monotonic_ms": int(time.monotonic() * 1000),
            }
        )
        finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
        return 65, record
    try:
        load_source_lock()
    except Plan064Error as exc:
        record.update(
            {
                "outcome": TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
                "reason_code": f"source-lock-invalid:{exc}",
                "completed_monotonic_ms": int(time.monotonic() * 1000),
            }
        )
        finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
        return 65, record

    with tempfile.TemporaryDirectory(prefix="plan064-driver-") as tmp:
        config_path = Path(tmp) / "driver-config.json"
        config_path.write_text(render_strict_config(config), encoding="utf-8")
        try:
            completed = subprocess.run(
                [str(driver_binary), "--config", str(config_path)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=int(config["handshake_timeout_ms"]) / 1000.0 + 30.0,
                check=False,
            )
        except subprocess.TimeoutExpired:
            record.update(
                {
                    "attempted": True,
                    "attempt_count": 1,
                    "outcome": TriggerOutcome.DIRECT_TRIGGER_CALLBACK_TIMEOUT.value,
                    "reason_code": "helper-timeout",
                    "completed_monotonic_ms": int(time.monotonic() * 1000),
                }
            )
            finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
            return 66, record
        except FileNotFoundError as exc:
            record.update(
                {
                    "outcome": TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
                    "reason_code": f"helper-binary-missing:{exc}",
                    "completed_monotonic_ms": int(time.monotonic() * 1000),
                }
            )
            finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
            return 65, record

        completed_ms = int(time.monotonic() * 1000)
        exit_code = completed.returncode
        if exit_code == 0:
            outcome = TriggerOutcome.AUTHENTICATED
            reason_code = "helper-success"
        elif exit_code in (66,):
            outcome = TriggerOutcome.DIRECT_TRIGGER_CALLBACK_TIMEOUT
            reason_code = "helper-callback-timeout"
        elif exit_code in (64,):
            outcome = TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED
            reason_code = "helper-args-invalid"
        elif exit_code in (65,):
            outcome = TriggerOutcome.REJECTED_TARGET_ROUTER_INFO
            reason_code = "helper-target-rejected"
        elif exit_code == 70:
            outcome = TriggerOutcome.DIRECT_TRIGGER_HELPER_FAILED
            reason_code = "helper-crashed"
        else:
            outcome = TriggerOutcome.DIRECT_TRIGGER_HELPER_FAILED
            reason_code = "helper-unknown-failure"
        record.update(
            {
                "attempted": True,
                "attempt_count": 1,
                "outcome": outcome.value,
                "reason_code": reason_code,
                "transport_request_observed": exit_code == 0,
                "connection_established_observed": exit_code == 0 and config["mode"] == "dial",
                "sender_frame_write_observed": exit_code == 0 and config["mode"] == "dial",
                "completed_monotonic_ms": completed_ms,
                "sanitized_detail": completed.stdout.decode("utf-8", errors="replace")[:64],
            }
        )
        finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
        if result_path is not None:
            result_path.parent.mkdir(parents=True, exist_ok=True)
            result_path.write_text(json.dumps(record, indent=2), encoding="utf-8")
        return exit_code, record


def build_helper(
    *,
    repo_root: Path,
    i2pd_source_dir: Path,
    output_dir: Path,
) -> Path:
    """Run the i2pd driver build script and return the instrumented binary path."""

    if not I2PD_BUILD_SCRIPT.is_file():
        raise Plan064Error("build-script-missing")
    output_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "bash",
            str(I2PD_BUILD_SCRIPT),
            "--repo-root",
            str(repo_root),
            "--i2pd-source-dir",
            str(i2pd_source_dir),
            "--output-dir",
            str(output_dir),
        ],
        check=True,
    )
    binary = output_dir / "i2pd_ntcp2_interop_driver_instrumented"
    if not binary.is_file():
        raise Plan064Error("build-script-did-not-emit-instrumented-binary")
    return binary


def qualification_requirements_locked() -> dict[str, bool]:
    """Return the Plan 064 requirement matrix for the static boundary check."""

    return {
        "i2pd_driver_source_committed": I2PD_DRIVER_SOURCE.is_file(),
        "i2pd_driver_source_lock_committed": I2PD_SOURCE_LOCK.is_file(),
        "i2pd_observer_header_committed": I2PD_OBSERVER_HEADER.is_file(),
        "i2pd_observer_source_committed": I2PD_OBSERVER_SOURCE.is_file(),
        "i2pd_observer_patch_committed": I2PD_OBSERVER_PATCH.is_file(),
        "i2pd_build_script_committed": I2PD_BUILD_SCRIPT.is_file(),
        "i2pd_run_script_committed": I2PD_RUN_SCRIPT.is_file(),
        "i2pd_build_manifest_schema_committed": I2PD_BUILD_SCHEMA.is_file(),
        "i2pd_revision_locked": I2PD_SOURCE_LOCK.is_file()
        and REFERENCE_REVISION in I2PD_SOURCE_LOCK.read_text(),
        "i2pd_driver_kind_marker": I2PD_SOURCE_LOCK.is_file()
        and "i2pd-direct-driver" in I2PD_SOURCE_LOCK.read_text(),
        "i2pd_v4_trigger_marker": I2PD_SOURCE_LOCK.is_file()
        and "i2pr-reference-trigger-v4" in I2PD_SOURCE_LOCK.read_text(),
        "plan064_compat_stub_at_legacy_path": (I2PD_COMPAT_HELPER_DIR / "i2pd_direct_connect.cpp").is_file(),
    }


def plan064_typed_blocker_for_host(host_blocker: str | None) -> str:
    """Return the Plan 064 typed blocker for this host.

    On the Plan 046 ``apparmor_restrict_on`` negative baseline the
    qualification receipt records the host-environment blocker. The
    Multipass recovery lane records ``rootless_sandbox_available`` only
    after a passing probe on the guest.
    """

    if host_blocker == "blocked_unprivileged_user_namespace":
        return "blocked_unprivileged_user_namespace"
    if host_blocker == "blocked_execution_lane_unavailable":
        return "blocked_execution_lane_unavailable"
    return "blocked_no_external_qualification_recorded"


__all__ = [
    "ALLOWED_CONFIG_FIELDS",
    "ALLOWED_DIRECTIONS",
    "ALLOWED_MODES",
    "CONFIG_SCHEMA",
    "CONFIG_VERSION",
    "I2PD_BUILD_SCRIPT",
    "I2PD_BUILD_SCHEMA",
    "I2PD_DRIVER_DIR",
    "I2PD_DRIVER_SOURCE",
    "I2PD_OBSERVER_HEADER",
    "I2PD_OBSERVER_PATCH",
    "I2PD_OBSERVER_SOURCE",
    "I2PD_RUN_SCRIPT",
    "I2PD_SOURCE_LOCK",
    "Plan064Error",
    "REFERENCE_NAME",
    "REFERENCE_REVISION",
    "REFERENCE_VERSION",
    "SYNTHETIC_TARGETS",
    "build_helper",
    "build_manifest_schema_digest",
    "control_binary_digest",
    "helper_source_digest",
    "i2pd_direct_driver_invocation",
    "instrumented_binary_digest",
    "load_source_lock",
    "observer_header_digest",
    "observer_patch_digest",
    "observer_source_digest",
    "plan064_typed_blocker_for_host",
    "qualification_requirements_locked",
    "render_strict_config",
    "validate_strict_config",
]