# Plan 114: Short-build terminal routing and tunnel-chain correction

- Status: **ready for implementation**
- Date: 2026-08-17
- Parent authority: `plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md`
- Immediate predecessors: `plans/112-status.md`, `plans/113-status.md`
- Supersedes for next-step sequencing: the post-Plan-113 statement that qualified external delivery is immediately unblocked
- Scope class: **narrow runtime-neutral short-build composition correction**
- External network gate: **none**
- Blocks: first qualified independent-router short-build delivery checkpoint
- Does not reopen: Plans 099-113 cryptographic or interop-harness work

## 1. Goal

Correct the remaining high-level routing/composition defects between `ShortBuildPath` and the already-corrected low-level ECIES-X25519 multi-record builder before any independent router consumes an i2pr ShortTunnelBuildMessage.

Plan 114 is intentionally small. It does **not** redesign short-build cryptography, fake-record handling, Noise state, multi-record preprocessing, reply decryption, or transport. It fixes only the metadata that determines where each hop forwards the tunnel and where the terminal hop sends the build reply or tunnel traffic.

The pass must close four tightly coupled issues:

1. the high-level builder currently assigns the terminal hop's `next_router_hash` to the terminal hop itself;
2. outbound paths cannot explicitly represent the OBEP reply-router identity at the `ShortBuildPath` boundary;
3. intermediate `next_tunnel` IDs are not required to equal the following hop's `receive_tunnel` ID, so a role-valid path can still encode a broken forwarding chain;
4. current high-level end-to-end tests are permissive enough to accept `InvalidReply` where a matched trajectory should deterministically establish.

At closure, the runtime-neutral builder must be able to prove, locally and byte-for-byte, that the decrypted request plaintext for every real hop contains the exact next-router and next-tunnel values implied by the configured path.

Only then may the qualified external-delivery checkpoint become the next executable action.

---

## 2. Why this corrective pass is required

### 2.1 Current terminal next-router derivation is wrong

`crates/i2pr-tunnel/src/short.rs::build_hop_specs()` currently derives intermediate `next_router_hash` values from the following hop, but uses the terminal hop's own router hash for the final real hop:

```rust
let next_router_hash = if index + 1 < path.hops.len() {
    &path.hops[index + 1].router_hash
} else {
    &hop.router_hash
};
```

That fallback is not a valid general terminal-routing rule.

For an **outbound** tunnel, the final real hop is the OBEP. Its request record's `next tunnel ID` and `next router identity hash` identify the reply path for the OutboundTunnelBuildReply. Current Java I2P passes an explicit `replyTunnel` and `replyRouter` into the terminal outbound record. Current i2pd calls `SetReplyHop(replyTunnelID, replyIdent)` on the final outbound hop.

For an **inbound** tunnel, the final real remote hop forwards toward the local creator. Current i2pd sets the final inbound real hop's next identity to the local router identity. The lower-level i2pr Plan 113 trajectory already models the same rule by supplying the `originator_hash` as the terminal inbound `next_router_hash`.

Therefore the high-level self-hash fallback must be removed.

### 2.2 Outbound reply-router identity is not representable

`ShortBuildPath` currently carries:

- direction;
- optional inbound `originator_hash`;
- creator tunnel ID;
- ordered `HopSpec` values;
- request time;
- next message ID;
- build options.

It does not carry an explicit outbound reply-router identity.

The final outbound `HopSpec.next_tunnel` can already represent the reply tunnel ID, but there is no equivalent high-level field for the reply router hash. As a result, `build_hop_specs()` has no correct value to serialize into bytes 8..39 of the OBEP request plaintext and falls back to the OBEP's own hash.

Plan 114 must make the outbound terminal reply router explicit at the path boundary rather than deriving or guessing it.

### 2.3 Intermediate tunnel-ID continuity is not validated

For every non-terminal real hop, the protocol forwarding chain requires:

```text
hop[i].next_router_hash == hop[i + 1].router_hash
hop[i].next_tunnel_id   == hop[i + 1].receive_tunnel_id
```

Current i2pr derives the first relationship at the high-level conversion boundary, but the second relationship is not enforced. `ShortBuildPath::validate()` checks only that each `receive_tunnel` and `next_tunnel` is nonzero.

This permits a path such as:

```text
hop0.next_tunnel    = 0x2000
hop1.receive_tunnel = 0x3000
```

which passes validation but cannot form a coherent forwarding chain.

Current Java I2P obtains each intermediate `nextTunnelId` from the next hop's receive tunnel ID. Current i2pd's `TunnelHopConfig::SetNext()` sets `nextTunnelID = next->tunnelID`.

Plan 114 must enforce this equality before cryptographic allocation.

### 2.4 The high-level E2E test does not prove success

The current `prepare_and_process_through_full_pipeline` test builds a two-hop state machine, feeds it a three-hop reference fixture, and accepts either:

```text
InvalidReply OR Established
```

That is useful as a structural smoke test but is not acceptable evidence for a pre-delivery closure.

A path declared locally ready for independent-router delivery must have a matched trajectory that deterministically reaches `Established`, with all request routing fields and reply record counts exactly matching the configured path.

Plan 114 must replace or supplement this permissive test with strict outbound and inbound trajectories.

---

## 3. Reference-backed routing semantics

### 3.1 Java I2P

Reference implementation:

```text
repository: i2p/i2p.i2p
file: router/java/src/net/i2p/router/tunnel/pool/BuildMessageGenerator.java
```

Current source behavior relevant to this pass:

- for a non-terminal hop, `nextTunnelId` is the next hop's receive tunnel ID and `nextPeer` is the next hop's router hash;
- for the final outbound hop, when a reply path is supplied, `nextTunnelId = replyTunnel` and `nextPeer = replyRouter`;
- outbound endpoint classification is applied only to the final outbound remote hop;
- inbound gateway classification is applied only to the first inbound remote hop.

Plan 114 does not need to copy Java object structure. It must preserve these wire semantics.

### 3.2 i2pd

Reference implementation:

```text
repository: PurpleI2P/i2pd
file: libi2pd/TunnelConfig.cpp
```

Current source behavior relevant to this pass:

- `TunnelHopConfig::SetNext()` sets the current hop's next identity to `next->ident` and `nextTunnelID` to `next->tunnelID`;
- the outbound constructor calls `m_LastHop->SetReplyHop(replyTunnelID, replyIdent)`;
- the inbound constructor sets the final real hop's next identity to the local router identity;
- the originator fake remains a separate inbound record and is unrelated to the terminal-routing bug fixed here.

### 3.3 i2pr lower-level evidence

The current Plan 113 lower-level inbound trajectory already supplies:

```text
next_hashes = [hop1, hop2, originator_hash]
```

and successfully round-trips one inbound build with exactly one originator fake.

This is important: Plan 114 is not correcting the low-level encrypted-record format. It is correcting the high-level path-to-record metadata adapter.

---

## 4. Required design

### 4.1 Preserve the existing low-level builder

Do not rewrite:

- `EciesX25519BuildCryptography`;
- Noise-N request state;
- reply/layer/IV/garlic KDFs;
- record-slot allocation;
- request preprocessing;
- `MessageHopProcessor`;
- `CreatorReplyPostprocessor`;
- originator-fake construction/integrity verification;
- count-prefixed STBM/OTBRM codec;
- Plan 111 fixed vectors;
- Plan 112 random-padding encoders.

Any change to these surfaces requires concrete evidence that Plan 114 cannot be completed through routing metadata alone.

### 4.2 Make outbound terminal reply router explicit

Preferred narrow API change:

```rust
pub struct ShortBuildPath {
    ...
    pub originator_hash: Option<Hash>,
    pub outbound_reply_router: Option<Hash>,
    ...
}
```

Direction-specific validation:

```text
Outbound:
  originator_hash        = None
  outbound_reply_router  = Some(...)

Inbound:
  originator_hash        = Some(...)
  outbound_reply_router  = None
```

Equivalent naming is acceptable if the semantic is equally explicit, e.g. `reply_router_hash`.

Do **not** use a generic ambiguous field such as `terminal_router` unless its direction-specific meaning is documented and validated.

Do **not** derive the outbound reply router from:

- the final hop;
- the first hop;
- a tunnel ID;
- a hash prefix;
- the local creator identity unless the caller explicitly selected the local creator as the reply router.

### 4.3 Retain explicit final `next_tunnel`

The existing final `HopSpec.next_tunnel` may remain the terminal next-tunnel value.

Document its meaning:

```text
Outbound final hop:
  next_tunnel = selected reply tunnel ID

Inbound final hop:
  next_tunnel = creator-side receive tunnel ID used by the final remote participant
```

Plan 114 must audit `creator_tunnel_id` before attempting to reuse it as the inbound terminal `next_tunnel` value. If `creator_tunnel_id` is already semantically that value, document and validate the equality. If it is only an attempt/pool identifier, keep it separate.

Do not silently alias these values based on naming alone.

### 4.4 Enforce intermediate tunnel-ID continuity

Add a path validation rule for every `i < hops.len() - 1`:

```rust
hops[i].next_tunnel == hops[i + 1].receive_tunnel
```

Failure must be typed and occur before request encryption or slot allocation.

Preferred error surface:

```rust
ShortBuildConstructionError::InvalidPath {
    reason: "intermediate next tunnel id does not match following receive tunnel id"
}
```

A dedicated variant is also acceptable if it improves diagnostics without broadening the API unnecessarily.

The lower-level `prepare_short_build_message()` path should also reject inconsistent chains when directly called. A caller must not be able to bypass the high-level invariant by constructing `MultiRecordHopSpec` values directly.

Recommended shared helper:

```text
validate_routing_chain(direction, hops, terminal metadata)
```

or a minimal lower-level continuity check adjacent to `validate_role_topology()`.

Do not rely only on tests or comments.

### 4.5 Derive next-router values literally

After validation, `build_hop_specs()` must follow this rule:

For every non-terminal hop:

```text
next_router_hash = path.hops[i + 1].router_hash
next_tunnel      = path.hops[i + 1].receive_tunnel
```

The implementation may either derive `next_tunnel` instead of storing it redundantly or retain the stored value after equality validation. Minimize API churn.

For the terminal outbound hop:

```text
next_router_hash = path.outbound_reply_router
next_tunnel      = final_hop.next_tunnel  // selected reply tunnel ID
```

For the terminal inbound hop:

```text
next_router_hash = path.originator_hash
next_tunnel      = final_hop.next_tunnel  // creator-side receive tunnel ID
```

No terminal self-hash fallback may remain in production code.

### 4.6 Keep role topology unchanged

Retain Plan 112 role validation:

```text
Outbound remote hops:
  Participant ... Participant -> OutboundEndpoint

Inbound remote hops:
  InboundGateway -> Participant ... Participant
```

Plan 114 changes routing metadata only.

### 4.7 Keep Plan 113 inbound policy unchanged

Retain:

```text
INBOUND_SHORT_BUILD_POLICY = "reference-compatible-spec-text-discrepancy"
```

and exactly one randomized originator fake:

```text
hash16 || fresh X25519 pub32 || random remainder
```

with creator-side integrity verification.

Plan 114 must not reinterpret request padding or add a new inbound creator-key plaintext field.

---

## 5. Required implementation phases

### Phase A — Add explicit terminal routing metadata

Modify `ShortBuildPath` to carry the outbound reply-router hash explicitly.

Required work:

1. add the field;
2. update constructors/fixtures/callers;
3. update `Debug` only if appropriate;
4. ensure no secret material is exposed;
5. add direction-specific validation for presence/absence.

Acceptance:

```text
outbound path without reply router -> rejected
inbound path without originator hash -> rejected
outbound path with inbound-only originator hash -> rejected or explicitly normalized by constructor
inbound path with outbound-only reply router -> rejected or explicitly normalized by constructor
```

Prefer rejection over silent normalization.

### Phase B — Validate tunnel-ID chain continuity

Implement and share the intermediate invariant:

```text
hop[i].next_tunnel == hop[i+1].receive_tunnel
```

Required negative tests:

1. swap one intermediate `next_tunnel` with an unrelated nonzero ID;
2. leave all role topology otherwise valid;
3. verify high-level `ShortBuildPath::validate()` rejects it;
4. verify lower-level builder rejects the equivalent `MultiRecordHopSpec` chain;
5. verify no crypto contexts or record payload are produced.

Remove or rewrite the current test whose comment says swapped tunnel IDs are merely "observable" while validation still accepts them. After Plan 114, that behavior is no longer acceptable.

### Phase C — Correct terminal next-router derivation

Replace the terminal self-hash fallback.

Required unit tests should directly inspect `build_hop_specs()` behavior where possible, or decrypt the resulting request records if the helper remains private.

Outbound terminal assertion:

```text
final role        = OutboundEndpoint
next router       = configured reply router
next tunnel       = configured reply tunnel
next router != implicitly replaced by OBEP identity
```

Inbound terminal assertion:

```text
final role        = Participant (unless one-hop inbound where first is IBGW and also terminal under supported semantics; see §8.4)
next router       = configured creator/originator identity
next tunnel       = configured creator-side receive tunnel
```

### Phase D — Add exact plaintext routing assertions

Build a complete STBM through `ShortBuildStateMachine::prepare()` using deterministic test RNG and known static private keys for each hop.

For each real hop:

1. undo the prior-hop preprocessing necessary to expose that hop's request at its processing stage, or drive the payload sequentially through `MessageHopProcessor`;
2. open the request with the hop's static key;
3. decode `ShortRequestRecord`;
4. assert exact `receive_tunnel`, `next_tunnel`, `next_router`, and `role`.

Required outbound fixture, minimum two real hops:

```text
creator
  -> participant A
  -> OBEP B
  -> reply tunnel R on router Q
```

Assertions:

```text
A.receive_tunnel = A.id
A.next_tunnel    = B.receive_tunnel
A.next_router    = B.router_hash

B.receive_tunnel = B.id
B.next_tunnel    = R
B.next_router    = Q
B.role           = OutboundEndpoint
```

Required inbound fixture, minimum two real hops:

```text
IBGW A
  -> participant B
  -> creator C
```

Assertions:

```text
A.receive_tunnel = A.id
A.next_tunnel    = B.receive_tunnel
A.next_router    = B.router_hash
A.role           = InboundGateway

B.receive_tunnel = B.id
B.next_tunnel    = C.receive_tunnel
B.next_router    = C.router_hash/originator_hash
B.role           = Participant
```

The originator fake must remain present exactly once and must still verify after reply processing.

### Phase E — Replace permissive high-level E2E acceptance

Replace or narrow the test that accepts either `InvalidReply` or `Established` from mismatched fixture topology.

Required outbound strict trajectory:

```text
prepare
-> validate delivery action
-> process exact hop A
-> process exact hop B
-> feed exact OTBRM payload
-> Established
```

Required inbound strict trajectory:

```text
prepare
-> verify exactly one originator fake
-> process exact inbound hops
-> feed exact reply payload
-> originator fake verifies
-> Established
```

The test must fail if:

- terminal router hash is changed;
- terminal next tunnel is changed;
- one intermediate next tunnel is changed;
- record count changes;
- originator fake is modified;
- a hop reply is rejected.

Do not accept multiple terminal outcomes in the success fixtures.

### Phase F — Update authority/status surfaces

On successful implementation, add `plans/114-status.md` and update only the status surfaces needed to make the next step unambiguous.

Required final authority state:

```text
plan_111                         = retained-core-crypto-corrected
plan_112                         = passed-outbound-pre-delivery-closure
plan_113                         = passed-inbound-reference-reconciliation
plan_114                         = passed-terminal-routing-chain-correction
intermediate_next_tunnel_chain   = validated
outbound_terminal_reply_router   = explicit-and-serialized
inbound_terminal_creator_router  = explicit-and-serialized
high_level_outbound_e2e          = strict-established
high_level_inbound_e2e           = strict-established
qualified_external_delivery      = unblocked-next-checkpoint
milestone4b                      = still-blocked-on-independent-router-evidence
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
```

Before Plan 114 passes, authority must instead say:

```text
qualified_external_delivery = blocked-on-plan114
```

---

## 6. Acceptance criteria

Plan 114 closes only if **all** criteria below pass.

### 6.1 API/model acceptance

- [ ] `ShortBuildPath` can explicitly represent the outbound OBEP reply-router identity.
- [ ] inbound creator identity remains explicit through `originator_hash` or an equally clear replacement.
- [ ] direction-specific terminal metadata is validated rather than guessed.
- [ ] no production terminal self-hash fallback remains.
- [ ] final `next_tunnel` semantics are documented for both directions.
- [ ] `creator_tunnel_id` semantics are audited and documented; no accidental alias is introduced.

### 6.2 Routing-chain acceptance

- [ ] every intermediate hop's `next_router` resolves to the following hop router hash.
- [ ] every intermediate hop's `next_tunnel` equals the following hop's `receive_tunnel`.
- [ ] inconsistent nonzero tunnel IDs fail before cryptographic allocation.
- [ ] the lower-level multi-record API cannot bypass the chain invariant.

### 6.3 Outbound terminal acceptance

- [ ] final outbound remote hop is `OutboundEndpoint`.
- [ ] final outbound `next_router` equals the explicitly configured reply router.
- [ ] final outbound `next_tunnel` equals the explicitly configured reply tunnel ID.
- [ ] decrypted OBEP request plaintext proves those exact values.
- [ ] the terminal router is not implicitly set to the OBEP itself.

### 6.4 Inbound terminal acceptance

- [ ] first inbound remote hop remains `InboundGateway`.
- [ ] later inbound remote hops remain `Participant`.
- [ ] terminal inbound `next_router` equals the explicit creator/originator router identity.
- [ ] terminal inbound `next_tunnel` equals the configured creator-side receive tunnel ID.
- [ ] decrypted terminal inbound request plaintext proves those exact values.
- [ ] exactly one originator fake remains present and integrity-checked.

One-hop inbound semantics must be checked against the current reference behavior before adding a special role exception. Do not weaken the role validator merely to make a one-hop fixture convenient. If one-hop inbound requires a distinct representation, document and defer it unless it blocks the intended minimum exploratory tunnel topology.

### 6.5 End-to-end test acceptance

- [ ] matched outbound high-level trajectory deterministically reaches `Established`.
- [ ] matched inbound high-level trajectory deterministically reaches `Established`.
- [ ] neither success test allows `InvalidReply` as an acceptable alternative.
- [ ] terminal router mutation causes deterministic failure.
- [ ] intermediate tunnel-chain mutation causes deterministic validation failure.
- [ ] originator fake mutation causes deterministic inbound failure.

### 6.6 Regression acceptance

- [ ] Plan 111 fixed-vector tests remain unchanged and passing unless a test-only constructor update is necessary.
- [ ] Plan 112 random-padding production path remains CSPRNG-backed.
- [ ] Plan 113 inbound policy string and evidence note remain intact.
- [ ] count-prefixed STBM/OTBRM framing remains `1 + count * 218`.
- [ ] no NTCP2 behavior changes.
- [ ] no Python interop code is added.
- [ ] no Docker, Multipass, root, or namespace requirement is added.

---

## 7. Required verification commands

Run the repository's currently declared toolchain/MSRV policy rather than copying obsolete version literals from historical plans.

Minimum required checks:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

If the repository's CI wrapper or pinned toolchain requires an explicit `+<toolchain>`, use the value declared in `rust-toolchain.toml` and current CI rather than a historical plan value.

Do not make unrelated dependency updates part of this pass.

---

## 8. Explicit non-goals

Plan 114 does **not** authorize:

- normal-daemon NTCP2 activation;
- reopening Plan 079;
- another NTCP2 wire-debugging campaign;
- Java I2P/i2pd/Emissary live execution as a Plan 114 closure gate;
- public I2P participation;
- SSU2 implementation;
- generic I2NP dispatch;
- transit tunnel data-plane implementation;
- NetDB lookup execution;
- exploratory-pool policy redesign;
- a new Python harness;
- containers, network namespaces, root, or Multipass;
- changing the Plan 113 reference-compatible inbound creator-key policy;
- optimizing record counts, path selection, latency, or throughput.

This pass exists solely to prevent the first real short-build delivery from failing because i2pr encoded the wrong terminal destination or a broken tunnel-ID chain.

---

## 9. Implementation guidance for a smaller coding model

Work in this order and do not broaden scope:

1. Read `crates/i2pr-tunnel/src/short.rs` and locate `ShortBuildPath`, `validate()`, `build_hop_specs()`, and the high-level tests.
2. Read `crates/i2pr-tunnel/src/multirecord.rs` and locate `MultiRecordHopSpec`, `validate_role_topology()`, and `prepare_short_build_message()`.
3. Add the outbound reply-router field to `ShortBuildPath`.
4. Update fixtures until the code compiles; do not change crypto.
5. Add direction-specific presence validation.
6. Add intermediate tunnel-ID continuity validation at both public layers.
7. Replace the terminal self-hash fallback with direction-specific terminal routing.
8. Add request-plaintext assertions before attempting full E2E tests.
9. Add exact outbound E2E success.
10. Add exact inbound E2E success with originator-fake verification.
11. Remove the success test's `InvalidReply | Established` ambiguity.
12. Run focused `i2pr-tunnel` tests.
13. Run workspace checks.
14. Update status/authority docs only after all checks pass.

If a failure appears inside Noise, KDF, AEAD, raw ChaCha20, or record layout, first determine whether the routing metadata is simply exposing a pre-existing test assumption. Do not rewrite those primitives without new protocol evidence.

---

## 10. Stop conditions

Stop and record a blocker instead of guessing if any of these occur:

1. current Java I2P and i2pd disagree on terminal outbound reply-router/tunnel semantics;
2. the intended meaning of `creator_tunnel_id` cannot be established from current code/plans and affects inbound terminal tunnel ID selection;
3. the current I2P specification requires a terminal-routing field not representable without a broader state-machine API change;
4. strict matched high-level trajectories expose a cryptographic failure that is reproducible with routing metadata proven correct.

If condition 4 occurs, capture the exact failing stage and create a separate narrow follow-up. Do not silently expand Plan 114 into another general short-build conformance program.

---

## 11. Closure result and next step

Successful Plan 114 closure means:

```text
short_build_crypto_core          = retained
short_build_padding              = retained-random-csprng
short_build_role_topology        = retained-validated
inbound_originator_fake          = retained-reference-compatible-and-verified
intermediate_router_chain        = exact
intermediate_tunnel_id_chain     = exact
outbound_terminal_route          = explicit-reply-router-and-tunnel
inbound_terminal_route           = explicit-creator-router-and-tunnel
high_level_e2e_outbound          = deterministic-established
high_level_e2e_inbound           = deterministic-established
qualified_external_delivery      = unblocked-next-checkpoint
```

The next step after successful closure is the **smallest qualified independent-router delivery checkpoint**. That later checkpoint must consume the exact `ShortBuildAction::Deliver` payload generated by the corrected state machine; it must not restart the historical broad interoperability harness program.
