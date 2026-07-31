"""Plan 063 Java I2P stripped-router direct NTCP2 driver harness.

This module owns the Python harness adapter for the Plan 063 Java
direct driver. The production driver is the source-locked Java helper
at ``tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java``
which links against the pinned Java I2P 2.12.0 libraries and exercises
the documented ``net.i2p.router.Router`` and ``net.i2p.router.transport.ntcp.NTCPTransport``
APIs.

The Python adapter is the bounded local qualification seam. It:

- validates the strict driver config (Plan 063 strict config contract);
- locates the pinned Java cache and the compiled helper jar;
- renders the strict driver config JSON consumed by the JVM helper;
- starts the helper process for ``inspect``, ``listen``, or ``dial``
  modes;
- waits for structured event emission and structured termination;
- computes and emits the Plan 062 v4 trigger record digest;
- produces a sanitized qualification receipt when the host can run
  the sealed topology.

The helper Python driver is invoked through ``java_helper_invocation``
for the Plan 063 test matrix. The Java direct driver mirrors the
Plan 059 i2pd direct helper surface: it is test-only, source-locked,
ADR 0022-compliant, and refuses to substitute SAM, HTTP/I2PControl,
tunnel pools, or floodfill integration for the direct transport path.

The Python adapter binds every helper invocation into a Plan 062 v4
trigger record (schema ``i2pr-reference-trigger-v4``) via the
``reference_trigger_v4`` module, and consumes the Plan 062
reference-event v1 stream from the Java helper output. Plan 063
closure on this host requires the canonical Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass recovery lane;
on the Plan 046 ``apparmor_restrict_on`` negative baseline the
qualification receipt records the typed host-level blocker rather
than a synthetic pass.
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import os
import re
import shutil
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
    validate_trigger_record,
)

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

JAVA_HELPER_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/java"
JAVA_HELPER_SOURCE = JAVA_HELPER_DIR / "src" / "JavaNtcp2InteropDriver.java"
JAVA_HELPER_SOURCE_LOCK = JAVA_HELPER_DIR / "source-lock.json"
JAVA_HELPER_CLASSPATH_MANIFEST = JAVA_HELPER_DIR / "classpath-manifest.json"
JAVA_HELPER_BUILD_SCHEMA = JAVA_HELPER_DIR / "build-manifest.schema.json"
JAVA_HELPER_BUILD_SCRIPT = JAVA_HELPER_DIR / "build-driver.sh"
JAVA_HELPER_RUN_SCRIPT = JAVA_HELPER_DIR / "run-driver.sh"

REFERENCE_NAME = "java_i2p"
REFERENCE_VERSION = "2.12.0"
REFERENCE_REVISION = "2800040deee9bb376567b671ef2e9c34cf3e30b6"

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

CONFIG_SCHEMA = "i2pr-java-direct-driver-config-v1"
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
        "classpath_manifest_sha256",
        "run_identity_sha256",
    }
)


class Plan063Error(ValueError):
    """Raised when a Plan 063 helper or qualification validator fails."""


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
        raise Plan063Error(f"json-unreadable:{path.name}:{exc}") from exc


def load_source_lock(path: Path = JAVA_HELPER_SOURCE_LOCK) -> HelperSourceLock:
    if not path.is_file():
        raise Plan063Error("helper-source-lock-missing")
    payload = _load_json(path)
    schema = payload.get("$schema") or payload.get("schema")
    if schema != "i2pr-java-helper-source-lock-v1":
        raise Plan063Error("helper-source-lock-schema-mismatch")
    if payload.get("helper_kind") != "java-direct-helper":
        raise Plan063Error("helper-source-lock-kind-mismatch")
    reference = payload.get("reference", {})
    if not HEX40.fullmatch(reference.get("revision", "")):
        raise Plan063Error("helper-source-lock-revision-not-hex")
    if not reference.get("source_revision_locked"):
        raise Plan063Error("helper-source-lock-not-revision-locked")
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
    """Return the SHA-256 of the Java helper source file."""

    if not JAVA_HELPER_SOURCE.is_file():
        raise Plan063Error("helper-source-missing")
    return hashlib.sha256(JAVA_HELPER_SOURCE.read_bytes()).hexdigest()


def classpath_manifest_digest() -> str:
    if not JAVA_HELPER_CLASSPATH_MANIFEST.is_file():
        raise Plan063Error("classpath-manifest-missing")
    return hashlib.sha256(JAVA_HELPER_CLASSPATH_MANIFEST.read_bytes()).hexdigest()


def build_manifest_schema_digest() -> str:
    if not JAVA_HELPER_BUILD_SCHEMA.is_file():
        raise Plan063Error("build-manifest-schema-missing")
    return hashlib.sha256(JAVA_HELPER_BUILD_SCHEMA.read_bytes()).hexdigest()


def validate_strict_config(config: dict[str, Any]) -> None:
    """Validate the Plan 063 strict driver config contract."""

    if not isinstance(config, dict):
        raise Plan063Error("config-not-object")
    extra = set(config) - ALLOWED_CONFIG_FIELDS
    if extra:
        raise Plan063Error(f"config-unknown-field:{','.join(sorted(extra))}")
    missing = ALLOWED_CONFIG_FIELDS - set(config)
    if missing:
        raise Plan063Error(f"config-missing-field:{','.join(sorted(missing))}")
    if config["schema"] != CONFIG_SCHEMA:
        raise Plan063Error("config-schema-mismatch")
    if config["schema_version"] != CONFIG_VERSION:
        raise Plan063Error("config-schema-version-mismatch")
    if not RUN_ID.fullmatch(str(config["run_id"])):
        raise Plan063Error("config-run-id-invalid")
    if config["direction"] not in ALLOWED_DIRECTIONS:
        raise Plan063Error("config-direction-not-allowlisted")
    if config["mode"] not in ALLOWED_MODES:
        raise Plan063Error("config-mode-not-allowlisted")
    if config["reference_revision"] != REFERENCE_REVISION:
        raise Plan063Error("config-reference-revision-mismatch")
    if config["network_id"] != 99:
        raise Plan063Error("config-network-id-not-99")
    if config["local_address"] not in SYNTHETIC_TARGETS:
        raise Plan063Error("config-local-address-not-synthetic")
    if not isinstance(config["local_port"], int) or not 1 <= config["local_port"] <= 65535:
        raise Plan063Error("config-local-port-out-of-range")
    if not HEX64.fullmatch(str(config["expected_local_router_hash_sha256"])):
        raise Plan063Error("config-local-router-hash-not-64-hex")
    if not HEX64.fullmatch(str(config["expected_peer_router_hash_sha256"])):
        raise Plan063Error("config-peer-router-hash-not-64-hex")
    if str(config["expected_local_router_hash_sha256"]) == "0" * 64:
        raise Plan063Error("config-local-router-hash-zero-provenance")
    if str(config["expected_peer_router_hash_sha256"]) == "0" * 64:
        raise Plan063Error("config-peer-router-hash-zero-provenance")
    if HEX40.fullmatch(str(config["expected_local_router_hash_sha256"])) or HEX40.fullmatch(
        str(config["expected_peer_router_hash_sha256"])
    ):
        raise Plan063Error("config-router-hash-40-hex-rejected")
    for field in (
        "driver_source_sha256",
        "driver_binary_sha256",
        "build_manifest_sha256",
        "classpath_manifest_sha256",
        "reference_tree_sha256",
        "run_identity_sha256",
    ):
        value = str(config[field])
        if not HEX64.fullmatch(value):
            raise Plan063Error(f"config-{field.replace('_', '-')}-not-64-hex")
        if value == "0" * 64:
            raise Plan063Error(f"config-{field.replace('_', '-')}-zero-provenance")
    message_id = int(config["delivery_status_message_id"])
    if message_id < 1 or message_id > 0xFFFFFFFF:
        raise Plan063Error("config-delivery-status-message-id-out-of-range")
    if not isinstance(config["expected_peer_port"], int) or not 1 <= config["expected_peer_port"] <= 65535:
        raise Plan063Error("config-peer-port-out-of-range")
    if config["expected_peer_address"] not in SYNTHETIC_TARGETS:
        raise Plan063Error("config-peer-address-not-synthetic")
    if config["handshake_timeout_ms"] <= 0 or config["handshake_timeout_ms"] > 600_000:
        raise Plan063Error("config-handshake-timeout-out-of-range")
    if config["shutdown_timeout_ms"] <= 0 or config["shutdown_timeout_ms"] > 60_000:
        raise Plan063Error("config-shutdown-timeout-out-of-range")


def render_strict_config(payload: dict[str, Any]) -> str:
    """Render the Plan 063 strict driver config to deterministic JSON.

    The harness is the single owner of field ordering and renders the
    canonical form the Java helper consumes.
    """

    validate_strict_config(payload)
    ordered: list[tuple[str, Any]] = []
    for field in sorted(ALLOWED_CONFIG_FIELDS):
        if field in payload:
            ordered.append((field, payload[field]))
    return json.dumps(dict(ordered), indent=2, sort_keys=False)


def java_helper_invocation(
    *,
    config: dict[str, Any],
    java_cache: Path,
    driver_jar: Path,
    helper_binary_sha256: str = "0" * 64,
    helper_source_sha256: str = "0" * 64,
    build_manifest_sha256: str = "0" * 64,
    classpath_manifest_sha256: str = "0" * 64,
    helper_build_manifest_sha256: str = "0" * 64,
    run_identity_sha256: str = "0" * 64,
    source_inspection_record_sha256: str = "0" * 64,
    helper_pinned_inputs_sha256: str | None = None,
    observer_patch_sha256: str = "0" * 64,
    result_path: Path,
) -> tuple[int, dict[str, Any]]:
    """Invoke the Java direct driver through the Plan 063 helper surface.

    Returns the exit code and the Plan 062 v4 trigger record describing
    the invocation. The Python adapter never reaches inside the Java
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
        helper_kind=TriggerHelperKind.JAVA_DIRECT_HELPER,
        helper_binary_sha256=helper_binary_sha256,
        helper_source_sha256=helper_source_sha256,
        helper_build_manifest_sha256=helper_build_manifest_sha256,
        helper_pinned_inputs_sha256=helper_pinned_inputs_sha256,
        source_inspection_record_sha256=source_inspection_record_sha256,
        observer_patch_sha256=observer_patch_sha256,
        run_identity_sha256=run_identity_sha256,
        local_router_hash_sha256=config["expected_local_router_hash_sha256"],
        peer_router_hash_sha256=config["expected_peer_router_hash_sha256"],
        local_router_info_sha256="0" * 64,
        peer_router_info_sha256="0" * 64,
        peer_ntcp2_static_key_sha256="0" * 64,
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

    if not java_cache.is_dir():
        record.update(
            {
                "outcome": TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
                "reason_code": "java-cache-missing",
                "completed_monotonic_ms": int(time.monotonic() * 1000),
            }
        )
        finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
        return 65, record
    if not driver_jar.is_file():
        record.update(
            {
                "outcome": TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
                "reason_code": "driver-jar-missing",
                "completed_monotonic_ms": int(time.monotonic() * 1000),
            }
        )
        finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
        return 65, record
    try:
        load_source_lock()
    except Plan063Error as exc:
        record.update(
            {
                "outcome": TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
                "reason_code": f"source-lock-invalid:{exc}",
                "completed_monotonic_ms": int(time.monotonic() * 1000),
            }
        )
        finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
        return 65, record

    with tempfile.TemporaryDirectory(prefix="plan063-driver-") as tmp:
        config_path = Path(tmp) / "driver-config.json"
        config_path.write_text(render_strict_config(config), encoding="utf-8")
        try:
            completed = subprocess.run(
                [
                    "java",
                    "-Xmx512m",
                    "-Djava.awt.headless=true",
                    f"-Di2p.dir.base={java_cache}",
                    f"-Di2p.dir.config={java_cache}",
                    f"-Di2p.dir.router={Path(config['data_dir'])}",
                    "-classpath",
                    f"{driver_jar}:{java_cache}/lib/*",
                    "i2pr.ntcp2.JavaNtcp2InteropDriver",
                    config["mode"],
                    "--config",
                    str(config_path),
                ],
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
                    "reason_code": f"java-runtime-missing:{exc}",
                    "completed_monotonic_ms": int(time.monotonic() * 1000),
                }
            )
            finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
            return 65, record

        completed_ms = int(time.monotonic() * 1000)
        exit_code = completed.returncode
        if exit_code == 0:
            outcome = TriggerOutcome.CONNECTED if config["mode"] == "dial" else TriggerOutcome.AUTHENTICATED
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
    java_cache: Path,
    output_dir: Path,
) -> Path:
    """Run the Java helper build script and return the driver jar path."""

    if not JAVA_HELPER_BUILD_SCRIPT.is_file():
        raise Plan063Error("build-script-missing")
    output_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "bash",
            str(JAVA_HELPER_BUILD_SCRIPT),
            "--repo-root",
            str(repo_root),
            "--java-cache",
            str(java_cache),
            "--output-dir",
            str(output_dir),
        ],
        check=True,
    )
    jar = output_dir / "driver.jar"
    if not jar.is_file():
        raise Plan063Error("build-script-did-not-emit-jar")
    return jar


def qualification_requirements_locked() -> dict[str, bool]:
    """Return the Plan 063 requirement matrix for the static boundary check."""

    return {
        "java_helper_source_committed": JAVA_HELPER_SOURCE.is_file(),
        "java_helper_source_lock_committed": JAVA_HELPER_SOURCE_LOCK.is_file(),
        "java_helper_classpath_manifest_committed": JAVA_HELPER_CLASSPATH_MANIFEST.is_file(),
        "java_helper_build_manifest_schema_committed": JAVA_HELPER_BUILD_SCHEMA.is_file(),
        "java_helper_build_script_committed": JAVA_HELPER_BUILD_SCRIPT.is_file(),
        "java_helper_run_script_committed": JAVA_HELPER_RUN_SCRIPT.is_file(),
        "java_helper_revision_locked": JAVA_HELPER_SOURCE_LOCK.is_file()
        and REFERENCE_REVISION in JAVA_HELPER_SOURCE_LOCK.read_text(),
        "java_helper_kind_marker": JAVA_HELPER_SOURCE_LOCK.is_file()
        and "java-direct-helper" in JAVA_HELPER_SOURCE_LOCK.read_text(),
    }


def plan063_typed_blocker_for_host(host_blocker: str | None) -> str:
    """Return the Plan 063 typed blocker for this host.

    On the Plan 046 ``apparmor_restrict_on`` negative baseline the
    qualification receipt records the host-environment blocker. The
    Multipaas recovery lane records ``rootless_sandbox_available`` only
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
    "JAVA_HELPER_BUILD_SCHEMA",
    "JAVA_HELPER_CLASSPATH_MANIFEST",
    "JAVA_HELPER_DIR",
    "JAVA_HELPER_RUN_SCRIPT",
    "JAVA_HELPER_SOURCE",
    "JAVA_HELPER_SOURCE_LOCK",
    "Plan063Error",
    "REFERENCE_NAME",
    "REFERENCE_REVISION",
    "REFERENCE_VERSION",
    "SYNTHETIC_TARGETS",
    "build_helper",
    "build_manifest_schema_digest",
    "classpath_manifest_digest",
    "helper_source_digest",
    "java_helper_invocation",
    "load_source_lock",
    "plan063_typed_blocker_for_host",
    "qualification_requirements_locked",
    "render_strict_config",
    "validate_strict_config",
]
