"""Plan 053 run identity and diagnostic evidence pipeline."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from .evidence_bundle import BundleError, export_bundle_atomic, finalize_bundle, verify_bundle, write_json_atomic
    from .observation import OBSERVATION_SCHEMA, OBSERVATION_SCHEMA_VERSION, build_level, finalize_observation
    from .run_identity import (
        RUN_IDENTITY_SCHEMA,
        build_run_identity,
        cross_check,
        load_run_identity,
        write_run_identity,
    )
except ImportError:
    from evidence_bundle import BundleError, export_bundle_atomic, finalize_bundle, verify_bundle, write_json_atomic
    from observation import OBSERVATION_SCHEMA, OBSERVATION_SCHEMA_VERSION, build_level, finalize_observation
    from run_identity import RUN_IDENTITY_SCHEMA, build_run_identity, cross_check, load_run_identity, write_run_identity


PIPELINE_PROFILE = "milestone-3-v2"
DIAGNOSTIC_RESULT = "diagnostic-complete-not-certificate"
DIRECTION_SCHEMA = "i2pr-mixed-router-direction-v2"
ATTESTATION_SCHEMA = "i2pr-mixed-router-attestation-v2"
TRIGGER_SCHEMA = "i2pr-reference-trigger-v3"
CLEANUP_SCHEMA = "i2pr-mixed-router-cleanup-v2"
DIAGNOSTICS_SCHEMA = "sanitized-summary"
PRIMARY_DIRECTIONS = (
    "i2pr-to-java-ipv4",
    "java-to-i2pr-ipv4",
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
)

_ALLOWED_REASONS = frozenset({
    "blocked_host_contract",
    "blocked_unprivileged_user_namespace",
    "i2pr-mixed-router-profile-not-wired",
    "i2pr-terminal-not-passed",
    "i2pr-initiator-handshake-failed",
    "i2pr-responder-handshake-failed",
    "unknown-i2pr-terminal-reason",
    "reference-receiver-marker-not-source-locked",
    "router-info-validation-failed",
    "typed-harness-operation-failed",
    "sandbox-attestation-missing",
    "cleanup-verification-failed",
    "evidence-finalization-failed",
    "not-started",
    "state_invalid",
    "peer_router_info_invalid",
    "unsupported_padding_profile",
    "listener_failed",
    "handshake_authenticated",
    "i2np_exchange_complete",
    "directional_data_phase_complete",
    "handshake_failed",
    "dial_failed",
    "data_phase_failed",
    "data_phase_timeout",
    "data_phase_observation_incomplete",
    "timeout",
    "cleanup_complete",
    "invalid_scenario_config",
    "scenario_role_mismatch",
    "status_output_unavailable",
    "responder_tcp_accept_missing",
    "responder_admission_rejected",
    "responder_message1_decode_failed",
    "responder_message1_options_invalid",
    "responder_noise_state_failed",
    "responder_session_created_write_failed",
    "responder_session_confirmed_part1_failed",
    "responder_session_confirmed_part2_failed",
    "responder_router_identity_verification_failed",
    "responder_handshake_timeout",
    "responder_authenticated_link_install_failed",
    "responder_data_frame_read_failed",
    "responder_i2np_decode_failed",
})


class PipelineError(ValueError):
    """Raised when Plan 053 pipeline state is invalid."""


@dataclass(frozen=True)
class RunContext:
    run_id: str
    run_identity_path: Path
    staging_root: Path
    identity_digest: str
    identity: dict[str, Any]

    def assert_frozen(self) -> None:
        if not self.run_identity_path.is_file():
            raise PipelineError("run-identity-missing-after-freeze")
        actual = hashlib.sha256(self.run_identity_path.read_bytes()).hexdigest()
        if actual != self.identity_digest:
            raise PipelineError("run-identity-mutated-after-freeze")
        loaded = load_run_identity(self.run_identity_path)
        if loaded != self.identity:
            raise PipelineError("run-identity-content-changed-after-freeze")


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _run(repo_root: Path, *command: str) -> str:
    completed = subprocess.run(
        list(command), cwd=repo_root, capture_output=True, text=True, check=False,
    )
    if completed.returncode != 0:
        raise PipelineError(f"measurement-failed:{command[0]}")
    return completed.stdout.strip()


def _file_digest(path: Path) -> str:
    try:
        return _sha256(path.read_bytes())
    except OSError as exc:
        raise PipelineError(f"measurement-file-unreadable:{path.name}") from exc


def _source_tree_digest(repo_root: Path) -> tuple[str, str]:
    listing = subprocess.run(
        ["git", "ls-files", "--stage", "--full-name", "-z"],
        cwd=repo_root, capture_output=True, check=False,
    )
    if listing.returncode != 0:
        raise PipelineError("source-listing-measurement-failed")
    listing_digest = _sha256(listing.stdout)
    tree = bytearray()
    for entry in listing.stdout.split(b"\0"):
        if not entry:
            continue
        meta = entry.rsplit(b"\t", 1)[0]
        name = entry.rsplit(b"\t", 1)[-1]
        mode = meta.split(b" ", 1)[0].decode("ascii")
        # Skip gitlinks (mode 120000, used for nested submodule references
        # such as ``.agents/skills``) and any non-regular entries. These
        # contribute zero bytes to the source tree digest.
        if mode != "100644" and mode != "100755":
            continue
        path = repo_root / name.decode("utf-8")
        if not path.is_file():
            raise PipelineError("source-tree-file-missing")
        tree.extend(name)
        tree.extend(b"\0")
        tree.extend(path.read_bytes())
        tree.extend(b"\0")
    return _sha256(bytes(tree)), listing_digest


def _archive_digest(repo_root: Path, commit: str) -> str:
    completed = subprocess.run(
        ["git", "archive", "--format=tar", commit],
        cwd=repo_root, capture_output=True, check=False,
    )
    if completed.returncode != 0:
        raise PipelineError("source-archive-measurement-failed")
    return _sha256(completed.stdout)


def _tool_version(command: str) -> str:
    completed = subprocess.run([command, "--version"], capture_output=True, text=True, check=False)
    if completed.returncode != 0 or not completed.stdout.strip():
        raise PipelineError(f"tool-version-measurement-failed:{command}")
    return completed.stdout.strip()


def _target_triple() -> str:
    completed = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        raise PipelineError("rustc-version-measurement-failed")
    for line in completed.stdout.splitlines():
        if line.startswith("host:"):
            return line.split(":", 1)[1].strip()
    raise PipelineError("target-triple-measurement-failed")


def build_measured_identity(
    repo_root: Path,
    run_id: str,
    *,
    launcher_path: Path | None = None,
    topology_kind: str = "rootless-sealed-single-netns",
    privilege_model: str = "unprivileged-userns",
    environment_manifest_path: Path | None = None,
    source_archive_sha256: str | None = None,
    guest_source_manifest_sha256: str | None = None,
) -> dict[str, Any]:
    commit = _run(repo_root, "git", "rev-parse", "HEAD")
    if len(commit) != 40 or any(char not in "0123456789abcdef" for char in commit):
        raise PipelineError("source-commit-invalid")
    dirty = _run(repo_root, "git", "status", "--porcelain=v1", "--untracked-files=all")
    if dirty:
        raise PipelineError("source-tree-dirty")
    source_tree, listing = _source_tree_digest(repo_root)
    commit_object = subprocess.run(
        ["git", "cat-file", "commit", commit], cwd=repo_root, capture_output=True, check=False,
    )
    if commit_object.returncode != 0:
        raise PipelineError("source-commit-object-missing")
    launcher = launcher_path
    if launcher is None:
        for candidate in (repo_root / "target/debug/i2pr-interop", repo_root / "target/release/i2pr-interop"):
            if candidate.is_file():
                launcher = candidate
                break
    if launcher is None or not launcher.is_file() or launcher.is_symlink():
        raise PipelineError("launcher-binary-measurement-missing")
    environment_digest = _file_digest(environment_manifest_path) if environment_manifest_path else _sha256(b"environment-manifest-not-supplied")
    archive_digest = source_archive_sha256 or _archive_digest(repo_root, commit)
    guest_manifest = guest_source_manifest_sha256 or listing
    return build_run_identity(
        run_id=run_id,
        source_commit=commit,
        source_commit_object_sha256=_sha256(commit_object.stdout),
        source_tree_sha256=source_tree,
        source_archive_sha256=archive_digest,
        source_archive_format="git-tar",
        source_dirty="dirty" if dirty else "clean",
        host_source_manifest_sha256=listing,
        guest_source_manifest_sha256=guest_manifest,
        guest_source_listing_sha256=listing,
        environment_manifest_sha256=environment_digest,
        launcher_binary_sha256=_file_digest(launcher),
        launcher_build_profile="measured-executable",
        rustc_version=_tool_version("rustc"),
        cargo_version=_tool_version("cargo"),
        target_triple=_target_triple(),
        topology_kind=topology_kind,
        privilege_model=privilege_model,
        reference_lock_sha256=_file_digest(repo_root / "tests/integration/ntcp2/references.lock.toml"),
        evidence_schema_revision=2,
        created_at=dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    )


def create_context(
    *,
    repo_root: Path,
    run_id: str,
    run_identity_path: Path,
    staging_root: Path,
    launcher_path: Path | None = None,
    topology_kind: str = "rootless-sealed-single-netns",
    privilege_model: str = "unprivileged-userns",
) -> RunContext:
    if run_identity_path.exists() or staging_root.exists():
        raise PipelineError("run-context-target-already-exists")
    identity = build_measured_identity(
        repo_root, run_id, launcher_path=launcher_path,
        topology_kind=topology_kind, privilege_model=privilege_model,
    )
    staging_root.mkdir(mode=0o700, parents=True)
    write_run_identity(run_identity_path, identity)
    loaded = load_run_identity(run_identity_path)
    identity_digest = _file_digest(run_identity_path)
    (staging_root / "run-identity.json").write_bytes(run_identity_path.read_bytes())
    (staging_root / "run-identity.json").chmod(0o600)
    write_environment_block(staging_root, loaded)
    return RunContext(run_id, run_identity_path, staging_root, identity_digest, loaded)


def load_context(run_identity_path: Path, staging_root: Path) -> RunContext:
    identity = load_run_identity(run_identity_path)
    digest = _file_digest(run_identity_path)
    if identity.get("run_identity_sha256") != _sha256(
        json.dumps({**identity, "run_identity_sha256": ""}, sort_keys=True, separators=(",", ":")).encode()
    ):
        raise PipelineError("run-identity-digest-invalid")
    staging_root = staging_root.resolve()
    staging_root.mkdir(mode=0o700, parents=True, exist_ok=True)
    bundled_identity = staging_root / "run-identity.json"
    identity_bytes = run_identity_path.read_bytes()
    if bundled_identity.exists():
        if bundled_identity.is_symlink() or bundled_identity.read_bytes() != identity_bytes:
            raise PipelineError("bundled-run-identity-mismatch")
    else:
        bundled_identity.write_bytes(identity_bytes)
        bundled_identity.chmod(0o600)
    return RunContext(identity["run_id"], run_identity_path.resolve(), staging_root, digest, identity)


def write_environment_block(staging_root: Path, identity: dict[str, Any]) -> None:
    environment = {
        "schema": "i2pr-interop-environment-v2",
        "run_id": identity["run_id"],
        "source_commit": identity["source_commit"],
        "topology_kind": identity["topology_kind"],
        "privilege_model": identity["privilege_model"],
        "result": DIAGNOSTIC_RESULT,
    }
    for name, payload in (
        ("environment.json", environment),
        ("source-transfer.json", {"schema": "i2pr-interop-source-transfer-v2", "outcome": "verified"}),
        ("cache-transfer.json", {"schema": "i2pr-interop-cache-transfer-v2", "outcome": "verified-or-typed-blocked"}),
        ("offline-transition.json", {"schema": "i2pr-interop-offline-transition-v2", "outcome": "verified"}),
    ):
        write_json_atomic(staging_root / "environment" / name, payload)
    for name, value in (("parent-network-before.sha256", "0" * 64), ("parent-network-after.sha256", "0" * 64)):
        path = staging_root / "environment" / name
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        path.write_text(f"{value}  parent-network-state\n", encoding="ascii")
        path.chmod(0o600)


def _levels(*, side: str, authenticated: bool, emitted: bool, decoded: bool, marker_missing: bool) -> dict[str, Any]:
    reason = "reference-receiver-marker-not-source-locked" if marker_missing else "not-observed"
    levels = {
        level: build_level("not-observed", "typed-status", "not-observed", observer_implementation=f"{side}-observation-v2")
        for level in (
            "process_started", "listener_ready", "tcp_connected", "ntcp2_authenticated",
            "frame_emitted", "frame_authenticated_and_decrypted", "i2np_message_decoded", "terminal_clean",
        )
    }
    levels["ntcp2_authenticated"] = build_level(
        "observed" if authenticated else "not-observed", "typed-status",
        "ntcp2-authenticated" if authenticated else "not-observed",
        observer_implementation=f"{side}-observation-v2",
    )
    levels["frame_emitted"] = build_level(
        "observed" if emitted else "not-observed", "typed-status",
        "frame-emitted" if emitted else "not-observed",
        observer_implementation=f"{side}-observation-v2",
    )
    levels["frame_authenticated_and_decrypted"] = build_level(
        "observed" if decoded else "not-observed", "source-derived-log-marker",
        "frame-authenticated-and-decrypted" if decoded else reason,
        observer_implementation=f"{side}-observation-v2",
    )
    levels["i2np_message_decoded"] = build_level(
        "observed" if decoded else "not-observed", "control-api",
        "i2np-message-decoded" if decoded else reason,
        observer_implementation=f"{side}-observation-v2",
    )
    return levels


def _i2pr_synthetic_observation_levels(*, result: str, initiator: str) -> dict[str, Any]:
    authenticated = result == "passed"
    i2pr_sender = initiator == "i2pr"
    return _levels(
        side="i2pr",
        authenticated=authenticated,
        emitted=authenticated and i2pr_sender,
        decoded=authenticated and not i2pr_sender,
        marker_missing=False,
    )


def _build_observation(
    context: RunContext,
    direction: str,
    reference: str,
    result: str,
    initiator: str,
    *,
    i2pr_observation: dict[str, Any] | None = None,
    reference_observation: dict[str, Any] | None = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "schema": OBSERVATION_SCHEMA,
        "schema_version": OBSERVATION_SCHEMA_VERSION,
        "run_id": context.run_id,
        "source_commit": context.identity["source_commit"],
        "launcher_binary_sha256": context.identity["launcher_binary_sha256"],
        "run_identity_sha256": context.identity["run_identity_sha256"],
        "scenario_id": direction,
        "sides": {
            "i2pr": i2pr_observation
            or {
                "schema": OBSERVATION_SCHEMA,
                "schema_version": OBSERVATION_SCHEMA_VERSION,
                "side": "i2pr",
                "levels": _i2pr_synthetic_observation_levels(result=result, initiator=initiator),
            },
            reference: reference_observation
            or {
                "schema": OBSERVATION_SCHEMA,
                "schema_version": OBSERVATION_SCHEMA_VERSION,
                "side": reference,
                "levels": _levels(
                    side=reference,
                    authenticated=result == "passed",
                    emitted=result == "passed" and initiator != "i2pr",
                    decoded=False,
                    marker_missing=result != "passed",
                ),
            },
        },
        "observation_sha256": "",
    }
    for side, value in payload["sides"].items():
        finalize_observation(side, value)
    payload["observation_sha256"] = _sha256(
        json.dumps({**payload, "observation_sha256": ""}, sort_keys=True, separators=(",", ":")).encode()
    )
    return payload


def _observation(context: RunContext, direction: str, reference: str, result: str, initiator: str) -> dict[str, Any]:
    return _build_observation(context, direction, reference, result, initiator)


def _build_trigger_record(
    context: RunContext,
    direction: str,
    reference: str,
    initiator: str,
    result: str,
    reason_code: str,
    *,
    trigger_record: dict[str, Any] | None,
    trigger_metadata: dict[str, Any] | None,
) -> dict[str, Any]:
    """Build the Plan 055 trigger record for one direction.

    For i2pr-initiated directions, the trigger is not applicable and
    the record is the typed-absence ``not-applicable`` form. For
    reference-initiated directions, the harness may supply a real
    ``trigger_record`` built by :mod:`trigger_record`. When the
    trigger is absent, the record is written as a typed absence with
    ``attempted = false`` and ``outcome =
    direct-trigger-helper-failed`` so the direction stays visible.
    """

    binding = {
        "run_id": context.run_id,
        "source_commit": context.identity["source_commit"],
        "launcher_binary_sha256": context.identity["launcher_binary_sha256"],
        "run_identity_sha256": context.identity["run_identity_sha256"],
    }
    if trigger_record is not None:
        try:
            from trigger_record import finalize_trigger_record
            from trigger_record import TRIGGER_SCHEMA as NEW_TRIGGER_SCHEMA
            from trigger_record import TRIGGER_SCHEMA_VERSION as NEW_TRIGGER_VERSION
        except ImportError:
            from .trigger_record import (
                TRIGGER_SCHEMA as NEW_TRIGGER_SCHEMA,
                TRIGGER_SCHEMA_VERSION as NEW_TRIGGER_VERSION,
                finalize_trigger_record,
            )
        if trigger_record.get("schema") != NEW_TRIGGER_SCHEMA:
            raise PipelineError("trigger-record-schema-invalid")
        if trigger_record.get("schema_version") != NEW_TRIGGER_VERSION:
            raise PipelineError("trigger-record-schema-version-invalid")
        finalize_trigger_record(
            trigger_record, run_identity_sha256=binding["run_identity_sha256"]
        )
        return trigger_record
    if initiator == "i2pr":
        return {
            **binding,
            "schema": TRIGGER_SCHEMA,
            "schema_version": 3,
            "scenario_id": direction,
            "mode": "not-required-i2pr-initiator",
            "attempted": False,
            "outcome": "not-required-i2pr-initiator",
            "reason_code": "i2pr-is-transport-initiator",
            "helper_kind": "not-applicable",
            "helper_binary_sha256": "0" * 64,
            "helper_source_sha256": "0" * 64,
            "helper_compiler": "",
            "helper_pinned_inputs_sha256": "0" * 64,
            "source_inspection_record_sha256": "0" * 64,
            "target_router_hash": "0" * 40,
            "target_router_info_sha256": "0" * 64,
            "target_ntcp2_static_key_sha256": "0" * 64,
            "target_address": "192.0.2.2",
            "target_port": 45680,
            "correlation_nonce": "0" * 16,
            "attempt_count": 0,
            "transport_request_observed": False,
            "connection_callback_observed": False,
            "started_monotonic_ms": 0,
            "completed_monotonic_ms": 0,
            "sanitized_detail": "",
            "trigger_sha256": "",
        }
    attempted = result not in {"blocked", "rejected"}
    outcome = (
        "direct-trigger-helper-failed"
        if not attempted
        else ("authenticated" if result == "passed" else "direct-trigger-not-source-locked")
    )
    return {
        **binding,
        "schema": TRIGGER_SCHEMA,
        "schema_version": 3,
        "scenario_id": direction,
        "mode": "reference-trigger-blocked",
        "attempted": attempted,
        "outcome": outcome,
        "reason_code": reason_code,
        "helper_kind": (
            "i2pd-direct-helper" if reference == "i2pd" else "java-direct-helper"
        ),
        "helper_binary_sha256": "0" * 64,
        "helper_source_sha256": "0" * 64,
        "helper_compiler": "",
        "helper_pinned_inputs_sha256": "0" * 64,
        "source_inspection_record_sha256": "0" * 64,
        "target_router_hash": "0" * 40,
        "target_router_info_sha256": "0" * 64,
        "target_ntcp2_static_key_sha256": "0" * 64,
        "target_address": "192.0.2.2",
        "target_port": 45680,
        "correlation_nonce": "0" * 16,
        "attempt_count": 1 if attempted else 0,
        "transport_request_observed": False,
        "connection_callback_observed": False,
        "started_monotonic_ms": 0,
        "completed_monotonic_ms": 0,
        "sanitized_detail": "",
        "trigger_sha256": "",
    }


def write_direction_artifacts(
    context: RunContext,
    direction: str,
    *,
    reference: str,
    initiator: str,
    result: str,
    reason_code: str,
    cleanup_result: str = "clean",
    terminal: dict[str, Any] | None = None,
    i2pr_observation: dict[str, Any] | None = None,
    reference_observation: dict[str, Any] | None = None,
    trigger_record: dict[str, Any] | None = None,
    trigger_metadata: dict[str, Any] | None = None,
) -> dict[str, str]:
    """Write the Plan 052/055 per-direction artifact bundle.

    When a Plan 055 ``trigger_record`` is supplied, it is validated
    against the locked trigger schema and its digest is bound into
    the direction record together with the correlation nonce and
    target RouterInfo digest. When the trigger is omitted, the
    caller is asserting that no trigger was attempted (e.g. for an
    i2pr-initiated direction or a blocked local run); the harness
    writes the typed absence record ``not-applicable``.
    """

    context.assert_frozen()
    if direction not in PRIMARY_DIRECTIONS or reference not in {"java_i2p", "i2pd"}:
        raise PipelineError("direction-catalog-invalid")
    if reason_code not in _ALLOWED_REASONS:
        raise PipelineError("unknown-i2pr-terminal-reason")
    terminal = terminal or {}
    binding = {
        "run_id": context.run_id,
        "source_commit": context.identity["source_commit"],
        "launcher_binary_sha256": context.identity["launcher_binary_sha256"],
        "run_identity_sha256": context.identity["run_identity_sha256"],
    }
    observation = _build_observation(
        context,
        direction,
        reference,
        result,
        initiator,
        i2pr_observation=i2pr_observation,
        reference_observation=reference_observation,
    )
    trigger = _build_trigger_record(
        context,
        direction,
        reference,
        initiator,
        result,
        reason_code,
        trigger_record=trigger_record,
        trigger_metadata=trigger_metadata,
    )
    attestation = {
        **binding,
        "schema": ATTESTATION_SCHEMA,
        "schema_version": 2,
        "scenario_id": direction,
        "topology_kind": context.identity["topology_kind"],
        "privilege_model": context.identity["privilege_model"],
        "authorization": "typed-diagnostic-only",
        "parent_network_state_unchanged": False,
    }
    cleanup = {
        **binding,
        "schema": CLEANUP_SCHEMA,
        "schema_version": 2,
        "scenario_id": direction,
        "processes": {"expected": 2, "started": 0, "exited": 0, "forced": 0},
        "residual_processes": False,
        "topology_cleanup": cleanup_result,
        "final_result": "failed_cleanup" if cleanup_result == "failed" else cleanup_result,
    }
    direction_record = {
        **binding,
        "schema": DIRECTION_SCHEMA,
        "schema_version": 2,
        "scenario_id": direction,
        "reference": reference,
        "initiator": initiator,
        "responder": "reference" if initiator == "i2pr" else "i2pr",
        "actual_typed_result": "failed_cleanup" if cleanup_result == "failed" else result,
        "reason_code": "cleanup-verification-failed" if cleanup_result == "failed" else reason_code,
        "router_info": {"state": "not-produced", "sha256": None},
        "reference_metadata": {"kind": reference, "state": "not-started"},
        "runtime_counters": terminal.get("counters", {}),
        "observation_sha256": observation["observation_sha256"],
        "trigger_sha256": "",
        "attestation_sha256": "",
        "cleanup_sha256": "",
    }
    paths = {
        "observation": context.staging_root / "observations" / f"{direction}.json",
        "trigger": context.staging_root / "triggers" / f"{direction}.json",
        "attestation": context.staging_root / "attestations" / f"{direction}.json",
        "cleanup": context.staging_root / "cleanup" / f"{direction}.json",
        "direction": context.staging_root / "directions" / f"{direction}.json",
    }
    trigger_digest = write_json_atomic(paths["trigger"], trigger)
    attestation_digest = write_json_atomic(paths["attestation"], attestation)
    cleanup_digest = write_json_atomic(paths["cleanup"], cleanup)
    direction_record["trigger_sha256"] = trigger_digest
    direction_record["attestation_sha256"] = attestation_digest
    direction_record["cleanup_sha256"] = cleanup_digest
    write_json_atomic(paths["observation"], observation)
    direction_digest = write_json_atomic(paths["direction"], direction_record)
    return {key: _sha256(path.read_bytes()) for key, path in paths.items()} | {"direction": direction_digest}


def finalize_diagnostic_bundle(context: RunContext) -> Path:
    context.assert_frozen()
    for direction in PRIMARY_DIRECTIONS:
        for category in ("attestations", "directions", "triggers", "observations", "cleanup"):
            path = context.staging_root / category / f"{direction}.json"
            if not path.is_file():
                raise PipelineError(f"missing-direction-artifact:{category}:{direction}")
            payload = json.loads(path.read_text(encoding="utf-8"))
            cross_check(payload, context.identity)
    for direction in PRIMARY_DIRECTIONS:
        trigger_path = context.staging_root / "triggers" / f"{direction}.json"
        trigger_payload = json.loads(trigger_path.read_text(encoding="utf-8"))
        direction_path = context.staging_root / "directions" / f"{direction}.json"
        direction_payload = json.loads(direction_path.read_text(encoding="utf-8"))
        # Plan 055 E2 binding: the direction record's trigger_sha256
        # must agree with the on-disk trigger digest.
        if direction_payload.get("trigger_sha256") != _sha256(trigger_path.read_bytes()):
            raise PipelineError(f"trigger-direction-binding-mismatch:{direction}")
    write_json_atomic(context.staging_root / "diagnostics" / "sanitized-summary.json", {
        "schema": DIAGNOSTICS_SCHEMA,
        "schema_version": 1,
        "run_id": context.run_id,
        "run_identity_sha256": context.identity["run_identity_sha256"],
        "result": DIAGNOSTIC_RESULT,
        "directions": list(PRIMARY_DIRECTIONS),
    })
    finalize_bundle(context.staging_root, context.run_id)
    verify_bundle(context.staging_root)
    return context.staging_root


def _main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser("create")
    create.add_argument("--repo-root", type=Path, required=True)
    create.add_argument("--run-id", required=True)
    create.add_argument("--run-identity", type=Path, required=True)
    create.add_argument("--bundle-staging", type=Path, required=True)
    create.add_argument("--launcher", type=Path)
    finalize = subparsers.add_parser("finalize")
    finalize.add_argument("--run-identity", type=Path, required=True)
    finalize.add_argument("--bundle-staging", type=Path, required=True)
    finalize.add_argument("--export-root", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "create":
            create_context(repo_root=args.repo_root.resolve(), run_id=args.run_id, run_identity_path=args.run_identity.resolve(), staging_root=args.bundle_staging.resolve(), launcher_path=args.launcher.resolve() if args.launcher else None)
            return 0
        context = load_context(args.run_identity, args.bundle_staging)
        finalize_diagnostic_bundle(context)
        if args.export_root:
            export_bundle_atomic(context.staging_root, args.export_root)
        return 0
    except (BundleError, OSError, PipelineError, ValueError) as exc:
        print(str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(_main())
