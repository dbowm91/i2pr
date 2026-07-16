#!/usr/bin/env python3
"""Strict reader for the Plan 048 Multipass environment manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any


REQUIRED_KEYS = frozenset(
    {
        "schema",
        "instance_name",
        "image",
        "cpus",
        "memory",
        "disk",
        "launch_timeout_seconds",
        "guest_admin_user",
        "guest_execution_user",
        "guest_repo_root",
        "guest_cache_root",
        "guest_evidence_root",
        "required_architecture",
        "required_os_id",
        "required_os_version",
        "required_rust_toolchain",
        "required_topology_kind",
        "required_privilege_model",
    }
)

SCENARIO_REFERENCES = {
    "i2pr-to-java-ipv4": "java_i2p",
    "java-to-i2pr-ipv4": "java_i2p",
    "i2pr-to-i2pd-ipv4": "i2pd",
    "i2pd-to-i2pr-ipv4": "i2pd",
}

_SAFE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
_ABSOLUTE_POSIX = re.compile(r"^/[A-Za-z0-9._/~+-]+(?:/[A-Za-z0-9._~+-]+)*$")


class EnvironmentManifestError(ValueError):
    """The canonical environment manifest is malformed or unsafe."""


def manifest_path() -> Path:
    return Path(__file__).resolve().with_name("environment.toml")


def load_manifest(path: Path | None = None) -> dict[str, Any]:
    selected = path or manifest_path()
    try:
        values = tomllib.loads(selected.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as exc:
        raise EnvironmentManifestError("environment manifest is not valid TOML") from exc
    if set(values) != REQUIRED_KEYS:
        raise EnvironmentManifestError("environment manifest keys do not match schema")
    expected_types = {
        "schema": int,
        "instance_name": str,
        "image": str,
        "cpus": int,
        "memory": str,
        "disk": str,
        "launch_timeout_seconds": int,
        "guest_admin_user": str,
        "guest_execution_user": str,
        "guest_repo_root": str,
        "guest_cache_root": str,
        "guest_evidence_root": str,
        "required_architecture": str,
        "required_os_id": str,
        "required_os_version": str,
        "required_rust_toolchain": str,
        "required_topology_kind": str,
        "required_privilege_model": str,
    }
    for key, kind in expected_types.items():
        if type(values[key]) is not kind:
            raise EnvironmentManifestError(f"{key} has the wrong type")
    if values["schema"] != 1:
        raise EnvironmentManifestError("unsupported environment manifest schema")
    if not _SAFE_NAME.fullmatch(values["instance_name"]):
        raise EnvironmentManifestError("instance name is not a safe canonical name")
    if values["image"] != "24.04":
        raise EnvironmentManifestError("environment image must be Ubuntu 24.04")
    if values["cpus"] != 4 or values["memory"] != "8G" or values["disk"] != "40G":
        raise EnvironmentManifestError("resource profile drifted from Plan 048")
    if values["launch_timeout_seconds"] != 1800:
        raise EnvironmentManifestError("launch timeout drifted from Plan 048")
    if values["guest_admin_user"] != "ubuntu" or values["guest_execution_user"] != "i2ptest":
        raise EnvironmentManifestError("guest account policy drifted from Plan 048")
    for key in ("guest_repo_root", "guest_cache_root", "guest_evidence_root"):
        if not _ABSOLUTE_POSIX.fullmatch(values[key]) or ".." in values[key].split("/"):
            raise EnvironmentManifestError(f"{key} is not a confined absolute POSIX path")
    if values["required_architecture"] != "x86_64":
        raise EnvironmentManifestError("guest architecture must be x86_64")
    if values["required_os_id"] != "ubuntu" or values["required_os_version"] != "24.04":
        raise EnvironmentManifestError("guest OS contract drifted from Ubuntu 24.04")
    if values["required_rust_toolchain"] != "1.95.0":
        raise EnvironmentManifestError("Rust toolchain contract drifted")
    if values["required_topology_kind"] != "rootless-sealed-single-netns":
        raise EnvironmentManifestError("rootless topology contract drifted")
    if values["required_privilege_model"] != "unprivileged-userns":
        raise EnvironmentManifestError("rootless privilege contract drifted")
    return values


def manifest_sha256(path: Path | None = None) -> str:
    selected = path or manifest_path()
    return hashlib.sha256(selected.read_bytes()).hexdigest()


def scenario_reference(scenario: str) -> str:
    try:
        return SCENARIO_REFERENCES[scenario]
    except KeyError as exc:
        raise EnvironmentManifestError("unknown Plan 045 direction") from exc


def _main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=manifest_path())
    parser.add_argument("--get", choices=sorted(REQUIRED_KEYS))
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--sha256", action="store_true")
    args = parser.parse_args()
    try:
        values = load_manifest(args.manifest)
        outputs = sum(bool(value) for value in (args.get, args.json, args.sha256))
        if outputs != 1:
            parser.error("choose exactly one of --get, --json, or --sha256")
        if args.get:
            print(values[args.get])
        elif args.json:
            print(json.dumps(values, sort_keys=True, separators=(",", ":")))
        else:
            print(manifest_sha256(args.manifest))
    except (EnvironmentManifestError, OSError) as exc:
        print(f"environment manifest error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(_main())
