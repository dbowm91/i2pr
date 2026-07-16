#!/usr/bin/env python3
"""Create and finalize sanitized Plan 049 Multipass records."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / "tests/integration/ntcp2"))
from lifecycle import instance_name_digest, write_json_atomic  # noqa: E402
from harness.evidence import validate_record  # noqa: E402


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path: Path, value: dict[str, object]) -> None:
    write_json_atomic(path, value)


def _now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def environment_record(args: argparse.Namespace) -> dict[str, object]:
    return {
        "schema": 1,
        "type": "multipass-interop-environment",
        "environment_id": args.environment_id,
        "run_id": args.run_id,
        "instance_generation": args.instance_generation,
        "instance_name_digest": args.instance_name_digest,
        "lifecycle_schema_version": args.lifecycle_schema_version,
        "ownership_record_sha256": args.ownership_record_sha256,
        "instance_image": "ubuntu-24.04",
        "architecture": "x86_64",
        "resource_profile": "4cpu-8g-40g",
        "cloud_init_sha256": args.cloud_init_sha256,
        "environment_manifest_sha256": args.environment_manifest_sha256,
        "provisioning_record_sha256": args.provisioning_sha256,
        "source_commit": args.source_commit,
        "source_tree_sha256": args.source_tree_sha256,
        "reference_cache_manifest_sha256": args.cache_manifest_sha256,
        "userns_clone": 1,
        "apparmor_restrict_unprivileged_userns": 0,
        "execution_user_privileged": False,
        "host_baseline_probe_outcome": args.host_baseline_probe_outcome,
        "guest_rootless_probe_outcome": args.guest_rootless_probe_outcome,
        "rootless_probe_outcome": args.guest_rootless_probe_outcome,
        "adoption_mode": args.adoption_mode,
        "offline_enforcement": "guest-nft-egress-deny",
    }


def annotate_direction(args: argparse.Namespace) -> None:
    value: dict[str, Any] = json.loads(args.path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("direction record is not an object")
    environment_hash = sha256(args.environment)
    additions = {
        "environment_id": args.environment_id,
        "run_id": args.run_id,
        "instance_generation": args.instance_generation,
        "environment_evidence_sha256": environment_hash,
        "instance_name_digest": args.instance_name_digest,
        "lifecycle_schema_version": args.lifecycle_schema_version,
        "ownership_record_sha256": args.ownership_record_sha256,
        "environment_manifest_sha256": args.environment_manifest_sha256,
        "cloud_init_sha256": args.cloud_init_sha256,
        "host_baseline_probe_outcome": args.host_baseline_probe_outcome,
        "guest_rootless_probe_outcome": args.guest_rootless_probe_outcome,
        "adoption_mode": args.adoption_mode,
    }
    value.update(additions)
    value["evidence_sha256"] = ""
    validate_record(value)
    unsigned = dict(value)
    value["evidence_sha256"] = hashlib.sha256(
        json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    validate_record(value)
    write(args.path, value)


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="operation", required=True)
    environment = sub.add_parser("environment")
    environment.add_argument("--output", type=Path, required=True)
    for name in (
        "environment-id", "run-id", "instance-generation", "instance-name-digest",
        "lifecycle-schema-version", "ownership-record-sha256", "source-commit",
        "source-tree-sha256", "cache-manifest-sha256", "cloud-init-sha256",
        "provisioning-sha256", "environment-manifest-sha256", "host-baseline-probe-outcome",
        "guest-rootless-probe-outcome", "adoption-mode",
    ):
        environment.add_argument(f"--{name}", required=True)
    lifecycle = sub.add_parser("lifecycle")
    lifecycle.add_argument("--output", type=Path, required=True)
    lifecycle.add_argument("--environment-manifest-sha256", required=True)
    direction = sub.add_parser("annotate-direction")
    direction.add_argument("--path", type=Path, required=True)
    direction.add_argument("--environment", type=Path, required=True)
    for name in (
        "environment-id", "run-id", "instance-generation", "instance-name-digest",
        "lifecycle-schema-version", "ownership-record-sha256", "environment-manifest-sha256",
        "cloud-init-sha256", "host-baseline-probe-outcome", "guest-rootless-probe-outcome",
        "adoption-mode",
    ):
        direction.add_argument(f"--{name}", required=True)
    args = parser.parse_args()
    if args.operation == "environment":
        write(args.output, environment_record(args))
    elif args.operation == "annotate-direction":
        annotate_direction(args)
    else:
        write(args.output, {
            "schema_version": 1,
            "environment_manifest_sha256": args.environment_manifest_sha256,
            "last_operation": "matrix-complete",
            "last_typed_outcome": "passed",
            "state": "exporting",
            "updated_at_utc": _now(),
        })
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
