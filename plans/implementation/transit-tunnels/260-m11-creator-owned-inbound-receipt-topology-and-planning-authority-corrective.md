# Plan 260 — M11 creator-owned inbound receipt topology and planning-authority corrective

Status at registration:
**registered-m11-creator-owned-inbound-receipt-topology-and-planning-authority-corrective-ready**

Baseline: `a545f09c463c03bfbac20a31c7f5bfa77d351da5`

Corrects:
- `plans/implementation/transit-tunnels/259-m11-ibgw-receipt-topology-adjudication-diagnostic.md`
- `plans/closure/transit-tunnels/259-status.md`
- stale M11 authority projection in `plans/registry.md`, the transit roadmap,
  `specs/support.toml`, `specs/CONFORMANCE.md`, and the tunnel dossier.

Retains:
- Plan 258's production H3 correction: every IBGW nested message uses the canonical
  `fragment_complete_message` + `build_cells` path.
- Plan 258's externally demonstrated multicell emission and zero forward-failure evidence.
- Plan 259's seven-run/nine-chain topology inventory as historical evidence.
- Plans 250–257 admission, build, ownership, typed evidence, state snapshot, bandwidth,
  replay/expiry/cancel/session-close/restart, and external-lane infrastructure unless this
  corrective localizes a new defect.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0 at
`635b013a612ff47278ef02acf8580a28e10e26c5`.

## 1. Objective

Correct the Plan 259 receipt model and resume M11 qualification against the actual
creator-owned inbound-tunnel endpoint path.

Plan 259 correctly showed that a **transit endpoint** cannot deliver a destination-owned Local
garlic through a foreign destination pool. It overgeneralized that fact to all possible inbound
termini. Exact-pinned i2pd has a separate creator-owned `InboundTunnel` path: the inbound
tunnel's last remote hop forwards to the creator's local router/tunnel id, the local
`InboundTunnel::HandleTunnelDataMsg` sets `msg->from` to that pool-owned inbound tunnel, and
LOCAL garlic dispatch selects `msg->from->GetTunnelPool()->ProcessGarlicMessage`.

Plan 260 must:

1. source-lock that creator-owned inbound path and distinguish it from
   `TransitTunnelEndpoint(false)`;
2. restore IBGW end-to-end receipt as a qualification row rather than treating it as
   intrinsically unexhibitable;
3. construct a deterministic, source-supported controlled topology in which i2pr is the
   receiver destination's inbound gateway and the next hop is the receiver router's
   creator-owned local inbound tunnel;
4. bind the i2pr IBGW registration ids to the receiver's actual inbound tunnel and tunnel pool;
5. send a real destination-addressed payload through the resulting LeaseSet/tunnel and prove
   receiver-side SAM delivery;
6. keep the Plan 258 multicell emission correction and all Plan 257 lifecycle/evidence rows;
7. fix the known constant fragmented IBGW message-id collision risk while this emission path is
   under explicit plan authority;
8. reconcile all current M11 planning/support/conformance projections;
9. only close M11 if the complete corrected external matrix passes twice on one implementation
   SHA with fresh datadirs and exact-head ordinary CI green.

This is a **corrective capability-qualification plan**, not a public-transit enablement plan.

## 2. Why Plan 260 is required

### 2.1 Plan 259 modeled the wrong endpoint class for the closing inbound path

Pinned i2pd distinguishes two materially different endpoint owners.

A transit outbound endpoint uses:

    TransitTunnelEndpoint::HandleTunnelDataMsg
      -> TunnelEndpoint(false)
      -> LOCAL
      -> i2p::HandleI2NPMessage
      -> msg->from has no destination TunnelPool
      -> router-context garlic handling

A creator-owned inbound tunnel uses:

    InboundTunnel::HandleTunnelDataMsg
      -> msg->from = GetSharedFromThis()
      -> TunnelEndpoint(inbound)
      -> LOCAL
      -> i2p::HandleI2NPMessage
      -> msg->from->GetTunnelPool() exists
      -> TunnelPool::ProcessGarlicMessage
      -> destination session

Plan 259 source-locked the first path and used it to conclude that no lane-buildable IBGW chain
could close. That conclusion is not established for the second path.

### 2.2 Pinned i2pd inbound tunnel construction explicitly targets the creator router

Exact-pinned `libi2pd/TunnelConfig.cpp` shows:

    TunnelConfig(peers) // inbound
      -> CreatePeers(peers)
      -> m_LastHop->SetNextIdent(i2p::context.GetIdentHash())

and `SetNextIdent` sets:

    nextIdent = local router hash
    isEndpoint = false
    nextTunnelID = fresh nonzero id

Therefore the last **remote** hop of an inbound tunnel is not a
`TransitTunnelEndpoint`. It forwards TunnelData to the creator router's local inbound tunnel.

For an inbound config:

    TunnelConfig::GetTunnelID()     = m_LastHop->nextTunnelID
    TunnelConfig::GetNextTunnelID() = m_FirstHop->tunnelID
    TunnelConfig::GetNextIdentHash()= m_FirstHop->ident

The first-hop pair is what the LeaseSet advertises as the inbound gateway/tunnel. The final
remote hop's `nextTunnelID` is the creator-local inbound tunnel id.

### 2.3 The creator-local dispatch path can reach the destination pool

Pinned `InboundTunnel::HandleTunnelDataMsg`:
- applies the creator-known inbound transform;
- sets `msg->from = GetSharedFromThis()`;
- passes the message to its `TunnelEndpoint`.

Pinned `TunnelEndpoint::HandleNextMessage` sends LOCAL messages to
`i2p::HandleI2NPMessage`.

Pinned `HandleI2NPMessage` sends garlic to:

    msg->from->GetTunnelPool()->ProcessGarlicMessage(msg)

when the incoming tunnel owns a pool.

That is the exact destination-owned receipt path Plan 259 did not test.

### 2.4 Plan 259's A-ending observation does not establish creator-owned termination

The historical A-ending chain `0xf6f56b0a` proved only that i2pr emitted toward router A.
The retained evidence does not prove that:
- its `next_tunnel` matched an established A `InboundTunnel::GetTunnelID()`;
- that inbound tunnel belonged to the receiver SAM destination's `TunnelPool`;
- the resulting TunnelData was resolved by A's local tunnel table;
- `msg->from` was that pool-owned inbound tunnel;
- the garlic reached `TunnelPool::ProcessGarlicMessage`.

Receipt 0 on that chain therefore cannot retire the receipt requirement.

### 2.5 Planning authority is currently inconsistent

At this baseline:
- `plans/registry.md` says no active/next M11 plan;
- `specs/support.toml` still names Plan 259 as `next_executable_plan`;
- the roadmap contains both retained and "registered ready" Plan 259 statements;
- `specs/CONFORMANCE.md` still projects the old Plan 255/256 state;
- the tunnel dossier still embeds the Plan 259 Fork-2 conclusion as current authority.

Plan 260 registration must reconcile those surfaces immediately.

### 2.6 A known production hardening issue remains on the same emission path

`InboundGatewayRole` currently uses constant fragment `message_id: 1`.
Sequential tests pass, but concurrent fragmented ingresses to the same downstream reassembler
can collide/cross-assemble before failing closed. This is a bounded availability/integrity risk
in the emission path already under Plan 260 authority and should be corrected before final M11
qualification.

## 3. Why ready

Hard dependencies are sufficient for this corrective:

- Plan 250: admission/reply/registry semantics retained.
- Plan 252: full-message STBM transaction retained.
- Plan 253/254: controlled live owner and data-plane composition retained.
- Plan 256/257: exact identity/NetDB/bootstrap, typed role/evidence, lifecycle and restart
  infrastructure retained.
- Plan 258: H3 fragmentation defect corrected and multicell emission externally demonstrated.
- Plan 259: topology inventory and transit-endpoint dispatch analysis retained as historical
  evidence, while its global receipt conclusion is narrowed by this plan.

No new architectural decision is required. The corrected endpoint distinction follows directly
from the frozen reference source.

## 4. Frozen invariants

### 4.1 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean and unmodified.
- Loopback-only.
- Public reseed/network disabled.
- Fresh datadirs for every counted complete attempt.
- No LD_PRELOAD, source patch, in-memory reference mutation, fake peer injection, or public peer.

### 4.2 Product authority

- Ordinary i2pr transit remains disabled.
- No RouterInfo transit capability.
- No router.version change.
- No public transit config option.
- No public-network transit.
- No M12 implementation or promotion before Plan 260 closure.

### 4.3 Protocol/data plane

- Plan 258 fragment-for-all-sizes path remains canonical.
- IBGW emits LOCAL Tunnel Message delivery, matching the reference.
- No delivery-type deviation to make receipt pass.
- No second I2NP/tunnel decoder.
- No direct registry insertion.
- No hand-built STBM after runtime startup.
- Existing replay/reassembly/resource bounds remain.
- No per-cell task spawning or unbounded queue.

### 4.4 Evidence

- Raw reference logs are diagnostic input only.
- Counted evidence is sanitized ids/counts/hashes/status only.
- No payload or private/secret material retained.
- Every receipt claim must bind one epoch's build, registration, downstream local tunnel,
  destination pool, emission, reference-local dispatch, and receiver socket delivery.
- Two attempts may not merge rows.

## 5. Work package A — source-lock the creator-owned inbound topology

Extend the exact-pinned source-lock checker with the full inbound construction and dispatch
chain.

Required pinned facts:

### A1. Inbound build topology

From `TunnelConfig.cpp`:
- inbound constructor calls `CreatePeers(peers)`;
- then calls `m_LastHop->SetNextIdent(i2p::context.GetIdentHash())`;
- `SetNextIdent` clears `isEndpoint`;
- `SetNextIdent` allocates nonzero `nextTunnelID`.

### A2. Tunnel id meanings

From `TunnelConfig.h`:
- inbound `GetTunnelID()` returns `m_LastHop->nextTunnelID`;
- `GetNextTunnelID()` returns `m_FirstHop->tunnelID`;
- `GetNextIdentHash()` returns the first remote hop.

These establish:
- LeaseSet-facing gateway tunnel id = first-hop receive tunnel id;
- creator-local receive id = last-hop `nextTunnelID`.

### A3. Local tunnel lookup

From `Tunnel.cpp` tunnel-loop dispatch:
- TunnelData/TunnelGateway reads the leading tunnel id;
- `GetTunnel(tunnelID)` resolves the registered local tunnel;
- TunnelData invokes `tunnel->HandleTunnelDataMsg`.

### A4. Creator-owned inbound owner

From `InboundTunnel::HandleTunnelDataMsg`:
- `msg->from = GetSharedFromThis()`;
- message enters its local `TunnelEndpoint`.

### A5. LOCAL garlic dispatch

From `TunnelEndpoint::HandleNextMessage`:
- LOCAL calls `i2p::HandleI2NPMessage`.

From `I2NPProtocol.cpp`:
- garlic with `msg->from && msg->from->GetTunnelPool()` calls
  `msg->from->GetTunnelPool()->ProcessGarlicMessage(msg)`;
- router-context processing is the fallback only when that pool condition is absent.

Required source-lock rows:

    m11-i2pd-inbound-last-hop-targets-creator-source-lock
    m11-i2pd-inbound-last-hop-not-transit-endpoint-source-lock
    m11-i2pd-inbound-local-tunnel-id-source-lock
    m11-i2pd-tunnel-data-local-lookup-source-lock
    m11-i2pd-inbound-sets-message-owner-source-lock
    m11-i2pd-local-garlic-pool-dispatch-source-lock
    m11-i2pd-transit-endpoint-vs-inbound-owner-distinction-source-lock

The checker must fail if the old Plan 259 model conflates
`TransitTunnelEndpoint(false)` with creator-owned `InboundTunnel`.

## 6. Work package B — source-lock deterministic explicit-peer construction

Use stock i2pd configuration mechanisms, not reference patches.

Pinned source already exposes:
- `inbound.length`;
- `inbound.lengthVariance`;
- `inbound.quantity`;
- `explicitPeers`.

`TunnelPool::SetExplicitPeers` accepts the explicit peer set and clamps inbound/outbound
lengths to the available explicit-peer count, setting one tunnel of each direction when
nonempty.

Before relying on it, source-lock the exact 2.61.0 selection path proving the explicit peer is
actually selected for the dedicated destination's inbound pool.

Preferred receiver fixture:

    router A:
      dedicated SAM receiver destination
      inbound.length = 1
      inbound.lengthVariance = 0
      inbound.quantity = 1
      explicitPeers = <i2pr router hash>

    router B:
      independent SAM sender destination
      sends to A receiver destination

This is a deterministic controlled test topology, not public-network tuning.

If `explicitPeers` is not sufficient to prove the receiver destination's inbound path without
also creating an ambiguous ownership state, stop and record the exact source/runtime boundary.
Do not patch i2pd or fake tunnel state.

Required rows:

    m11-i2pd-explicit-peer-inbound-selection-source-lock
    m11-i2pd-receiver-destination-created
    m11-i2pd-receiver-inbound-one-hop-selected-i2pr

## 7. Work package C — bind the full tunnel-id tuple

For the counted receiver inbound tunnel, evidence must bind all ids, not merely router hashes.

Define sanitized tuple:

    receiver_destination_hash
    i2pr_router_hash
    ibgw_receive_tunnel      // first-hop receive id advertised in LeaseSet
    creator_router_hash      // A
    creator_local_tunnel     // last-hop nextTunnelID / A InboundTunnel::GetTunnelID
    pool_owner_destination   // receiver destination hash

Required proof:

1. A's receiver pool creates an inbound tunnel whose only remote peer is i2pr.
2. The i2pr build event is typed `InboundGateway`.
3. i2pr's accepted registration:
   - `receive_tunnel == ibgw_receive_tunnel`;
   - `next_router == A`;
   - `next_tunnel == creator_local_tunnel`.
4. A has an established creator-owned `InboundTunnel` whose
   `GetTunnelID() == creator_local_tunnel`.
5. That local inbound tunnel's `GetTunnelPool()` belongs to the receiver destination.
6. A's published receiver LeaseSet advertises:
   - gateway = i2pr;
   - tunnel id = `ibgw_receive_tunnel`.

No row may pass from a next-router hash alone.

Use an existing source-supported reference observable if available. If the reference does not
expose enough runtime ids without modification, derive the tuple from independently source-locked
public build/LeaseSet/log facts. If the mapping cannot be established fail-closed, stop rather
than infer it.

Required evidence keys:

    ibgw-receipt/receiver-destination
    ibgw-receipt/ibgw-receive-id
    ibgw-receipt/creator-router
    ibgw-receipt/creator-local-tunnel-id
    ibgw-receipt/pool-owner-destination
    ibgw-receipt/leaseset-gateway-match
    ibgw-receipt/leaseset-tunnel-match
    ibgw-receipt/full-tuple-bound

## 8. Work package D — restore genuine IBGW end-to-end receipt

The counted data experiment is:

    B sender destination
      -> destination-addressed garlic for A receiver
      -> outbound delivery to A's published lease
      -> TunnelGateway to i2pr (accepted IBGW receive id)
      -> Plan 258 canonical 1-or-more TunnelData emission
      -> A creator-local InboundTunnel id
      -> creator-owned inbound transform/reassembly
      -> LOCAL I2NP garlic
      -> receiver TunnelPool::ProcessGarlicMessage
      -> A receiver SAM socket

A pass requires all of:

1. genuine TunnelGateway ingress to the accepted i2pr registration;
2. payload size that forces at least two emitted TunnelData cells;
3. zero i2pr forward failures;
4. A resolves the emitted `next_tunnel` as the exact creator-owned inbound tunnel from §7;
5. reference-side inbound owner/pool dispatch is observed through source-locked sanitized facts;
6. receiver SAM socket obtains the exact payload bytes/digest once;
7. no duplicate semantic delivery;
8. no public network/reseed.

Restore mandatory rows:

    m11-i2pd-ibgw-gateway-ingress
    m11-i2pd-ibgw-multicell-bounded
    m11-i2pd-ibgw-creator-local-tunnel-bound
    m11-i2pd-ibgw-pool-owned-local-dispatch
    m11-i2pd-ibgw-gateway-receipt
    m11-i2pd-ibgw-gateway-receipt-once

The Plan 258 byte-exact parse simulation remains a supporting regression but does not replace
the live receipt row.

## 9. Work package E — adjudicate the historical Plan 259 A-ending chain correctly

Retain the Plan 259 seven-run/nine-chain table unchanged as historical evidence.

For the historical A-ending chain, classify only what is directly known:

    i2pr emitted toward A
    receipt = 0

Do **not** call it a creator-owned inbound endpoint unless the historical evidence binds the
`next_tunnel` to an A `InboundTunnel` owned by the receiver pool.

The Plan 260 closure should state one of:

- historical chain was not a creator-owned receiver inbound tunnel;
- historical evidence is insufficient to classify it;
- historical chain did target such a tunnel, in which case preserve it as a distinct failure
  and localize why the controlled Plan 260 path differs.

Do not retroactively rewrite Plan 259 raw counts.

## 10. Work package F — fix constant fragmented IBGW message ids

Replace the constant `message_id: 1` used by IBGW fragmentation with a bounded uniqueness
scheme.

Preferred ownership:
- per-registration monotonic `u32` fragment-message sequence;
- initialized from a nonzero random seed when the registration is created if convenient;
- increment/wrap while skipping zero;
- owned by the IBGW registration/role;
- no global lock;
- no external dependency.

Alternative:
- thread an existing nested I2NP message id if the canonical API exposes it cleanly and doing so
  does not duplicate parsing.

Requirements:

1. two concurrent fragmented ingresses on one registration receive different fragment message
   ids;
2. follow-on fragments of one message retain the same id;
3. wrap skips zero;
4. distinct registrations do not share mutable counter state;
5. no payload-dependent id;
6. no secret material;
7. cancellation/drop destroys the counter with the registration;
8. wire format unchanged.

Add an adversarial interleaving test:
- fragment two >1-cell nested messages;
- interleave cells at the downstream endpoint/reassembler;
- both messages reassemble exactly once to their own bytes;
- no cross-assembly.

If the correct ownership requires a wider data-plane redesign, stop and register a separate
hardening plan instead of bloating Plan 260.

## 11. Work package G — preserve Plan 257 completion rows

Once the corrected IBGW receipt gate passes, continue the complete existing matrix. Do not stop
after the new receipt row.

The complete attempt must still prove:
- exact role registration cardinality;
- typed bandwidth request/reply disposition;
- Participant independent far-side receipt;
- replay suppression;
- expiry cleanup;
- full-dimensional cancellation drain;
- session-close A-removal/B-retention;
- real i2pr runtime restart and fresh post-restart accepted build;
- no secret/raw-log evidence.

Any newly exposed production defect receives a new corrective plan under the existing stop
policy. Do not weaken the row.

## 12. Work package H — repair planning/support/conformance authority

Registration itself must correct current projection enough that execution authority is
unambiguous.

At registration:
- Plan 259 becomes retained with its topology inventory and transit-endpoint finding preserved,
  but its "receipt is OBEP-only/unexhibitable" conclusion is explicitly narrowed and corrective
  required via Plan 260;
- Plan 260 becomes `ready`;
- `active_plan = 260`, `next_executable_plan = 260`;
- `specs/support.toml` names 260, not 259;
- roadmap table/summary names Plan 260 as current authority;
- `specs/CONFORMANCE.md` stops projecting Plan 256 as the current future qualifier and names
  M11 as retained/unclaimed pending Plan 260;
- tunnel dossier distinguishes transit endpoints from creator-owned inbound endpoints.

On closure, update all surfaces in the same commit.

## 13. External attempt model

Reuse the existing Plan 257/258 runner and driver. Do not create a second harness.

The workflow remains a two-attempt same-SHA gate:

    attempt 1: fresh A/B datadirs, fresh ports, fresh evidence root
    attempt 2: fresh A/B datadirs, fresh ports, fresh evidence root

Both attempts must independently pass every mandatory Plan 260 row.

Each manifest must include:

    schema
    plan = 260
    i2pr SHA
    i2pd version
    i2pd commit
    attempt id
    receiver destination hash (sanitized hash is acceptable)
    ibgw receive id
    creator local tunnel id
    fresh datadir ids/non-secret hashes
    all row statuses
    overall status

No cross-attempt merge.

## 14. Failure/cancellation/restart semantics

- Every reference child and i2pr owner remains bounded by one lifecycle owner.
- Every wait is finite.
- Failure to obtain the exact receiver-owned one-hop inbound topology is a failed/blocked
  attempt, not a reason to use an arbitrary A-ending chain.
- A wrong `next_tunnel` fails before receipt is counted.
- A receiver payload arriving through a different lease/tunnel does not satisfy the row.
- Duplicate receipt fails.
- Cancellation/restart behavior remains Plan 257 authority and must still be executed after
  receipt.
- No periodic state clearing, route forcing after startup, retry-until-green, or timeout
  inflation.
- No reference debug log line alone counts as semantic receipt.

## 15. Compatibility and migration

No user migration is expected.

Must not change:
- config schema;
- CLI;
- public SAM/I2CP API;
- RouterInfo capabilities;
- router.version;
- product transit default;
- public network behavior.

The fragment-message-id fix is internal state/wire-field uniqueness only; it does not alter
message format.

No new dependency is expected.

## 16. Required focused tests

At minimum:

1. source lock: inbound constructor targets local creator router;
2. source lock: `SetNextIdent` clears endpoint flag;
3. source lock: inbound `GetTunnelID` is last-hop next id;
4. source lock: first-hop gateway id/hash semantics;
5. source lock: local TunnelData id lookup invokes resolved tunnel;
6. source lock: `InboundTunnel` sets `msg->from`;
7. source lock: LOCAL garlic with pool calls `TunnelPool::ProcessGarlicMessage`;
8. source lock: transit endpoint and creator inbound endpoint are distinct classes;
9. explicit-peer source lock yields a one-hop inbound pool for one explicit peer;
10. evidence predicate rejects A-ending router hash without local-tunnel binding;
11. evidence predicate rejects local-tunnel binding without pool-owner binding;
12. evidence predicate rejects pool binding without LeaseSet gateway/tunnel binding;
13. evidence predicate accepts the complete six-field tuple;
14. IBGW 500-byte nested message emits one cell;
15. IBGW 1.5-KiB nested message emits at least two cells;
16. IBGW ~62-KiB legal nested message emits bounded multiple cells;
17. two fragmented ingresses use distinct message ids;
18. every fragment of one message uses the same id;
19. message-id wrap skips zero;
20. interleaved two-message downstream reassembly returns both exact byte strings once;
21. replay still gives no second delivery;
22. cancellation still drains all five state dimensions;
23. session close still retains unrelated peer;
24. runtime restart still begins zero and accepts a fresh build;
25. two-attempt checker rejects one attempt;
26. checker rejects mixed SHAs;
27. checker rejects stale Plan 259 "receipt unexhibitable" as current qualification authority;
28. missing environment/pin fails before network startup;
29. ordinary product transit remains disabled.

## 17. Exact verification floor

Run:

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

Run every new Plan 260 focused test exactly by name.

Then execute two complete fresh-datadir external attempts on the same implementation SHA via the
hosted matrix or equivalent exact local commands.

Closure requires exact-head ordinary CI green:
- Quality (ubuntu-latest);
- Quality (macos-latest);
- MSRV (Ubuntu);
- Dependency policy.

## 18. Acceptance criteria

Plan 260 closes only when all are directly evidenced:

1. Plan 258 fragmentation correction remains intact.
2. Plan 259 topology inventory remains traceable and unmodified as historical evidence.
3. Plan 259's global "receipt is OBEP-only/unexhibitable" interpretation is superseded as
   current authority.
4. Exact-pinned i2pd source locks the creator-owned inbound construction path.
5. Exact-pinned source distinguishes `TransitTunnelEndpoint(false)` from
   creator-owned `InboundTunnel`.
6. Exact-pinned source proves pool-owned LOCAL garlic dispatch.
7. Dedicated A receiver destination owns an established inbound tunnel whose only remote peer
   is i2pr.
8. i2pr accepts the corresponding build as typed IBGW.
9. i2pr's receive id matches A's published lease gateway tunnel id.
10. i2pr's `next_router` is A.
11. i2pr's `next_tunnel` matches A's creator-local `InboundTunnel::GetTunnelID()`.
12. That A inbound tunnel belongs to the receiver destination's pool.
13. The full six-field receipt tuple is recorded in sanitized evidence.
14. B sends a genuine destination-addressed payload through A's published LeaseSet.
15. i2pr receives genuine TunnelGateway ingress on the counted IBGW registration.
16. The counted payload forces at least two TunnelData cells.
17. All emitted cells are accepted with zero i2pr forwarding failures.
18. A resolves the exact creator-local tunnel id.
19. LOCAL garlic dispatch is observed on the pool-owned inbound path.
20. The receiver SAM socket receives the exact payload/digest exactly once.
21. Plan 257 typed cardinality and bandwidth rows pass.
22. Participant far-side row passes independently.
23. Replay row passes.
24. Expiry row passes.
25. Cancellation all-dimension drain passes.
26. Session-close unrelated-peer retention passes.
27. Runtime restart + fresh accepted build passes.
28. Fragmented IBGW messages use nonconstant bounded unique ids.
29. Interleaved fragmentation regression proves no cross-assembly.
30. Complete external attempt 1 passes every mandatory row.
31. Complete external attempt 2 passes every mandatory row.
32. Both attempts name the same i2pr SHA and independent fresh datadirs.
33. Full workspace verification passes.
34. Exact-head ordinary CI is green on all four required jobs.
35. Registry, roadmap, support ledger, conformance matrix, and tunnel dossier agree on the final
    state.
36. No product default/capability/version/public-network change lands.
37. No critical/high finding remains open.

Only then may the unblock audit mark the ADR 0026 one-family M11 experimental qualification
passed and reconsider M12 registration.

## 19. Stop conditions

Stop and record the exact boundary if:

- exact-pinned i2pd contradicts the creator-owned inbound model stated above;
- stock i2pd cannot construct the dedicated one-hop inbound receiver pool through supported
  configuration;
- the reference exposes no source-supported way to bind the creator-local tunnel id/pool owner
  strongly enough for counted evidence;
- a correctly bound creator-owned inbound tunnel receives the exact cells but fails before
  destination-pool garlic processing, localizing a new reference/interoperability boundary;
- destination-pool garlic processing occurs but the exact receiver SAM socket still sees no
  payload, localizing a higher-layer defect;
- the fragment-id fix requires a broad data-plane redesign;
- another production crypto/data-plane defect outside this scope appears;
- public network, reference patching, false capabilities, or a new dependency would be needed.

Do not retire the receipt row merely because the controlled topology is difficult to construct.

## 20. Closure evidence required

Write `plans/closure/transit-tunnels/260-status.md` with:

- implementation and closure SHAs;
- exact reference pin/version;
- source-lock table for the creator-owned inbound path;
- explicit-peer selection proof;
- complete receipt tuple;
- receiver LeaseSet binding;
- i2pr IBGW build/data evidence;
- creator-local tunnel resolution/pool ownership evidence;
- exact SAM payload receipt evidence;
- fragment message-id hardening evidence;
- all carried-forward Plan 257 lifecycle rows;
- both complete same-SHA attempt manifests;
- full verification floor;
- exact-head CI run;
- security/resource/concurrency review;
- findings by severity;
- roadmap/unblock audit.

If the plan stops at a new boundary, close as retained/blocked with the exact failing tuple and
register the narrow successor. Do not convert a stopped row into a pass.

## 21. Handoff order

1. add the creator-owned inbound source locks;
2. add negative evidence tests for the Plan 259 endpoint-class conflation;
3. source-lock explicit-peer one-hop inbound construction;
4. add the six-field receipt-tuple evidence model;
5. construct the dedicated A receiver / B sender controlled topology;
6. prove exact build/LeaseSet/local-tunnel/pool bindings before sending payload;
7. restore and execute live multicell IBGW receipt;
8. fix the constant fragmented message id and add interleaving regression;
9. carry the remaining Plan 257 external epochs to completion;
10. harden checker/manifest rules for Plan 260;
11. run the full local floor;
12. run two complete same-SHA external attempts;
13. obtain exact-head ordinary CI green;
14. close only if all 37 acceptance criteria are directly evidenced.

Do not begin by removing the receipt row or by treating another arbitrary A-ending chain as the
creator-owned receiver path.
