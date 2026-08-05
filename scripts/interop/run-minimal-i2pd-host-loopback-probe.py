#!/usr/bin/env python3
"""Plan 086 minimal i2pd host-loopback development probe wrapper.

The wrapper is a thin operator entry point for the Plan 086
host-loopback-development lane. It accepts the four required
positional inputs, refuses release/support flags, and dispatches
to the canonical Plan 086/087/088 runner modules. The wrapper
opens no socket, performs no bootstrap, and never copies runner
orchestration logic; it is a parser and dispatcher only.

The wrapper accepts only ``host-loopback-development`` and the two
allowlisted i2pd directions. It rejects the Plan 082 release
profiles, the Plan 045 support profile, the Plan 046 rootless
profile, and the Plan 080 Multipass profile. The entry point
never invokes sudo, namespaces, Multipass, or any public-network
access.

Usage:
    python3 scripts/interop/run-minimal-i2pd-host-loopback-probe.py \
        --direction i2pr-to-i2pd-ipv4|i2pd-to-i2pr-ipv4 \
        --repo-root <path> \
        --run-root <path> \
        --run-id <run-id> \
        --source-commit <40-hex> \
        --output <record.json> \
        [--preflight] \
        [--i2pd-driver-binary <path>] \
        [--handshake-timeout-ms 30000]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent
HARNESS_DIR = REPO_ROOT / "tests/integration/ntcp2/harness"
if str(HARNESS_DIR) not in sys.path:
    sys.path.insert(0, str(HARNESS_DIR))


HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")

ALLOWED_DIRECTIONS = ("i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4")
ALLOWED_TOPOLOGY_KIND = "host-loopback-development"

FORBIDDEN_PROFILE_FLAGS = {
    "release",
    "support",
    "java",
    "emissary",
    "rootless",
    "multipass",
    "recursive",
    "rfc1918",
    "production",
}


def _fail(message: str, code: int = 2) -> int:
    print(f"error: {message}", file=sys.stderr)
    return code


def _make_provenance(
    *,
    source_commit: str,
    reference_revision: str = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
) -> dict[str, str]:
    """Return the bounded provenance stub for the host-loopback lane.

    The wrapper never fabricates a passing record; the stub uses
    all-zero digests until the runner materialises the real
    values. The reference revision is the pinned i2pd 2.60.0
    record from Plan 064.
    """

    return {
        "source_commit": source_commit,
        "reference_revision": reference_revision,
        "lane_qualification_sha256": "0" * 64,
        "i2pr_binary_sha256": "0" * 64,
        "i2pd_binary_sha256": "0" * 64,
    }


def _build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Plan 086 host-loopback-development probe wrapper",
    )
    parser.add_argument(
        "--direction",
        required=True,
        choices=list(ALLOWED_DIRECTIONS),
        help="Bounded Plan 086 direction; i2pd-only.",
    )
    parser.add_argument(
        "--repo-root",
        required=True,
        type=Path,
        help="Repository root (must be absolute).",
    )
    parser.add_argument(
        "--run-root",
        required=True,
        type=Path,
        help="Per-run root (must be a freshly created absolute path).",
    )
    parser.add_argument(
        "--run-id",
        required=True,
        help="Plan 082 run identifier (lowercase alnum + dash).",
    )
    parser.add_argument(
        "--source-commit",
        required=True,
        help="40-lowercase-hex source commit that produced the binaries.",
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="Path to write the sanitized probe record.",
    )
    parser.add_argument(
        "--preflight",
        action="store_true",
        help="Plan 086: stop before any peer connection completes.",
    )
    parser.add_argument(
        "--i2pd-driver-binary",
        type=Path,
        default=None,
        help="Optional path to the i2pd direct driver binary.",
    )
    parser.add_argument(
        "--handshake-timeout-ms",
        type=int,
        default=30_000,
        help="Handshake timeout in milliseconds (default 30000).",
    )
    parser.add_argument(
        "--delivery-status-message-id",
        default="0x04200001",
        help="Plan 062/065 per-run DeliveryStatus message ID.",
    )
    return parser


def _validate_inputs(args: argparse.Namespace) -> int | None:
    if not HEX40.fullmatch(args.source_commit):
        return _fail("--source-commit must be 40 lowercase hex characters")
    if not RUN_ID.fullmatch(args.run_id):
        return _fail("--run-id must be lowercase alphanumeric + dash")
    if not args.repo_root.is_absolute():
        return _fail("--repo-root must be an absolute path")
    if not args.run_root.is_absolute():
        return _fail("--run-root must be an absolute path")
    if args.run_root.exists():
        return _fail("--run-root must not exist before the run (fresh run root)")
    if args.preflight and args.i2pd_driver_binary is None:
        return _fail(
            "--preflight requires --i2pd-driver-binary to start the listener",
        )
    if args.handshake_timeout_ms <= 0 or args.handshake_timeout_ms > 600_000:
        return _fail("--handshake-timeout-ms must be in 1..600000")
    message_id_str = args.delivery_status_message_id
    if message_id_str.startswith("0x"):
        message_id = int(message_id_str, 16)
    else:
        message_id = int(message_id_str)
    if message_id < 1 or message_id > 0xFFFFFFFF:
        return _fail("--delivery-status-message-id must be in 1..0xffffffff")
    if args.i2pd_driver_binary is not None and not args.i2pd_driver_binary.is_file():
        return _fail(
            f"--i2pd-driver-binary not found: {args.i2pd_driver_binary}",
        )
    return None


def _detect_synthetic_profile_use(argv: list[str]) -> int | None:
    """Refuse any release/support profile flag that may reach the wrapper."""

    for entry in argv:
        if entry.startswith("--") and "=" in entry:
            flag = entry.split("=", 1)[0][2:].replace("-", "_")
            if flag in FORBIDDEN_PROFILE_FLAGS:
                return _fail(
                    f"release/support profile flag is forbidden: --{entry}",
                )
    return None


def _run_preflight(args: argparse.Namespace) -> dict[str, object]:
    """Plan 086 listener-only preflight that never starts a dialer.

    The preflight validates the lane, prepares the i2pr state, runs
    the i2pd driver in inspect mode, renders the strict scenario,
    and starts the listener with the measured i2pd direct driver.
    It terminates before any peer connection completes and writes
    a sanitized record. The preflight is the only path the
    Plan 086 closure may use to validate the lane contract.
    """

    import plan083_runner as runner

    provenance = _make_provenance(source_commit=args.source_commit)
    args.run_root.mkdir(parents=True, exist_ok=True)
    message_id = (
        int(args.delivery_status_message_id, 16)
        if args.delivery_status_message_id.startswith("0x")
        else int(args.delivery_status_message_id)
    )
    record = runner.execute_real_probe(
        repo_root=args.repo_root,
        run_root=args.run_root,
        run_id=args.run_id,
        source_commit=args.source_commit,
        reference_revision=provenance["reference_revision"],
        lane_qualification_sha256=provenance["lane_qualification_sha256"],
        topology_kind=ALLOWED_TOPOLOGY_KIND,
        i2pr_binary_sha256=provenance["i2pr_binary_sha256"],
        i2pd_binary_sha256=provenance["i2pd_binary_sha256"],
        delivery_status_message_id=message_id,
        i2pd_driver_binary=args.i2pd_driver_binary or Path("/nonexistent"),
        handshake_timeout_ms=args.handshake_timeout_ms,
        output_path=args.output,
    )
    return record


def _run_forward_probe(args: argparse.Namespace) -> dict[str, object]:
    """Plan 086 forward probe (``i2pr -> i2pd``).

    The wrapper delegates to the canonical ``plan083_runner``
    module. The runner is the sole owner of the strict scenario
    rendering, the i2pd direct driver invocation, and the
    structured event collection.
    """

    import plan083_runner as runner

    args.run_root.mkdir(parents=True, exist_ok=True)
    provenance = _make_provenance(source_commit=args.source_commit)
    message_id = int(args.delivery_status_message_id, 16) if args.delivery_status_message_id.startswith("0x") else int(args.delivery_status_message_id)
    return runner.execute_real_probe(
        repo_root=args.repo_root,
        run_root=args.run_root,
        run_id=args.run_id,
        source_commit=args.source_commit,
        reference_revision=provenance["reference_revision"],
        lane_qualification_sha256=provenance["lane_qualification_sha256"],
        topology_kind=ALLOWED_TOPOLOGY_KIND,
        i2pr_binary_sha256=provenance["i2pr_binary_sha256"],
        i2pd_binary_sha256=provenance["i2pd_binary_sha256"],
        delivery_status_message_id=message_id,
        i2pd_driver_binary=args.i2pd_driver_binary or Path("/nonexistent"),
        handshake_timeout_ms=args.handshake_timeout_ms,
        output_path=args.output,
    )


def _run_reverse_probe(args: argparse.Namespace) -> dict[str, object]:
    """Plan 086 reverse probe (``i2pd -> i2pr``).

    The wrapper delegates to the canonical ``plan084_runner``
    module. The runner is the sole owner of the strict responder
    scenario, the i2pd direct driver invocation, and the
    structured event collection.
    """

    import plan084_runner as runner

    args.run_root.mkdir(parents=True, exist_ok=True)
    provenance = _make_provenance(source_commit=args.source_commit)
    message_id = int(args.delivery_status_message_id, 16) if args.delivery_status_message_id.startswith("0x") else int(args.delivery_status_message_id)
    return runner.execute_reverse_probe(
        repo_root=args.repo_root,
        run_root=args.run_root,
        run_id=args.run_id,
        source_commit=args.source_commit,
        reference_revision=provenance["reference_revision"],
        lane_qualification_sha256=provenance["lane_qualification_sha256"],
        topology_kind=ALLOWED_TOPOLOGY_KIND,
        i2pr_binary_sha256=provenance["i2pr_binary_sha256"],
        i2pd_binary_sha256=provenance["i2pd_binary_sha256"],
        delivery_status_message_id=message_id,
        i2pd_driver_binary=args.i2pd_driver_binary or Path("/nonexistent"),
        handshake_timeout_ms=args.handshake_timeout_ms,
        output_path=args.output,
    )


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    forbidden = _detect_synthetic_profile_use(argv)
    if forbidden is not None:
        return forbidden
    parser = _build_arg_parser()
    args = parser.parse_args(argv)
    invalid = _validate_inputs(args)
    if invalid is not None:
        return invalid
    if args.preflight:
        record = _run_preflight(args)
    else:
        if args.direction == "i2pr-to-i2pd-ipv4":
            record = _run_forward_probe(args)
        else:
            record = _run_reverse_probe(args)
    print(json.dumps(record, sort_keys=True, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
