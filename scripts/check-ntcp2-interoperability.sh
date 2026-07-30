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

# Plan 055: locked trigger record schema and source-inspected trigger
# contracts must be present and the ADR for the optional Java support
# topology must be written before the helper is implemented.
if ! grep -Fq 'TRIGGER_SCHEMA = "i2pr-reference-trigger-v3"' "$root/tests/integration/ntcp2/harness/trigger_record.py"; then
  echo "Plan 055 trigger schema is missing or wrong version" >&2
  exit 1
fi
if ! grep -Fq '"i2pr-reference-trigger-v3"' "$root/tests/integration/ntcp2/harness/evidence_bundle.py"; then
  echo "Plan 055 trigger schema is not allowlisted in evidence bundle" >&2
  exit 1
fi
if ! grep -Fq 'Plan 055 C5' "$root/tests/integration/ntcp2/reference-trigger-contracts.md"; then
  echo "Plan 055 reference-trigger contracts are missing the C5 decision record" >&2
  exit 1
fi
if ! test -f "$root/docs/adr/0021-minimal-java-support-topology.md"; then
  echo "Plan 055 ADR 0021 (minimal Java support topology) is missing" >&2
  exit 1
fi
if ! grep -Fq 'test_plan055.py' "$root/AGENTS.md"; then
  echo "Plan 055 test_plan055 is not wired into AGENTS.md" >&2
  exit 1
fi

# Plan 056: two-bundle Milestone 3 certificate verifier and its test
# matrix must be present.
if ! test -f "$root/tests/integration/ntcp2/harness/verify_milestone3_certificate.py"; then
  echo "Plan 056 certificate verifier is missing" >&2
  exit 1
fi
if ! grep -Fq 'CERTIFICATE_SCHEMA = "i2pr-milestone3-certificate-v1"' \
    "$root/tests/integration/ntcp2/harness/verify_milestone3_certificate.py"; then
  echo "Plan 056 certificate verifier schema is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan056.py"; then
  echo "Plan 056 certificate verification test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'CertificatePositiveTests' "$root/tests/integration/ntcp2/harness/test_plan056.py"; then
  echo "Plan 056 certificate verification tests do not include positive case" >&2
  exit 1
fi
if ! grep -Fq 'CertificateNegativeTests' "$root/tests/integration/ntcp2/harness/test_plan056.py"; then
  echo "Plan 056 certificate verification tests do not include negative case" >&2
  exit 1
fi

# Plan 058: candidate record integrity, supersession, and execution-lane
# invariants. The retired candidate must remain retired; the
# superseded Plan 057 must remain superseded; the Plan 058 candidate
# record validator must be present; the locked candidate record
# schema marker must be present; ADR 0021 must carry an explicit
# Accepted/Rejected decision.
if ! test -f "$root/tests/integration/ntcp2/harness/candidate_record.py"; then
  echo "Plan 058 candidate record validator is missing" >&2
  exit 1
fi
if ! grep -Fq 'CANDIDATE_SCHEMA = "i2pr-interop-candidate-v1"' \
    "$root/tests/integration/ntcp2/harness/candidate_record.py"; then
  echo "Plan 058 candidate record schema is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan058.py"; then
  echo "Plan 058 candidate record integrity tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'CandidatePositiveTests' "$root/tests/integration/ntcp2/harness/test_plan058.py"; then
  echo "Plan 058 tests do not include positive case" >&2
  exit 1
fi
if ! grep -Fq 'CandidateRejectionTests' "$root/tests/integration/ntcp2/harness/test_plan058.py"; then
  echo "Plan 058 tests do not include rejection case" >&2
  exit 1
fi
if ! grep -Fq 'ExecutionLaneTests' "$root/tests/integration/ntcp2/harness/test_plan058.py"; then
  echo "Plan 058 tests do not include execution-lane case" >&2
  exit 1
fi
plan056_candidate="$root/plans/056-candidate.md"
if ! grep -Eq '^#+\s*Status:\s*\*?\*?retired|^Status:\s*\*?\*?retired' "$plan056_candidate"; then
  echo "Plan 056 candidate must declare retired status" >&2
  exit 1
fi
plan057="$root/plans/057-cross-host-milestone-3-external-evidence-run.md"
if ! grep -Eq '^#+\s*Status:\s*\*?\*?superseded|^Status:\s*\*?\*?superseded' "$plan057"; then
  echo "Plan 057 must declare superseded status" >&2
  exit 1
fi
adr_0021="$root/docs/adr/0021-minimal-java-support-topology.md"
if ! grep -Eq '^- Status:\s*(Accepted|Rejected)\b' "$adr_0021"; then
  echo "ADR 0021 must declare an explicit Accepted or Rejected decision" >&2
  exit 1
fi
if ! grep -Fq 'target/interop/evidence/plan056' \
    "$root/plans/056-closure.md"; then
  echo "Plan 056 closure must describe the local diagnostics accurately" >&2
  exit 1
fi
if ! grep -Eq 'local-untracked|artifacts? (under|are) (the|an?) ignored' \
    "$root/plans/056-closure.md"; then
  echo "Plan 056 closure must mark the local diagnostics as local-untracked" >&2
  exit 1
fi

# Plan 059: reference-side implementation and qualification closure
# pass. The i2pd direct helper source, build contract, and source-lock
# record must be present; the per-reference observation qualification
# receipts and the typed-absence summary must be present; the Plan 059
# test matrix must be present with positive, rejection, execution-lane,
# pipeline, and ADR-decision cases; the canonical pipeline must consume
# live trigger and observation records under ``live_mode`` and refuse
# the synthetic fallback for a passed reference-initiated direction.
if ! test -f "$root/tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/CMakeLists.txt"; then
  echo "Plan 059 i2pd direct helper CMakeLists.txt is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.cpp"; then
  echo "Plan 059 i2pd direct helper C++ source is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json"; then
  echo "Plan 059 i2pd direct helper source-lock record is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-i2pd-helper-source-lock-v1' \
    "$root/tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json"; then
  echo "Plan 059 i2pd direct helper source-lock schema marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pd-direct-helper' \
    "$root/tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json"; then
  echo "Plan 059 i2pd direct helper kind marker is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/reference-observation-qualification/i2pd-2.60.0.json"; then
  echo "Plan 059 i2pd observation qualification receipt is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/reference-observation-qualification/java_i2p-2.12.0.json"; then
  echo "Plan 059 Java observation qualification receipt is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-reference-observation-qualification-v1' \
    "$root/tests/integration/ntcp2/reference-observation-qualification/i2pd-2.60.0.json"; then
  echo "Plan 059 i2pd qualification receipt schema marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'blocked_java_support_topology_rejected' \
    "$root/tests/integration/ntcp2/reference-observation-qualification/java_i2p-2.12.0.json"; then
  echo "Plan 059 Java qualification receipt must carry the typed blocker" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/plan059.py"; then
  echo "Plan 059 helper module is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan059.py"; then
  echo "Plan 059 test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'I2pdHelperSourceLockTests' "$root/tests/integration/ntcp2/harness/test_plan059.py"; then
  echo "Plan 059 test matrix must include the i2pd helper cases" >&2
  exit 1
fi
if ! grep -Fq 'JavaSupportTopologyGateTests' "$root/tests/integration/ntcp2/harness/test_plan059.py"; then
  echo "Plan 059 test matrix must include the Java support-topology gate" >&2
  exit 1
fi
if ! grep -Fq 'ReceiverObservationTests' "$root/tests/integration/ntcp2/harness/test_plan059.py"; then
  echo "Plan 059 test matrix must include the receiver-observation cases" >&2
  exit 1
fi
if ! grep -Fq 'PipelineLiveModeTests' "$root/tests/integration/ntcp2/harness/test_plan059.py"; then
  echo "Plan 059 test matrix must include the pipeline live-mode cases" >&2
  exit 1
fi
if ! grep -Fq 'live-mode-requires-trigger-record' \
    "$root/tests/integration/ntcp2/harness/plan052_pipeline.py"; then
  echo "Plan 059 pipeline must refuse the synthetic trigger fallback in live mode" >&2
  exit 1
fi
if ! grep -Fq 'live-mode-requires-i2pr-observation' \
    "$root/tests/integration/ntcp2/harness/plan052_pipeline.py"; then
  echo "Plan 059 pipeline must require live i2pr observation in live mode" >&2
  exit 1
fi
if ! grep -Fq 'cleanup-failure-overrides-pass' \
    "$root/tests/integration/ntcp2/harness/plan052_pipeline.py"; then
  echo "Plan 059 pipeline must override pass when cleanup fails" >&2
  exit 1
fi
if ! grep -Fq 'helper_digest_sha256' \
    "$root/tests/integration/ntcp2/harness/plan052_pipeline.py"; then
  echo "Plan 059 pipeline must bind helper digest into the direction record" >&2
  exit 1
fi

python3 "$root/scripts/interop/validate-evidence.py"
python3 "$root/scripts/interop/validate-scenarios.py"

echo "NTCP2 interoperability manifest and sanitized evidence boundary are valid (${scenario_count} scenarios)."
