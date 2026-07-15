#!/usr/bin/env python3
"""Fail-closed Plan 038 scenario runner.

The runner emits one typed JSON line and never emits child-router log text.
Missing host/build/driver prerequisites are blocked outcomes, not successes.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import secrets
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
    from harness.topology import IsolationError, NamespaceTopology  # type: ignore
else:
    from .evidence import write_record
    from .i2pd import I2pdAdapter, I2pdError
    from .java_i2p import JavaI2pAdapter, JavaI2pError
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
    aliases = {
        "smoke-java-ipv4": "java-ipv4-inbound-outbound",
        "smoke-i2pd-ipv4": "i2pd-ipv4-inbound-outbound",
    }
    requested_id = scenario_id
    scenario_id = aliases.get(scenario_id, scenario_id)
    scenario_dir = repo_root / "tests/integration/ntcp2/scenarios"
    for path in sorted(scenario_dir.glob("*.toml")):
        value = tomllib.loads(path.read_text(encoding="utf-8"))
        scenario = value.get("scenario", {})
        if scenario.get("id") == scenario_id:
            if requested_id != scenario_id:
                scenario = dict(scenario)
                scenario["profile"] = "environment-smoke"
            return scenario
    raise HarnessBlocked("unknown-scenario", "blocked_missing_driver")


def _run_id() -> str:
    return f"run-{dt.datetime.now(dt.UTC).strftime('%Y%m%dT%H%M%SZ')}-{os.getpid()}-{uuid.uuid4().hex[:8]}"


def _cache_for(base: Path, reference: str) -> Path:
    if (base / reference / "build-metadata.txt").is_file():
        return base / reference
    candidates = sorted(path.parent for path in base.rglob("build-metadata.txt") if path.is_file())
    for candidate in candidates:
        metadata = candidate / "build-metadata.txt"
        if f"reference={reference}" in metadata.read_text(encoding="utf-8"):
            return candidate
    raise HarnessBlocked("missing-reference-cache", "blocked_missing_driver")


def _host_check(repo_root: Path) -> None:
    checker = repo_root / "scripts/interop/ubuntu/check-host.sh"
    result = subprocess.run(
        ["bash", str(checker), "--post-install"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if result.returncode != 0:
        raise HarnessBlocked("host-contract-failed", "blocked_host_contract")


def _hash_file(path: Path) -> str:
    if not path.is_file():
        return "0" * 64
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _record(scenario: dict[str, Any], reference: str, result: str, cleanup: str, reason: str, run_root: Path) -> dict[str, Any]:
    reference_values = {"java_i2p": ("2.12.0", "2800040"), "i2pd": ("2.60.0", "f618e41")}
    version, revision = reference_values[reference]
    return {
        "schema": 1,
        "scenario_id": str(scenario["id"]),
        "date_utc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "i2pr_commit": "record-at-execution",
        "reference": reference,
        "reference_version": version,
        "reference_revision": revision,
        "artifact_sha256": "0" * 64,
        "installed_tree_sha256": "0" * 64,
        "configuration_sha256": _hash_file(run_root / "config.sha256"),
        "namespace_topology_sha256": "0" * 64,
        "direction": str(scenario.get("direction", "both")),
        "address_family": str(scenario.get("address_family", "ipv4")),
        "deterministic_parameters": "seed=1;timeouts=bounded;network=synthetic-private-036",
        "expected": str(scenario.get("expected", "bounded-result")),
        "actual_typed_result": result,
        "resource_counters": {"tasks": 0, "queues": 0, "permits": 0, "links": 0},
        "process_counters": {"started": 0, "exited": 0, "forced": 0},
        "cleanup_result": cleanup,
        "evidence_sha256": "",
        "known_deviation": reason,
        "reproduction": "sudo -E bash scripts/interop/run-scenario.sh --scenario " + str(scenario["id"]),
    }


def _emit(scenario_id: str, reference: str, result: str, reason: str, cleanup: str) -> None:
    print(json.dumps({
        "schema": 1,
        "type": "i2pr-interop-result",
        "scenario_id": scenario_id,
        "reference": reference,
        "actual_typed_result": result,
        "reason_code": reason,
        "cleanup_result": cleanup,
    }, separators=(",", ":")))


def run(args: argparse.Namespace) -> int:
    repo_root = _repo_root()
    scenario = _load_scenario(repo_root, args.scenario)
    reference = args.reference
    base = Path(args.run_root or repo_root / "target/interop/runs").resolve()
    cache_base = Path(args.build_cache or repo_root / "target/interop/cache").resolve()
    base.mkdir(mode=0o700, parents=True, exist_ok=True)
    run_dir = base / _run_id()
    run_dir.mkdir(mode=0o700)
    topology: NamespaceTopology | None = None
    reference_adapter: Any = None
    cleanup = "not-started"
    result = "blocked_missing_driver"
    reason = "not-started"
    try:
        _host_check(repo_root)
        cache = _cache_for(cache_base, reference)
        if scenario.get("profile") != "environment-smoke":
            raise HarnessBlocked("i2pr-wire-driver-not-available", "blocked_missing_driver")
        scenario_path = run_dir / "scenario.toml"
        scenario_path.write_text("[scenario]\n" + "\n".join(f'{key} = {json.dumps(value)}' for key, value in scenario.items()) + "\n", encoding="utf-8")
        (run_dir / "secrets").mkdir(mode=0o700)
        (run_dir / "secrets/router.identity").write_bytes(secrets.token_bytes(32))
        (run_dir / "secrets/ntcp2.static.key").write_bytes(secrets.token_bytes(32))
        ipv6 = str(scenario.get("address_family")) == "ipv6"
        topology = NamespaceTopology(repo_root, run_dir.name.removeprefix("run-"), ipv6)
        topology.create()
        if reference == "java_i2p":
            reference_adapter = JavaI2pAdapter(cache, run_dir, topology.reference_namespace, repo_root)
        else:
            reference_adapter = I2pdAdapter(cache, run_dir, topology.reference_namespace, repo_root)
        reference_adapter.start()
        reference_adapter.wait_ready()
        reference_adapter.export_router_info()
        result = "passed"
        reason = "environment-smoke-only"
    except HarnessBlocked as exc:
        result = exc.result
        reason = exc.code
    except (IsolationError, JavaI2pError, I2pdError) as exc:
        result = "failed_cleanup" if exc.code == "isolation-preflight-failed" else "rejected"
        reason = exc.code
    except (OSError, ValueError, RuntimeError):
        result = "rejected"
        reason = "typed-harness-operation-failed"
    finally:
        if reference_adapter is not None:
            try:
                cleanup = reference_adapter.stop()
            except RuntimeError:
                cleanup = "failed"
        if topology is not None:
            topology_cleanup = topology.destroy()
            if topology_cleanup == "forced" and cleanup == "clean":
                cleanup = "forced"
        if cleanup == "failed":
            result = "failed_cleanup"
        if args.keep_failed_sanitized or result == "passed":
            record_path = run_dir / "sanitized-result.json"
            try:
                write_record(record_path, _record(scenario, reference, result, cleanup, reason, run_dir))
            except ValueError:
                pass
        if not args.keep_failed_sanitized and run_dir.exists():
            shutil.rmtree(run_dir)
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
