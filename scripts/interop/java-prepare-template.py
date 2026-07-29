#!/usr/bin/env python3
"""Plan 054 Java frozen-template preparation driver.

The authoritative execution phase may only ``clone`` and use a frozen
Java template; it must never download, install, or reseed from the
network. This script is the preparation-phase command that builds the
template from the pinned Java reference and records the immutable
template manifest and tree digest.

Usage:

```text
python3 scripts/interop/java-prepare-template.py \
    --reference-install <path> \
    --data-state fresh-unique-seed \
    --output-template <target-template-root>
```

The output root is **outside** every per-direction run root. The
preparation phase must run with the network available (apt, source
fetch); the execution phase must run with the network disabled and must
never invoke this script.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
HARNESS = REPO_ROOT / "tests" / "integration" / "ntcp2" / "harness"
sys.path.insert(0, str(HARNESS))

import java_startup_probe as _probe  # type: ignore  # noqa: E402
import java_matrix  # type: ignore  # noqa: E402
build_template_manifest = java_matrix.build_template_manifest
compute_template_tree_digest = java_matrix.compute_template_tree_digest


def _run_initial_start(
    reference_install: Path,
    data_dir: Path,
    launcher: str,
    timeout_seconds: float,
) -> int:
    completed = subprocess.run(
        [
            sys.executable,
            str(HARNESS / "java_startup_probe.py"),
            "--reference-install", str(reference_install),
            "--data-dir", str(data_dir),
            "--data-state", "empty",
            "--launcher", launcher,
            "--namespace", "outer",
            "--sequence", "single",
            "--attempts", "1",
            "--timeout-seconds", str(int(timeout_seconds)),
            "--output", str(data_dir / "java-prepare-probe.json"),
        ],
        cwd=HARNESS,
        capture_output=True, text=True, check=False,
    )
    return completed.returncode


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference-install", type=Path, required=True)
    parser.add_argument("--launcher", choices=sorted(_probe.ALLOWED_LAUNCHER), default="runplain")
    parser.add_argument("--data-state", choices=sorted(_probe.ALLOWED_DATA_STATE), default="empty")
    parser.add_argument("--output-template", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=float, default=60.0)
    parser.add_argument("--reference-version", default="2.12.0")
    parser.add_argument("--reference-revision", default="2800040deee9bb376567b671ef2e9c34cf3e30b6")
    args = parser.parse_args()
    output = args.output_template.resolve()
    if output.exists():
        print(f"template already exists at {output}", file=sys.stderr)
        return 2
    output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="i2pr-template-prep-") as scratch:
        scratch_path = Path(scratch) / "data"
        scratch_path.mkdir(parents=True, exist_ok=True)
        rc = _run_initial_start(args.reference_install, scratch_path, args.launcher, args.timeout_seconds)
        if rc != 0 and rc != 2:
            print(f"java initial start failed with rc={rc}", file=sys.stderr)
            return 3
        try:
            shutil.copytree(scratch_path, output)
        except OSError as exc:
            print(f"failed to freeze template: {exc}", file=sys.stderr)
            return 4
    manifest = build_template_manifest(
        output,
        reference_version=args.reference_version,
        reference_revision=args.reference_revision,
    )
    digest = compute_template_tree_digest(output)
    print(json.dumps({
        "schema": "i2pr-java-template-prepare-v1",
        "schema_version": 1,
        "template_root": str(output),
        "tree_sha256": digest,
        "manifest_sha256": manifest["tree_sha256"],
        "reference_version": args.reference_version,
        "reference_revision": args.reference_revision,
    }, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    import json
    raise SystemExit(main())
