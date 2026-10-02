#!/usr/bin/env bash
# Plan 296 service-boundary leak regression checker.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
pattern='i2pr/[0-9][^[:space:]<>]*|Proxy-Agent:[[:space:]]*i2pr|DEFAULT_QUIT_REASON[[:space:]]*:[^;]*i2pr|product-version-sentinel|build-sha-sentinel|local-hostname-sentinel|local-ip-sentinel|filesystem-path-sentinel|local-destination-alias-sentinel'
scan_paths=(
  "$root/crates/i2pr-service-tunnels/src/http"
  "$root/crates/i2pr-service-tunnels/src/irc"
  "$root/crates/i2pr-daemon/src/service_tunnels_http.rs"
  "$root/crates/i2pr-daemon/src/service_tunnels_socks5.rs"
  "$root/crates/i2pr-daemon/src/service_tunnels_irc_client.rs"
  "$root/crates/i2pr-daemon/src/service_tunnels_irc_server.rs"
  "$root/crates/i2pr-daemon/tests"
)

if grep -REni "$pattern" "${scan_paths[@]}" >/dev/null; then
  echo "service-boundary anonymity leak marker found" >&2
  exit 1
fi

# Exercise the same marker expression against seeded violations so
# a future pattern edit cannot silently disable the negative gate.
seed="${TMPDIR:-/tmp}/i2pr-anonymity-check-$$"
trap 'rm -f "$seed"' EXIT
printf '%s\n' \
  'User-Agent: i2pr/9.9' \
  'product-version-sentinel build-sha-sentinel local-hostname-sentinel' \
  'local-ip-sentinel filesystem-path-sentinel local-destination-alias-sentinel' \
  'Proxy-Agent: i2pr' >"$seed"
if ! grep -Ein "$pattern" "$seed" >/dev/null; then
  echo "service-boundary checker failed its seeded leak self-check" >&2
  exit 1
fi

echo "service-boundary anonymity checks passed"
