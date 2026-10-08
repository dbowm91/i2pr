#!/usr/bin/env python3
"""Static Plan 383 policy, CLI, and launch-authority boundary checks."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    raise SystemExit(f"check-managed-app-policy-boundary: FAIL: {message}")


def strip_comments(source: str) -> str:
    return "\n".join(line for line in source.splitlines() if not line.lstrip().startswith(("//", "/*", "*", "*/")))


def load_sources() -> dict[str, str]:
    result: dict[str, str] = {}
    for crate in ("i2pr-app-state", "i2pr-appctl", "i2pr-appd"):
        for path in sorted((ROOT / "crates" / crate / "src").rglob("*.rs")):
            result[path.relative_to(ROOT).as_posix()] = strip_comments(path.read_text())
    if not result:
        fail("managed-app implementation sources are missing")
    return result


def check(sources: dict[str, str]) -> None:
    state = "\n".join(value for path, value in sources.items() if path.startswith("crates/i2pr-app-state/"))
    ctl = "\n".join(value for path, value in sources.items() if path.startswith("crates/i2pr-appctl/"))
    appd = "\n".join(value for path, value in sources.items() if path.startswith("crates/i2pr-appd/"))
    appd_catalog = sources.get("crates/i2pr-appd/src/catalog.rs", "")
    violations = {
        "state runtime/network capability": r"(?:tokio::|std::net::|TcpStream|UdpSocket|reqwest|i2pr_daemon|i2pr_runtime)",
        "state launch authority": r"LaunchAuthority|AuthorityRequest",
        "CLI router/network capability": r"(?:i2pr_daemon|i2pr_runtime|tokio::|std::net::|reqwest|TcpStream|UdpSocket|i2pr_appd)",
        "state process execution": r"(?:std::process::Command|Command\s*::\s*new|\.spawn\s*\()",
        "CLI process execution": r"(?:std::process::Command|Command\s*::\s*new|\.spawn\s*\()",
    }
    for name, pattern in violations.items():
        haystack = state if name.startswith("state") else ctl
        if re.search(pattern, haystack):
            fail(f"{name} was introduced")

    required = [
        "schema_version", "generation", "trusted_publishers", "granted_capabilities",
        "file.try_lock()", "file.lock()", "MAX_GENERATIONS", "cleanup_policy_staging",
        "verify_installed", "SecuredUnavailable", "binary_search", "MAX_CONNECTIONS",
    ]
    for token in required:
        if token not in state:
            fail(f"persistent policy is missing required guard {token!r}")
    commands = [
        "Verify", "Install", "List", "Inspect", "Remove", "Trust", "Untrust",
        "Select", "Grant", "Revoke", "Profile", "Autostart",
    ]
    for command in commands:
        if not re.search(rf"\b{command}\b", ctl):
            fail(f"offline CLI command is missing: {command}")
    if "allow_direct_host_network" not in ctl or "UnsafeDirectAcknowledgementRequired" not in ctl:
        fail("UnsafeDirect acknowledgement is not an explicit CLI gate")
    untrust = re.search(r"PublisherCommand::Untrust\s*\{.*?\}\s*=>\s*\{(.*?)\n\s*\}", ctl, re.S)
    if not untrust or not all(token in untrust.group(1) for token in ("granted_capabilities.clear()", "launch_profile = None", "autostart = false")):
        fail("untrust does not clear latent launch-enabling policy")

    for token in ("PersistentLaunchCatalog", "I2PR_APP_STATE_ROOT", "lock_runtime", "target_triple", "try_fill_bytes", "from_trusted_local_policy", "LaunchAuthority::new"):
        if token not in appd:
            fail(f"production manager composition is missing {token!r}")
    if "EmptyCatalog" in sources.get("crates/i2pr-appd/src/main.rs", ""):
        fail("shipped manager still selects EmptyCatalog")
    if "LaunchAuthority::new" not in appd_catalog:
        fail("production authority construction is not localized to the appd catalog")

    metadata = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT, check=True, capture_output=True, text=True)
    packages = {package["name"]: package for package in json.loads(metadata.stdout)["packages"]}
    expected = {
        "i2pr-app-state": {"i2pr-app-package", "i2pr-app-proto"},
        "i2pr-appctl": {"i2pr-app-package", "i2pr-app-proto", "i2pr-app-state"},
    }
    for crate, allowed in expected.items():
        package = packages.get(crate)
        if package is None:
            fail(f"workspace package missing: {crate}")
        direct = {dep["name"] for dep in package["dependencies"] if dep["name"].startswith("i2pr-") and dep["kind"] in (None, "normal")}
        if direct != allowed:
            fail(f"{crate} workspace edges are {sorted(direct)}, expected {sorted(allowed)}")


def self_test(sources: dict[str, str]) -> None:
    mutations = [
        ("crates/i2pr-app-state/src/lib.rs", "use tokio::net::TcpStream;", "state runtime/network capability"),
        ("crates/i2pr-app-state/src/lib.rs", "LaunchAuthority::new", "state launch authority"),
        ("crates/i2pr-appctl/src/main.rs", "use i2pr_appd::Appd;", "CLI router/network capability"),
        ("crates/i2pr-appctl/src/main.rs", "std::process::Command::new(\"sh\")", "CLI process execution"),
    ]
    for path, addition, expected in mutations:
        mutated = dict(sources)
        mutated[path] = mutated.get(path, "") + "\n" + addition
        try:
            # Source-only controls are checked here; metadata remains unchanged.
            check_source_only(mutated)
        except SystemExit as error:
            if expected not in str(error):
                fail(f"negative control caught the wrong condition: {error}")
        else:
            fail(f"negative control missed {expected}")
    print("check-managed-app-policy-boundary: self-test ok (4 mutations)")


def check_source_only(sources: dict[str, str]) -> None:
    state = "\n".join(value for path, value in sources.items() if path.startswith("crates/i2pr-app-state/"))
    ctl = "\n".join(value for path, value in sources.items() if path.startswith("crates/i2pr-appctl/"))
    if re.search(r"(?:tokio::|std::net::|TcpStream|UdpSocket|reqwest|i2pr_daemon|i2pr_runtime)", state): fail("state runtime/network capability was introduced")
    if re.search(r"LaunchAuthority|AuthorityRequest", state): fail("state launch authority was introduced")
    if re.search(r"(?:i2pr_daemon|i2pr_runtime|tokio::|std::net::|reqwest|TcpStream|UdpSocket|i2pr_appd)", ctl): fail("CLI router/network capability was introduced")
    if re.search(r"(?:std::process::Command|Command\s*::\s*new|\.spawn\s*\()", ctl): fail("CLI process execution was introduced")


def main() -> None:
    sources = load_sources()
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        check(sources)
        self_test(sources)
    elif len(sys.argv) == 1:
        check(sources)
        print("check-managed-app-policy-boundary: ok")
    else:
        fail("usage: check-managed-app-policy-boundary.py [--self-test]")


if __name__ == "__main__":
    main()
