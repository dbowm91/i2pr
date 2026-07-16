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
    *) die "usage: run-gate.sh --profile <environment-smoke|reference-crosscheck-ipv4|handshake-smoke|handshake-smoke-rootless|full> --offline" ;;
  esac
  shift
done
[[ "$profile" =~ ^(environment-smoke|reference-crosscheck-ipv4|handshake-smoke|handshake-smoke-rootless|full)$ ]] || die "invalid gate profile"
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

staging_dir="$INTEROP_TARGET/evidence/staging/$profile"

cleanup_staging() {
  if [[ -d "$staging_dir" ]]; then
    rm -rf -- "$staging_dir" 2>/dev/null || true
  fi
}

on_signal() {
  cleanup_staging
  cleanup_and_verify || true
  exit 143
}
trap on_signal INT TERM

snapshot_file="$(mktemp)"
trap 'rm -f "$snapshot_file"; cleanup_staging; cleanup_and_verify || true' EXIT

for existing in "$INTEROP_TARGET/evidence"/*.json; do
  [[ -f "$existing" ]] || continue
  bn="$(basename "$existing")"
  [[ "$bn" == "run-manifest.json" ]] && continue
  gate_prefix="${bn%%--*}"
  if [[ "$gate_prefix" != "$profile" ]]; then
    sha256sum "$existing" >> "$snapshot_file"
  fi
done

rm -rf -- "$staging_dir" 2>/dev/null || true
mkdir -p -m 0700 "$staging_dir"

export INTEROP_EVIDENCE_DIR="$staging_dir"

set +e
"$script_dir/run-matrix.sh" --profile "$profile" --offline
gate_status=$?
set -e

cleanup_status=0
cleanup_and_verify || cleanup_status=$?

if [[ -d "$staging_dir" ]]; then
  for record in "$staging_dir"/*.json; do
    [[ -f "$record" ]] || continue
    basename_record="$(basename "$record")"

    scenario_id="$(python3 -c "import json,sys; print(json.loads(open('$record').read()).get('scenario_id',''))")"
    [[ -n "$scenario_id" ]] || die "staged record missing scenario_id: $basename_record"

    case "$profile" in
      environment-smoke)
        [[ "$scenario_id" =~ ^(java-ipv4-inbound-outbound|i2pd-ipv4-inbound-outbound)$ ]] \
          || die "scenario $scenario_id not allowed for gate $profile"
        ;;
      reference-crosscheck-ipv4)
        [[ "$scenario_id" =~ ^(reference-java-i2pd-ipv4|reference-i2pd-java-ipv4)$ ]] \
          || die "scenario $scenario_id not allowed for gate $profile"
        ;;
      handshake-smoke)
        [[ "$scenario_id" =~ ^(java-ipv4-inbound-outbound|i2pd-ipv4-inbound-outbound)$ ]] \
          || die "scenario $scenario_id not allowed for gate $profile"
        ;;
      handshake-smoke-rootless)
        [[ "$scenario_id" =~ ^(i2pr-to-java-ipv4|java-to-i2pr-ipv4|i2pr-to-i2pd-ipv4|i2pd-to-i2pr-ipv4)$ ]] \
          || die "scenario $scenario_id not allowed for gate $profile"
        ;;
      full)
        [[ "$scenario_id" =~ ^(java-ipv4-inbound-outbound|java-ipv6-inbound-outbound|java-adversarial-and-resource|java-duplicate-link-race|i2pd-ipv4-inbound-outbound|i2pd-ipv6-inbound-outbound|i2pd-adversarial-and-resource|i2pd-duplicate-link-race)$ ]] \
          || die "scenario $scenario_id not allowed for gate $profile"
        ;;
    esac

    destination="$INTEROP_TARGET/evidence/${profile}--${basename_record}"
    [[ ! -e "$destination" ]] || die "evidence filename collision: ${profile}--${basename_record}"

    mv -- "$record" "$destination"
  done
fi

while IFS= read -r line; do
  expected_hash="${line%%  *}"
  file_path="${line#*  }"
  if [[ -f "$file_path" ]]; then
    actual_hash="$(sha256sum "$file_path" | awk '{print $1}')"
    [[ "$actual_hash" == "$expected_hash" ]] \
      || die "earlier gate record modified: $(basename "$file_path")"
  else
    die "earlier gate record deleted: $(basename "$file_path")"
  fi
done < "$snapshot_file"

cleanup_staging

if ((gate_status != 0)); then
  printf 'gate %s failed with typed matrix status %d\n' "$profile" "$gate_status" >&2
  exit "$gate_status"
fi
((cleanup_status == 0)) || exit "$cleanup_status"
printf 'gate %s passed and clean-host verification completed\n' "$profile"
