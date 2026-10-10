#!/usr/bin/env python3
"""Fail-closed source checks for the Plan 410 current-pin helper."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "tools/i2pr-interop/reference/i2pd-current/src/i2pd_current_ntcp2_driver.cpp"
BUILD = ROOT / "tools/i2pr-interop/reference/i2pd-current/build.sh"
CMAKE = ROOT / "tools/i2pr-interop/reference/i2pd-current/CMakeLists.txt"
OBSERVER = ROOT / "tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py"
RUNNER = ROOT / "tools/i2pr-interop/reference/i2pd-current/run_plan414.py"
PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
REJECTION_COUNTERS = (
    "session_confirmed_part2_kdf_failure_count",
    "session_confirmed_unexpected_block_count",
    "session_confirmed_unexpected_router_info_size_count",
    "session_confirmed_router_info_verification_failure_count",
    "session_confirmed_router_info_too_old_count",
    "session_confirmed_router_info_from_future_count",
    "session_confirmed_router_version_too_old_count",
    "session_confirmed_router_info_update_failure_count",
    "session_confirmed_address_not_found_count",
    "session_confirmed_host_mismatch_count",
    "session_confirmed_wrong_static_key_count",
    "session_confirmed_router_info_accepted_count",
    "ntcp2_session_terminated_count",
)


def source_findings(driver: str, build: str, cmake: str, observer: str, runner: str) -> list[str]:
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
        "private helper enables debug before logger startup": (
            driver.count('Logger().SetLogLevel("debug")') == 1
            and driver.find("Logger().SendTo") < driver.find('Logger().SetLogLevel("debug")')
            < driver.find("Logger().Start")
        ),
        "closed stock handshake stage allowlist": "STAGE_PATTERNS = {" in observer
        and "session_request_received_count" in observer
        and "session_created_received_count" in observer
        and "session_confirmed_received_count" in observer
        and all(name in observer and name in runner for name in REJECTION_COUNTERS),
        "source-verified SessionConfirmed rejection marker phrases": all(
            phrase in observer for phrase in (
                "SessionConfirmed Part2 KDF failed", "Unexpected block",
                "Unexpected RouterInfo size", "RouterInfo verification failed in SessionConfirmed",
                "RouterInfo is too old in SessionConfirmed", "RouterInfo is from future",
                "Router version",
                "Couldn't update RouterInfo from SessionConfirmed in netdb",
                "Address not found in SessionConfirmed", "Host mismatch between published address",
                "Wrong static key in SessionConfirmed",
                "SessionConfirmed from", "Session with",
            )
        ),
        "post-baseline bounded observer": "raw[baseline_offset:]" in observer
        and "MAX_LINE_BYTES = 4096" in observer,
        "no retained raw-log digest": '"log_sha256":' not in observer
        and '"raw_log_sha256"' not in runner,
        "runner passes the log baseline": '"--baseline-offset", str(log_baseline)' in runner,
        "closed direction selector exposes reverse-only mode": (
            'choices=("forward", "reverse", "both")' in runner
            and 'if selection == "reverse":' in runner
            and 'return ("reverse",)' in runner
            and 'if selection == "both":' in runner
            and 'return ("forward", "reverse")' in runner
            and "direction-selection-invalid" in runner
        ),
    }
    findings.extend(name for name, passed in checks.items() if not passed)
    return findings


def self_test() -> bool:
    driver = DRIVER.read_text()
    build = BUILD.read_text()
    cmake = CMAKE.read_text()
    observer = OBSERVER.read_text()
    runner = RUNNER.read_text()
    if source_findings(driver, build, cmake, observer, runner):
        return False
    mutations = [
        (driver.replace("if (cfg.network_id != 2)", "if (cfg.network_id != 99)"), build, cmake),
        (driver.replace('"current-network-loopback"', '"host-loopback-development"'), build, cmake),
        (driver.replace('set_string_option("reseed.urls", "");', ""), build, cmake),
        (driver + '\n"local_router_hash_sha256":"\n', build, cmake),
        (driver, build, cmake + "\n-DI2PD_INTEROP_OBSERVER=1\n"),
        (driver, build, cmake, observer.replace("session_request_received_count", "request_stage_removed"), runner),
        (driver, build, cmake, observer.replace(REJECTION_COUNTERS[0], "rejection_stage_removed"), runner),
        (driver, build, cmake, observer.replace("session_confirmed_router_info_accepted_count",
                                                 "post_validation_stage_removed"), runner),
        (driver, build, cmake, observer.replace("raw[baseline_offset:]", "raw"), runner),
        (driver, build, cmake, observer, runner.replace('"--baseline-offset", str(log_baseline)', '"--baseline-offset", "0"')),
        (driver, build, cmake, observer, runner.replace('return ("reverse",)', 'return ("forward",)')),
        (driver.replace('Logger().SetLogLevel("debug");', ""), build, cmake, observer, runner),
        (driver.replace('Logger().SetLogLevel("debug");\n    i2p::log::Logger().Start();',
                        'i2p::log::Logger().Start();\n    i2p::log::Logger().SetLogLevel("debug");'),
         build, cmake, observer, runner),
    ]
    normalized = [candidate if len(candidate) == 5 else (*candidate, observer, runner) for candidate in mutations]
    return all(source_findings(*candidate) for candidate in normalized)


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
    findings = source_findings(DRIVER.read_text(), BUILD.read_text(), CMAKE.read_text(),
                               OBSERVER.read_text(), RUNNER.read_text())
    if findings:
        for finding in findings:
            print(f"current-pin NTCP2 runner check failed: {finding}")
        return 1
    print("current-pin NTCP2 runner source checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
