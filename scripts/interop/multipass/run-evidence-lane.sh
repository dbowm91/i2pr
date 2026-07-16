#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

operation=""
destroy_after_export=0
run_id=""
instance_name_arg=""
instance_name_provided=0
generation="1"
resume_owned=0
adopt_owned=0
recreate_owned=0
destroy_owned=0
keep_on_blocker=0
inspect=0
while (($#)); do
  case "$1" in
    --create|--prepare|--probe|--run|--export|--destroy|--all)
      [[ -z "$operation" ]] || die "exactly one operation is required"
      operation="${1#--}"
      ;;
    --destroy-after-export) [[ "$destroy_after_export" == 0 ]] || die "duplicate --destroy-after-export"; destroy_after_export=1 ;;
    --run-id) [[ -z "$run_id" && $# -ge 2 ]] || die "duplicate or incomplete --run-id"; run_id=$2; shift ;;
    --instance-name) [[ -z "$instance_name_arg" && $# -ge 2 ]] || die "duplicate or incomplete --instance-name"; instance_name_arg=$2; instance_name_provided=1; shift ;;
    --generation) [[ "$generation" == 1 && $# -ge 2 ]] || die "duplicate or incomplete --generation"; generation=$2; shift ;;
    --resume-owned) [[ "$resume_owned" == 0 ]] || die "duplicate --resume-owned"; resume_owned=1 ;;
    --adopt-owned) [[ "$adopt_owned" == 0 ]] || die "duplicate --adopt-owned"; adopt_owned=1 ;;
    --recreate-owned) [[ "$recreate_owned" == 0 ]] || die "duplicate --recreate-owned"; recreate_owned=1 ;;
    --destroy-owned) [[ "$destroy_owned" == 0 ]] || die "duplicate --destroy-owned"; destroy_owned=1 ;;
    --keep-on-blocker) [[ "$keep_on_blocker" == 0 ]] || die "duplicate --keep-on-blocker"; keep_on_blocker=1 ;;
    --inspect) [[ "$inspect" == 0 && -z "$operation" ]] || die "--inspect must be the only operation"; inspect=1 ;;
    --help|-h) printf 'usage: run-evidence-lane.sh (--create|--prepare|--probe|--run|--export|--destroy|--all|--inspect) [--run-id <id>] [--instance-name <name>] [--resume-owned] [--adopt-owned] [--recreate-owned] [--destroy-owned] [--destroy-after-export] [--keep-on-blocker]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[[ -n "$operation" || "$inspect" == 1 ]] || die "one operation is required"
if [[ -z "$run_id" && ( "$operation" == all || "$operation" == create ) ]]; then
  run_id=$(python3 "$lifecycle_py" generate-run-id)
fi
if [[ -z "$run_id" ]]; then run_id="legacy-plan048"; fi
if [[ -n "$run_id" ]]; then validate_run_id "$run_id"; fi
if [[ -z "$instance_name_arg" && "$run_id" != legacy-plan048 ]]; then
  instance_name_arg=$(python3 "$lifecycle_py" derive-instance --run-id "$run_id" --generation "$generation")
fi
if [[ -n "$instance_name_arg" ]]; then
  python3 "$lifecycle_py" validate-instance-name "$instance_name_arg" >/dev/null
else
  instance_name_arg="$legacy_instance_name"
fi
configure_context "$run_id" "$instance_name_arg" "$generation"
if [[ "$destroy_after_export" == 1 && "$operation" != all ]]; then
  die "--destroy-after-export is valid only with --all"
fi
if [[ "$inspect" == 1 ]]; then
  run_id="${run_id:-legacy-plan048}"
  configure_context "$run_id" "$instance_name_arg" "$generation"
  if [[ ! -f "$instance_lifecycle_path" ]]; then
    printf '%s\n' '{"schema":1,"type":"multipass-inspection","outcome":"blocked_host_state_without_instance","ownership_verified":false,"recommended_next_operation":"choose-new-run-id"}'
    exit 2
  fi
  instance_name_arg=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["instance_name"])' <"$instance_lifecycle_path")
  generation=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["instance_generation"])' <"$instance_lifecycle_path")
  configure_context "$run_id" "$instance_name_arg" "$generation"
  list_json=''
  status_json=''
  if command -v multipass >/dev/null 2>&1 && multipass version >/dev/null 2>&1; then
    list_json=$(multipass list --format json 2>/dev/null || true)
    status_json=$(bash "$script_dir/status.sh" --json 2>/dev/null || true)
  fi
  offline_status=not-reached
  if [[ -n "$status_json" ]] && guest_root_exec nft list table inet i2pr_interop_offline >/dev/null 2>&1; then
    offline_status=verified
  fi
  python3 - "$instance_lifecycle_path" "$list_json" "$status_json" "$offline_status" <<'PY'
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import parse_multipass_list

value = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
try:
    entries = parse_multipass_list(sys.argv[2])
except Exception:
    entries = []
entry = next((item for item in entries if item["name"] == value.get("instance_name")), None)
status = {}
try:
    status = json.loads(sys.argv[3]) if sys.argv[3] else {}
except json.JSONDecodeError:
    status = {}
state = value.get("state", "unknown")
normalized = entry["state"] if entry else "absent"
if state == "destroyed" or normalized == "absent":
    recommendation = "choose-new-run-id" if state == "destroyed" else "resume-owned"
elif state == "exported":
    recommendation = "destroy-owned"
elif state == "blocked":
    recommendation = "recreate-owned"
else:
    recommendation = "resume-owned"
allowed = {
    "schema_version", "environment_id", "run_id", "instance_name", "instance_generation", "state",
    "source_commit", "reference_cache_manifest_sha256", "environment_manifest_sha256", "cloud_init_sha256",
    "adoption_mode", "last_operation", "last_typed_outcome",
}
result = {key: value.get(key) for key in sorted(allowed)}
result.update({
    "type": "multipass-inspection",
    "instance_normalized_state": normalized,
    "ownership_verified": status.get("ownership_verified") is True,
    "contract_verified": status.get("ownership_verified") is True,
    "source_prepared": status.get("source_manifest") is not None or value.get("source_archive_sha256") not in {None, "pending"},
    "cache_prepared": status.get("cache_verified") is True or value.get("reference_cache_manifest_sha256") not in {None, "pending"},
    "guest_probe_status": status.get("latest_probe_outcome", "not-reached"),
    "offline_status": sys.argv[4],
    "matrix_status": "passed" if state in {"running", "exporting", "exported"} else "not-reached",
    "export_status": "exported" if state == "exported" else "not-exported",
    "recommended_next_operation": recommendation,
})
print(json.dumps(result, sort_keys=True, separators=(",", ":")))
PY
  exit 0
fi

current_commit() {
  git -C "$repo_root" rev-parse HEAD
}

read_lifecycle_state() {
  python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])' <"$instance_lifecycle_path"
}

prepare_source() {
  local commit
  commit=$(current_commit)
  bash "$script_dir/transfer-source.sh" --commit "$commit"
}

prepare_cache() {
  bash "$script_dir/transfer-cache.sh"
}

build_guest_interop() {
  guest_exec /home/i2ptest/.cargo/bin/cargo +1.95.0 build --locked --package i2pr-interop >/dev/null
}

prepare_inputs() {
  prepare_source
  prepare_cache
  build_guest_interop
}

snapshot_if_missing() {
  local name=$1
  if [[ ! -f "$instance_state_dir/snapshot-$name.json" ]]; then
    bash "$script_dir/snapshot.sh" --name "$name"
  fi
}

run_all() {
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
  host_baseline_probe_outcome=not-run
  host_probe_output=$(bash "$repo_root/scripts/interop/probe-rootless-sandbox.sh" 2>&1 || true)
  host_baseline_probe_outcome=$(python3 - "$host_probe_output" <<'PY'
import json
import sys
for line in reversed(sys.argv[1].splitlines()):
    try:
        value = json.loads(line)
    except json.JSONDecodeError:
        continue
    if value.get("type") == "rootless-sandbox-probe":
        print(value.get("outcome", "other"))
        break
else:
    print("other")
PY
  )
  export host_baseline_probe_outcome
  if [[ ( "$resume_owned" == 1 || "$recreate_owned" == 1 ) && -f "$instance_lifecycle_path" ]]; then
    instance_name_arg=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["instance_name"])' <"$instance_lifecycle_path")
    generation=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["instance_generation"])' <"$instance_lifecycle_path")
    instance_name_provided=1
    configure_context "$run_id" "$instance_name_arg" "$generation"
  fi
  if [[ -f "$instance_lifecycle_path" && "$resume_owned" == 0 && "$recreate_owned" == 0 ]]; then
    write_environment_blocker blocked_stale_state_ambiguity allocation inspect-owned-instance
    typed_blocker blocked_stale_state_ambiguity
    exit 2
  fi
  if [[ -f "$instance_lifecycle_path" && "$resume_owned" == 1 ]]; then
    existing_state=$(read_lifecycle_state)
    case "$existing_state" in
      blocked|abandoned|destroyed)
        write_environment_blocker blocked_failed_state_requires_recreate resume explicit-recreate-required
        typed_blocker blocked_failed_state_requires_recreate
        exit 2
        ;;
    esac
  fi
  if [[ "$instance_name_provided" == 0 ]]; then
    allocated=0
    for attempt in $(seq 1 16); do
      candidate=$(python3 "$lifecycle_py" derive-instance --run-id "$run_id" --generation "$generation" --attempt "$attempt")
      if ! multipass info "$candidate" --format json >/dev/null 2>&1; then
        instance_name_arg="$candidate"
        allocated=1
        break
      fi
    done
    if [[ "$allocated" != 1 ]]; then
      write_environment_blocker blocked_instance_name_allocation_exhausted allocation choose-new-run-id
      typed_blocker blocked_instance_name_allocation_exhausted
      exit 2
    fi
    configure_context "$run_id" "$instance_name_arg" "$generation"
  fi
  export I2PR_MULTIPASS_RUN_ID="$run_id" I2PR_MULTIPASS_INSTANCE_NAME="$instance_name" I2PR_MULTIPASS_GENERATION="$instance_generation"
  if [[ "$recreate_owned" == 1 ]]; then
    bash "$script_dir/destroy.sh" --run-id "$run_id" --instance-name "$instance_name" --destroy-owned --recreate-owned
    generation=$((generation + 1))
    instance_name_arg=$(python3 "$lifecycle_py" derive-instance --run-id "$run_id" --generation "$generation")
    instance_name_provided=1
    configure_context "$run_id" "$instance_name_arg" "$generation"
    export I2PR_MULTIPASS_INSTANCE_NAME="$instance_name" I2PR_MULTIPASS_GENERATION="$instance_generation"
  fi
  if [[ "$resume_owned" == 1 || "$adopt_owned" == 1 ]]; then
    bash "$script_dir/create.sh" --run-id "$run_id" --instance-name "$instance_name" --generation "$instance_generation" --adopt-owned
  else
    bash "$script_dir/create.sh" --run-id "$run_id" --instance-name "$instance_name" --generation "$instance_generation"
  fi
  # This is intentionally before source/cache transfer; the host baseline is
  # informational, while the guest probe is the execution gate.
  bash "$script_dir/probe.sh"
  lifecycle_state=$(read_lifecycle_state)
  case "$lifecycle_state" in
    provisioned)
      snapshot_if_missing provisioned
      prepare_inputs
      snapshot_if_missing source-and-cache-ready
      ;;
    source_ready)
      prepare_cache
      build_guest_interop
      snapshot_if_missing source-and-cache-ready
      ;;
    cache_ready)
      prepare_source
      build_guest_interop
      snapshot_if_missing source-and-cache-ready
      ;;
    source_and_cache_ready|probe_passed|offline_ready|running)
      ;;
    exporting)
      bash "$script_dir/export-evidence.sh" --run-id "$run_id"
      lifecycle_state=exported
      ;;
    exported)
      printf '%s\n' '{"schema":1,"type":"multipass-lifecycle","outcome":"already-exported"}'
      ;;
    *)
      write_environment_blocker blocked_unknown_lifecycle_state resume inspect-owned-instance
      typed_blocker blocked_unknown_lifecycle_state
      exit 2
      ;;
  esac
  if [[ "$lifecycle_state" != exporting && "$lifecycle_state" != exported ]]; then
    bash "$script_dir/probe.sh"
    if [[ "$lifecycle_state" != running && "$lifecycle_state" != offline_ready ]]; then
      bash "$script_dir/prepare-offline.sh"
    fi
    bash "$script_dir/run-matrix.sh"
    bash "$script_dir/export-evidence.sh" --run-id "$run_id"
  fi
  if [[ "$destroy_after_export" == 1 || "$destroy_owned" == 1 ]]; then
    bash "$script_dir/destroy.sh" --run-id "$run_id" --instance-name "$instance_name" --destroy-owned
    bash "$script_dir/verify-clean-host.sh" --verify --run-id "$run_id" --instance-name "$instance_name"
  fi
}

case "$operation" in
  create) bash "$script_dir/create.sh" --run-id "$run_id" --instance-name "$instance_name" --generation "$instance_generation" ;;
  prepare) require_instance; prepare_inputs ;;
  probe) require_instance; bash "$script_dir/probe.sh" ;;
  run) require_instance; bash "$script_dir/prepare-offline.sh"; bash "$script_dir/run-matrix.sh" ;;
  export)
    require_instance
    [[ -n "$run_id" ]] || die "--run-id is required for --export"
    bash "$script_dir/export-evidence.sh" --run-id "$run_id"
    ;;
  destroy) bash "$script_dir/destroy.sh" --run-id "$run_id" --instance-name "$instance_name" --destroy-owned ;;
  all) run_all ;;
  *) die "unknown operation: $operation" ;;
esac
