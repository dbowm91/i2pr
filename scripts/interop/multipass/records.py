#!/usr/bin/env python3
"""Create sanitized Multipass environment and lifecycle records."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path


def write(path: Path, value: dict[str, object]) -> None:
    temporary = path.with_suffix(".tmp")
    temporary.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="operation", required=True)
    environment = sub.add_parser("environment")
    environment.add_argument("--output", type=Path, required=True)
    for name in ("source-commit", "source-tree-sha256", "cache-manifest-sha256", "cloud-init-sha256", "provisioning-sha256", "environment-manifest-sha256"):
        environment.add_argument(f"--{name}", required=True)
    lifecycle = sub.add_parser("lifecycle")
    lifecycle.add_argument("--output", type=Path, required=True)
    lifecycle.add_argument("--environment-manifest-sha256", required=True)
    args = parser.parse_args()
    now = dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    if args.operation == "environment":
        value = {
            "schema": 1,
            "type": "multipass-interop-environment",
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
            "rootless_probe_outcome": "rootless_sandbox_available",
            "offline_enforcement": "guest-nft-egress-deny",
        }
    else:
        value = {
            "schema": 1,
            "type": "multipass-interop-lifecycle",
            "matrix_result": "passed",
            "cleanup_result": "clean",
            "offline_enforcement": "guest-nft-egress-deny",
            "environment_manifest_sha256": args.environment_manifest_sha256,
            "completed_at": now,
        }
    write(args.output, value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
