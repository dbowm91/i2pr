#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
profile=""
offline=0
keep=0
usage='usage: run-matrix.sh --profile <environment-smoke|handshake-smoke|reference-crosscheck-ipv4|full> [--offline] [--keep-failed-sanitized]'
while (($#)); do
  case "$1" in
    --profile)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      [[ -z "$profile" ]] || { printf 'duplicate --profile\n' >&2; exit 2; }
      profile=$2
      shift
      ;;
    --offline) offline=1 ;;
    --keep-failed-sanitized) keep=1 ;;
    *) printf 'unknown run-matrix option: %s\n%s\n' "$1" "$usage" >&2; exit 2 ;;
  esac
  shift
done
[[ -n "$profile" ]] || { printf '%s\n' "$usage" >&2; exit 2; }
case "$profile" in
  environment-smoke|handshake-smoke|reference-crosscheck-ipv4|full) ;;
  *) printf 'invalid profile: %s\n' "$profile" >&2; exit 2 ;;
esac
ids=()
mixed_ids=()
case "$profile" in
  environment-smoke) ids=(smoke-java-ipv4 smoke-i2pd-ipv4) ;;
  handshake-smoke) mixed_ids=(i2pr-to-java-ipv4 java-to-i2pr-ipv4 i2pr-to-i2pd-ipv4 i2pd-to-i2pr-ipv4) ;;
  reference-crosscheck-ipv4)
    status=0
    for scenario in reference-java-i2pd-ipv4 reference-i2pd-java-ipv4; do
      args=(--scenario "$scenario")
      [[ "$offline" == "1" ]] && args+=(--offline)
      [[ "$keep" == "1" ]] && args+=(--keep-failed-sanitized)
      if ! python3 "$root/tests/integration/ntcp2/harness/reference_runner.py" "${args[@]}"; then status=1; fi
    done
    exit "$status"
    ;;
  full)
    ids=(java-ipv4-inbound-outbound java-ipv6-inbound-outbound java-adversarial-and-resource java-duplicate-link-race i2pd-ipv4-inbound-outbound i2pd-ipv6-inbound-outbound i2pd-adversarial-and-resource i2pd-duplicate-link-race)
    mixed_ids=(i2pr-to-java-ipv4 java-to-i2pr-ipv4 i2pr-to-i2pd-ipv4 i2pd-to-i2pr-ipv4)
    ;;
esac
status=0
reference_for() {
  local scenario=$1
  case "$scenario" in
    *-java-*|java-to-*) echo java_i2p ;;
    *-i2pd-*|i2pd-to-*) echo i2pd ;;
    *) echo i2pd ;;
  esac
}

if [[ ${#ids[@]} -gt 0 ]]; then
  for scenario in "${ids[@]}"; do
    reference=$(reference_for "$scenario")
    args=(--scenario "$scenario" --reference "$reference")
    [[ "$offline" == "1" ]] && args+=(--offline)
    [[ "$keep" == "1" ]] && args+=(--keep-failed-sanitized)
    if ! bash "$root/scripts/interop/run-scenario.sh" "${args[@]}"; then status=1; fi
  done
fi
if [[ ${#mixed_ids[@]} -gt 0 ]]; then
  for scenario in "${mixed_ids[@]}"; do
    reference=$(reference_for "$scenario")
    args=(--scenario "$scenario" --reference "$reference")
    [[ "$offline" == "1" ]] && args+=(--offline)
    [[ "$keep" == "1" ]] && args+=(--keep-failed-sanitized)
    if ! python3 "$root/tests/integration/ntcp2/harness/mixed_runner.py" "${args[@]}"; then status=1; fi
  done
fi
exit "$status"
