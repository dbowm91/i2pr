#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$root/tests/integration/ntcp2/manifest.toml"
evidence="$root/tests/integration/ntcp2/evidence"

test -f "$manifest"
test -d "$evidence"

required=(
  'network_id = "synthetic-private-036"'
  'public_network = false'
  'reseed = false'
  'bootstrap = false'
  'release = "2.12.0"'
  'source_revision = "2800040"'
  'release = "2.60.0"'
  'source_revision = "f618e41"'
  'daemon_activation = "disabled; no complete wire-level composition is currently exposed"'
)
for entry in "${required[@]}"; do
  if ! grep -Fq "$entry" "$manifest"; then
    echo "NTCP2 interoperability manifest entry missing: $entry" >&2
    exit 1
  fi
done

scenario_count=$(grep -Ec '^\[\[scenario\]\]$' "$manifest" || true)
if [[ "$scenario_count" -ne 8 ]]; then
  echo "expected eight bounded NTCP2 interoperability scenarios, found $scenario_count" >&2
  exit 1
fi

expected_ids=(
  java-ipv4-inbound-outbound
  java-ipv6-inbound-outbound
  java-adversarial-and-resource
  java-duplicate-link-race
  i2pd-ipv4-inbound-outbound
  i2pd-ipv6-inbound-outbound
  i2pd-adversarial-and-resource
  i2pd-duplicate-link-race
)
for scenario_id in "${expected_ids[@]}"; do
  count=$(grep -Ec "^id = \"${scenario_id//-/\\-}\"$" "$manifest" || true)
  if [[ "$count" -ne 1 ]]; then
    echo "expected exactly one NTCP2 scenario id: $scenario_id (found $count)" >&2
    exit 1
  fi
done

duplicate_ids=$(grep -E '^id = "' "$manifest" | sort | uniq -d || true)
if [[ -n "$duplicate_ids" ]]; then
  echo "duplicate NTCP2 scenario id(s): $duplicate_ids" >&2
  exit 1
fi

# The committed evidence directory is intentionally text-only and sanitized.
if find "$evidence" -type f \( -name '*.pcap' -o -name '*.pcapng' -o -name 'router.identity' -o -name 'ntcp2.static.key' \) -print -quit | grep -q .; then
  echo "forbidden NTCP2 evidence artifact present" >&2
  exit 1
fi
if find "$evidence" -type f ! -name README.md -print0 \
  | xargs -0 grep -En -- '-----BEGIN .*PRIVATE KEY-----|-----BEGIN OPENSSH PRIVATE KEY-----' >/dev/null 2>&1; then
  echo "private-key material found in NTCP2 evidence" >&2
  exit 1
fi

echo "NTCP2 interoperability manifest and sanitized evidence boundary are valid (${scenario_count} scenarios)."
