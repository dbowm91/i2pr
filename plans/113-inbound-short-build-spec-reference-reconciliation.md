# Plan 113: Inbound short-build specification/reference reconciliation

- Status: **passed-inbound-reference-reconciliation**
- Date: 2026-08-15
- Parent roadmap: `plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`
- Parent authority: `plans/102-amendment-exploratory-tunnel-dependency.md`
- Predecessors: Plan 111 core, Plan 112 inbound fail-closed gate
- Scope class: **narrow inbound semantics reconciliation**
- External network gate: **none required for local closure**
- Blocks: production inbound short-build enablement and inbound external-delivery testing
- Does not block: outbound-only external-delivery checkpoint after Plan 112

## 1. Goal

Resolve the remaining discrepancy between the final I2P ECIES-X25519 Tunnel Creation Specification and the behavior of current Java I2P and i2pd implementations for **inbound** short tunnel builds.

The specific question is not whether an inbound build requires an originator fake — all sources agree that it does. The question is whether an additional creator ephemeral public key must also be serialized inside the 154-byte **real request plaintext**, and if so, exactly how.

Plan 113 must end with one explicit result:

1. **resolved and enabled** — a source-backed interoperable inbound representation is implemented locally; or
2. **unresolved and fail-closed** — inbound remains disabled while outbound progress continues.

A guessed private layout is not an acceptable result.

## 1A. Closure result

Policy B is selected: `reference-compatible-spec-text-discrepancy`. The final
specification does not define a concrete plaintext creator-key encoding, while
the pinned Java I2P and i2pd implementations agree on the deployed originator
fake. i2pr therefore retains the normal fixed-field + Mapping/padding request,
requires explicit inbound creator identity, emits exactly one
`hash16 || fresh X25519 pub32 || random remainder` originator fake, and checks
its creator-side integrity after reply processing. Strict final-spec text
conformance is not claimed for the unresolved creator-key sentence.

## 2. Why this is separate from Plan 112

The post-Plan-111 review originally treated inbound creator-key placement as simply “missing.” Deeper research found that this was too simplistic.

The current final specification says the creator ephemeral public key is included in an inbound plaintext record, but its fixed 154-byte layout table provides no byte range for that key.

At the same time, two current independent router implementations agree on a different visible construction pattern:

- real short request plaintext remains the normal fixed fields + Mapping + padding;
- the inbound build includes a dedicated originator fake record;
- that fake record begins with the creator's truncated hash and a fresh X25519 public key;
- no separate creator-key field is visible in the real short-request constructors.

This is a standards/reference discrepancy, not an ordinary implementation omission.

Plan 112 therefore gates production inbound construction explicitly. Plan 113 owns the evidence and compatibility decision without blocking outbound work.

## 3. Pinned evidence

### 3.1 Final specification

Source:

`https://i2p.net/en/docs/specs/tunnel-creation-ecies/`

Observed metadata:

```text
Updated: 2025-06
Accurate for: 0.9.66
```

Relevant short request layout:

```text
0-3    receive tunnel ID
4-7    next tunnel ID
8-39   next router identity hash
40     flags
41-42  reserved flags
43     layer encryption type
44-47  request time
48-51  expiration
52-55  next message ID
56-x   Mapping
x-x    other data implied by flags/options
x-153  random padding
```

Immediately after the layout, the specification states that an inbound build includes the creator ephemeral public key in plaintext because there is no DH “at this layer” for deriving IBGW layer/reply material.

The layout does not define:

- a fixed offset;
- a Mapping key;
- an option flag indicating presence;
- whether the key consumes padding bytes;
- whether the statement instead refers to the originator fake record described later.

The implementation-notes section separately requires an inbound originator fake record with the creator's correct 16-byte hash prefix and a real X25519 ephemeral key.

### 3.2 Java I2P master

Pinned tree:

```text
repo    = i2p/i2p.i2p
branch  = master
commit  = 498488b0d01d9f59efe906424e56ff5e25f58a4d
date    = 2026-08-14
```

Relevant files:

- `router/java/src/net/i2p/data/i2np/BuildRequestRecord.java`
- `router/java/src/net/i2p/router/tunnel/pool/BuildMessageGenerator.java`

Observed real short-request construction:

- fixed fields are serialized through byte 55;
- Mapping is serialized at byte 56;
- remaining plaintext is random padding;
- no explicit creator-ephemeral field is exposed by the short constructor.

Observed inbound originator-fake construction in `BuildMessageGenerator.createRecord(...)` when the creator/IBEP slot is blank:

```text
0..16    local creator/router truncated identity hash
16..48   fresh X25519 public key
48..218  random bytes
```

The Java creator stores a SHA-256 hash over the complete fake record for later modification detection.

Observed role semantics:

```text
inbound first remote hop = IBGW
inbound remaining remote hops = participants
local creator = IBEP represented by blank/fake build slot, not a real remote-hop request
```

### 3.3 i2pd current source

Pinned tree:

```text
repo    = PurpleI2P/i2pd
branch  = openssl
commit  = dfcb8a8043c0c689e5681c5ae5da89df5643347e
date    = 2026-08-14
```

Relevant file:

`libi2pd/TunnelConfig.cpp`

Observed real short-request construction:

- fixed short fields + Mapping;
- no separate creator-ephemeral field;
- currently zero padding after Mapping (a separate spec divergence handled by Plan 112).

Observed `ShortPhonyTunnelHopConfig::CreateBuildRequestRecord(...)`:

```text
0..16    local creator/router truncated identity hash
16..48   fresh X25519 public key
48..218  random bytes
```

Observed role configuration:

- inbound first remote hop starts as gateway;
- subsequent remote hops are not gateway;
- final remote inbound hop points to creator and is not marked outbound endpoint.

### 3.4 Existing i2pr behavior

Current i2pr already has a strong originator-fake primitive:

- `OriginatorFake` stores hash prefix, X25519 public key, full 218-byte wire record, and integrity hash;
- `build_originator_fake_record()` emits:
  - local hash prefix at `0..16`;
  - fresh X25519 pub at `16..48`;
  - random remainder;
- `verify_originator_fake()` rejects modification using a creator-side SHA-256 integrity hash.

This structure is closely aligned with both current Java I2P and i2pd.

The unresolved portion is the final-spec prose about an additional plaintext creator key, not the fake-record format.

## 4. Authority rule for this discrepancy

Normally the final specification wins over implementation details.

Here, the final text is incomplete for implementation because it does not define a serializable location, while two independent current implementations agree on the same deployed construction pattern.

Plan 113 must therefore use the following decision rule:

1. search current upstream source history, comments, issue trackers, and any newer spec revision for a concrete serialization definition;
2. if a concrete final-spec-compatible definition exists, implement it exactly;
3. if no such definition exists and Java I2P + i2pd still agree on the originator-fake-only visible representation, document that agreement explicitly;
4. do not invent a field from padding bytes solely to satisfy prose;
5. if enabling reference-compatible inbound behavior despite unresolved spec prose, label it precisely as **reference-compatible / spec-text discrepancy**, not strict final-spec conformance;
6. if evidence becomes inconsistent, keep inbound disabled.

## 5. Non-goals

Plan 113 must not:

- run a public I2P node as a closure requirement;
- activate NTCP2;
- repair NTCP2;
- implement SSU2;
- add Python harnesses;
- build a generic I2NP dispatcher;
- change outbound short-build crypto;
- change Plan 112 random-padding behavior;
- implement mixed ElGamal/ECIES builds;
- implement tunnel data-plane forwarding;
- require root, containers, namespaces, or privileged networking.

## 6. Work package A — pin and archive source evidence

Before touching inbound behavior, add a concise repository-local evidence note containing:

- final spec URL and observed metadata;
- exact final-spec sentence that causes the discrepancy, paraphrased if necessary;
- Java I2P repo/commit/path/method names;
- i2pd repo/commit/path/method names;
- summary of the shared deployed originator-fake format;
- summary of the missing separate plaintext field in both implementations.

Do not copy large upstream code blocks into the repository.

Recommended location:

```text
specs/references/short-build-inbound-creator-key.md
```

or an equivalent existing references location.

The note is evidence, not a new specification.

## 7. Work package B — inspect upstream history narrowly

Research only the affected semantics.

Required searches:

### Java I2P

- history of `BuildRequestRecord.java` short-record constructor;
- history of `BuildMessageGenerator.java` inbound blank/originator fake;
- commits around API 0.9.51 / Proposal 157 rollout;
- comments/issues mentioning creator ephemeral key, IBEP fake, short inbound build, or phony record.

### i2pd

- history of `ShortECIESTunnelHopConfig::CreateBuildRequestRecord`;
- history of `ShortPhonyTunnelHopConfig::CreateBuildRequestRecord`;
- issues/commits for Proposal 157 or short inbound builds.

### Specification

- current final ECIES tunnel-creation page;
- Proposal 157 only as historical rationale;
- current I2NP message-flow notes.

Stop research once one of these is established:

1. explicit offset/encoding is found;
2. explicit upstream statement says the prose refers to the fake record;
3. explicit upstream statement says the prose is stale/erroneous;
4. no clarification exists and both current implementations remain aligned.

Do not turn Plan 113 into an open-ended archaeology project.

## 8. Work package C — choose one explicit inbound policy

After Work package B, write the selected policy into code comments and `plans/113-status.md`.

### Policy A — strict-spec clarified

Use only if an authoritative concrete encoding is found.

Requirements:

- implement the defined field exactly;
- update request codec bounds correctly;
- ensure Mapping and random padding remain valid;
- add independent fixtures;
- update Java/i2pd compatibility analysis.

### Policy B — deployed-reference compatible

Use if no concrete spec encoding exists but current Java I2P and i2pd remain aligned.

Requirements:

- do **not** add a private creator-key field to real request plaintext;
- retain normal real request layout;
- retain mandatory originator fake:
  `hash16 || fresh X25519 pub32 || random remainder`;
- retain creator-side modification detection;
- mark inbound support as:
  `reference-compatible-spec-text-discrepancy`;
- do not claim strict final-spec conformance for this one semantic;
- cite pinned reference commits in docs/status.

### Policy C — unresolved

Use if evidence diverges or upstream behavior has changed incompatibly.

Requirements:

- leave Plan 112 inbound production gate enabled;
- close Plan 113 as `inbound-spec-reference-discrepancy-unresolved`;
- outbound remains unaffected.

## 9. Work package D — make inbound high-level state explicit if enabled

Only Policies A or B authorize enabling inbound construction.

Current `ShortBuildStateMachine::prepare()` does not own the creator identity hash and currently passes `originator_hash = None`.

If inbound is enabled, add explicit originator identity material to the validated high-level path.

Acceptable shape:

```rust
pub struct ShortBuildPath {
    ...
    pub originator_hash: Option<Hash>,
}
```

or a direction-specific typed configuration that avoids meaningless optional fields.

Prefer the smallest representation that keeps invalid states difficult to construct.

Requirements:

- outbound must not require originator fake identity;
- inbound must require originator identity before crypto/slot allocation;
- missing inbound originator identity returns a typed validation error;
- do not infer creator identity from `first_hop`, `next_router`, tunnel ID, or unrelated state.

## 10. Work package E — retain canonical inbound topology

If inbound is enabled, enforce:

```text
first remote hop = InboundGateway
remaining remote hops = Participant
local creator = inbound endpoint, represented separately from remote-hop roles
```

No remote hop may be `OutboundEndpoint`.

The originator fake is not a real hop and must not appear in `ShortBuildPath.hops` as a fake router identity.

## 11. Work package F — originator fake exactness

Existing i2pr fake behavior is valuable and should largely be retained.

Required invariants:

- exactly one originator fake for inbound production builds;
- randomized slot placement;
- correct creator hash prefix at bytes `0..16`;
- fresh X25519 public key at bytes `16..48`;
- random bytes for `48..218`;
- full-record integrity value retained by creator before dispatch;
- after reply preprocessing/postprocessing, the creator verifies the fake was not modified;
- modification of hash prefix, ephemeral key, or remainder rejects the tunnel;
- fake key material is fresh per build;
- fake private key, if generated only to produce a public key and not otherwise required, is zeroized immediately.

Cross-check these against both pinned Java and i2pd implementations.

## 12. Work package G — inbound multi-record trajectory

If enabled, add a deterministic local trajectory that is independent of the outbound fixture.

Fixture shape:

```text
2 or 3 real inbound remote hops
+ exactly 1 originator fake
+ padding fake if needed to reach record-count policy
```

The fixture must verify:

- first remote hop is IBGW;
- later remote hops are participants;
- originator fake is in a randomized wire slot;
- each real request is encrypted and preprocessed correctly;
- each remote hop locates/open its own record;
- each remote hop inserts its own reply and transforms all other records;
- creator removes accumulated transforms;
- creator authenticates all real replies;
- creator verifies originator fake integrity;
- all accepted replies produce success;
- any fake modification rejects success.

Do not require a socket or external router for this local trajectory.

## 13. Work package H — inbound delivery semantics boundary

Plan 113 owns local message semantics only.

It must document, but not implement, the later delivery rule:

- inbound STBM is delivered through an existing outbound tunnel toward the new IBGW;
- the creator eventually receives the transformed build message as inbound endpoint;
- this is not the same transport path as direct outbound STBM delivery to a first remote hop.

Do not create that delivery adapter in Plan 113.

If this boundary reveals a missing runtime-neutral action type, define only the minimum semantic action necessary; do not build the transport.

## 14. Work package I — tests for policy enforcement

Always required:

- Plan 112 production inbound gate remains active until Plan 113 policy decision is applied;
- no code path silently enables inbound merely because `originator_hash` is present;
- no guessed 32-byte field appears in request padding.

If Policies A/B enable inbound:

- inbound path without originator hash -> reject;
- inbound path first role not IBGW -> reject;
- inbound path with OBEP -> reject;
- duplicate/missing originator fake -> reject;
- modified fake -> reject;
- fake slot deterministic under seeded test RNG and different under different seeds;
- fake body randomized;
- complete inbound local trajectory passes;
- outbound fixtures remain byte-correct and unaffected.

If Policy C retains the gate:

- every public production inbound builder returns the typed disabled/reconciliation error;
- component fake-record tests remain green.

## 15. Work package J — documentation/status semantics

On closure, create `plans/113-status.md` with one exact state.

### If enabled from authoritative clarification

```text
plan_113                    = passed-inbound-spec-reconciliation
inbound_short_build         = locally-conformant
creator_key_semantics       = source-pinned
originator_fake             = implemented-and-integrity-checked
inbound_external_delivery   = next-later-checkpoint
```

### If enabled for deployed compatibility with unresolved spec prose

```text
plan_113                    = passed-inbound-reference-reconciliation
inbound_short_build         = locally-reference-compatible
creator_key_semantics       = deployed-reference-policy
spec_text_discrepancy       = documented
originator_fake             = implemented-and-integrity-checked
inbound_external_delivery   = eligible-for-independent-check
```

### If unresolved

```text
plan_113                    = closed-inbound-spec-reference-discrepancy-unresolved
inbound_short_build         = disabled
outbound_short_build        = unaffected
outbound_external_delivery  = allowed-after-plan112
```

Update:

- `plans/102-amendment-exploratory-tunnel-dependency.md`;
- `AGENTS.md` current state;
- `README.md` only if it currently overstates inbound support;
- `docs/protocol-support.md`;
- `docs/architecture/i2pr-tunnel.md`;
- `specs/support.toml`.

Do not hide the spec/reference discrepancy if Policy B is chosen.

## 16. Validation

Minimum local validation:

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

No new CI workflow and no live router process are required for Plan 113 local closure.

## 17. Explicit acceptance criteria

Plan 113 closes only if all applicable items are true.

### Evidence

- [x] Final spec discrepancy is quoted/paraphrased and source-pinned.
- [x] Java I2P current source is pinned by commit and inspected.
- [x] i2pd current source is pinned by commit and inspected.
- [x] Upstream history search is bounded and documented.
- [x] No invented offset or private option is introduced.

### Policy

- [x] One and only one of Policy A, B, or C is selected.
- [x] Selected policy is reflected in code behavior and docs.
- [x] Strict-conformance wording is not used if Policy B remains spec-text-discrepant.

### Inbound construction, if enabled

- [x] Originator identity is explicit at high-level path boundary.
- [x] First remote hop is IBGW; remaining remote hops are participants.
- [x] Exactly one originator fake is emitted.
- [x] Fake is `hash16 || fresh X25519 pub32 || random remainder` unless authoritative Policy A evidence explicitly requires a different final layout.
- [x] Fake integrity is verified before tunnel success.
- [x] Full local inbound multi-record trajectory passes.

### If disabled

- [ ] All production inbound construction paths fail with a typed intentional error.
- [ ] Outbound Plan 112 state is unchanged.
- [ ] Status documentation explicitly says inbound remains disabled.

### Scope

- [ ] No NTCP2 activation/repair.
- [ ] No SSU2.
- [ ] No Python harness.
- [ ] No root/container/namespace dependency.
- [ ] No generic dispatcher.
- [ ] No public-network claim.

## 18. Stop conditions

Stop and close fail-closed if:

- Java I2P and i2pd current implementations diverge materially on inbound originator-fake semantics;
- a newer final spec assigns a concrete field but current references do not implement it and the compatibility impact cannot be resolved locally;
- enabling inbound requires transport/tunnel-data-plane implementation;
- evidence would require guessing how padding bytes are reinterpreted;
- the research starts expanding into unrelated historical proposals.

## 19. Handoff after success

Plan 113 does not automatically authorize public-network use.

If inbound is enabled locally, the next inbound-specific checkpoint is a **small independent-router delivery test** using the already-defined message semantics.

Plan 113 does not authorize public-network use. The next inbound-specific
checkpoint is a small independent-router delivery test using the existing
message semantics; no dispatcher or transport adapter is added here.
