#!/usr/bin/env python3
"""Fail-closed source checks for the Plan 410 current-pin helper."""

from __future__ import annotations

import argparse
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "tools/i2pr-interop/reference/i2pd-current/src/i2pd_current_ntcp2_driver.cpp"
BUILD = ROOT / "tools/i2pr-interop/reference/i2pd-current/build.sh"
CMAKE = ROOT / "tools/i2pr-interop/reference/i2pd-current/CMakeLists.txt"
OBSERVER = ROOT / "tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py"
RUNNER = ROOT / "tools/i2pr-interop/reference/i2pd-current/run_plan414.py"
LAUNCHER = ROOT / "tools/i2pr-interop/src/main.rs"
STATUS = ROOT / "tools/i2pr-interop/src/status.rs"
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


PART1_CATEGORIES = {
    "InvalidFixedLength": ("InvalidFixedLength", "invalid_fixed_length"),
    "Truncated": ("Truncated", "truncated"),
    "ExcessivePadding": ("ExcessivePadding", "excessive_padding"),
    "DeobfuscationFailure": ("DeobfuscationFailed", "deobfuscation_failed"),
    "AuthenticationFailure": ("AuthenticationFailed", "authentication_failed"),
    "TranscriptMismatch": ("TranscriptMismatch", "transcript_mismatch"),
    "InvalidKeyAgreement": ("KeyAgreementInvalid", "key_agreement_invalid"),
}

RESPONDER_IO_CODES = (
    "responder_session_created_write_closed",
    "responder_session_created_write_deadline",
    "responder_session_created_write_cancelled",
    "responder_session_created_write_io_failed",
    "responder_session_confirmed_read_closed",
    "responder_session_confirmed_read_deadline",
    "responder_session_confirmed_read_cancelled",
    "responder_session_confirmed_read_io_failed",
)
RESPONDER_IO_CLASSIFICATIONS = (
    ("SessionCreatedWrite", "Closed", "responder_session_created_write_closed"),
    ("SessionCreatedWrite", "Deadline", "responder_session_created_write_deadline"),
    ("SessionCreatedWrite", "Cancelled", "responder_session_created_write_cancelled"),
    ("SessionCreatedWrite", "Failed", "responder_session_created_write_io_failed"),
    ("SessionConfirmedRead", "Closed", "responder_session_confirmed_read_closed"),
    ("SessionConfirmedRead", "Deadline", "responder_session_confirmed_read_deadline"),
    ("SessionConfirmedRead", "Cancelled", "responder_session_confirmed_read_cancelled"),
    ("SessionConfirmedRead", "Failed", "responder_session_confirmed_read_io_failed"),
)


def source_findings(driver: str, build: str, cmake: str, observer: str, runner: str,
                    launcher: str | None = None, status: str | None = None) -> list[str]:
    launcher = LAUNCHER.read_text() if launcher is None else launcher
    status = STATUS.read_text() if status is None else status
    responder_allowlist = runner.partition("RESPONDER_REASON_CODES = frozenset({")[2].partition("})")[0]
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
        "scenario identities follow the I2NP sender and receiver roles": (
            "def scenario_identity_values(direction: str, i2pr_hash: str" in runner
            and "return i2pr_hash, i2pd_hash" in runner
            and "scenario_identity_values(" in runner
            and "scenario-router-identity-role-mismatch" in runner
            and "self-test-swapped-or-duplicate-identities-accepted" in runner
        ),
        "SessionConfirmed Part 1 protocol errors map to distinct fixed statuses": all(
            re.search(
                rf"HandshakeError::{error}\s*=>\s*\{{?\s*LauncherError::ResponderSessionConfirmedPart1"
                rf"{suffix[0]}", launcher
            ) is not None
            and f"StatusReason::ResponderSessionConfirmedPart1{suffix[0]}" in launcher
            and f'"responder_session_confirmed_part1_{suffix[1]}"' in status
            for error, suffix in PART1_CATEGORIES.items()
        ) and 'ResponderHandshakeIoOperation::SessionCreatedWrite' in launcher
        and 'ResponderHandshakeIoOperation::SessionConfirmedRead' in launcher
        and all(re.search(
            rf"\(\s*Some\(ResponderHandshakeIoOperation::{operation}\),\s*IoErrorKind::{kind},?\s*\)\s*=>\s*(?:\{{\s*)?LauncherError::Responder{''.join(word.title() for word in code.removeprefix('responder_').split('_'))}",
            launcher,
        ) for operation, kind, code in RESPONDER_IO_CLASSIFICATIONS)
        and all(f"StatusReason::Responder{''.join(word.title() for word in code.removeprefix('responder_').split('_'))}" in launcher
                for code in RESPONDER_IO_CODES)
        and all(f'"{code}"' in status for code in RESPONDER_IO_CODES),
        "evidence projection whitelists fixed responder reason codes": all(
            f'"responder_session_confirmed_part1_{suffix}"' in responder_allowlist
            for _, suffix in PART1_CATEGORIES.values()
        ) and all(f'"{code}"' in responder_allowlist for code in RESPONDER_IO_CODES)
        and "reason not in RESPONDER_REASON_CODES" in runner,
    }
    findings.extend(name for name, passed in checks.items() if not passed)
    return findings


def self_test() -> bool:
    driver = DRIVER.read_text()
    build = BUILD.read_text()
    cmake = CMAKE.read_text()
    observer = OBSERVER.read_text()
    runner = RUNNER.read_text()
    launcher = LAUNCHER.read_text()
    status = STATUS.read_text()
    if source_findings(driver, build, cmake, observer, runner, launcher, status):
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
        (driver, build, cmake, observer,
         runner.replace("return i2pr_hash, i2pd_hash", "return i2pd_hash, i2pr_hash")),
        (driver.replace('Logger().SetLogLevel("debug");', ""), build, cmake, observer, runner),
        (driver.replace('Logger().SetLogLevel("debug");\n    i2p::log::Logger().Start();',
                        'i2p::log::Logger().Start();\n    i2p::log::Logger().SetLogLevel("debug");'),
         build, cmake, observer, runner),
    ]
    normalized = [
        (*candidate, observer, runner, launcher, status) if len(candidate) == 3
        else (*candidate, launcher, status) if len(candidate) == 5
        else candidate
        for candidate in mutations
    ]
    normalized.extend([
        (driver, build, cmake, observer, runner,
         launcher.replace("LauncherError::ResponderSessionConfirmedPart1AuthenticationFailed",
                          "LauncherError::ResponderSessionConfirmedPart1Failed"), status),
        (driver, build, cmake, observer,
         runner.replace('    "responder_session_confirmed_part1_authentication_failed",\n', ''),
         launcher, status),
        (driver, build, cmake, observer, runner,
         launcher.replace("IoErrorKind::Deadline", "IoErrorKind::Cancelled", 1), status),
        (driver, build, cmake, observer, runner,
         launcher.replace("ResponderHandshakeIoOperation::SessionCreatedWrite", "ResponderHandshakeIoOperation::SessionConfirmedRead", 1), status),
        (driver, build, cmake, observer,
         runner.replace('"responder_session_created_write_closed",', '', 1), launcher, status),
    ])
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
                               OBSERVER.read_text(), RUNNER.read_text(),
                               LAUNCHER.read_text(), STATUS.read_text())
    if findings:
        for finding in findings:
            print(f"current-pin NTCP2 runner check failed: {finding}")
        return 1
    print("current-pin NTCP2 runner source checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
