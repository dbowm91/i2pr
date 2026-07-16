#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "$BASH_SOURCE")" && pwd)
source "$script_dir/common.sh"

requested_run_id=$(printenv I2PR_MULTIPASS_RUN_ID || true)
requested_instance_name=$(printenv I2PR_MULTIPASS_INSTANCE_NAME || true)
generation=$(printenv I2PR_MULTIPASS_GENERATION || printf '1')
destroy_owned=0
recreate_owned=0
while (($#)); do
  case "$1" in
    --run-id) [[ $# -ge 2 && ( -z "$requested_run_id" || "$requested_run_id" == "$2" ) ]] || die "duplicate or incomplete --run-id"; requested_run_id=$2; shift ;;
    --instance-name) [[ $# -ge 2 && ( -z "$requested_instance_name" || "$requested_instance_name" == "$2" ) ]] || die "duplicate or incomplete --instance-name"; requested_instance_name=$2; shift ;;
    --destroy-owned) [[ "$destroy_owned" == 0 ]] || die "duplicate --destroy-owned"; destroy_owned=1 ;;
    --recreate-owned) [[ "$recreate_owned" == 0 ]] || die "duplicate --recreate-owned"; recreate_owned=1 ;;
    --discard-unexported|--purge) die "unsafe legacy destruction option is removed" ;;
    --help|-h) printf 'usage: destroy.sh --run-id <safe-id> --instance-name <safe-name> --destroy-owned [--recreate-owned]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[[ "$destroy_owned" == 1 ]] || die "--destroy-owned is required"
[[ -n "$requested_run_id" ]] || die "--run-id is required"
[[ -n "$requested_instance_name" ]] || requested_instance_name="$legacy_instance_name"
validate_run_id "$requested_run_id"
python3 "$lifecycle_py" validate-instance-name "$requested_instance_name" >/dev/null
configure_context "$requested_run_id" "$requested_instance_name" "$generation"
require_multipass
ensure_dirs
acquire_lifecycle_lock
[[ -f "$instance_lifecycle_path" ]] || {
  write_environment_blocker blocked_instance_without_host_state destroy ownership-reconciliation-required
  typed_blocker blocked_instance_without_host_state
  exit 2
}
if ! instance_exists; then
  list_json=$(multipass list --format json 2>/dev/null || true)
  deleted_state=$(python3 - "$list_json" "$instance_name" <<'PY'
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import parse_multipass_list
try:
    for entry in parse_multipass_list(sys.argv[1]):
        if entry["name"] == sys.argv[2]:
            print(entry["state"])
            break
except Exception:
    pass
PY
)
  if [[ "$deleted_state" == deleted-unpurged ]]; then
    write_environment_blocker blocked_deleted_instance_requires_purge destroy operator-purge-required
    typed_blocker blocked_deleted_instance_requires_purge
  else
    write_environment_blocker blocked_instance_missing destroy inspect-owned-instance
    typed_blocker blocked_instance_missing
  fi
  exit 2
fi

guest_contract=$(guest_root_exec cat /var/lib/i2pr-interop/environment.json 2>/dev/null) || {
  write_environment_blocker blocked_existing_instance_contract_mismatch destroy ownership-reconciliation-required
  typed_blocker blocked_existing_instance_contract_mismatch
  exit 2
}
guest_token_sha256=$(guest_root_exec sha256sum /var/lib/i2pr-interop/ownership-token 2>/dev/null | awk '{print $1}') || {
  write_environment_blocker blocked_ownership_token_mismatch destroy ownership-reconciliation-required
  typed_blocker blocked_ownership_token_mismatch
  exit 2
}
token_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/ownership-token 2>/dev/null)
contract_metadata=$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/environment.json 2>/dev/null)
if ! ownership_outcome=$(python3 - "$(cat "$instance_lifecycle_path")" "$guest_contract" "$guest_token_sha256" "$token_metadata" "$contract_metadata" <<'PY'
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import ownership_proof
record = json.loads(sys.argv[1]); contract = json.loads(sys.argv[2])
token_owner, token_mode = sys.argv[4].rsplit(":", 1)
contract_owner, contract_mode = sys.argv[5].rsplit(":", 1)
ok, outcome = ownership_proof(record, contract, guest_token_sha256=sys.argv[3], token_owner=token_owner,
                              token_mode=int(token_mode, 8), contract_owner=contract_owner,
                              contract_mode=int(contract_mode, 8))
print(outcome)
if not ok:
    raise SystemExit(2)
PY
); then
  write_environment_blocker "$ownership_outcome" destroy ownership-reconciliation-required
  typed_blocker "$ownership_outcome"
  exit 2
fi

if guest_root_exec test -f "$guest_evidence_root/aggregate.json" >/dev/null 2>&1 && \
   [[ ! -f "$host_evidence_root/$run_id/manifest.json" ]]; then
  write_environment_blocker blocked_unexported_evidence destroy export-before-destroy
  typed_blocker blocked_unexported_evidence
  exit 2
fi
python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state destroying \
  --operation destroy-owned --outcome destruction-started >/dev/null
if ! multipass stop "$instance_name" >/dev/null 2>&1; then
  info=$(multipass info "$instance_name" --format json 2>/dev/null || true)
  if [[ -n "$info" ]]; then
    typed_blocker blocked_destroy_failed
    exit 2
  fi
fi
multipass delete "$instance_name" >/dev/null || {
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
    --operation destroy-owned --outcome blocked_destroy_failed >/dev/null || true
  typed_blocker blocked_destroy_failed
  exit 2
}
if multipass list --format json 2>/dev/null | grep -Fq "$instance_name"; then
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state blocked \
    --operation destroy-owned --outcome blocked_deleted_instance_requires_purge >/dev/null || true
  write_environment_blocker blocked_deleted_instance_requires_purge destroy operator-purge-required
  typed_blocker blocked_deleted_instance_requires_purge
  exit 2
fi
python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state destroyed \
  --operation destroy-owned --outcome destroyed >/dev/null
printf '%s\n' '{"schema":1,"type":"multipass-destruction","outcome":"destroyed","instance_present_after":false,"purge_performed":false}'
if [[ "$recreate_owned" == 1 ]]; then
  printf '%s\n' '{"schema":1,"type":"multipass-lifecycle","outcome":"recreate-ready","next_operation":"create"}'
fi
