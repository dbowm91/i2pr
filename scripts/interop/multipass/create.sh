#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "$BASH_SOURCE")" && pwd)
source "$script_dir/common.sh"

requested_run_id=$(printenv I2PR_MULTIPASS_RUN_ID || true)
requested_instance_name=$(printenv I2PR_MULTIPASS_INSTANCE_NAME || true)
requested_generation=$(printenv I2PR_MULTIPASS_GENERATION || printf '1')
adopt_owned=0
while (($#)); do
  case "$1" in
    --run-id) [[ $# -ge 2 && ( -z "$requested_run_id" || "$requested_run_id" == "$2" ) ]] || die "duplicate or incomplete --run-id"; requested_run_id=$2; shift ;;
    --instance-name) [[ $# -ge 2 && ( -z "$requested_instance_name" || "$requested_instance_name" == "$2" ) ]] || die "duplicate or incomplete --instance-name"; requested_instance_name=$2; shift ;;
    --generation) [[ $# -ge 2 && ( "$requested_generation" == 1 || "$requested_generation" == "$2" ) ]] || die "duplicate or incomplete --generation"; requested_generation=$2; shift ;;
    --adopt-owned) [[ "$adopt_owned" == 0 ]] || die "duplicate --adopt-owned"; adopt_owned=1 ;;
    --replace) die "--replace is removed; use --recreate-owned after ownership proof" ;;
    --help|-h) printf 'usage: create.sh [--run-id <safe-id>] [--instance-name <safe-name>] [--generation <n>] [--adopt-owned]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

require_command python3
require_command flock
require_command openssl
if [[ -z "$requested_run_id" ]]; then requested_run_id=$(python3 "$lifecycle_py" generate-run-id); fi
validate_run_id "$requested_run_id"
if [[ -z "$requested_instance_name" ]]; then
  requested_instance_name=$(python3 "$lifecycle_py" derive-instance --run-id "$requested_run_id" --generation "$requested_generation")
else
  python3 "$lifecycle_py" validate-instance-name "$requested_instance_name" >/dev/null
fi
configure_context "$requested_run_id" "$requested_instance_name" "$requested_generation"
ensure_dirs
acquire_lifecycle_lock
if ! command -v multipass >/dev/null 2>&1; then
  write_environment_blocker blocked_multipass_missing preparation choose-new-run-id
  typed_blocker blocked_multipass_missing
  exit 2
fi
if ! multipass version >/dev/null 2>&1; then
  write_environment_blocker blocked_multipass_daemon_unavailable preparation inspect-owned-instance
  typed_blocker blocked_multipass_daemon_unavailable
  exit 2
fi

check_ownership() {
  python3 - "$1" "$2" "$3" "$4" "$5" <<'PY'
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import ownership_proof
record = json.loads(sys.argv[1]); contract = json.loads(sys.argv[2])
token_owner, token_mode = sys.argv[4].rsplit(":", 1)
contract_owner, contract_mode = sys.argv[5].rsplit(":", 1)
ok, outcome = ownership_proof(record, contract, guest_token_sha256=sys.argv[3],
                              token_owner=token_owner, token_mode=int(token_mode, 8),
                              contract_owner=contract_owner, contract_mode=int(contract_mode, 8))
if not ok:
    print(outcome)
    raise SystemExit(2)
PY
}

validate_guest_contract() {
  local status_json=$1
  local lifecycle_json=$2
  python3 - "$status_json" "$lifecycle_json" <<'PY'
import json
import sys

status = json.loads(sys.argv[1])
record = json.loads(sys.argv[2])
if status.get("ownership_verified") is not True:
    print("blocked_ownership_token_mismatch")
    raise SystemExit(2)
if str(status.get("instance_state", "")).lower() != "running":
    print("blocked_existing_instance_state_ambiguous")
    raise SystemExit(2)
expected_sysctls = {
    "kernel.unprivileged_userns_clone": "1",
    "kernel.apparmor_restrict_unprivileged_userns": "0",
}
if status.get("sysctls") != expected_sysctls:
    print("blocked_guest_policy_mismatch")
    raise SystemExit(2)
if status.get("execution_user_uid_is_non_root") is not True or \
   status.get("execution_user_capabilities_zero") is not True or \
   status.get("execution_user_sudo_allowed") is not False or \
   status.get("execution_user_privileged") is not False:
    print("blocked_guest_execution_user_contract")
    raise SystemExit(2)
if status.get("unexpected_mounts") is not False:
    print("blocked_guest_mount_contract")
    raise SystemExit(2)
if status.get("unexpected_snapshots") is not False:
    print("blocked_guest_snapshot_contract")
    raise SystemExit(2)
if status.get("unexpected_router_process") is not False:
    print("blocked_guest_router_process_present")
    raise SystemExit(2)
if status.get("secret_run_state_present") is not False:
    print("blocked_guest_secret_state_present")
    raise SystemExit(2)
state = record.get("state")
source_states = {"source_ready", "cache_ready", "source_and_cache_ready", "probe_passed", "offline_ready", "running", "exporting", "exported"}
source = status.get("source_manifest")
if state in source_states and (not isinstance(source, dict) or source.get("commit") != record.get("source_commit")):
    print("blocked_source_contract_mismatch")
    raise SystemExit(2)
cache_states = {"cache_ready", "source_and_cache_ready", "probe_passed", "offline_ready", "running", "exporting", "exported"}
if state in cache_states and (
    status.get("cache_verified") is not True or
    status.get("cache_manifest_sha256") != record.get("reference_cache_manifest_sha256")
):
    print("blocked_guest_cache_contract")
    raise SystemExit(2)
print("guest_contract_verified")
PY
}

if instance_exists; then
  if [[ ! -f "$instance_lifecycle_path" ]]; then
    write_environment_blocker blocked_instance_without_host_state collision ownership-reconciliation-required
    typed_blocker blocked_instance_without_host_state
    exit 2
  fi
  if [[ "$adopt_owned" != 1 ]]; then
    write_environment_blocker blocked_instance_name_owned_by_other_workflow collision inspect-owned-instance
    typed_blocker blocked_instance_name_owned_by_other_workflow
    exit 2
  fi
  guest_contract=$(guest_root_exec cat /var/lib/i2pr-interop/environment.json 2>/dev/null) || {
    write_environment_blocker blocked_existing_instance_contract_mismatch adoption ownership-reconciliation-required
    typed_blocker blocked_existing_instance_contract_mismatch
    exit 2
  }
  guest_token_sha256=$(guest_root_exec sha256sum /var/lib/i2pr-interop/ownership-token 2>/dev/null | awk '{print $1}') || {
    write_environment_blocker blocked_ownership_token_mismatch adoption ownership-reconciliation-required
    typed_blocker blocked_ownership_token_mismatch
    exit 2
  }
  token_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/ownership-token 2>/dev/null) || token_metadata=unknown:unknown:000
  contract_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/environment.json 2>/dev/null) || contract_metadata=unknown:unknown:000
  if ! outcome=$(check_ownership "$(cat "$instance_lifecycle_path")" "$guest_contract" "$guest_token_sha256" "$token_metadata" "$contract_metadata" 2>&1); then
    write_environment_blocker "$outcome" adoption ownership-reconciliation-required
    typed_blocker "$outcome"
    exit 2
  fi
  adopted_info=$(multipass info "$instance_name" --format json 2>/dev/null || true)
  adopted_state=$(python3 - "$adopted_info" "$instance_name" <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import parse_multipass_info
try:
    print(parse_multipass_info(sys.argv[1], sys.argv[2])["state"])
except Exception:
    print("unknown")
PY
  )
  case "$adopted_state" in
    stopped|suspended|delayed-shutdown)
      if ! multipass start "$instance_name" >/dev/null 2>&1; then
        write_environment_blocker blocked_adopt_start_failed adoption inspect-owned-instance
        typed_blocker blocked_adopt_start_failed
        exit 2
      fi
      ;;
    running|starting|restarting) ;;
    *)
      write_environment_blocker blocked_existing_instance_state_ambiguous adoption inspect-owned-instance
      typed_blocker blocked_existing_instance_state_ambiguous
      exit 2
      ;;
  esac
  ready=0
  for attempt in $(seq 1 60); do
    runtime_info=$(multipass info "$instance_name" --format json 2>/dev/null || true)
    runtime_state=$(python3 - "$runtime_info" "$instance_name" <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import parse_multipass_info
try:
    print(parse_multipass_info(sys.argv[1], sys.argv[2])["state"])
except Exception:
    print("unknown")
PY
    )
    if [[ "$runtime_state" == running ]] && \
       guest_admin_exec test -f /var/lib/cloud/instance/boot-finished >/dev/null 2>&1 && \
       guest_admin_exec id -u "$guest_execution_user" >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 2
  done
  if [[ "$ready" != 1 ]]; then
    python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
      --operation adopt-owned --outcome blocked_guest_execution_user_contract >/dev/null || true
    write_environment_blocker blocked_guest_execution_user_contract adoption inspect-owned-instance
    typed_blocker blocked_guest_execution_user_contract
    exit 2
  fi
  status_json=$(bash "$script_dir/status.sh" --json 2>/dev/null) || {
    python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
      --operation adopt-owned --outcome blocked_guest_policy_mismatch >/dev/null || true
    write_environment_blocker blocked_guest_policy_mismatch adoption inspect-owned-instance
    typed_blocker blocked_guest_policy_mismatch
    exit 2
  }
  if ! adoption_outcome=$(validate_guest_contract "$status_json" "$(cat "$instance_lifecycle_path")" 2>&1); then
    python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
      --operation adopt-owned --outcome "$adoption_outcome" >/dev/null || true
    write_environment_blocker "$adoption_outcome" adoption ownership-reconciliation-required
    typed_blocker "$adoption_outcome"
    exit 2
  fi
  adopted_state=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])' <"$instance_lifecycle_path")
  case "$adopted_state" in
    provisioned|source_ready|cache_ready|source_and_cache_ready|probe_passed|offline_ready|running|exporting|exported) ;;
    *)
      adopted_state=provisioned
      ;;
  esac
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state "$adopted_state" --operation adopt-owned \
    --outcome ownership_verified --updates-json '{"adoption_mode":"adopted"}' >/dev/null
  printf '%s\n' '{"schema":1,"type":"multipass-lifecycle","outcome":"ownership_verified","adoption_mode":"adopted"}'
  exit 0
fi

source_commit=$(git -C "$repo_root" rev-parse HEAD)
validate_commit "$source_commit"
host_multipass_version=$(multipass version | head -n 1)
cloud_init_sha256=$(sha256sum "$script_dir/cloud-init.yaml" | awk '{print $1}')
token_file=$(mktemp)
contract_file=$(mktemp)
runtime_cloud_init=$(mktemp "$instance_state_dir/.cloud-init.XXXXXX")
cleanup_files() { rm -f "$token_file" "$contract_file" "$runtime_cloud_init"; }
trap cleanup_files EXIT
umask 077
openssl rand -hex 32 >"$token_file"
owner_token_sha256=$(sha256sum "$token_file" | awk '{print $1}')
contract_file_json=$(python3 - "$environment_id" "$run_id" "$instance_name" "$source_commit" "$environment_manifest_sha256" "$cloud_init_sha256" "$owner_token_sha256" <<'PY'
import json, sys
print(json.dumps({"schema_version": 1, "environment_id": sys.argv[1], "run_id": sys.argv[2],
                  "instance_name": sys.argv[3], "source_commit_expected": sys.argv[4],
                  "environment_manifest_sha256": sys.argv[5], "cloud_init_sha256": sys.argv[6],
                  "owner_token_sha256": sys.argv[7]}, sort_keys=True, separators=(",", ":")))
PY
)
printf '%s\n' "$contract_file_json" >"$contract_file"
python3 - "$script_dir/cloud-init.yaml" "$runtime_cloud_init" "$token_file" "$contract_file" <<'PY'
import base64
import sys
from pathlib import Path
base, output, token_path, contract_path = map(Path, sys.argv[1:])
token = token_path.read_text(encoding="ascii")
contract = base64.b64encode(contract_path.read_bytes()).decode("ascii")
marker = "\nruncmd:\n"
addition = ("\n  - path: /var/lib/i2pr-interop/ownership-token\n"
            "    owner: root:root\n    permissions: '0600'\n    content: |\n      " + token + "\n"
            "  - path: /var/lib/i2pr-interop/environment.json\n"
            "    owner: root:root\n    permissions: '0644'\n    encoding: b64\n    content: " + contract + "\n")
rendered = base.read_text(encoding="utf-8").replace(marker, addition + marker, 1)
output.write_text(rendered, encoding="utf-8")
output.chmod(0o644)
PY
python3 "$lifecycle_py" reserve --output "$instance_lifecycle_path" --environment-id "$environment_id" \
  --run-id "$run_id" --instance-name "$instance_name" --generation "$instance_generation" \
  --source-commit "$source_commit" --environment-manifest-sha256 "$environment_manifest_sha256" \
  --cloud-init-sha256 "$cloud_init_sha256" --owner-token-sha256 "$owner_token_sha256" \
  --host-multipass-version "$host_multipass_version" >/dev/null
python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state launching \
  --operation launch --outcome launch-started >/dev/null
if ! multipass launch "$image" --name "$instance_name" --cpus "$cpus" --memory "$memory" --disk "$disk" \
    --timeout "$launch_timeout" --cloud-init "$runtime_cloud_init" >/dev/null; then
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
    --operation launch --outcome blocked_launch_failed >/dev/null || true
  write_environment_blocker blocked_launch_failed launch retry-new-run
  typed_blocker blocked_launch_failed
  exit 2
fi
python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state provisioning \
  --operation launch --outcome launch-complete >/dev/null

deadline=$((SECONDS + launch_timeout))
ready=0
while ((SECONDS < deadline)); do
  info=$(multipass info "$instance_name" --format json 2>/dev/null || true)
  if python3 - "$info" "$instance_name" <<'PY'
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import parse_multipass_info
try:
    if parse_multipass_info(sys.argv[1], sys.argv[2])["state"] != "running":
        raise SystemExit(1)
    print(True)
except Exception:
    raise SystemExit(1)
PY
  then ready=1; break; fi
  sleep 2
done
if [[ "$ready" != 1 ]]; then
  write_environment_blocker blocked_launch_failed provisioning retry-new-run
  typed_blocker blocked_launch_failed
  exit 2
fi
if ! guest_admin_exec test -f /var/lib/i2pr-interop/provisioning.json >/dev/null 2>&1; then
  cloud_init_status_path="$instance_state_dir/cloud-init-status.json"
  if ! bash "$script_dir/cloud-init-status.sh" --output "$cloud_init_status_path" 2>/dev/null; then
    write_environment_blocker blocked_cloud_init_status_unparseable provisioning inspect-owned-instance
    typed_blocker blocked_cloud_init_status_unparseable
    exit 2
  fi
  classified=$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("failure_class","blocked_cloud_init_status_unparseable"))' <"$cloud_init_status_path" 2>/dev/null || printf 'blocked_cloud_init_status_unparseable')
  remediation=$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("recommended_action","operator-inspection-required"))' <"$cloud_init_status_path" 2>/dev/null || printf 'operator-inspection-required')
  write_environment_blocker "$classified" provisioning "$remediation" "$classified"
  typed_blocker "$classified"
  exit 2
fi
provisioning=$(guest_root_exec cat /var/lib/i2pr-interop/provisioning.json)
if ! printf '%s' "$provisioning" | python3 -c 'import json,sys; value=json.load(sys.stdin); expected={"kernel.unprivileged_userns_clone":1,"kernel.apparmor_restrict_unprivileged_userns":0};
if value.get("schema") != 1 or value.get("image_release") != "24.04" or value.get("architecture") != "x86_64" or value.get("effective_sysctls") != expected: raise SystemExit(1)'
then
  write_environment_blocker blocked_cloud_init_terminal_error provisioning inspect-owned-instance
  typed_blocker blocked_cloud_init_terminal_error
  exit 2
fi
cloud_init_status_path="$instance_state_dir/cloud-init-status.json"
if ! bash "$script_dir/cloud-init-status.sh" --output "$cloud_init_status_path" 2>/dev/null; then
  write_environment_blocker blocked_cloud_init_status_unparseable post-verify operator-inspection-required
  typed_blocker blocked_cloud_init_status_unparseable
  exit 2
fi
if ! python3 -c 'import json,sys; value=json.load(sys.stdin)
if value.get("cloud_init_state") not in {"done","running"}: raise SystemExit(1)' <"$cloud_init_status_path"; then
  classified=$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("failure_class","blocked_cloud_init_terminal_error"))' <"$cloud_init_status_path" 2>/dev/null || printf 'blocked_cloud_init_terminal_error')
  write_environment_blocker "$classified" post-verify operator-inspection-required
  typed_blocker "$classified"
  exit 2
fi
python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state provisioned \
  --operation provision --outcome provisioning-complete >/dev/null

guest_contract=$(guest_root_exec cat /var/lib/i2pr-interop/environment.json)
guest_token_sha256=$(guest_root_exec sha256sum /var/lib/i2pr-interop/ownership-token | awk '{print $1}')
token_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/ownership-token)
contract_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/environment.json)
if ! outcome=$(check_ownership "$(cat "$instance_lifecycle_path")" "$guest_contract" "$guest_token_sha256" "$token_metadata" "$contract_metadata" 2>&1); then
  write_environment_blocker "$outcome" ownership ownership-reconciliation-required
  typed_blocker "$outcome"
  exit 2
fi
if ! guest_admin_exec id -u "$guest_execution_user" >/dev/null 2>&1; then
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
    --operation provision --outcome blocked_guest_execution_user_contract >/dev/null || true
  write_environment_blocker blocked_guest_execution_user_contract provisioning inspect-owned-instance
  typed_blocker blocked_guest_execution_user_contract
  exit 2
fi
status_json=$(bash "$script_dir/status.sh" --json 2>/dev/null) || {
  write_environment_blocker blocked_guest_policy_mismatch ownership inspect-owned-instance
  typed_blocker blocked_guest_policy_mismatch
  exit 2
}
if ! guest_outcome=$(validate_guest_contract "$status_json" "$(cat "$instance_lifecycle_path")" 2>&1); then
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
    --operation ownership-verify --outcome "$guest_outcome" >/dev/null || true
  write_environment_blocker "$guest_outcome" ownership ownership-reconciliation-required
  typed_blocker "$guest_outcome"
  exit 2
fi
python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state provisioned \
  --operation ownership-verify --outcome ownership_verified >/dev/null
printf '%s\n' '{"schema":1,"type":"multipass-lifecycle","outcome":"ownership_verified","state":"provisioned"}'
