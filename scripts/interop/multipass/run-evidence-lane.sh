#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

operation=""
destroy_after_export=0
run_id=""
while (($#)); do
  case "$1" in
    --create|--prepare|--probe|--run|--export|--destroy|--all)
      [[ -z "$operation" ]] || die "exactly one operation is required"
      operation="${1#--}"
      ;;
    --destroy-after-export) [[ "$destroy_after_export" == 0 ]] || die "duplicate --destroy-after-export"; destroy_after_export=1 ;;
    --run-id) [[ -z "$run_id" && $# -ge 2 ]] || die "duplicate or incomplete --run-id"; run_id=$2; shift ;;
    --help|-h) printf 'usage: run-evidence-lane.sh (--create|--prepare|--probe|--run|--export|--destroy|--all) [--run-id <id>] [--destroy-after-export]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[[ -n "$operation" ]] || die "one operation is required"
if [[ -n "$run_id" ]]; then validate_run_id "$run_id"; fi
if [[ "$destroy_after_export" == 1 && "$operation" != all ]]; then
  die "--destroy-after-export is valid only with --all"
fi

current_commit() {
  git -C "$repo_root" rev-parse HEAD
}

prepare_inputs() {
  local commit
  commit=$(current_commit)
  bash "$script_dir/transfer-source.sh" --commit "$commit"
  bash "$script_dir/transfer-cache.sh"
  guest_exec /home/i2ptest/.cargo/bin/cargo +1.95.0 build --locked --package i2pr-interop >/dev/null
}

run_all() {
  bash "$script_dir/verify-clean-host.sh" --record-baseline >/dev/null
  bash "$script_dir/create.sh"
  bash "$script_dir/snapshot.sh" --name provisioned
  prepare_inputs
  bash "$script_dir/snapshot.sh" --name source-and-cache-ready
  bash "$script_dir/probe.sh"
  bash "$script_dir/prepare-offline.sh"
  bash "$script_dir/run-matrix.sh"
  if [[ -z "$run_id" ]]; then
    run_id="plan048-$(date -u +%Y%m%dT%H%M%SZ)"
  fi
  bash "$script_dir/export-evidence.sh" --run-id "$run_id"
  if [[ "$destroy_after_export" == 1 ]]; then
    bash "$script_dir/destroy.sh"
    bash "$script_dir/verify-clean-host.sh" --verify
  fi
}

case "$operation" in
  create) bash "$script_dir/create.sh" ;;
  prepare) require_instance; prepare_inputs ;;
  probe) require_instance; bash "$script_dir/probe.sh" ;;
  run) require_instance; bash "$script_dir/prepare-offline.sh"; bash "$script_dir/run-matrix.sh" ;;
  export)
    require_instance
    [[ -n "$run_id" ]] || die "--run-id is required for --export"
    bash "$script_dir/export-evidence.sh" --run-id "$run_id"
    ;;
  destroy) bash "$script_dir/destroy.sh" ;;
  all) run_all ;;
  *) die "unknown operation: $operation" ;;
esac
