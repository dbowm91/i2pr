#!/usr/bin/env python3
"""Run one bounded Plan 041 Java I2P/i2pd reference-pair crosscheck."""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import json
import os
import shutil
import sys
import time
import uuid
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from harness.evidence import write_record  # type: ignore
    from harness.i2pd import I2pdAdapter, I2pdError  # type: ignore
    from harness.java_i2p import JavaI2pAdapter, JavaI2pError  # type: ignore
    from harness.reference_scenario import ReferenceScenarioError, load_reference_scenario  # type: ignore
    from harness.reference_topology import ReferenceTopologyError, ReferencePairTopology  # type: ignore
    from harness.router_info import RouterInfoPathError, strict_validate_router_info  # type: ignore
    from harness.topology import EndpointDescription  # type: ignore
    from harness.runner import HarnessBlocked, _cache_for, _git_identity, _host_check  # type: ignore
else:
    from .evidence import write_record
    from .i2pd import I2pdAdapter, I2pdError
    from .java_i2p import JavaI2pAdapter, JavaI2pError
    from .reference_scenario import ReferenceScenarioError, load_reference_scenario
    from .reference_topology import ReferenceTopologyError, ReferencePairTopology
    from .router_info import RouterInfoPathError, strict_validate_router_info
    from .runner import HarnessBlocked, _cache_for, _git_identity, _host_check
    from .topology import EndpointDescription


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


def _run_id() -> str:
    return f"reference-{dt.datetime.now(dt.UTC).strftime('%Y%m%dT%H%M%SZ')}-{os.getpid()}-{uuid.uuid4().hex[:8]}"


def _acquire_run_lock(repo_root: Path):
    lock_path = repo_root / "target/interop/reference-crosscheck.lock"
    lock_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    handle = lock_path.open("a+", encoding="ascii")
    try:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as exc:
        handle.close()
        raise HarnessBlocked("reference-crosscheck-concurrent-run", "blocked_host_contract") from exc
    return handle


def _scenario_path(repo_root: Path, scenario_id: str) -> Path:
    path = repo_root / "tests/integration/ntcp2/reference-scenarios" / f"{scenario_id}.toml"
    if not path.is_file():
        raise HarnessBlocked("unknown-reference-scenario", "blocked_missing_driver")
    return path


def _adapter_pair(
    scenario: Any, topology: ReferencePairTopology, cache_java: Path, cache_i2pd: Path, run_dir: Path, repo_root: Path
) -> tuple[JavaI2pAdapter, I2pdAdapter]:
    java_endpoint = EndpointDescription(
        local_address=scenario.java.address,
        peer_address=scenario.i2pd.address,
        local_port=scenario.java.port,
        peer_port=scenario.i2pd.port,
        address_family="ipv4",
        namespace=topology.java_namespace,
        network_id=str(scenario.private_network_id),
    )
    i2pd_endpoint = EndpointDescription(
        local_address=scenario.i2pd.address,
        peer_address=scenario.java.address,
        local_port=scenario.i2pd.port,
        peer_port=scenario.java.port,
        address_family="ipv4",
        namespace=topology.i2pd_namespace,
        network_id=str(scenario.private_network_id),
    )
    return (
        JavaI2pAdapter(cache_java, run_dir / "java", java_endpoint, repo_root),
        I2pdAdapter(cache_i2pd, run_dir / "i2pd", i2pd_endpoint, repo_root),
    )


def _wait_authenticated(adapters: tuple[Any, Any], timeout_seconds: int) -> dict[str, str]:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        observations = {
            "java_i2p": adapters[0].authenticated_observation(),
            "i2pd": adapters[1].authenticated_observation(),
        }
        if all(value == "authenticated" for value in observations.values()):
            return observations
        if any(adapter.process is not None and adapter.process.snapshot()["running"] == 0 for adapter in adapters):
            break
        time.sleep(0.05)
    return {
        "java_i2p": adapters[0].authenticated_observation(),
        "i2pd": adapters[1].authenticated_observation(),
    }


def _pair_record(
    scenario: Any,
    result: str,
    reason: str,
    cleanup: str,
    commit: str,
    metadata_java: Any,
    metadata_i2pd: Any,
    topology: ReferencePairTopology | None,
    java_config: str,
    i2pd_config: str,
    observations: dict[str, str],
    router_info_validation: dict[str, str],
    connection_counters: dict[str, dict[str, int]],
    process_counters: dict[str, dict[str, int]],
) -> dict[str, Any]:
    zero = "0" * 64
    return {
        "schema": 2,
        "scenario_id": scenario.scenario_id,
        "date_utc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "i2pr_commit": commit.split(";", 1)[0],
        "java_reference": "java_i2p",
        "java_version": "2.12.0",
        "java_revision": getattr(metadata_java, "source_revision", "0" * 40),
        "java_artifact_sha256": getattr(metadata_java, "artifact_sha256", zero),
        "java_installed_tree_sha256": getattr(metadata_java, "installed_tree_sha256", zero),
        "java_configuration_sha256": java_config or zero,
        "i2pd_reference": "i2pd",
        "i2pd_version": "2.60.0",
        "i2pd_revision": getattr(metadata_i2pd, "source_revision", "0" * 40),
        "i2pd_artifact_sha256": getattr(metadata_i2pd, "artifact_sha256", zero),
        "i2pd_installed_tree_sha256": getattr(metadata_i2pd, "installed_tree_sha256", zero),
        "i2pd_configuration_sha256": i2pd_config or zero,
        "namespace_topology_sha256": topology.description.digest() if topology is not None else zero,
        "private_network_id": "explicit-non-public",
        "direction_policy": scenario.dial_initiator,
        "router_info_validation": router_info_validation,
        "authenticated_link_observations": observations,
        "connection_counters": connection_counters,
        "process_counters": process_counters,
        "expected_authenticated_link_count": scenario.expected_authenticated_link_count,
        "actual_typed_result": result,
        "cleanup_result": cleanup,
        "evidence_sha256": "",
        "known_deviation": reason,
        "reproduction": "sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4",
    }


def _emit(scenario_id: str, result: str, reason: str, cleanup: str) -> None:
    print(json.dumps({
        "schema": 2,
        "type": "i2pr-reference-pair-result",
        "scenario_id": scenario_id,
        "actual_typed_result": result,
        "reason_code": reason,
        "cleanup_result": cleanup,
    }, separators=(",", ":")))


def run(args: argparse.Namespace) -> int:
    repo_root = _repo_root()
    scenario = load_reference_scenario(_scenario_path(repo_root, args.scenario))
    lock_handle = _acquire_run_lock(repo_root)
    runs_root = (repo_root / "target/interop/runs").resolve()
    base = Path(args.run_root or runs_root).resolve()
    if base != runs_root and runs_root not in base.parents:
        raise HarnessBlocked("run-root-outside-target", "blocked_host_contract")
    base.mkdir(mode=0o700, parents=True, exist_ok=True)
    run_dir = base / _run_id()
    run_dir.mkdir(mode=0o700)
    evidence_root = repo_root / "target/interop/evidence"
    topology: ReferencePairTopology | None = None
    owned: dict[str, list[Any]] = {"java_i2p": [], "i2pd": []}
    stopped: set[int] = set()
    final: tuple[Any, Any] | None = None
    metadata_java = metadata_i2pd = None
    result = "blocked_missing_driver"
    reason = "not-started"
    cleanup = "not-started"
    commit = "0" * 40 + ";dirty"
    router_validation = {"java_i2p": "not-run", "i2pd": "not-run"}
    observations = {"java_i2p": "not-observed", "i2pd": "not-observed"}
    connection_counters = {
        "java_i2p": {"attempts": 0, "authenticated": 0},
        "i2pd": {"attempts": 0, "authenticated": 0},
    }
    process_counters = {
        "java_i2p": {"started": 0, "exited": 0, "forced": 0},
        "i2pd": {"started": 0, "exited": 0, "forced": 0},
    }
    try:
        _host_check(repo_root, run_dir)
        commit = _git_identity(repo_root)
        cache_root = Path(args.build_cache or repo_root / "target/interop/cache").resolve()
        cache_java, metadata_java = _cache_for(cache_root, "java_i2p", repo_root)
        cache_i2pd, metadata_i2pd = _cache_for(cache_root, "i2pd", repo_root)
        topology = ReferencePairTopology(scenario, run_dir.name.removeprefix("reference-"))
        topology.create()
        (run_dir / "exchange").mkdir(mode=0o700)
        # Generate each RouterInfo without a peer, in the scenario's declared order.
        for reference in scenario.startup_order:
            adapters = _adapter_pair(scenario, topology, cache_java, cache_i2pd, run_dir, repo_root)
            adapter = adapters[0] if reference == "java_i2p" else adapters[1]
            owned[reference].append(adapter)
            adapter.start()
            adapter.wait_ready(scenario.handshake_deadline_seconds)
            adapter.export_router_info()
            stop_result = adapter.stop(scenario.handshake_deadline_seconds)
            stopped.add(id(adapter))
            if stop_result == "failed":
                raise RuntimeError("initial-reference-cleanup-failed")
        java_info = run_dir / "java/exchange/java-router.info"
        i2pd_info = run_dir / "i2pd/exchange/i2pd-router.info"
        for reference in scenario.router_info_exchange_order:
            if reference == "java_i2p":
                strict_validate_router_info(
                    java_info,
                    expected_address=scenario.java.address,
                    expected_port=scenario.java.port,
                    repo_root=repo_root,
                )
                router_validation["java_i2p"] = "validated-and-bound"
            else:
                strict_validate_router_info(
                    i2pd_info,
                    expected_address=scenario.i2pd.address,
                    expected_port=scenario.i2pd.port,
                    repo_root=repo_root,
                )
                router_validation["i2pd"] = "validated-and-bound"
        final = _adapter_pair(scenario, topology, cache_java, cache_i2pd, run_dir, repo_root)
        owned["java_i2p"].append(final[0])
        owned["i2pd"].append(final[1])
        final[0].prepare()
        final[1].prepare()
        for reference in scenario.router_info_exchange_order:
            if reference == "java_i2p":
                final[0].import_peer_router_info(i2pd_info)
            else:
                final[1].import_peer_router_info(java_info)
        for reference in scenario.startup_order:
            adapter = final[0] if reference == "java_i2p" else final[1]
            adapter.start()
            adapter.wait_ready(scenario.handshake_deadline_seconds)
        connection_counters[scenario.dial_initiator]["attempts"] = 1
        observations = _wait_authenticated(final, scenario.handshake_deadline_seconds)
        if all(value == "authenticated" for value in observations.values()):
            connection_counters["java_i2p"]["authenticated"] = 1
            connection_counters["i2pd"]["authenticated"] = 1
            result, reason = "passed", "dual-authenticated-reference-observation"
        else:
            result, reason = "rejected", "authenticated-link-observation-missing"
    except (HarnessBlocked, ReferenceScenarioError) as exc:
        result, reason = getattr(exc, "result", "blocked_missing_driver"), str(exc)
    except (ReferenceTopologyError, JavaI2pError, I2pdError, RouterInfoPathError, OSError, RuntimeError) as exc:
        result, reason = "rejected", getattr(exc, "code", "typed-reference-pair-operation-failed")
    finally:
        for reference in ("i2pd", "java_i2p"):
            for adapter in reversed(owned[reference]):
                try:
                    adapter_cleanup = "not-started" if id(adapter) in stopped else adapter.stop(scenario.handshake_deadline_seconds)
                    stopped.add(id(adapter))
                    snapshot = adapter.counters()
                    for key in process_counters[reference]:
                        process_counters[reference][key] += snapshot.get(key, 0)
                    if adapter_cleanup == "failed":
                        cleanup = "failed"
                except RuntimeError:
                    cleanup = "failed"
        if topology is not None:
            if topology.destroy() == "failed":
                cleanup = "failed"
            elif topology.residual_state():
                cleanup = "failed"
        if cleanup == "not-started":
            cleanup = "clean"
        if cleanup == "failed":
            result = "failed_cleanup"
        if metadata_java is not None and metadata_i2pd is not None and (result == "passed" or args.keep_failed_sanitized):
            evidence_root.mkdir(mode=0o700, parents=True, exist_ok=True)
            record = _pair_record(
                scenario, result, reason, cleanup, commit, metadata_java, metadata_i2pd, topology,
                getattr(final[0], "configuration_sha256", "") if final else "",
                getattr(final[1], "configuration_sha256", "") if final else "",
                observations, router_validation, connection_counters, process_counters,
            )
            try:
                write_record(evidence_root / f"{run_dir.name}.json", record)
            except (OSError, ValueError):
                result, cleanup, reason = "failed_cleanup", "failed", "evidence-finalization-failed"
        try:
            shutil.rmtree(run_dir)
        except OSError:
            result, cleanup, reason = "failed_cleanup", "failed", "run-root-delete-failed"
        fcntl.flock(lock_handle.fileno(), fcntl.LOCK_UN)
        lock_handle.close()
    _emit(scenario.scenario_id, result, reason, cleanup)
    return 0 if result == "passed" else 2


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--build-cache")
    parser.add_argument("--run-root")
    parser.add_argument("--keep-failed-sanitized", action="store_true")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    try:
        return run(args)
    except (HarnessBlocked, ReferenceScenarioError) as exc:
        _emit(args.scenario, getattr(exc, "result", "blocked_missing_driver"), str(exc), "not-started")
        return 2
    except (OSError, RuntimeError, ValueError) as exc:
        _emit(args.scenario, "rejected", "typed-reference-pair-operation-failed", "not-started")
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
