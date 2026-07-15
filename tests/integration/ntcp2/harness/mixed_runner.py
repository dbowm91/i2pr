#!/usr/bin/env python3
"""Fail-closed Plan 044 mixed-router directional scenario runner."""

from __future__ import annotations

import argparse
import datetime as dt
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
    from harness.evidence import write_record  # type: ignore
    from harness.i2pd import I2pdAdapter, I2pdError  # type: ignore
    from harness.i2pr import I2prAdapter  # type: ignore
    from harness.java_i2p import JavaI2pAdapter, JavaI2pError  # type: ignore
    from harness.launcher_renderer import RenderError, render_and_validate  # type: ignore
    from harness.launcher_protocol import LauncherScenarioError  # type: ignore
    from harness.metadata import CacheMetadata  # type: ignore
    from harness.router_info import RouterInfoPathError, strict_validate_router_info  # type: ignore
    from harness.topology import EndpointDescription, IsolationError, NamespaceTopology  # type: ignore
    from harness.runner import HarnessBlocked, _cache_for, _git_identity, _host_check  # type: ignore
else:
    from .evidence import write_record
    from .i2pd import I2pdAdapter, I2pdError
    from .i2pr import I2prAdapter
    from .java_i2p import JavaI2pAdapter, JavaI2pError
    from .launcher_renderer import RenderError, render_and_validate
    from .launcher_protocol import LauncherScenarioError
    from .metadata import CacheMetadata
    from .router_info import RouterInfoPathError, strict_validate_router_info
    from .topology import EndpointDescription, IsolationError, NamespaceTopology
    from .runner import HarnessBlocked, _cache_for, _git_identity, _host_check


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
    topology: NamespaceTopology | None,
    router_validation: dict[str, str],
    observations: dict[str, str],
    process_counters: dict[str, dict[str, int]],
) -> dict[str, Any]:
    zero = "0" * 64
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
        "configuration_sha256": zero,
        "namespace_topology_sha256": topology.description.digest() if topology is not None else zero,
        "direction": direction.direction,
        "address_family": direction.address_family,
        "deterministic_parameters": f"seed=1;timeouts=bounded;network=synthetic-private-036;ipv6-probe=passed",
        "expected": direction.expected,
        "actual_typed_result": result,
        "resource_counters": {
            "tasks": 0, "queues": 0, "permits": 0,
            "links": 0, "handshakes": 0, "i2np_sent": 0, "i2np_received": 0,
        },
        "process_counters": process_counters,
        "cleanup_result": cleanup,
        "evidence_sha256": "",
        "known_deviation": reason,
        "reproduction": f"bash scripts/interop/run-scenario.sh --scenario {direction.execution_id} --reference {direction.reference}",
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


def _no_residual_state(topology: NamespaceTopology) -> bool:
    prefix = [] if os.geteuid() == 0 else ["sudo", "-n"]
    namespaces = subprocess.run(
        prefix + ["ip", "netns", "list"], capture_output=True, text=True, check=False,
    )
    if namespaces.returncode != 0:
        return False
    for name in (topology.i2pr_namespace, topology.reference_namespace):
        for line in namespaces.stdout.splitlines():
            if line.split() and line.split()[0] == name:
                return False
    links = subprocess.run(
        prefix + ["ip", "-o", "link", "show"], capture_output=True, text=True, check=False,
    )
    if links.returncode != 0:
        return False
    return not any(name in links.stdout for name in (topology.i2pr_if, topology.reference_if))


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
    reference = args.reference
    if direction.reference != reference:
        raise MixedRunError("scenario-reference-mismatch", "rejected")
    base = Path(args.run_root or repo_root / "target/interop/runs").resolve()
    runs_root = (repo_root / "target/interop/runs").resolve()
    if base != runs_root and runs_root not in base.parents:
        raise MixedRunError("run-root-outside-target", "blocked_host_contract")
    cache_base = Path(args.build_cache or repo_root / "target/interop/cache").resolve()
    base.mkdir(mode=0o700, parents=True, exist_ok=True)
    run_dir = base / _run_id()
    run_dir.mkdir(mode=0o700)
    evidence_root = Path(os.environ.get("INTEROP_EVIDENCE_DIR", str(repo_root / "target/interop/evidence")))
    topology: NamespaceTopology | None = None
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
    try:
        _host_check(repo_root, run_dir)
        i2pr_commit = _git_identity(repo_root)
        cache, metadata = _cache_for(cache_base, reference, repo_root)
        ipv6 = direction.address_family == "ipv6"
        if ipv6:
            disabled = Path("/proc/sys/net/ipv6/conf/all/disable_ipv6")
            capability = subprocess.run(
                ["ip", "-6", "route", "show"], capture_output=True, text=True, check=False,
            )
            if capability.returncode != 0 or (disabled.is_file() and disabled.read_text().strip() != "0"):
                raise MixedRunError("ipv6-capability-unavailable", "skipped_ipv6")
        ref_port = _reference_port_for(reference)
        topology = NamespaceTopology(
            repo_root, run_dir.name.removeprefix("mixed-"), ipv6,
            reference_port=ref_port, i2pr_port=45680,
        )
        topology.create()
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
        if direction.i2pr_is_initiator:
            _run_initiator_first(
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
            )
            ref_adapter = _make_ref_adapter(direction.reference, cache, run_dir / "ref", ref_endpoint, repo_root)
            ref_adapter.start()
            ref_adapter.wait_ready()
            i2pr_adapter = I2prAdapter(repo_root, run_dir / "i2pr", topology.i2pr_namespace)
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
            )
            i2pr_adapter.start("dial")
            terminal = i2pr_adapter.wait_terminal(timeout_seconds=30.0)
            if terminal["result"] != "passed":
                raise MixedRunError("i2pr-initiator-handshake-failed")
            observations["i2pr"] = "authenticated"
            if ref_adapter.observed_phrase(ref_adapter.authenticated_phrases):
                observations[direction.reference] = "authenticated"
            result, reason = "passed", "mixed-router-direction-authenticated"
        else:
            _run_responder_first(
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
            )
            i2pr_adapter = I2prAdapter(repo_root, run_dir / "i2pr", topology.i2pr_namespace)
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
            )
            i2pr_adapter.start("listen")
            i2pr_adapter.wait_ready(timeout_seconds=30.0)
            ref_adapter = _make_ref_adapter(direction.reference, cache, run_dir / "ref", ref_endpoint, repo_root)
            ref_adapter.start()
            ref_adapter.wait_ready()
            terminal = i2pr_adapter.wait_terminal(timeout_seconds=30.0)
            if terminal["result"] != "passed":
                raise MixedRunError("i2pr-responder-handshake-failed")
            observations["i2pr"] = "authenticated"
            if ref_adapter.observed_phrase(ref_adapter.authenticated_phrases):
                observations[direction.reference] = "authenticated"
            result, reason = "passed", "mixed-router-direction-authenticated"
    except MixedRunError as exc:
        result, reason = exc.result, exc.code
    except (HarnessBlocked,) as exc:
        result, reason = exc.result, exc.code
    except (IsolationError, JavaI2pError, I2pdError) as exc:
        result, reason = "rejected", exc.code
    except (OSError, ValueError, RuntimeError):
        result, reason = "rejected", "typed-harness-operation-failed"
    finally:
        if i2pr_adapter is not None:
            i2pr_cleanup = _stop_adapter(i2pr_adapter, 10.0)
            if i2pr_cleanup == "failed":
                cleanup = "failed"
            try:
                snap = i2pr_adapter.process.snapshot() if i2pr_adapter.process else {}
                process_counters["i2pr"]["started"] += int(snap.get("running", 0) or 1)
                process_counters["i2pr"]["exited"] += int(snap.get("running", 1) == 0)
                process_counters["i2pr"]["forced"] += int(snap.get("forced", 0))
            except RuntimeError:
                pass
        if ref_adapter is not None:
            ref_cleanup = _stop_adapter(ref_adapter, 10.0)
            if ref_cleanup == "failed":
                cleanup = "failed"
            try:
                snap = ref_adapter.process.snapshot() if ref_adapter.process else {}
                process_counters[direction.reference]["started"] += int(snap.get("running", 0) or 1)
                process_counters[direction.reference]["exited"] += int(snap.get("running", 1) == 0)
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
        if metadata is not None and result in {"passed", "skipped_ipv6"}:
            evidence_root.mkdir(mode=0o700, parents=True, exist_ok=True)
            evidence_path = evidence_root / f"{run_dir.name}-{reference}.json"
            record = _record_mixed(
                direction, result, reason, cleanup, i2pr_commit, metadata,
                topology, router_validation, observations, process_counters,
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
    _emit(direction.execution_id, reference, result, reason, cleanup)
    return 0 if result == "passed" else 2


def _run_initiator_first(
    *,
    direction: MixedDirection,
    run_dir: Path,
    repo_root: Path,
    cache: Path,
    metadata: CacheMetadata | None,
    topology: NamespaceTopology,
    i2pr_endpoint: EndpointDescription,
    ref_endpoint: EndpointDescription,
    router_validation: dict[str, str],
    observations: dict[str, str],
) -> None:
    ref_adapter = _make_ref_adapter(direction.reference, cache, run_dir / "ref-gen", ref_endpoint, repo_root)
    ref_adapter.start()
    ref_adapter.wait_ready()
    ri_path = ref_adapter.export_router_info()
    _validate_router_info_for_direction(
        ri_path, ref_endpoint.local_address, ref_endpoint.local_port,
        repo_root, router_validation, direction.reference,
    )
    exchange_dir = run_dir / "i2pr" / "exchange"
    exchange_dir.mkdir(parents=True, exist_ok=True)
    import shutil
    shutil.copyfile(ri_path, exchange_dir / "ref-router.info")
    ref_adapter.stop()


def _run_responder_first(
    *,
    direction: MixedDirection,
    run_dir: Path,
    repo_root: Path,
    cache: Path,
    metadata: CacheMetadata | None,
    topology: NamespaceTopology,
    i2pr_endpoint: EndpointDescription,
    ref_endpoint: EndpointDescription,
    router_validation: dict[str, str],
    observations: dict[str, str],
) -> None:
    i2pr_adapter = I2prAdapter(repo_root, run_dir / "i2pr-gen", topology.i2pr_namespace)
    gen_toml = render_and_validate(
        run_dir / "i2pr-gen",
        execution_id=direction.execution_id + "-gen",
        role="responder",
        address_family=direction.address_family,
        local_address=i2pr_endpoint.local_address,
        local_port=i2pr_endpoint.local_port,
        peer_address=None,
        peer_port=None,
        state_dir="state",
        peer_router_info=None,
        padding_profile=direction.padding,
    )
    i2pr_adapter.start("listen")
    i2pr_adapter.wait_ready(timeout_seconds=30.0)
    i2pr_adapter.stop(timeout_seconds=10.0)
    router_validation["i2pr"] = "validated-and-bound"
    ref_adapter = _make_ref_adapter(direction.reference, cache, run_dir / "ref-import", ref_endpoint, repo_root)
    ref_adapter.start()
    ref_adapter.wait_ready()
    ri_path = run_dir / "i2pr-gen" / "exchange" / "router.info"
    if ri_path.is_file():
        ref_adapter.import_peer_router_info(ri_path)
    ref_adapter.stop()


def _make_ref_adapter(
    reference: str,
    cache: Path,
    run_root: Path,
    endpoint: EndpointDescription,
    repo_root: Path,
) -> JavaI2pAdapter | I2pdAdapter:
    if reference == "java_i2p":
        return JavaI2pAdapter(cache, run_root, endpoint, repo_root)
    return I2pdAdapter(cache, run_root, endpoint, repo_root)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--reference", choices=("java_i2p", "i2pd"), required=True)
    parser.add_argument("--build-cache")
    parser.add_argument("--run-root")
    parser.add_argument("--keep-failed-sanitized", action="store_true")
    parser.add_argument("--offline", action="store_true")
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
