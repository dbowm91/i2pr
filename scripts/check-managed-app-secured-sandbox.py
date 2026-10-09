#!/usr/bin/env python3
"""Fail-closed source guard for the Plan 407 Linux secured apphost backend."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def check(sandbox: str, host: str) -> list[str]:
    failures: list[str] = []
    requirements = {
        "static native ELF gate": "fn ensure_static_elf",
        "interpreter refusal": "== 3",
        "hard address-space limit": "Resource::As",
        "hard descriptor limit": "Resource::Nofile",
        "Landlock ABI v3": "ABI::V3",
        "Landlock hard requirement": "CompatLevel::HardRequirement",
        "Landlock enforced status": "RulesetStatus::FullyEnforced",
        "no-new-privileges enforced status": "status.no_new_privs",
        "seccomp default-kill filter": "SECCOMP_RET_KILL_PROCESS",
        "no child process syscalls": "libc::SYS_clone3",
        "explicit fork/clone denial controls": "libc::SYS_vfork",
        "read-only prlimit query filter": "new-limit pointer",
        "private app data without execute": "!AccessFs::Execute",
    }
    for label, token in requirements.items():
        if token not in sandbox:
            failures.append(label)
    secured = host.find("if request.launch_profile == i2pr_app_proto::LaunchProfile::Secured")
    ready = host.find("ApphostReply::Ready", secured)
    install = host.find("sandbox::install(", secured)
    execute = host.find("return exec_in_place(&launch)", secured)
    if secured < 0 or min(ready, install, execute) < 0 or not install < ready < execute:
        failures.append("setup, complete attestation, readiness, then in-place exec ordering")
    if "LaunchProfile::Secured" not in host or "I2PR_APP_DATA_DIR" not in host:
        failures.append("secured profile and manager-owned data root reach exec")
    return failures


def self_test(sandbox: str, host: str) -> None:
    mutations = [
        (sandbox.replace("Resource::As", "Resource::Cpu", 1), host, "memory limit"),
        (sandbox.replace("CompatLevel::HardRequirement", "CompatLevel::BestEffort", 1), host, "Landlock compatibility"),
        (sandbox.replace("== 3", "== 2", 1), host, "ELF interpreter gate"),
        (sandbox.replace("new-limit pointer", "mutable limit pointer", 1), host, "prlimit policy"),
        (sandbox.replace("status.no_new_privs", "true", 1), host, "no-new-privileges attestation"),
        (sandbox, host.replace("sandbox::install(", "sandbox::skip(", 1), "setup ordering"),
    ]
    for changed_sandbox, changed_host, name in mutations:
        if not check(changed_sandbox, changed_host):
            raise SystemExit(f"check-managed-app-secured-sandbox: self-test missed {name}")


def main() -> int:
    sandbox_path = ROOT / "crates/i2pr-apphost/src/sandbox.rs"
    host_path = ROOT / "crates/i2pr-apphost/src/lib.rs"
    sandbox, host = sandbox_path.read_text(), host_path.read_text()
    if "--self-test" in sys.argv[1:]:
        self_test(sandbox, host)
        print("check-managed-app-secured-sandbox: self-test passed (6 mutations)")
        return 0
    failures = check(sandbox, host)
    if failures:
        for failure in failures:
            print(f"check-managed-app-secured-sandbox: FAIL: {failure}", file=sys.stderr)
        return 1
    print("check-managed-app-secured-sandbox: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
