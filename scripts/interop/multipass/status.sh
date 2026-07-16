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
cache_verified=0
if guest_user_value python3 "$guest_repo_root/scripts/interop/cache-manifest.py" --verify >/dev/null 2>&1; then cache_verified=1; fi

python3 - "$info" "$os_release" "$architecture" "$userns_clone" "$apparmor_restrict" "$apparmor_enabled" "$groups" "$uid" "$cap_status" "$sudo_allowed" "$source_manifest" "$cache_verified" "$probe" <<'PY'
import json
import sys

info, os_release, architecture, userns, apparmor, apparmor_enabled, groups, uid, caps, sudo_allowed, source, cache, probe = sys.argv[1:]
try:
    parsed = json.loads(info)
    info_value = parsed.get("info", {}) if isinstance(parsed, dict) else {}
    if isinstance(info_value, dict):
        entry = info_value.get("i2pr-interop-rootless", {})
    elif isinstance(info_value, list):
        entry = info_value[0] if info_value else {}
    else:
        entry = {}
    instance_state = entry.get("state", "unknown")
except (json.JSONDecodeError, AttributeError, TypeError):
    instance_state = "unknown"
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
    "instance_name": "i2pr-interop-rootless",
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
    "latest_probe_outcome": (probe_value or {}).get("outcome", "not-run"),
}
print(json.dumps(value, sort_keys=True, separators=(",", ":")))
PY
