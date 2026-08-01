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

# Plan 060: fresh candidate and two-run Milestone 3 certificate
# closure pass. The Plan 060 helper module, the Plan 060 test
# matrix, the typed blocker, and the close-status classifier must be
# committed and the static boundary checker must refuse to allow a
# freeze while Plan 058/059 prerequisites are absent.
if ! test -f "$root/tests/integration/ntcp2/harness/plan060.py"; then
  echo "Plan 060 helper module is missing" >&2
  exit 1
fi
if ! grep -Fq 'TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE = "blocked_execution_lane_unavailable"' \
    "$root/tests/integration/ntcp2/harness/plan060.py"; then
  echo "Plan 060 typed blocker marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'def plan060_close_status' \
    "$root/tests/integration/ntcp2/harness/plan060.py"; then
  echo "Plan 060 close-status classifier is missing" >&2
  exit 1
fi
if ! grep -Fq 'def execution_lane_lock' \
    "$root/tests/integration/ntcp2/harness/plan060.py"; then
  echo "Plan 060 execution-lane lock helper is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan060.py"; then
  echo "Plan 060 test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'RetiredAndSupersededMarkerTests' \
    "$root/tests/integration/ntcp2/harness/test_plan060.py"; then
  echo "Plan 060 test matrix must include the retired/superseded markers" >&2
  exit 1
fi
if ! grep -Fq 'ExecutionLaneTests' \
    "$root/tests/integration/ntcp2/harness/test_plan060.py"; then
  echo "Plan 060 test matrix must include the execution-lane cases" >&2
  exit 1
fi
if ! grep -Fq 'TwoBundlePositiveFixtureTests' \
    "$root/tests/integration/ntcp2/harness/test_plan060.py"; then
  echo "Plan 060 test matrix must include the two-bundle positive fixture" >&2
  exit 1
fi
if ! grep -Fq 'FreezeReadinessTests' \
    "$root/tests/integration/ntcp2/harness/test_plan060.py"; then
  echo "Plan 060 test matrix must include the freeze-readiness cases" >&2
  exit 1
fi
if ! grep -Fq 'test_plan060.py' "$root/AGENTS.md"; then
  echo "Plan 060 test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! test -f "$root/plans/060-candidate.md"; then
  echo "Plan 060 candidate record is missing" >&2
  exit 1
fi
if ! grep -Eq 'declared-not-executable|i2pr-interop-candidate-v1' \
    "$root/plans/060-candidate.md"; then
  echo "Plan 060 candidate must declare declared-not-executable status or embed a candidate record" >&2
  exit 1
fi
if ! grep -Fq 'blocked_execution_lane_unavailable' \
    "$root/plans/060-candidate.md"; then
  echo "Plan 060 candidate must carry the typed blocker marker" >&2
  exit 1
fi
if ! test -f "$root/plans/060-closure.md"; then
  echo "Plan 060 closure record is missing" >&2
  exit 1
fi
if ! grep -Eq 'declared-not-executable|blocked_execution_lane_unavailable' \
    "$root/plans/060-closure.md"; then
  echo "Plan 060 closure must record the typed blocker and the close-status" >&2
  exit 1
fi
if ! grep -Fq 'Plan 060' "$root/AGENTS.md"; then
  echo "AGENTS.md must record the Plan 060 closure section" >&2
  exit 1
fi

# Plan 062: NTCP2 evidence-contract and architecture correction. The
# v4 trigger schema, the reference-event v1 schema, the v3 observation
# schema, ADR 0022, the Plan 062 source-verification record, the
# Plan 062 test matrix, and the Plan 060 retirement marker must all
# be committed. Active code must not carry the 40-hex SHA-1 Router
# Hash width; the v3 trigger schema must remain a bounded
# historical-reader path.
if ! test -f "$root/tests/integration/ntcp2/harness/reference_trigger_v4.py"; then
  echo "Plan 062 v4 trigger schema module is missing" >&2
  exit 1
fi
if ! grep -Fq 'TRIGGER_SCHEMA = "i2pr-reference-trigger-v4"' \
    "$root/tests/integration/ntcp2/harness/reference_trigger_v4.py"; then
  echo "Plan 062 v4 trigger schema is missing or wrong version" >&2
  exit 1
fi
if ! grep -Fq '"i2pr-reference-trigger-v4"' \
    "$root/tests/integration/ntcp2/harness/evidence_bundle.py"; then
  echo "Plan 062 v4 trigger schema is not allowlisted in evidence bundle" >&2
  exit 1
fi
if ! grep -Fq 'delivery_status_message_id' \
    "$root/tests/integration/ntcp2/harness/reference_trigger_v4.py"; then
  echo "Plan 062 v4 trigger schema is missing the per-run DeliveryStatus message ID" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/reference_event.py"; then
  echo "Plan 062 reference event schema module is missing" >&2
  exit 1
fi
if ! grep -Fq 'EVENT_SCHEMA = "i2pr-reference-event-v1"' \
    "$root/tests/integration/ntcp2/harness/reference_event.py"; then
  echo "Plan 062 reference event schema is missing or wrong version" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/observation_v3.py"; then
  echo "Plan 062 v3 observation schema module is missing" >&2
  exit 1
fi
if ! grep -Fq 'OBSERVATION_SCHEMA = "i2pr-ntcp2-direction-observation-v3"' \
    "$root/tests/integration/ntcp2/harness/observation_v3.py"; then
  echo "Plan 062 v3 observation schema is missing or wrong version" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-ntcp2-direction-observation-v3' \
    "$root/tests/integration/ntcp2/harness/evidence_bundle.py"; then
  echo "Plan 062 v3 observation schema is not allowlisted in evidence bundle" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/reference-drivers/source-verification.md"; then
  echo "Plan 062 source-verification record is missing" >&2
  exit 1
fi
if ! test -f "$root/docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md"; then
  echo "Plan 062 ADR 0022 (direct reference router NTCP2 interop drivers) is missing" >&2
  exit 1
fi
if ! grep -Eq '^- Status:\s*Accepted\b' \
    "$root/docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md"; then
  echo "Plan 062 ADR 0022 must be Accepted after source verification" >&2
  exit 1
fi
if ! grep -Fq 'Plan 062' "$root/docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md"; then
  echo "ADR 0022 must reference Plan 062" >&2
  exit 1
fi
if ! grep -Fq 'Plan 062' "$root/plans/061-ntcp2-direct-reference-driver-corrective-roadmap.md"; then
  echo "Plan 061 must reference Plan 062" >&2
  exit 1
fi
if ! test -f "$root/plans/062-ntcp2-evidence-contract-and-architecture-correction.md"; then
  echo "Plan 062 plan-of-record is missing" >&2
  exit 1
fi
if ! test -f "$root/plans/063-java-i2p-stripped-router-direct-ntcp2-driver.md"; then
  echo "Plan 063 plan-of-record is missing" >&2
  exit 1
fi
if ! test -f "$root/plans/064-i2pd-direct-ntcp2-driver-and-observer-correction.md"; then
  echo "Plan 064 plan-of-record is missing" >&2
  exit 1
fi
if ! test -f "$root/plans/065-ntcp2-canonical-integration-and-live-qualification.md"; then
  echo "Plan 065 plan-of-record is missing" >&2
  exit 1
fi
if ! test -f "$root/plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md"; then
  echo "Plan 066 plan-of-record is missing" >&2
  exit 1
fi
if ! grep -Eq 'Status:.*retired|Status:\*\*retired' "$root/plans/060-candidate.md"; then
  echo "Plan 060 candidate must be retired by Plan 062" >&2
  exit 1
fi
if ! grep -Fqi 'retired by Plan 062' "$root/plans/060-candidate.md"; then
  echo "Plan 060 candidate must carry the explicit Plan 062 retirement marker" >&2
  exit 1
fi
if ! grep -Fq 'Superseded by Plan 062' "$root/plans/060-closure.md"; then
  echo "Plan 060 closure must carry the Plan 062 supersession marker" >&2
  exit 1
fi
# Plan 062: the active v4 trigger schema must use 64-hex Router Hash
# fields. The historical v3 trigger schema remains readable but is
# the bounded historical-reader path.
if grep -nE 're\.compile\(r"\^\\[0-9a-f\\]\\{40\\}\\$"\)|_HEX40' \
    "$root/tests/integration/ntcp2/harness/reference_trigger_v4.py"; then
  echo "Plan 062 v4 trigger schema must not use 40-hex Router Hash width" >&2
  exit 1
fi
if grep -nE 're\.compile\(r"\^\\[0-9a-f\\]\\{40\\}\\$"\)|_HEX40' \
    "$root/tests/integration/ntcp2/harness/observation_v3.py"; then
  echo "Plan 062 v3 observation schema must not use 40-hex Router Hash width" >&2
  exit 1
fi
if grep -nE 're\.compile\(r"\^\\[0-9a-f\\]\\{40\\}\\$"\)|_HEX40' \
    "$root/tests/integration/ntcp2/harness/reference_event.py"; then
  echo "Plan 062 reference event schema must not use 40-hex Router Hash width" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_reference_trigger_v4.py"; then
  echo "Plan 062 v4 trigger schema test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_reference_event.py"; then
  echo "Plan 062 reference event schema test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_observation_v3.py"; then
  echo "Plan 062 v3 observation schema test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan062.py"; then
  echo "Plan 062 plan matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'test_plan062.py' "$root/AGENTS.md"; then
  echo "Plan 062 test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! grep -Fq 'test_reference_trigger_v4.py' "$root/AGENTS.md"; then
  echo "Plan 062 v4 trigger schema test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! grep -Fq 'test_reference_event.py' "$root/AGENTS.md"; then
  echo "Plan 062 reference event schema test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! grep -Fq 'test_observation_v3.py' "$root/AGENTS.md"; then
  echo "Plan 062 v3 observation schema test matrix is not wired into AGENTS.md" >&2
  exit 1
fi

# Plan 063: Java I2P stripped-router direct NTCP2 driver. The Java
# direct helper source, the source-lock record, the classpath
# manifest, the build-manifest schema, the build/run scripts, the
# Python adapter, the test matrix, and the qualification receipt must
# all be committed and the locked 64-hex SHA-256 Router Hash contract
# must remain active.
java_helper_dir="$root/tests/integration/ntcp2/reference-drivers/java"
if ! test -f "$java_helper_dir/src/JavaNtcp2InteropDriver.java"; then
  echo "Plan 063 Java direct driver source is missing" >&2
  exit 1
fi
if ! test -f "$java_helper_dir/source-lock.json"; then
  echo "Plan 063 Java direct driver source-lock record is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-java-helper-source-lock-v1' "$java_helper_dir/source-lock.json"; then
  echo "Plan 063 Java helper source-lock schema marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'java-direct-helper' "$java_helper_dir/source-lock.json"; then
  echo "Plan 063 Java direct helper kind marker is missing" >&2
  exit 1
fi
if ! grep -Fq '2800040deee9bb376567b671ef2e9c34cf3e30b6' "$java_helper_dir/source-lock.json"; then
  echo "Plan 063 Java direct helper pinned revision marker is missing" >&2
  exit 1
fi
if ! test -f "$java_helper_dir/classpath-manifest.json"; then
  echo "Plan 063 Java direct driver classpath manifest is missing" >&2
  exit 1
fi
if ! test -f "$java_helper_dir/build-manifest.schema.json"; then
  echo "Plan 063 Java direct driver build-manifest schema is missing" >&2
  exit 1
fi
if ! test -f "$java_helper_dir/build-driver.sh"; then
  echo "Plan 063 Java direct driver build script is missing" >&2
  exit 1
fi
if ! test -f "$java_helper_dir/run-driver.sh"; then
  echo "Plan 063 Java direct driver run script is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/java_direct_driver.py"; then
  echo "Plan 063 Java direct driver Python adapter is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_java_direct_driver.py"; then
  echo "Plan 063 Java direct driver test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_java_direct_control.py"; then
  echo "Plan 063 Java direct driver control test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'JavaHelperArtifactsPresentTests' "$root/tests/integration/ntcp2/harness/test_java_direct_driver.py"; then
  echo "Plan 063 Java helper artifact tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'JavaStrictConfigValidationTests' "$root/tests/integration/ntcp2/harness/test_java_direct_driver.py"; then
  echo "Plan 063 strict config validation tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'ControlTopologyContractTests' "$root/tests/integration/ntcp2/harness/test_java_direct_control.py"; then
  echo "Plan 063 control topology contract tests are missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/qualification/java-direct-driver.json"; then
  echo "Plan 063 Java direct driver qualification receipt is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-java-direct-driver-qualification-v1' "$root/tests/integration/ntcp2/qualification/java-direct-driver.json"; then
  echo "Plan 063 Java direct driver qualification receipt schema marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'java-direct-driver' "$java_helper_dir/src/JavaNtcp2InteropDriver.java"; then
  echo "Plan 063 Java direct driver implementation marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-reference-event-v1' "$java_helper_dir/src/JavaNtcp2InteropDriver.java"; then
  echo "Plan 063 Java direct driver reference-event v1 marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-reference-trigger-v4' "$root/tests/integration/ntcp2/harness/java_direct_driver.py"; then
  echo "Plan 063 Java direct driver Python adapter v4 trigger binding is missing" >&2
  exit 1
fi
if ! grep -Fq 'java-direct-driver' "$root/tests/integration/ntcp2/harness/java_direct_driver.py"; then
  echo "Plan 063 Java direct driver Python adapter implementation marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'Plan 063' "$root/AGENTS.md"; then
  echo "AGENTS.md must record the Plan 063 closure section" >&2
  exit 1
fi
if ! grep -Fq 'test_java_direct_driver.py' "$root/AGENTS.md"; then
  echo "Plan 063 Java direct driver test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! grep -Fq 'test_java_direct_control.py' "$root/AGENTS.md"; then
  echo "Plan 063 Java direct driver control test matrix is not wired into AGENTS.md" >&2
  exit 1
fi

# Plan 064: i2pd direct NTCP2 driver. The Plan 064 driver source, the
# observer header, the observer source, the observer patch, the
# source-lock record, the build-manifest schema, the build/run
# scripts, the CMakeLists, the Python adapter, the test matrices, and
# the qualification receipt must all be committed. The driver is
# source-locked to the pinned i2pd 2.60.0 revision; the 64-hex SHA-256
# Router Hash contract remains active.
i2pd_helper_dir="$root/tests/integration/ntcp2/reference-drivers/i2pd"
if ! test -f "$i2pd_helper_dir/src/i2pd_ntcp2_interop_driver.cpp"; then
  echo "Plan 064 i2pd direct driver source is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pd-direct-driver' "$i2pd_helper_dir/src/i2pd_ntcp2_interop_driver.cpp"; then
  echo "Plan 064 i2pd direct driver implementation marker is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/src/interop_observer.h"; then
  echo "Plan 064 i2pd observer header is missing" >&2
  exit 1
fi
if ! grep -Fq 'I2PD_INTEROP_OBSERVER' "$i2pd_helper_dir/src/interop_observer.h"; then
  echo "Plan 064 i2pd observer header macro gate is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/src/interop_observer.cpp"; then
  echo "Plan 064 i2pd observer source is missing" >&2
  exit 1
fi
if ! grep -Fq 'I2PD_INTEROP_OBSERVER' "$i2pd_helper_dir/src/interop_observer.cpp"; then
  echo "Plan 064 i2pd observer source macro gate is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/patches/i2pd-2.60.0-interop-observer.patch"; then
  echo "Plan 064 i2pd observer patch is missing" >&2
  exit 1
fi
if ! grep -Fq 'I2PD_INTEROP_OBSERVER' "$i2pd_helper_dir/patches/i2pd-2.60.0-interop-observer.patch"; then
  echo "Plan 064 i2pd observer patch macro gate is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/source-lock.json"; then
  echo "Plan 064 i2pd direct driver source-lock record is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-i2pd-direct-driver-source-lock-v1' "$i2pd_helper_dir/source-lock.json"; then
  echo "Plan 064 i2pd direct driver source-lock schema marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pd-direct-driver' "$i2pd_helper_dir/source-lock.json"; then
  echo "Plan 064 i2pd direct driver helper-kind marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e' "$i2pd_helper_dir/source-lock.json"; then
  echo "Plan 064 i2pd direct driver pinned revision marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-reference-trigger-v4' "$i2pd_helper_dir/source-lock.json"; then
  echo "Plan 064 i2pd direct driver v4 trigger binding marker is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/build-manifest.schema.json"; then
  echo "Plan 064 i2pd direct driver build-manifest schema is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-i2pd-direct-driver-build-manifest-v1' "$i2pd_helper_dir/build-manifest.schema.json"; then
  echo "Plan 064 i2pd direct driver build-manifest schema marker is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/CMakeLists.txt"; then
  echo "Plan 064 i2pd direct driver CMake build contract is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/build-driver.sh"; then
  echo "Plan 064 i2pd direct driver build script is missing" >&2
  exit 1
fi
if ! test -f "$i2pd_helper_dir/run-driver.sh"; then
  echo "Plan 064 i2pd direct driver run script is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/i2pd_direct_driver.py"; then
  echo "Plan 064 i2pd direct driver Python adapter is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-reference-trigger-v4' "$root/tests/integration/ntcp2/harness/i2pd_direct_driver.py"; then
  echo "Plan 064 i2pd direct driver Python adapter v4 trigger binding is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pd-direct-driver' "$root/tests/integration/ntcp2/harness/i2pd_direct_driver.py"; then
  echo "Plan 064 i2pd direct driver Python adapter implementation marker is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_i2pd_direct_driver.py"; then
  echo "Plan 064 i2pd direct driver test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'I2pdDriverArtifactsPresentTests' "$root/tests/integration/ntcp2/harness/test_i2pd_direct_driver.py"; then
  echo "Plan 064 i2pd direct driver artifact tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'I2pdStrictConfigValidationTests' "$root/tests/integration/ntcp2/harness/test_i2pd_direct_driver.py"; then
  echo "Plan 064 i2pd direct driver strict config validation tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'I2pdStructuredEventContractTests' "$root/tests/integration/ntcp2/harness/test_i2pd_direct_driver.py"; then
  echo "Plan 064 i2pd direct driver structured event contract tests are missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_i2pd_direct_control.py"; then
  echo "Plan 064 i2pd direct driver control test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'ControlTopologyContractTests' "$root/tests/integration/ntcp2/harness/test_i2pd_direct_control.py"; then
  echo "Plan 064 i2pd direct driver control topology contract tests are missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/qualification/i2pd-direct-driver.json"; then
  echo "Plan 064 i2pd direct driver qualification receipt is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-i2pd-direct-driver-qualification-v1' "$root/tests/integration/ntcp2/qualification/i2pd-direct-driver.json"; then
  echo "Plan 064 i2pd direct driver qualification receipt schema marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'Plan 064' "$root/AGENTS.md"; then
  echo "AGENTS.md must record the Plan 064 closure section" >&2
  exit 1
fi
if ! grep -Fq 'test_i2pd_direct_driver.py' "$root/AGENTS.md"; then
  echo "Plan 064 i2pd direct driver test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! grep -Fq 'test_i2pd_direct_control.py' "$root/AGENTS.md"; then
  echo "Plan 064 i2pd direct driver control test matrix is not wired into AGENTS.md" >&2
  exit 1
fi

# Plan 065: NTCP2 canonical integration and live qualification. The
# strict launcher scenario schema must be bumped to v2 with the
# per-run DeliveryStatus message_id, expected 64-hex Router Hashes,
# reference_driver_mode, and run_identity_sha256 fields. The active
# primary code must not hard-code a DeliveryStatus authority, must
# not use a type-only DeliveryStatus success path, must not select
# SAM/HTTP/I2PControl/support-topology helpers for a primary
# direction, must not use a 40-hex Router Hash, must not rely on
# a generic phrase catalog as sole receiver evidence, must not reuse
# the retired Plan 060 candidate as the active candidate, and must
# not let a synthetic fallback reach the `passed` outcome.
if ! grep -Fq 'i2pr-launcher-scenario-v2' "$root/tools/i2pr-interop/src/scenario.rs"; then
  echo "Plan 065 strict launcher scenario schema v2 marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'delivery_status_message_id' "$root/tools/i2pr-interop/src/scenario.rs"; then
  echo "Plan 065 strict launcher scenario schema v2 must require delivery_status_message_id" >&2
  exit 1
fi
if ! grep -Fq 'expected_sender_router_hash_sha256' "$root/tools/i2pr-interop/src/scenario.rs"; then
  echo "Plan 065 strict launcher scenario schema v2 must require expected_sender_router_hash_sha256" >&2
  exit 1
fi
if ! grep -Fq 'expected_receiver_router_hash_sha256' "$root/tools/i2pr-interop/src/scenario.rs"; then
  echo "Plan 065 strict launcher scenario schema v2 must require expected_receiver_router_hash_sha256" >&2
  exit 1
fi
if ! grep -Fq 'reference_driver_mode' "$root/tools/i2pr-interop/src/scenario.rs"; then
  echo "Plan 065 strict launcher scenario schema v2 must require reference_driver_mode" >&2
  exit 1
fi
if ! grep -Fq 'run_identity_sha256' "$root/tools/i2pr-interop/src/scenario.rs"; then
  echo "Plan 065 strict launcher scenario schema v2 must require run_identity_sha256" >&2
  exit 1
fi
if ! grep -Fq 'SenderDeliveryStatusMessageIdZero' "$root/tools/i2pr-interop/src/main.rs"; then
  echo "Plan 065 bounded sender DeliveryStatusMessageIdZero reason is missing" >&2
  exit 1
fi
if ! grep -Fq 'ReceiverDeliveryStatusIdMismatch' "$root/tools/i2pr-interop/src/main.rs"; then
  echo "Plan 065 bounded receiver DeliveryStatusIdMismatch reason is missing" >&2
  exit 1
fi
if ! grep -Fq 'ReceiverDeliveryStatusMissing' "$root/tools/i2pr-interop/src/main.rs"; then
  echo "Plan 065 bounded receiver DeliveryStatusMissing reason is missing" >&2
  exit 1
fi
if ! grep -Fq 'ReceiverDeliveryStatusDuplicate' "$root/tools/i2pr-interop/src/main.rs"; then
  echo "Plan 065 bounded receiver DeliveryStatusDuplicate reason is missing" >&2
  exit 1
fi
if grep -Fq 'message_id = 0x0420_0001' "$root/tools/i2pr-interop/src/main.rs"; then
  echo "Plan 065 forbids the hard-coded 0x0420_0001 DeliveryStatus authority" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-launcher-scenario-v2' "$root/tests/integration/ntcp2/harness/launcher_protocol.py"; then
  echo "Plan 065 Python launcher scenario schema v2 marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'delivery_status_message_id' "$root/tests/integration/ntcp2/harness/launcher_protocol.py"; then
  echo "Plan 065 Python launcher scenario schema v2 must require delivery_status_message_id" >&2
  exit 1
fi
if ! grep -Fq 'expected_sender_router_hash_sha256' "$root/tests/integration/ntcp2/harness/launcher_protocol.py"; then
  echo "Plan 065 Python launcher scenario schema v2 must require expected_sender_router_hash_sha256" >&2
  exit 1
fi
if ! grep -Fq 'reference_driver_mode' "$root/tests/integration/ntcp2/harness/launcher_protocol.py"; then
  echo "Plan 065 Python launcher scenario schema v2 must require reference_driver_mode" >&2
  exit 1
fi
if ! grep -Fq 'REFERENCE_DRIVER_MODE_BY_DIRECTION' "$root/tests/integration/ntcp2/harness/launcher_protocol.py"; then
  echo "Plan 065 Python launcher scenario direction-to-helper map is missing" >&2
  exit 1
fi
if ! grep -Fq 'REFERENCE_DRIVER_MODES' "$root/tests/integration/ntcp2/harness/launcher_renderer.py"; then
  echo "Plan 065 Python renderer reference_driver_mode allowlist is missing" >&2
  exit 1
fi
if ! grep -Fq '_plan065_primary_fields' "$root/tests/integration/ntcp2/harness/mixed_runner.py"; then
  echo "Plan 065 canonical mixed-runner primary-fields helper is missing" >&2
  exit 1
fi
if ! grep -Fq '_reference_driver_mode_for' "$root/tests/integration/ntcp2/harness/mixed_runner.py"; then
  echo "Plan 065 canonical mixed-runner reference-driver-mode helper is missing" >&2
  exit 1
fi
if grep -Fq '"reference" not in {"java_i2p", "i2pd"}' "$root/tests/integration/ntcp2/harness/mixed_runner.py"; then
  : # legacy line tolerated
fi
if grep -E '^[^#]*"sam-trigger"' "$root/tests/integration/ntcp2/harness/mixed_runner.py" >/dev/null; then
  echo "Plan 065 canonical mixed-runner must not select sam-trigger helpers" >&2
  exit 1
fi
if grep -E '^[^#]*"http-trigger"' "$root/tests/integration/ntcp2/harness/mixed_runner.py" >/dev/null; then
  echo "Plan 065 canonical mixed-runner must not select http-trigger helpers" >&2
  exit 1
fi
if grep -E '^[^#]*"support-topology"' "$root/tests/integration/ntcp2/harness/mixed_runner.py" >/dev/null; then
  echo "Plan 065 canonical mixed-runner must not select support-topology helpers" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan065.py"; then
  echo "Plan 065 test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'DeliveryStatusMessageIdDerivationTests' "$root/tests/integration/ntcp2/harness/test_plan065.py"; then
  echo "Plan 065 DeliveryStatus message ID derivation tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'PassPredicateTests' "$root/tests/integration/ntcp2/harness/test_plan065.py"; then
  echo "Plan 065 pass predicate tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'ReferenceTriggerCorrelationTests' "$root/tests/integration/ntcp2/harness/test_plan065.py"; then
  echo "Plan 065 reference trigger correlation tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'SupportRouterRejectionTests' "$root/tests/integration/ntcp2/harness/test_plan065.py"; then
  echo "Plan 065 support-router rejection tests are missing" >&2
  exit 1
fi
if ! grep -Fq 'Plan 065' "$root/AGENTS.md"; then
  echo "AGENTS.md must record the Plan 065 closure section" >&2
  exit 1
fi
if ! grep -Fq 'test_plan065.py' "$root/AGENTS.md"; then
  echo "Plan 065 test matrix is not wired into AGENTS.md" >&2
  exit 1
fi

# Plan 066: fresh candidate and authoritative NTCP2 two-run closure
# pass. The Plan 066 helper module, the Plan 066 test matrix, the
# typed blocker, the close-status classifier, the execution-lane
# lock helper, and the candidate/closure marker invariants must be
# committed; AGENTS.md must record the Plan 066 closure section and
# wire the Plan 066 test matrix; the support rustdoc build must
# remain warning-clean.
if ! test -f "$root/tests/integration/ntcp2/harness/plan066.py"; then
  echo "Plan 066 helper module is missing" >&2
  exit 1
fi
if ! grep -Fq 'TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE' \
    "$root/tests/integration/ntcp2/harness/plan066.py"; then
  echo "Plan 066 typed blocker marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'blocked_execution_lane_unavailable' \
    "$root/tests/integration/ntcp2/harness/plan066.py"; then
  echo "Plan 066 typed blocker string marker is missing" >&2
  exit 1
fi
if ! grep -Fq 'def plan066_close_status' \
    "$root/tests/integration/ntcp2/harness/plan066.py"; then
  echo "Plan 066 close-status classifier is missing" >&2
  exit 1
fi
if ! grep -Fq 'def plan066_execution_lane_lock' \
    "$root/tests/integration/ntcp2/harness/plan066.py"; then
  echo "Plan 066 execution-lane lock helper is missing" >&2
  exit 1
fi
if ! grep -Fq 'def plan066_freeze_readiness_report' \
    "$root/tests/integration/ntcp2/harness/plan066.py"; then
  echo "Plan 066 freeze-readiness helper is missing" >&2
  exit 1
fi
if ! grep -Fq 'def plan066_two_bundle_independence' \
    "$root/tests/integration/ntcp2/harness/plan066.py"; then
  echo "Plan 066 two-bundle independence helper is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan066.py"; then
  echo "Plan 066 test matrix is missing" >&2
  exit 1
fi
if ! grep -Fq 'FreezeReadinessTests' \
    "$root/tests/integration/ntcp2/harness/test_plan066.py"; then
  echo "Plan 066 test matrix must include the freeze-readiness cases" >&2
  exit 1
fi
if ! grep -Fq 'TwoBundlePositiveFixtureTests' \
    "$root/tests/integration/ntcp2/harness/test_plan066.py"; then
  echo "Plan 066 test matrix must include the two-bundle positive fixture" >&2
  exit 1
fi
if ! grep -Fq 'PrerequisiteAndAdrTests' \
    "$root/tests/integration/ntcp2/harness/test_plan066.py"; then
  echo "Plan 066 test matrix must include the prerequisite and ADR cases" >&2
  exit 1
fi
if ! grep -Fq 'DirectionOrderIndependenceTests' \
    "$root/tests/integration/ntcp2/harness/test_plan066.py"; then
  echo "Plan 066 test matrix must include the direction-order independence cases" >&2
  exit 1
fi
if ! grep -Fq 'Plan066TypedBlockerTests' \
    "$root/tests/integration/ntcp2/harness/test_plan066.py"; then
  echo "Plan 066 test matrix must include the typed blocker / close-status cases" >&2
  exit 1
fi
if ! test -f "$root/plans/066-candidate.md"; then
  echo "Plan 066 candidate record is missing" >&2
  exit 1
fi
if ! grep -Eq 'declared-not-executable|i2pr-interop-candidate-v1' \
    "$root/plans/066-candidate.md"; then
  echo "Plan 066 candidate must declare declared-not-executable status or embed a candidate record" >&2
  exit 1
fi
if ! grep -Fq 'blocked_execution_lane_unavailable' \
    "$root/plans/066-candidate.md"; then
  echo "Plan 066 candidate must carry the typed blocker marker" >&2
  exit 1
fi
if ! test -f "$root/plans/066-closure.md"; then
  echo "Plan 066 closure record is missing" >&2
  exit 1
fi
if ! grep -Eq 'declared-not-executable|blocked_execution_lane_unavailable' \
    "$root/plans/066-closure.md"; then
  echo "Plan 066 closure must record the typed blocker and the close-status" >&2
  exit 1
fi
if ! grep -Fq 'Plan 066' "$root/AGENTS.md"; then
  echo "AGENTS.md must record the Plan 066 closure section" >&2
  exit 1
fi
if ! grep -Fq 'test_plan066.py' "$root/AGENTS.md"; then
  echo "Plan 066 test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! grep -Fq 'Plan 066' "$root/.opencode/skills/i2pr-ntcp2-interop/SKILL.md"; then
  echo "i2pr-ntcp2-interop skill must record the Plan 066 section" >&2
  exit 1
fi

python3 "$root/scripts/interop/validate-evidence.py"
python3 "$root/scripts/interop/validate-scenarios.py"

# Plan 068: staged evidence tier separation. The evidence-tier module, the
# smoke record schema, and the development validation summary schema must be
# committed, and the smoke/development schemas must never be accepted inside a
# release bundle.
if ! test -f "$root/tests/integration/ntcp2/harness/evidence_tier.py"; then
  echo "Plan 068 evidence_tier module is missing" >&2
  exit 1
fi
if ! grep -Eq 'RELEASE_QUALIFICATION[[:space:]]*:[[:space:]]*Final\[str\][[:space:]]*=[[:space:]]*"release-qualification"|RELEASE_QUALIFICATION[[:space:]]*=[[:space:]]*"release-qualification"' \
    "$root/tests/integration/ntcp2/harness/evidence_tier.py"; then
  echo "Plan 068 evidence_tier release-qualification marker is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/loopback_smoke_record.py"; then
  echo "Plan 068 smoke record schema module is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-ntcp2-loopback-smoke-v1' \
    "$root/tests/integration/ntcp2/harness/loopback_smoke_record.py"; then
  echo "Plan 068 smoke record schema marker is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/development_validation.py"; then
  echo "Plan 068 development validation summary schema module is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-ntcp2-development-validation-v1' \
    "$root/tests/integration/ntcp2/harness/development_validation.py"; then
  echo "Plan 068 development validation summary schema marker is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_evidence_tier.py"; then
  echo "Plan 068 evidence_tier test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_loopback_smoke_record.py"; then
  echo "Plan 068 smoke record schema test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_development_validation.py"; then
  echo "Plan 068 development validation summary schema test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/docs/adr/0023-staged-ntcp2-interoperability-evidence.md"; then
  echo "Plan 068 ADR 0023 (staged NTCP2 interoperability evidence) is missing" >&2
  exit 1
fi
if ! grep -Eq '^- Status:\s*Accepted\b' \
    "$root/docs/adr/0023-staged-ntcp2-interoperability-evidence.md"; then
  echo "Plan 068 ADR 0023 must be Accepted after tests pass" >&2
  exit 1
fi
if grep -Eq '^- Status:\s*Rejected\b' \
    "$root/docs/adr/0023-staged-ntcp2-interoperability-evidence.md"; then
  echo "Plan 068 ADR 0023 cannot also be Rejected" >&2
  exit 1
fi
if ! grep -Fq 'Plan 068' "$root/AGENTS.md"; then
  echo "AGENTS.md must record the Plan 068 section" >&2
  exit 1
fi
if ! grep -Fq 'Plan 068' "$root/.opencode/skills/i2pr-ntcp2-interop/SKILL.md"; then
  echo "i2pr-ntcp2-interop skill must record the Plan 068 section" >&2
  exit 1
fi
if ! grep -Fq 'Plan 068' "$root/docs/architecture/interop-apparatus.md"; then
  echo "docs/architecture/interop-apparatus.md must record the Plan 068 section" >&2
  exit 1
fi
if ! grep -Fq 'Plan 068' "$root/docs/protocol-support.md"; then
  echo "docs/protocol-support.md must record the Plan 068 section" >&2
  exit 1
fi
if ! grep -Fq 'Plan 068' "$root/README.md"; then
  echo "README.md must record the Plan 068 section" >&2
  exit 1
fi
# Plan 068 release-bundle smoke/development rejection. The release bundle
# validators must refuse smoke and development records. The Plan 066
# freeze-readiness table may continue to require historical plan surfaces but
# must not reference the smoke/development schemas inside the release-only
# v1-v3 bundle code paths.
if grep -Eq 'i2pr-ntcp2-loopback-smoke-v1|i2pr-ntcp2-development-validation-v1' \
    "$root/tests/integration/ntcp2/harness/evidence_bundle.py"; then
  echo "Plan 068 release bundle cannot reference the smoke/development schemas" >&2
  exit 1
fi

# Plan 069: host-compatible NTCP2 loopback smoke lane. The runner
# module, the shell entry point, the focused test matrix, and the
# static boundary checker must all be present and reference the
# expected markers. The runner must remain free of
# Plan 056/066 candidate/bundle/certificate/rootless/Multipass
# authority.
if ! test -f "$root/tests/integration/ntcp2/harness/loopback_smoke.py"; then
  echo "Plan 069 loopback smoke runner module is missing" >&2
  exit 1
fi
if ! test -f "$root/scripts/interop/run-ntcp2-loopback-smoke.sh"; then
  echo "Plan 069 shell entry point is missing" >&2
  exit 1
fi
if ! grep -Fq 'i2pr-ntcp2-loopback-smoke-v1' \
    "$root/tests/integration/ntcp2/harness/loopback_smoke.py"; then
  echo "Plan 069 loopback runner must reference the smoke record schema" >&2
  exit 1
fi
if ! grep -Fq 'Plan 069' "$root/AGENTS.md"; then
  echo "AGENTS.md must record the Plan 069 closure section" >&2
  exit 1
fi
if ! grep -Fq 'test_loopback_smoke.py' "$root/AGENTS.md"; then
  echo "Plan 069 test matrix is not wired into AGENTS.md" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_loopback_smoke.py"; then
  echo "Plan 069 test matrix is missing" >&2
  exit 1
fi
if ! test -f "$root/scripts/check-ntcp2-loopback-smoke-boundary.sh"; then
  echo "Plan 069 static boundary checker is missing" >&2
  exit 1
fi
bash "$root/scripts/check-ntcp2-loopback-smoke-boundary.sh"

# Plan 080: Multipass lane prequalification for Plan 078.  The plan doc,
# the status doc, the helper module, and the test matrix must be committed
# and the public surface must be present.
if ! test -f "$root/tests/integration/ntcp2/harness/plan080.py"; then
  echo "Plan 080 helper module is missing" >&2
  exit 1
fi
if ! test -s "$root/tests/integration/ntcp2/harness/plan080.py"; then
  echo "Plan 080 helper module is empty" >&2
  exit 1
fi
if ! test -f "$root/tests/integration/ntcp2/harness/test_plan080.py"; then
  echo "Plan 080 test matrix is missing" >&2
  exit 1
fi
if ! test -s "$root/tests/integration/ntcp2/harness/test_plan080.py"; then
  echo "Plan 080 test matrix is empty" >&2
  exit 1
fi
if ! (cd "$root/tests/integration/ntcp2/harness" && PYTHONPATH="$root/tests/integration/ntcp2/harness" python3 -c '
from plan080 import (
    SCHEMA, SCHEMA_VERSION, TYPED_BLOCKER,
    CLOSE_STATUS_IN_PROGRESS, CLOSE_STATUS_QUALIFIED, CLOSE_STATUS_BLOCKED,
    plan080_typed_blocker, plan080_close_status,
    plan080_lane_qualification_digest, plan080_guest_inspect_record,
    plan080_qualification_writer,
)
assert SCHEMA == "i2pr-plan080-closure-v1", f"SCHEMA={SCHEMA}"
assert SCHEMA_VERSION == 1, f"SCHEMA_VERSION={SCHEMA_VERSION}"
assert TYPED_BLOCKER == "blocked_execution_lane_unavailable", f"TYPED_BLOCKER={TYPED_BLOCKER}"
'); then
  echo "Plan 080 helper module public surface is missing or wrong" >&2
  exit 1
fi
if ! grep -q -- '--lane-from-guest' "$root/scripts/interop/probe-constrained-host-lanes.sh"; then
  echo "probe wrapper is missing --lane-from-guest flag" >&2
  exit 1
fi
if ! grep -q -- '--artifact-digest' "$root/scripts/interop/probe-constrained-host-lanes.sh"; then
  echo "probe wrapper is missing --artifact-digest flag" >&2
  exit 1
fi
if ! test -f "$root/plans/080-multipass-lane-prequalification-for-plan-078.md"; then
  echo "Plan 080 plan doc is missing" >&2
  exit 1
fi
if ! test -s "$root/plans/080-multipass-lane-prequalification-for-plan-078.md"; then
  echo "Plan 080 plan doc is empty" >&2
  exit 1
fi
if ! test -f "$root/plans/080-status.md"; then
  echo "Plan 080 status doc is missing" >&2
  exit 1
fi
# Verify the status line still carries an in-progress marker (do not require closed).
if grep -q '## Status' "$root/plans/080-status.md"; then
  if ! grep -Eq '(in-progress|in.progress|active|superseded|closed|blocked|retired)' "$root/plans/080-status.md"; then
    echo "Plan 080 status doc has a Status line but no recognized marker" >&2
    exit 1
  fi
fi

echo "NTCP2 interoperability manifest and sanitized evidence boundary are valid (${scenario_count} scenarios)."
