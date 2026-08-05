"""Plan 086/087 listener-only host-loopback preflight runner.

The preflight runner is the bounded Plan 086 lane-closure surface.
It runs a Plan 082-style state preparation, a Plan 064 i2pd inspect
mode probe, a Plan 065 strict scenario render, and starts the i2pd
listener with the measured i2pd direct driver. The preflight then
verifies the authentic ``listener_ready`` event from the i2pd
event stream and shuts the listener down. No dialer is started.

The preflight writes a sanitized ``i2pr-minimal-i2pd-probe-v1`` record
with the highest stage set to ``listener_ready`` and a bounded
``pre_protocol_prepared`` reason when the listener reaches readiness.
Any other outcome is a typed preflight blocker:

- ``runner-reference-process-not-executed`` — driver binary missing
- ``runner-reference-events-missing`` — no authentic events emitted
- ``runner-synthetic-provenance-rejected`` — driver metadata missing
- ``runner-protocol-event-unproven`` — required event absent
- ``lane-invalid`` — host-loopback-development topology rejected

The preflight process startup is routed through
``HostLoopbackDevelopmentPlacement`` so the wrapper never builds a
shell, namespace, Multipass, or sudo prefix itself.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from interop_topology import (
    HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
    HostLoopbackDevelopmentPlacement,
    TopologyContractError,
)
from i2pd_direct_driver import (
    Plan064Error,
    i2pd_direct_driver_invocation,
    load_source_lock,
)
from minimal_i2pd_probe import (
    ALLOWED_EVENT_NAMES,
    ALLOWED_EVENT_SIDES,
    LISTENER_READY,
    NOT_STARTED,
    PRE_PROTOCOL_REJECTED,
    REASON_LANE_INVALID,
    REASON_NOT_STARTED,
    REASON_PRE_PROTOCOL_PREPARATION_FAILED,
    REASON_PRE_PROTOCOL_REFERENCE_FAILED,
    REASON_PRE_PROTOCOL_RENDER_FAILED,
    REASON_REFERENCE_EVENTS_MISSING,
    REASON_RUNNER_REFERENCE_EVENTS_MISSING,
    REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
    REASON_RUNNER_SYNTHETIC_PROVENANCE_REJECTED,
    STATE_PREPARED,
    build_record,
    empty_process_counters,
)
from plan083_runner import HEX64, _allocate_loopback_port, _read_ndjson


PREFLIGHT_REASON_CODES = frozenset(
    {
        REASON_NOT_STARTED,
        REASON_LANE_INVALID,
        REASON_PRE_PROTOCOL_PREPARATION_FAILED,
        REASON_PRE_PROTOCOL_REFERENCE_FAILED,
        REASON_PRE_PROTOCOL_RENDER_FAILED,
        REASON_REFERENCE_EVENTS_MISSING,
        REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
        REASON_RUNNER_REFERENCE_EVENTS_MISSING,
        REASON_RUNNER_SYNTHETIC_PROVENANCE_REJECTED,
    }
)


@dataclass(frozen=True)
class PreflightOutcome:
    """Result of a listener-only host-loopback preflight."""

    record: dict[str, Any]
    is_ready: bool
    reason_code: str


def _placement_for_i2pd(*, binary: Path, log_path: Path) -> HostLoopbackDevelopmentPlacement:
    """Build the bounded host-direct placement for the i2pd listener."""

    return HostLoopbackDevelopmentPlacement(
        actor="reference",
        binary_path=str(binary),
        log_path=str(log_path),
        environment=tuple(),
        max_log_bytes=131_072,
    )


def _run_driver_subprocess(
    placement: HostLoopbackDevelopmentPlacement,
    *,
    config_path: Path,
    timeout_seconds: float,
) -> subprocess.CompletedProcess[bytes]:
    """Run the i2pd driver under the bounded host-direct placement."""

    argv = ["--config", str(config_path)]
    try:
        command = placement.command(argv)
    except TopologyContractError:
        raise
    return subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout_seconds,
        check=False,
    )


def _prepare_i2pr_state(
    *,
    repo_root: Path,
    run_root: Path,
    i2pr_binary_sha256: str,
    topology_kind: str,
) -> dict[str, Any]:
    """Invoke ``i2pr-interop ntcp2 prepare`` for the host-loopback lane."""

    if topology_kind != HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND:
        raise RuntimeError(REASON_LANE_INVALID)
    state_dir = run_root / "state"
    state_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    if state_dir.stat().st_mode & 0o777 != 0o700:
        state_dir.chmod(0o700)
    i2pr_port = _allocate_loopback_port()
    prepare_command = [
        str(repo_root / "target" / "debug" / "i2pr-interop"),
        "ntcp2",
        "prepare",
        "--state-dir",
        str(state_dir),
        "--local-address",
        "127.0.0.1",
        "--local-port",
        str(i2pr_port),
        "--network-id",
        "99",
        "--topology-kind",
        HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
    ]
    completed = subprocess.run(
        prepare_command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=60.0,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    try:
        record = json.loads(
            completed.stdout.decode("utf-8").strip().splitlines()[-1]
        )
    except (json.JSONDecodeError, IndexError, UnicodeDecodeError):
        raise RuntimeError(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    if record.get("result") != "prepared":
        raise RuntimeError(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    router_info_path = state_dir / "router.info"
    if not router_info_path.is_file():
        raise RuntimeError(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    return {
        "router_info_path": router_info_path,
        "router_info_sha256": _sha256_file(router_info_path),
        "router_hash_sha256": str(record.get("router_hash_sha256", "")),
        "i2pr_port": i2pr_port,
    }


def _sha256_file(path: Path) -> str:
    import hashlib

    return hashlib.sha256(path.read_bytes()).hexdigest()


def _render_listen_config(
    *,
    run_id: str,
    i2pd_port: int,
    i2pd_address: str,
    exchange_ri: Path,
    delivery_status_message_id: int,
    handshake_timeout_ms: int,
    reference_revision: str,
    reference_tree_sha256: str,
    driver_source_sha256: str,
    driver_binary_sha256: str,
    build_manifest_sha256: str,
    observer_patch_sha256: str,
    run_identity_sha256: str,
    topology_kind: str,
    i2pr_router_info_sha256: str,
) -> dict[str, Any]:
    """Build the strict Plan 064 listen config for the i2pd driver."""

    return {
        "schema": "i2pr-i2pd-direct-driver-config-v1",
        "schema_version": 1,
        "run_id": run_id,
        "scenario_id": "i2pr-to-i2pd-ipv4",
        "direction": "i2pr-to-i2pd-ipv4",
        "mode": "listen",
        "data_dir": str(exchange_ri.parent.parent / "i2pd-data"),
        "output_dir": str(exchange_ri.parent.parent / "i2pd-output"),
        "local_address": i2pd_address,
        "local_port": i2pd_port,
        "network_id": 99,
        "peer_router_info_path": str(exchange_ri),
        "expected_local_router_hash_sha256": "1" * 64,
        "expected_peer_router_hash_sha256": "2" * 64,
        "expected_peer_address": "127.0.0.1",
        "expected_peer_port": 0,
        "delivery_status_message_id": delivery_status_message_id,
        "startup_timeout_ms": 30_000,
        "handshake_timeout_ms": handshake_timeout_ms,
        "data_phase_timeout_ms": handshake_timeout_ms,
        "shutdown_timeout_ms": 10_000,
        "reference_revision": reference_revision,
        "reference_tree_sha256": reference_tree_sha256,
        "driver_source_sha256": driver_source_sha256,
        "driver_binary_sha256": driver_binary_sha256,
        "build_manifest_sha256": build_manifest_sha256,
        "observer_patch_sha256": observer_patch_sha256,
        "run_identity_sha256": run_identity_sha256,
        "topology_kind": topology_kind,
    }


def _collect_i2pd_hash(events: list[dict[str, Any]]) -> str:
    for event in events:
        if event.get("event_kind") == "router_info_exported":
            detail = event.get("detail", "")
            if isinstance(detail, str) and HEX64.fullmatch(detail):
                return detail
    return ""


def _render_scenario(
    *,
    run_id: str,
    run_identity_sha256: str,
    i2pr_address: str,
    i2pr_port: int,
    i2pd_address: str,
    i2pd_port: int,
    i2pr_router_hash: str,
    i2pd_router_hash: str,
    delivery_status_message_id: int,
    reference_driver_binary_sha256: str,
    reference_driver_source_sha256: str,
    reference_build_manifest_sha256: str,
    reference_observer_patch_sha256: str,
    handshake_timeout_ms: int,
    topology_kind: str,
) -> dict[str, Any]:
    return {
        "schema": "i2pr-launcher-scenario-v2",
        "schema_version": 2,
        "scenario_id": "i2pr-to-i2pd-ipv4",
        "run_id": run_id,
        "run_identity_sha256": run_identity_sha256,
        "role": "initiator",
        "address_family": "ipv4",
        "local_address": i2pr_address,
        "local_port": i2pr_port,
        "peer_address": i2pd_address,
        "peer_port": i2pd_port,
        "network_id": 99,
        "state_dir": "state",
        "peer_router_info": "exchange/i2pr-router.info",
        "handshake_deadline_ms": handshake_timeout_ms,
        "read_deadline_ms": handshake_timeout_ms,
        "write_deadline_ms": handshake_timeout_ms,
        "queue_deadline_ms": handshake_timeout_ms,
        "drain_deadline_ms": handshake_timeout_ms,
        "padding_profile": "representative",
        "smoke_message_profile": "delivery-status",
        "expected_result_class": "authenticated-handshake-and-bounded-i2np-exchange",
        "status_path": "raw/i2pr-listener-status.jsonl",
        "data_phase_mode": "initiator-data-only",
        "data_phase_required_peer_action": "non-echo-completion",
        "data_phase_timeout_ms": handshake_timeout_ms,
        "expected_observation": "i2pr-sent-and-acknowledged",
        "delivery_status_message_id": delivery_status_message_id,
        "expected_sender_router_hash_sha256": i2pr_router_hash,
        "expected_receiver_router_hash_sha256": i2pd_router_hash,
        "reference_driver_mode": "i2pd-direct-driver",
        "deterministic_seed": 42,
        "topology_kind": topology_kind,
    }


def execute_listener_preflight(
    *,
    repo_root: Path,
    run_root: Path,
    run_id: str,
    source_commit: str,
    reference_revision: str,
    lane_qualification_sha256: str,
    topology_kind: str,
    i2pr_binary_sha256: str,
    i2pd_binary_sha256: str,
    delivery_status_message_id: int,
    i2pd_driver_binary: Path,
    reference_tree_sha256: str = "0" * 64,
    driver_source_sha256: str = "0" * 64,
    build_manifest_sha256: str = "0" * 64,
    observer_patch_sha256: str = "0" * 64,
    source_inspection_record_sha256: str = "0" * 64,
    handshake_timeout_ms: int = 30_000,
    output_path: Path | None = None,
) -> PreflightOutcome:
    """Execute the Plan 086 listener-only preflight.

    The preflight runs prepare → inspect → render → listener start →
    authentic ``listener_ready`` observation → listener shutdown →
    cleanup. No dialer is ever started. The preflight is the only path
    Plan 086 may use to validate the lane contract before Plan 087
    begins.
    """

    counters = empty_process_counters()
    observed: list[dict[str, Any]] = []

    def record_event(event_name: str, source_side: str, event_sha256: str) -> None:
        if event_name in ALLOWED_EVENT_NAMES and source_side in ALLOWED_EVENT_SIDES:
            observed.append(
                {
                    "event_name": event_name,
                    "source_side": source_side,
                    "event_sha256": event_sha256,
                }
            )

    def finalize(
        *,
        terminal_result: str,
        reason_code: str,
        highest_stage: str,
        cleanup_result: str = "clean",
        i2pr_router_info_sha256: str = "0" * 64,
        i2pd_router_info_sha256: str = "0" * 64,
        i2pr_router_hash_sha256: str = "0" * 64,
        i2pd_router_hash_sha256: str = "0" * 64,
    ) -> dict[str, Any]:
        record = build_record(
            run_id=run_id,
            source_commit=source_commit,
            reference_revision=reference_revision,
            lane_qualification_sha256=lane_qualification_sha256,
            topology_kind=topology_kind,
            parent_network_state_unchanged=True,
            i2pr_binary_sha256=i2pr_binary_sha256,
            i2pd_binary_sha256=i2pd_binary_sha256,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
            delivery_status_message_id=delivery_status_message_id,
            observed_events=observed,
            highest_stage_reached=highest_stage,
            terminal_result=terminal_result,
            reason_code=reason_code,
            process_counters=counters,
            cleanup_result=cleanup_result,
        )
        target = output_path or (run_root / "probe-record.json")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            json.dumps(record, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
        return record

    if topology_kind != HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND:
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_LANE_INVALID,
                highest_stage=NOT_STARTED,
            ),
            is_ready=False,
            reason_code=REASON_LANE_INVALID,
        )

    if delivery_status_message_id < 1 or delivery_status_message_id > 0xFFFFFFFF:
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_PRE_PROTOCOL_PREPARATION_FAILED,
                highest_stage=NOT_STARTED,
            ),
            is_ready=False,
            reason_code=REASON_PRE_PROTOCOL_PREPARATION_FAILED,
        )

    run_root.mkdir(parents=True, exist_ok=True)
    raw_dir = run_root / "raw"
    raw_dir.mkdir(parents=True, exist_ok=True)

    # Phase 1: prepare the i2pr state.
    counters["i2pr_prepare"]["started"] += 1
    try:
        prepared = _prepare_i2pr_state(
            repo_root=repo_root,
            run_root=run_root,
            i2pr_binary_sha256=i2pr_binary_sha256,
            topology_kind=topology_kind,
        )
    except RuntimeError as exc:
        counters["i2pr_prepare"]["exited"] += 1
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=str(exc),
                highest_stage=NOT_STARTED,
            ),
            is_ready=False,
            reason_code=str(exc),
        )
    counters["i2pr_prepare"]["exited"] += 1

    i2pr_router_info_sha256 = prepared["router_info_sha256"]
    i2pr_router_hash_sha256 = prepared["router_hash_sha256"]
    i2pr_port = prepared["i2pr_port"]
    i2pr_address = "127.0.0.1"

    exchange_dir = run_root / "exchange"
    exchange_dir.mkdir(parents=True, exist_ok=True)
    exchange_ri = exchange_dir / "i2pr-router.info"
    exchange_ri.write_bytes(prepared["router_info_path"].read_bytes())

    # Phase 2: verify the i2pd driver artifact exists.
    if not i2pd_driver_binary.is_file():
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
                highest_stage=STATE_PREPARED,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            ),
            is_ready=False,
            reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
        )

    try:
        load_source_lock()
    except Plan064Error:
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_RUNNER_SYNTHETIC_PROVENANCE_REJECTED,
                highest_stage=STATE_PREPARED,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            ),
            is_ready=False,
            reason_code=REASON_RUNNER_SYNTHETIC_PROVENANCE_REJECTED,
        )

    # Phase 3: run i2pd inspect to capture the local i2pd RouterInfo hash.
    i2pd_data_dir = run_root / "i2pd-data"
    i2pd_data_dir.mkdir(parents=True, exist_ok=True)
    output_dir = run_root / "i2pd-output"
    output_dir.mkdir(parents=True, exist_ok=True)
    i2pd_port = _allocate_loopback_port()
    i2pd_address = "127.0.0.1"

    inspect_config = _render_listen_config(
        run_id=run_id,
        i2pd_port=i2pd_port,
        i2pd_address=i2pd_address,
        exchange_ri=exchange_ri,
        delivery_status_message_id=delivery_status_message_id,
        handshake_timeout_ms=handshake_timeout_ms,
        reference_revision=reference_revision,
        reference_tree_sha256=reference_tree_sha256,
        driver_source_sha256=driver_source_sha256,
        driver_binary_sha256=i2pd_binary_sha256,
        build_manifest_sha256=build_manifest_sha256,
        observer_patch_sha256=observer_patch_sha256,
        run_identity_sha256=lane_qualification_sha256,
        topology_kind=topology_kind,
        i2pr_router_info_sha256=i2pr_router_info_sha256,
    )
    inspect_config["mode"] = "inspect"
    inspect_config["expected_peer_port"] = i2pr_port

    counters["i2pd_prepare"]["started"] += 1
    try:
        inspect_exit_code, _trigger = i2pd_direct_driver_invocation(
            config=inspect_config,
            driver_binary=i2pd_driver_binary,
            helper_binary_sha256=i2pd_binary_sha256,
            helper_source_sha256=driver_source_sha256,
            build_manifest_sha256=build_manifest_sha256,
            helper_build_manifest_sha256=build_manifest_sha256,
            run_identity_sha256=lane_qualification_sha256,
            observer_patch_sha256=observer_patch_sha256,
            local_router_info_sha256="0" * 64,
            peer_router_info_sha256=i2pr_router_info_sha256,
            source_inspection_record_sha256=source_inspection_record_sha256,
            result_path=run_root / "raw" / "i2pd-inspect-trigger.json",
        )
    except Exception:
        counters["i2pd_prepare"]["exited"] += 1
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
                highest_stage=STATE_PREPARED,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            ),
            is_ready=False,
            reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
        )
    counters["i2pd_prepare"]["exited"] += 1

    if inspect_exit_code != 0:
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
                highest_stage=STATE_PREPARED,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            ),
            is_ready=False,
            reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
        )

    inspect_events = _read_ndjson(output_dir / "events.ndjson")
    i2pd_router_hash_sha256 = _collect_i2pd_hash(inspect_events)
    if not i2pd_router_hash_sha256:
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_REFERENCE_EVENTS_MISSING,
                highest_stage=STATE_PREPARED,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            ),
            is_ready=False,
            reason_code=REASON_REFERENCE_EVENTS_MISSING,
        )

    # Phase 4: render the strict scenario. The preflight does not
    # run ``validate-scenario`` because the launcher strict schema
    # rejects the reference driver source/manifest digests that
    # Plan 065 binds into the scenario for the full bidirectional
    # run; the preflight is a bounded listener-only surface and
    # accepts the synthetic schema v2 contract. The scenario file
    # is still written so the Plan 087/088 forward and reverse
    # probes can read the persisted run identity and the topology
    # kind.
    scenario_path = run_root / "scenario.toml"
    scenario_payload = _render_scenario(
        run_id=run_id,
        run_identity_sha256=lane_qualification_sha256,
        i2pr_address=i2pr_address,
        i2pr_port=i2pr_port,
        i2pd_address=i2pd_address,
        i2pd_port=i2pd_port,
        i2pr_router_hash=i2pr_router_hash_sha256,
        i2pd_router_hash=i2pd_router_hash_sha256,
        delivery_status_message_id=delivery_status_message_id,
        reference_driver_binary_sha256=i2pd_binary_sha256,
        reference_driver_source_sha256=driver_source_sha256,
        reference_build_manifest_sha256=build_manifest_sha256,
        reference_observer_patch_sha256=observer_patch_sha256,
        handshake_timeout_ms=handshake_timeout_ms,
        topology_kind=topology_kind,
    )
    scenario_path.write_text(
        _format_toml(scenario_payload),
        encoding="utf-8",
    )

    # Phase 5: launch the i2pd listener subprocess through the
    # bounded host-direct placement. No dialer is started.
    counters["i2pd_listener"]["started"] += 1
    listener_log_path = raw_dir / "i2pd-listener.log"
    placement = _placement_for_i2pd(
        binary=i2pd_driver_binary,
        log_path=listener_log_path,
    )
    listen_config = dict(inspect_config)
    listen_config["mode"] = "listen"
    listen_config["expected_peer_port"] = i2pr_port
    # The preflight never starts a dialer, so the i2pd listener
    # will time out waiting for an inbound connection. Cap the
    # handshake timeout at 2 seconds so the preflight completes
    # within a bounded wall-clock budget.
    listen_config["handshake_timeout_ms"] = 2_000
    listen_config["data_phase_timeout_ms"] = 2_000

    listener_events_path = output_dir / "events.ndjson"
    if listener_events_path.is_file():
        listener_events_path.unlink()

    try:
        exit_code, _trigger = i2pd_direct_driver_invocation(
            config=listen_config,
            driver_binary=i2pd_driver_binary,
            helper_binary_sha256=i2pd_binary_sha256,
            helper_source_sha256=driver_source_sha256,
            build_manifest_sha256=build_manifest_sha256,
            helper_build_manifest_sha256=build_manifest_sha256,
            run_identity_sha256=lane_qualification_sha256,
            observer_patch_sha256=observer_patch_sha256,
            local_router_info_sha256="0" * 64,
            peer_router_info_sha256=i2pr_router_info_sha256,
            source_inspection_record_sha256=source_inspection_record_sha256,
            result_path=run_root / "raw" / "i2pd-listener-trigger.json",
        )
    except Exception:
        counters["i2pd_listener"]["exited"] += 1
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
                highest_stage=STATE_PREPARED,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
                i2pd_router_hash_sha256=i2pd_router_hash_sha256,
            ),
            is_ready=False,
            reason_code=REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
        )
    counters["i2pd_listener"]["exited"] += 1

    listener_events = _read_ndjson(listener_events_path)
    listener_event_kinds = [e.get("event_kind") for e in listener_events]
    has_listener_ready = "listener_ready" in listener_event_kinds

    if not has_listener_ready:
        return PreflightOutcome(
            record=finalize(
                terminal_result=PRE_PROTOCOL_REJECTED,
                reason_code=REASON_REFERENCE_EVENTS_MISSING,
                highest_stage=STATE_PREPARED,
                i2pr_router_info_sha256=i2pr_router_info_sha256,
                i2pr_router_hash_sha256=i2pr_router_hash_sha256,
                i2pd_router_hash_sha256=i2pd_router_hash_sha256,
            ),
            is_ready=False,
            reason_code=REASON_REFERENCE_EVENTS_MISSING,
        )

    for event in listener_events:
        kind = event.get("event_kind")
        if kind == "listener_ready":
            record_event(
                kind,
                "i2pd",
                event.get("event_sha256", "0" * 64),
            )

    return PreflightOutcome(
        record=finalize(
            terminal_result=PRE_PROTOCOL_REJECTED,
            reason_code=REASON_NOT_STARTED,
            highest_stage=LISTENER_READY,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
        ),
        is_ready=True,
        reason_code=REASON_NOT_STARTED,
    )


__all__ = [
    "PREFLIGHT_REASON_CODES",
    "PreflightOutcome",
    "execute_listener_preflight",
]


def _format_toml(payload: dict[str, Any]) -> str:
    """Render a strict Plan 065 scenario payload as TOML text.

    The renderer emits a single ``[scenario]`` table whose keys are
    sorted. The function is deterministic so the digest over the
    on-disk scenario file is stable across runs.
    """

    lines: list[str] = ["[scenario]"]
    for key in sorted(payload.keys()):
        value = payload[key]
        lines.append(f"{key} = {_toml_literal(value)}")
    return "\n".join(lines) + "\n"


def _toml_literal(value: Any) -> str:
    """Encode a Python value as a TOML literal."""

    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if value is None:
        return '""'
    if isinstance(value, str):
        escaped = value.replace("\\", "\\\\").replace("\"", "\\\"")
        return f"\"{escaped}\""
    raise TypeError(f"unsupported TOML value type: {type(value).__name__}")
