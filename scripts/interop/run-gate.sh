#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"

profile=""
offline=0
while (($#)); do
  case "$1" in
    --profile)
      (($# >= 2)) || die "--profile requires a value"
      profile=$2
      shift
      ;;
    --offline) offline=1 ;;
    *) die "usage: run-gate.sh --profile <environment-smoke|reference-crosscheck-ipv4|handshake-smoke|full> --offline" ;;
  esac
  shift
done
[[ "$profile" =~ ^(environment-smoke|reference-crosscheck-ipv4|handshake-smoke|full)$ ]] || die "invalid gate profile"
((offline == 1)) || die "Plan 043 execution gates require --offline"

cleanup_and_verify() {
  local cleanup_status=0
  local verify_status=0
  "$script_dir/cleanup.sh" >/dev/null 2>&1 || cleanup_status=$?
  "$script_dir/verify-clean-host.sh" --verify >/dev/null 2>&1 || verify_status=$?
  if ((cleanup_status != 0 || verify_status != 0)); then
    printf 'gate cleanup failed: cleanup=%d verify=%d\n' "$cleanup_status" "$verify_status" >&2
    return 1
  fi
}

on_signal() {
  cleanup_and_verify || true
  exit 143
}
trap on_signal INT TERM

set +e
"$script_dir/run-matrix.sh" --profile "$profile" --offline
gate_status=$?
set -e

cleanup_status=0
cleanup_and_verify || cleanup_status=$?

for record in "$INTEROP_TARGET/evidence"/*.json; do
  [[ -f "$record" ]] || continue
  [[ "$(basename "$record")" != "run-manifest.json" ]] || die "aggregate manifest exists before gate archival"
  destination="$INTEROP_TARGET/evidence/${profile}--$(basename "$record")"
  [[ ! -e "$destination" ]] || die "evidence filename collision"
  mv -- "$record" "$destination"
done

if ((gate_status != 0)); then
  printf 'gate %s failed with typed matrix status %d\n' "$profile" "$gate_status" >&2
  exit "$gate_status"
fi
((cleanup_status == 0)) || exit "$cleanup_status"
printf 'gate %s passed and clean-host verification completed\n' "$profile"
