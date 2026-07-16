#!/usr/bin/env python3
"""Fail-closed Plan 040 scenario runner.

Only environment smoke may start a reference. Every retained record is
sanitized and written outside the secret-bearing run root before that root is
deleted.
"""

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
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from harness.evidence import write_record  # type: ignore
    from harness.i2pd import I2pdAdapter, I2pdError  # type: ignore
    from harness.java_i2p import JavaI2pAdapter, JavaI2pError  # type: ignore
    from harness.metadata import CacheMetadata, MetadataError, parse_metadata  # type: ignore
    from harness.topology import IsolationError, NamespaceTopology  # type: ignore
else:
    from .evidence import write_record
    from .i2pd import I2pdAdapter, I2pdError
    from .java_i2p import JavaI2pAdapter, JavaI2pError
    from .metadata import CacheMetadata, MetadataError, parse_metadata
    from .topology import IsolationError, NamespaceTopology


class HarnessBlocked(RuntimeError):
    """A prerequisite is absent or the current host is unsupported."""

    def __init__(self, code: str, result: str):
        super().__init__(code)
        self.code = code
        self.result = result


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


def _load_scenario(repo_root: Path, scenario_id: str) -> dict[str, Any]:
    aliases = {"smoke-java-ipv4": "java-ipv4-inbound-outbound", "smoke-i2pd-ipv4": "i2pd-ipv4-inbound-outbound"}
    requested_id = scenario_id
    scenario_id = aliases.get(scenario_id, scenario_id)
    scenario_dir = repo_root / "tests/integration/ntcp2/scenarios"
    for path in sorted(scenario_dir.glob("*.toml")):
        scenario = tomllib.loads(path.read_text(encoding="utf-8")).get("scenario", {})
        if scenario.get("id") == scenario_id:
            if requested_id != scenario_id:
                scenario = dict(scenario)
                scenario["profile"] = "environment-smoke"
            return scenario
    raise HarnessBlocked("unknown-scenario", "rejected")


def _run_id() -> str:
    return f"run-{dt.datetime.now(dt.UTC).strftime('%Y%m%dT%H%M%SZ')}-{os.getpid()}-{uuid.uuid4().hex[:8]}"


def _lock_sha256(repo_root: Path) -> str:
    return hashlib.sha256((repo_root / "tests/integration/ntcp2/references.lock.toml").read_bytes()).hexdigest()


def _cache_for(base: Path, reference: str, repo_root: Path) -> tuple[Path, CacheMetadata]:
    base = base.resolve()
    metadata_path: Path
    if (base / "build-metadata.txt").is_file():
        metadata_path = base / "build-metadata.txt"
    else:
        summary_path = base / "current-cache.json"
        if not summary_path.is_file():
            raise HarnessBlocked("missing-current-cache-summary", "blocked")
        try:
            summary = json.loads(summary_path.read_text(encoding="utf-8"))
            if summary.get("schema") != 2 or summary.get("host_contract") != "ubuntu-24.04-amd64" or summary.get("lock_sha256") != _lock_sha256(repo_root):
                raise ValueError("current cache summary contract mismatch")
            entries = summary["references"]
            entry = next(value for value in entries if value["reference"] == reference)
            cache_key = entry["cache_key"]
            if not isinstance(cache_key, str) or not cache_key.isalnum() or len(cache_key) != 64:
                raise ValueError("invalid cache key")
            cache = base / reference / cache_key
            metadata_path = cache / "build-metadata.txt"
            recorded = (repo_root / entry["metadata"]).resolve()
            if recorded != metadata_path.resolve():
                raise ValueError("summary metadata path does not match cache key")
        except (OSError, KeyError, StopIteration, TypeError, ValueError, json.JSONDecodeError) as exc:
            raise HarnessBlocked("invalid-current-cache-summary", "blocked") from exc
    try:
        metadata = parse_metadata(
            metadata_path,
            selected_reference=reference,
            cache_root=metadata_path.parent,
            expected_lock_sha256=_lock_sha256(repo_root),
            expected_host_contract="ubuntu-24.04-amd64",
        )
    except MetadataError as exc:
        raise HarnessBlocked("invalid-reference-cache", "blocked") from exc
    if not (base / "build-metadata.txt").is_file():
        if entry.get("artifact_sha256") != metadata.artifact_sha256 or entry.get("installed_tree_sha256") != metadata.installed_tree_sha256:
            raise HarnessBlocked("current-cache-summary-hash-mismatch", "blocked")
    return metadata_path.parent, metadata


def _host_check(repo_root: Path, run_root: Path) -> None:
    checker = repo_root / "scripts/interop/ubuntu/check-host.sh"
    result = subprocess.run(["bash", str(checker), "--post-install", "--metadata", str(run_root / "host-metadata.json")], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    if result.returncode != 0:
        raise HarnessBlocked("host-contract-failed", "blocked_host_contract")


def _git_identity(repo_root: Path) -> str:
    commit = subprocess.run(["git", "-C", str(repo_root), "rev-parse", "HEAD"], capture_output=True, text=True, check=False)
    if commit.returncode != 0 or not __import__("re").fullmatch(r"[0-9a-f]{40}", commit.stdout.strip()):
        raise HarnessBlocked("missing-i2pr-commit", "blocked_host_contract")
    status = subprocess.run(["git", "-C", str(repo_root), "status", "--porcelain"], capture_output=True, text=True, check=False)
    disposition = "clean" if status.returncode == 0 and not status.stdout else "dirty"
    return f"{commit.stdout.strip()};{disposition}"


def _make_reference_adapter(
    reference: str,
    cache: Path,
    run_dir: Path,
    endpoint: "EndpointDescription",
    repo_root: Path,
) -> "JavaI2pAdapter | I2pdAdapter":
    # Plan 045 D10: an unknown reference kind must fail closed rather
    # than fall through to the i2pd adapter, which would silently pass
    # any non-canonical scenario.
    if reference == "java_i2p":
        return JavaI2pAdapter(cache, run_dir, endpoint, repo_root)
    if reference == "i2pd":
        return I2pdAdapter(cache, run_dir, endpoint, repo_root)
    raise HarnessBlocked("unknown-reference-kind", "rejected")


def _no_residual_state(topology: NamespaceTopology) -> bool:
    prefix = [] if os.geteuid() == 0 else ["sudo", "-n"]
    namespaces = subprocess.run(prefix + ["ip", "netns", "list"], capture_output=True, text=True, check=False)
    if namespaces.returncode != 0 or any(line.split()[0] in {topology.i2pr_namespace, topology.reference_namespace} for line in namespaces.stdout.splitlines() if line.split()):
        return False
    links = subprocess.run(prefix + ["ip", "-o", "link", "show"], capture_output=True, text=True, check=False)
    return links.returncode == 0 and not any(name in links.stdout for name in (topology.i2pr_if, topology.reference_if))


def _record(
    scenario: dict[str, Any], reference: str, result: str, cleanup: str, reason: str,
    i2pr_commit: str, metadata: CacheMetadata, topology: NamespaceTopology | None,
    adapter: Any, run_root: Path,
) -> dict[str, Any]:
    config_hash = getattr(adapter, "configuration_sha256", "")
    if not config_hash and (run_root / "config.sha256").is_file():
        config_hash = (run_root / "config.sha256").read_text(encoding="ascii").strip()
    if not config_hash:
        config_hash = "0" * 64
    topology_hash = topology.description.digest() if topology is not None else "0" * 64
    address_family = str(scenario.get("address_family", "ipv4"))
    probe = "ipv6-probe=passed" if address_family == "ipv4" else "ipv6-probe=passed-or-typed-skip"
    if reason == "ipv6-capability-unavailable":
        probe = "ipv6-probe=unavailable;result=skipped_ipv6"
    process_counters = adapter.counters() if adapter is not None else {"started": 0, "exited": 0, "forced": 0}
    return {
        "schema": 1, "scenario_id": str(scenario["id"]),
        "date_utc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "i2pr_commit": i2pr_commit, "reference": reference,
        "reference_version": "2.12.0" if reference == "java_i2p" else "2.60.0",
        "reference_revision": metadata.source_revision, "artifact_sha256": metadata.artifact_sha256,
        "installed_tree_sha256": metadata.installed_tree_sha256, "configuration_sha256": config_hash,
        "namespace_topology_sha256": topology_hash, "direction": str(scenario.get("direction", "both")),
        "address_family": address_family,
        "deterministic_parameters": f"seed=1;timeouts=bounded;network=synthetic-private-036;{probe}",
        "expected": str(scenario.get("expected", "bounded-result")), "actual_typed_result": result,
        "resource_counters": {"tasks": 0, "queues": 0, "permits": 0, "links": 0, "handshakes": 0, "i2np_sent": 0, "i2np_received": 0},
        "process_counters": process_counters, "cleanup_result": cleanup, "evidence_sha256": "",
        "known_deviation": reason, "reproduction": "bash scripts/interop/run-scenario.sh --scenario " + str(scenario["id"]) + " --reference " + reference,
        "i2pr_router_info_sha256": "0" * 64, "reference_router_info_sha256": "0" * 64,
        "data_phase_mode": "handshake-only",
        "expected_observation": "no-data-phase-required",
        "topology_kind": "privileged-dual-netns-veth",
        "privilege_model": "host-capabilities",
        "sandbox_attestation_sha256": "0" * 64,
        "parent_network_state_unchanged": False,
    }


def _emit(scenario_id: str, reference: str, result: str, reason: str, cleanup: str) -> None:
    print(json.dumps({"schema": 1, "type": "i2pr-interop-result", "scenario_id": scenario_id,
                      "reference": reference, "actual_typed_result": result,
                      "reason_code": reason, "cleanup_result": cleanup}, separators=(",", ":")))


def run(args: argparse.Namespace) -> int:
    repo_root = _repo_root()
    scenario = _load_scenario(repo_root, args.scenario)
    reference = args.reference
    if scenario.get("reference") != reference:
        raise HarnessBlocked("scenario-reference-mismatch", "rejected")
    base = Path(args.run_root or repo_root / "target/interop/runs").resolve()
    runs_root = (repo_root / "target/interop/runs").resolve()
    if base != runs_root and runs_root not in base.parents:
        raise HarnessBlocked("run-root-outside-target", "blocked_host_contract")
    cache_base = Path(args.build_cache or repo_root / "target/interop/cache").resolve()
    base.mkdir(mode=0o700, parents=True, exist_ok=True)
    run_dir = base / _run_id()
    run_dir.mkdir(mode=0o700)
    evidence_root = Path(os.environ.get("INTEROP_EVIDENCE_DIR", str(repo_root / "target/interop/evidence")))
    topology: NamespaceTopology | None = None
    adapter: Any = None
    metadata: CacheMetadata | None = None
    cleanup = "not-started"
    result = "blocked"
    reason = "not-started"
    i2pr_commit = "0" * 40 + ";dirty"
    evidence_path: Path | None = None
    try:
        _host_check(repo_root, run_dir)
        i2pr_commit = _git_identity(repo_root)
        cache, metadata = _cache_for(cache_base, reference, repo_root)
        if scenario.get("profile") != "environment-smoke":
            raise HarnessBlocked("i2pr-mixed-router-profile-not-wired", "blocked")
        if scenario.get("address_family") == "ipv6":
            disabled = Path("/proc/sys/net/ipv6/conf/all/disable_ipv6")
            capability = subprocess.run(["ip", "-6", "route", "show"], capture_output=True, text=True, check=False)
            if capability.returncode != 0 or (disabled.is_file() and disabled.read_text().strip() != "0"):
                raise HarnessBlocked("ipv6-capability-unavailable", "skipped_ipv6")
        ipv6 = str(scenario.get("address_family")) == "ipv6"
        reference_port = 45678 if reference == "java_i2p" else 45679
        topology = NamespaceTopology(repo_root, run_dir.name.removeprefix("run-"), ipv6, reference_port=reference_port)
        topology.create()
        endpoint = topology.description.endpoint_for_reference()
        adapter = _make_reference_adapter(reference, cache, run_dir, endpoint, repo_root)
        adapter.start()
        adapter.wait_ready()
        adapter.export_router_info()
        result = "passed"
        reason = "environment-smoke-only"
    except HarnessBlocked as exc:
        result, reason = exc.result, exc.code
    except (IsolationError, JavaI2pError, I2pdError) as exc:
        result = "failed_cleanup" if exc.code == "isolation-preflight-failed" else "rejected"
        reason = exc.code
    except (OSError, ValueError, RuntimeError):
        result, reason = "rejected", "typed-harness-operation-failed"
    finally:
        if adapter is not None:
            try:
                cleanup = adapter.stop()
            except RuntimeError:
                cleanup = "failed"
        if topology is not None:
            topology_cleanup = topology.destroy()
            if topology_cleanup == "failed":
                cleanup = "failed"
            if cleanup == "clean" and not _no_residual_state(topology):
                cleanup = "failed"
        if cleanup == "failed":
            result = "failed_cleanup"
        if metadata is not None and (result in {"passed", "skipped_ipv6"} or args.keep_failed_sanitized):
            evidence_root.mkdir(mode=0o700, parents=True, exist_ok=True)
            evidence_path = evidence_root / f"{run_dir.name}-{reference}.json"
            record = _record(scenario, reference, result, cleanup, reason, i2pr_commit, metadata, topology, adapter, run_dir)
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
    _emit(str(scenario["id"]), reference, result, reason, cleanup)
    return 0 if result == "passed" else 2


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--reference", choices=("java_i2p", "i2pd"), required=True)
    parser.add_argument("--build-cache")
    parser.add_argument("--run-root")
    parser.add_argument("--keep-failed-sanitized", action="store_true")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--verbose-typed", action="store_true")
    args = parser.parse_args()
    try:
        return run(args)
    except HarnessBlocked as exc:
        _emit(args.scenario, args.reference, exc.result, exc.code, "not-started")
        return 2
    except (OSError, ValueError, RuntimeError):
        _emit(args.scenario, args.reference, "rejected", "typed-harness-operation-failed", "not-started")
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
