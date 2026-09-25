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

# Plan 255 qualification boundaries.
#
# 15. The external qualification driver must exist, must be
#     `#[ignore]`-gated with the Plan 255 explanation, and must
#     require exact i2pd environment variables.
external_driver="$root/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"
if [[ ! -f "$external_driver" ]]; then
    fail "Plan 255 external driver missing: $external_driver"
fi
if ! rg -qF '#[ignore = "Plan 255' "$external_driver"; then
    fail "external driver must be #[ignore]-gated with the Plan 255 explanation"
fi
for env_name in 'I2PD_ROUTER_INFO' 'I2PD_SSU2_ENDPOINT' 'I2PR_SSU2_BIND' 'EVIDENCE_DIR'; do
    if ! rg -qF "$env_name" "$external_driver"; then
        fail "external driver must require env $env_name"
    fi
done
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

echo "check-m11-transit-boundaries: passed"
