#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$root/tests/integration/ntcp2/manifest.toml"
lock="$root/tests/integration/ntcp2/references.lock.toml"
evidence="$root/tests/integration/ntcp2/evidence"

test -f "$manifest"
test -f "$lock"
test -d "$evidence"

required=(
  'network_id = "synthetic-private-036"'
  'public_network = false'
  'reseed = false'
  'bootstrap = false'
  'release = "2.12.0"'
  'source_revision = "2800040deee9bb376567b671ef2e9c34cf3e30b6"'
  'release = "2.60.0"'
  'source_revision = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"'
  'daemon_activation = "disabled; no complete wire-level composition is currently exposed"'
)
for entry in "${required[@]}"; do
  if ! grep -Fq "$entry" "$manifest"; then
    echo "NTCP2 interoperability manifest entry missing: $entry" >&2
    exit 1
  fi
done
for entry in \
  'host_contract = "ubuntu-24.04-amd64"' \
  'execution_network = "forbidden"' \
  'sha256 = "a3f2c85afea82e04ebca5ebb1b9b5c95ea770c4d35a7635de312370e14a44d43"'; do
  if ! grep -Fq "$entry" "$lock"; then
    echo "NTCP2 reference lock entry missing: $entry" >&2
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

if ! grep -Fq 'PIPELINE_PROFILE' "$root/tests/integration/ntcp2/harness/mixed_runner.py"; then
  echo "Plan 052 pipeline profile is not wired into mixed runner" >&2
  exit 1
fi
if ! grep -Fq 'write_direction_artifacts' "$root/tests/integration/ntcp2/harness/mixed_runner.py"; then
  echo "Plan 052 direction artifact writer is not wired" >&2
  exit 1
fi
if grep -Fq 'export_root / "export-acknowledgement.json"' "$root/tests/integration/ntcp2/harness/evidence_bundle.py"; then
  echo "Plan 052 export acknowledgement must remain outside immutable bundle" >&2
  exit 1
fi
if grep -Fq 'raise MixedRunError("i2pr-responder-handshake-failed")' "$root/tests/integration/ntcp2/harness/mixed_runner.py"; then
  echo "bounded responder reason was collapsed to historical generic code" >&2
  exit 1
fi

# Plan 054: machine-readable observation catalog must be present and consistent
# with the explanatory Markdown document.
catalog="$root/tests/integration/ntcp2/reference-observation-catalog.toml"
test -f "$catalog" || { echo "Plan 054 observation catalog missing" >&2; exit 1; }
if grep -Eq 'PENDING-SOURCE-INSPECTION|PENDING' "$catalog"; then
  echo "Plan 054 observation catalog still has pending source entries" >&2
  exit 1
fi
if ! grep -Fq 'def collect_observation' "$root/tests/integration/ntcp2/harness/java_i2p.py"; then
  echo "Java I2P adapter missing Plan 054 collect_observation" >&2
  exit 1
fi
if ! grep -Fq 'def collect_observation' "$root/tests/integration/ntcp2/harness/i2pd.py"; then
  echo "i2pd adapter missing Plan 054 collect_observation" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-reference-observation-catalog-v1' "$catalog"; then
  echo "Plan 054 observation catalog schema marker is missing" >&2
  exit 1
fi
# The hardcoded "always reject" pattern is detectable: the predicate must
# never unconditionally return ``reference-receiver-marker-not-source-locked``
# as its final return. Ensure the predicate has at least one ``"passed"``
# return.
if ! grep -Eq 'return "passed", "mixed-router-direction-authenticated"' "$root/tests/integration/ntcp2/harness/mixed_runner.py"; then
  echo "Plan 052 predicate is missing a passed terminal return" >&2
  exit 1
fi
if ! grep -Fq 'seeded-clone' "$root/tests/integration/ntcp2/harness/java_startup_probe.py"; then
  echo "Plan 054 Java seeded-clone data state is missing" >&2
  exit 1
fi
if ! grep -Fq 'java-random-source-shutdown' "$root/tests/integration/ntcp2/harness/java_startup_probe.py"; then
  echo "Plan 054 Java failure-stage taxonomy is missing" >&2
  exit 1
fi

python3 "$root/scripts/interop/validate-evidence.py"
python3 "$root/scripts/interop/validate-scenarios.py"

echo "NTCP2 interoperability manifest and sanitized evidence boundary are valid (${scenario_count} scenarios)."
