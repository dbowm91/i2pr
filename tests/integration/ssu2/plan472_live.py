#!/usr/bin/env python3
"""Plan 472 per-host offline preparation and signed RouterInfo verification.

No command in this module starts a router or emits a packet. ``run`` is a
fail-closed authorization gate: a real live exchange also needs a reviewed
pair-specific execution adapter and owner approval on both hosts. Until those
inputs exist, the command reports environment-unavailable rather than
pretending the offline checks are a live qualification.
"""

from __future__ import annotations

import argparse
import importlib.util
import ipaddress
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
PREFLIGHT = ROOT / "scripts/plan449-ssu2-preflight.py"
PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
NETWORK_ID = "2"
SCHEMA = "i2pr-plan472-live-runner-v1"
MAX_ROUTER_INFO_AGE_MS = 10 * 60 * 1000
MAX_PAIR_CLOCK_SKEW_MS = 5 * 60 * 1000


class RunnerError(Exception):
    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


def preflight_module():
    spec = importlib.util.spec_from_file_location("plan449_preflight", PREFLIGHT)
    if spec is None or spec.loader is None:
        raise RunnerError("preflight-unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_inventory(path: Path):
    preflight = preflight_module()
    try:
        repo = preflight.repository_root()
        return preflight, preflight.load_inventory(path, repo)
    except preflight.PreflightError as error:
        raise RunnerError(error.code) from None


def validate_summary(summary: object, host: dict[str, object]) -> str:
    if not isinstance(summary, dict) or summary.get("result") != "validated":
        raise RunnerError("router-info-invalid")
    if summary.get("network_id") != NETWORK_ID:
        raise RunnerError("network-id-mismatch")
    addresses = summary.get("addresses")
    if not isinstance(addresses, list):
        raise RunnerError("router-info-address-missing")
    matching = [
        row for row in addresses
        if isinstance(row, dict)
        and row.get("expected_endpoint_matches") is True
        and row.get("style") == "SSU2"
        and row.get("static_key") is True
        and row.get("i_key") is True
    ]
    if not matching:
        raise RunnerError("router-info-endpoint-or-key-mismatch")
    published_ms = summary.get("published_ms")
    now_ms = int(time.time() * 1000)
    if (not isinstance(published_ms, int) or isinstance(published_ms, bool)
            or published_ms > now_ms + MAX_PAIR_CLOCK_SKEW_MS
            or now_ms - published_ms > MAX_ROUTER_INFO_AGE_MS):
        raise RunnerError("router-info-stale-or-clock-misaligned")
    return str(summary.get("version", "")), published_ms


def inspect_router_info(binary: Path, path: Path, host: dict[str, object]) -> dict[str, object]:
    endpoint = f"{host['_ip']}:{host['udp_port']}"
    try:
        result = subprocess.run(
            [str(binary), "ntcp2", "router-info-summary", "--router-info", str(path),
             "--expected-endpoint", endpoint],
            capture_output=True, text=True, check=False, timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired):
        raise RunnerError("router-info-inspector-unavailable") from None
    try:
        summary = json.loads(result.stdout)
    except json.JSONDecodeError:
        raise RunnerError("router-info-invalid") from None
    if result.returncode != 0:
        raise RunnerError("router-info-invalid")
    version, published_ms = validate_summary(summary, host)
    try:
        identity = subprocess.run(
            [str(binary), "ntcp2", "router-hash", "--router-info", str(path)],
            capture_output=True, text=True, check=False, timeout=10,
        )
        identity_record = json.loads(identity.stdout)
    except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
        raise RunnerError("router-info-invalid") from None
    if identity.returncode != 0 or identity_record.get("result") != "validated":
        raise RunnerError("router-info-invalid")
    return {"network_id": NETWORK_ID, "version": version,
            "published_ms": published_ms,
            "identity_hash": identity_record.get("router_hash_sha256"), "validated": True}


def validate_pair_reports(reports: list[dict[str, object]]) -> None:
    if len(reports) != 2:
        raise RunnerError("router-info-pair-invalid")
    if reports[0].get("identity_hash") == reports[1].get("identity_hash"):
        raise RunnerError("router-identities-not-distinct")
    if reports[1].get("version") != "0.9.62":
        raise RunnerError("reference-router-version-mismatch")
    first, second = reports[0].get("published_ms"), reports[1].get("published_ms")
    if not isinstance(first, int) or isinstance(first, bool) or not isinstance(second, int) or isinstance(second, bool):
        raise RunnerError("router-info-clock-skew")
    if abs(first - second) > MAX_PAIR_CLOCK_SKEW_MS:
        raise RunnerError("router-info-clock-skew")


def verify_pair(args: argparse.Namespace) -> dict[str, object]:
    preflight, hosts = load_inventory(args.manifest)
    i2pr_host, i2pd_host = hosts
    reports = []
    for role, host, source, binary in (
        ("i2pr", i2pr_host, args.i2pr_router_info, args.i2pr_inspector),
        ("i2pd", i2pd_host, args.i2pd_router_info, args.i2pd_inspector),
    ):
        if not source.is_file():
            raise RunnerError("router-info-unavailable")
        report = inspect_router_info(binary, source, host)
        reports.append(report)
    validate_pair_reports(reports)
    # The signed identity hashes are intentionally not printed or persisted.
    return {"pair_validated": True, "network_id": NETWORK_ID,
            "reference_revision": PIN, "packet_attempt": "none"}


def run_self_test() -> None:
    row = {"style": "SSU2", "expected_endpoint_matches": True,
           "static_key": True, "i_key": True}
    summary = {"result": "validated", "network_id": "2", "version": "0.9.62",
               "published_ms": int(time.time() * 1000),
               "addresses": [row]}
    host = {"_ip": ipaddress.ip_address("192.168.44.11"), "udp_port": 19002}
    version, published_ms = validate_summary(summary, host)
    assert version == "0.9.62" and published_ms == summary["published_ms"]
    cases = [
        ({**summary, "network_id": "3"}, "network-id-mismatch"),
        ({**summary, "addresses": [{**row, "expected_endpoint_matches": False}]},
         "router-info-endpoint-or-key-mismatch"),
        ({**summary, "addresses": [{**row, "i_key": False}]},
         "router-info-endpoint-or-key-mismatch"),
        ({**summary, "addresses": [{**row, "style": "NTCP2"}]},
         "router-info-endpoint-or-key-mismatch"),
        ({**summary, "published_ms": 0}, "router-info-stale-or-clock-misaligned"),
    ]
    for candidate, expected in cases:
        try:
            validate_summary(candidate, host)
        except RunnerError as error:
            assert error.code == expected
        else:
            raise AssertionError(f"expected {expected}")
    report = {"identity_hash": "a" * 64, "version": "0.9.62",
              "published_ms": summary["published_ms"]}
    validate_pair_reports([report, {**report, "identity_hash": "b" * 64}])
    for reports, expected in [
        ([report, report], "router-identities-not-distinct"),
        ([report, {**report, "identity_hash": "b" * 64, "version": "0.9.61"}],
         "reference-router-version-mismatch"),
        ([report, {**report, "identity_hash": "b" * 64,
                   "published_ms": report["published_ms"] + 400_000}],
         "router-info-clock-skew"),
    ]:
        try:
            validate_pair_reports(reports)
        except RunnerError as error:
            assert error.code == expected
        else:
            raise AssertionError(f"expected {expected}")
    print("plan472-live-self-test: passed (2 positive/8 negative cases)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "verify", "run"), nargs="?")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--role", choices=("i2pr", "i2pd"))
    parser.add_argument("--reference-source", type=Path)
    parser.add_argument("--i2pd-binary", type=Path)
    parser.add_argument("--i2pr-router-info", type=Path)
    parser.add_argument("--i2pd-router-info", type=Path)
    parser.add_argument("--i2pr-inspector", type=Path, default=ROOT / "target/debug/i2pr-interop")
    parser.add_argument("--i2pd-inspector", type=Path, default=ROOT / "target/debug/i2pr-interop")
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        run_self_test()
        return 0
    if args.command is None:
        parser.error("select prepare, verify, or run")
    try:
        if not args.manifest:
            raise RunnerError("authorization-missing")
        if args.command == "run" and not args.live:
            raise RunnerError("live-flag-required")
        if args.command == "prepare":
            if not all((args.role, args.reference_source, args.i2pd_binary)):
                raise RunnerError("preflight-input-missing")
            preflight, hosts = load_inventory(args.manifest)
            i2pr_host, i2pd_host = hosts
            local, peer = (i2pr_host, i2pd_host) if args.role == "i2pr" else (i2pd_host, i2pr_host)
            preflight.check_local_host(local, peer, preflight.repository_root())
            binary_sha256 = preflight.verify_reference(args.reference_source, args.i2pd_binary)
            output = {"schema": SCHEMA, "result": "prepare-passed", "role": args.role,
                      "reference_revision": PIN, "reference_binary_sha256": binary_sha256,
                      "packet_attempt": "none"}
        else:
            output = {"schema": SCHEMA, **verify_pair(args)}
            output["result"] = "verify-passed"
            if args.command == "run":
                # No executor is currently qualified to generate attributable
                # bidirectional product receipts on separate owner-operated hosts.
                raise RunnerError("live-executor-unavailable")
    except RunnerError as error:
        print(json.dumps({"schema": SCHEMA, "result": "environment-unavailable",
                          "reason": error.code, "packet_attempt": "none",
                          "live_qualification": "not-run"}, sort_keys=True))
        return 1
    except preflight_module().PreflightError as error:
        print(json.dumps({"schema": SCHEMA, "result": "environment-unavailable",
                          "reason": error.code, "packet_attempt": "none",
                          "live_qualification": "not-run"}, sort_keys=True))
        return 1
    print(json.dumps(output, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
