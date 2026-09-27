#!/usr/bin/env bash
# Plan 257 §15 — fail-closed evidence-integrity checker for the M11
# production self-reply qualification and external evidence
# completion lane. Retains every Plan 256 guard and adds the
# Plan 257 false-positive rejections (far-side, full-drain,
# session-close, typed bandwidth, exact cardinality, reply
# source-locks, two-attempt gate).
#
# Rejects the Plan 255 false-positive shapes at the source level:
#
# - bulk success-key emission gated only by a generic build
#   observation (`observed_build` fan-out);
# - role rows without a decoded typed role (`TransitHopRoleKind`);
# - Participant evidence when no second reference process/topology
#   is proven (`I2PD_B_DATADIR`, i2pd-B spawn + dial);
# - RI-loaded rows derived only from a file write (requires the
#   `NetDb::Load` count-line proof plus the functional
#   explicit-peer proof);
# - fallback NetDB paths derived from the evidence directory;
# - an unrelated fresh X25519 transit responder key
#   (`X25519PrivateKey::generate(` in the driver);
# - ownership rows emitted before the matching `next_inbound`
#   observation (every mandatory row flows through `record_row`
#   with an explicit epoch);
# - rejection/expiry/replay/cancel/session-close/restart rows
#   without dedicated epoch evidence;
# - retained secret/private-key material.
#
# Every mandatory row must flow through `record_guarded` (which gates
# on the actual command exit code) or `m11_row` (which additionally
# requires the row's own epoch-qualified sanitized driver evidence
# key in the driver-evidence TSV). No literal `record "<row>" passed`
# line for a required row is permitted.
#
# Usage: bash scripts/check-m11-transit-qualification-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh"
DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"
WORKFLOW="${REPO_ROOT}/.github/workflows/m11-transit-external.yml"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"

# Plan 256 §5-§11 + Plan 257 §8-12/J mandatory rows. Every label
# must be reachable through `record_guarded`, `m11_row`,
# `exact_row`, or `exact_lib_row` in the runner, and every
# `epoch/key` must be emitted through `record_row` in the driver.
GUARDED=(
  # Work package A — identity/key coherence (exact local tests)
  m11-i2pd-routeridentity-build-key-coherent
  m11-i2pd-routerinfo-hash-coherent
  m11-i2pd-ssu2-key-not-build-key
  m11-i2pd-no-independent-transit-responder-key
  # Work package B — exact NetDB owner + pin gate
  m11-i2pd-netdb-owner-exact
  m11-i2pd-pin-mismatch-fails
  # Work package D — typed ledger anti-fan-out units
  m11-i2pd-anti-fanout-unit
  m11-i2pd-role-separation-unit
  m11-i2pd-participant-topology-unit
  m11-i2pd-ownership-order-unit
  m11-i2pd-code30-unit
  m11-i2pd-replay-unit
  m11-i2pd-expiry-unit
  m11-i2pd-cancel-unit
  m11-i2pd-restart-unit
  m11-i2pd-epoch-uniqueness-unit
  m11-i2pd-checker-invariants-unit
  # Plan 257 §4 — reply-branch source locks
  m11-i2pd-obep-remote-reply-source-lock
  m11-i2pd-obep-local-ibgw-reply-source-lock
  m11-i2pd-b-endpoint-source-lock
  # Plan 257 work packages B/C/E/I — production unit rows
  m11-i2pd-obep-remote-reply-unit
  m11-i2pd-local-reply-bypass-unit
  m11-i2pd-reply-id-preservation-unit
  m11-i2pd-snapshot-dimensions-unit
  m11-i2pd-cardinality-plus-one-unit
  m11-i2pd-reject-zero-growth-unit
  m11-i2pd-bandwidth-absence-unit
  m11-i2pd-bandwidth-present-unit
  m11-i2pd-cancel-full-drain-unit
  m11-i2pd-session-close-ab-unit
  m11-i2pd-self-reply-rollback-unit
  # Plan 257 work package A — negative evidence rows
  m11-i2pd-farside-local-only-unit
  m11-i2pd-farside-binding-unit
  m11-i2pd-farside-creator-secondary-unit
  m11-i2pd-cancel-narrow-unit
  m11-i2pd-session-close-narrow-unit
  m11-i2pd-restart-narrow-unit
  m11-i2pd-two-pass-gate-unit
  m11-i2pd-cross-sha-unit
  m11-i2pd-no-merge-unit
  m11-i2pd-bandwidth-hardcoded-unit
  m11-i2pd-cardinality-receive-only-unit
  m11-i2pd-reply-source-lock-unit
  # Plan 257 work package D — exact cardinality per role
  m11-i2pd-obep-active-before
  m11-i2pd-obep-active-after
  m11-i2pd-obep-registration-delta
  m11-i2pd-obep-receive-id
  m11-i2pd-obep-pending-baseline
  m11-i2pd-ibgw-active-before
  m11-i2pd-ibgw-active-after
  m11-i2pd-ibgw-registration-delta
  m11-i2pd-ibgw-receive-id
  m11-i2pd-ibgw-pending-baseline
  m11-i2pd-participant-active-before
  m11-i2pd-participant-active-after
  m11-i2pd-participant-registration-delta
  m11-i2pd-participant-receive-id
  m11-i2pd-participant-pending-baseline
  # Plan 257 work package E — typed bandwidth per role + reject
  m11-i2pd-obep-bandwidth-request
  m11-i2pd-obep-bandwidth-reply
  m11-i2pd-obep-bandwidth-disposition-observed
  m11-i2pd-ibgw-bandwidth-request
  m11-i2pd-ibgw-bandwidth-reply
  m11-i2pd-ibgw-bandwidth-disposition-observed
  m11-i2pd-participant-bandwidth-request
  m11-i2pd-participant-bandwidth-reply
  m11-i2pd-participant-bandwidth-disposition-observed
  m11-i2pd-bandwidth-request
  m11-i2pd-bandwidth-reply
  m11-i2pd-bandwidth-disposition-observed
  # Plan 257 work package F — far-side rows
  m11-i2pd-participant-data-local-forward
  m11-i2pd-participant-data-next-tunnel
  m11-i2pd-participant-data-b-endpoint-observed
  m11-i2pd-participant-data-far-side-count
  # Plan 257 work package G — full cancel + session-close rows
  m11-i2pd-cancel-active-before
  m11-i2pd-cancel-active-after
  m11-i2pd-cancel-pending-after
  m11-i2pd-cancel-peer-index-after
  m11-i2pd-cancel-queued-work-after
  m11-i2pd-cancel-new-ingress-refused
  m11-i2pd-session-close-a-before
  m11-i2pd-session-close-b-before
  m11-i2pd-session-close-a-removed
  m11-i2pd-session-close-b-retained
  m11-i2pd-session-close-final-peer-baseline
  # Plan 257 work package H — real restart rows
  m11-i2pd-restart-old-owner-drained
  m11-i2pd-restart-new-owner-zero
  m11-i2pd-restart-sessions-reestablished
  m11-i2pd-restart-fresh-build-accepted
  m11-i2pd-restart-fresh-registration-delta
  m11-i2pd-restart-final-baseline
  # Foundation / gates
  m11-i2pd-live-owner-enabled
  m11-i2pd-no-direct-build-injection
  m11-i2pd-driver-exists
  m11-i2pd-driver-ignored-gated
  m11-i2pd-driver-missing-env-fails
  m11-i2pd-no-evidence-netdb-fallback
  # Work package B/C source locks
  m11-i2pd-source-pin
  m11-i2pd-source-clean
  m11-i2pd-short-build-source-lock
  m11-i2pd-explicit-peer-source-lock
  m11-i2pd-trusted-router-source-lock
  m11-i2pd-netdb-storage-source-lock
  m11-i2pd-select-explicit-source-lock
  m11-i2pd-sam-params-source-lock
  m11-i2pd-netdb-load-source-lock
  m11-i2pd-tunnel-maintenance-source-lock
  # Static gates
  m11-i2pd-runner-pinned
  m11-i2pd-runner-loopback
  m11-i2pd-runner-no-public-network
  m11-i2pd-workflow-exists
  m11-i2pd-evidence-no-secret
  # External bootstrap epochs (driver epoch keys)
  m11-i2pd-a-netdb-owner-exact
  m11-i2pd-b-netdb-owner-exact
  m11-i2pd-a-loaded-i2pr-ri
  m11-i2pd-b-loaded-i2pr-ri
  m11-i2pd-reference-knows-i2pr-ri
  m11-i2pd-live-next-inbound-observed
  m11-i2pd-live-owner-enabled
  m11-i2pd-authenticated-peer-bound
  m11-i2pd-no-direct-build-injection
  m11-i2pd-session-close-reconciled
  m11-i2pd-b-provisioned
  # Role matrix epochs
  m11-i2pd-obep-build-received
  m11-i2pd-obep-build-accepted
  m11-i2pd-obep-registration-live
  m11-i2pd-ibgw-build-received
  m11-i2pd-ibgw-build-accepted
  m11-i2pd-ibgw-registration-live
  m11-i2pd-participant-build-received
  m11-i2pd-participant-build-accepted
  m11-i2pd-participant-registration-live
  # Data-plane epochs
  m11-i2pd-participant-data-forward
  m11-i2pd-participant-data-digest
  m11-i2pd-participant-creator-accepted
  m11-i2pd-obep-delivery
  m11-i2pd-obep-fragmented-once
  m11-i2pd-ibgw-gateway-ingress
  m11-i2pd-ibgw-multicell-bounded
  m11-i2pd-replay-no-second-delivery
  # Rejection / bandwidth epoch
  m11-i2pd-code30-build-rejected
  m11-i2pd-code30-no-registration
  m11-i2pd-code30-pending-baseline
  # Lifecycle epochs
  m11-i2pd-expiry-drops-live-data
  m11-i2pd-expiry-resource-baseline
  m11-i2pd-cancel-drains
  m11-i2pd-session-close-peer-baseline
  m11-i2pd-restart-clean-baseline
)

# Driver epoch keys: each must be emitted through `record_row` with
# its epoch, never from a generic branch.
EPOCH_KEYS=(
  "bootstrap/a-netdb-owner-exact"
  "bootstrap/b-netdb-owner-exact"
  "bootstrap/a-netdb-load-observed"
  "bootstrap/b-netdb-load-observed"
  "bootstrap/live-next-inbound-observed"
  "reject/build-rejected"
  "obep/build-accepted"
  "ibgw/build-accepted"
  "participant/build-accepted"
  "participant/next-hop-is-i2pd-b"
  "obep-data/delivery"
  "obep-data/fragmented-once"
  "ibgw-data/gateway-ingress"
  "ibgw-data/multicell-bounded"
  "participant-data/forward"
  "participant-data/creator-accepted"
  "participant-data/local-forward"
  "participant-data/next-tunnel"
  "participant-data/b-endpoint-observed"
  "participant-data/far-side-count"
  "replay/no-second-delivery"
  "expiry/drops-live-data"
  "expiry/resource-baseline"
  "cancel/drains"
  "cancel/active-before"
  "cancel/active-after"
  "cancel/pending-after"
  "cancel/peer-index-after"
  "cancel/queued-work-after"
  "cancel/new-ingress-refused"
  "session-close/peer-baseline"
  "session-close/a-before"
  "session-close/b-before"
  "session-close/a-removed"
  "session-close/b-retained"
  "session-close/final-peer-baseline"
  "restart/clean-baseline"
  "restart/old-owner-drained"
  "restart/new-owner-zero"
  "restart/sessions-reestablished"
  "restart/fresh-build-accepted"
  "restart/fresh-registration-delta"
  "restart/final-baseline"
)

failures=0
fail() {
  echo "check-m11-transit-qualification-evidence: $*" >&2
  failures=$((failures + 1))
}

# ---- Driver + runner + workflow must exist ------------------------------
for path in "${DRIVER}" "${HARNESS}" "${WORKFLOW}"; do
  if [[ ! -f "${path}" ]]; then
    fail "required Plan 256 surface missing: ${path}"
  fi
done

# ---- Runner must verify exact i2pd pin + version ------------------------
if ! grep -qF "${I2PD_PIN}" "${HARNESS}"; then
  fail "runner lost the exact i2pd pin ${I2PD_PIN}"
fi
if ! grep -qF "${I2PD_VERSION}" "${HARNESS}"; then
  fail "runner lost the exact i2pd version ${I2PD_VERSION}"
fi

# ---- Runner must stay loopback-only and reject public network -----------
if ! grep -qF '127.0.0.1' "${HARNESS}"; then
  fail "runner lost its loopback bind policy"
fi
# The driver writes both reference i2pd.conf files (single-owner
# lane), so the disabled-reseed proof lives in the driver; the
# runner must not introduce any public reseed URL either.
if ! grep -qF '[reseed]' "${DRIVER}"; then
  fail "driver must explicitly disable reseed in the reference configs"
fi
if grep -qE 'http://reseed|https://reseed|reseed\.i2p' "${DRIVER}" "${HARNESS}" | grep -v 'grep -qE' | grep -q .; then
  fail "driver/runner must not reference public reseed URLs"
fi
DRIVER_RESEED="$(grep -nA 4 '\[reseed\]' "${DRIVER}" | head -20 || true)"
if ! printf '%s\n' "${DRIVER_RESEED}" | grep -qF 'urls ='; then
  fail "driver reseed section must leave urls empty"
fi

# ---- Driver must be #[ignore]-gated and require exact i2pd env ----------
if ! grep -qE '#\[ignore\s*=.*Plan 256' "${DRIVER}"; then
  fail "driver must be #[ignore]-gated with the Plan 256 explanation"
fi
if ! grep -qE 'I2PD_A_DATADIR|I2PD_B_DATADIR|I2PR_SSU2_BIND' "${DRIVER}"; then
  fail "driver must require explicit reference datadirs and loopback bind"
fi
if grep -qE 'cargo test .*\|\| true' "${HARNESS}"; then
  fail "external driver invocation must not be forgiven with || true"
fi
if ! grep -qF -- '--ignored --exact' "${HARNESS}"; then
  fail "harness must invoke the external driver with --ignored --exact"
fi

# ---- Plan 256 corrective: no generic build fan-out ----------------------
# The `observed_build` boolean that fanned one dispatch into
# unrelated Plan 255 rows must not exist in the corrected driver
# as code. Doc/test mentions of the historical identifier (without
# an assignment or branch) are allowed so the corrective can name
# the defect it removes.
if grep -vE '^[[:space:]]*(//|//!|///)' "${DRIVER}" | grep -qE 'let[[:space:]]+(mut[[:space:]]+)?observed_build|observed_build[[:space:]]*=|if[[:space:]]+observed_build'; then
  fail "driver must not contain the generic observed_build fan-out"
fi

# ---- Plan 256 corrective: typed decoded roles ---------------------------
# Role rows must bind to the decoded TransitHopRoleKind carried by
# the live-owner evidence, recorded per epoch through record_row.
if ! grep -qE 'TransitHopRoleKind::(OutboundEndpoint|InboundGateway|Participant)' "${DRIVER}"; then
  fail "driver must bind role rows to typed TransitHopRoleKind evidence"
fi
if ! grep -qE 'fn record_row' "${DRIVER}"; then
  fail "driver must emit mandatory rows through epoch-qualified record_row"
fi
if ! grep -qE 'enum Epoch' "${DRIVER}"; then
  fail "driver must scope evidence by a typed epoch"
fi
for epoch in 'Epoch::Reject' 'Epoch::Expiry' 'Epoch::Replay' 'Epoch::Cancel' 'Epoch::Restart' 'Epoch::SessionClose' 'Epoch::Participant'; do
  if ! grep -qF "${epoch}" "${DRIVER}"; then
    fail "driver must execute a dedicated ${epoch} epoch"
  fi
done

# ---- Plan 256 corrective: second reference topology ----------------------
if ! grep -qF 'I2PD_B_DATADIR' "${HARNESS}"; then
  fail "runner must provision an explicit i2pd-B datadir"
fi
if ! grep -qE 'ReferenceProcess|i2pd-B' "${DRIVER}"; then
  fail "driver must own the second exact-pinned reference lifecycle"
fi
if ! grep -qE 'b_target|dial\(b_target' "${DRIVER}"; then
  fail "driver must establish an authenticated session to i2pd-B"
fi

# ---- Plan 256 corrective: exact NetDB owner, no fallback -----------------
if ! grep -qF 'routerInfo-' "${DRIVER}"; then
  fail "driver must write the source-locked routerInfo- NetDB filename"
fi
if grep -qF '../i2pd-a/data/netDb' "${DRIVER}"; then
  fail "driver must not derive a NetDB path from the evidence directory"
fi

# ---- Plan 256 corrective: coherent build responder key -------------------
# The only responder secret source is the bundle encryption key; a
# fresh X25519 responder key after the bundle exists is the exact
# Plan 255 defect. Lines that merely assert on the identifier
# (doc comments stripped separately, `contains(` self-checks) are
# allowed; an actual `generate(` call is not.
if grep -vE '^[[:space:]]*(//|//!|///)' "${DRIVER}" | grep -v 'contains(' | grep -qE 'X25519PrivateKey::generate\('; then
  fail "driver must not generate an independent transit responder key"
fi
if ! grep -qF 'encryption_key().secret_bytes()' "${DRIVER}"; then
  fail "driver must source the responder secret from the bundle encryption key"
fi

# ---- Plan 256 corrective: external flow must not hand-build STBMs --------
# Local fixture helpers (make_stbm_payload, build_*_record) exist for
# the non-environment unit rows only. The external `run_qualification`
# flow must consume authenticated inbound events, never synthesize
# build payloads or invoke build-crypto primitives.
QUAL_FLOW="$(mktemp)"
awk '/async fn run_qualification/,/^async fn drain_build_epoch/' "${DRIVER}" > "${QUAL_FLOW}"
for helper in 'make_stbm_payload' 'build_obep_record' 'build_ibgw_record' 'build_participant_record' 'seal_short_request' 'encode_standard_stbm'; do
  if grep -qF "${helper}" "${QUAL_FLOW}"; then
    fail "external run_qualification flow must not call local fixture helper ${helper}"
  fi
done
rm -f "${QUAL_FLOW}"

# ---- Every guarded row must flow through record_guarded or m11_row ------
for label in "${GUARDED[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    fail "literal passed record for required row '${label}' (must flow through record_guarded)"
  fi
  if ! grep -q -E "record_guarded \"${label}\"" "${HARNESS}" &&
     ! grep -q -E "m11_row \"${label}\"" "${HARNESS}" &&
     ! grep -q -E "exact_row \"${label}\"" "${HARNESS}" &&
     ! grep -q -E "exact_lib_row \"${label}\"" "${HARNESS}"; then
    fail "required row '${label}' has no record_guarded/m11_row/exact_row/exact_lib_row call site"
  fi
done

# ---- Plan 257 helper-mediated epoch keys -----------------------------
# `record_cardinality_rows` / `record_cardinality_for_obs` /
# `record_bandwidth_rows` emit their keys with a caller-supplied
# epoch, so the literal `(Epoch, key)` pair never appears in the
# driver. The binding is still static: every helper body must
# emit its exact keys, and every required literal epoch must
# reach the helper at a call site. A helper called with the wrong
# epoch (or not called for an epoch) fails here.
CARDINALITY_KEYS=(
  "active-before"
  "active-after"
  "registration-delta"
  "receive-id"
  "pending-baseline"
)
for key in "${CARDINALITY_KEYS[@]}"; do
  if ! grep -qF "\"${key}\"" "${DRIVER}"; then
    fail "cardinality helper never emits evidence key '${key}'"
  fi
done
for epoch in 'Epoch::Obep' 'Epoch::Ibgw' 'Epoch::Participant' 'Epoch::Restart'; do
  if ! grep -A8 'record_cardinality' "${DRIVER}" | grep -qF "${epoch}"; then
    fail "no record_cardinality call site for ${epoch}"
  fi
done
BANDWIDTH_KEYS=(
  "bandwidth-request"
  "bandwidth-reply"
  "bandwidth-disposition-observed"
)
for key in "${BANDWIDTH_KEYS[@]}"; do
  if ! grep -qF "\"${key}\"" "${DRIVER}"; then
    fail "bandwidth helper never emits evidence key '${key}'"
  fi
done
for epoch in 'Epoch::Reject' 'Epoch::Obep' 'Epoch::Ibgw' 'Epoch::Participant'; do
  if ! grep -A8 'record_bandwidth_rows' "${DRIVER}" | grep -qF "${epoch}"; then
    fail "no record_bandwidth_rows call site for ${epoch}"
  fi
done

# ---- Plan 257 §15 false-positive rejections ---------------------------
# 1. Participant far-side pass without a B-side endpoint/counter
#    observation: the driver must count the source-locked B log
#    line and bind it through far_side_satisfied.
if ! grep -qF 'count_endpoint_messages' "${DRIVER}"; then
  fail "driver must count the B-side endpoint receipt (count_endpoint_messages)"
fi
if ! grep -qF 'far_side_satisfied' "${DRIVER}"; then
  fail "driver must gate far-side rows on far_side_satisfied"
fi
if ! grep -qF 'b-endpoint-observed' "${DRIVER}"; then
  fail "driver must record the b-endpoint-observed evidence key"
fi
# 2. creator-accepted used as the only far-side proof: the
#    participant-data far-side rows must exist alongside (not
#    instead of) the secondary creator-accepted row. Keys are
#    emitted via record_row with literal Epoch::ParticipantData,
#    so match the short key literal.
if ! grep -qF '"far-side-count"' "${DRIVER}"; then
  fail "driver must record the far-side-count evidence key (creator-accepted is secondary only)"
fi
if ! grep -qF '"local-forward"' "${DRIVER}"; then
  fail "driver must record the local-forward evidence key"
fi
if ! grep -qF '"next-tunnel"' "${DRIVER}"; then
  fail "driver must record the next-tunnel evidence key"
fi
# 3. restart pass produced only by constructing a new owner and
#    checking active_count: the driver must run the real restart
#    experiment (sessions re-established + fresh build accepted).
if ! grep -qF '"sessions-reestablished"' "${DRIVER}"; then
  fail "driver must prove sessions-reestablished in the restart experiment"
fi
if ! grep -qF '"fresh-build-accepted"' "${DRIVER}"; then
  fail "driver must prove fresh-build-accepted in the restart experiment"
fi
if ! grep -qF '"old-owner-drained"' "${DRIVER}"; then
  fail "driver must prove old-owner-drained in the restart experiment"
fi
if ! grep -qF '"fresh-registration-delta"' "${DRIVER}"; then
  fail "driver must prove fresh-registration-delta in the restart experiment"
fi
if ! grep -qF '"final-baseline"' "${DRIVER}"; then
  fail "driver must prove final-baseline in the restart experiment"
fi
# 4. cancellation pass that checks only active registrations: the
#    driver must prove the full snapshot drain.
if ! grep -qF 'cancel_fully_drained' "${DRIVER}"; then
  fail "driver must gate cancellation on cancel_fully_drained (all dimensions)"
fi
if ! grep -qF 'live_state_snapshot' "${DRIVER}"; then
  fail "driver must bind lifecycle rows to live_state_snapshot"
fi
if ! grep -qF '"new-ingress-refused"' "${DRIVER}"; then
  fail "driver must prove new-ingress-refused after cancellation"
fi
for key in '"active-before"' '"active-after"' '"pending-after"' '"peer-index-after"' '"queued-work-after"'; do
  if ! grep -qF "${key}" "${DRIVER}"; then
    fail "driver must record the cancel ${key} evidence key"
  fi
done
for key in '"a-before"' '"b-before"' '"a-removed"' '"b-retained"' '"final-peer-baseline"'; do
  if ! grep -qF "${key}" "${DRIVER}"; then
    fail "driver must record the session-close ${key} evidence key"
  fi
done
# 5. session-close pass that removes A without proving B remains.
if ! grep -qF 'session_close_a_removed_b_retained' "${DRIVER}"; then
  fail "driver must gate session close on session_close_a_removed_b_retained"
fi
if ! grep -qF 'has_peer' "${DRIVER}"; then
  fail "driver must check identity-specific peer membership (has_peer)"
fi
# 6. bandwidth disposition emitted from a literal/hard-coded
#    string rather than typed decoded evidence.
if grep -vE '^[[:space:]]*(//|//!|///)' "${DRIVER}" | grep -qF 'reference-options-unobserved-no-fabrication'; then
  fail "driver must not emit the hard-coded bandwidth string (use typed TransitBandwidthSummary)"
fi
if ! grep -qF 'bandwidth_summary' "${DRIVER}" && ! grep -qF 'record_bandwidth_rows' "${DRIVER}"; then
  fail "driver must derive bandwidth rows from the typed summary (record_bandwidth_rows)"
fi
# 7. exact-registration wording without before/after delta evidence.
if ! grep -qF 'record_cardinality' "${DRIVER}"; then
  fail "driver must prove exact cardinality through record_cardinality (delta +1)"
fi
if ! grep -qF 'registration-delta' "${DRIVER}"; then
  fail "driver must record the registration-delta evidence key"
fi
# 8. local-IBGW self-reply qualification without exact pinned
#    source locks.
if ! grep -qF 'm11-i2pd-obep-remote-reply-source-lock' "${HARNESS}"; then
  fail "runner must source-lock the remote OBEP reply branch"
fi
if ! grep -qF 'm11-i2pd-obep-local-ibgw-reply-source-lock' "${HARNESS}"; then
  fail "runner must source-lock the local-IBGW reply branch"
fi
if ! grep -qF 'SelfReplyOtbrmArgs' "${REPO_ROOT}/crates/i2pr-daemon/src/transit_compose.rs"; then
  fail "production self-reply seam must bundle args (SelfReplyOtbrmArgs, no allow)"
fi
# The self-reply seam must not carry a too_many_arguments
# suppression: the two lines above the function must not contain
# an allow attribute.
SELF_REPLY_LINE="$(grep -n 'pub fn deliver_self_reply_otbrm' "${REPO_ROOT}/crates/i2pr-daemon/src/transit_compose.rs" | cut -d: -f1 | head -1)"
if [[ -n "${SELF_REPLY_LINE}" ]]; then
  SELF_REPLY_CTX="$(sed -n "$((SELF_REPLY_LINE - 3)),$((SELF_REPLY_LINE))p" "${REPO_ROOT}/crates/i2pr-daemon/src/transit_compose.rs")"
  if printf '%s\n' "${SELF_REPLY_CTX}" | grep -q 'allow(clippy::too_many_arguments)'; then
    fail "production self-reply seam must not suppress too_many_arguments"
  fi
fi
# 9/10. complete-qualification claim with fewer than two
#     independent same-SHA attempts, or evidence merged across
#     attempts: the workflow must run a two-attempt matrix on one
#     SHA with per-attempt artifacts, and the manifest must carry
#     plan 257 + attempt ids.
if ! grep -qE 'attempt.*\[1.*2\]|\[1, 2\]' "${WORKFLOW}"; then
  fail "workflow must run a two-attempt matrix (attempt: [1, 2])"
fi
if ! grep -qF 'fail-fast: false' "${WORKFLOW}"; then
  fail "workflow matrix must set fail-fast: false"
fi
if ! grep -qF '"plan": 257' "${HARNESS}" && ! grep -qF "'plan': 257" "${HARNESS}"; then
  if ! grep -qF '"plan": 257' "${HARNESS}"; then
    fail "runner manifest must name plan 257"
  fi
fi
if ! grep -qF 'I2PR_M11_ATTEMPT' "${HARNESS}"; then
  fail "runner manifest must carry the per-attempt id (I2PR_M11_ATTEMPT)"
fi

# ---- record_guarded must gate on the exit code ---------------------------
if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]' "${HARNESS}"; then
  fail "record_guarded helper lost its exit-code gate"
fi

# ---- m11_row must additionally require the row's evidence key ----------
if ! grep -q -E 'grep -Fq "\$\{key\}" "\$\{DRIVER_TSV\}"' "${HARNESS}"; then
  fail "m11_row helper lost its per-row evidence-key gate"
fi

# ---- Every driver epoch key must be emitted via record_row ---------------
# The pair (Epoch variant, short key) must appear together so a row
# for one epoch can never be satisfied by another epoch's emission.
# record_row calls may span lines, so match against a flattened copy.
DRIVER_FLAT="$(mktemp)"
tr '\n' ' ' < "${DRIVER}" | tr -s ' ' > "${DRIVER_FLAT}"
for key in "${EPOCH_KEYS[@]}"; do
  epoch="${key%%/*}"
  short="${key#*/}"
  variant=""
  case "${epoch}" in
    bootstrap) variant="Epoch::Bootstrap" ;;
    reject) variant="Epoch::Reject" ;;
    obep) variant="Epoch::Obep" ;;
    obep-data) variant="Epoch::ObepData" ;;
    ibgw) variant="Epoch::Ibgw" ;;
    ibgw-data) variant="Epoch::IbgwData" ;;
    participant) variant="Epoch::Participant" ;;
    participant-data) variant="Epoch::ParticipantData" ;;
    replay) variant="Epoch::Replay" ;;
    expiry) variant="Epoch::Expiry" ;;
    cancel) variant="Epoch::Cancel" ;;
    session-close) variant="Epoch::SessionClose" ;;
    restart) variant="Epoch::Restart" ;;
    *) fail "checker has no Epoch variant for '${epoch}'" ;;
  esac
  if [[ -n "${variant}" ]] && ! grep -qF "${variant}, \"${short}\"" "${DRIVER_FLAT}"; then
    fail "driver never emits epoch key '${key}' through record_row(${variant})"
  fi
done
rm -f "${DRIVER_FLAT}"

# ---- Driver must consume Ssu2InboundI2np through the live owner ----------
if ! grep -qE 'Ssu2InboundI2np|next_inbound|handle_inbound|TransitLiveOwner' "${DRIVER}"; then
  fail "driver must consume Ssu2InboundI2np through TransitLiveOwner::handle_inbound"
fi

# ---- Driver must forbid hand-built STBMs after runtime startup ----------
if grep -qE 'fn short_build_payload|fn build_short_payload' "${DRIVER}"; then
  fail "driver must not define a hand-built STBM helper"
fi

# ---- Driver must reject public-network fallback ------------------------
if grep -qE 'reseedFrom|publicreseed' "${DRIVER}"; then
  fail "driver must not introduce reseed or public network references"
fi

# ---- One matrix per run: no cross-run evidence merge ----------------------
# The runner must wipe the driver evidence directory before invoking
# the driver so a row can never be satisfied by a previous run's
# keys (complementary partial runs must never merge into a pass).
if ! grep -qE 'rm -f.*driver-evidence\.tsv' "${HARNESS}"; then
  fail "runner must wipe prior driver evidence before each run (no cross-run merge)"
fi

# ---- Every child reference belongs to a bounded lifecycle owner ---------
if ! grep -qE 'struct ReferenceProcess' "${DRIVER}"; then
  fail "driver must own reference processes through a bounded lifecycle struct"
fi
if ! grep -qE '\.kill\(\)' "${DRIVER}"; then
  fail "reference lifecycle owner must terminate children on shutdown/drop"
fi

# ---- Workflow must build the i2pd cache + run the runner ---------------
if ! grep -qE 'fetch-ssu2-reference.sh' "${WORKFLOW}"; then
  fail "workflow must build the exact-pinned i2pd cache via fetch-ssu2-reference.sh"
fi
if ! grep -qE 'run-i2pd.sh' "${WORKFLOW}"; then
  fail "workflow must invoke tests/integration/m11-transit/run-i2pd.sh"
fi

# ---- Evidence must not retain secrets ----------------------------------
# The forbidden tokens flag logging/serialization of static secrets
# or session keys. The driver legitimately reads
# `Ssu2IdentityMaterial::static_secret_bytes` once to prove the
# Plan 256 WP-A key-separation invariant
# (`bootstrap/ssu2-key-not-build-key`); that comparison is allowed,
# any other retention is not.
HARNESS_CODE="$(mktemp)"
grep -v -E '^[[:space:]]*#' "${HARNESS}" > "${HARNESS_CODE}"
DRIVER_CODE="$(mktemp)"
grep -v -E '^[[:space:]]*#' "${DRIVER}" > "${DRIVER_CODE}"
for forbidden in 'static_priv_key' 'session_priv' 'router_secret' 'private_key_file' 'privkey_path'; do
  hits="$(grep -nE "${forbidden}" "${HARNESS_CODE}" "${DRIVER_CODE}" || true)"
  # Filter out lines that are themselves the checker's pattern definitions.
  filtered="$(printf '%s\n' "${hits}" | grep -v 'if ! rg -q' | grep -v 'for forbidden in' | grep -v "rg '" || true)"
  if [[ -n "${filtered}" ]]; then
    fail "runner/driver must not retain secret material (matched forbidden token ${forbidden})"
    printf '%s\n' "${filtered}" >&2
  fi
done
static_hits="$(grep -nE 'static_secret' "${HARNESS_CODE}" "${DRIVER_CODE}" || true)"
# The driver's WP-A key-separation proof reads the redacted
# `Ssu2IdentityMaterial::static_secret_bytes` field in two
# comparison expressions; those exact lines are allowed, any other
# retention is not.
static_filtered="$(printf '%s\n' "${static_hits}" | grep -v 'if ! rg -q' | grep -v 'for forbidden in' | grep -v "rg '" | grep -v 'grep -v' | grep -v 'identity\.static_secret_bytes' || true)"
if [[ -n "${static_filtered}" ]]; then
  fail "runner/driver must not retain secret material (matched forbidden token static_secret)"
  printf '%s\n' "${static_filtered}" >&2
fi
rm -f "${HARNESS_CODE}" "${DRIVER_CODE}"

if [[ "${failures}" -ne 0 ]]; then
  echo "check-m11-transit-qualification-evidence: ${failures} violation(s)" >&2
  exit 1
fi
echo "check-m11-transit-qualification-evidence: ${#GUARDED[@]} guarded Plan 257 rows + ${#EPOCH_KEYS[@]} epoch keys command-derived, no literal pass records"
