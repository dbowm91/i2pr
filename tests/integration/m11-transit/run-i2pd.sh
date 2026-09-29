#!/usr/bin/env bash
# Plan 257 — M11 production self-reply qualification and external evidence completion runner.
#
# The external lane is owned by the single Rust qualification driver
# `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs`, which generates
# one ephemeral RouterIdentityBundle, installs the public RouterInfo into
# the exact source-locked i2pd NetDB layout before reference startup,
# spawns exact-pinned i2pd-A and i2pd-B itself under a bounded lifecycle
# owner, drives deterministic OBEP / IBGW / Participant epochs through
# i2pd-A SAM sessions with source-locked `explicitPeers` placement, and
# records typed epoch-scoped evidence. This script remains the outer
# cache/pin/workflow wrapper: it verifies the exact pin, provisions fresh
# loopback datadirs, runs the local foundation rows, invokes the driver
# once with explicit datadir/port environment, and collects one
# epoch-qualified evidence matrix per run. No rows are merged across runs.
#
# The lane is unprivileged and loopback-only. Required failures make
# this script fail. Sanitized evidence defaults below
# target/interop/m11-transit-evidence; set I2PR_M11_EVIDENCE_DIR to
# retain it elsewhere. Private router keys stay in the ephemeral
# driver process memory and are never copied to evidence.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M11_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m11-transit-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
I2PD_SOURCES="${REPO_ROOT}/target/interop/ssu2-sources/i2pd-${I2PD_PIN}"
I2PD_PORT="${I2PR_I2PD_PORT:-43983}"
I2PD_B_PORT="${I2PR_I2PD_B_PORT:-43984}"
I2PR_PORT="${I2PR_M11_PORT:-44181}"
I2PD_SAM_PORT="${I2PR_I2PD_SAM_PORT:-44983}"
# Plan 261 work package A: B's stock SAM bridge port (required lane
# input; the driver fails closed when I2PR_I2PD_B_SAM_PORT is absent).
I2PD_B_SAM_PORT="${I2PR_I2PD_B_SAM_PORT:-44984}"
DRIVER_TIMEOUT="1500s"

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m11-plan257.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"
# Fresh per-run reference datadirs. The driver installs the public
# i2pr RouterInfo into their exact NetDB layout before starting the
# references; nothing is reused across runs.
I2PD_A_DATADIR="${SCRATCH}/i2pd-a"
I2PD_B_DATADIR="${SCRATCH}/i2pd-b"
mkdir -p "${I2PD_A_DATADIR}" "${I2PD_B_DATADIR}"

cleanup() {
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

# ---- i2pd cache verification (fail closed before any network use) -------
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified Plan 257 source revision" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if "${I2PD_BIN}" --version 2>&1 | grep -Fq "${I2PD_VERSION}"; then
  echo "==> i2pd reference: ${I2PD_VERSION} (${I2PD_PIN})"
else
  echo "i2pd binary does not report ${I2PD_VERSION}" >&2
  exit 1
fi

REQUIRED_FAILED=0
record() {
  local label="$1"
  local status="$2"
  local detail="${3:-}"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  printf '%s\t%s\t%s\n' "${label}" "${status}" "${detail}" >> "${RESULTS_FILE}"
  [[ "${status}" == "passed" ]] || REQUIRED_FAILED=1
}

record_guarded() {
  local label="$1"
  local detail="$2"
  local rc="$3"
  if [[ "${rc}" -eq 0 ]]; then
    record "${label}" passed "${detail}"
  else
    record "${label}" failed "${detail} (exit ${rc})"
  fi
}

# Runs one exact local regression test by name; the row passes only
# when that named test passes.
exact_row() {
  local label="$1"
  local test_name="$2"
  local detail="$3"
  local log="${EVIDENCE_DIR}/local-${label}.log"
  : > "${log}"
  local rc=0
  if cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external \
       "${test_name}" -- --exact --test-threads=1 >>"${log}" 2>&1; then
    rc=0
  else
    rc=$?
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}

# Runs one exact library unit test by name (Plan 257 production
# reply/snapshot/bandwidth/lifecycle regressions live in the
# transit composition unit surface, not the external driver
# binary); the row passes only when that named test passes.
exact_lib_row() {
  local label="$1"
  local test_name="$2"
  local detail="$3"
  local log="${EVIDENCE_DIR}/local-${label}.log"
  : > "${log}"
  local rc=0
  if cargo test --locked -p i2pr-daemon --lib \
       "${test_name}" -- --exact --test-threads=1 >>"${log}" 2>&1; then
    rc=0
  else
    rc=$?
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}

echo "==> local Plan 256 corrective rows (identity, ledger, anti-fan-out)"
echo "==> local Plan 257 production-qualification rows (reply, snapshot, bandwidth, negatives)"
LOCAL_LOG="${EVIDENCE_DIR}/local-foundation.log"
: > "${LOCAL_LOG}"
local_rc=0
cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- \
  --test-threads=1 >>"${LOCAL_LOG}" 2>&1 || local_rc=$?
record_guarded "m11-i2pd-live-owner-enabled" \
  "live-owner 34-row matrix (cargo test -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1)" \
  "${local_rc}"

DISABLED_LOG="${EVIDENCE_DIR}/local-disabled-probe.log"
: > "${DISABLED_LOG}"
disabled_rc=0
cargo test --locked -p i2pr-daemon --lib transit_owner -- \
  --test-threads=1 >>"${DISABLED_LOG}" 2>&1 || disabled_rc=$?
record_guarded "m11-i2pd-no-direct-build-injection" \
  "disabled-probe + direct-injection regression rows (cargo test -p i2pr-daemon --lib transit_owner -- --test-threads=1)" \
  "${disabled_rc}"

FOCUS_LOG="${EVIDENCE_DIR}/local-focus.log"
: > "${FOCUS_LOG}"
focus_rc=0
if cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external -- \
   --test-threads=1 --skip m11_transit_against_i2pd >>"${FOCUS_LOG}" 2>&1; then
  focus_rc=0
else
  focus_rc=$?
fi
record_guarded "m11-i2pd-driver-exists" \
  "external driver compiles + the non-environment local rows pass (cargo test -p i2pr-daemon --test m11_transit_i2pd_external --skip external)" \
  "${focus_rc}"

# Work package A — identity/key coherence (one exact test per row so
# a single generic pass can never fan out into unrelated rows).
exact_row "m11-i2pd-routeridentity-build-key-coherent" \
  "plan256_routeridentity_build_key_coherent" \
  "responder secret derives the advertised RouterIdentity encryption key"
exact_row "m11-i2pd-routerinfo-hash-coherent" \
  "plan256_routerinfo_hash_coherent" \
  "signed RouterInfo carries the bundle RouterIdentity hash"
exact_row "m11-i2pd-ssu2-key-not-build-key" \
  "plan256_ssu2_key_not_build_key" \
  "SSU2 transport static key is distinct from the build responder key"
exact_row "m11-i2pd-no-independent-transit-responder-key" \
  "plan256_no_independent_transit_responder_key" \
  "driver sources the responder secret only from the bundle encryption key"

# Work package B — exact NetDB owner + pin gate.
exact_row "m11-i2pd-netdb-owner-exact" \
  "plan256_netdb_path_is_exact_hashed_owner" \
  "RI path is <datadir>/netDb/r<C0>/routerInfo-<i2p-b64>.dat, never the flat fallback"
exact_row "m11-i2pd-pin-mismatch-fails" \
  "plan256_pin_mismatch_fails_before_network_startup" \
  "pin/version mismatch fails before any socket, process, or file mutation"

# Work package D — typed ledger anti-fan-out units.
exact_row "m11-i2pd-anti-fanout-unit" \
  "plan256_single_obep_observation_satisfies_only_obep" \
  "one OBEP observation satisfies only its own epoch/role row"
exact_row "m11-i2pd-role-separation-unit" \
  "plan256_obep_evidence_cannot_satisfy_ibgw_or_participant" \
  "OBEP evidence cannot satisfy IBGW or Participant rows"
exact_row "m11-i2pd-participant-topology-unit" \
  "plan256_participant_requires_proven_b_topology" \
  "Participant row fails without the proven i2pd-B topology"
exact_row "m11-i2pd-ownership-order-unit" \
  "plan256_ownership_rows_require_inbound_observation" \
  "ownership rows require a matching inbound observation first"
exact_row "m11-i2pd-code30-unit" \
  "plan256_code30_requires_rejection_epoch_and_zero_growth" \
  "code-30 rows require a real rejection epoch and zero growth"
exact_row "m11-i2pd-replay-unit" \
  "plan256_replay_requires_one_delivery_then_drop" \
  "replay row requires one delivery plus a later drop for the same digest"
exact_row "m11-i2pd-expiry-unit" \
  "plan256_expiry_requires_logical_time_and_zero_forward" \
  "expiry row requires post-lifetime logical time and zero forward"
exact_row "m11-i2pd-cancel-unit" \
  "plan256_cancel_requires_nonzero_pre_and_zero_post" \
  "cancel row requires nonzero pre-state and zero post-state"
exact_row "m11-i2pd-restart-unit" \
  "plan256_restart_requires_zero_state_from_new_owner" \
  "restart row requires observed zero state from a new owner"
exact_row "m11-i2pd-epoch-uniqueness-unit" \
  "plan256_evidence_parser_rejects_epoch_reuse" \
  "evidence parser rejects duplicate epoch rows and cross-epoch reuse"
exact_row "m11-i2pd-checker-invariants-unit" \
  "plan256_checker_rejects_fanout_and_requires_typed_roles" \
  "static checker carries the Plan 256 corrective invariants"

# Plan 257 work package A/B/C/E/I — one exact test per row so a
# single generic pass can never fan out into unrelated rows.
exact_lib_row "m11-i2pd-obep-remote-reply-unit" \
  "plan257_remote_obep_reply_uses_remote_path_once" \
  "remote OBEP reply uses the remote garlic/TunnelGateway path once (lib test)"
exact_lib_row "m11-i2pd-local-reply-bypass-unit" \
  "plan257_local_reply_bypasses_remote_path" \
  "local reply bypasses the remote garlic/TunnelGateway path (lib test)"
exact_lib_row "m11-i2pd-reply-id-preservation-unit" \
  "plan257_reply_preserves_message_and_tunnel_ids" \
  "local/remote reply preserves exact reply message id and tunnel id (lib test)"
exact_lib_row "m11-i2pd-snapshot-dimensions-unit" \
  "plan257_state_snapshot_has_five_dimensions_without_secrets" \
  "state snapshot exposes five secret-free dimensions (lib test)"
exact_lib_row "m11-i2pd-cardinality-plus-one-unit" \
  "plan257_role_accept_moves_active_by_exactly_plus_one" \
  "role accept moves active state by exactly +1 (lib test)"
exact_lib_row "m11-i2pd-reject-zero-growth-unit" \
  "plan257_role_reject_moves_active_by_zero_and_pending_baseline" \
  "role reject moves active state by +0 with pending baseline (lib test)"
exact_lib_row "m11-i2pd-bandwidth-absence-unit" \
  "plan257_bandwidth_evidence_reports_typed_absence" \
  "bandwidth evidence reports typed absence (lib test)"
exact_lib_row "m11-i2pd-bandwidth-present-unit" \
  "plan257_bandwidth_evidence_reports_typed_present_values" \
  "bandwidth evidence reports typed present values (lib test)"
exact_lib_row "m11-i2pd-cancel-full-drain-unit" \
  "plan257_cancellation_requires_all_dimensions_zero" \
  "cancellation drains every snapshot dimension (lib test)"
exact_lib_row "m11-i2pd-session-close-ab-unit" \
  "plan257_session_close_removes_a_and_retains_b" \
  "session close removes A and retains B (lib test)"
exact_lib_row "m11-i2pd-self-reply-rollback-unit" \
  "plan257_self_reply_missing_ibgw_rolls_back_endpoint" \
  "self reply with missing IBGW rolls back the endpoint (lib test)"
exact_row "m11-i2pd-farside-local-only-unit" \
  "plan257_local_only_forward_cannot_satisfy_farside" \
  "local-only forward cannot satisfy far-side receipt"
exact_row "m11-i2pd-farside-binding-unit" \
  "plan257_farside_binds_b_observation_to_exact_next_tunnel" \
  "B observation binds to the exact next tunnel id"
exact_row "m11-i2pd-farside-creator-secondary-unit" \
  "plan257_creator_accepted_alone_cannot_satisfy_farside" \
  "creator-accepted alone cannot satisfy far-side proof"
exact_row "m11-i2pd-cancel-narrow-unit" \
  "plan257_active_only_cancel_cannot_satisfy_full_drain" \
  "active-only cancel cannot satisfy the full drain"
exact_row "m11-i2pd-session-close-narrow-unit" \
  "plan257_remove_a_without_b_retained_cannot_satisfy_session_close" \
  "removing A without B retained cannot satisfy session close"
exact_row "m11-i2pd-restart-narrow-unit" \
  "plan257_constructor_only_restart_evidence_is_rejected" \
  "constructor-only restart evidence is rejected"
exact_row "m11-i2pd-two-pass-gate-unit" \
  "plan257_single_attempt_cannot_satisfy_two_pass_closure" \
  "one complete external attempt cannot satisfy two-pass closure"
exact_row "m11-i2pd-cross-sha-unit" \
  "plan257_cross_sha_attempts_are_rejected" \
  "two attempts with different i2pr SHAs are rejected"
exact_row "m11-i2pd-no-merge-unit" \
  "plan257_cross_attempt_row_merging_is_rejected" \
  "cross-attempt row merging is rejected"
exact_row "m11-i2pd-bandwidth-hardcoded-unit" \
  "plan257_hardcoded_bandwidth_string_cannot_satisfy_disposition" \
  "a hard-coded bandwidth string cannot satisfy disposition"
exact_row "m11-i2pd-cardinality-receive-only-unit" \
  "plan257_receive_id_only_row_cannot_satisfy_cardinality" \
  "a receive-id-only row cannot satisfy cardinality"
exact_row "m11-i2pd-reply-source-lock-unit" \
  "plan257_runner_source_locks_both_reply_branches" \
  "runner source-locks both i2pd reply branches"

# Local row: the driver must be #[ignore]-gated so ordinary CI stays green
if grep -qE '#\[ignore\s*=.*Plan 256' \
     "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"; then
  record_guarded "m11-i2pd-driver-ignored-gated" \
    "external driver is #[ignore]-gated with the Plan 256 explanation" \
    "0"
else
  record_guarded "m11-i2pd-driver-ignored-gated" \
    "external driver is #[ignore]-gated with the Plan 256 explanation" \
    "1"
fi

# Local row: missing i2pd environment must fail the driver closed
if grep -qE 'missing required env' \
     "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"; then
  record_guarded "m11-i2pd-driver-missing-env-fails" \
    "external driver requires exact i2pd env variables and fails closed on absence" \
    "0"
else
  record_guarded "m11-i2pd-driver-missing-env-fails" \
    "external driver requires exact i2pd env variables and fails closed on absence" \
    "1"
fi

# Local row: no evidence-directory-derived NetDB fallback may exist.
if grep -qF '../i2pd-a/data/netDb' \
     "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"; then
  record_guarded "m11-i2pd-no-evidence-netdb-fallback" \
    "driver must not derive a NetDB path from the evidence directory" \
    "1"
else
  record_guarded "m11-i2pd-no-evidence-netdb-fallback" \
    "driver requires explicit I2PD_A_DATADIR / I2PD_B_DATADIR" \
    "0"
fi

# Work package B/C source-lock rows -------------------------------------
SHA_PIN="$(<"${I2PD_CACHE}/source-revision.txt")"
if [[ "${SHA_PIN}" == "${I2PD_PIN}" ]]; then
  record_guarded "m11-i2pd-source-pin" \
    "i2pd cache revision is exactly ${I2PD_PIN}" \
    "0"
else
  record_guarded "m11-i2pd-source-pin" \
    "i2pd cache revision is exactly ${I2PD_PIN}" \
    "1"
fi

if git -C "${I2PD_SOURCES}" \
     status --porcelain --untracked-files=no 2>/dev/null | rg -q .; then
  record_guarded "m11-i2pd-source-clean" \
    "i2pd source tree is clean (no tracked modifications)" \
    "1"
else
  record_guarded "m11-i2pd-source-clean" \
    "i2pd source tree is clean (no tracked modifications)" \
    "0"
fi

if grep -qE 'eI2NPShortTunnelBuild|HandleShortTunnelBuildMsg' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null &&
   grep -qE 'Short request record' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-short-build-source-lock" \
    "short-build creation/handling paths exist in i2pd 2.61.0 source" \
    "0"
else
  record_guarded "m11-i2pd-short-build-source-lock" \
    "short-build creation/handling paths exist in i2pd 2.61.0 source" \
    "1"
fi

if grep -qE 'I2CP_PARAM_TRUSTED_ROUTERS' \
     "${I2PD_SOURCES}/libi2pd/Destination.h" 2>/dev/null &&
   grep -qE 'SetTrustedRouters' \
     "${I2PD_SOURCES}/libi2pd/Destination.cpp" \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-trusted-router-source-lock" \
    "trustedRouters first-hop pinning source-locked (quantity-respecting placement)" \
    "0"
else
  record_guarded "m11-i2pd-trusted-router-source-lock" \
    "trustedRouters first-hop pinning source-locked (quantity-respecting placement)" \
    "1"
fi

if grep -qE 'SetExplicitPeers|GetNextRouter|I2CP_PARAM_EXPLICIT_PEERS' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.h" \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" \
     "${I2PD_SOURCES}/libi2pd/Destination.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-explicit-peer-source-lock" \
    "explicitPeers + getNextRouter source-locked in i2pd 2.61.0" \
    "0"
else
  record_guarded "m11-i2pd-explicit-peer-source-lock" \
    "explicitPeers + getNextRouter source-locked in i2pd 2.61.0" \
    "1"
fi

# Plan 256 source locks: exact NetDB hashed owner, explicit-peer
# selection gate, SAM params passthrough, NetDb::Load count line,
# tunnel maintenance cadence.
if grep -qF 'm_Storage("netDb", "r", "routerInfo-", "dat")' \
     "${I2PD_SOURCES}/libi2pd/NetDb.cpp" 2>/dev/null &&
   grep -qE 'prefix1 << safe_ident\[0\]' \
     "${I2PD_SOURCES}/libi2pd/FS.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-netdb-storage-source-lock" \
    "HashedStorage(netDb/r/routerInfo-/dat) + hashed Path() source-locked" \
    "0"
else
  record_guarded "m11-i2pd-netdb-storage-source-lock" \
    "HashedStorage(netDb/r/routerInfo-/dat) + hashed Path() source-locked" \
    "1"
fi

if grep -qE 'FindRouter \(ident\)' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null &&
   grep -qE 'IsECIES \(\)' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-select-explicit-source-lock" \
    "SelectExplicitPeers FindRouter + ECIES gate source-locked" \
    "0"
else
  record_guarded "m11-i2pd-select-explicit-source-lock" \
    "SelectExplicitPeers FindRouter + ECIES gate source-locked" \
    "1"
fi

if grep -qE 'CreateSession \(id, type, destination' \
     "${I2PD_SOURCES}/libi2pd_client/SAM.cpp" 2>/dev/null &&
   grep -qE 'explicitPeersStr = \(\*params\)\[I2CP_PARAM_EXPLICIT_PEERS\]' \
     "${I2PD_SOURCES}/libi2pd/Destination.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-sam-params-source-lock" \
    "SAM SESSION CREATE params passthrough to explicitPeers source-locked" \
    "0"
else
  record_guarded "m11-i2pd-sam-params-source-lock" \
    "SAM SESSION CREATE params passthrough to explicitPeers source-locked" \
    "1"
fi

if grep -qF 'routers loaded (' \
     "${I2PD_SOURCES}/libi2pd/NetDb.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-netdb-load-source-lock" \
    "NetDb::Load count line source-locked as the load observable" \
    "0"
else
  record_guarded "m11-i2pd-netdb-load-source-lock" \
    "NetDb::Load count line source-locked as the load observable" \
    "1"
fi

if grep -qE 'TUNNEL_POOL_MANAGE_INTERVAL' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.h" 2>/dev/null &&
   grep -qE 'TestTunnels \(ts\)' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-tunnel-maintenance-source-lock" \
    "reference tunnel maintenance/test cadence source-locked" \
    "0"
else
  record_guarded "m11-i2pd-tunnel-maintenance-source-lock" \
    "reference tunnel maintenance/test cadence source-locked" \
    "1"
fi

# Plan 257 §4 source locks: the short-build endpoint reply branch
# has two distinct paths in exact-pinned TransitTunnel.cpp. The
# remote path derives RGarlicKeyAndTag, ECIES-garlic-wraps the
# ShortTunnelBuildReply, nests it in a TunnelGateway envelope, and
# sends it to the requested reply router. The local path detects
# the reply router hash is local (`IBGW is local`), resolves the
# requested local tunnel id, injects the reply with
# SendTunnelDataMsg, and flushes — never the remote garlic path.
if grep -qF 'RGarlicKeyAndTag' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null &&
   grep -qF 'CreateTunnelGatewayMsg' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null &&
   grep -qF 'WrapECIESX25519Message' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-obep-remote-reply-source-lock" \
    "OBEP remote reply branch source-locked (RGarlicKeyAndTag + ECIES garlic + TunnelGateway)" \
    "0"
else
  record_guarded "m11-i2pd-obep-remote-reply-source-lock" \
    "OBEP remote reply branch source-locked (RGarlicKeyAndTag + ECIES garlic + TunnelGateway)" \
    "1"
fi

if grep -qF 'IBGW is local' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null &&
   grep -qF 'SendTunnelDataMsg (replyMsg)' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null &&
   grep -qF 'FlushTunnelDataMsgs' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null &&
   grep -qF 'not found for short tunnel build reply' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-obep-local-ibgw-reply-source-lock" \
    "OBEP local-IBGW reply branch source-locked (IBGW is local + SendTunnelDataMsg + Flush + fail-closed drop)" \
    "0"
else
  record_guarded "m11-i2pd-obep-local-ibgw-reply-source-lock" \
    "OBEP local-IBGW reply branch source-locked (IBGW is local + SendTunnelDataMsg + Flush + fail-closed drop)" \
    "1"
fi

# Plan 257 §10 far-side source lock: the B-side endpoint receipt
# observable the Participant far-side row binds to.
if grep -qF 'TransitTunnel: handle msg for endpoint ' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-b-endpoint-source-lock" \
    "i2pd-B endpoint receipt line source-locked (TransitTunnelEndpoint::HandleTunnelDataMsg)" \
    "0"
else
  record_guarded "m11-i2pd-b-endpoint-source-lock" \
    "i2pd-B endpoint receipt line source-locked (TransitTunnelEndpoint::HandleTunnelDataMsg)" \
    "1"
fi

# Plan 260 work package A source locks: the creator-owned inbound
# tunnel path is a distinct endpoint class from the transit
# endpoint Plan 259 modeled. An inbound `TunnelConfig(peers)`
# targets the last remote hop at the creator's local router via
# `SetNextIdent` (endpoint flag cleared, fresh nonzero local
# tunnel id); the creator-local `InboundTunnel` sets `msg->from`
# to the pool-owned tunnel; LOCAL garlic with a pool owner
# dispatches to `TunnelPool::ProcessGarlicMessage` (the
# destination session), with router-context processing only as
# the pool-less fallback.
if grep -qF 'CreatePeers (peers);' \
     "${I2PD_SOURCES}/libi2pd/TunnelConfig.cpp" 2>/dev/null &&
   grep -qF 'm_LastHop->SetNextIdent (i2p::context.GetIdentHash ());' \
     "${I2PD_SOURCES}/libi2pd/TunnelConfig.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-inbound-last-hop-targets-creator-source-lock" \
    "inbound last remote hop targets creator router source-locked (CreatePeers + SetNextIdent(GetIdentHash))" \
    "0"
else
  record_guarded "m11-i2pd-inbound-last-hop-targets-creator-source-lock" \
    "inbound last remote hop targets creator router source-locked (CreatePeers + SetNextIdent(GetIdentHash))" \
    "1"
fi

if grep -qF 'isEndpoint = false;' \
     "${I2PD_SOURCES}/libi2pd/TunnelConfig.cpp" 2>/dev/null &&
   grep -qF 'if (!nextTunnelID) nextTunnelID = 1; // tunnelID can'"'"'t be zero' \
     "${I2PD_SOURCES}/libi2pd/TunnelConfig.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-inbound-last-hop-not-transit-endpoint-source-lock" \
    "SetNextIdent clears endpoint flag with nonzero local tunnel id source-locked" \
    "0"
else
  record_guarded "m11-i2pd-inbound-last-hop-not-transit-endpoint-source-lock" \
    "SetNextIdent clears endpoint flag with nonzero local tunnel id source-locked" \
    "1"
fi

if grep -qF 'return IsInbound () ? m_LastHop->nextTunnelID : m_FirstHop->tunnelID;' \
     "${I2PD_SOURCES}/libi2pd/TunnelConfig.h" 2>/dev/null &&
   grep -qF 'return m_FirstHop->tunnelID;' \
     "${I2PD_SOURCES}/libi2pd/TunnelConfig.h" 2>/dev/null &&
   grep -qF 'return m_FirstHop->ident->GetIdentHash ();' \
     "${I2PD_SOURCES}/libi2pd/TunnelConfig.h" 2>/dev/null; then
  record_guarded "m11-i2pd-inbound-local-tunnel-id-source-lock" \
    "inbound GetTunnelID is last-hop next id, first-hop gateway tuple source-locked" \
    "0"
else
  record_guarded "m11-i2pd-inbound-local-tunnel-id-source-lock" \
    "inbound GetTunnelID is last-hop next id, first-hop gateway tuple source-locked" \
    "1"
fi

if grep -qF 'tunnel = GetTunnel (tunnelID);' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null &&
   grep -qF 'tunnel->HandleTunnelDataMsg' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-tunnel-data-local-lookup-source-lock" \
    "TunnelData local id lookup invokes resolved tunnel source-locked" \
    "0"
else
  record_guarded "m11-i2pd-tunnel-data-local-lookup-source-lock" \
    "TunnelData local id lookup invokes resolved tunnel source-locked" \
    "1"
fi

if grep -qF 'void InboundTunnel::HandleTunnelDataMsg' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null &&
   grep -qF 'msg->from = GetSharedFromThis ();' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-inbound-sets-message-owner-source-lock" \
    "creator-owned InboundTunnel sets msg->from source-locked" \
    "0"
else
  record_guarded "m11-i2pd-inbound-sets-message-owner-source-lock" \
    "creator-owned InboundTunnel sets msg->from source-locked" \
    "1"
fi

if grep -qF 'if (msg->from && msg->from->GetTunnelPool ())' \
     "${I2PD_SOURCES}/libi2pd/I2NPProtocol.cpp" 2>/dev/null &&
   grep -qF 'msg->from->GetTunnelPool ()->ProcessGarlicMessage (msg);' \
     "${I2PD_SOURCES}/libi2pd/I2NPProtocol.cpp" 2>/dev/null &&
   grep -qF 'i2p::context.ProcessGarlicMessage (msg);' \
     "${I2PD_SOURCES}/libi2pd/I2NPProtocol.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-local-garlic-pool-dispatch-source-lock" \
    "pool-owned LOCAL garlic dispatch source-locked (pool path with router-context fallback)" \
    "0"
else
  record_guarded "m11-i2pd-local-garlic-pool-dispatch-source-lock" \
    "pool-owned LOCAL garlic dispatch source-locked (pool path with router-context fallback)" \
    "1"
fi

if grep -qF 'class InboundTunnel: public Tunnel' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.h" 2>/dev/null &&
   grep -qF 'class TransitTunnelEndpoint: public TransitTunnel' \
     "${I2PD_SOURCES}/libi2pd/TransitTunnel.h" 2>/dev/null; then
  record_guarded "m11-i2pd-transit-endpoint-vs-inbound-owner-distinction-source-lock" \
    "transit endpoint and creator-owned inbound endpoint are distinct classes source-locked" \
    "0"
else
  record_guarded "m11-i2pd-transit-endpoint-vs-inbound-owner-distinction-source-lock" \
    "transit endpoint and creator-owned inbound endpoint are distinct classes source-locked" \
    "1"
fi

# Plan 260 work package B source lock: stock explicit-peer
# controls select the dedicated destination inbound pool. The
# pool prefers the explicit set when nonempty; the destination
# layer wires `inbound.length` / `inbound.lengthVariance` /
# `inbound.quantity` / `explicitPeers` from SAM SESSION CREATE
# params into that pool.
if grep -qF 'bool TunnelPool::SelectExplicitPeers (Path& path, bool isInbound)' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null &&
   grep -qF 'if (!m_ExplicitPeers.empty ()) return SelectExplicitPeers (path, isInbound);' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null &&
   grep -qF 'I2CP_PARAM_EXPLICIT_PEERS[] = "explicitPeers"' \
     "${I2PD_SOURCES}/libi2pd/Destination.h" 2>/dev/null &&
   grep -qF 'I2CP_PARAM_INBOUND_TUNNEL_LENGTH[] = "inbound.length"' \
     "${I2PD_SOURCES}/libi2pd/Destination.h" 2>/dev/null; then
  record_guarded "m11-i2pd-explicit-peer-inbound-selection-source-lock" \
    "explicit-peer one-hop inbound pool selection source-locked (SelectExplicitPeers + inbound.length + explicitPeers)" \
    "0"
else
  record_guarded "m11-i2pd-explicit-peer-inbound-selection-source-lock" \
    "explicit-peer one-hop inbound pool selection source-locked (SelectExplicitPeers + inbound.length + explicitPeers)" \
    "1"
fi

# Plan 261 work package A source locks: the B-side sender lane
# rests on four stock reference behaviors, each pinned to the exact
# tree. The B SAM bridge is a config surface (`sam.enabled` in
# i2pd.conf); B's outbound pool selects explicit peers
# (`CreateOutboundTunnel` → `SelectPeers(path, false)` with the
# nonempty-explicit preference); the outbound endpoint forwards
# TUNNEL-delivery garlics to the lease gateway
# (`TunnelEndpoint::HandleNextMessage` → `SendMessageTo` with a
# `CreateTunnelGatewayMsg` envelope); and B-side LeaseSet
# resolution is source-supported because a floodfill lists itself
# (`m_Floodfills.Insert(GetSharedRouterInfo())`), answers
# LeaseSet lookups from its local store (`FindLeaseSet` →
# `CreateDatabaseStoreMsg`), and the client request path asks the
# closest floodfill (`RequestLeaseSet` → `GetClosestFloodfill` →
# `SendLeaseSetRequest`).
if grep -qF '("sam.enabled", value<bool>()->default_value(true)' \
     "${I2PD_SOURCES}/libi2pd/Config.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-b-sam-bridge-enabled-source-lock" \
    "B stock SAM bridge config surface source-locked (sam.enabled in Config.cpp)" \
    "0"
else
  record_guarded "m11-i2pd-b-sam-bridge-enabled-source-lock" \
    "B stock SAM bridge config surface source-locked (sam.enabled in Config.cpp)" \
    "1"
fi

if grep -qF 'void TunnelPool::CreateOutboundTunnel (uint64_t ts)' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null &&
   grep -qF 'if (SelectPeers (path, false))' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null &&
   grep -qF 'if (!m_ExplicitPeers.empty ()) return SelectExplicitPeers (path, isInbound);' \
     "${I2PD_SOURCES}/libi2pd/TunnelPool.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-explicit-peer-outbound-selection-source-lock" \
    "explicit-peer outbound selection source-locked (CreateOutboundTunnel + SelectPeers false + explicit preference)" \
    "0"
else
  record_guarded "m11-i2pd-explicit-peer-outbound-selection-source-lock" \
    "explicit-peer outbound selection source-locked (CreateOutboundTunnel + SelectPeers false + explicit preference)" \
    "1"
fi

if grep -qF 'case eDeliveryTypeTunnel:' \
     "${I2PD_SOURCES}/libi2pd/TunnelEndpoint.cpp" 2>/dev/null &&
   grep -qF 'SendMessageTo (msg.hash, i2p::CreateTunnelGatewayMsg (msg.tunnelID, msg.data));' \
     "${I2PD_SOURCES}/libi2pd/TunnelEndpoint.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-outbound-endpoint-tunnel-forward-source-lock" \
    "outbound-endpoint TUNNEL-forward to lease gateway source-locked (TunnelEndpoint::HandleNextMessage + TunnelGateway)" \
    "0"
else
  record_guarded "m11-i2pd-outbound-endpoint-tunnel-forward-source-lock" \
    "outbound-endpoint TUNNEL-forward to lease gateway source-locked (TunnelEndpoint::HandleNextMessage + TunnelGateway)" \
    "1"
fi

if grep -qF 'm_Floodfills.Insert (i2p::context.GetSharedRouterInfo ());' \
     "${I2PD_SOURCES}/libi2pd/NetDb.cpp" 2>/dev/null &&
   grep -qF 'auto leaseSet = FindLeaseSet (ident);' \
     "${I2PD_SOURCES}/libi2pd/NetDb.cpp" 2>/dev/null &&
   grep -qF 'replyMsg = CreateDatabaseStoreMsg (ident, leaseSet);' \
     "${I2PD_SOURCES}/libi2pd/NetDb.cpp" 2>/dev/null &&
   grep -qF 'auto floodfill = i2p::data::netdb.GetClosestFloodfill (dest, excluded);' \
     "${I2PD_SOURCES}/libi2pd/Destination.cpp" 2>/dev/null &&
   grep -qF 'if (!SendLeaseSetRequest (dest, floodfill, request))' \
     "${I2PD_SOURCES}/libi2pd/Destination.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-b-leaseset-resolution-source-lock" \
    "B-side LeaseSet resolution source-locked (floodfill self-listing + local-store lookup reply + client request path)" \
    "0"
else
  record_guarded "m11-i2pd-b-leaseset-resolution-source-lock" \
    "B-side LeaseSet resolution source-locked (floodfill self-listing + local-store lookup reply + client request path)" \
    "1"
fi

# Plan 262 work package A source locks: the self-delivery
# loopback arm and the receive-id gateway dispatch are both
# source-locked against exact-pinned i2pd 2.61.0. The self arm
# compares the target identity with the local RouterInfo hash and
# queues to the loopback handler; the gateway arm dispatches by
# payload tunnel id without creator-peer affinity.
if grep -qF 'Transports::PostMessages' \
     "${I2PD_SOURCES}/libi2pd/Transports.cpp" 2>/dev/null &&
   grep -qF 'GetRouterInfo ().GetIdentHash ()' \
     "${I2PD_SOURCES}/libi2pd/Transports.cpp" 2>/dev/null &&
   grep -qF 'm_LoopbackHandler.PutNextMessage' \
     "${I2PD_SOURCES}/libi2pd/Transports.cpp" 2>/dev/null &&
   grep -qF 'm_LoopbackHandler.Flush ()' \
     "${I2PD_SOURCES}/libi2pd/Transports.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-self-loopback-source-lock" \
    "self-delivery loopback source-locked (PostMessages ident==local + LoopbackHandler PutNextMessage + Flush)" \
    "0"
else
  record_guarded "m11-i2pd-self-loopback-source-lock" \
    "self-delivery loopback source-locked (PostMessages ident==local + LoopbackHandler PutNextMessage + Flush)" \
    "1"
fi

if grep -qF 'eI2NPTunnelGateway' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null &&
   grep -qF 'tunnelID = bufbe32toh (msg->GetPayload ())' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null &&
   grep -qF 'tunnel = GetTunnel (tunnelID);' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null &&
   grep -qF 'HandleTunnelGatewayMsg (tunnel, msg);' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null &&
   grep -qF 'tunnel->SendTunnelDataMsg (msg);' \
     "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-tunnel-gateway-by-receive-id-source-lock" \
    "TunnelGateway receive-id dispatch source-locked (eI2NPTunnelGateway + GetTunnel(tunnelID) + HandleTunnelGatewayMsg + SendTunnelDataMsg)" \
    "0"
else
  record_guarded "m11-i2pd-tunnel-gateway-by-receive-id-source-lock" \
    "TunnelGateway receive-id dispatch source-locked (eI2NPTunnelGateway + GetTunnel(tunnelID) + HandleTunnelGatewayMsg + SendTunnelDataMsg)" \
    "1"
fi

# The counted gateway dispatch must contain no sender/build-creator
# identity comparison between TunnelGateway dispatch and
# SendTunnelDataMsg. A future implementation that inserts a fake
# local or creator PeerId to satisfy the old i2pr peer lock would
# require such a comparison here; its absence is the static guard.
GATEWAY_DISPATCH="$(awk '/case eI2NPTunnelGateway:/,/SendTunnelDataMsg/' "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null || true)"
GATEWAY_HANDLER="$(awk '/void Tunnels::HandleTunnelGatewayMsg/,/^	}/' "${I2PD_SOURCES}/libi2pd/Tunnel.cpp" 2>/dev/null || true)"
if [[ -n "${GATEWAY_DISPATCH}" ]] && [[ -n "${GATEWAY_HANDLER}" ]] &&
   ! printf '%s\n%s\n' "${GATEWAY_DISPATCH}" "${GATEWAY_HANDLER}" | grep -qE 'GetIdentHash|previous_peer|creator|build-creator'; then
  record_guarded "m11-i2pd-tunnel-gateway-no-creator-peer-affinity-source-lock" \
    "gateway dispatch carries no creator-peer affinity (no ident/creator comparison in dispatch + handler)" \
    "0"
else
  record_guarded "m11-i2pd-tunnel-gateway-no-creator-peer-affinity-source-lock" \
    "gateway dispatch carries no creator-peer affinity (no ident/creator comparison in dispatch + handler)" \
    "1"
fi

# Plan 261 §11.15: missing B-SAM env fails before network startup
# (one exact test so a generic pass can never fan out).
exact_row "m11-i2pd-b-sam-missing-env-fails" \
  "plan261_b_sam_port_missing_fails_before_network_startup" \
  "absent/invalid I2PD_B_SAM_PORT fails closed before any socket, process, or file mutation"

# Static guard rows --------------------------------------------------------
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
bash "${REPO_ROOT}/scripts/check-m11-transit-boundaries.sh" >>"${GATES_LOG}" 2>&1 || gates_rc=1
bash "${REPO_ROOT}/scripts/check-m11-transit-qualification-evidence.sh" >>"${GATES_LOG}" 2>&1 || gates_rc=1
record_guarded "m11-i2pd-runner-pinned" \
  "static boundary + Plan 256 evidence checkers (check-m11-transit-{boundaries,qualification-evidence}.sh)" \
  "${gates_rc}"

if grep -qF '127.0.0.1' "${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh"; then
  record_guarded "m11-i2pd-runner-loopback" \
    "runner stays loopback-only (127.0.0.1)" \
    "0"
else
  record_guarded "m11-i2pd-runner-loopback" \
    "runner stays loopback-only (127.0.0.1)" \
    "1"
fi

# The driver writes both reference i2pd.conf files (single-owner
# lane) with reseed disabled; prove the template carries the
# disabled `[reseed]` section and no public URL.
if grep -qF '[reseed]' "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs" &&
   ! grep -qE 'http://reseed|https://reseed' "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"; then
  record_guarded "m11-i2pd-runner-no-public-network" \
    "driver reference configs disable public reseed/network" \
    "0"
else
  record_guarded "m11-i2pd-runner-no-public-network" \
    "driver reference configs disable public reseed/network" \
    "1"
fi

if [[ -f "${REPO_ROOT}/.github/workflows/m11-transit-external.yml" ]]; then
  record_guarded "m11-i2pd-workflow-exists" \
    "hosted M11 transit external workflow exists" \
    "0"
else
  record_guarded "m11-i2pd-workflow-exists" \
    "hosted M11 transit external workflow exists" \
    "1"
fi

if ! rg -q 'static_priv_key|session_priv|router_secret|private_key_file|privkey_path' \
     "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs" &&
   ! rg 'static_secret' "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs" \
     | grep -v 'identity\.static_secret_bytes' | grep -q .; then
  record_guarded "m11-i2pd-evidence-no-secret" \
    "external driver does not retain secret material" \
    "0"
else
  record_guarded "m11-i2pd-evidence-no-secret" \
    "external driver does not retain secret material" \
    "1"
fi

echo "==> external M11 transit qualification against exact-pinned i2pd (A+B)"
# The single Rust qualification owner generates the identity, writes
# the exact NetDB files, spawns i2pd-A and i2pd-B, drives the SAM
# epochs, and records typed epoch-scoped evidence. The driver is
# `#[ignore]`-gated; explicit selection fails closed without env vars.
DRIVER_EVIDENCE="${EVIDENCE_DIR}/driver"
mkdir -p "${DRIVER_EVIDENCE}"
# One matrix per run: wipe prior driver evidence so no row can be
# satisfied by a previous run's keys (Plan 256 anti-merge rule).
rm -f "${DRIVER_EVIDENCE}/driver-evidence.tsv" \
      "${DRIVER_EVIDENCE}/ledger-evidence.tsv" \
      "${DRIVER_EVIDENCE}/snapshot-deltas.tsv" \
      "${DRIVER_EVIDENCE}/i2pd-a-driver.log" \
      "${DRIVER_EVIDENCE}/i2pd-b-driver.log"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0
# Plan 257 work package F: the dedicated Participant B reference
# runs at debug level for the bounded Participant data epoch so
# the source-locked `TransitTunnel: handle msg for endpoint <id>`
# line is emitted. Debug volume on B only (A stays at info);
# triage may override via I2PR_M11_LOGLEVEL_B.
if I2PD_BIN="${I2PD_BIN}" \
   I2PD_PIN="${I2PD_PIN}" \
   I2PD_VERSION="${I2PD_VERSION}" \
   I2PD_A_DATADIR="${I2PD_A_DATADIR}" \
   I2PD_B_DATADIR="${I2PD_B_DATADIR}" \
   I2PD_A_PORT="${I2PD_PORT}" \
   I2PD_B_PORT="${I2PD_B_PORT}" \
    I2PD_A_SAM_PORT="${I2PD_SAM_PORT}" \
    I2PD_B_SAM_PORT="${I2PD_B_SAM_PORT}" \
    I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   I2PR_M11_LOGLEVEL_B="${I2PR_M11_LOGLEVEL_B:-debug}" \
   EVIDENCE_DIR="${DRIVER_EVIDENCE}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external \
   m11_transit_against_i2pd -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${DRIVER_EVIDENCE}/driver-evidence.tsv"
m11_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  if [[ "${driver_rc}" -eq 0 && -f "${DRIVER_TSV}" ]] &&
     grep -Fq "${key}" "${DRIVER_TSV}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}

# Work package B — deterministic reference bootstrap (epoch-qualified)
m11_row "m11-i2pd-a-netdb-owner-exact" "bootstrap/a-netdb-owner-exact" \
  "public i2pr RI at the exact i2pd-A NetDB owner path before startup"
m11_row "m11-i2pd-b-netdb-owner-exact" "bootstrap/b-netdb-owner-exact" \
  "public i2pr RI at the exact i2pd-B NetDB owner path before startup"
m11_row "m11-i2pd-a-loaded-i2pr-ri" "bootstrap/a-netdb-load-observed" \
  "i2pd-A NetDb::Load count line proves the storage was traversed"
m11_row "m11-i2pd-b-loaded-i2pr-ri" "bootstrap/b-netdb-load-observed" \
  "i2pd-B NetDb::Load count line proves the storage was traversed"
m11_row "m11-i2pd-reference-knows-i2pr-ri" "bootstrap/reference-routerinfo-verified" \
  "reference RouterInfos verified out-of-band before dial"
m11_row "m11-i2pd-live-next-inbound-observed" "bootstrap/live-next-inbound-observed" \
  "real Ssu2DaemonHandle::next_inbound events observed (count-qualified)"
m11_row "m11-i2pd-live-owner-enabled" "bootstrap/live-owner-enabled" \
  "controlled TransitLiveOwner is enabled and reachable from the runtime"
m11_row "m11-i2pd-authenticated-peer-bound" "bootstrap/authenticated-peer-bound" \
  "peer/link provenance is bound to the authenticated SSU2 session"
m11_row "m11-i2pd-no-direct-build-injection" "bootstrap/no-direct-build-injection" \
  "no hand-built STBM enters the live owner after runtime startup"
m11_row "m11-i2pd-session-close-reconciled" "session-close/peer-baseline" \
  "session close invokes note_session_closed and reconciles the peer index"
m11_row "m11-i2pd-b-provisioned" "participant/next-hop-is-i2pd-b" \
  "second exact-pinned reference proven as the Participant next hop"

# Work package C — accepted build matrix (one epoch per role)
m11_row "m11-i2pd-obep-build-received" "obep/build-received" \
  "real i2pd-A outbound tunnel build reaches i2pr as OBEP"
m11_row "m11-i2pd-obep-build-accepted" "obep/build-accepted" \
  "OBEP build accepted with typed role evidence and live registration"
m11_row "m11-i2pd-obep-registration-live" "obep/registration-live" \
  "exactly one OBEP live registration installed after acceptance"
# Plan 257 work package D/E — exact cardinality + typed bandwidth per role
m11_row "m11-i2pd-obep-active-before" "obep/active-before" \
  "OBEP counted build active state before"
m11_row "m11-i2pd-obep-active-after" "obep/active-after" \
  "OBEP counted build active state after"
m11_row "m11-i2pd-obep-registration-delta" "obep/registration-delta" \
  "OBEP counted build registration delta exactly +1"
m11_row "m11-i2pd-obep-receive-id" "obep/receive-id" \
  "OBEP counted receive tunnel id"
m11_row "m11-i2pd-obep-pending-baseline" "obep/pending-baseline" \
  "OBEP pending state returned to baseline"
m11_row "m11-i2pd-obep-bandwidth-request" "obep/bandwidth-request" \
  "OBEP typed decoded m/r/l request (absence is typed)"
m11_row "m11-i2pd-obep-bandwidth-reply" "obep/bandwidth-reply" \
  "OBEP typed emitted b reply disposition"
m11_row "m11-i2pd-obep-bandwidth-disposition-observed" "obep/bandwidth-disposition-observed" \
  "OBEP bandwidth disposition from typed decoded evidence"
m11_row "m11-i2pd-ibgw-build-received" "ibgw/build-received" \
  "real i2pd-A inbound tunnel build reaches i2pr as IBGW"
m11_row "m11-i2pd-ibgw-build-accepted" "ibgw/build-accepted" \
  "IBGW build accepted with typed role evidence and live registration"
m11_row "m11-i2pd-ibgw-registration-live" "ibgw/registration-live" \
  "exactly one IBGW live registration installed after acceptance"
m11_row "m11-i2pd-ibgw-active-before" "ibgw/active-before" \
  "IBGW counted build active state before"
m11_row "m11-i2pd-ibgw-active-after" "ibgw/active-after" \
  "IBGW counted build active state after"
m11_row "m11-i2pd-ibgw-registration-delta" "ibgw/registration-delta" \
  "IBGW counted build registration delta exactly +1"
m11_row "m11-i2pd-ibgw-receive-id" "ibgw/receive-id" \
  "IBGW counted receive tunnel id"
m11_row "m11-i2pd-ibgw-pending-baseline" "ibgw/pending-baseline" \
  "IBGW pending state returned to baseline"
m11_row "m11-i2pd-ibgw-bandwidth-request" "ibgw/bandwidth-request" \
  "IBGW typed decoded m/r/l request (absence is typed)"
m11_row "m11-i2pd-ibgw-bandwidth-reply" "ibgw/bandwidth-reply" \
  "IBGW typed emitted b reply disposition"
m11_row "m11-i2pd-ibgw-bandwidth-disposition-observed" "ibgw/bandwidth-disposition-observed" \
  "IBGW bandwidth disposition from typed decoded evidence"
m11_row "m11-i2pd-participant-build-received" "participant/build-received" \
  "real i2pd-A -> i2pr -> i2pd-B intermediate build reaches i2pr as Participant"
m11_row "m11-i2pd-participant-build-accepted" "participant/build-accepted" \
  "Participant build accepted with typed role evidence and live registration"
m11_row "m11-i2pd-participant-registration-live" "participant/registration-live" \
  "exactly one Participant live registration installed after acceptance"
m11_row "m11-i2pd-participant-active-before" "participant/active-before" \
  "Participant counted build active state before"
m11_row "m11-i2pd-participant-active-after" "participant/active-after" \
  "Participant counted build active state after"
m11_row "m11-i2pd-participant-registration-delta" "participant/registration-delta" \
  "Participant counted build registration delta exactly +1"
m11_row "m11-i2pd-participant-receive-id" "participant/receive-id" \
  "Participant counted receive tunnel id"
m11_row "m11-i2pd-participant-pending-baseline" "participant/pending-baseline" \
  "Participant pending state returned to baseline"
m11_row "m11-i2pd-participant-bandwidth-request" "participant/bandwidth-request" \
  "Participant typed decoded m/r/l request (absence is typed)"
m11_row "m11-i2pd-participant-bandwidth-reply" "participant/bandwidth-reply" \
  "Participant typed emitted b reply disposition"
m11_row "m11-i2pd-participant-bandwidth-disposition-observed" "participant/bandwidth-disposition-observed" \
  "Participant bandwidth disposition from typed decoded evidence"

# Work package D/E — role data plane (genuine reference traffic)
m11_row "m11-i2pd-participant-data-forward" "participant-data/forward" \
  "Participant TunnelData transforms and forwards to i2pd-B"
m11_row "m11-i2pd-participant-data-digest" "participant-data/digest" \
  "cell digest proves the transform observation is not a no-op"
# Plan 257 work package F — independent i2pd-B far-side proof
m11_row "m11-i2pd-participant-data-local-forward" "participant-data/local-forward" \
  "Participant local forward half of the far-side proof"
m11_row "m11-i2pd-participant-data-next-tunnel" "participant-data/next-tunnel" \
  "exact next tunnel id binding the forward to the B observation"
m11_row "m11-i2pd-participant-data-b-endpoint-observed" "participant-data/b-endpoint-observed" \
  "independent i2pd-B endpoint receipt for the exact next tunnel"
m11_row "m11-i2pd-participant-data-far-side-count" "participant-data/far-side-count" \
  "sanitized B-side endpoint receipt count"
m11_row "m11-i2pd-participant-creator-accepted" "participant-data/creator-accepted" \
  "A reuses the tunnel, proving the creator accepted the build (secondary only)"
m11_row "m11-i2pd-obep-delivery" "obep-data/delivery" \
  "OBEP semantic delivery emits exactly one routing action"
m11_row "m11-i2pd-obep-fragmented-once" "obep-data/fragmented-once" \
  "fragmented OBEP delivery reassembles to one semantic delivery"
m11_row "m11-i2pd-ibgw-gateway-ingress" "ibgw-data/gateway-ingress" \
  "IBGW TunnelGateway ingress emits bounded TunnelData cells"
m11_row "m11-i2pd-ibgw-multicell-bounded" "ibgw-data/multicell-bounded" \
  "IBGW multi-cell delivery is bounded (multi-cell case observed)"
# Plan 260 work package D — creator-owned inbound receipt. The
# dedicated one-hop receiver topology binds the six-field tuple
# (receive id + creator router + creator-local tunnel + pool
# owner + LeaseSet gateway/tunnel) and proves receiver-socket
# receipt exactly once through the pool-owned inbound path.
m11_row "m11-i2pd-ibgw-creator-local-tunnel-bound" "ibgw-receipt/creator-local-tunnel-id" \
  "counted emission addresses the bound creator-local inbound tunnel id"
m11_row "m11-i2pd-ibgw-pool-owned-local-dispatch" "ibgw-receipt/pool-owner-destination" \
  "pool-owner destination bound to the receiver destination"
m11_row "m11-i2pd-ibgw-gateway-receipt" "ibgw-receipt/gateway-receipt" \
  "creator-owned IBGW receipt at the receiver SAM socket"
m11_row "m11-i2pd-ibgw-gateway-receipt-once" "ibgw-receipt/gateway-receipt-once" \
  "creator-owned IBGW receipt exactly once (no duplicate delivery)"
# Plan 261 work package B — B-sender receipt rows. The A-side
# sender leg is deleted from the counted matrix (its B-endpoint
# death stays retained as the Plan 260 B2 boundary); these rows
# bind the B-side sender establishment, the B-side LeaseSet
# resolution proof, and the terminal send-leg signature.
m11_row "m11-i2pd-ibgw-b-sender-obep-accepted" "ibgw-receipt/b-sender-obep-accepted" \
  "B-side sender establishes outbound [i2pr] (typed OBEP accept replying to B)"
m11_row "m11-i2pd-ibgw-b-leaseset-resolved" "ibgw-receipt/b-leaseset-resolved" \
  "B-side LeaseSet resolution proven behaviorally (B-originated tunnel data, no lookup failure)"
m11_row "m11-i2pd-ibgw-b-sender-outcome" "ibgw-receipt/b-sender-outcome" \
  "B-sender terminal send-leg signature (sanitized counts only)"
m11_row "m11-i2pd-replay-no-second-delivery" "replay/no-second-delivery" \
  "duplicate TunnelData produces no second delivery"

# Work package F — rejection and bandwidth (deterministic epoch)
m11_row "m11-i2pd-code30-build-rejected" "reject/build-rejected" \
  "deterministic-reject admission returns code 30 to i2pd-A"
m11_row "m11-i2pd-code30-no-registration" "reject/no-registration" \
  "code 30 leaves zero live registrations"
m11_row "m11-i2pd-code30-pending-baseline" "reject/pending-baseline" \
  "all pending / per-peer / global counters return to baseline"
# Plan 257 work package E — typed bandwidth from the decoded transaction
m11_row "m11-i2pd-bandwidth-request" "reject/bandwidth-request" \
  "typed decoded m/r/l request (absence is typed, never fabricated)"
m11_row "m11-i2pd-bandwidth-reply" "reject/bandwidth-reply" \
  "typed emitted b reply disposition on the code-30 path"
m11_row "m11-i2pd-bandwidth-disposition-observed" "reject/bandwidth-disposition-observed" \
  "bandwidth disposition from typed decoded evidence"

# Work package G — expiry / cancellation / restart
m11_row "m11-i2pd-expiry-drops-live-data" "expiry/drops-live-data" \
  "logical 600-second expiry drops later genuine TunnelData"
m11_row "m11-i2pd-expiry-resource-baseline" "expiry/resource-baseline" \
  "expiry sweep removes registration and clears secret-owning state"
# Plan 257 work package G.1 — full cancellation drain
m11_row "m11-i2pd-cancel-active-before" "cancel/active-before" \
  "cancellation starts from nonzero live state"
m11_row "m11-i2pd-cancel-active-after" "cancel/active-after" \
  "cancellation leaves zero active registrations"
m11_row "m11-i2pd-cancel-pending-after" "cancel/pending-after" \
  "cancellation leaves zero pending reservations"
m11_row "m11-i2pd-cancel-peer-index-after" "cancel/peer-index-after" \
  "cancellation leaves zero peer-index entries"
m11_row "m11-i2pd-cancel-queued-work-after" "cancel/queued-work-after" \
  "cancellation leaves zero transit-owned queued work"
m11_row "m11-i2pd-cancel-drains" "cancel/drains" \
  "cancellation drains live transit synchronously from nonzero pre-state"
m11_row "m11-i2pd-cancel-new-ingress-refused" "cancel/new-ingress-refused" \
  "new build/data ingress fails closed after cancellation"
# Plan 257 work package G.2 — session close with B retained
m11_row "m11-i2pd-session-close-a-before" "session-close/a-before" \
  "A mapping exists before close"
m11_row "m11-i2pd-session-close-b-before" "session-close/b-before" \
  "B mapping exists before close"
m11_row "m11-i2pd-session-close-a-removed" "session-close/a-removed" \
  "A mapping absent after close"
m11_row "m11-i2pd-session-close-b-retained" "session-close/b-retained" \
  "unrelated B mapping survives A's close"
m11_row "m11-i2pd-session-close-peer-baseline" "session-close/peer-baseline" \
  "session close returns the peer mapping to baseline"
m11_row "m11-i2pd-session-close-final-peer-baseline" "session-close/final-peer-baseline" \
  "final peer-index baseline after session close"
# Plan 257 work package H — real runtime restart
m11_row "m11-i2pd-restart-old-owner-drained" "restart/old-owner-drained" \
  "old owner drained before restart"
m11_row "m11-i2pd-restart-new-owner-zero" "restart/new-owner-zero" \
  "new owner starts from zero state (no state transferred)"
m11_row "m11-i2pd-restart-sessions-reestablished" "restart/sessions-reestablished" \
  "authenticated sessions re-established after restart"
m11_row "m11-i2pd-restart-fresh-build-accepted" "restart/fresh-build-accepted" \
  "fresh role-correct build accepted after restart"
m11_row "m11-i2pd-restart-fresh-registration-delta" "restart/fresh-registration-delta" \
  "fresh build installs exactly one new registration"
m11_row "m11-i2pd-restart-final-baseline" "restart/final-baseline" \
  "restarted owner drains cleanly at the end"
m11_row "m11-i2pd-restart-clean-baseline" "restart/clean-baseline" \
  "newly constructed owner proves zero transit state"

echo "==> workspace gates slice"
WS_GATES_LOG="${EVIDENCE_DIR}/workspace-gates-slice.log"
: > "${WS_GATES_LOG}"
ws_rc=0
cargo fmt --all --check >>"${WS_GATES_LOG}" 2>&1 || ws_rc=1
cargo check --locked --workspace --all-targets >>"${WS_GATES_LOG}" 2>&1 || ws_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-service-tunnel-boundaries \
           check-m11-transit-boundaries check-m11-transit-qualification-evidence \
           check-fixture-manifest check-ntcp2-vectors check-ssu2-vectors check-i2cp-vectors \
           check-ntcp2-interoperability check-constrained-host-lane-boundary \
           check-sam-acceptance-evidence check-ssu2-acceptance-evidence \
           check-i2cp-acceptance-evidence check-service-tunnel-acceptance-evidence \
           check-exploratory-tunnel-evidence check-netdb-tunnel-evidence \
           check-destination-tunnel-evidence check-streaming-tunnel-evidence \
           check-m6-mixed-router-acceptance-evidence; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${WS_GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${WS_GATES_LOG}"
    ws_rc=1
  fi
done
record_guarded "m11-i2pd-driver-exists" \
  "fmt + workspace check + 18 static gate scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${ws_rc}"

python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PD_PIN}" "${I2PD_VERSION}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root = sys.argv[1:4]
i2pd_pin, i2pd_version = sys.argv[4:6]
rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, status, detail = line.rstrip("\n").split("\t", 2)
        rows.append({"label": label, "status": status, "detail": detail})

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
driver_keys = []
driver_values = {}
driver_tsv = Path(evidence_dir) / "driver" / "driver-evidence.tsv"
if driver_tsv.exists():
    for line in driver_tsv.read_text(encoding="utf-8").splitlines():
        label = line.split("\t", 1)[0]
        driver_keys.append(label)
        parts = line.split("\t", 1)
        if len(parts) == 2 and parts[0] not in driver_values:
            driver_values[parts[0]] = parts[1]


def receipt_value(key):
    return driver_values.get(f"ibgw-receipt/{key}", "")
ledger_keys = []
ledger_tsv = Path(evidence_dir) / "driver" / "ledger-evidence.tsv"
if ledger_tsv.exists():
    for line in ledger_tsv.read_text(encoding="utf-8").splitlines():
        ledger_keys.append(line.split("\t", 1)[0])
all_passed = all(row["status"] == "passed" for row in rows)
import os
attempt = os.environ.get("I2PR_M11_ATTEMPT", "1")
datadir_id = os.environ.get(
    "I2PR_M11_DATADIR_ID", Path(evidence_dir).name or "local",
)
evidence = {
    "schema": "i2pr-m11-transit-qualification-v3",
    "plan": 262,
    "attempt": attempt,
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m11-transit-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "fresh_datadir_id": datadir_id,
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "build_command": "make USE_UPNP=no DEBUG=0 (via scripts/interop/fetch-ssu2-reference.sh)",
        "role": "mandatory independent M11 controlled transit reference, unmodified (A + B)",
    },
    "ibgw_receipt": {
        "receiver_destination": receipt_value("receiver-destination"),
        "ibgw_receive_id": receipt_value("ibgw-receive-id"),
        "creator_router": receipt_value("creator-router"),
        "creator_local_tunnel_id": receipt_value("creator-local-tunnel-id"),
        "pool_owner_destination": receipt_value("pool-owner-destination"),
        "leaseset_gateway_match": receipt_value("leaseset-gateway-match"),
        "leaseset_tunnel_match": receipt_value("leaseset-tunnel-match"),
        "full_tuple_bound": receipt_value("full-tuple-bound"),
        "gateway_receipt": receipt_value("gateway-receipt"),
        "gateway_receipt_once": receipt_value("gateway-receipt-once"),
    },
    "driver_evidence_keys": driver_keys,
    "ledger_epochs": sorted(set(ledger_keys)),
    "results": rows,
    "m11_transit_qualification": (
        "passed-via-i2pd-2.61.0" if all_passed else "failed"
    ),
    "known_limitations": [
        "controlled transit qualification only; no public transit, RouterInfo capability, or public-network participation",
        "direct loopback SSU2 session evidence only; no public I2P participation",
        "exact-pinned i2pd 2.61.0 reference; Java second-family lane stays retained/deferred",
        "closure requires two complete same-SHA attempts (attempt ids recorded per manifest); one attempt never closes Plan 261",
        "creator-local tunnel id and pool ownership bind behaviorally (typed i2pr evidence + receiver-socket receipt through the one-hop topology); stock i2pd exposes no independent numeric read of InboundTunnel::GetTunnelID",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 261 M11 B-sender receipt requalification\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- attempt: `{attempt}` (fresh datadirs per attempt; no cross-attempt merge)\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified, A + B)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 261 M11 transit qualification lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 261 M11 transit qualification lane passed; sanitized evidence: ${EVIDENCE_DIR}"
