#!/usr/bin/env python3
"""Validate static Plan 043 workflow and repository contracts."""

from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests/integration/ntcp2"))
from harness.build_gate import BuildGateError, PROFILE_GATES  # noqa: E402


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    workflow = (root / ".github/workflows/ntcp2-interop-ubuntu.yml").read_text(encoding="utf-8")
    lock = tomllib.loads((root / "tests/integration/ntcp2/references.lock.toml").read_text(encoding="utf-8"))
    try:
        revisions = {
            lock["reference"]["java_i2p"]["source_revision"],
            lock["reference"]["i2pd"]["source_revision"],
        }
        if any(not re.fullmatch(r"[0-9a-f]{40}", value) for value in revisions):
            raise BuildGateError("reference lock contains an abbreviated revision")
        if lock["host_contract"] != "ubuntu-24.04-amd64":
            raise BuildGateError("reference lock host contract drifted")
        if workflow.count("ubuntu-24.04") < 1 or "ubuntu-latest" in workflow:
            raise BuildGateError("workflow runner contract is not explicit Ubuntu 24.04")
        if "cancel-in-progress: false" not in workflow or "workflow_dispatch:" not in workflow:
            raise BuildGateError("workflow trigger or concurrency contract is missing")
        if "uses: actions/checkout@v4" not in workflow or "uses: actions/upload-artifact@v4" not in workflow:
            raise BuildGateError("workflow action versions are not fixed major releases")
        if re.search(r"^\s*uses:\s+[^\s]+@(master|main|latest)\s*$", workflow, re.MULTILINE):
            raise BuildGateError("workflow uses a moving action reference")
        for profile in PROFILE_GATES:
            if f"run-gate.sh --profile {profile}" not in workflow:
                raise BuildGateError(f"workflow does not expose gate profile: {profile}")
        matrix = (root / "scripts/interop/run-matrix.sh").read_text(encoding="utf-8")
        reference_block = matrix.split("reference-crosscheck-ipv4)", 1)[1].split(";;", 1)[0]
        if "java-ipv4-inbound-outbound" in reference_block or "i2pd-ipv4-inbound-outbound" in reference_block:
            raise BuildGateError("reference crosscheck aliases an i2pr scenario")
        offline_reuse = (root / "scripts/interop/offline-reuse.sh").read_text(encoding="utf-8")
        if "cargo +1.95.0 build --locked --package i2pr-interop" not in workflow + offline_reuse:
            raise BuildGateError("workflow does not build the current launcher checkout")
        tracked = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=False
        )
        if tracked.returncode != 0:
            raise BuildGateError("cannot inspect tracked files")
        forbidden_names = re.compile(
            r"(?:^|/)(?:router\.identity|ntcp2\.static\.key|.*\.pcap(?:ng)?|.*\.log|i2pd(?:\.exe)?|install\.jar)$",
            re.IGNORECASE,
        )
        if any(forbidden_names.search(path.decode("utf-8")) for path in tracked.stdout.split(b"\0") if path):
            raise BuildGateError("tracked generated or secret-bearing interop artifact exists")
    except (KeyError, OSError, UnicodeError, tomllib.TOMLDecodeError, BuildGateError) as exc:
        print(f"build contract error: {exc}", file=sys.stderr)
        return 1
    print("validated Plan 043 workflow, lock, profile, launcher, and tracked-artifact contracts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
