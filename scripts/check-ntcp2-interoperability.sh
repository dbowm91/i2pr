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
  rg -Fq "$entry" "$manifest" || {
    echo "NTCP2 interoperability manifest entry missing: $entry" >&2
    exit 1
  }
done

scenario_count=$(rg -c '^\[\[scenario\]\]$' "$manifest")
if [[ "$scenario_count" -ne 8 ]]; then
  echo "expected eight bounded NTCP2 interoperability scenarios, found $scenario_count" >&2
  exit 1
fi

# The committed evidence directory is intentionally text-only and sanitized.
if find "$evidence" -type f \( -name '*.pcap' -o -name '*.pcapng' -o -name 'router.identity' -o -name 'ntcp2.static.key' \) -print -quit | rg -q .; then
  echo "forbidden NTCP2 evidence artifact present" >&2
  exit 1
fi
if rg -n --hidden --glob '!README.md' --glob '!*.tsv' \
  -- '-----BEGIN .*PRIVATE KEY-----|-----BEGIN OPENSSH PRIVATE KEY-----' "$evidence"; then
  echo "private-key material found in NTCP2 evidence" >&2
  exit 1
fi

echo "NTCP2 interoperability manifest and sanitized evidence boundary are valid (${scenario_count} scenarios)."
