#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
profile=""
expect_profile=0
offline=0
keep=0
for arg in "$@"; do
  case "$arg" in
    --profile) expect_profile=1 ;;
    handshake-smoke|environment-smoke|reference-crosscheck-ipv4|full)
      if [[ "$expect_profile" == "1" ]]; then profile="$arg"; expect_profile=0; fi ;;
    --offline) offline=1 ;;
    --keep-failed-sanitized) keep=1 ;;
    *) : ;;
  esac
done
[[ -n "$profile" && "$expect_profile" == "0" ]] || { printf 'usage: run-matrix.sh --profile <environment-smoke|handshake-smoke|reference-crosscheck-ipv4|full>\n' >&2; exit 2; }
case "$profile" in
  environment-smoke) ids=(smoke-java-ipv4 smoke-i2pd-ipv4) ;;
  handshake-smoke) ids=(java-ipv4-inbound-outbound i2pd-ipv4-inbound-outbound) ;;
  reference-crosscheck-ipv4) ids=(java-ipv4-inbound-outbound i2pd-ipv4-inbound-outbound) ;;
  full) ids=(java-ipv4-inbound-outbound java-ipv6-inbound-outbound java-adversarial-and-resource java-duplicate-link-race i2pd-ipv4-inbound-outbound i2pd-ipv6-inbound-outbound i2pd-adversarial-and-resource i2pd-duplicate-link-race) ;;
esac
status=0
for scenario in "${ids[@]}"; do
  reference=java_i2p
  [[ "$scenario" == i2pd-* ]] && reference=i2pd
  args=(--scenario "$scenario" --reference "$reference")
  [[ "$offline" == "1" ]] && args+=(--offline)
  [[ "$keep" == "1" ]] && args+=(--keep-failed-sanitized)
  if ! bash "$root/scripts/interop/run-scenario.sh" "${args[@]}"; then status=1; fi
done
exit "$status"
