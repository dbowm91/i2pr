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

echo "check-m11-transit-boundaries: passed"
