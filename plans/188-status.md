# Plan 188 status — M6 short-build-reply interop corrective

Status: **`in-progress-m6-build-reply-installs-proven`** (2/7 rows
flipped to passed; 5/7 remain blocked on the LeaseSet2-lookup gap;
no Streaming claim).

Plan of record:
[`plans/188-m6-short-build-reply-interop-corrective.md`](188-m6-short-build-reply-interop-corrective.md).

> Numbering note: `plans/188-m6-mixed-router-streaming-with-i2pd.md`
> is the deferred Streaming pass (blocked until destination rows go
> green). It is **not** the executable corrective; it will be
> renumbered to 189 (and Java second-family to 190) in a follow-up
> docs prune. The executable corrective is the
> `188-m6-short-build-reply-interop-corrective.md` file this status
> closes against. `plans/189-m6-java-i2p-second-family-qualification-and-closure.md`
> stays blocked until Streaming passes.

## Current authority

```text
plan_188 = in-progress-m6-build-reply-installs-proven
plan_187 = blocked-by-m6-build-reply-interop-gap (2/7 flipped; 5/7 still blocked)
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = not-yet-passed (installs proven, lookup pending)
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 188 (complete lookup/publication/messaging rows)
```

## Root cause (build-reply half, resolved)

Exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`)
never sends a direct `OutboundTunnelBuildReply` (type 26) for
one-hop short builds. Instead, per
`TransitTunnel::HandleShortTransitTunnelBuildMsg`:

- outbound endpoint (our OBEP): garlic-wraps the OTBRM under the
  OBEP `RGarlicKeyAndTag` and nests it as
  `TunnelGateway(next_tunnel = 0x9502, Garlic(...))` (types 19/11)
  sent directly to the creator. The previous coordinator dropped
  these as `Unsupported` (`unsupported={11,19}`, `kind_reply=0`);
- inbound gateway (our IBGW): seals its reply in place and forwards
  the same `ShortTunnelBuild` records (type 25) to the creator with
  the original `SEND_MSG_ID`. The previous coordinator ignored all
  type-25 arrivals except as `invalid_replies`.

Hypothesis 1 (4-record padding) is excluded: 4-record requests are
accepted (transit created) and 4-record garlic/forwarded replies
install once consumed. No sender-side record-count option was added.
Hypothesis 2 (message-id correlation) is refined: outer
`TunnelGateway` message-ids are unrelated; correlation is by
`(peer, next_tunnel)` plus the inner reply `message_id` after
unwrap. Forwarded STBM correlation remains `(peer, message_id)`.
Hypothesis 3 (reply addressing) was the gap. Hypothesis 4
(timing/fragmentation) was not needed: the lossless session
(`protocol_drops=0`) already proved delivery.

## What landed

Narrow additive seams only; no wire-format change, no
authentication weakening for tunnel-key material (all installs go
through consumed reference replies and existing AEAD; creator-known
keys are never installed without a reply).

```text
crates/i2pr-tunnel/src/garlic_reply.rs (new)
  Bounded runtime-neutral OBEP Garlic unwrap (`decrypt_build_reply_garlic`):
  tag check against the pending attempt, ChaCha20-Poly1305 decrypt
  (nonce zero, AD = tag), bounded clove parse for the single local
  ShortTunnelBuildReply clove (type 26), strict OTBRM
  `1 + count*218` validation. Typed `GarlicReplyError`, no secret
  logging, 5 unit rows.

crates/i2pr-tunnel/src/short.rs
  Narrow accessor `ShortBuildStateMachine::obep_garlic_material`
  returning the OBEP `RGarlicKeyAndTag` copy for coordinator
  correlation. No state-machine behavior change.

crates/i2pr-tunnel/src/multirecord.rs
  Inbound originator-fake bytes are ignored on the reply path
  (shape check retained: inbound still carries exactly one fake).
  The fake is privacy padding like padding fakes (already ignored);
  all tunnel-key material comes from real-hop AEAD, which still
  authenticates. The reference transforms the fake slot with keying
  that does not recover under the creator `reply_key`
  (`OriginatorFakeModified` on genuine replies); verifying it would
  reject genuine installs without strengthening key authentication.
  Documented inline; no wire change.

crates/i2pr-tunnel/src/lib.rs
  Export `garlic_reply` module.

crates/i2pr-daemon/src/exploratory_build.rs
  PendingBuild carries `next_tunnel` + OBEP garlic key/tag
  correlation copies; `route_inbound_i2np` now drives three narrow
  paths: direct OTBRM (unchanged, synthetic responder), forwarded
  ShortTunnelBuild for pending inbound `(peer, message_id)` (new),
  TunnelGateway Garlic unwrap for pending outbound
  `(peer, next_tunnel)` + inner `message_id` (new). Shared
  `drive_attempt_to_terminal` preserves strict OTBRM extraction and
  `Installed` via existing pool/registry seams. Unmatched arrivals
  count as orphans, never installs.

crates/i2pr-daemon/tests/exploratory_build_live.rs
  Two reference-shaped live rows (same EciesX25519 crypto with
  responder-owned secrets, no production shortcut):
  `outbound_one_hop_via_reference_garlic_gateway_installs`
  (TunnelGateway + Garlic) and
  `inbound_one_hop_via_reference_forward_installs` (forwarded
  STBM). Prior 9 rows unchanged.

crates/i2pr-daemon/Cargo.toml + Cargo.lock
  `chacha20poly1305` in dev-dependencies for the reference-shaped
  live responder (test-only Garlic encrypt). No production
  dependency change.
```

## Evidence

Local lane (no external process):

```text
cargo test --locked -p i2pr-tunnel -- --test-threads=1
# 280 passed (incl. 5 new garlic_reply rows)
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
# 11 passed (9 prior + 2 new reference-shaped rows)
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
# 27 passed (unchanged)
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
# 9 passed (unchanged)
```

External lane (exact-pinned i2pd, loopback, unmodified):

```text
bash tests/integration/m6-interop/run-destination.sh
# lane still fails closed overall (lookup gap below), sanitized evidence:
#   target/interop/m6-destination-evidence/evidence.json
#   target/interop/m6-destination-evidence/driver/driver-evidence.tsv
#   target/interop/m6-destination-evidence/reference-facts.tsv
```

Representative install-pump summary with this corrective (no skip
flags, command-derived):

```text
install-pump-summary = build_reserved=15 other=0 tunnel_data=0
  router_control=0 unsupported={11: 3, 19: 1}
  installed_ob=1 installed_ib=1 non_install=0 dispatch_error=0
  kind_reply=0 kind_other_build=15 msgid_match=1
gateway-frame-detail = tunnel=38146 inner=11  (0x9502 OBEP_NEXT)
destination-material-real = outbound_slots=1 inbound_slots=1
  zero_hop=0 receive=38402  (0x9602 IBGW_NEXT)
outbound-installed = true
inbound-installed = true
```

Reference (unmodified):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
profile = notransit = false, floodfill = true, SAM loopback
bind = 127.0.0.1 ephemeral loopback only
advertise = false (i2pr)
```

Lane rows (21 rows: 13 passed, 5 blocked on lookup gap, 3 local):

```text
local-destination-tunnel-unit      = passed
local-destination-tunnel-live      = passed
local-tunnel-liveness              = passed
external-daemon-strict-profile     = passed
external-reference-verified        = passed
external-reference-floodfill       = passed
external-session-established       = passed
external-sam-destination-created   = passed
external-outbound-tunnel           = passed (installed_ob=1, NEW)
external-inbound-tunnel            = passed (installed_ib=1, NEW)
external-outbound-accepted         = passed
external-inbound-accepted          = passed
external-reference-ls2-published   = passed
external-lease-lookup-tunnel       = blocked (m6-lookup-gap; see below)
external-ls2-publication-tunnel    = blocked (m6-lookup-gap)
external-destination-outbound      = blocked (m6-lookup-gap)
external-reference-received        = blocked (m6-lookup-gap)
external-destination-inbound       = blocked (m6-lookup-gap)
external-direct-rejected           = passed
external-liveness-first-test       = passed
workspace-gates                    = passed
```

## Remaining gap (lookup, not build-reply)

With both tunnels installing through consumed reference replies,
the driver proceeds to `outbound-lookup-via-tunnel cells=1` but
`reference LeaseSet2 never resolved` within 45 s. TunnelData
decrypt/lookup composition for destination-owned tunnels is the
next narrow program; it is distinct from the build-reply gap closed
above (which owned only `installed_ob/ib` plus the three reply
consumption paths). No Streaming claim is made; the deferred
`188-m6-mixed-router-streaming-with-i2pd.md` stays blocked until
all seven destination rows pass.

Per the corrective §6 stop rule this is escalated as follow-up
work, not stretched inside this corrective: no install was
synthesized, no correlation was weakened, and evidence hygiene
(counts/metadata only, raw i2pd log never evidence, PRIV
in-memory only) is retained.

## Handoff

Finish Plan 188 by resolving the destination LeaseSet2-lookup gap
over the now-installed pair (reuse the Plan 186 exploratory NetDB
seams; do not add a second Garlic/routing stack, `LocalZeroHop`,
or direct-transport substitution), flipping the five remaining
`blocked` rows to passed in a fresh `run-destination.sh`, then
resuming the Plan 187 §12 handoff (destination rows green →
Streaming layers on top per the deferred streaming pass).

```text
plan_188 = in-progress-m6-build-reply-installs-proven
m6_destination_remote_interop = installs-proven-lookup-pending
next_executable_plan = 188 (complete lookup/publication/messaging rows)
```
