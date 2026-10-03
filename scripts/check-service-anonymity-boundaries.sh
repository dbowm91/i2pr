#!/usr/bin/env bash
# Plan 307 service-boundary matrix and leak regression checker.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
pattern='i2pr/[0-9][^[:space:]<>]*|Proxy-Agent:[[:space:]]*i2pr|DEFAULT_QUIT_REASON[[:space:]]*:[^;]*i2pr|product-version-sentinel|build-sha-sentinel|router-hash-sentinel|routerinfo-sentinel|transport-address-sentinel|local-hostname-sentinel|local-ip-sentinel|filesystem-path-sentinel|local-destination-alias-sentinel|raw-error-sentinel'
scan_paths=(
  "$root/crates/i2pr-service-tunnels/src"
  "$root/crates/i2pr-daemon/src/service_tunnels_http.rs"
  "$root/crates/i2pr-daemon/src/service_tunnels_socks5.rs"
  "$root/crates/i2pr-daemon/src/service_tunnels_irc_client.rs"
  "$root/crates/i2pr-daemon/src/service_tunnels_irc_server.rs"
  "$root/crates/i2pr-daemon/src/service_tunnels.rs"
  "$root/crates/i2pr-daemon/src/service_delivery.rs"
  "$root/crates/i2pr-daemon/src/service_product.rs"
  "$root/crates/i2pr-daemon/tests"
)

python3 - "$root/specs/service-boundary-matrix.toml" <<'PY'
import sys
import tomllib

with open(sys.argv[1], "rb") as source:
    matrix = tomllib.load(source)
required = {
    ("http-client", "local-client-to-remote-destination"),
    ("http-connect", "local-client-to-remote-destination"),
    ("socks5-client", "local-client-to-remote-destination"),
    ("generic-client", "local-client-to-remote-destination"),
    ("generic-server", "remote-destination-to-loopback-backend"),
    ("irc-client", "local-client-to-remote-destination"),
    ("irc-server", "remote-destination-to-loopback-backend"),
}
actual = {(row.get("service"), row.get("direction")) for row in matrix.get("boundary", [])}
if matrix.get("schema_version") != 1 or actual != required:
    raise SystemExit("service-boundary matrix is incomplete or has an unexpected schema")
for row in matrix["boundary"]:
    for field in ("peer_input", "synthesized", "pass_through", "forbidden"):
        if not isinstance(row.get(field), list):
            raise SystemExit(f"service-boundary row {row['service']} lacks {field}")
PY

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
  'router-hash-sentinel routerinfo-sentinel transport-address-sentinel' \
  'local-ip-sentinel filesystem-path-sentinel local-destination-alias-sentinel raw-error-sentinel' \
  'Proxy-Agent: i2pr' >"$seed"
if ! grep -Ein "$pattern" "$seed" >/dev/null; then
  echo "service-boundary checker failed its seeded leak self-check" >&2
  exit 1
fi

echo "service-boundary anonymity checks passed"
