#!/usr/bin/env python3
"""Fail-closed Plan 308 validator for sanitized family captures."""

from __future__ import annotations

import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

PINNED = {
    "i2pd": "635b013a612ff47278ef02acf8580a28e10e26c5",
    "java_i2p": "9134f808337b401e8e53c73734c81fab04280c9d",
}
FAMILIES = ("i2pr", "i2pd", "java_i2p")
HEX_256 = re.compile(r"^[0-9a-f]{64}$")


def validate(repo_root: Path, evidence_root: Path) -> list[str]:
    errors: list[str] = []
    lock = tomllib.loads((repo_root / "tests/integration/anonymity/references.lock.toml").read_text())
    corpus_bytes = (repo_root / "tests/integration/anonymity/http-corpus.toml").read_bytes()
    expected_names = [row["name"] for row in tomllib.loads(corpus_bytes.decode())["request"]]
    manifest_path = evidence_root / "manifest.json"
    if not manifest_path.is_file():
        return ["manifest-missing"]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return ["manifest-invalid-json"]
    if manifest.get("schema") != 1 or manifest.get("plan") != 308:
        errors.append("manifest-schema-mismatch")
    if manifest.get("source_head") != manifest.get("verified_source_head"):
        errors.append("source-head-mismatch")
    if manifest.get("corpus_sha256") != hashlib.sha256(corpus_bytes).hexdigest():
        errors.append("corpus-hash-mismatch")
    refs = manifest.get("references")
    if not isinstance(refs, dict):
        errors.append("reference-manifest-missing")
        refs = {}
    for name, pin in PINNED.items():
        locked = lock.get(name, {}).get("commit")
        if locked != pin or refs.get(name) != pin:
            errors.append(f"reference-pin-mismatch:{name}")
    families = manifest.get("families")
    if not isinstance(families, dict):
        return errors + ["family-set-missing"]
    for family in FAMILIES:
        facts = families.get(family)
        if not isinstance(facts, dict):
            errors.append(f"family-missing:{family}")
            continue
        if facts.get("cleanup_complete") is not True:
            errors.append(f"cleanup-incomplete:{family}")
        capture_path = evidence_root / f"{family}.json"
        if not capture_path.is_file():
            errors.append(f"capture-missing:{family}")
            continue
        try:
            capture = json.loads(capture_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            errors.append(f"capture-invalid-json:{family}")
            continue
        rows = capture.get("requests")
        if capture.get("schema") != 1 or not isinstance(rows, list):
            errors.append(f"capture-schema-mismatch:{family}")
            continue
        names = [row.get("name") for row in rows if isinstance(row, dict)]
        if names != expected_names:
            errors.append(f"corpus-incomplete:{family}")
        for row in rows:
            if not isinstance(row, dict):
                errors.append(f"capture-row-invalid:{family}")
                continue
            if not HEX_256.fullmatch(str(row.get("request_sha256", ""))):
                errors.append(f"request-digest-invalid:{family}")
            if not isinstance(row.get("body_len"), int) or row["body_len"] < 0:
                errors.append(f"body-length-invalid:{family}")
            serialized = json.dumps(row).lower()
            if "body" in row or ".b32.i2p" in serialized or "priv" in serialized:
                errors.append(f"unsanitized-capture:{family}")
    return errors


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: check-http-anonymity-evidence.py REPO_ROOT EVIDENCE_ROOT", file=sys.stderr)
        return 2
    try:
        errors = validate(Path(sys.argv[1]), Path(sys.argv[2]))
    except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError) as exc:
        print(f"P308-EVIDENCE-STOP invalid-input:{type(exc).__name__}", file=sys.stderr)
        return 2
    if errors:
        print("P308-EVIDENCE-STOP " + ";".join(errors), file=sys.stderr)
        return 1
    print("Plan 308 HTTP capture evidence passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
