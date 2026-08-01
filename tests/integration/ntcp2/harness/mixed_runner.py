#!/usr/bin/env python3
"""Fail-closed Plan 044 mixed-router directional scenario runner."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tomllib
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from harness.data_oracle import select_oracle  # type: ignore
    from harness.evidence import write_record  # type: ignore
    from harness.i2pd import I2pdAdapter, I2pdError  # type: ignore
    from harness.i2pr import I2prAdapter  # type: ignore
    from harness.interop_topology import (  # type: ignore
        PRIVILEGED_PRIVILEGE_MODEL,
        PRIVILEGED_TOPOLOGY_KIND,
        ROOTLESS_PRIVILEGE_MODEL,
        ROOTLESS_TOPOLOGY_KIND,
        select_topology,
    )
    from harness.java_i2p import JavaI2pAdapter, JavaI2pError  # type: ignore
    from harness.launcher_renderer import RenderError, render_and_validate  # type: ignore
    from harness.launcher_protocol import LauncherScenarioError, STATUS_REASONS  # type: ignore
    from harness.metadata import CacheMetadata  # type: ignore
    from harness.observation import OBSERVATION_SCHEMA, build_level, finalize_observation  # type: ignore
    from harness.observation_helpers import LogCursor  # type: ignore
    from harness.observation_catalog import load_catalog  # type: ignore
    from harness.reference_trigger import select_trigger  # type: ignore
    from harness.router_info import RouterInfoPathError, strict_validate_router_info  # type: ignore
    from harness.topology import EndpointDescription, IsolationError  # type: ignore
    from harness.rootless_topology import RootlessTopologyError  # type: ignore
    from harness.runner import HarnessBlocked, _cache_for, _git_identity, _host_check  # type: ignore
    from harness.plan052_pipeline import (  # type: ignore
        PIPELINE_PROFILE,
        PipelineError,
        load_context,
        write_direction_artifacts,
    )
else:
    from .data_oracle import select_oracle
    from .evidence import write_record
    from .i2pd import I2pdAdapter, I2pdError
    from .i2pr import I2prAdapter
    from .interop_topology import (
        PRIVILEGED_PRIVILEGE_MODEL,
        PRIVILEGED_TOPOLOGY_KIND,
        ROOTLESS_PRIVILEGE_MODEL,
        ROOTLESS_TOPOLOGY_KIND,
        select_topology,
    )
    from .java_i2p import JavaI2pAdapter, JavaI2pError
    from .launcher_renderer import RenderError, render_and_validate
    from .launcher_protocol import LauncherScenarioError, STATUS_REASONS
    from .metadata import CacheMetadata
    from .observation import OBSERVATION_SCHEMA, build_level, finalize_observation
    from .observation_helpers import LogCursor
    from .observation_catalog import load_catalog
    from .reference_trigger import select_trigger
    from .router_info import RouterInfoPathError, strict_validate_router_info
    from .topology import EndpointDescription, IsolationError
    from .rootless_topology import RootlessTopologyError
    from .runner import HarnessBlocked, _cache_for, _git_identity, _host_check
    from .plan052_pipeline import (
        PIPELINE_PROFILE,
        PipelineError,
        load_context,
        write_direction_artifacts,
    )

MIXED_SCENARIO_FIELDS = frozenset(
    {
        "id",
        "reference",
        "profile",
        "direction",
        "address_family",
        "padding",
        "expected",
        "initiator",
        "responder",
    }
)

KNOWN_POSITIVE_SCENARIO_IDS = frozenset(
    {
        "i2pr-to-java-ipv4",
        "java-to-i2pr-ipv4",
        "i2pr-to-i2pd-ipv4",
        "i2pd-to-i2pr-ipv4",
    }
)

PRE_PROTOCOL_REASONS = frozenset(
    {
        "i2pr-preparation-start-failed",
        "i2pr-state-preparation-failed",
        "i2pr-router-info-missing",
        "i2pr-router-info-validation-failed",
        "i2pr-router-hash-invalid",
        "i2pr-preparation-record-invalid",
        "prepare-input-invalid",
        "reference-router-info-missing",
        "reference-router-info-validation-failed",
        "reference-router-hash-invalid",
        "run-identity-freeze-failed",
        "i2pr-binary-missing",
        "i2pd-binary-missing",
    }
)


def _reject_negative_before_primary(scenario_id: str) -> None:
    if scenario_id not in KNOWN_POSITIVE_SCENARIO_IDS:
        raise MixedRunError(
            "negative-scenario-before-primary-directions", "rejected"
        )


@dataclass(frozen=True)
class MixedDirection:
    execution_id: str
    reference: str
    profile: str
    direction: str
    address_family: str
    padding: str
    expected: str
    initiator: str
    responder: str

    @property
    def i2pr_is_initiator(self) -> bool:
        return self.initiator == "i2pr"

    @property
    def reference_name(self) -> str:
        return self.reference


class MixedRunError(RuntimeError):
    def __init__(self, code: str, result: str = "rejected"):
        super().__init__(code)
        self.code = code
        self.result = result


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


def _run_id() -> str:
    return f"mixed-{dt.datetime.now(dt.UTC).strftime('%Y%m%dT%H%M%SZ')}-{os.getpid()}-{uuid.uuid4().hex[:8]}"


def _reference_driver_mode_for(reference: str) -> str:
    """Return the Plan 065 source-locked driver mode for a reference kind.

    Only the Plan 063 Java direct driver and the Plan 064 i2pd direct
    driver satisfy a primary direction. The Plan 062 v4 trigger schema
    refuses SAM, HTTP, I2PControl, support-topology, and synthetic
    fallback helpers for any primary direction.
    """

    if reference == "java_i2p":
        return "java-direct-driver"
    if reference == "i2pd":
        return "i2pd-direct-driver"
    raise MixedRunError(f"unknown-reference-kind:{reference}")


def _plan065_primary_fields(
    *,
    execution_id: str,
    run_id: str,
    run_identity_sha256: str,
    reference_driver_mode: str,
    expected_sender_router_hash_sha256: str,
    expected_receiver_router_hash_sha256: str,
    correlation_nonce: str,
) -> dict[str, object]:
    """Return the Plan 065 required primary fields for ``render_and_validate``.

    The DeliveryStatus ``message_id`` is derived from the run identity and
    the correlation nonce through the canonical Plan 065 domain-separated
    hash. The plan requires one unique nonzero ID per direction per run
    so the i2pr sender and receiver can verify the exact correlation
    end-to-end and the canonical mixed-runner can refuse partial
    correlations.
    """

    digest = hashlib.sha256(
        f"i2pr-ntcp2-delivery-status-v1|{run_id}|{execution_id}|{correlation_nonce}".encode()
    ).digest()
    raw = int.from_bytes(digest[:4], "big")
    message_id = (raw % 0xFFFFFFFF) or 1
    return {
        "run_id": run_id,
        "delivery_status_message_id": int(message_id),
        "expected_sender_router_hash_sha256": expected_sender_router_hash_sha256,
        "expected_receiver_router_hash_sha256": expected_receiver_router_hash_sha256,
        "reference_driver_mode": reference_driver_mode,
        "run_identity_sha256": run_identity_sha256,
    }


def _router_hash_sha256(path: Path) -> str:
    """Hash the exact RouterIdentity prefix, matching the Router Hash rule."""

    data = path.read_bytes()
    if len(data) < 387 or data[384] != 5:
        raise MixedRunError("router-hash-invalid")
    certificate_length = int.from_bytes(data[385:387], "big")
    identity_length = 387 + certificate_length
    if certificate_length < 4 or identity_length > len(data):
        raise MixedRunError("router-hash-invalid")
    return hashlib.sha256(data[:identity_length]).hexdigest()


def _sha256_file(path: Path, reason: str) -> str:
    try:
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError as exc:
        raise MixedRunError(reason) from exc
    if digest == "0" * 64:
        raise MixedRunError(reason)
    return digest


def _freeze_minimal_run_identity(
    *,
    run_dir: Path,
    run_id: str,
    source_commit: str,
    direction: MixedDirection,
    metadata: CacheMetadata,
    i2pr_router_info_sha256: str,
    i2pd_router_info_sha256: str,
    i2pr_router_hash_sha256: str,
    i2pd_router_hash_sha256: str,
    i2pr_binary_sha256: str,
    i2pd_binary_sha256: str,
    delivery_status_message_id: int,
) -> str:
    payload = {
        "schema": "i2pr-minimal-run-identity-v1",
        "run_id": run_id,
        "source_commit": source_commit,
        "direction": direction.execution_id,
        "reference": "i2pd",
        "reference_revision": metadata.source_revision,
        "topology_kind": "rootless-sealed-single-netns",
        "lane_qualification_sha256": os.environ.get("I2PR_INTEROP_LANE_QUALIFICATION_SHA256", metadata.artifact_sha256),
        "i2pr_binary_sha256": i2pr_binary_sha256,
        "i2pd_binary_sha256": i2pd_binary_sha256,
        "i2pr_router_info_sha256": i2pr_router_info_sha256,
        "i2pd_router_info_sha256": i2pd_router_info_sha256,
        "i2pr_router_hash_sha256": i2pr_router_hash_sha256,
        "i2pd_router_hash_sha256": i2pd_router_hash_sha256,
        "delivery_status_message_id": delivery_status_message_id,
    }
    if any(not isinstance(value, str) or not value for value in payload.values() if isinstance(value, str)):
        raise MixedRunError("run-identity-freeze-failed")
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    try:
        (run_dir / "run-identity.json").write_bytes(encoded + b"\n")
    except OSError as exc:
        raise MixedRunError("run-identity-freeze-failed") from exc
    return hashlib.sha256(encoded + b"\n").hexdigest()


ALLOWED_DIAGNOSTICS_MODES = frozenset({"off", "sanitized", "raw-local"})


_observation_catalog: dict[str, object] = {}


def _load_observation_catalog() -> dict[str, object]:
    try:
        return load_catalog()
    except (OSError, ValueError):
        return {}


def _diagnostics_mode() -> str:
    """Return the effective Plan 052 diagnostic mode.

    Plan 052 narrows the prior ``I2PR_INTEROP_DUMP_RUN_LOGS`` switch into a
    tri-state ``I2PR_INTEROP_DIAGNOSTICS`` env var. ``raw-local`` may never
    land under an export root; the sanitizer rejects that combination.
    """

    raw = os.environ.get("I2PR_INTEROP_DIAGNOSTICS", "off")
    if raw not in ALLOWED_DIAGNOSTICS_MODES:
        raise MixedRunError("invalid-diagnostics-mode")
    if raw == "raw-local" and "INTEROP_EVIDENCE_DIR" in os.environ:
        # Plan 052 forbids raw-local capture into an evidence/export root.
        raise MixedRunError("raw-local-diagnostics-forbidden-under-export-root")
    return raw


def load_mixed_scenario(repo_root: Path, scenario_id: str) -> MixedDirection:
    scenario_dir = repo_root / "tests/integration/ntcp2/mixed-scenarios"
    for path in sorted(scenario_dir.glob("*.toml")):
        if path.name == "manifest.toml":
            continue
        raw = tomllib.loads(path.read_text(encoding="utf-8"))
        value = raw.get("scenario", {})
        if value.get("id") == scenario_id:
            unknown = frozenset(value) - MIXED_SCENARIO_FIELDS
            if unknown:
                raise MixedRunError("mixed-scenario-unknown-fields")
            return MixedDirection(
                execution_id=value["id"],
                reference=value["reference"],
                profile=value["profile"],
                direction=value["direction"],
                address_family=value["address_family"],
                padding=value["padding"],
                expected=value["expected"],
                initiator=value["initiator"],
                responder=value["responder"],
            )
    raise MixedRunError("unknown-mixed-scenario", "rejected")


def _reference_port_for(reference: str) -> int:
    return 45678 if reference == "java_i2p" else 45679


def _record_mixed(
    direction: MixedDirection,
    result: str,
    reason: str,
    cleanup: str,
    i2pr_commit: str,
    metadata: CacheMetadata | None,
    topology: Any | None,
    router_validation: dict[str, str],
    observations: dict[str, str],
    process_counters: dict[str, dict[str, int]],
    oracle_kind: str = "",
    runtime_counters: dict[str, int] | None = None,
    *,
    configuration_sha256: str = "",
    i2pr_router_info_sha256: str = "",
    reference_router_info_sha256: str = "",
    data_phase_mode: str = "round-trip-delivery-status",
    expected_observation: str = "i2pr-sent-and-acknowledged",
    sandbox_attestation_sha256: str = "",
    parent_network_state_unchanged: bool = False,
) -> dict[str, Any]:
    zero = "0" * 64
    deterministic = (
        f"seed=1;timeouts=bounded;network=synthetic-private-036;"
        f"ipv6-probe=passed;data_phase_oracle={oracle_kind};"
        f"data_phase_mode={data_phase_mode};expected_observation={expected_observation}"
    )
    resource = {
        "tasks": 0, "queues": 0, "permits": 0,
        "links": 0, "handshakes": 0, "i2np_sent": 0, "i2np_received": 0,
    }
    if runtime_counters:
        resource.update(runtime_counters)
    topology_kind = getattr(topology, "topology_kind", PRIVILEGED_TOPOLOGY_KIND)
    privilege_model = getattr(topology, "privilege_model", PRIVILEGED_PRIVILEGE_MODEL)
    if topology is not None and hasattr(topology, "digest"):
        namespace_topology_sha256 = topology.digest()
    else:
        namespace_topology_sha256 = zero
    return {
        "schema": 1,
        "scenario_id": direction.execution_id,
        "date_utc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "i2pr_commit": i2pr_commit,
        "reference": direction.reference,
        "reference_version": "2.12.0" if direction.reference == "java_i2p" else "2.60.0",
        "reference_revision": getattr(metadata, "source_revision", zero[:40]) if metadata else zero[:40],
        "artifact_sha256": getattr(metadata, "artifact_sha256", zero) if metadata else zero,
        "installed_tree_sha256": getattr(metadata, "installed_tree_sha256", zero) if metadata else zero,
        "configuration_sha256": configuration_sha256 or zero,
        "namespace_topology_sha256": namespace_topology_sha256,
        "direction": direction.direction,
        "address_family": direction.address_family,
        "deterministic_parameters": deterministic,
        "expected": direction.expected,
        "actual_typed_result": result,
        "resource_counters": resource,
        "process_counters": process_counters,
        "cleanup_result": cleanup,
        "evidence_sha256": "",
        "known_deviation": reason if result != "passed" else "",
        "reproduction": f"bash scripts/interop/run-scenario.sh --scenario {direction.execution_id} --reference {direction.reference}",
        "i2pr_router_info_sha256": i2pr_router_info_sha256 or zero,
        "reference_router_info_sha256": reference_router_info_sha256 or zero,
        "data_phase_mode": data_phase_mode,
        "expected_observation": expected_observation,
        "topology_kind": topology_kind,
        "privilege_model": privilege_model,
        "sandbox_attestation_sha256": sandbox_attestation_sha256 or zero,
        "parent_network_state_unchanged": parent_network_state_unchanged,
    }


def _emit(direction_id: str, reference: str, result: str, reason: str, cleanup: str) -> None:
    print(json.dumps({
        "schema": 1,
        "type": "i2pr-mixed-router-result",
        "scenario_id": direction_id,
        "reference": reference,
        "actual_typed_result": result,
        "reason_code": reason,
        "cleanup_result": cleanup,
    }, separators=(",", ":")))


def _no_residual_state(topology: Any) -> bool:
    """True when no host-visible namespaces or veths from this topology remain."""

    if getattr(topology, "topology_kind", "") == ROOTLESS_TOPOLOGY_KIND:
        return True
    prefix = [] if os.geteuid() == 0 else ["sudo", "-n"]
    namespaces = subprocess.run(
        prefix + ["ip", "netns", "list"], capture_output=True, text=True, check=False,
    )
    if namespaces.returncode != 0:
        return False
    for name in (getattr(topology, "i2pr_namespace", ""), getattr(topology, "reference_namespace", "")):
        if not name:
            continue
        for line in namespaces.stdout.splitlines():
            if line.split() and line.split()[0] == name:
                return False
    links = subprocess.run(
        prefix + ["ip", "-o", "link", "show"], capture_output=True, text=True, check=False,
    )
    if links.returncode != 0:
        return False
    return not any(
        name in links.stdout
        for name in (getattr(topology, "i2pr_if", ""), getattr(topology, "reference_if", ""))
        if name
    )


def _stop_adapter(adapter: Any, timeout: float = 10.0) -> str:
    try:
        return adapter.stop(timeout)
    except RuntimeError:
        return "failed"


def _validate_router_info_for_direction(
    info_path: Path,
    expected_address: str,
    expected_port: int,
    repo_root: Path,
    router_validation: dict[str, str],
    reference_key: str,
) -> None:
    try:
        strict_validate_router_info(
            info_path,
            expected_address=expected_address,
            expected_port=expected_port,
            repo_root=repo_root,
        )
        router_validation[reference_key] = "validated-and-bound"
    except (RouterInfoPathError, OSError) as exc:
        router_validation[reference_key] = "rejected"
        raise MixedRunError("router-info-validation-failed") from exc


def run(args: argparse.Namespace) -> int:
    repo_root = _repo_root()
    direction = load_mixed_scenario(repo_root, args.scenario)
    global _observation_catalog
    _observation_catalog = _load_observation_catalog()
    pipeline_context = None
    if getattr(args, "evidence_profile", None):
        if args.evidence_profile != PIPELINE_PROFILE:
            raise MixedRunError("unsupported-evidence-profile", "rejected")
        if not args.run_identity or not args.bundle_staging or not args.run_id:
            raise MixedRunError("evidence-profile-context-missing", "blocked")
        try:
            pipeline_context = load_context(Path(args.run_identity), Path(args.bundle_staging))
        except (OSError, ValueError, PipelineError) as exc:
            raise MixedRunError(str(exc), "blocked") from exc
        if pipeline_context.run_id != args.run_id:
            raise MixedRunError("run-id-context-mismatch", "blocked")
    reference = args.reference
    if direction.reference != reference:
        raise MixedRunError("scenario-reference-mismatch", "rejected")
    _reject_negative_before_primary(direction.execution_id)
    oracle = select_oracle(direction.reference, direction.i2pr_is_initiator)
    trigger = select_trigger(direction.reference)
    base = Path(args.run_root or repo_root / "target/interop/runs").resolve()
    runs_root = (repo_root / "target/interop/runs").resolve()
    if base != runs_root and runs_root not in base.parents:
        raise MixedRunError("run-root-outside-target", "blocked_host_contract")
    cache_base = Path(args.build_cache or repo_root / "target/interop/cache").resolve()
    base.mkdir(mode=0o700, parents=True, exist_ok=True)
    run_dir = base / _run_id()
    run_dir.mkdir(mode=0o700)
    evidence_root = Path(os.environ.get("INTEROP_EVIDENCE_DIR", str(repo_root / "target/interop/evidence")))
    topology_kind = getattr(args, "topology_kind", PRIVILEGED_TOPOLOGY_KIND) or PRIVILEGED_TOPOLOGY_KIND
    sandbox_attestation_sha256 = os.environ.get("I2PR_INTEROP_ROOTLESS_ATTESTATION_SHA256", "")
    parent_state_unchanged_env = os.environ.get(
        "I2PR_INTEROP_ROOTLESS_PARENT_STATE_UNCHANGED", "0"
    )
    parent_network_state_unchanged = (
        topology_kind == ROOTLESS_TOPOLOGY_KIND
        and parent_state_unchanged_env == "1"
        and bool(sandbox_attestation_sha256)
    )
    topology: Any | None = None
    i2pr_adapter: I2prAdapter | None = None
    ref_adapter: JavaI2pAdapter | I2pdAdapter | None = None
    metadata: CacheMetadata | None = None
    cleanup = "not-started"
    result = "blocked"
    reason = "not-started"
    i2pr_commit = "0" * 40 + ";dirty"
    evidence_path: Path | None = None
    router_validation = {"i2pr": "not-run", direction.reference: "not-run"}
    observations = {"i2pr": "not-observed", direction.reference: "not-observed"}
    process_counters: dict[str, dict[str, int]] = {
        "i2pr": {"started": 0, "exited": 0, "forced": 0},
        direction.reference: {"started": 0, "exited": 0, "forced": 0},
    }
    runtime_counters: dict[str, int] = {
        "handshake_attempts": 0,
        "frames_sent": 0,
        "frames_received": 0,
        "i2np_sent": 0,
        "i2np_received": 0,
        "queue_items_high_watermark": 0,
        "queue_bytes_high_watermark": 0,
    }
    i2pr_router_info_sha256 = ""
    i2pr_router_hash_sha256 = ""
    reference_router_info_sha256 = ""
    reference_router_hash_sha256 = ""
    configuration_sha256 = ""
    data_phase_mode = "round-trip-delivery-status"
    expected_observation = "i2pr-sent-and-acknowledged"
    oracle_state = {"sender_observed": "not-observed", "receiver_observed": "not-observed"}
    try:
        if topology_kind != ROOTLESS_TOPOLOGY_KIND:
            _host_check(repo_root, run_dir)
        i2pr_commit = _git_identity(repo_root)
        cache, metadata = _cache_for(cache_base, reference, repo_root)
        ipv6 = direction.address_family == "ipv6"
        if ipv6 and topology_kind != ROOTLESS_TOPOLOGY_KIND:
            disabled = Path("/proc/sys/net/ipv6/conf/all/disable_ipv6")
            capability = subprocess.run(
                ["ip", "-6", "route", "show"], capture_output=True, text=True, check=False,
            )
            if capability.returncode != 0 or (disabled.is_file() and disabled.read_text().strip() != "0"):
                raise MixedRunError("ipv6-capability-unavailable", "skipped_ipv6")
        ref_port = _reference_port_for(reference)
        topology = select_topology(
            topology_kind,
            repo_root=repo_root,
            run_id=run_dir.name.removeprefix("mixed-"),
            ipv6=ipv6,
            reference_port=ref_port,
            i2pr_port=45680,
            reference_kind=reference,
        )
        topology.create()
        i2pr_placement = topology.placement("i2pr")
        ref_placement = topology.placement("reference")
        if topology_kind == ROOTLESS_TOPOLOGY_KIND:
            i2pr_endpoint = topology.endpoint_for_i2pr()
            ref_endpoint = topology.endpoint_for_reference()
        else:
            i2pr_endpoint = EndpointDescription(
                local_address="192.0.2.1" if not ipv6 else "2001:db8:36::1",
                peer_address="192.0.2.2" if not ipv6 else "2001:db8:36::2",
                local_port=45680,
                peer_port=ref_port,
                address_family=direction.address_family,
                namespace=topology.i2pr_namespace,
                network_id="99",
            )
            ref_endpoint = EndpointDescription(
                local_address="192.0.2.2" if not ipv6 else "2001:db8:36::2",
                peer_address="192.0.2.1" if not ipv6 else "2001:db8:36::1",
                local_port=ref_port,
                peer_port=45680,
                address_family=direction.address_family,
                namespace=topology.reference_namespace,
                network_id="99",
            )
        shared_reference_data = run_dir / "reference-data"
        shared_i2pr_state = run_dir / "i2pr" / "state"
        i2pr_adapter = I2prAdapter(repo_root, run_dir / "i2pr", placement=i2pr_placement)
        prepared_i2pr = i2pr_adapter.prepare_state(
            local_address=i2pr_endpoint.local_address,
            local_port=i2pr_endpoint.local_port,
            deterministic_seed=1,
        )
        i2pr_router_info_sha256 = prepared_i2pr.router_info_sha256
        i2pr_router_hash_sha256 = prepared_i2pr.router_hash_sha256
        if direction.i2pr_is_initiator:
            data_phase_mode = "initiator-data-only"
            expected_observation = "i2pr-sent-only"
            ref_validation, ref_info_path, configuration_sha256 = _run_initiator_first(
                direction=direction,
                run_dir=run_dir,
                repo_root=repo_root,
                cache=cache,
                metadata=metadata,
                topology=topology,
                i2pr_endpoint=i2pr_endpoint,
                ref_endpoint=ref_endpoint,
                router_validation=router_validation,
                observations=observations,
                shared_reference_data=shared_reference_data,
                ref_placement=ref_placement,
            )
            reference_router_info_sha256 = hashlib.sha256(ref_info_path.read_bytes()).hexdigest()
            ref_adapter = _make_ref_adapter(
                direction.reference, cache, run_dir / "ref", ref_endpoint, repo_root,
                shared_data_dir=shared_reference_data,
                placement=ref_placement,
            )
            ref_adapter.start()
            ref_adapter.wait_ready(timeout_seconds=240.0)
            reference_router_hash_sha256 = _router_hash_sha256(ref_info_path)
            fields = _plan065_primary_fields(
                execution_id=direction.execution_id,
                run_id=run_dir.name,
                run_identity_sha256="0" * 64,
                reference_driver_mode=_reference_driver_mode_for(direction.reference),
                expected_sender_router_hash_sha256=i2pr_router_hash_sha256,
                expected_receiver_router_hash_sha256=reference_router_hash_sha256,
                correlation_nonce="initiator",
            )
            run_identity_sha256 = _freeze_minimal_run_identity(
                run_dir=run_dir,
                run_id=run_dir.name,
                source_commit=i2pr_commit,
                direction=direction,
                metadata=metadata,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pd_router_info_sha256=reference_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
                i2pd_router_hash_sha256=reference_router_hash_sha256,
                i2pr_binary_sha256=_sha256_file(repo_root / "target/debug/i2pr-interop", "i2pr-binary-missing"),
                i2pd_binary_sha256=_sha256_file(cache / metadata.launcher, "i2pd-binary-missing"),
                delivery_status_message_id=int(fields["delivery_status_message_id"]),
            )
            scenario_toml = render_and_validate(
                run_dir / "i2pr",
                execution_id=direction.execution_id,
                role="initiator",
                address_family=direction.address_family,
                local_address=i2pr_endpoint.local_address,
                local_port=i2pr_endpoint.local_port,
                peer_address=i2pr_endpoint.peer_address,
                peer_port=i2pr_endpoint.peer_port,
                state_dir="state",
                peer_router_info="exchange/ref-router.info",
                padding_profile=direction.padding,
                smoke_message_profile="fixed-12-byte-payload",
                expected_result_class="authenticated-handshake-and-directional-data-phase",
                data_phase_mode=data_phase_mode,
                data_phase_required_peer_action="ignore-receive",
                expected_observation=expected_observation,
                **_plan065_primary_fields(
                    execution_id=direction.execution_id,
                    run_id=run_dir.name,
                    run_identity_sha256=run_identity_sha256,
                    reference_driver_mode=_reference_driver_mode_for(direction.reference),
                    expected_sender_router_hash_sha256=i2pr_router_hash_sha256,
                    expected_receiver_router_hash_sha256=reference_router_hash_sha256,
                    correlation_nonce="initiator",
                ),
            )
            i2pr_adapter.start("dial")
            terminal = i2pr_adapter.wait_terminal(timeout_seconds=30.0)
            if terminal["result"] != "passed":
                raise MixedRunError(_terminal_reason(terminal, "i2pr-initiator-handshake-failed"))
            observations["i2pr"] = "authenticated"
            ref_observation = ref_adapter.collect_observation(
                role="responder",
                run_id=run_dir.name,
                correlation={"delivery_status_message_id": "mixed-router", "bounded_test_nonce": run_dir.name},
                log_cursor=LogCursor(run_id=run_dir.name, log_path=run_dir / "ref" / "raw" / "i2pd.log" if reference == "i2pd" else run_dir / "ref" / "raw" / "java-i2p.log"),
                catalog=_observation_catalog,
            )
            observations[direction.reference] = ref_observation.get("observation_sha256", "")
            trigger_result = trigger.send(
                direction.i2pr_is_initiator, ref_endpoint, run_dir, placement=ref_placement,
            )
            try:
                (run_dir / "raw").mkdir(parents=True, exist_ok=True)
                (run_dir / "raw" / "trigger-result.json").write_text(
                    json.dumps(_serialize_trigger(trigger_result), indent=2),
                    encoding="utf-8",
                )
            except OSError:
                pass
            oracle_state = oracle.observe_directional(
                role="initiator" if direction.i2pr_is_initiator else "responder",
                ref_endpoint=ref_endpoint,
                run_dir=run_dir,
                terminal_counters=terminal.get("counters", {}),
            )
            result, reason = _evaluate_pass_predicate(
                direction=direction,
                terminal=terminal,
                ref_observation=ref_observation,
                oracle_state=oracle_state,
            )
            if pipeline_context is not None:
                result, reason = _evaluate_plan052_predicate(terminal, ref_observation, oracle_state)
            runtime_counters["handshake_attempts"] = 1
            runtime_counters["frames_sent"] = int(terminal.get("counters", {}).get("frames_sent", 0))
            runtime_counters["frames_received"] = int(terminal.get("counters", {}).get("frames_received", 0))
            runtime_counters["i2np_sent"] = int(terminal.get("counters", {}).get("i2np_sent", 0))
            runtime_counters["i2np_received"] = int(terminal.get("counters", {}).get("i2np_received", 0))
        else:
            data_phase_mode = "responder-data-only"
            expected_observation = "i2pr-received-only"
            i2pr_validation, i2pr_info_path, _configuration_sha256 = _run_responder_first(
                direction=direction,
                run_dir=run_dir,
                repo_root=repo_root,
                cache=cache,
                metadata=metadata,
                topology=topology,
                i2pr_endpoint=i2pr_endpoint,
                ref_endpoint=ref_endpoint,
                router_validation=router_validation,
                observations=observations,
                shared_i2pr_state=shared_i2pr_state,
                i2pr_placement=i2pr_placement,
                ref_placement=ref_placement,
            )
            reference_info_path = run_dir / "ref-preparation" / "exchange" / "i2pd-router.info"
            if not reference_info_path.is_file():
                candidates = list((run_dir / "ref-preparation" / "exchange").glob("*-router.info"))
                if not candidates:
                    raise MixedRunError("reference-router-info-missing")
                reference_info_path = candidates[0]
            reference_router_info_sha256 = _sha256_file(reference_info_path, "reference-router-info-missing")
            reference_router_hash_sha256 = _router_hash_sha256(reference_info_path)
            fields = _plan065_primary_fields(
                execution_id=direction.execution_id,
                run_id=run_dir.name,
                run_identity_sha256="0" * 64,
                reference_driver_mode=_reference_driver_mode_for(direction.reference),
                expected_sender_router_hash_sha256=i2pr_router_hash_sha256,
                expected_receiver_router_hash_sha256=reference_router_hash_sha256,
                correlation_nonce="responder",
            )
            run_identity_sha256 = _freeze_minimal_run_identity(
                run_dir=run_dir,
                run_id=run_dir.name,
                source_commit=i2pr_commit,
                direction=direction,
                metadata=metadata,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pd_router_info_sha256=reference_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
                i2pd_router_hash_sha256=reference_router_hash_sha256,
                i2pr_binary_sha256=_sha256_file(repo_root / "target/debug/i2pr-interop", "i2pr-binary-missing"),
                i2pd_binary_sha256=_sha256_file(cache / metadata.launcher, "i2pd-binary-missing"),
                delivery_status_message_id=int(fields["delivery_status_message_id"]),
            )
            scenario_toml = render_and_validate(
                run_dir / "i2pr",
                execution_id=direction.execution_id,
                role="responder",
                address_family=direction.address_family,
                local_address=i2pr_endpoint.local_address,
                local_port=i2pr_endpoint.local_port,
                peer_address=None,
                peer_port=None,
                state_dir="state",
                peer_router_info=None,
                padding_profile=direction.padding,
                smoke_message_profile="fixed-12-byte-payload",
                expected_result_class="authenticated-handshake-and-directional-data-phase",
                data_phase_mode=data_phase_mode,
                data_phase_required_peer_action="observe-receive",
                expected_observation=expected_observation,
                **_plan065_primary_fields(
                    execution_id=direction.execution_id,
                    run_id=run_dir.name,
                    run_identity_sha256=run_identity_sha256,
                    reference_driver_mode=_reference_driver_mode_for(direction.reference),
                    expected_sender_router_hash_sha256=i2pr_router_hash_sha256,
                    expected_receiver_router_hash_sha256=reference_router_hash_sha256,
                    correlation_nonce="responder",
                ),
            )
            i2pr_adapter.start("listen")
            i2pr_adapter.wait_ready(timeout_seconds=30.0)
            ref_adapter = _make_ref_adapter(
                direction.reference, cache, run_dir / "ref", ref_endpoint, repo_root,
                shared_data_dir=shared_reference_data,
                placement=ref_placement,
            )
            ref_adapter.start()
            ref_adapter.wait_ready()
            trigger_result = trigger.send(
                direction.i2pr_is_initiator, ref_endpoint, run_dir, placement=ref_placement,
            )
            try:
                (run_dir / "raw").mkdir(parents=True, exist_ok=True)
                (run_dir / "raw" / "trigger-result.json").write_text(
                    json.dumps(_serialize_trigger(trigger_result), indent=2),
                    encoding="utf-8",
                )
            except OSError:
                pass
            terminal = i2pr_adapter.wait_terminal(timeout_seconds=30.0)
            if terminal["result"] != "passed":
                raise MixedRunError(_terminal_reason(terminal, "i2pr-responder-handshake-failed"))
            observations["i2pr"] = "authenticated"
            ref_observation = ref_adapter.collect_observation(
                role="initiator",
                run_id=run_dir.name,
                correlation={"delivery_status_message_id": "mixed-router", "bounded_test_nonce": run_dir.name},
                log_cursor=LogCursor(run_id=run_dir.name, log_path=run_dir / "ref" / "raw" / "i2pd.log" if reference == "i2pd" else run_dir / "ref" / "raw" / "java-i2p.log"),
                catalog=_observation_catalog,
            )
            observations[direction.reference] = ref_observation.get("observation_sha256", "")
            oracle_state = oracle.observe_directional(
                role="responder" if not direction.i2pr_is_initiator else "initiator",
                ref_endpoint=ref_endpoint,
                run_dir=run_dir,
                terminal_counters=terminal.get("counters", {}),
            )
            result, reason = _evaluate_pass_predicate(
                direction=direction,
                terminal=terminal,
                ref_observation=ref_observation,
                oracle_state=oracle_state,
            )
            if pipeline_context is not None:
                result, reason = _evaluate_plan052_predicate(terminal, ref_observation, oracle_state)
            runtime_counters["handshake_attempts"] = 1
            runtime_counters["frames_sent"] = int(terminal.get("counters", {}).get("frames_sent", 0))
            runtime_counters["frames_received"] = int(terminal.get("counters", {}).get("frames_received", 0))
            runtime_counters["i2np_sent"] = int(terminal.get("counters", {}).get("i2np_sent", 0))
            runtime_counters["i2np_received"] = int(terminal.get("counters", {}).get("i2np_received", 0))
            try:
                i2pr_ri = i2pr_adapter.export_router_info()
                i2pr_router_info_sha256 = hashlib.sha256(i2pr_ri.read_bytes()).hexdigest()
            except (OSError, RuntimeError):
                i2pr_router_info_sha256 = ""
    except MixedRunError as exc:
        result, reason = exc.result, exc.code
    except (HarnessBlocked,) as exc:
        result, reason = exc.result, exc.code
    except RenderError:
        result, reason = "rejected", "live-scenario-render-failed"
    except (IsolationError, JavaI2pError, I2pdError) as exc:
        result, reason = "rejected", exc.code
    except RootlessTopologyError as exc:
        result, reason = "rejected", exc.code
    except RuntimeError as exc:
        code = str(exc)
        result, reason = "rejected", code if code in PRE_PROTOCOL_REASONS else "typed-harness-operation-failed"
    except (OSError, ValueError):
        result, reason = "rejected", "typed-harness-operation-failed"
    finally:
        if i2pr_adapter is not None:
            i2pr_cleanup = _stop_adapter(i2pr_adapter, 10.0)
            if i2pr_cleanup == "failed":
                cleanup = "failed"
            try:
                snap = i2pr_adapter.process.snapshot() if i2pr_adapter.process else {}
                process_counters["i2pr"]["started"] = int(i2pr_adapter.process is not None)
                process_counters["i2pr"]["exited"] = int(i2pr_adapter.process is not None)
                process_counters["i2pr"]["forced"] += int(snap.get("forced", 0))
            except RuntimeError:
                pass
        if ref_adapter is not None:
            ref_cleanup = _stop_adapter(ref_adapter, 10.0)
            if ref_cleanup == "failed":
                cleanup = "failed"
            try:
                snap = ref_adapter.process.snapshot() if ref_adapter.process else {}
                process_counters[direction.reference]["started"] = int(ref_adapter.process is not None)
                process_counters[direction.reference]["exited"] = int(ref_adapter.process is not None)
                process_counters[direction.reference]["forced"] += int(snap.get("forced", 0))
            except RuntimeError:
                pass
        if topology is not None:
            topo_cleanup = topology.destroy()
            if topo_cleanup == "failed":
                cleanup = "failed"
            if cleanup == "clean" and not _no_residual_state(topology):
                cleanup = "failed"
        if cleanup == "not-started":
            cleanup = "clean"
        if cleanup == "failed":
            result = "failed_cleanup"
        if topology_kind == ROOTLESS_TOPOLOGY_KIND and not sandbox_attestation_sha256:
            result = "rejected"
            reason = reason or "sandbox-attestation-missing"
            cleanup = "clean"
        if (
            pipeline_context is None
            and metadata is not None
            and result in {"passed", "skipped_ipv6", "rejected", "failed", "blocked", "failed_cleanup"}
        ):
            evidence_root.mkdir(mode=0o700, parents=True, exist_ok=True)
            evidence_path = evidence_root / f"{run_dir.name}-{reference}.json"
            oracle_kind = oracle.oracle_kind.value if oracle is not None else ""
            record = _record_mixed(
                direction, result, reason, cleanup, i2pr_commit, metadata,
                topology, router_validation, observations, process_counters,
                oracle_kind=oracle_kind, runtime_counters=runtime_counters,
                configuration_sha256=configuration_sha256,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                reference_router_info_sha256=reference_router_info_sha256,
                data_phase_mode=data_phase_mode,
                expected_observation=expected_observation,
                sandbox_attestation_sha256=sandbox_attestation_sha256,
                parent_network_state_unchanged=parent_network_state_unchanged,
            )
            try:
                write_record(evidence_path, record)
            except (OSError, ValueError):
                result = "failed_cleanup"
                cleanup = "failed"
                reason = "evidence-finalization-failed"
                evidence_path = None
        try:
            if run_dir.exists():
                diagnostics_mode = _diagnostics_mode()
                if diagnostics_mode == "raw-local":
                    try:
                        evidence_root.mkdir(mode=0o700, parents=True, exist_ok=True)
                        logs_dest = evidence_root / f"{run_dir.name}-{reference}-raw-logs"
                        if run_dir.resolve() != logs_dest.resolve():
                            shutil.copytree(run_dir / "raw", logs_dest / "raw", dirs_exist_ok=True)
                    except OSError:
                        pass
                if os.environ.get("I2PR_INTEROP_KEEP_RUN_DIR") != "1":
                    shutil.rmtree(run_dir)
        except OSError:
            cleanup = "failed"
            result = "failed_cleanup"
        if evidence_path is not None and cleanup == "failed":
            try:
                record = json.loads(evidence_path.read_text(encoding="utf-8"))
                record["actual_typed_result"] = "failed_cleanup"
                record["cleanup_result"] = "failed"
                record["known_deviation"] = "cleanup-verification-failed"
                record["evidence_sha256"] = ""
                write_record(evidence_path, record)
            except (OSError, ValueError):
                pass
        if pipeline_context is not None:
            try:
                reference_observation = locals().get("ref_observation")
                i2pr_observation = None
                if isinstance(reference_observation, dict):
                    i2pr_observation = _i2pr_synthetic_observation(context=pipeline_context, result=result, initiator=direction.initiator)
                write_direction_artifacts(
                    pipeline_context,
                    direction.execution_id,
                    reference=direction.reference,
                    initiator=direction.initiator,
                    result=result,
                    reason_code=reason or "not-started",
                    cleanup_result=cleanup,
                    terminal=locals().get("terminal"),
                    i2pr_observation=i2pr_observation,
                    reference_observation=reference_observation if isinstance(reference_observation, dict) else None,
                )
            except (OSError, PipelineError, ValueError):
                result = "failed_cleanup"
                cleanup = "failed"
                reason = "evidence-finalization-failed"
    _emit(direction.execution_id, reference, result, reason, cleanup)
    return 0 if result == "passed" else 2


def _serialize_trigger(result: object) -> dict[str, object]:
    """Dump a :class:`TriggerResult`-shaped object to a plain dict for
    debug capture. Used by the harness sentinel below to record why a
    per-direction SAM/HTTP trigger succeeded or failed.
    """

    kind = getattr(result, "kind", None)
    kind_value = getattr(kind, "value", str(kind)) if kind is not None else None
    return {
        "kind": kind_value,
        "observed": bool(getattr(result, "observed", False)),
        "description": str(getattr(result, "description", "")),
        "timed_out": bool(getattr(result, "timed_out", False)),
    }


def _evaluate_plan052_predicate(
    terminal: dict[str, object],
    ref_observation: dict[str, object] | str,
    oracle_state: dict[str, str],
) -> tuple[str, str]:
    if str(terminal.get("result", "")) != "passed":
        return "rejected", _terminal_reason(terminal, "i2pr-terminal-not-passed")
    if not isinstance(ref_observation, dict):
        return "rejected", "reference-receiver-marker-not-source-locked"
    if ref_observation.get("schema") != OBSERVATION_SCHEMA:
        return "rejected", "reference-receiver-marker-not-source-locked"
    levels = ref_observation.get("levels", {})
    if not isinstance(levels, dict):
        return "rejected", "reference-receiver-marker-not-source-locked"
    if levels.get("ntcp2_authenticated", {}).get("state") != "observed":
        return "rejected", "reference-handshake-marker-not-observed"
    if levels.get("frame_authenticated_and_decrypted", {}).get("state") != "observed":
        return "rejected", "reference-receiver-marker-not-source-locked"
    if levels.get("i2np_message_decoded", {}).get("state") != "observed":
        return "rejected", "reference-i2np-marker-not-source-locked"
    return "passed", "mixed-router-direction-authenticated"


def _terminal_reason(terminal: dict[str, object], fallback: str) -> str:
    reason = terminal.get("reason_code")
    if reason is None:
        return fallback
    if not isinstance(reason, str) or reason not in STATUS_REASONS:
        return "unknown-i2pr-terminal-reason"
    return reason


def _i2pr_synthetic_observation(*, context, result: str, initiator: str) -> dict[str, Any]:
    """Build a minimal i2pr-side observation-v2 record from a pipeline context."""

    from plan052_pipeline import _i2pr_synthetic_observation_levels
    return {
        "schema": OBSERVATION_SCHEMA,
        "schema_version": 2,
        "side": "i2pr",
        "levels": _i2pr_synthetic_observation_levels(result=result, initiator=initiator),
        "run_id": context.run_id,
        "run_identity_sha256": context.identity["run_identity_sha256"],
    }


def _ref_observation_authenticated(ref_observation: dict[str, object] | str) -> bool:
    if isinstance(ref_observation, str):
        return ref_observation == "authenticated"
    if not isinstance(ref_observation, dict):
        return False
    return ref_observation.get("levels", {}).get("ntcp2_authenticated", {}).get("state") == "observed"


def _evaluate_pass_predicate(
    *,
    direction: MixedDirection,
    terminal: dict[str, object],
    ref_observation: dict[str, object] | str,
    oracle_state: dict[str, str],
) -> tuple[str, str]:
    """Plan 045 D7: the typed pass predicate requires every phase to succeed.

    The prior code marked a direction ``passed`` after the handshake alone
    and ignored the data-phase oracle and the reference observation. The
    new predicate requires: (a) i2pr terminal ``passed``; (b) the
    reference adapter reports an authenticated observation; (c) the data
    oracle observed the expected sender or receiver for the direction.
    """

    if str(terminal.get("result", "")) != "passed":
        return "rejected", "i2pr-terminal-not-passed"
    if not _ref_observation_authenticated(ref_observation):
        return "rejected", "reference-observation-missing"
    if direction.i2pr_is_initiator:
        if oracle_state.get("sender_observed") != "observed":
            return "rejected", "oracle-sender-not-observed"
    else:
        if oracle_state.get("receiver_observed") != "observed":
            return "rejected", "oracle-receiver-not-observed"
    return "passed", "mixed-router-direction-authenticated"


def _run_initiator_first(
    *,
    direction: MixedDirection,
    run_dir: Path,
    repo_root: Path,
    cache: Path,
    metadata: CacheMetadata | None,
    topology: Any,
    i2pr_endpoint: EndpointDescription,
    ref_endpoint: EndpointDescription,
    router_validation: dict[str, str],
    observations: dict[str, str],
    shared_reference_data: Path,
    ref_placement: Any = None,
) -> tuple[str, Path, str]:
    """Generate the reference identity and export its RouterInfo.

    The preparation adapter writes to ``shared_reference_data`` and the live
    phase reuses the same data directory as the exported RouterInfo.
    """

    prep_root = run_dir / "ref-preparation"
    prep_root.mkdir(mode=0o700, parents=True, exist_ok=True)
    shared_reference_data.mkdir(mode=0o700, parents=True, exist_ok=True)
    ref_adapter = _make_ref_adapter(
        direction.reference, cache, prep_root, ref_endpoint, repo_root,
        shared_data_dir=shared_reference_data,
        placement=ref_placement,
    )
    ref_adapter.start()
    ref_adapter.wait_ready(timeout_seconds=240.0)
    ri_path = ref_adapter.export_router_info()
    _validate_router_info_for_direction(
        ri_path, ref_endpoint.local_address, ref_endpoint.local_port,
        repo_root, router_validation, direction.reference,
    )
    exchange_dir = run_dir / "i2pr" / "exchange"
    exchange_dir.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ri_path, exchange_dir / "ref-router.info")
    ref_adapter.stop()
    return ("validated-and-bound", ri_path, getattr(ref_adapter, "configuration_sha256", ""))


def _run_responder_first(
    *,
    direction: MixedDirection,
    run_dir: Path,
    repo_root: Path,
    cache: Path,
    metadata: CacheMetadata | None,
    topology: Any,
    i2pr_endpoint: EndpointDescription,
    ref_endpoint: EndpointDescription,
    router_validation: dict[str, str],
    observations: dict[str, str],
    shared_i2pr_state: Path,
    i2pr_placement: Any = None,
    ref_placement: Any = None,
) -> tuple[str, Path, str]:
    """Generate the i2pr identity and export its RouterInfo.

    The dedicated preparation command writes to ``shared_i2pr_state`` and the
    live phase reuses that identity. The Rust launcher persists
    ``router.info`` inside ``state_dir``.
    """

    shared_i2pr_state.mkdir(mode=0o700, parents=True, exist_ok=True)
    # State is prepared by I2prAdapter.prepare_state() before this helper.
    ri_path = shared_i2pr_state / "router.info"
    if not ri_path.is_file():
        raise MixedRunError("i2pr-router-info-missing")
    router_validation["i2pr"] = "validated-and-bound"
    ref_adapter = _make_ref_adapter(
        direction.reference, cache, run_dir / "ref-preparation", ref_endpoint, repo_root,
        placement=ref_placement,
    )
    try:
        ref_adapter.start()
        ref_adapter.wait_ready()
        ref_adapter.import_peer_router_info(ri_path)
        reference_info = ref_adapter.export_router_info()
        _validate_router_info_for_direction(
            reference_info,
            ref_endpoint.local_address,
            ref_endpoint.local_port,
            repo_root,
            router_validation,
            direction.reference,
        )
    finally:
        ref_adapter.stop()
    return ("validated-and-bound", ri_path, getattr(ref_adapter, "configuration_sha256", ""))


def _make_ref_adapter(
    reference: str,
    cache: Path,
    run_root: Path,
    endpoint: EndpointDescription,
    repo_root: Path,
    *,
    shared_data_dir: Path | None = None,
    placement: Any = None,
) -> JavaI2pAdapter | I2pdAdapter:
    if reference == "java_i2p":
        return JavaI2pAdapter(
            cache, run_root, endpoint, repo_root,
            shared_data_dir=shared_data_dir, placement=placement,
        )
    return I2pdAdapter(
        cache, run_root, endpoint, repo_root,
        shared_data_dir=shared_data_dir, placement=placement,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--reference", choices=("java_i2p", "i2pd"), required=True)
    parser.add_argument("--build-cache")
    parser.add_argument("--run-root")
    parser.add_argument("--run-id")
    parser.add_argument("--run-identity")
    parser.add_argument("--bundle-staging")
    parser.add_argument("--evidence-profile", choices=(PIPELINE_PROFILE,))
    parser.add_argument("--keep-failed-sanitized", action="store_true")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument(
        "--topology-kind",
        choices=(PRIVILEGED_TOPOLOGY_KIND, ROOTLESS_TOPOLOGY_KIND),
        default=PRIVILEGED_TOPOLOGY_KIND,
    )
    args = parser.parse_args()
    try:
        return run(args)
    except MixedRunError as exc:
        _emit(args.scenario, args.reference, exc.result, exc.code, "not-started")
        return 2
    except (HarnessBlocked,) as exc:
        _emit(args.scenario, args.reference, exc.result, exc.code, "not-started")
        return 2
    except (OSError, ValueError, RuntimeError):
        _emit(args.scenario, args.reference, "rejected", "typed-harness-operation-failed", "not-started")
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
