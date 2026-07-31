#!/usr/bin/env bash
# Static fail-closed checks for the Plan 069 host-compatible
# NTCP2 loopback smoke lane. The smoke runner must remain free of
# Plan 056/066 candidate/bundle/certificate/rootless/Multipass
# authority, must support only the two i2pd directions, must
# disable raw diagnostics, must require the smoke record schema,
# and must require the exact correlation fields.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

fail() {
  echo "Plan 069 loopback smoke boundary violation: $1" >&2
  exit 1
}

runner="$root/tests/integration/ntcp2/harness/loopback_smoke.py"
shell="$root/scripts/interop/run-ntcp2-loopback-smoke.sh"
tests="$root/tests/integration/ntcp2/harness/test_loopback_smoke.py"

for path in "$runner" "$shell" "$tests"; do
  [[ -f "$path" ]] || fail "missing required file: $path"
done

[[ -x "$shell" ]] || fail "shell wrapper must be executable (chmod +x): $shell"

# --- Required artifacts and markers ---

required_markers=(
  "SMOKE_SCHEMA"
  "loopback_smoke_record"
  "external-loopback-smoke"
  "delivery_status_message_id"
  "expected_sender_router_hash_sha256"
  "expected_receiver_router_hash_sha256"
  "run_identity_sha256"
  "i2pr-launcher-scenario-v2"
  "i2pr-i2pd-direct-driver-config-v1"
  "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
  "i2pd-direct-driver"
)
for needle in "${required_markers[@]}"; do
  if ! grep -Fq -- "$needle" "$runner"; then
    fail "missing required marker '$needle' in $runner"
  fi
done

# --- Prohibited imports/authority in the runner ---

forbidden_in_runner=(
  "verify_milestone3_certificate:release/certificate authority"
  "candidate_record:candidate authority"
  "plan060:Plan 060 authority"
  "plan066:Plan 066 authority"
  "rootless_topology:rootless authority"
  "rootless_supervisor:rootless authority"
  "multipass:Multipass authority"
  "evidence_bundle:release bundle authority"
  "export_acknowledgement:export authority"
  "raw-local:raw diagnostics must be forbidden"
)
for entry in "${forbidden_in_runner[@]}"; do
  IFS=':' read -r needle label <<<"$entry"
  if grep -Fq -- "$needle" "$runner"; then
    fail "$label: forbidden token '$needle' present in $runner"
  fi
done

forbidden_in_shell=(
  "sudo:privilege escalation"
  "ip netns:network namespace mutation"
  "setcap:capability mutation"
  "--privileged:privileged container"
  "--network host:host network access"
  "/var/run/docker.sock:docker socket access"
)
for entry in "${forbidden_in_shell[@]}"; do
  IFS=':' read -r needle label <<<"$entry"
  # The shell wrapper may mention ``sudo`` in a no-sudo intent
  # comment; we therefore only reject non-comment matches by
  # scanning line by line.
  while IFS= read -r line; do
    if [[ "$line" =~ ^[[:space:]]*[^#[:space:]].*${needle} ]]; then
      fail "$label: forbidden token '$needle' present in $shell (line: $line)"
    fi
  done < "$shell"
done

# --- Shell wrapper must be strict and self-locating ---

shell_text=$(cat "$shell")
[[ "$shell_text" == *"set -euo pipefail"* ]] || fail "shell wrapper is missing strict mode"
[[ "$shell_text" == *"exec python3"* ]] || fail "shell wrapper must exec the Python runner"

# --- Tests cover the required surface ---

required_test_markers=(
  "StrictConfigTests"
  "CliParsingTests"
  "HelperTests"
  "NetworkAuditTests"
  "RunnerOrchestrationTests"
  "RecordWriterTests"
  "IndependentGuaranteeTests"
  "StaticArtifactTests"
  "TypedBlockerTests"
  "CorrelationAuthorityTests"
  "FailureStagingTests"
)
for marker in "${required_test_markers[@]}"; do
  if ! grep -Fq -- "$marker" "$tests"; then
    fail "tests missing required class: $marker"
  fi
done

# --- Smoke record schema is referenced ---

[[ -f "$root/tests/integration/ntcp2/harness/loopback_smoke_record.py" ]] \
  || fail "loopback_smoke_record.py is missing"
grep -Fq 'i2pr-ntcp2-loopback-smoke-v1' \
  "$root/tests/integration/ntcp2/harness/loopback_smoke_record.py" \
  || fail "loopback_smoke_record.py is missing the v1 schema marker"

# --- Direction allowlist restricted to the two i2pd directions ---
if grep -Eq "i2pr-to-emissary-ipv4|i2pr-to-java-ipv4|java-to-i2pr-ipv4" "$runner"; then
  fail "runner must not allow Java or Emissary directions"
fi

# --- Plan 069 must be documented ---
[[ -f "$root/plans/069-host-compatible-ntcp2-loopback-smoke-lane.md" ]] \
  || fail "Plan 069 plan-of-record is missing"

echo "Plan 069 loopback smoke boundary checks passed"
