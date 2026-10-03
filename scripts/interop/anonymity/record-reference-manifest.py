#!/usr/bin/env python3
"""Write a sanitized Plan 304 manifest from verified local caches."""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

I2PD_PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
JAVA_PIN = "9134f808337b401e8e53c73734c81fab04280c9d"


def read_kv(path: Path) -> dict[str, str]:
    values = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            values[key] = value
    return values


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: record-reference-manifest.py <repo-root> <evidence-root>", file=sys.stderr)
        return 64
    root, evidence = map(Path, sys.argv[1:])
    i2pd_cache = root / "target/interop/cache/ssu2/i2pd" / I2PD_PIN
    java_cache = root / "target/interop/cache/m6-java" / JAVA_PIN
    i2pd = read_kv(i2pd_cache / "build-sha256.txt")
    i2pd_revision = (i2pd_cache / "source-revision.txt").read_text(encoding="utf-8").strip()
    java = read_kv(java_cache / "build-metadata.txt")
    if i2pd_revision != I2PD_PIN or java.get("source_revision") != JAVA_PIN:
        raise SystemExit("P304-REFERENCE-STOP exact-reference-cache-pin-mismatch")
    i2pd_bin = i2pd_cache / "bin/i2pd"
    java_router = java_cache / "lib/router.jar"
    java_launcher = java_cache / java.get("launcher", "")
    for artifact in (i2pd_bin, java_router, java_launcher):
        if not artifact.is_file():
            raise SystemExit("P304-REFERENCE-STOP required-reference-artifact-missing")
    cache_root = root / "target/interop/cache"
    records = {
        "i2pd": {
            "version": "2.61.0", "commit": I2PD_PIN,
            "artifact": i2pd_bin.relative_to(cache_root).as_posix(),
            "artifact_sha256": sha256(i2pd_bin),
            "build_command": (i2pd_cache / "build-command.txt").read_text(encoding="utf-8").strip(),
        },
        "java_i2p": {
            "version": "2.13.0", "commit": JAVA_PIN,
            "artifact": java_router.relative_to(cache_root).as_posix(),
            "artifact_sha256": sha256(java_router),
            "launcher": java_launcher.relative_to(cache_root).as_posix(),
            "launcher_sha256": sha256(java_launcher),
            "cache_tree_sha256": java.get("installed_tree_sha256", ""),
            "build_command": java.get("build_command_version", ""),
        },
    }
    manifest = {"schema": 1, "plan": 304, "cache_root": "target/interop/cache", "references": records}
    evidence.mkdir(mode=0o700, parents=True, exist_ok=True)
    destination = evidence / "reference-manifest.json"
    destination.write_text(json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    destination.chmod(0o600)
    print(json.dumps(manifest, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
