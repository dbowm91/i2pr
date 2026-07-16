#!/usr/bin/env python3
"""Hash an immutable source tree and write its sanitized transfer manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
MANIFEST_NAME = ".i2pr-source-manifest.json"


def tree_hash(root: Path) -> str:
    if not root.is_dir():
        raise ValueError("source root is not a directory")
    entries: list[tuple[str, str, int]] = []
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative == MANIFEST_NAME or relative == ".git" or relative.startswith(".git/"):
            continue
        if relative == "target" or relative.startswith("target/"):
            continue
        if relative == ".agents" or relative.startswith(".agents/"):
            continue
        if relative == ".opencode/node_modules" or relative.startswith(".opencode/node_modules/"):
            continue
        if relative.endswith("/__pycache__") or relative.startswith("__pycache__/") or "/__pycache__/" in relative:
            continue
        if path.is_symlink():
            raise ValueError(f"source archive refuses symlink: {relative}")
        if path.is_file():
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            mode = path.stat().st_mode & 0o111
            entries.append((relative, digest, mode))
    canonical = "".join(f"{digest} {mode:o} {relative}\n" for relative, digest, mode in entries)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def write_manifest(root: Path, commit: str, archive_sha256: str, output: Path) -> dict[str, str | int]:
    if not HEX40.fullmatch(commit):
        raise ValueError("source commit is not a full object ID")
    if not HEX64.fullmatch(archive_sha256):
        raise ValueError("source archive hash is not SHA-256")
    value: dict[str, str | int] = {
        "schema": 1,
        "commit": commit,
        "archive_sha256": archive_sha256,
        "tree_sha256": tree_hash(root),
        "archive_format": "git-archive-tar-gzip",
    }
    output.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    output.chmod(0o600)
    return value


def verify_manifest(root: Path, manifest_path: Path) -> dict[str, str | int]:
    value = json.loads(manifest_path.read_text(encoding="utf-8"))
    if set(value) != {"schema", "commit", "archive_sha256", "tree_sha256", "archive_format"}:
        raise ValueError("source manifest shape mismatch")
    if value["schema"] != 1 or value["archive_format"] != "git-archive-tar-gzip":
        raise ValueError("unsupported source manifest")
    if not isinstance(value["commit"], str) or not HEX40.fullmatch(value["commit"]):
        raise ValueError("source manifest commit is invalid")
    if not isinstance(value["archive_sha256"], str) or not HEX64.fullmatch(value["archive_sha256"]):
        raise ValueError("source manifest archive hash is invalid")
    if not isinstance(value["tree_sha256"], str) or not HEX64.fullmatch(value["tree_sha256"]):
        raise ValueError("source manifest tree hash is invalid")
    if tree_hash(root) != value["tree_sha256"]:
        raise ValueError("source tree hash mismatch")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--commit", default="")
    parser.add_argument("--archive-sha256", default="")
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    output = args.output or args.root / MANIFEST_NAME
    if args.verify:
        verify_manifest(args.root, output)
    else:
        if not args.commit or not args.archive_sha256:
            parser.error("--commit and --archive-sha256 are required when writing a manifest")
        write_manifest(args.root, args.commit, args.archive_sha256, output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
