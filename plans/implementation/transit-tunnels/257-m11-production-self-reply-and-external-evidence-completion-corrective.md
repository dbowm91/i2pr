# Plan 257 — M11 production self-reply qualification and external evidence completion corrective

Status at registration:
**registered-m11-production-self-reply-and-external-evidence-completion-corrective-ready**

Baseline: bc9b5c6172d0560d3eb8d2ef1ae0d447478ba480

Corrects:
- plans/implementation/transit-tunnels/256-m11-i2pd-qualification-evidence-topology-corrective.md
- plans/closure/transit-tunnels/256-status.md

Retains:
- Plan 250 admission/reply/registry semantics.
- Plan 252 runtime-neutral full-message ShortTunnelBuild transaction.
- Plan 253 runtime-neutral data plane, rollback/drain, and bounded peer-state work.
- Plan 254 live ingress/body-threading owner boundary.
- The valid Plan 256 identity/key-coherence, exact NetDB bootstrap, real i2pd-B topology,
  typed role evidence, OBEP/IBGW data-plane work, and anti-fan-out infrastructure.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0 at
635b013a612ff47278ef02acf8580a28e10e26c5.

## 1. Objective

Finish M11 controlled i2pd qualification without weakening the corrected Plan 256 lane.

Plan 257 has one bounded outcome: convert the useful Plan 256 implementation into evidence that
is strong enough to close the one-family ADR 0026 experimental M11 gate.

It must do three things in order:

1. qualify or narrowly correct the production local-IBGW / OBEP short-build-reply behavior that
   Plan 256 introduced after external investigation crossed Plan 256's production-defect stop
   condition;
2. close the remaining evidence gaps in registration cardinality, Participant far-side receipt,
   bandwidth-option observation, cancellation/session-close state, and actual runtime restart;
3. execute the complete corrected exact-pinned i2pd matrix twice on one i2pr SHA with fresh
   datadirs, with ordinary exact-head CI green on Ubuntu, macOS, MSRV, and dependency policy.

This is a **corrective capability-qualification plan**. It is not permission to add public
transit, broaden protocol claims, tune away reference failures, or create another harness.

## 2. Why Plan 257 is required

Plan 256 materially repaired the Plan 255 false-positive lane, but its implementation cannot be
closed as written.

### 2.1 Plan 256 crossed its production-defect stop condition

The Plan 256 implementation added production behavior in:
- crates/i2pr-daemon/src/transit_compose.rs
- crates/i2pr-daemon/src/transit_owner.rs
- crates/i2pr-daemon/src/router_i2np.rs
- crates/i2pr-tunnel/src/garlic_reply.rs
- crates/i2pr-tunnel/src/short_record.rs
- crates/i2pr-tunnel/src/transit.rs

The new behavior includes local-IBGW self-reply routing, OBEP OTBRM relay construction, router
garlic handling, and associated reply-envelope helpers. Plan 256 explicitly required a narrow
successor if corrected external evidence localized a production defect. Plan 257 is that
successor and must qualify this behavior before treating it as retained production authority.

### 2.2 Participant forwarding lacks an independent far-side observation

The current external lane proves a local TransitDataDisposition::Forwarded event, records a
digest, and then records creator-accepted. That does not independently prove i2pd-B received the
transformed cell.

Exact-pinned i2pd 2.61.0 provides a source-lockable far-side observable in
libi2pd/TransitTunnel.cpp: TransitTunnelEndpoint::HandleTunnelDataMsg logs the endpoint tunnel
id before handing decrypted data to the endpoint. Plan 257 must bind a sanitized B-side
observation to the exact next tunnel id from the counted Participant forward, or use an
equally strong source-locked B-side counter. A local i2pr forward or A-side creator reuse is
not a substitute.

### 2.3 Restart is currently a constructor check

The current restart epoch constructs a new TransitLiveOwner and asserts active_count() == 0.
That proves constructor state, not runtime restart semantics. Plan 257 must stop the controlled
i2pr runtime owner/service, create a new owner/service, re-establish authenticated reference
sessions, and prove a fresh post-restart build can be accepted from zero state.

The references do not need to be restarted solely to prove i2pr state unless the implementation
cannot otherwise establish a clean ownership boundary. The two complete external attempts
already use fresh reference datadirs and independently prove reference-process restart.

### 2.4 Cancellation and session-close evidence is too narrow

Cancellation currently observes active registrations only. Closure requires explicit
non-secret state evidence for active registrations, pending admission reservations, peer-index
entries, and any transit-owned queued work.

Session close currently proves that A was removed. It must also prove an unrelated B mapping
survives until its own close/cancel event.

### 2.5 Bandwidth disposition is hard-coded rather than decoded evidence

The current rejection epoch records
reference-options-unobserved-no-fabrication. That is truthful as a statement about the harness,
but it does not prove what the actual decoded request contained.

The tunnel transaction must surface a non-secret typed summary of observed m/r/l request values
and emitted b reply disposition, or an explicit typed absence. The external row must be derived
from that summary.

### 2.6 Exact registration cardinality is not yet proven

The runner describes each accepted role as installing exactly one registration, but the driver
primarily records a receive tunnel id. Each counted role epoch must record before/after bounded
state and prove a delta of exactly +1 for the counted accepted build, with the counted receive
id present. Background builds may be observed, but they may not satisfy or inflate the counted
row.

### 2.7 Plan 256 has no counted external passes

No M11 external workflow run has closed the Plan 256 matrix. Plan 257 therefore starts with
zero counted external passes.

### 2.8 Exact-head ordinary CI is not green

For Plan 256 implementation head bc9b5c6172d0560d3eb8d2ef1ae0d447478ba480, Actions run 36332304955 had:
- MSRV: passed;
- dependency policy: passed;
- macOS Quality: failed in Clippy;
- Ubuntu Quality: still running at the Plan 257 registration audit.

Plan 257 must diagnose and correct the macOS Clippy failure without suppressing the warning or
weakening platform coverage.

## 3. Frozen invariants

The following are non-negotiable.

### 3.1 Reference and network

- i2pd version 2.61.0, commit 635b013a612ff47278ef02acf8580a28e10e26c5.
- Reference source is unmodified and clean.
- IPv4 loopback only.
- Public reseed and public-network fallback disabled.
- Fresh reference datadirs for every counted complete attempt.
- No patching, LD_PRELOAD, in-memory reference mutation, fake peer injection, or public router.

### 3.2 Identity and cryptography

- The tunnel-build responder secret is the RouterIdentity encryption private key matching the
  public encryption key in the signed i2pr RouterInfo.
- The SSU2 transport static key remains distinct.
- Build request open, reply sealing, full-message reply-key transform, and LayerKeys stay in
  i2pr-tunnel.
- No secret/private key, LayerKeys, replyKey, Noise state, or decrypted request is retained in
  evidence.
- Secret-owning wrappers remain move-only, redacted, and zeroizing.

### 3.3 Ownership

- Ssu2DaemonHandle::next_inbound is the authenticated external ingress.
- TransitLiveOwner::handle_inbound remains the single controlled transit owner.
- Creator/service ownership precedes transit ownership.
- One canonical router-I2NP decode; no second production decoder.
- Ordinary product construction remains transit-disabled.
- No second socket/event loop and no per-cell task spawning.
- All maps, queues, reassembly state, replay state, and evidence ledgers remain bounded.

### 3.4 Capability

- No router.version change.
- No RouterInfo transit capability advertisement.
- No public transit config toggle.
- No public-network participation.
- No M12 floodfill implementation or planning promotion until Plan 257 closes.
- Full two-family router conformance remains separate from ADR 0026 one-family experimental
  progression.

## 4. Source-lock the Plan 256 production reply behavior

Before changing production code, source-lock exact i2pd 2.61.0 behavior in
libi2pd/TransitTunnel.cpp.

The counted source-lock must prove the short-build endpoint branch has two distinct paths:

1. remote reply IBGW:
   - derive RGarlicKeyAndTag;
   - wrap the ShortTunnelBuildReply in ECIES garlic;
   - wrap that in TunnelGateway for the requested reply tunnel;
   - send to the requested reply router;

2. local reply IBGW:
   - identify that the reply router hash is local;
   - resolve the requested local tunnel id;
   - inject the ShortTunnelBuildReply into that local tunnel with SendTunnelDataMsg;
   - flush that tunnel;
   - do not perform the remote-router garlic/TunnelGateway path.

Add exact source-lock rows for the branch predicates and operations. Do not source-lock only
comments or symbol names.

Required source-lock evidence:

    m11-i2pd-obep-remote-reply-source-lock
    m11-i2pd-obep-local-ibgw-reply-source-lock

If exact pinned source contradicts the current i2pr production behavior, fix i2pr narrowly and
add a regression before any external rerun.

## 5. Work package A — freeze Plan 256 evidence authority

1. Keep Plan 256's implementation commit and evidence history intact.
2. Treat Plan 256 as retained corrective infrastructure, not closed qualification.
3. Add negative tests for every Plan 257 evidence gap before changing the driver:
   - local-only Participant forward cannot satisfy far-side receipt;
   - constructor-only restart cannot satisfy restart;
   - active_count-only cancellation cannot satisfy full drain;
   - removing A without proving B retained cannot satisfy session-close;
   - a hard-coded bandwidth string cannot satisfy bandwidth disposition;
   - a receive-id-only row cannot satisfy exact registration cardinality;
   - one successful external attempt cannot satisfy the two-attempt gate.
4. Extend scripts/check-m11-transit-qualification-evidence.sh so these false-positive shapes
   fail statically or through focused parser tests.

Do not delete useful Plan 256 anti-fan-out tests.

## 6. Work package B — qualify local-IBGW / OBEP production reply semantics

Audit the Plan 256 production path centered on:
- TransitLiveOwner::is_self_reply
- TransitLiveOwner::deliver_self_reply
- TransitBuildService::deliver_self_reply_otbrm
- TransitBuildService::build_tunnel_relay_otbrm
- otbrm_nested_standard_envelope
- route_router_garlic and the new garlic-reply helpers.

Required behavior:

### 6.1 Remote reply IBGW

For an OBEP whose reply router is remote:
- preserve the transformed count-prefixed OTBRM body;
- create the correct ShortTunnelBuildReply I2NP message id;
- apply the RGarlicKeyAndTag wrapper exactly once;
- preserve requested reply tunnel id;
- emit one TunnelGateway delivery to the remote reply router;
- rollback an accepted endpoint registration on every terminal non-Accepted delivery outcome.

### 6.2 Local reply IBGW

For an OBEP whose reply router is the local i2pr identity:
- require a live matching local IBGW registration for the requested reply tunnel;
- inject the reply into the canonical IBGW/tunnel data path;
- do not send the reply through RouterDeliveryService as a remote router delivery;
- do not apply the remote-router garlic/TunnelGateway wrapper;
- apply the local inbound-tunnel preprocessing/layering exactly once;
- preserve the reply message id and count-prefixed OTBRM body;
- fail closed and rollback the endpoint registration if the local IBGW registration is absent,
  wrong-role, expired, cancelled, or cannot emit the cell.

### 6.3 No double ownership

A reply may take exactly one of:
- remote reply-router path;
- local IBGW self-reply path.

It must never be delivered by both.

Required focused regressions:
1. remote OBEP reply uses remote garlic/TunnelGateway path once;
2. local reply bypasses remote garlic/TunnelGateway path;
3. local reply with live IBGW registration emits through that exact registration;
4. local reply with missing IBGW registration fails closed and rolls back endpoint state;
5. wrong-role receive id cannot satisfy local IBGW;
6. expired/cancelled IBGW cannot satisfy local reply;
7. local and remote branches preserve exact reply message id and reply tunnel id;
8. no double delivery when reply router equals local identity;
9. source-lock checker rejects drift from exact i2pd 2.61.0 branch semantics.

This package may change production code only where these tests/source locks prove a defect.

## 7. Work package C — expose a bounded non-secret transit state snapshot

The external lane needs state evidence stronger than active_count().

Prefer one narrow non-secret snapshot owned by the transit service/owner, for example:

    TransitLiveStateSnapshot {
        active_registrations,
        pending_global,
        pending_peer_entries,
        peer_index_entries,
        transit_owned_queued_work,
    }

The exact API name may differ. Requirements:

- no secret material or payload bytes;
- O(1) or bounded O(n) over already bounded state;
- read-only and side-effect free;
- available to the controlled qualification driver without duplicating state;
- production semantics unchanged;
- Debug output safe.

If transit owns no independent queue, encode that architectural fact explicitly in the snapshot
or a static invariant rather than inventing a runtime counter.

Use the snapshot for:
- exact registration delta;
- cancellation drain;
- restart baseline;
- session-close peer-index checks;
- pending baseline after rejection.

## 8. Work package D — exact accepted-registration cardinality

For each fresh OBEP, IBGW, and Participant counted epoch:

1. capture state before the counted build;
2. observe the typed accepted build event;
3. capture state immediately after the event;
4. require active_registrations delta == 1;
5. require the counted receive tunnel id resolves to that registration;
6. require pending state returned to baseline;
7. record role, authenticated previous peer, receive id, and next/reply routing facts;
8. do not allow unrelated accepted builds to satisfy the row.

If background reference churn makes the delta ambiguous, use a fresh bounded role owner/topology
epoch or a one-registration admission ceiling. Do not periodically reset live state merely to
make a row pass.

Required evidence per role:

    <role>/active-before
    <role>/active-after
    <role>/registration-delta
    <role>/receive-id
    <role>/pending-baseline

The runner's "exactly one registration" wording is permitted only when these values prove it.

## 9. Work package E — typed bandwidth request/reply evidence

Surface the already-decoded, non-secret build-option disposition from i2pr-tunnel to the typed
build evidence. Do not decode Mapping again in the daemon.

The evidence must distinguish:
- m absent vs present(value);
- r absent vs present(value);
- l absent vs present(value);
- b absent vs present(value) in the reply;
- accepted vs code-30 rejected.

For stock i2pd traffic, if m/r/l are absent, the counted row must say they are absent because
the decoded request summary says so. If they are present, record the actual sanitized numeric
values and validate the b/code-30 behavior against Plan 250 semantics.

Do not require i2pd to emit options it does not emit. Do not synthesize options in the external
counted lane.

Required evidence:

    reject/bandwidth-request
    reject/bandwidth-reply
    reject/bandwidth-disposition-observed

## 10. Work package F — independent Participant far-side proof

The Participant data row must prove both sides of the hop:

    i2pr local forward
        AND
    i2pd-B receipt at the exact next tunnel id

Preferred implementation:

1. source-lock exact i2pd 2.61.0 TransitTunnelEndpoint::HandleTunnelDataMsg;
2. run the dedicated Participant B reference at debug only for the bounded Participant data
   epoch if necessary;
3. capture the exact next_tunnel from TransitDataForwardEvidence;
4. record a sanitized B-side delta/count for
   "TransitTunnel: handle msg for endpoint <next_tunnel>";
5. bind that observation to the same epoch after the local forward;
6. retain only the sanitized count/tunnel id or digest needed for evidence.

Raw reference logs are diagnostic input, not counted evidence.

If B debug logging measurably destabilizes the lane, source-lock and use another exact-pinned
B-side counter/management observable. Do not fall back to:
- local DataForwarded alone;
- A-side creator-accepted alone;
- "B process is running";
- a file/configuration fact.

Required rows:

    participant-data/local-forward
    participant-data/next-tunnel
    participant-data/b-endpoint-observed
    participant-data/far-side-count
    participant-data/creator-accepted

The first four are required for the forwarding claim. creator-accepted remains useful secondary
evidence but cannot replace B-side receipt.

## 11. Work package G — cancellation and session-close state proof

### 11.1 Cancellation

Start from nonzero live transit state. Record the full state snapshot before cancellation.
Cancel synchronously. Record the snapshot after cancellation.

Require after cancellation:
- active_registrations == 0;
- pending_global == 0;
- pending_peer_entries == 0;
- peer_index_entries == 0;
- transit_owned_queued_work == 0;
- new build/data/gateway ingress fails closed.

Required rows:

    cancel/active-before
    cancel/active-after
    cancel/pending-after
    cancel/peer-index-after
    cancel/queued-work-after
    cancel/new-ingress-refused

### 11.2 Session close

With authenticated A and B mappings installed:
1. prove both mappings exist;
2. close/reconcile A;
3. prove A mapping is absent;
4. prove B mapping remains;
5. close/reconcile B later or let cancellation drain it.

Required rows:

    session-close/a-before
    session-close/b-before
    session-close/a-removed
    session-close/b-retained
    session-close/final-peer-baseline

No row may infer B retention from a total count alone when identity-specific membership can be
checked.

## 12. Work package H — real i2pr runtime restart experiment

Replace the constructor-only restart row.

The counted restart experiment must:

1. begin with a functioning controlled i2pr SSU2 service and TransitLiveOwner;
2. retain only public/reference material needed to reconnect;
3. shut down the i2pr service/owner through the normal bounded lifecycle;
4. prove the old owner's state is drained;
5. construct a new i2pr service/owner from zero state;
6. prove the new state snapshot is all zero before peer installation;
7. re-establish authenticated sessions to the exact-pinned reference routers;
8. install peer mappings from those authenticated sessions;
9. execute one fresh role-correct build after restart;
10. prove that build is accepted into exactly one new registration;
11. shut down and drain again.

Do not satisfy restart with:
- TransitLiveOwner::new_disabled plus active_count == 0;
- reusing the old owner;
- copying old registration state;
- a local hand-built STBM.

Required rows:

    restart/old-owner-drained
    restart/new-owner-zero
    restart/sessions-reestablished
    restart/fresh-build-accepted
    restart/fresh-registration-delta
    restart/final-baseline

## 13. Work package I — exact-head macOS Clippy corrective

Reproduce the Clippy failure from Actions run 36332304955 against Plan 256 head
bc9b5c6172d0560d3eb8d2ef1ae0d447478ba480.

Fix the underlying cross-platform warning/error narrowly.

Forbidden shortcuts:
- adding a broad allow attribute;
- skipping Clippy on macOS;
- cfg-gating production behavior away only to make CI green;
- lowering -D warnings;
- removing the macOS Quality job.

Add a focused regression or compile-time guard if the failure reflects a platform-specific code
path rather than a simple lint.

Plan 257 closure requires a new exact-head CI run where Ubuntu Quality, macOS Quality, MSRV,
and dependency policy all pass.

## 14. Work package J — make two complete same-SHA passes mechanically unambiguous

The current hosted workflow runs one complete matrix per workflow dispatch. Plan 257 should
make the two-pass closure gate explicit.

Preferred workflow shape:
- one workflow_dispatch;
- matrix attempt = [1, 2];
- both jobs check out the same github.sha;
- fail-fast = false;
- each attempt creates fresh datadirs/ports/evidence;
- artifact names include attempt number;
- no evidence is shared or merged between attempts;
- closure records both job/run ids and verifies both result manifests name the same i2pr SHA.

Two separate workflow dispatches are acceptable only if the checker/closure tooling proves the
same exact SHA and fresh datadirs for both. Do not accept "latest main" equivalence.

Each result manifest must contain at least:
- schema version;
- plan = 257;
- i2pr SHA;
- exact i2pd version and commit;
- execution/attempt id;
- all mandatory row statuses;
- fresh datadir identifiers or non-secret hashes;
- sanitized environment/toolchain identity;
- overall pass/fail.

No complementary partial attempts may merge into a pass.

## 15. Evidence checker hardening

Extend scripts/check-m11-transit-qualification-evidence.sh and focused parser tests so closure
cannot regress to Plan 256's remaining false-positive shapes.

The checker must reject:

1. Participant far-side pass without a B-side endpoint/counter observation;
2. creator-accepted used as the only far-side proof;
3. restart pass produced only by constructing a new owner and checking active_count;
4. cancellation pass that checks only active registrations;
5. session-close pass that removes A without proving B remains;
6. bandwidth disposition emitted from a literal/hard-coded string rather than typed decoded
   evidence;
7. exact-registration wording without before/after delta evidence;
8. local-IBGW self-reply qualification without exact pinned source locks;
9. a complete qualification claim with fewer than two independent same-SHA complete attempts;
10. evidence merged across attempts;
11. raw reference logs treated as evidence;
12. any retained private/secret material;
13. public-network/reseed fallback;
14. ordinary product transit enablement.

Keep all Plan 256 anti-fan-out, role-typing, pin, NetDB, identity-coherence, and no-direct-build
guards.

## 16. Failure, cancellation, contention, and restart semantics

- Every reference child and i2pr runtime owner has one bounded lifecycle owner.
- Every wait has a finite timeout.
- Startup failure is a failed attempt, never skipped/pass.
- Missing B-side observation is a failed Participant row, not a retry-until-green loop.
- Resource/code-30 rejection is recorded as the protocol result; no hidden admission reset may
  convert it to acceptance.
- Do not clear live registrations periodically to increase reference retry success.
- Evidence/event storage has a hard ceiling.
- No per-cell tasks.
- Cancellation is synchronous from the transit owner's perspective.
- Restart creates a new owner/runtime boundary; old state is never transferred.
- Session close removes only the closed peer mapping.
- A terminal delivery failure rolls back only state created by the affected accepted build.
- Failure to prove local-IBGW reply semantics stops external qualification.

## 17. Compatibility and migration

No user migration is expected.

Plan 257 must not change:
- config schema;
- CLI surface;
- RouterInfo advertised capabilities;
- router.version;
- SAM/I2CP public behavior;
- default transit-disabled product construction;
- reference pins.

A new non-secret state/evidence type may be added if required for qualification. Keep it narrow
and document that it exposes bounded counts/routing facts only.

No new dependency is expected. If a dependency is proposed, stop for review rather than adding
it to make the lane pass.

## 18. Required focused tests

At minimum, add or retain explicit tests for:

1. exact i2pd remote-reply source lock;
2. exact i2pd local-IBGW source lock;
3. local self-reply uses live IBGW registration;
4. local self-reply missing/wrong/expired IBGW fails closed;
5. remote reply uses garlic/TunnelGateway exactly once;
6. local reply does not use remote garlic/TunnelGateway;
7. local/remote reply message id and tunnel id preservation;
8. no double local+remote delivery;
9. state snapshot contains active/pending/peer/queue dimensions without secrets;
10. role accept changes active state by exactly +1;
11. role reject changes active state by +0 and pending returns to baseline;
12. bandwidth evidence reports typed absence;
13. bandwidth evidence reports typed present values in local fixtures;
14. Participant far-side row fails without B observation;
15. Participant far-side row binds B observation to exact next tunnel id;
16. creator-accepted alone cannot satisfy far-side proof;
17. replay remains one-delivery-then-drop;
18. expiry remains post-lifetime drop with resource cleanup;
19. cancellation requires all state dimensions zero;
20. session close proves A removed and B retained;
21. constructor-only restart evidence is rejected;
22. runtime restart re-establishes sessions and accepts a fresh build;
23. one complete external attempt cannot satisfy two-pass closure;
24. two attempts with different i2pr SHAs are rejected;
25. cross-attempt row merging is rejected;
26. exact pin/version mismatch fails before network startup;
27. missing environment fails nonzero;
28. ordinary default remains transit-disabled.

## 19. Exact verification floor

Run the routine floor:

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
    bash scripts/check-m11-transit-qualification-evidence.sh
    bash scripts/check-m6-mixed-router-acceptance-evidence.sh
    bash scripts/check-exploratory-tunnel-evidence.sh
    git diff --check

Run every new Plan 257 focused test exactly by name at least once.

Then run the complete external matrix twice on the exact same implementation SHA. If the
workflow uses a two-attempt matrix, both jobs must pass. If executed locally:

    bash tests/integration/m11-transit/run-i2pd.sh
    bash tests/integration/m11-transit/run-i2pd.sh

Each invocation must create fresh datadirs and a fresh evidence root and must independently
contain every mandatory row.

Finally require exact-head ordinary CI green:
- Quality (ubuntu-latest);
- Quality (macos-latest);
- MSRV (Ubuntu);
- Dependency policy.

## 20. Acceptance criteria

Plan 257 closes only when all are directly evidenced:

1. Plan 256 historical implementation/evidence remains traceable and is not rewritten as a pass.
2. Exact-pinned unmodified i2pd 2.61.0 / 635b013a... remains the reference.
3. Public reseed/network remain disabled and all sockets are loopback.
4. RouterIdentity/build-responder key coherence remains proven.
5. Exact NetDB owner/load proof remains proven for A and B.
6. OBEP, IBGW, and Participant remain typed separate epochs.
7. The Plan 256 local-IBGW/OBEP production reply path is source-locked against exact i2pd.
8. Local self-reply uses a live local IBGW and never the remote reply path.
9. Remote reply uses the correct remote garlic/TunnelGateway path exactly once.
10. Missing/wrong/expired local IBGW fails closed and rolls back affected endpoint state.
11. Each counted accepted role proves active registration delta exactly +1.
12. Each counted accepted role proves pending state returned to baseline.
13. Participant local forward is paired with an independent i2pd-B far-side observation for the
    exact next tunnel id.
14. A-side creator acceptance is retained only as secondary Participant evidence.
15. OBEP unfragmented and fragmented/reassembled semantic delivery remains proven.
16. IBGW genuine gateway ingress and bounded multi-cell emission remain proven.
17. Replay produces no second semantic delivery.
18. Code-30 rejection installs no registration and pending state returns to baseline.
19. Bandwidth disposition comes from typed decoded m/r/l and b evidence, including typed absence.
20. Expiry drops post-lifetime genuine data and removes secret-owning state.
21. Cancellation proves active, pending, peer-index, and transit-owned queue dimensions all zero.
22. Session close proves A removed while unrelated B remains.
23. Runtime restart drains the old owner, starts a zero-state new owner, re-establishes
    authenticated sessions, and accepts a fresh role-correct build.
24. Evidence checker rejects every Plan 257 false-positive shape listed in section 15.
25. No raw reference log or secret material is counted as evidence.
26. Complete external attempt 1 passes every mandatory row.
27. Complete external attempt 2 passes every mandatory row.
28. Both complete attempts name the same exact i2pr SHA and use fresh independent datadirs.
29. Full workspace verification passes.
30. Exact-head Ubuntu Quality, macOS Quality, MSRV, and dependency-policy jobs all pass.
31. The Plan 256 macOS Clippy failure is corrected without lint/platform suppression.
32. No product default, config surface, router.version, capability advertisement, or public
    transit behavior changes.
33. No known critical/high finding remains in the Plan 257 closure record.

Only after all 33 criteria pass may the unblock audit:
- mark m11_transit_qualification = passed-via-i2pd-2.61.0 for the ADR 0026 experimental gate;
- mark M11 experimental progression closed;
- register M12 floodfill planning.

## 21. Stop conditions

Stop and record the exact boundary rather than broadening the plan if:

- exact i2pd source contradicts the new local-IBGW/remote-reply production behavior in a way that
  requires redesign beyond this narrow reply path;
- Participant B-side receipt cannot be observed through a source-locked unmodified mechanism;
- a real role-correct external build/data event localizes another production crypto/data-plane
  defect outside the Plan 257 reply/state/evidence scope;
- restart requires transferring secret state from the old owner;
- exact registration cardinality requires deleting unrelated live state;
- deterministic completion requires patching i2pd;
- public network/reseed is required;
- a new dependency or public transit toggle is proposed;
- the macOS failure reflects a broader portability defect outside this bounded pass.

If another production defect is localized, preserve sanitized failing evidence and register the
next narrow corrective. Do not weaken a mandatory row.

## 22. Documentation and closure evidence

On closure, write plans/closure/transit-tunnels/257-status.md with:

- implementation and closure SHAs;
- exact i2pd pin/version;
- Plan 256 retained findings disposition;
- local-IBGW/remote-reply source-lock evidence;
- production reply-path focused tests;
- state snapshot and lifecycle evidence;
- typed bandwidth evidence;
- Participant B-side far-side evidence;
- exact registration cardinality evidence;
- runtime restart evidence;
- both complete same-SHA external attempt ids and manifests;
- exact-head ordinary CI run id;
- full verification command outcomes;
- security/secret review;
- resource/contention review;
- known limitations/findings by severity;
- unblock audit and M12 disposition.

Update:
- plans/registry.md
- plans/subsystems/transit-tunnels-roadmap.md
- specs/support.toml
- specs/protocols/05-tunnels.md
- specs/CONFORMANCE.md only if the experimental support state actually changes.

Do not mark M11 passed merely because Plan 257 implementation code lands.

## 23. Handoff order

Execute in this order:

1. reproduce and diagnose the Plan 256 macOS Clippy failure;
2. add Plan 257 negative evidence regressions;
3. source-lock exact i2pd remote/local OBEP reply behavior;
4. qualify/fix the local-IBGW and remote-reply production paths;
5. add the bounded non-secret state snapshot;
6. make role registration cardinality exact;
7. surface typed bandwidth request/reply disposition;
8. add independent i2pd-B Participant far-side observation;
9. strengthen cancellation and session-close experiments;
10. replace constructor-only restart with the real runtime restart experiment;
11. harden the checker and two-attempt workflow gate;
12. run focused tests and the full local verification floor;
13. obtain exact-head ordinary CI green on all required jobs;
14. execute the complete external matrix twice on one SHA;
15. write closure only if every acceptance criterion is directly evidenced.

Do not begin by repeatedly dispatching the current Plan 256 workflow. The remaining evidence and
production-qualification defects must be corrected first.
