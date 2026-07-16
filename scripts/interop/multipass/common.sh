#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "$script_dir/../../.." && pwd)
config_py="$script_dir/config.py"
lifecycle_py="$script_dir/lifecycle.py"
environment_manifest="$script_dir/environment.toml"
environment_id=$(python3 "$config_py" --manifest "$environment_manifest" --get environment_id)
legacy_instance_name=$(python3 "$config_py" --manifest "$environment_manifest" --get instance_name)
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
cloud_init_sha256=$(sha256sum "$script_dir/cloud-init.yaml" | awk '{print $1}')
state_root="$repo_root/target/interop/multipass/state"
run_id="${I2PR_MULTIPASS_RUN_ID:-legacy-plan048}"
instance_name="${I2PR_MULTIPASS_INSTANCE_NAME:-$legacy_instance_name}"
instance_generation="${I2PR_MULTIPASS_GENERATION:-1}"
instance_state_dir="$state_root/$run_id"
instance_lifecycle_path="$instance_state_dir/lifecycle.json"
instance_lock_dir="$state_root/.instance-locks"
instance_lock_path="$instance_lock_dir/$instance_name.lock"
host_evidence_root="$repo_root/target/interop/evidence/multipass"
host_target="$repo_root/target/interop"
export I2PR_MULTIPASS_ENVIRONMENT_ID="$environment_id"
export I2PR_MULTIPASS_ENVIRONMENT_MANIFEST_SHA256="$environment_manifest_sha256"

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

write_environment_blocker() {
  local outcome=$1
  local phase=$2
  local remediation=$3
  local guest_outcome=${4:-not-reached}
  local blocker_dir="$host_evidence_root/$run_id"
  install -d -m 0700 "$blocker_dir"
  python3 - "$blocker_dir/environment-blocker.json" "$run_id" "$environment_id" \
    "$instance_generation" "$phase" "$outcome" "$remediation" "$environment_manifest_sha256" \
    "$(sha256sum "$script_dir/cloud-init.yaml" | awk '{print $1}')" \
    "${host_baseline_probe_outcome:-not-run}" "$guest_outcome" <<'PY'
import json
import os
import sys
from pathlib import Path
path = Path(sys.argv[1])
value = {
    "schema": 1,
    "run_id": sys.argv[2],
    "environment_id": sys.argv[3],
    "instance_generation": int(sys.argv[4]),
    "phase": sys.argv[5],
    "outcome": sys.argv[6],
    "remediation_class": sys.argv[7],
    "host_baseline_probe_outcome": sys.argv[10],
    "guest_probe_outcome": sys.argv[11],
    "environment_manifest_sha256": sys.argv[8],
    "cloud_init_sha256": sys.argv[9],
}
temporary = path.with_name(f".{path.name}.tmp.{os.getpid()}")
temporary.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
os.chmod(temporary, 0o600)
os.replace(temporary, path)
PY
}

acquire_lifecycle_lock() {
  if [[ "${I2PR_MULTIPASS_LOCK_HELD:-0}" == 1 ]]; then
    return
  fi
  ensure_dirs
  exec {lifecycle_lock_fd}>"$instance_state_dir/.lifecycle.lock"
  if ! flock -n "$lifecycle_lock_fd"; then
    typed_blocker blocked_lifecycle_lock_held
    write_environment_blocker blocked_lifecycle_lock_held lifecycle-lock inspect-owned-instance
    exit 2
  fi
  install -d -m 0700 "$instance_lock_dir"
  exec {instance_lock_fd}>"$instance_lock_path"
  if ! flock -n "$instance_lock_fd"; then
    typed_blocker blocked_lifecycle_lock_held
    write_environment_blocker blocked_lifecycle_lock_held instance-lock inspect-owned-instance
    exit 2
  fi
  export I2PR_MULTIPASS_LOCK_HELD=1
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
  install -d -m 0700 "$state_root" "$instance_state_dir" "$host_evidence_root"
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
  [[ "$1" =~ ^[a-z0-9][a-z0-9-]{6,46}[a-z0-9]$ ]] || die "run-id is not safe: $1"
}

configure_context() {
  local selected_run_id=${1:-$run_id}
  local selected_instance_name=${2:-$instance_name}
  local selected_generation=${3:-$instance_generation}
  validate_run_id "$selected_run_id"
  [[ "$selected_instance_name" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,62}$ ]] || die "instance name is not safe: $selected_instance_name"
  [[ "$selected_generation" =~ ^[1-9][0-9]{0,2}$ ]] || die "instance generation is not safe: $selected_generation"
  run_id=$selected_run_id
  instance_name=$selected_instance_name
  instance_generation=$selected_generation
  instance_state_dir="$state_root/$run_id"
  instance_lifecycle_path="$instance_state_dir/lifecycle.json"
  instance_lock_path="$instance_lock_dir/$instance_name.lock"
}

instance_exists() {
  multipass info "$instance_name" --format json >/dev/null 2>&1
}

require_instance() {
  require_multipass
  instance_exists || { typed_blocker blocked_instance_missing; exit 2; }
}

require_owned_instance() {
  require_instance
  ensure_dirs
  if [[ ! -f "$instance_lifecycle_path" ]]; then
    write_environment_blocker blocked_instance_without_host_state ownership ownership-reconciliation-required
    typed_blocker blocked_instance_without_host_state
    exit 2
  fi
  lifecycle_json=$(cat "$instance_lifecycle_path")
  guest_contract=$(guest_root_exec cat /var/lib/i2pr-interop/environment.json 2>/dev/null) || {
    write_environment_blocker blocked_existing_instance_contract_mismatch ownership ownership-reconciliation-required
    typed_blocker blocked_existing_instance_contract_mismatch
    exit 2
  }
  guest_token_sha256=$(guest_root_exec sha256sum /var/lib/i2pr-interop/ownership-token 2>/dev/null | awk '{print $1}') || {
    write_environment_blocker blocked_ownership_token_mismatch ownership ownership-reconciliation-required
    typed_blocker blocked_ownership_token_mismatch
    exit 2
  }
  token_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/ownership-token 2>/dev/null) || token_metadata=unknown:unknown:000
  contract_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/environment.json 2>/dev/null) || contract_metadata=unknown:unknown:000
  ownership_outcome=$(python3 - "$lifecycle_json" "$guest_contract" "$guest_token_sha256" "$token_metadata" "$contract_metadata" <<'PY'
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
    ok, outcome = ownership_proof(
        record,
        contract,
        guest_token_sha256=sys.argv[3],
        token_owner=token_owner,
        token_mode=int(token_mode, 8),
        contract_owner=contract_owner,
        contract_mode=int(contract_mode, 8),
    )
except (ValueError, KeyError, IndexError, json.JSONDecodeError):
    ok, outcome = False, "blocked_ownership_token_mismatch"
if not ok:
    print(outcome)
    raise SystemExit(2)
print("ownership_verified")
PY
  ) || {
    write_environment_blocker "$ownership_outcome" ownership ownership-reconciliation-required
    typed_blocker "$ownership_outcome"
    exit 2
  }
  lifecycle_state=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])' <<<"$lifecycle_json")
  case "$lifecycle_state" in
    destroyed|destroying)
      write_environment_blocker blocked_lifecycle_state_not_operable ownership inspect-owned-instance
      typed_blocker blocked_lifecycle_state_not_operable
      exit 2
      ;;
  esac
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
