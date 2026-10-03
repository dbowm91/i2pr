#!/usr/bin/env bash
# Plan 304 — fail-closed host/source preflight for the anonymity capture lane.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
evidence_root=${I2PR_ANONYMITY_EVIDENCE_ROOT:-"$repo_root/target/interop/anonymity"}
if [[ "$evidence_root" != /* ]]; then evidence_root="$repo_root/$evidence_root"; fi
mkdir -p "$evidence_root"
chmod 0700 "$evidence_root"

die() { printf 'P304-PREFLIGHT-STOP %s\n' "$*" >&2; exit 1; }
[[ "$(uname -s)" == Linux ]] || die "host-is-not-linux"
[[ -r /etc/os-release ]] || die "os-release-unavailable"
source /etc/os-release
[[ "${ID:-}" == ubuntu ]] || die "host-is-not-ubuntu"
for cmd in python3 git curl make c++ java ant rustc cargo openssl find sort sha256sum timeout; do
  command -v "$cmd" >/dev/null 2>&1 || die "required-tool-missing:$cmd"
done

lock="$repo_root/tests/integration/anonymity/references.lock.toml"
[[ -f "$lock" ]] || die "reference-lock-missing"
actual_head=$(git -C "$repo_root" rev-parse HEAD)
git -C "$repo_root" diff --quiet --ignore-submodules HEAD -- || die "tracked-worktree-dirty"
python3 - "$repo_root" "$evidence_root" "$actual_head" <<'PY'
import hashlib
import json
import os
import platform
import resource
import re
import shutil
import socket
import subprocess
import sys
import tomllib
from pathlib import Path

root, out, head = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
lock_path = root / "tests/integration/anonymity/references.lock.toml"
lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
os_release = {}
for line in Path("/etc/os-release").read_text(encoding="utf-8").splitlines():
    if "=" in line:
        key, value = line.split("=", 1)
        os_release[key] = value.strip().strip('"')
expected = {
    "i2pd": "635b013a612ff47278ef02acf8580a28e10e26c5",
    "java_i2p": "9134f808337b401e8e53c73734c81fab04280c9d",
}
for name, revision in expected.items():
    value = lock.get(name)
    if not isinstance(value, dict) or value.get("commit") != revision:
        raise SystemExit(f"P304-PREFLIGHT-STOP reference-lock-mismatch:{name}")
    if not isinstance(value.get("repository"), str) or not value["repository"].startswith("https://"):
        raise SystemExit(f"P304-PREFLIGHT-STOP reference-lock-repository-invalid:{name}")

def first(*args):
    try:
        return subprocess.run(args, check=True, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT).stdout.splitlines()[0]
    except (OSError, subprocess.CalledProcessError, IndexError):
        return "unavailable"

boost_header = Path("/usr/include/boost/version.hpp")
boost_version = "unavailable"
if boost_header.is_file():
    match = re.search(r'^#define BOOST_LIB_VERSION "([^"]+)"',
                      boost_header.read_text(encoding="utf-8"), re.MULTILINE)
    if match:
        boost_version = match.group(1)

probe = socket.socket()
try:
    probe.bind(("127.0.0.1", 0))
finally:
    probe.close()
disk = shutil.disk_usage(root / "target" if (root / "target").exists() else root)
soft_fd, hard_fd = resource.getrlimit(resource.RLIMIT_NOFILE)
tools = {
    "rustc": first("rustc", "--version"), "cargo": first("cargo", "--version"),
    "java": first("java", "-version"), "ant": first("ant", "-version"),
    "cmake": first("cmake", "--version"), "make": first("make", "--version"),
    "ninja": first("ninja", "--version"), "openssl": first("openssl", "version"),
    "boost": boost_version,
    "python": first("python3", "--version"), "find": first("find", "--version"),
    "coreutils": first("ls", "--version"),
}
manifest = {
    "schema": 1, "plan": 304, "host": {
        "os_id": os_release.get("ID", "unknown"), "os_version": os_release.get("VERSION_ID", "unknown"),
        "architecture": platform.machine(), "kernel": platform.release(), "uname": platform.platform(),
        "loopback_bind": True, "free_disk_bytes": disk.free,
        "nofile_soft": soft_fd, "nofile_hard": hard_fd,
    }, "i2pr_commit": head, "lock_sha256": hashlib.sha256(lock_path.read_bytes()).hexdigest(),
    "references": expected, "tools": tools,
}
if disk.free < 4 * 1024**3:
    raise SystemExit("P304-PREFLIGHT-STOP less-than-4GiB-free-disk")
if soft_fd != resource.RLIM_INFINITY and soft_fd < 256:
    raise SystemExit("P304-PREFLIGHT-STOP file-descriptor-limit-below-256")
path = out / "host-manifest.json"
path.write_text(json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
path.chmod(0o600)
print(json.dumps(manifest, sort_keys=True, separators=(",", ":")))
PY
