# Plan 252 — M11 daemon transit composition

Status: **registered-m11-daemon-transit-composition-ready**

Baseline: Plan 250 implementation `44187ce` (ordinary CI green; see Plan 250 status).

Planning correction: this plan was amended before execution to make the full-message
ShortTunnelBuild transformation/routing seam explicit. The original registered wording
treated the Plan 250 per-record transaction as if its sealed reply were sufficient for
daemon forwarding. It is not: the short-build protocol uses the same derived `replyKey`
for the hop's own AEAD reply and for ChaCha20 transformation of every other record in the
message. Plan 252 owns that composition gap before daemon wiring. Plan 250 remains closed
for its defined per-record admission/ownership scope.

## Objective

Compose the corrected runtime-neutral M11 transit participant into the authenticated
daemon router-I2NP path, beginning with one canonical runtime-neutral **full-message transit
processor**.

For each authenticated ShortTunnelBuild message the tunnel layer must:

1. validate the complete count-prefixed record set;
2. locate and open the local hop's record exactly once;
3. make the Plan 250 admission decision;
4. derive the hop reply/layer keys exactly once;
5. seal the local 0/30 reply record with the Plan 250 bandwidth semantics;
6. replace the local slot and ChaCha20-transform every other record exactly once with the
   same derived `replyKey` and record-number nonce;
7. return the complete transformed count-prefixed payload plus non-secret ephemeral routing
   metadata;
8. commit accepted participant state only after the full transformed message is ready;
9. return a transformed code-30 message with zero participant state for a valid local
   policy rejection.

Then compose that operation into the authenticated daemon router-I2NP path, forward the
result through the existing router-delivery seams, route established TunnelData by receive
id, and reclaim all participant state on expiry, terminal delivery failure, cancellation,
and shutdown.

This remains infrastructure. It does not qualify M11 capability or interoperability.

## Why this plan is ready

- Plan 250 closes the per-record admission/ownership contract: explicit authenticated
  `previous_peer` input, real pending reservations, sealed code-30 policy replies, correct
  reply-side `b` semantics, correct request lifetime handling, move-only secret owners, and
  panic-free registry operations. Implementation `44187ce` has green Actions run
  `36060781701`.
- Plan 251 closes ordinary-CI Java source-lock gating on `6cf441d`; Actions run
  `36050553519` is green.
- The repository already has the required multi-record primitives in
  `i2pr-tunnel::multirecord`:
  `decode_short_tunnel_build_payload` /
  `validate_count_prefixed_short_payload`,
  `encode_count_prefixed_short_payload`,
  canonical per-slot ChaCha20 transformation, and
  `MessageHopProcessor`.
- `MessageHopProcessor` proves the current local algorithm shape but cannot be invoked
  independently from `process_short_build_request` in production: doing so would
  decrypt/KDF the same record twice, independently construct a second own reply, and make
  it easy to transform with key material disconnected from the actual Plan 250 outcome.
  Plan 252 must factor/reuse the canonical primitive instead of stacking the two APIs.
- The daemon already receives `Ssu2InboundI2np` with authenticated peer/link metadata,
  classifies ShortTunnelBuild and TunnelData in `router_i2np`, and has bounded outbound
  `RouterDeliveryService::deliver` backed by `Ssu2RuntimeService::send_i2np`.
- `ExploratoryBuildCoordinator` remains the serialized creator-build owner. Transit
  composition can be added alongside it without changing creator state semantics.
- No new protocol decision is required. The current I2P ECIES short-build specification
  defines 218-byte records, record-number nonce semantics, the common reply key, and the
  STBM/OTBRM message flow.

## Protocol seam that this plan must preserve

For short records, the hop's own reply record is encrypted with
ChaCha20/Poly1305. Every other record in the same build message is iteratively transformed
with plain ChaCha20 using the **same derived reply key**, with the record number encoded in
the nonce.

Therefore a production transit hop cannot treat `TransitBuildOutcome::sealed_reply` as the
whole build result.

The canonical per-hop state transition is:

```text
count-prefixed STBM
  -> validate 1 + n*218 shape
  -> locate unique local hash-prefix slot
  -> open/decode local record once
  -> admission decision
  -> derive replyKey/layer keys once
  -> construct + seal local reply (0 or 30)
  -> replace local slot
  -> ChaCha20-transform every other slot once using replyKey + target slot nonce
  -> encode complete count-prefixed result
  -> commit accepted registration only now
  -> return transformed message + non-secret routing metadata
```

For a valid/authenticated local policy rejection the reply byte is 30 and no transit
registration is installed, but the processable build message still requires the normal
record-set reply transformation before it is forwarded/terminated according to the
decoded role. Malformed, unauthenticated, ambiguous-slot, or otherwise fatal input fails
closed without inventing a reply or forwarding partially transformed state.

## Invariants

### Authentication and identity

- Only authenticated `Ssu2InboundI2np::peer` establishes the previous peer.
- Never derive previous peer from local identity, decoded next-router fields, or other
  unauthenticated caller data.
- Local hop identity remains the ECIES record-selection/authentication identity.

### Full-message cryptographic processing

- Open the local request record once per message.
- Derive `replyKey` / layer material once per message.
- The same derived `replyKey` used to seal the local record is used to transform every
  non-local record.
- Preserve record count, slot ordering, and fake-record positions. Only the local slot
  replacement and required symmetric transforms may change record bytes.
- Transform each non-local slot exactly once for this hop, using that target slot's
  record-number nonce.
- Do not invoke `MessageHopProcessor::process_hop` and
  `process_short_build_request` independently on the same production message.
- Do not introduce a second ChaCha20 transform implementation. Factor or expose the
  existing canonical helper narrowly so both existing conformance code and transit message
  processing share it.
- Do not export `replyKey`, `LayerKeys`, Noise state, decrypted request bytes, or other
  hop secrets to the daemon simply to perform the transform. Full-message cryptography
  stays inside `i2pr-tunnel`.

### Transaction and registration

- Admission remains globally/per-peer bounded.
- The accepted path must not install a live registration until own-reply sealing,
  non-local record transforms, and final count-prefixed payload encoding all succeed.
- Any fatal failure before commit releases the pending token and leaves zero new active
  state.
- Policy rejection returns a transformed full-message outcome with no active registration.
- Duplicate receive ids fail closed; never evict an unexpired participant.

### Routing metadata

The full-message outcome may expose only the non-secret authenticated/decrypted routing
facts needed by the runtime, preferably as a typed move-only value:

```text
TransitBuildRoute {
    role,
    receive_tunnel,
    next_router,
    next_tunnel,
    next_message_id,
}
```

Exact names may differ.

- Participant and IBGW continuation keeps the message as STBM and routes toward the decoded
  next router with the decoded next message id.
- OBEP terminates STBM propagation and constructs the corresponding OTBRM from the already
  transformed record set, using the decoded next-router / next-tunnel / next-message-id
  reply-routing metadata.
- The daemon must not reopen the encrypted request merely to recover these fields.
- Long-lived registrations retain only fields needed for established TunnelData behavior;
  build-only routing metadata must not be persisted without a runtime need.

### Runtime ownership

- `i2pr-tunnel` remains runtime-neutral: no Tokio, sockets, sleeps, filesystem, or shared
  synchronization.
- The daemon owns serialization, queueing, timers, cancellation, sockets, and shutdown.
- No task-per-cell or task-per-build detached lifecycle.
- Transit traffic uses existing bounded router-delivery/resource-governance paths and
  remains lower priority than router-owned/client traffic.
- No RouterInfo capability, router.version, listener exposure, or public-network change.

## Scope

### In scope

- Refactor the Plan 250 transaction internals as needed to support a single-open,
  single-KDF, commit-after-full-message transaction without weakening its tested
  per-record semantics.
- Add a runtime-neutral message-level transit API in `i2pr-tunnel`, e.g.
  `process_short_build_message` / `TransitMessageProcessor`.
- Reuse the canonical count-prefixed codec and canonical ChaCha20 record transform.
- Return complete transformed STBM/OTBRM record payload plus typed non-secret routing and
  admission outcome.
- Compose that API into authenticated daemon ShortTunnelBuild processing.
- Bounded daemon ownership for build delivery, TunnelData forwarding, expiry, cancellation,
  and shutdown.
- Deterministic runtime-neutral full-message tests and daemon composition tests.
- Documentation/evidence updates preserving `advertised=false`.

### Explicitly out of scope

- Exact-pinned i2pd qualification or repeated external-router execution (Plan 253).
- Public-network transit.
- New wire format, response code, KDF, or ChaCha implementation.
- Legacy/ElGamal build compatibility.
- Capability/version/RouterInfo changes.
- Broad bandwidth-governor redesign.
- Floodfill.
- Reopening Plan 250's already-closed bandwidth/time/admission policy unless a direct
  regression is found while extracting the message-level transaction.

## Required production changes

### 1. Factor one canonical full-message transit operation

Add a tunnel-layer function/type accepting:

- the full count-prefixed STBM payload;
- local static private key + local identity;
- authenticated previous peer;
- caller time;
- admission policy/state + transit registry;
- production RNG.

It must validate count `1..=8` and exact `1 + n*218` length, find exactly one local
hash-prefix slot, and run the Plan 250 logic without a second decrypt or KDF.

If the current `process_short_build_request` cannot safely defer registration commit until
after the multi-record transform, split its internals into private staging/commit helpers.
Do not expose secret staging state as a public daemon contract.

### 2. Reuse the existing record-set transform

Extract/reuse the narrow operation currently embedded in `MessageHopProcessor`:

```text
replace own slot with sealed reply
for every other slot:
    chacha20_transform(reply_key, target_slot, slot_bytes)
encode_count_prefixed_short_payload(...)
```

Keep `MessageHopProcessor` behavior green. Prefer one shared helper over duplicated loops.

### 3. Return routing facts with the transformed message

The message-level result must carry enough non-secret information for the daemon to route
without reopening the request:

- role;
- receive tunnel id;
- next router hash;
- next tunnel id;
- next message id;
- response/admission disposition;
- complete transformed count-prefixed payload.

The result's Debug implementation must summarize/redact payload bytes.

### 4. Define role-specific build dispatch

For Participant / IBGW:

- construct the next ShortTunnelBuild from the already transformed full payload;
- preserve record count and transformed records;
- use the decoded next router and next message id;
- do not regenerate any reply record.

For OBEP:

- stop STBM hop-to-hop propagation;
- construct the OutboundTunnelBuildReply from the already transformed record set;
- use decoded reply-routing metadata, including next tunnel id where tunnel delivery
  requires it;
- preserve the transformed bytes exactly.

Both accepted and valid code-30 outcomes follow the appropriate build-message routing
shape. Only accepted outcomes own an installed transit registration.

### 5. Add daemon composition ownership

Add daemon state for the Plan 250 registry/admission owner and route authenticated
ShortTunnelBuild input into the message-level operation only when the controlled transit
path is enabled.

Participation remains disabled in ordinary product profiles. Do not introduce public
configuration merely for this plan unless an existing controlled opt-in cannot represent
the test lane.

### 6. Compose established TunnelData forwarding

On TunnelData:

- look up receive id;
- compare authenticated sender with registered previous peer;
- apply the existing role's bounded transform/replay protection;
- forward exactly one correctly addressed cell according to registered role state;
- fail closed on unknown id, wrong peer, expiry, malformed data, duplicate, or replay.

### 7. Lifecycle and delivery failures

Use one bounded daemon owner for expiry/shutdown. No per-cell task spawning.

If transformed build delivery fails after an accepted registration was committed, remove
that registration synchronously when the participant can no longer be represented
correctly. QueueFull, ResourceDenied, NoActiveSession, deadline, cancellation, and oversize
must be terminal typed outcomes rather than retry loops or hidden buffering.

### 8. Preserve creator-build correlation

Before classifying an incoming ShortTunnelBuild as new transit work, preserve the existing
`ExploratoryBuildCoordinator` correlation rules. A creator-side reply/forwarding path must
not be consumed as a fresh transit request.

## Ordered work packages

1. **Write failing full-message tests first.** Exercise a four-record message with a local
   real slot plus other real/fake slots; prove the current per-record API is insufficient
   without adding a daemon workaround.
2. **Factor the shared transform primitive.** Reuse `chacha20_transform` and existing
   count-prefixed codec; keep `MessageHopProcessor` tests green.
3. **Implement the runtime-neutral message transaction.** Open once, decide once, derive
   once, seal own record, transform others, encode, then commit accepted state.
4. **Add typed route/output surface.** Keep keys/Noise/plaintext private to
   `i2pr-tunnel`.
5. **Integrate daemon ShortTunnelBuild dispatch.** Use authenticated ingress peer only;
   preserve creator correlation and route Participant/IBGW vs OBEP correctly.
6. **Integrate TunnelData + expiry/shutdown ownership.**
7. **Run local composition/failure/queue tests and full verification.**
8. **Update closure/docs and perform the Plan 253 unblock audit.**

Do not begin with daemon socket/task edits before work packages 1-4 are green.

## Required tests

### Runtime-neutral full-message tests

1. four-record accepted message finds exactly one local slot;
2. local accepted slot decrypts to code 0;
3. accepted m/r request's local slot contains correct reply `b`;
4. each non-local record equals exactly one application of the canonical ChaCha transform
   using this hop's reply key and that target slot number;
5. count byte, slot count, and slot order are unchanged;
6. fake records are transformed exactly like other non-local records;
7. the full-message accepted path commits exactly one registration only after successful
   final payload encoding;
8. deterministic transform failure leaves zero new active state and returns pending count
   to baseline;
9. valid policy rejection produces local code 30, transforms every non-local record, and
   installs no registration;
10. rejection with bandwidth options does not leak local reject taxonomy into the wire
    Mapping;
11. no matching local hash-prefix -> fatal/no transformed output;
12. duplicate matching hash-prefix -> fatal/no transformed output;
13. malformed count/trailing/truncated payload -> fatal/no state;
14. instrumented cryptography proves one local request open per processed message;
15. no daemon-visible output contains reply key, layer keys, Noise state, or request
    plaintext;
16. Participant route metadata exactly matches decoded receive/next router/next tunnel/next
    message id;
17. real IBGW route metadata is preserved exactly;
18. real OBEP route metadata is preserved exactly;
19. no-bandwidth accepted message is byte-compatible with existing
    `MessageHopProcessor` accepted processing when both are driven with equivalent crypto,
    slot, and RNG inputs;
20. existing Plan 250 per-record tests remain green.

### Daemon composition tests

21. authenticated previous peer is passed unchanged from `Ssu2InboundI2np`;
22. spoofed/different peer cannot install or use a registration;
23. Participant accepted message sends transformed STBM to exact next router/message id;
24. IBGW accepted message sends transformed STBM to exact next router/message id;
25. OBEP accepted message emits OTBRM from the exact transformed records and routes with
    the decoded reply-routing fields;
26. Participant/IBGW code-30 rejection still forwards the correctly transformed STBM with
    no registration;
27. OBEP code-30 rejection emits the correctly transformed OTBRM with no registration;
28. daemon never reconstructs/reseals the local reply;
29. global/per-peer pending/active limits remain enforced under composition;
30. duplicate receive id, queue pressure, resource denial, and fatal delivery paths restore
    counters/registry to expected baselines;
31. TunnelData success enforces receive-id + previous-peer and exact next-hop transform;
32. unknown id, wrong peer, expired, malformed, duplicate, and replay TunnelData fail
    closed;
33. expiry removes each entry once;
34. cancellation and normal shutdown drain all registrations/secrets;
35. restart begins with empty transit state;
36. matching and nonmatching creator-build traffic preserves existing coordinator behavior;
37. queue/lifecycle tests prove no task-per-cell or unbounded channel growth.

## Failure, cancellation, restart, and contention semantics

- Authentication/codec/ambiguous-slot failures occur before any forwarding outcome and
  leave no active registration.
- Policy rejection is not a fatal crypto failure: it returns a complete transformed
  message with code 30 and zero registration.
- Any accepted-path error before final transformed payload construction releases pending
  state and commits nothing.
- Registration commit is the final tunnel-layer success transition after full-message
  construction.
- A daemon delivery failure after commit synchronously removes the registration when that
  failed delivery makes the accepted hop unusable.
- Cancellation before queue admission prevents delivery. Cancellation/shutdown after
  registration drains state under the single daemon owner.
- Queue/resource contention is explicit; no hidden retries or unbounded buffering.
- Expiry uses caller-supplied time and removes once.
- Process restart starts with empty in-memory active/pending transit state.

## Compatibility and migration

No persisted-format or dependency change is expected.

The new message-level API is an internal experimental Rust surface. Preserve
`process_short_build_request` where useful for focused tests/consumers, but production
daemon transit must use the full-message API rather than combining the per-record API with
a second independent processor.

Existing creator build construction and reply postprocessing must remain byte-compatible.

## Exact verification commands

Run from repository root, serializing loopback tests:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
git diff --check
```

Extend `check-m11-transit-boundaries.sh` narrowly to prove production daemon code uses the
message-level transit API rather than calling `process_short_build_request` directly and
then independently invoking/reimplementing multi-record transformation.

Also run every new focused full-message and daemon integration target exactly. Ordinary
GitHub Actions on the exact implementation SHA are required closure evidence.

Do not use Plan 253 external i2pd execution as a substitute for Plan 252 local composition
closure.

## Documentation updates

- `plans/subsystems/transit-tunnels-roadmap.md`: record the full-message adapter as the
  first Plan 252 layer and preserve the Plan 253 gate.
- `docs/architecture/i2pr-daemon.md`: document authenticated ingress -> message processor
  -> role-specific build delivery -> established TunnelData path.
- `specs/protocols/05-tunnels.md`: document the short-build message-level transform
  invariant and current non-advertised status.
- `specs/support.toml`: infrastructure status only; `advertised=false`.
- `plans/registry.md` and `plans/closure/transit-tunnels/252-status.md`: record actual
  execution evidence and successor audit.

## Acceptance criteria

Plan 252 closes only when all of the following are directly demonstrated:

1. the production transit build path consumes a complete validated count-prefixed STBM;
2. the local request is opened once and the reply/layer keys are derived once;
3. the local slot contains the exact Plan 250 0/30 reply semantics;
4. every non-local record is transformed exactly once using the same derived reply key and
   the correct target-slot nonce;
5. accepted registration commits only after complete transformed payload construction;
6. valid policy rejection returns a transformed message and zero registration;
7. Participant/IBGW/OBEP routing metadata is returned without reopening the request;
8. Participant/IBGW STBM continuation and OBEP OTBRM termination are proven locally;
9. no secret crypto state crosses into daemon routing;
10. daemon ingress uses authenticated previous-peer provenance;
11. established TunnelData previous-peer/replay/next-hop behavior is proven;
12. queue/resource/delivery failures, expiry, cancellation, shutdown, and restart have
    bounded deterministic cleanup;
13. creator-build routing remains green;
14. no detached task, unbounded queue, dependency, RouterInfo/capability/version, listener,
    or public-network change lands;
15. focused and full verification plus ordinary CI are green on the exact implementation
    SHA.

This remains infrastructure-only. M11 capability and interoperability remain unclaimed.

## Stop conditions

Stop and record a typed blocker rather than papering over it if:

- full-message processing would require opening the local request twice;
- the only proposed implementation exports `replyKey` / `LayerKeys` / Noise state to the
  daemon;
- the existing canonical ChaCha record transform cannot be reused without changing its
  wire semantics;
- exact role-specific STBM/OTBRM routing cannot be represented with the authenticated
  decoded request fields already on the wire;
- the Plan 250 admission transaction cannot be staged so full-message transform failure
  occurs before accepted commit;
- correct delivery requires unauthenticated peer provenance, a new wire format, legacy
  build support, capability advertisement, or public-network behavior;
- a new dependency is proposed for functionality already present in the workspace.

If a direct Plan 250 semantic regression is discovered, stop and register a new corrective
rather than silently broadening Plan 252.

## Closure evidence required

The closure record must include:

- implementation SHA and exact CI run;
- named full-message tests proving own-slot reply + every-other-slot transform;
- proof of one-open/one-KDF behavior;
- accepted and code-30 full-message evidence;
- Participant/IBGW/OBEP routing evidence;
- daemon queue/lifecycle/TunnelData evidence;
- requirement-to-test matrix;
- dependency/secret/runtime-boundary review;
- exact verification command results;
- Plan 253 unblock audit.

Plan 253 may be registered only when this complete message-level + daemon composition
boundary is stable. Otherwise preserve it as unregistered with the concrete blocker.

## Handoff notes

Start in `i2pr-tunnel`, not the daemon.

The first deliverable is the full-message transaction with direct deterministic tests.
Reuse `MessageHopProcessor`'s canonical transform primitive, but do not invoke
`MessageHopProcessor::process_hop` as a second production processor beside the Plan 250
transaction.

Only after the full-message outcome is stable should the implementation agent wire it into
`router_i2np` / daemon ownership.

Treat transformed build payloads as opaque at the daemon boundary. The daemon routes them;
it does not inspect, reseal, or retransforms individual build records.

Do not claim router interoperability from these local tests. Plan 253 owns exact-pinned
i2pd qualification.
