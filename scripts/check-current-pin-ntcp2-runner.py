#!/usr/bin/env python3
"""Fail-closed source checks for the Plan 410 current-pin helper."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "tools/i2pr-interop/reference/i2pd-current/src/i2pd_current_ntcp2_driver.cpp"
BUILD = ROOT / "tools/i2pr-interop/reference/i2pd-current/build.sh"
CMAKE = ROOT / "tools/i2pr-interop/reference/i2pd-current/CMakeLists.txt"
PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"


def source_findings(driver: str, build: str, cmake: str) -> list[str]:
    findings = []
    checks = {
        "pinned current revision": PIN in driver and PIN in build,
        "current network topology": '"current-network-loopback"' in driver,
        "network ID 2 only": "if (cfg.network_id != 2)" in driver,
        "loopback-only address allowlist": '"127.0.0.1"' in driver,
        "NTCP2 only, SSU2 disabled": 'set_bool_option("ssu2.enabled", false)' in driver,
        "no external reseed endpoints": 'set_string_option("reseed.urls", "")' in driver
        and 'set_string_option("reseed.yggurls", "")' in driver,
        "observer instrumentation disabled": "I2PD_INTEROP_OBSERVER=1" not in cmake,
        "stock library build": "-DWITH_LIBRARY=ON" in build and "patch " not in build,
        "no identity hashes in evidence": "local_router_hash_sha256" not in driver
        and "peer_router_hash_sha256\\\":" not in driver,
        "no raw detail in evidence": '"detail"' not in driver,
    }
    findings.extend(name for name, passed in checks.items() if not passed)
    return findings


def self_test() -> bool:
    driver = DRIVER.read_text()
    build = BUILD.read_text()
    cmake = CMAKE.read_text()
    if source_findings(driver, build, cmake):
        return False
    mutations = [
        (driver.replace("if (cfg.network_id != 2)", "if (cfg.network_id != 99)"), build, cmake),
        (driver.replace('"current-network-loopback"', '"host-loopback-development"'), build, cmake),
        (driver.replace('set_string_option("reseed.urls", "");', ""), build, cmake),
        (driver + '\n"local_router_hash_sha256":"\n', build, cmake),
        (driver, build, cmake + "\n-DI2PD_INTEROP_OBSERVER=1\n"),
    ]
    return all(source_findings(*candidate) for candidate in mutations)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        if self_test():
            print("current-pin NTCP2 runner checker self-test passed")
            return 0
        print("current-pin NTCP2 runner checker self-test failed")
        return 1
    findings = source_findings(DRIVER.read_text(), BUILD.read_text(), CMAKE.read_text())
    if findings:
        for finding in findings:
            print(f"current-pin NTCP2 runner check failed: {finding}")
        return 1
    print("current-pin NTCP2 runner source checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
