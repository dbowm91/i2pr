# Plan 115: qualified independent short-build consumption and external-delivery checkpoint

## Status and authority

- Status: **ready for execution**.
- Date: 2026-08-17.
- Immediate predecessor: [`plans/114-status.md`](114-status.md).
- Handoff: [`plans/115-handoff.md`](115-handoff.md).
- Roadmap continuation: [`plans/115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md).
- Milestone relationship: first independent-implementation evidence checkpoint for Milestone 5 tunnel construction; it does **not** by itself close Milestone 3, Milestone 4, or Milestone 5.

Current authority entering this plan:

```text
plan_111                         = retained-core-crypto-corrected
plan_112                         = passed-outbound-pre-delivery-closure
plan_113                         = passed-inbound-reference-reconciliation
plan_114                         = passed-terminal-routing-chain-correction
short_build_local_outbound       = strict-established
short_build_local_inbound        = strict-established
qualified_external_delivery      = unblocked-next-checkpoint
milestone4b                      = blocked-on-independent-router-evidence
milestone5_mixed_router_exit     = blocked-on-independent-router-evidence
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
development_ntcp2                = protocol-defect-localized-at-noise_authenticated
```

Plan 115 supersedes the generic "smallest available qualified external-delivery checkpoint" language in Plan 114. It is the only currently executable plan on this line of work.

## 1. Purpose

The project has enough local short-build machinery. The next question is no longer whether i2pr can construct and consume its own ECIES-X25519 short tunnel-build messages. Plans 111-114 answer that locally.

Plan 115 must answer a narrower and more valuable question:

> Can a Plan-114-correct production i2pr ShortTunnelBuild message be consumed by one independent I2P implementation, and how far can the same bytes travel through an already-existing external delivery seam in the current unprivileged Ubuntu environment?

The plan deliberately separates **tunnel-protocol interoperability** from **transport interoperability**. This prevents the historical NTCP2 environment/harness blocker from becoming a prerequisite for learning whether the short-build implementation itself is compatible with another router.

The minimum useful result is independent native consumption of the exact production-generated STBM by one reference implementation. The preferred stronger result is authenticated process-to-process delivery and a reply that i2pr postprocesses to `Established`.

A failure must be localized to a named stage. "Interop failed" is not an acceptable result.

## 2. Strategic decision

### 2.1 Independent consumer evidence comes before another transport campaign

Use one independent implementation initially. Preferred candidates are:

1. **i2pd**, because it is a mature deployed implementation and the repository already retains a Plan 099 i2pd reference-build path and pinned-reference history;
2. **Emissary**, if its current native tunnel-build API exposes a materially smaller direct consumption seam in the available environment;
3. Java I2P only if neither i2pd nor Emissary can provide a bounded consumer path without materially expanding tooling.

One implementation is sufficient for Plan 115. The original Milestone 3 requirement for two independent NTCP2 implementations remains a separate deferred transport gate and must not be smuggled into this plan.

### 2.2 Evidence tiers are intentionally distinct

Plan 115 recognizes three increasing evidence levels:

```text
Q0 = independent native short-build consumer evidence
Q1 = authenticated transport delivery to the independent router
Q2 = independent-router build reply returned to i2pr and Established
```

Definitions:

- **Q0**: the independent implementation receives the exact i2pr-generated I2NP ShortTunnelBuild message, or its exact STBM body at a documented native parser boundary, executes its own deployed short-build parsing/record cryptography, and produces a native accept/reject result and, when its API supports it, the corresponding reply bytes. No i2pr parser, crypto primitive, or copied reference algorithm may stand in for the independent implementation.
- **Q1**: the same production-generated encoded I2NP message reaches the independent process over an authenticated router transport and reaches that implementation's I2NP/tunnel-build dispatch path.
- **Q2**: the independent implementation produces the appropriate reply, the reply is returned through the selected lane, and `ShortBuildStateMachine` reaches `Established` after `BuildEvent::BuildReply`.

A Q0 pass is a real protocol-interoperability result but is **not** a live mixed-router transport result. A Q1 pass is transport delivery evidence but is not a complete tunnel build unless reply processing is observed. Q2 is the strongest Plan 115 result.

### 2.3 A Q0 pass is enough to unblock local Milestone 5 data-plane construction

If Q0 passes but Q1/Q2 is blocked by the existing NTCP2/environment lane, record the transport blocker and move to the Plan 116 data-plane phase described by the roadmap note. Do not hold all router construction hostage to the historical transport harness.

Q1/Q2 remain mandatory before claiming the mixed-router Milestone 5 exit or live Milestone 4 NetDB acceptance.

## 3. Protocol and repository facts that constrain the implementation

The execution model must preserve these facts:

1. I2P ShortTunnelBuild is I2NP type 25 and OutboundTunnelBuildReply is type 26. Short build bodies are variable-record bodies with a one-byte record count and 218-byte records.
2. The production tunnel builder already emits a validated count-prefixed STBM body via `ShortBuildAction::Deliver`:

   ```text
   Deliver {
       first_hop,
       message,       // count-prefixed STBM body
       record_count,
       deadline_ms,
   }
   ```

3. `i2pr-proto` already models `I2npBody::ShortTunnelBuild` and `I2npBody::OutboundTunnelBuildReply`, plus standard and short-transport I2NP headers.
4. `DeferredBuildRecords::new(count, 218, records)` expects **raw record bytes only** (`count * 218` bytes). The `I2npBody::ShortTunnelBuild` encoder writes the variable-record count itself.
5. Therefore the Plan 115 bridge must never wrap the already count-prefixed `ShortBuildAction::Deliver.message` directly as `DeferredBuildRecords`. Doing so would double-prefix the count and generate a non-canonical body.
6. `i2pr-transport` already owns `EncodedI2npMessage`, `DeliveryRequest`, bounded queues, and typed `DeliveryOutcome` values. The plan should connect to this existing boundary rather than introduce a second generic delivery abstraction.
7. Normal `i2pr-daemon` NTCP2 activation remains forbidden under Plan 101 authority. Experimental/test composition may use `i2pr-runtime` or `tools/i2pr-interop`; normal daemon configuration may not be changed.
8. Plan 099 remains closed at `protocol-defect-localized`, highest observed stage `noise_authenticated`. Plan 115 may reuse useful Rust/runtime/reference components from that work but may not restart its broad workflow or recreate deleted plan-specific Python machinery.

Official references to pin in the closure record:

- I2P I2NP specification: `https://geti2p.net/spec/i2np`
- I2P ECIES-X25519 Tunnel Creation specification: `https://geti2p.net/spec/tunnel-creation-ecies`
- If i2pd is selected: exact upstream repository revision under `https://github.com/PurpleI2P/i2pd`
- If Emissary is selected: exact upstream/fork revision used by the execution environment; the closure must state which repository is the independent oracle and why it is independent from i2pr.

Do not cite an unpinned `main` branch as execution evidence.

## 4. Hard environment and scope constraints

### Required environment behavior

- Ubuntu user-space execution.
- No requirement for `sudo`.
- No requirement for unprivileged user namespaces.
- No `ip netns`, veth, nftables, `setcap`, privileged containers, or VM orchestration.
- Host loopback is acceptable for the development lane.
- A non-public development network identifier must be used when a real router transport is exercised; reuse the existing Plan 099 network-id 99 convention when compatible with the selected reference implementation.
- No public I2P network participation is required or authorized by this plan.

### Explicit non-goals

Plan 115 must not:

- enable or advertise NTCP2 in `i2pr-daemon`;
- implement SSU2;
- implement a generic inbound I2NP router dispatcher;
- implement TunnelData forwarding, fragmentation, or reassembly;
- implement NetDB-over-tunnel traffic;
- add a new Python harness;
- resurrect deleted plan-number-specific Python orchestration;
- add a new GitHub Actions workflow merely to obtain this evidence;
- require rootless namespace isolation;
- modify Plans 109-114 cryptographic/layout behavior without a separately justified corrective plan;
- claim Milestone 3 closure from a one-reference result;
- claim Milestone 5 closure from Q0 or Q1 alone.

## 5. Work package A — preflight and lane inventory

Before editing code, write a short execution note in the eventual `plans/115-status.md` recording the inspected baseline and candidate lane decision.

### A1. Confirm the production-source path

Trace and record the exact path:

```text
ShortBuildPath
  -> ShortBuildStateMachine::prepare
  -> ShortBuildStateMachine::deliver_action
  -> ShortBuildAction::Deliver
  -> I2NP type-25 wrapper
  -> selected independent-consumer/delivery seam
```

The STBM bytes supplied to the reference must originate from `ShortBuildStateMachine`. A manually assembled independent test vector does not satisfy Plan 115.

### A2. Inventory the three candidate lanes

Inspect, in this order:

1. **Direct native independent consumer**
   - Can the pinned i2pd or Emissary source expose a test/program entry that accepts a complete type-25 I2NP message or the type-25 body and runs its native tunnel-build processor?
   - Can that entry run in-process or as a tiny one-shot helper without starting a public router?
   - Can the helper report only typed stage/result metadata and optionally reply bytes?

2. **Existing `tools/i2pr-interop` + runtime NTCP2 lane**
   - Can the existing Rust launcher already authenticate and queue one arbitrary `EncodedI2npMessage` after the known Plan 099 stage?
   - What exact condition causes the current forward dialer to stop before observing the peer's authenticated state?
   - Is there one unambiguous i2pr-owned correction that allows a single type-25 message to be written without reworking the harness?

3. **Reference process without i2pr transport**
   - If the reference exposes a local test/control injection boundary that runs its actual I2NP/tunnel-build processor, this may satisfy Q0 even though it cannot satisfy Q1.

### A3. Selection rule

Choose the lane that obtains Q0 with the fewest new LOC and dependencies. Prefer reference-native code over new orchestration.

Do not choose a lane because it produces the most elaborate evidence. Choose the lane that answers the protocol question with the smallest trustworthy boundary.

If i2pd and Emissary are both readily callable, prefer i2pd first. If using i2pd requires substantial C++ build-system surgery while Emissary exposes a small native short-build consumer, use Emissary and record the rationale.

### A4. Stop condition for lane inventory

If no independent implementation can be invoked without creating more than a small one-shot adapter/helper, close Plan 115 as:

```text
qualified_external_delivery = blocked-no-bounded-independent-consumer-seam
```

and document the exact missing API/process boundary. Do not solve that by rebuilding the old generic harness.

## 6. Work package B — canonical production I2NP bridge

This is the one product-side composition seam Plan 115 is expected to need even for Q0.

### B1. Convert `ShortBuildAction::Deliver` into one canonical I2NP message

Implement the smallest reusable function or adapter that:

1. validates `message.len() == 1 + record_count * 218`;
2. validates `message[0] == record_count`;
3. removes the one-byte count **only for construction of `DeferredBuildRecords`**;
4. constructs:

   ```text
   DeferredBuildRecords::new(record_count, 218, raw_records)
   I2npBody::ShortTunnelBuild(...)
   I2npMessage::<selected header form>
   EncodedI2npMessage
   ```

5. ensures the I2NP body's encoded bytes are byte-for-byte equal to the original `ShortBuildAction::Deliver.message`;
6. never decrypts, mutates, reorders, or regenerates the records.

### B2. Header choice must match the delivery boundary

- For a direct/reference consumer that accepts a standard I2NP message, use the standard I2NP header.
- For an authenticated NTCP2/SSU2 transport message block, use the protocol-correct short-transport I2NP header if that is what the existing runtime encoder expects.
- Do not invent a third framing form.

Tests must round-trip the selected encoded form through `i2pr-proto` and recover exactly the original count-prefixed STBM body.

### B3. Ownership location

Preferred ownership:

- codec/body transformation primitives remain in `i2pr-proto`;
- tunnel-specific conversion from `ShortBuildAction` should live in `i2pr-tunnel` only if it can do so without depending on `i2pr-transport`;
- transport/runtime composition belongs in `tools/i2pr-interop` or `i2pr-runtime`, consistent with existing dependency direction.

Do not introduce a dependency from `i2pr-tunnel` to `i2pr-runtime` or Tokio.

### B4. Required bridge tests

At minimum:

1. 1-record STBM wraps once, no duplicate count byte;
2. 4-record STBM wraps once, canonical body length `1 + 4 * 218`;
3. body recovered by `I2npMessage` decode equals the source delivery body exactly;
4. count mismatch fails before creating `EncodedI2npMessage`;
5. truncated record body fails;
6. oversized/invalid count fails;
7. source buffer is not logged through `Debug`.

## 7. Work package C — Q0 independent native consumer

### C1. Reference must remain independent

The selected reference consumer must execute code from the pinned i2pd or Emissary checkout. Do not copy its parser or cryptography into i2pr tests.

A small adapter around the reference is acceptable when all of the following hold:

- it calls the reference implementation's existing I2NP/tunnel-build parsing/processing code;
- it does not reimplement the build algorithm;
- it receives the exact bytes emitted by the production i2pr bridge;
- any reference-source patch is instrumentation-only, minimal, and separately hashed;
- the unmodified reference revision is recorded.

Prefer zero reference patching.

### C2. Minimum Q0 observation stages

The adapter must distinguish as many of these as the reference exposes:

```text
reference_input_received
reference_i2np_header_accepted
reference_stbm_count_accepted
reference_target_record_identified
reference_request_decrypted
reference_request_fields_accepted
reference_build_decision_accepted | reference_build_decision_rejected
reference_otbrm_produced
```

If the reference API starts below the I2NP header, mark the header stage `not-exercised` rather than pretending it passed.

### C3. Topology for the first Q0 case

Use the smallest topology that exercises the real record role semantics:

- start with a **one-real-hop outbound path** whose sole remote hop is OBEP if the reference consumer can supply the required reply-router metadata;
- if the reference's native processor requires a realistic two-hop chain to produce a useful reply, use a two-real-hop outbound path;
- do not add additional hops for realism alone.

Inbound Q0 is optional for initial Plan 115 closure if outbound Q0 proves independent record compatibility. If outbound Q0 passes cheaply, run the equivalent smallest inbound case as a secondary result without making it a prerequisite for the first protocol result.

### C4. Native acceptance is the gate

A Q0 pass requires the independent implementation to accept the record according to its native processing rules. Merely decoding the count field or locating the target hash prefix is insufficient.

If the reference emits a reply record/body, preserve it only long enough to hash it and feed it into the i2pr reply path. Do not commit raw secret-bearing messages.

## 8. Work package D — optional/preferred Q1 authenticated delivery

Attempt Q1 after Q0 succeeds, or earlier only if the existing runtime lane is plainly simpler than a direct consumer.

### D1. Reuse, do not rebuild, the existing Rust NTCP2 runtime seam

The repository already contains:

```text
i2pr-transport-ntcp2
i2pr-runtime::ntcp2_driver / ntcp2_link / ntcp2_runtime
tools/i2pr-interop
```

Use those. Do not add a second socket/handshake implementation.

### D2. Plan 099 blocker treatment

Plan 099 remains authoritative:

```text
development_interop = protocol-defect-localized
exact_wire_stage     = noise_authenticated
```

Plan 115 may make **at most one narrow transport correction** if the Q1 attempt produces an unambiguous i2pr-owned defect and all of these are true:

- the defect is in existing Rust runtime/adapter/data-phase composition;
- the correction does not alter NTCP2 cryptography or handshake protocol semantics;
- the correction can be tested locally and described in one paragraph;
- no new harness architecture is required.

Examples of permitted correction classes:

- state observation ordering in the one-shot launcher;
- handing an existing authenticated link an `EncodedI2npMessage`;
- typed result propagation after an authenticated write;
- an obvious short-transport I2NP wrapper mismatch.

Not permitted inside Plan 115:

- redesigning the NTCP2 state machine;
- changing Noise/KDF/transcript logic;
- new cross-process control protocols;
- new namespace/container topology;
- multiple successive transport fixes.

If the first transport attempt does not clearly localize one such defect, stop Q1 and retain Q0 as the protocol result.

### D3. Q1 observations

Record:

```text
i2pr_transport_tcp_connected
transport_authenticated_local
transport_authenticated_reference
encoded_i2np_queued
encoded_i2np_write_completed
reference_i2np_received
reference_stbm_dispatched
```

`DeliveryOutcome::Accepted` alone means queue admission, not reference receipt. Do not count it as Q1.

## 9. Work package E — Q2 reply round-trip when available

If the selected reference produces an OTBRM and the chosen lane can return it without new architecture:

1. decode it as I2NP type 26 with the existing `i2pr-proto` body registry;
2. recover the exact count-prefixed 218-byte-record body;
3. feed only that body into `BuildEvent::BuildReply`;
4. require the state machine terminal outcome to be exactly `ShortBuildOutcome::Established`;
5. register the resulting tunnel only through the existing success-only registrar path.

If the reference accepts the request but its selected test API cannot route the reply back, classify:

```text
independent_short_build = passed-native-consumer
reply_roundtrip         = blocked-reference-api-no-return-path
```

Do not downgrade a valid Q0 result to generic failure.

## 10. Work package F — evidence schema and failure localization

`plans/115-status.md` must be created by the executor. It is the durable sanitized evidence summary.

Required identity fields:

```text
i2pr_source_commit
plan_115_execution_commit
reference_kind                = i2pd | emissary | java-i2p
reference_repository
reference_revision
reference_version_if_available
reference_patch_sha256        = none | <sha256>
lane_kind                     = native-consumer | authenticated-ntcp2 | other-bounded-native
network_id                    = 99 | not-applicable
bind_scope                    = loopback | not-applicable
```

Required message evidence:

```text
direction
real_hop_count
record_count
stbm_body_length
stbm_body_sha256
i2np_header_kind
i2np_encoded_length
i2np_encoded_sha256
reference_highest_stage
reference_decision
otbrm_body_length             = <n> | not-produced
otbrm_body_sha256             = <sha256> | not-produced
i2pr_terminal_outcome         = Established | <typed state> | not-returned
```

Do **not** record private/static/ephemeral keys, raw RouterInfo bytes, raw STBM/OTBRM payloads, raw authenticated transport transcripts, arbitrary reference logs, or public addresses not needed for a loopback development record.

Allowed terminal classifications:

```text
passed-external-established
passed-independent-native-consumer
blocked-no-bounded-independent-consumer-seam
blocked-transport-before-authentication
blocked-transport-after-authentication-before-i2np
blocked-reference-api-no-return-path
failed-i2np-envelope-rejected
failed-stbm-framing-rejected
failed-target-record-not-found
failed-build-record-decrypt
failed-build-request-fields
rejected-by-peer-policy
failed-i2pr-reply-postprocess
environment-or-build-blocked
```

Do not invent a generic `failed` token when one of these can classify the observation.

## 11. Work package G — strict correction and attempt budget

The purpose of this section is to prevent another validation spiral.

### G1. Reference-consumer lane budget

- One baseline attempt after local tests pass.
- If blocked by a trivial adapter/build issue, one direct correction is allowed.
- One confirmation attempt after that correction.
- If the selected reference cannot expose the native consumer at all, switching once to the other preferred reference (i2pd <-> Emissary) is allowed.
- Do not alternate repeatedly between reference implementations.

### G2. Authenticated transport lane budget

- One baseline attempt.
- At most one unambiguously i2pr-owned narrow Rust correction under §8.2.
- One confirmation attempt.
- Then stop and record the exact stage.

### G3. Cryptographic boundary

Any evidence suggesting a Plan 109-114 cryptographic/layout defect is **not** authorization to patch it inside Plan 115.

Instead:

1. preserve the failing body hash and reference stage;
2. reduce the disagreement to the smallest field/vector possible;
3. compare against the pinned official specification and reference behavior;
4. close Plan 115 with the exact defect classification;
5. author one narrow corrective successor plan.

No speculative crypto edits.

## 12. Work package H — local validation

Before any independent reference run:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Also run the existing `tools/i2pr-interop` cargo test/build command if Plan 115 touches that tool. Do not change workspace membership solely to make a preferred `cargo -p` spelling work.

Reference-specific build/tests should be the minimum native commands needed for the selected pinned revision. Record those commands in `plans/115-status.md`.

No Plan 099 GitHub Actions workflow dispatch is required by Plan 115.

## 13. Acceptance criteria

Plan 115 is complete when **all local criteria** and at least one **external evidence branch** below are satisfied.

### 13.1 Mandatory local criteria

1. A production `ShortBuildAction::Deliver` can be converted to one canonical complete I2NP type-25 message without double-prefixing the STBM record count.
2. Round-trip decoding recovers the exact original count-prefixed STBM body.
3. The adapter preserves `first_hop`, deadline, and record count without deriving routing metadata from hashes or tunnel IDs.
4. Existing Plans 111-114 tests remain green and unchanged in semantic strength.
5. Normal-daemon NTCP2 remains disabled and unenableable.
6. NTCP2 remains non-advertised.
7. No Python harness, namespace, container, VM, public-network, or generic I2NP-dispatch architecture is added.
8. Full workspace validation passes.

### 13.2 Branch A — strongest success

```text
qualified_external_delivery = passed-external-established
Q0                           = passed
Q1                           = passed
Q2                           = passed
reference                    = one pinned independent implementation
i2pr_terminal                = Established
```

This is a Plan 115 full pass. It still does not close the Milestone 3 two-reference transport criterion or the complete Milestone 5 data-plane criterion.

### 13.3 Branch B — minimum useful protocol success

```text
qualified_external_delivery = passed-independent-native-consumer
Q0                           = passed
Q1                           = blocked-or-not-exercised
Q2                           = blocked-or-not-exercised
reference                    = one pinned independent implementation
```

The reference must have executed native short-build record processing, not just generic I2NP decoding.

Branch B is sufficient to proceed to local Milestone 5 data-plane Plan 116 while preserving a separate transport blocker for later mixed-router exit.

### 13.4 Branch C — localized protocol defect

The independent implementation reaches the short-build processor but rejects the i2pr record/message at a reproducible protocol stage.

Required closure:

```text
qualified_external_delivery = protocol-defect-localized
reference_highest_stage      = <exact stage>
transport_dependency         = not-the-owning-blocker
next_plan                    = one narrow corrective protocol plan
```

Do not begin Plan 116 until the localized short-build defect is corrected or explicitly proven to be a reference-specific incompatibility.

### 13.5 Branch D — transport blocker after Q0 success

```text
independent_short_build      = passed-independent-native-consumer
qualified_live_delivery      = blocked-<exact transport stage>
short_build_protocol         = independently-consumed
plan_116_local_data_plane    = unblocked
mixed_router_milestone5_exit = still-blocked
```

This branch is intentionally allowed. It is the escape hatch from the previous "transport harness blocks all router construction" failure mode.

### 13.6 Branch E — environment/reference consumer blocked

If neither selected reference can be invoked at a native tunnel-build boundary under the environment constraints, close with an exact blocker and stop. Do not generate additional harness plans automatically.

## 14. Closure/status updates required from the executor

At Plan 115 closure:

1. create `plans/115-status.md` with the sanitized evidence schema above;
2. update `plans/115-handoff.md` from `ready-for-execution` to a closure pointer and name the actual next plan;
3. update the roadmap note with the achieved Q-level and the resulting Plan 116 gate;
4. update `README.md` only if the user-facing implemented/not-implemented status materially changed;
5. update `specs/support.toml` only if a support claim genuinely changed; Q0 alone must not advertise a transport or claim live network support;
6. do not rewrite historical Plan 099 evidence.

## 15. Handoff state after a successful minimum result

Expected minimum useful state:

```text
plan_115                       = passed-independent-native-consumer
short_build_outbound           = independently-consumed-by-one-reference
short_build_local_inbound      = strict-established
short_build_local_outbound     = strict-established
live_transport                 = unchanged-or-exactly-localized
normal_daemon_ntcp2            = disabled-and-unenableable
ntcp2                          = experimental-non-advertised
plan_116                       = eligible-local-tunnel-data-plane
milestone5_mixed_router_exit   = still-requires-qualified-live-lane
milestone4b                    = still-requires-live-exploratory-path
```

Expected strongest state:

```text
plan_115                       = passed-external-established
short_build_external_roundtrip = established-with-one-reference
plan_116                       = eligible-tunnel-data-plane
milestone5_mixed_router_exit   = still-requires-data-plane/exploratory-proof
milestone4b                    = still-requires-live-netdb-over-exploratory
normal_daemon_ntcp2            = disabled-and-unenableable
```

The project should then move into actual tunnel message forwarding rather than another short-build conformance audit.
