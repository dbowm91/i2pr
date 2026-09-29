#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_file="$root/crates/i2pr-tunnel/src/transit.rs"
daemon_transit="$root/crates/i2pr-daemon/src/transit_compose.rs"
daemon_transit_owner="$root/crates/i2pr-daemon/src/transit_owner.rs"

fail() { echo "check-m11-transit-boundaries: $*" >&2; exit 1; }

for owner in TransitHopRole TransitHopRegistration TransitRegistry TransitAdmissionState TransitAdmissionToken; do
    if rg -U -n "derive\([^)]*Clone[^)]*\)[[:space:]]*(pub[[:space:]]+)?(struct|enum)[[:space:]]+$owner" "$source_file"; then
        fail "$owner must not implement Clone"
    fi
done

# Plan 253: TransitHopMaterial in the daemon must be move-only; the
# static private key never crosses a Clone boundary.
if rg -U -n "derive\([^)]*Clone[^)]*\)[[:space:]]*(pub[[:space:]]+)?(struct|enum)[[:space:]]+TransitHopMaterial" "$daemon_transit"; then
    fail "TransitHopMaterial must not implement Clone"
fi
if rg -n "^impl[[:space:]]+Clone[[:space:]]+for[[:space:]]+TransitHopMaterial" "$daemon_transit"; then
    fail "TransitHopMaterial must not hand-implement Clone"
fi

remove_body="$(sed -n '/pub fn remove(/,/pub fn remove_into(/p' "$source_file")"
[[ -n "$remove_body" ]] || fail "TransitRegistry::remove not found"
if rg -n 'expect\(|unwrap\(|panic!|unreachable!' <<<"$remove_body"; then
    fail "TransitRegistry::remove must return typed unknown-id errors"
fi
if rg -n 'tokio::|std::net|std::fs|async[[:space:]]+fn|JoinHandle|spawn\(' "$source_file"; then
    fail "runtime ownership leaked into runtime-neutral transit module"
fi
if rg -n 'std::sync|Mutex|RwLock|AtomicU|AtomicI' "$source_file"; then
    fail "M11 foundation must leave synchronization ownership to the runtime plan"
fi

# Plan 252: production daemon code must use the message-level
# transit API (`process_short_build_message`) rather than calling
# the per-record Plan 250 transaction and then independently
# re-running `MessageHopProcessor` or `chacha20_transform` to
# apply the canonical per-slot ChaCha transforms.
# The Plan 250 per-record API is retained for focused tests and
# historical consumers; it must not appear in the production
# daemon code under `crates/i2pr-daemon/src/`.
daemon_root="$root/crates/i2pr-daemon/src"
# Production boundary checks run against non-test code only: inline
# `#[cfg(test)]` modules may seal synthetic fixtures to construct
# inputs, but production paths must treat transformed payloads as
# opaque. Strip everything from the first `#[cfg(test)]` marker to
# end-of-file per source file before matching.
production_src="$(mktemp -d)"
trap 'rm -rf "$production_src"' EXIT
while IFS= read -r src; do
    rel="${src#$root/}"
    dest="$production_src/$rel"
    mkdir -p "$(dirname "$dest")"
    awk '/#\[cfg\(test\)\]/{exit} {print}' "$src" > "$dest"
done < <(find "$daemon_root" -name '*.rs')
if rg -n 'process_short_build_request\b' "$production_src"; then
    fail "production daemon code must not call process_short_build_request; use process_short_build_message"
fi
# The canonical per-slot ChaCha transform primitive is owned by
# i2pr-tunnel's multirecord module. Production daemon code must
# never reach in to invoke it directly alongside a Plan 250
# per-record transaction.
if rg -n 'chacha20_transform\(' "$production_src"; then
    fail "production daemon code must not call chacha20_transform directly; compose via process_short_build_message"
fi
# The Plan 250 per-record post-`process_short_build_request` reply
# envelope must not be resealed by the daemon; the daemon must
# treat the transformed build payload as opaque.
if rg -n 'seal_short_reply\(|open_short_reply\(|seal_short_request\(|open_short_request\(' "$production_src"; then
    fail "production daemon code must not invoke build-cryptography primitives; secrets stay in i2pr-tunnel"
fi

# Plan 253 corrective boundaries. Each new check must remain
# small and semantic; do not turn this file into a source-text
# grep harness.
#
# 1. The Plan 252 placeholder `forward_participant_layer` is
#    removed; production daemon code must never introduce a
#    no-op TunnelData transform helper.
if rg -n -v '// Plan 253 removes' "$production_src" | rg -n 'forward_participant_layer\b'; then
    fail "production daemon code must not call forward_participant_layer; compose via TransitHopRegistration::process_tunnel_data"
fi
# 2. Production daemon code must use the runtime-neutral
#    `TransitHopRegistration::process_tunnel_data` API rather than
#    re-implementing the canonical participant / OBEP / IBGW
#    transforms locally. Comment lines beginning with `//` are
#    excluded because the production daemon may mention the
#    canonical helper in documentation only.
comment_stripped="$(mktemp -d)/stripped"
trap 'rm -rf "$production_src" "$comment_stripped"' EXIT
while IFS= read -r src; do
    rel="${src#$production_src/}"
    dest="$comment_stripped/$rel"
    mkdir -p "$(dirname "$dest")"
    # Strip comment lines; do not strip string literals — the
    # production code never references the helper in string
    # literals because it is a typed constant.
    grep -v -E '^[[:space:]]*//' "$src" > "$dest"
done < <(find "$production_src" -name '*.rs')
if rg -n 'TunnelLayerTransform::participant_forward' "$comment_stripped"; then
    fail "production daemon code must not invoke TunnelLayerTransform directly; the runtime-neutral data plane owns the transform"
fi
# 3. `TransitBuildService` must have a production caller outside
#    its own test module once the daemon composition is enabled.
#    The presence of `TransitOwner::dispatch_short_build` and the
#    lib-public `transit_owner` module satisfy the rule.
if [[ ! -f "$daemon_transit_owner" ]]; then
    fail "transit_owner module must exist to provide a live caller for TransitBuildService"
fi
if ! rg -q 'gate\.dispatch_short_build\b' "$daemon_transit_owner"; then
    fail "transit_owner must wire the controlled gate via TransitIngressGate::dispatch_short_build"
fi
# 4. Rejection delivery must not discard the route metadata —
#    every code-30 path must surface the role-correct
#    `TransitDispatchRole` shape.
if ! rg -q 'enum TransitDispatchRole' "$daemon_transit"; then
    fail "daemon transit_compose must expose TransitDispatchRole for code-30 route metadata"
fi
# 5. The daemon-side build dispatcher must wrap the
#    transformed body in a complete I2NP envelope (STBM type
#    0x19 or OTBRM type 0x1A) before handing bytes to
#    `RouterDeliveryService`; a bare STBM/OTBRM body is
#    never sent.
if ! rg -q 'wrap_short_tunnel_build_envelope' "$daemon_transit"; then
    fail "daemon transit_compose must wrap ShortTunnelBuild bodies via wrap_short_tunnel_build_envelope"
fi
if ! rg -q 'wrap_outbound_tunnel_build_reply_envelope' "$daemon_transit"; then
    fail "daemon transit_compose must wrap OutboundTunnelBuildReply bodies via wrap_outbound_tunnel_build_reply_envelope"
fi
# 6. The peer/session index is bounded; the production daemon
#    must use `TransitPeerIndex` rather than `BTreeMap<Hash, PeerId>`.
if rg -n 'peer_index: BTreeMap' "$production_src"; then
    fail "production daemon code must use TransitPeerIndex, not BTreeMap<Hash, PeerId>"
fi
# 7. The peer/session index must declare the
#    `MAX_TRANSIT_PEER_INDEX` ceiling; the daemon must not
#    silently accept an unlimited container.
if ! rg -q 'MAX_TRANSIT_PEER_INDEX' "$daemon_transit"; then
    fail "daemon transit_compose must reference MAX_TRANSIT_PEER_INDEX"
fi
# 8. `TransitBuildService::cancel` must drain the registry
#    synchronously rather than wait for the 600-second
#    expiration timer.
if rg -n 'fn cancel' "$daemon_transit" | rg -q .; then
    if ! rg -n 'registry.expire\(u64::MAX\)' "$daemon_transit"; then
        fail "TransitBuildService::cancel must drain registry synchronously via registry.expire(u64::MAX)"
    fi
fi

# Plan 254 corrective boundaries.
#
# 9. The Plan 253 empty-body shim is removed completely. Neither the
#    original `plan253_short_build_payload` helper nor a renamed
#    equivalent returning a hard-coded empty transit build body may
#    remain in daemon transit code.
if rg -n 'plan253_short_build_payload' "$daemon_root"; then
    fail "plan253_short_build_payload must be removed; thread the canonical decoded body instead"
fi
if rg -n 'fn .*short_build_payload' "$daemon_root"; then
    fail "no short_build_payload helper may remain in daemon code; use the canonical transit body handoff"
fi
# 10. The production live-owner module must be referenced by the
#     actual inbound owner outside tests. The ordinary daemon SSU2
#     pump in `lib.rs` consults the disabled probe so the caller
#     exists in non-test production code.
if ! rg -q 'transit_owner' "$root/crates/i2pr-daemon/src/lib.rs"; then
    fail "production lib.rs must reference the transit_owner live-owner module"
fi
if ! rg -q 'controlled_transit_disabled_probe' "$root/crates/i2pr-daemon/src/lib.rs"; then
    fail "production lib.rs must consult the controlled transit probe on the inbound path"
fi
# 11. The production short-build dispatch path must use the outer
#     owner's real cancellation token, never a fresh token created
#     inside the dispatch.
if rg -n 'CancellationToken::new\(\)' "$daemon_transit_owner"; then
    fail "transit_owner must not create a fresh CancellationToken; use the outer owner token"
fi
# 12. No second I2NP decode may live in `transit_owner.rs`. The
#     canonical `router_i2np` dispatcher is the single decoder; the
#     live owner consumes `dispatch_router_i2np_with_transit_bodies`
#     only.
if rg -n 'decode_standard|decode_short_transport|I2npMessage::decode' "$daemon_transit_owner"; then
    fail "transit_owner must not decode I2NP envelopes; use the canonical router_i2np handoff"
fi
# 13. Daemon transit code must never hold raw `LayerKeys` or reach
#     the canonical `TunnelLayerTransform` directly; the
#     runtime-neutral data plane owns the transform.
if rg -n 'LayerKeys' "$daemon_transit_owner"; then
    fail "transit_owner must not name LayerKeys; secrets stay in i2pr-tunnel"
fi
if rg -n 'TunnelLayerTransform' "$daemon_transit_owner"; then
    fail "transit_owner must not invoke TunnelLayerTransform directly"
fi
# 14. The live owner must expose the controlled enablement, the
#     creator/service ownership probes, the outer-cancellation
#     drain, and the session-close peer reconciliation the Plan 254
#     acceptance criteria require.
for symbol in 'TransitLiveOwner' 'install_creator_build' 'install_creator_data' 'install_creator_gateway' 'note_session_closed' 'dispatch_router_i2np_with_transit_bodies' 'TransitInboundBodies'; do
    if ! rg -q "$symbol" "$daemon_transit_owner" && ! rg -q "$symbol" "$root/crates/i2pr-daemon/src/router_i2np.rs"; then
        fail "expected Plan 254 live-ingress symbol $symbol in transit_owner/router_i2np"
    fi
done

# Plan 255 qualification boundaries (historical; the Plan 256
# corrective supersedes the driver/runner/checker expectations in
# rules 20-24 below, but the production-code invariants stay).
#
# 15. The external qualification driver must exist and must be
#     `#[ignore]`-gated with its plan explanation.
external_driver="$root/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"
if [[ ! -f "$external_driver" ]]; then
    fail "Plan 255 external driver missing: $external_driver"
fi
if ! rg -qF '#[ignore = "Plan 25' "$external_driver"; then
    fail "external driver must be #[ignore]-gated with its plan explanation"
fi
# 16. The external driver must consume Ssu2InboundI2np through
#     `TransitLiveOwner::handle_inbound` rather than constructing
#     STBMs by hand after runtime startup.
for symbol in 'Ssu2InboundI2np' 'next_inbound' 'TransitLiveOwner' 'handle_inbound' 'dispatch_router_i2np_with_transit_bodies'; do
    if ! rg -q "$symbol" "$external_driver"; then
        fail "external driver missing Plan 255 symbol $symbol"
    fi
done
# 17. The external driver must not reintroduce the Plan 253 empty-body
#     shim or hand-built STBM helper in production daemon code.
#     (The driver file may name the historical symbol inside an
#     assertion that proves the production invariant; production
#     daemon source is the actual surface to guard.)
if rg -q 'plan253_short_build_payload' "$daemon_root"; then
    fail "daemon transit code must not reintroduce the empty-body shim"
fi
if rg -q 'fn short_build_payload' "$daemon_root"; then
    fail "daemon transit code must not define a hand-built STBM helper"
fi
# 18. The fail-closed runner must exist, must verify the exact i2pd
#     pin + version, must stay loopback-only, and must disable public
#     reseed.
external_runner="$root/tests/integration/m11-transit/run-i2pd.sh"
external_checker="$root/scripts/check-m11-transit-qualification-evidence.sh"
for path in "$external_runner" "$external_checker"; do
    if [[ ! -f "$path" ]]; then
        fail "Plan 255 surface missing: $path"
    fi
done
if ! rg -qF '635b013a612ff47278ef02acf8580a28e10e26c5' "$external_runner"; then
    fail "external runner lost the exact i2pd pin"
fi
if ! rg -qF '2.61.0' "$external_runner"; then
    fail "external runner lost the exact i2pd version"
fi
if ! rg -qF '127.0.0.1' "$external_runner"; then
    fail "external runner lost the loopback bind policy"
fi
# 19. The static evidence checker must exist and require every
#     mandatory Plan 255 row.
if ! rg -qF 'm11-i2pd-live-next-inbound-observed' "$external_checker"; then
    fail "evidence checker missing Plan 255 row labels"
fi

# Plan 256 qualification evidence/topology corrective boundaries.
#
# 20. The external driver is `#[ignore]`-gated with the Plan 256
#     explanation and requires the explicit single-owner
#     environment (binary, pin, both reference datadirs,
#     loopback bind, evidence dir). The Plan 255
#     `I2PD_ROUTER_INFO` / `I2PD_SSU2_ENDPOINT` split inputs are
#     gone: identity and references are owned by one process.
if ! rg -qF '#[ignore = "Plan 256' "$external_driver"; then
    fail "external driver must be #[ignore]-gated with the Plan 256 explanation"
fi
for env_name in 'I2PD_BIN' 'I2PD_A_DATADIR' 'I2PD_B_DATADIR' 'I2PD_PIN' 'I2PD_VERSION' 'I2PR_SSU2_BIND' 'EVIDENCE_DIR'; do
    if ! rg -qF "$env_name" "$external_driver"; then
        fail "external driver must require env $env_name"
    fi
done
# 21. The driver must bind role rows to typed decoded roles through
#     an epoch-qualified evidence recorder, and must own both
#     reference lifecycles.
for symbol in 'TransitHopRoleKind' 'record_row' 'enum Epoch' 'ReferenceProcess' 'I2PD_B_DATADIR' 'routerInfo-'; do
    if ! rg -qF "$symbol" "$external_driver"; then
        fail "external driver missing Plan 256 symbol $symbol"
    fi
done
# 22. The Plan 255 defects must stay fixed: no generic build fan-out
#     boolean, no independent transit responder key, no
#     evidence-directory-derived NetDB fallback. Comment and
#     self-check mentions are excluded; only code counts.
driver_code_nocomments="$(mktemp)"
grep -vE '^[[:space:]]*(//|//!|///)' "$external_driver" > "$driver_code_nocomments"
if grep -qE 'let[[:space:]]+(mut[[:space:]]+)?observed_build|observed_build[[:space:]]*=|if[[:space:]]+observed_build' "$driver_code_nocomments"; then
    fail "external driver must not reintroduce the observed_build fan-out"
fi
rm -f "$driver_code_nocomments"
if rg -v 'contains\(' "$external_driver" | rg -q 'X25519PrivateKey::generate\('; then
    fail "external driver must not generate an independent transit responder key"
fi
if rg -qF '../i2pd-a/data/netDb' "$external_driver"; then
    fail "external driver must not derive a NetDB path from the evidence directory"
fi
# 23. The runner must provision explicit fresh datadirs for both
#     references, drive the corrected lane once per invocation, and
#     keep the exact pin/version/loopback/reseed gates.
for token in 'I2PD_A_DATADIR' 'I2PD_B_DATADIR' 'I2PD_A_SAM_PORT' 'I2PD_B_SAM_PORT' 'DRIVER_TIMEOUT' 'm11_row' 'exact_row'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 256 token $token"
    fi
done
# 24. The evidence checker must carry the Plan 256 corrective
#     invariants (anti-fan-out, typed roles, i2pd-B, exact owner,
#     key coherence, epoch keys).
for token in 'observed_build' 'TransitHopRoleKind' 'I2PD_B_DATADIR' 'routerInfo-' 'X25519PrivateKey::generate' 'EPOCH_KEYS'; do
    if ! rg -qF "$token" "$external_checker"; then
        fail "evidence checker missing Plan 256 invariant $token"
    fi
done

# Plan 257 production self-reply and evidence-completion boundaries.
#
# 25. The non-secret snapshot, the self-reply arg bundle, and the
#     typed bandwidth summary must exist with the exact Plan 257
#     shapes; the daemon must surface bandwidth from the
#     runtime-neutral transaction (never re-decode the Mapping).
for symbol in 'TransitLiveStateSnapshot' 'SelfReplyOtbrmArgs' 'TransitBandwidthSummary'; do
    if ! rg -q "$symbol" "$daemon_transit"; then
        fail "daemon transit_compose missing Plan 257 symbol $symbol"
    fi
done
if ! rg -q 'TransitBandwidthSummary' "$source_file"; then
    fail "i2pr-tunnel transit must expose TransitBandwidthSummary"
fi
if ! rg -q 'live_state_snapshot' "$daemon_transit"; then
    fail "TransitBuildService must expose live_state_snapshot"
fi
if ! rg -q 'live_state_snapshot' "$daemon_transit_owner"; then
    fail "TransitLiveOwner must expose live_state_snapshot"
fi
if ! rg -q 'has_peer' "$daemon_transit_owner"; then
    fail "TransitLiveOwner must expose has_peer for session-close B-retained proof"
fi
# 26. The daemon must not re-decode build-option bandwidth: no
#     Mapping/get("m") lookup in daemon transit code.
if rg -n 'mapping\.get\(|mapping\(\).get\(|get\("m"\)|get\("r"\)|get\("l"\)|get\("b"\)' "$production_src"; then
    fail "daemon transit code must not re-decode bandwidth options; copy TransitBandwidthSummary from the transaction"
fi
# 27. The self-reply seam must take the bundled args (Plan 257
#     work package I clippy-ceiling fix without suppression).
if ! rg -q 'SelfReplyOtbrmArgs' "$daemon_transit_owner"; then
    fail "transit_owner must route self replies through SelfReplyOtbrmArgs"
fi
# 28. The evidence checker must carry the Plan 257 invariants
#     (reply source-locks, far-side, full-drain, typed bandwidth,
#     cardinality, two-attempt gate).
for token in 'count_endpoint_messages' 'far_side_satisfied' 'cancel_fully_drained' 'record_cardinality' 'record_bandwidth_rows' 'SelfReplyOtbrmArgs' 'I2PR_M11_ATTEMPT'; do
    if ! rg -qF "$token" "$external_checker"; then
        fail "evidence checker missing Plan 257 invariant $token"
    fi
done
# 29. The runner must source-lock both i2pd reply branches and
#     emit the Plan 262 manifest shape (plan 262 + attempt).
for token in 'm11-i2pd-obep-remote-reply-source-lock' 'm11-i2pd-obep-local-ibgw-reply-source-lock' '"plan": 262' 'I2PR_M11_ATTEMPT' 'exact_lib_row'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 262 token $token"
    fi
done
# 31. Plan 261 work package A: the runner must source-lock the
#     B-side sender behaviors and provision the B SAM port; the
#     manifest must not name the superseded A-sender plan 260.
for token in 'm11-i2pd-b-sam-bridge-enabled-source-lock' 'm11-i2pd-explicit-peer-outbound-selection-source-lock' 'm11-i2pd-outbound-endpoint-tunnel-forward-source-lock' 'm11-i2pd-b-leaseset-resolution-source-lock' 'I2PD_B_SAM_PORT'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 261 token $token"
    fi
done
if rg -qF '"plan": 260' "$external_runner"; then
    fail "external runner must not name stale plan 260 (B-sender authority is Plan 261)"
fi
if rg -qF '"plan": 261' "$external_runner"; then
    fail "external runner must not name stale plan 261 (B3 self-delivery boundary corrected by Plan 262)"
fi
# 32. Plan 262 work package B: the dedicated IBGW state must not
#     inherit participant previous-peer locking. The type alias is
#     gone; the struct owns only the fragment sequence.
if rg -q 'pub type TransitGatewayData' "$source_file"; then
    fail "TransitGatewayData must be a dedicated struct, not a type alias for TransitParticipantData"
fi
if ! rg -q 'pub struct TransitGatewayData' "$source_file"; then
    fail "i2pr-tunnel transit must own dedicated TransitGatewayData"
fi
gateway_struct="$(sed -n '/pub struct TransitGatewayData/,/^}/p' "$source_file")"
if printf '%s\n' "$gateway_struct" | rg -q 'locked_previous_peer'; then
    fail "TransitGatewayData must not contain locked_previous_peer"
fi
if printf '%s\n' "$gateway_struct" | rg -q 'duplicates'; then
    fail "TransitGatewayData must not inherit the participant replay window"
fi
# 33. Plan 262 work package C: the runtime-neutral IBGW gateway API
#     must not accept or check a previous-peer argument; exact
#     receive-id equality is required.
gateway_fn="$(awk '/pub fn process_tunnel_gateway/{on=1} on{print; o+=gsub(/{/,"{"); c+=gsub(/}/,"}"); if(o>0 && o==c){exit}}' "$source_file" | grep -vE '^[[:space:]]*(//|//!|///)' || true)"
if printf '%s\n' "$gateway_fn" | rg -q 'previous_peer'; then
    fail "process_tunnel_gateway must not accept or check previous_peer"
fi
if ! printf '%s\n' "$gateway_fn" | rg -q 'expected_receive'; then
    fail "process_tunnel_gateway must bind the registry key for exact receive-id equality"
fi
# 34. Plan 262 work package D: one canonical source-neutral IBGW
#     seam for network and local ingress; the OBEP TUNNEL-to-self
#     branch must not synthesize peer state.
if ! rg -q 'pub fn route_ibgw_gateway' "$daemon_transit"; then
    fail "daemon transit_compose must own source-neutral route_ibgw_gateway"
fi
if ! rg -q 'deliver_obep_tunnel_to_self' "$daemon_transit_owner"; then
    fail "transit_owner must own the OBEP TUNNEL-to-self local branch"
fi
if ! rg -q 'LocalIbgwDelivered' "$daemon_transit_owner"; then
    fail "transit_owner must expose LocalIbgwDelivered"
fi
if ! rg -q 'LocalIbgwDropped' "$daemon_transit_owner"; then
    fail "transit_owner must expose LocalIbgwDropped"
fi
self_fn="$(awk '/fn deliver_obep_tunnel_to_self/{on=1} on{print; o+=gsub(/{/,"{"); c+=gsub(/}/,"}"); if(o>0 && o==c){exit}}' "$daemon_transit_owner" | grep -vE '^[[:space:]]*(//|//!|///)' || true)"
if printf '%s\n' "$self_fn" | rg -q 'PeerId::from_hash|PeerId::from_bytes'; then
    fail "self-loop must not synthesize a PeerId"
fi
if printf '%s\n' "$self_fn" | rg -q 'install_peer'; then
    fail "self-loop must never insert the local router into the peer index"
fi
# 35. Plan 262 work package A: the runner must source-lock the
#     self-loopback and receive-id gateway dispatch.
for token in 'm11-i2pd-self-loopback-source-lock' 'm11-i2pd-tunnel-gateway-by-receive-id-source-lock' 'm11-i2pd-tunnel-gateway-no-creator-peer-affinity-source-lock'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 262 token $token"
    fi
done
# 30. The external workflow must run the two-attempt matrix on one
#     SHA with fail-fast disabled and per-attempt artifacts.
external_workflow="$root/.github/workflows/m11-transit-external.yml"
if [[ ! -f "$external_workflow" ]]; then
    fail "M11 external workflow missing: $external_workflow"
fi
for token in 'attempt: [1, 2]' 'fail-fast: false' 'I2PR_M11_ATTEMPT' 'm11-transit-evidence-attempt-'; do
    if ! rg -qF "$token" "$external_workflow"; then
        fail "external workflow missing Plan 257 token $token"
    fi
done

echo "check-m11-transit-boundaries: passed"
