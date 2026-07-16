#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

json_only=0
while (($#)); do
  case "$1" in
    --json) [[ "$json_only" == 0 ]] || die "duplicate --json"; json_only=1 ;;
    --help|-h) printf 'usage: status.sh [--json]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
require_instance

info=$(multipass info "$instance_name" --format json)
guest_value() {
  guest_admin_exec "$@" 2>/dev/null || true
}
guest_user_value() {
  guest_exec "$@" 2>/dev/null
}
os_release=$(guest_value cat /etc/os-release)
architecture=$(guest_value uname -m)
userns_clone=$(guest_value sysctl -n kernel.unprivileged_userns_clone)
apparmor_restrict=$(guest_value sysctl -n kernel.apparmor_restrict_unprivileged_userns)
apparmor_enabled=$(guest_value cat /sys/module/apparmor/parameters/enabled)
groups=$(guest_value id -nG "$guest_execution_user")
uid=$(guest_user_value id -u || true)
cap_status=$(guest_user_value cat /proc/self/status || true)
sudo_allowed=0
if guest_exec sudo -n -l >/dev/null 2>&1; then sudo_allowed=1; fi
source_manifest=$(guest_user_value cat "$guest_repo_root/.i2pr-source-manifest.json" || true)
probe=$(guest_user_value cat "$guest_evidence_root/environment-probe/probe.json" || true)
cache_manifest=$(guest_user_value sha256sum "$guest_repo_root/target/interop/build/reference-cache-manifest.json" || true)
router_processes=$(guest_admin_exec ps -eo comm= 2>/dev/null | awk '$1 ~ /^(i2pr|i2pr-interop|i2pd|java|ref-gen|ref)$/ {print $1}' || true)
secret_run_state=$(guest_admin_exec find "$guest_repo_root/target/interop/runs" -mindepth 1 -print -quit 2>/dev/null || true)
cache_verified=0
if guest_user_value python3 "$guest_repo_root/scripts/interop/cache-manifest.py" --verify >/dev/null 2>&1; then cache_verified=1; fi
lifecycle_json=$(cat "$instance_lifecycle_path" 2>/dev/null || true)
guest_contract=$(guest_root_exec cat /var/lib/i2pr-interop/environment.json 2>/dev/null || true)
guest_token_sha256=$(guest_root_exec sha256sum /var/lib/i2pr-interop/ownership-token 2>/dev/null | awk '{print $1}' || true)
token_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/ownership-token 2>/dev/null || true)
contract_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/environment.json 2>/dev/null || true)
ownership_verified=0
if ownership_verified=$(python3 - "$lifecycle_json" "$guest_contract" "$guest_token_sha256" "$token_metadata" "$contract_metadata" <<'PY'
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import ownership_proof
try:
    record = json.loads(sys.argv[1])
    contract = json.loads(sys.argv[2])
    token_owner, token_mode = sys.argv[4].rsplit(":", 1)
    contract_owner, contract_mode = sys.argv[5].rsplit(":", 1)
    ok, _ = ownership_proof(
        record,
        contract,
        guest_token_sha256=sys.argv[3],
        token_owner=token_owner,
        token_mode=int(token_mode, 8),
        contract_owner=contract_owner,
        contract_mode=int(contract_mode, 8),
    )
except (ValueError, KeyError, IndexError, json.JSONDecodeError):
    ok = False
print("1" if ok else "0")
PY
); then :; else ownership_verified=0; fi

python3 - "$instance_name" "$info" "$os_release" "$architecture" "$userns_clone" "$apparmor_restrict" "$apparmor_enabled" "$groups" "$uid" "$cap_status" "$sudo_allowed" "$source_manifest" "$cache_verified" "$probe" "$instance_lifecycle_path" "$cache_manifest" "$router_processes" "$secret_run_state" "$ownership_verified" <<'PY'
import json
import sys

instance_name, info, os_release, architecture, userns, apparmor, apparmor_enabled, groups, uid, caps, sudo_allowed, source, cache, probe, lifecycle_path, cache_manifest, router_processes, secret_run_state, ownership_verified = sys.argv[1:]
try:
    parsed = json.loads(info)
    info_value = parsed.get("info", {}) if isinstance(parsed, dict) else {}
    if isinstance(info_value, dict):
        entry = info_value.get(instance_name, {})
    elif isinstance(info_value, list):
        entry = info_value[0] if info_value else {}
    else:
        entry = {}
    instance_state = entry.get("state", "unknown")
    mounts = entry.get("mounts", [])
    if isinstance(mounts, dict):
        mounts = list(mounts)
    unexpected_mounts = bool(mounts)
    snapshots = entry.get("snapshots", [])
    snapshot_names = {
        item.get("name") for item in snapshots
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }
    unexpected_snapshots = bool(snapshot_names - {"provisioned", "source-and-cache-ready"})
except (json.JSONDecodeError, AttributeError, TypeError):
    instance_state = "unknown"
    unexpected_mounts = True
    unexpected_snapshots = True
release = {}
for line in os_release.splitlines():
    if "=" in line:
        key, value = line.split("=", 1)
        release[key] = value.strip('"')
cap_fields = {}
for line in caps.splitlines():
    if line.startswith(("CapInh:", "CapPrm:", "CapEff:", "CapBnd:", "CapAmb:")):
        key, value = line.split(":", 1)
        cap_fields[key] = value.strip()
try:
    source_value = json.loads(source) if source else None
except json.JSONDecodeError:
    source_value = None
try:
    probe_value = json.loads(probe) if probe else None
except json.JSONDecodeError:
    probe_value = None
# CapBnd is the inherited bounding set and is normally non-zero for a
# non-root process. Plan 048 requires the effective, permitted, inheritable,
# and ambient sets to be empty; a non-zero bounding set alone is not a
# privilege grant.
caps_zero = all(cap_fields.get(key, "") in {"", "0000000000000000"} for key in ("CapInh", "CapPrm", "CapEff", "CapAmb"))
value = {
    "schema": 1,
    "type": "multipass-interop-status",
    "instance_name": instance_name,
    "instance_state": instance_state,
    "guest_os_id": release.get("ID", ""),
    "guest_os_version": release.get("VERSION_ID", ""),
    "guest_architecture": architecture.strip(),
    "sysctls": {
        "kernel.unprivileged_userns_clone": userns.strip(),
        "kernel.apparmor_restrict_unprivileged_userns": apparmor.strip(),
    },
    "apparmor_enabled": apparmor_enabled.strip() == "Y",
    "execution_user_groups": sorted(set(groups.split())),
    "execution_user_uid_is_non_root": uid.strip() not in {"", "0"},
    "execution_user_capabilities_zero": caps_zero,
    "execution_user_sudo_allowed": sudo_allowed == "1",
    "execution_user_privileged": sudo_allowed == "1" or uid.strip() in {"", "0"} or not caps_zero,
    "source_manifest": source_value,
    "cache_verified": cache == "1",
    "cache_manifest_sha256": (cache_manifest.split()[0] if cache_manifest.split() else None),
    "latest_probe_outcome": (probe_value or {}).get("outcome", "not-run"),
    "unexpected_mounts": unexpected_mounts,
    "unexpected_snapshots": unexpected_snapshots,
    "unexpected_router_process": bool(router_processes.strip()),
    "secret_run_state_present": bool(secret_run_state.strip()),
    "ownership_verified": ownership_verified == "1",
}
try:
    lifecycle = json.load(open(lifecycle_path, encoding="utf-8"))
except (OSError, json.JSONDecodeError):
    lifecycle = {}
value.update({
    "environment_id": lifecycle.get("environment_id", ""),
    "run_id": lifecycle.get("run_id", ""),
    "instance_generation": lifecycle.get("instance_generation", 0),
    "lifecycle_state": lifecycle.get("state", "unknown"),
})
print(json.dumps(value, sort_keys=True, separators=(",", ":")))
PY
