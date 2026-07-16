#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "$script_dir/../../.." && pwd)
config_py="$script_dir/config.py"
environment_manifest="$script_dir/environment.toml"
instance_name=$(python3 "$config_py" --manifest "$environment_manifest" --get instance_name)
image=$(python3 "$config_py" --manifest "$environment_manifest" --get image)
cpus=$(python3 "$config_py" --manifest "$environment_manifest" --get cpus)
memory=$(python3 "$config_py" --manifest "$environment_manifest" --get memory)
disk=$(python3 "$config_py" --manifest "$environment_manifest" --get disk)
launch_timeout=$(python3 "$config_py" --manifest "$environment_manifest" --get launch_timeout_seconds)
guest_admin_user=$(python3 "$config_py" --manifest "$environment_manifest" --get guest_admin_user)
guest_execution_user=$(python3 "$config_py" --manifest "$environment_manifest" --get guest_execution_user)
guest_repo_root=$(python3 "$config_py" --manifest "$environment_manifest" --get guest_repo_root)
guest_cache_root=$(python3 "$config_py" --manifest "$environment_manifest" --get guest_cache_root)
guest_evidence_root=$(python3 "$config_py" --manifest "$environment_manifest" --get guest_evidence_root)
environment_manifest_sha256=$(python3 "$config_py" --manifest "$environment_manifest" --sha256)
instance_state_dir="$repo_root/target/interop/multipass/$instance_name"
host_evidence_root="$repo_root/target/interop/evidence/multipass"
host_target="$repo_root/target/interop"

die() {
  printf 'multipass interop error: %s\n' "$*" >&2
  exit 1
}

typed_blocker() {
  local outcome=$1
  local detail=${2:-}
  python3 - "$outcome" "$detail" <<'PY'
import json
import sys
value = {"schema": 1, "type": "multipass-interop", "outcome": sys.argv[1]}
if sys.argv[2]:
    value["detail"] = sys.argv[2]
print(json.dumps(value, sort_keys=True, separators=(",", ":")))
PY
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command is missing: $1"
}

require_multipass() {
  if ! command -v multipass >/dev/null 2>&1; then
    typed_blocker blocked_multipass_missing
    exit 2
  fi
  if ! multipass version >/dev/null 2>&1; then
    typed_blocker blocked_multipass_daemon_unavailable
    exit 2
  fi
}

ensure_dirs() {
  install -d -m 0700 "$instance_state_dir" "$host_evidence_root"
}

validate_path() {
  local value=$1
  local label=$2
  [[ -n "$value" && "$value" != -* ]] || die "$label must be non-empty and must not start with '-': $value"
  [[ "$value" != *$'\n'* && "$value" != *$'\r'* ]] || die "$label contains a control character"
  case "/$value/" in
    */../*|*/./*) die "$label contains unsafe traversal: $value" ;;
  esac
}

validate_commit() {
  [[ "$1" =~ ^[0-9a-f]{40}$ ]] || die "commit must be a full lowercase SHA-1"
}

validate_run_id() {
  [[ "$1" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$ ]] || die "run-id is not safe: $1"
}

instance_exists() {
  multipass info "$instance_name" --format json >/dev/null 2>&1
}

require_instance() {
  require_multipass
  instance_exists || { typed_blocker blocked_instance_missing; exit 2; }
}

guest_admin_exec() {
  multipass exec "$instance_name" -- "$@"
}

guest_root_exec() {
  multipass exec "$instance_name" -- sudo -n -- "$@"
}

guest_exec() {
  multipass exec "$instance_name" -- sudo -n -iu "$guest_execution_user" -- "$@"
}

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

write_json() {
  local output=$1
  local value=$2
  mkdir -p "$(dirname "$output")"
  python3 - "$output" "$value" <<'PY'
import json
import os
import sys
path = sys.argv[1]
value = json.loads(sys.argv[2])
temporary = f"{path}.tmp.{os.getpid()}"
with open(temporary, "w", encoding="utf-8") as handle:
    json.dump(value, handle, sort_keys=True, separators=(",", ":"))
    handle.write("\n")
os.chmod(temporary, 0o600)
os.replace(temporary, path)
PY
}

scenario_reference() {
  case "$1" in
    i2pr-to-java-ipv4|java-to-i2pr-ipv4) printf 'java_i2p\n' ;;
    i2pr-to-i2pd-ipv4|i2pd-to-i2pr-ipv4) printf 'i2pd\n' ;;
    *) die "unknown scenario: $1" ;;
  esac
}
