#!/usr/bin/env python3
"""Fail-closed structural checks for the Plan 382 package trust zone."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / "crates/i2pr-app-package"


def fail(message: str) -> None:
    raise SystemExit(f"check-managed-app-package-boundary: FAIL: {message}")


def source_text() -> str:
    files = sorted((CRATE / "src").glob("*.rs"))
    if not files:
        fail("package crate has no source")
    return "\n".join(path.read_text() for path in files)


def check_source(source: str) -> None:
    code = "\n".join(
        line for line in source.splitlines()
        if not line.lstrip().startswith(("//", "/*", "*", "*/"))
    )
    forbidden = {
        "process execution": r"(?:std::process::Command|Command\s*::\s*new|\.spawn\s*\()",
        "shell execution": r"(?:sh\s+-c|cmd\.exe|powershell)",
        "launch authority": r"LaunchAuthority",
        "router runtime": r"(?:i2pr_daemon|i2pr_runtime|tokio::|std::net::)",
        "network client": r"(?:reqwest|hyper::Client|TcpStream|UdpSocket)",
    }
    for name, pattern in forbidden.items():
        if re.search(pattern, code, re.IGNORECASE):
            fail(f"package source contains forbidden {name} capability")
    required = [
        "verify_strict", "I2PR-APP-PACKAGE-V1", "CompressionMethod::Stored",
        "MAX_ARCHIVE_BYTES", "MAX_PAYLOAD_FILES", "MAX_TOTAL_PAYLOAD_BYTES",
        "create_new(true)", "by_index(", "fs::rename", "cleanup_staging",
    ]
    for token in required:
        if token not in code:
            fail(f"package implementation no longer contains required control {token!r}")


def check_manifest() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    packages = {item["name"]: item for item in json.loads(result.stdout)["packages"]}
    package = packages.get("i2pr-app-package")
    if package is None:
        fail("i2pr-app-package is not a workspace member")
    workspace_deps = {
        dep["name"] for dep in package["dependencies"]
        if dep["name"].startswith("i2pr-") and dep["kind"] in (None, "normal")
    }
    if workspace_deps != {"i2pr-app-proto"}:
        fail(f"unexpected workspace dependencies: {sorted(workspace_deps)}")


def self_test(source: str) -> None:
    mutations = [
        ("std::process::Command::new(\"sh\")", "process execution"),
        ("LaunchAuthority::new()", "launch authority"),
        ("tokio::spawn(task)", "router runtime"),
    ]
    for injected, expected in mutations:
        try:
            check_source(source + "\n" + injected)
        except SystemExit as error:
            if expected not in str(error):
                fail(f"negative control failed for {expected}: {error}")
        else:
            fail(f"negative control did not catch {expected}")
    print("check-managed-app-package-boundary: self-test ok (3 mutations)")


def main() -> None:
    source = source_text()
    check_source(source)
    check_manifest()
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test(source)
    elif len(sys.argv) != 1:
        fail("usage: check-managed-app-package-boundary.py [--self-test]")
    else:
        print("check-managed-app-package-boundary: ok")


if __name__ == "__main__":
    main()
