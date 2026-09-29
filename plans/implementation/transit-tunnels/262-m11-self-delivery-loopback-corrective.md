# Plan 262 — M11 IBGW ingress ownership + self-delivery loopback corrective

Status at registration:
**registered-m11-self-delivery-loopback-corrective-ready**

Scope refinement baseline:
`3289051d368dda1aa4ea5e0cc9c88807e708d183`

Planning reconciliation through `f4325248e72967f7d71b348ff405869fc6166fc2` is
planning/specification-only; it does not change the production source baseline inherited from
Plan 261. Subsequent planning-only reconciliation commits may advance `main` again. The
implementation agent MUST start from the latest `main`, record the exact pre-implementation
HEAD in the handoff, and bind every counted external attempt to the eventual implementation SHA.
The historical registration/scope-refinement SHAs are planning provenance, not qualification
SHAs.

Original registration baseline and Plan 261 closure authority remain recorded in
`plans/closure/transit-tunnels/261-status.md`.

Corrects:

- `plans/implementation/transit-tunnels/261-m11-b-sender-receipt-requalification.md`
  (§10 stop #2 / B3: B-sender garlics die as OBEP TUNNEL `NoActiveSession` before IBGW ingress);
- `plans/closure/transit-tunnels/261-status.md` (B3 boundary);
- the inherited runtime-neutral IBGW ingress ownership model in
  `crates/i2pr-tunnel/src/transit.rs` / `crates/i2pr-daemon/src/transit_compose.rs`,
  where `TunnelGateway` acceptance currently reuses participant-style build-creator
  `previous_peer` affinity even though the exact-pinned reference dispatches gateway
  traffic by tunnel id without creator-peer affinity.

Retains:

- Plan 260 WP A/B/F: creator-owned inbound source locks, explicit-peer inbound lock,
  per-registration/per-role fragment-id hardening with regressions;
- Plan 260 receipt-epoch harness: six-field `ReceiptTuple` validator,
  `Epoch::IbgwReceipt`, and `I2PR_M11_ONLY_EPOCH=receipt`;
- Plan 261 WP A/B lane work: B-side source locks, B SAM plumbing, `m11-tx-b`,
  B LeaseSet-resolution evidence, `b-sender-outcome`, and send-window budget;
- Plan 261 external fact: four B-originated sends reached i2pr OBEP as TUNNEL actions
  targeting the local router and counted A IBGW tunnel ids, then died at
  `NoActiveSession`;
- participant and OBEP TunnelData previous-peer locking/replay behavior;
- Plan 258 canonical fragment-for-all-sizes IBGW emission.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0 at
`635b013a612ff47278ef02acf8580a28e10e26c5`.

Normative protocol authority is independent of that behavioral source lock:

- `https://www.i2p.net/en/docs/specs/tunnel-creation-ecies/` defines the IBGW flag as
  "allow messages from anyone"; build-creator identity is therefore not an IBGW data-plane
  authorization predicate.
- `https://www.i2p.net/en/docs/specs/i2np/` defines `TunnelGateway` routing by its nonzero
  destination tunnel id.
- `https://www.i2p.net/en/docs/specs/tunnel-implementation/` defines tunnel ids as the
  per-hop receive/forward routing identifiers.

The exact-pinned i2pd source lock remains the required independent behavioral corroboration; it
is not the sole authority for removing creator-peer affinity.

## 1. Objective

Close the final known M11 data-plane ownership defect and then execute the complete milestone
qualification.

There are two related semantics to correct at the same IBGW ingress ownership boundary:

1. **Self-loopback:** a decoded OBEP TUNNEL action whose target router is i2pr itself must enter
   the local IBGW registration rather than the remote-session peer map.
2. **Gateway source affinity:** an IBGW `TunnelGateway` is addressed by receive tunnel id; it
   is not authenticated to the router that created the tunnel. The current runtime-neutral
   `process_tunnel_gateway` incorrectly rejects gateway traffic whose transport peer differs
   from the ShortTunnelBuild creator.

After these corrections, Plan 262 must rerun the proven B-sender receipt topology and, only if
receipt closes, execute the complete carried-forward M11 matrix twice on one SHA.

This is a **narrow production ownership/data-plane corrective plus final qualification pass**.
It does not enable public transit, change wire format, add a task/channel/queue, raise quotas,
or alter RouterInfo/router.version.

## 2. Why the scope refinement is required

### 2.1 Plan 261 localized B3 exactly

On `m11-plan261-diag-bsender2`:

- A's receiver built a one-hop inbound tunnel through i2pr with counted typed IBGW
  registrations.
- B's stock SAM sender established outbound `[i2pr]`.
- B resolved A's LeaseSet from the controlled floodfill store.
- Four 1400-byte sends named counted A IBGW receive ids.
- Each send decrypted at i2pr's OBEP into a TUNNEL action with
  `target_router == local_router_hash`.
- The current delivery path looked the local hash up in the remote session peer index and
  returned `NoActiveSession`.
- Result: `terminal-garlic-self:4/ingress:0/socket:0`.

That is the starting defect and remains the primary external regression.

### 2.2 Exact-pinned i2pd has an explicit self-delivery arm

In `libi2pd/Transports.cpp`, exact-pinned
`Transports::PostMessages` compares the target identity with the local RouterInfo hash and,
for self-addressed messages, queues them to `m_LoopbackHandler` and flushes locally instead of
opening a transport session.

Plan 262 must source-lock this behavior before implementing the i2pr arm.

### 2.3 Exact-pinned i2pd does not bind TunnelGateway to the build creator

Exact-pinned `libi2pd/Tunnel.cpp` dispatches inbound `eI2NPTunnelGateway` by:

    tunnelID = payload tunnel id
    tunnel = GetTunnel(tunnelID)
    HandleTunnelGatewayMsg(tunnel, msg)
    tunnel->SendTunnelDataMsg(msg)

The dispatch surface receives the I2NP message and tunnel id; it does not compare the transport
sender with the router that built the tunnel.

That is protocol-significant. An inbound tunnel gateway is published in a LeaseSet so arbitrary
senders can inject destination traffic through that gateway. The build creator is not the
exclusive data-plane sender.

### 2.4 Current i2pr conflates Participant and IBGW peer ownership

Current `TransitGatewayData` is a type alias for `TransitParticipantData`; therefore its
debug/state shape includes `locked_previous_peer`. Current
`TransitHopRegistration::process_tunnel_gateway` rejects unless:

    registration.previous_peer == inbound transport peer

and then stores the same peer in `locked_previous_peer`.

That restriction is correct for Participant/OBEP TunnelData hop provenance but not for IBGW
TunnelGateway injection.

The self-loopback corrective must not work around this by fabricating a `PeerId` matching the
build creator. Doing so would make the counted lane pass while preserving the wrong production
authorization model.

## 3. Why one plan is still the correct size

No new plan number is needed before execution:

- Plan 262 is still registered/ready and unimplemented.
- Both defects are the same ownership boundary: how a valid TunnelGateway reaches an accepted
  local IBGW registration.
- The exact reference supplies stable contracts for both self-delivery and receive-id dispatch.
- The B-sender external topology already exists and is the consumer.
- Do not pre-register Plan 263. The M11 dependency graph remains linear at Plan 262; register a
  successor only if execution satisfies a Plan 262 stop condition and exposes a new bounded
  defect or qualification boundary.

If execution exposes a defect outside this boundary, stop and register the narrow successor.

## 4. Frozen invariants

### 4.1 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean/unmodified.
- Loopback-only qualification mesh.
- Public reseed/network disabled.
- Fresh datadirs for every counted complete attempt.
- No reference patching, LD_PRELOAD, fake peer state, or public fallback.

### 4.2 Product authority

- Ordinary i2pr transit remains disabled.
- No RouterInfo transit capability.
- No router.version change.
- No public transit config option.
- No M12 implementation.

### 4.3 Role ownership

- **Participant TunnelData:** authenticated previous-hop identity remains locked and replay
  bounded.
- **OBEP TunnelData:** previous-hop/replay semantics remain unchanged.
- **IBGW TunnelGateway:** authorization is by live receive tunnel id + IBGW role + expiry +
  bounded resource state; it is not bound to the build creator's transport identity.
- The build creator identity remains meaningful for build admission/accounting/reply routing;
  removing IBGW data-plane affinity must not erase that build provenance.
- A local self-loop is trusted router-internal delivery after OBEP decryption and local-router
  target comparison. It must not synthesize or spoof an authenticated network peer.

### 4.4 Resource/security

- No new task/channel/queue.
- Existing global/per-peer build admission remains.
- Existing active-tunnel and bounded fragmentation/reassembly limits remain.
- Unknown receive id, zero id, non-IBGW registration, expired registration, malformed nested
  I2NP, cancelled owner, and empty forward set fail closed.
- A guessed live tunnel id may reach the same bounded IBGW processing the reference permits;
  tunnel id is routing state, not an authentication token. Do not invent sender affinity as an
  authorization substitute.

### 4.5 Evidence

- Raw reference logs remain diagnostic input only.
- Counted evidence contains public hashes/ids/counts/status only.
- Diagnostic attempts never satisfy final rows.
- Two final attempts may not merge evidence.

## 5. Work package A — source-lock both reference semantics

Add exact-pin guarded rows.

### A1. Self loopback

Require `libi2pd/Transports.cpp` to prove:

    PostMessages(...)
    ident == local RouterInfo hash
    m_LoopbackHandler.PutNextMessage(...)
    m_LoopbackHandler.Flush()

Row:

    m11-i2pd-self-loopback-source-lock

### A2. TunnelGateway receive-id dispatch

Require `libi2pd/Tunnel.cpp` to prove:

    eI2NPTunnelGateway
    tunnelID = payload
    GetTunnel(tunnelID)
    HandleTunnelGatewayMsg(tunnel, msg)
    tunnel->SendTunnelDataMsg(msg)

and ensure the counted source path contains no sender/build-creator identity comparison between
the TunnelGateway dispatch and `SendTunnelDataMsg`.

Rows:

    m11-i2pd-tunnel-gateway-by-receive-id-source-lock
    m11-i2pd-tunnel-gateway-no-creator-peer-affinity-source-lock

Static checker must reject a future Plan 262 implementation that merely inserts a fake local or
creator `PeerId` to satisfy the old i2pr peer lock.

## 6. Work package B — separate IBGW data-plane state from Participant state

Replace the `TransitGatewayData = TransitParticipantData` alias with a dedicated move-only
IBGW state object.

Minimum IBGW state:

    TransitGatewayData {
        next_fragment_id
        // any other genuinely IBGW-specific bounded state proven necessary
    }

It must **not** contain:

    locked_previous_peer

and must not inherit Participant-only replay/source-affinity semantics accidentally.

Keep `TransitParticipantData` unchanged for Participant TunnelData.

Update Debug output so:
- Participant reports previous-peer/replay state;
- InboundGateway reports only non-secret IBGW-specific bounded state;
- no secret/key material is exposed.

The Plan 260 fragment-id behavior (nonzero seed, per-registration sequence, wrap skips zero,
distinct concurrent ingresses) moves cleanly into `TransitGatewayData`.

Do not add a global counter or lock.

## 7. Work package C — correct the runtime-neutral IBGW gateway API

Refactor `TransitHopRegistration::process_tunnel_gateway` so it no longer accepts or checks a
`previous_peer` argument.

It must:

1. validate expiry;
2. validate role/data-plane variant is InboundGateway;
3. validate the TunnelGateway receive id exactly matches the registry key/canonical receive id;
4. encode/fragment the nested standard I2NP message through the existing Plan 258 path;
5. claim one per-ingress fragment id;
6. apply the IBGW layer exactly once;
7. return bounded next-hop cells.

Important correction: the current implementation comments that it cannot recover the canonical
receive id from the role and therefore effectively accepts any nonzero id after registry lookup.
Fix this ownership ambiguity. The service/registry lookup must pass or bind the actual receive id
so the role can assert exact equality rather than reassigning "expected = actual".

Preferred shapes:

- store `receive_tunnel` in the registration's IBGW data/role state; or
- pass the registry key into the internal processor after lookup.

Do not duplicate the receive id in multiple mutable owners.

Required local rows:

    m11-i2pr-ibgw-exact-receive-id-unit
    m11-i2pr-ibgw-creator-peer-not-data-auth-unit
    m11-i2pr-ibgw-third-party-authenticated-peer-accepted-unit
    m11-i2pr-participant-peer-lock-unchanged-unit

"Third-party peer accepted" means a valid TunnelGateway from a different authenticated router
than the build creator reaches the live IBGW registration; it does not relax transport
authentication generally.

## 8. Work package D — one canonical service seam for network and local gateway ingress

Refactor the daemon service so IBGW processing has a source-neutral internal operation, e.g.:

    route_ibgw_gateway(tunnel_id, nested_standard_i2np, now_ms, rng)

Exact naming is implementation-owned.

### D1. Network path

Canonical decoded network `TunnelGateway` calls the source-neutral service operation after the
normal SSU2/router-I2NP owner has authenticated and decoded the transport message.

The authenticated network peer may remain in **diagnostic evidence**, but is not an IBGW
authorization predicate.

### D2. Local self-loop path

When a decoded OBEP TUNNEL action has:

    action.target_router == local_router_hash

the daemon must:
- take `action.tunnel_id`;
- take the already reconstructed standard I2NP bytes;
- call the same source-neutral IBGW operation directly;
- never create a synthetic `PeerId`;
- never insert the local router into the remote peer index;
- never serialize the message through an artificial SSU2 path.

### D3. Remote OBEP path

When `target_router != local_router_hash`, behavior remains bit-identical:
- resolve the authenticated remote peer;
- wrap TunnelGateway;
- use the bounded router-delivery seam.

### D4. Outcomes

Expose enough typed outcome to distinguish:

    LocalIbgwDelivered { receive_id, next_router, next_tunnel, cells }
    LocalIbgwDropped { reason-class-without-secret }
    RemoteAccepted / existing remote outcome

Do not create a new public API solely for evidence.

## 9. Work package E — local/remote negative guards

Required regressions:

1. self TUNNEL + live matching IBGW registration -> local ingress and emitted cells;
2. self TUNNEL + unknown id -> drop;
3. self TUNNEL + zero id -> drop;
4. self TUNNEL + non-IBGW registration -> drop;
5. self TUNNEL + expired IBGW -> drop;
6. self TUNNEL + cancelled owner -> drop/refuse;
7. self TUNNEL does not add local hash to peer index;
8. self TUNNEL does not invoke RouterDeliveryService;
9. non-self TUNNEL invokes the existing remote peer seam unchanged;
10. self ROUTER action retains existing behavior;
11. self TunnelData-forward target retains existing behavior unless an external stop proves it
    separately necessary.

No silent widening beyond decoded OBEP TUNNEL-to-self.

## 10. Work package F — B-sender receipt requalification

Run the existing receipt diagnostic first with a Plan 262 manifest/evidence shape.

A successful B3 flip requires:

    b-sender-outcome = terminal-garlic-self:0/ingress:>=1/socket:1

and all of:

- B SAM sender established outbound `[i2pr]`;
- B LeaseSet resolution proven;
- send addresses a counted A receiver lease tuple;
- OBEP action target router == self;
- local IBGW ingress occurs on that exact counted receive id;
- ≥2 TunnelData cells emitted for the 1400-byte stimulus;
- zero cell-forward failures;
- committed `next_router == A`;
- committed `next_tunnel` equals A creator-local inbound id;
- A pool-owned local dispatch occurs;
- A receiver SAM socket receives exact payload/digest exactly once;
- six-field tuple validates.

If receipt does not close, stop with the exact new boundary. Do not proceed to the full matrix.

## 11. Work package G — remote third-party gateway regression

Before final external closure, include one controlled data-plane test proving that an IBGW
registration built by A accepts a valid TunnelGateway delivered over an authenticated session
whose router identity is B (or another controlled non-creator reference) when the receive id is
correct.

This test exists to prevent the self-loop fix from masking the inherited source-affinity bug.

Required evidence:

    ibgw-third-party-ingress/build-creator = A
    ibgw-third-party-ingress/data-sender = B
    ibgw-third-party-ingress/receive-id = counted live IBGW id
    ibgw-third-party-ingress/accepted = true

A wrong receive id must still drop.

This may be a deterministic local/service-level regression if the external B-sender topology's
self-loop path cannot independently expose a direct network TunnelGateway sender. Do not build a
second external harness just for this row.

## 12. Work package H — complete M11 qualification matrix

Only after Work package F passes, run the existing receipt-first full matrix.

It must include:

- OBEP unfragmented + fragmented semantic delivery;
- IBGW live receipt and multicell bounded emission;
- Participant far-side forwarding proof;
- exact accepted-registration cardinality;
- typed bandwidth option/reply evidence;
- code-30 rejection with zero state;
- replay suppression;
- logical >600-second expiry and post-expiry drop;
- cancellation all-dimensional drain;
- session-close removal with unrelated peer retained;
- real runtime restart, zero new state, re-established sessions, fresh accepted build;
- fragment-id hardening;
- source locks and all fail-closed environment gates.

Then execute **two complete independent attempts on one implementation SHA**, each with fresh A/B
datadirs, ports, and evidence root. No cross-attempt merge.

## 13. Mesh sustainability policy

Recent controlled meshes are not perfectly stable. Treat that as an environmental/protocol
qualification fact, not a reason to tune until green.

- Receipt-first ordering is required.
- Existing finite timeouts remain.
- No quota enlargement.
- No repeated restart loop.
- No "retry until two passes".
- Each complete attempt is one fresh execution and its result is retained.
- If the self/IBGW ownership corrections are proven and repeated complete attempts fail only
  because the controlled references cannot sustain the matrix, stop and register a narrow
  qualification-sustainability successor with the exact epoch/time/failure boundary.

Do not mix that future environmental corrective into production routing code.

## 14. Compatibility / migration / security

No user migration.

No change to:
- config schema;
- CLI;
- SAM/I2CP public API;
- RouterInfo capabilities;
- router.version;
- public-network behavior;
- default transit-disabled construction.

Security interpretation:

- transport authentication still protects network origin identity at the transport owner;
- IBGW data-plane authorization intentionally does not require that identity to equal the
  tunnel builder;
- tunnel receive id + live role + expiry/resource state select the tunnel, matching the frozen
  reference;
- Participant/OBEP hop provenance remains peer-locked;
- local loopback is allowed only after comparing the decoded OBEP target router with the
  service's own RouterIdentity hash.

No new dependency.

## 15. Stop conditions

Stop and register a new narrow corrective if:

- exact-pinned source contradicts either source-lock premise;
- removing IBGW creator-peer affinity changes Participant/OBEP peer-lock behavior;
- the self arm requires fake peer state, a second decoder, wire change, task/channel/queue, or
  quota/timeout change;
- ROUTER-kind self delivery or TunnelData-forward-to-self proves necessary;
- B3 flips but the receiver path localizes a new crypto/fragment/pool defect outside this
  ownership seam;
- two complete runs reach a repeatable mesh-sustainability boundary after all semantic rows pass;
- public network, false capability advertisement, reference patching, or a new dependency would
  be needed.

Do not weaken or retire the receipt row.

## 16. Required focused tests

At minimum:

1. exact i2pd self-loopback source lock;
2. exact i2pd TunnelGateway receive-id dispatch source lock;
3. static guard: no creator-peer comparison in counted reference gateway dispatch;
4. dedicated `TransitGatewayData` has no previous-peer lock;
5. IBGW exact receive-id match required;
6. IBGW wrong receive id drops;
7. IBGW data sender may differ from build creator;
8. Participant wrong previous peer still drops;
9. self TUNNEL + live IBGW ingresses locally;
10. self TUNNEL unknown id drops;
11. self TUNNEL expired/non-IBGW drops;
12. self TUNNEL cancelled owner refuses;
13. self TUNNEL does not mutate peer index;
14. self TUNNEL does not invoke remote RouterDeliveryService;
15. non-self TUNNEL remote behavior unchanged;
16. self ROUTER behavior unchanged;
17. fragment-id uniqueness/interleaving regressions remain green;
18. B sender establishes `[i2pr]`;
19. B LeaseSet addresses counted tuple;
20. counted self action becomes exact local IBGW ingress;
21. 1400-byte stimulus emits ≥2 cells with zero failures;
22. receiver socket receives exact payload once;
23. six-field tuple validator passes only complete binding;
24. third-party IBGW sender semantics regression;
25. legacy Participant/OBEP/IBGW + replay/expiry/cancel/close/restart rows;
26. two-attempt checker rejects one attempt;
27. two-attempt checker rejects mixed SHAs/cross-attempt merge;
28. diagnostic-only epoch cannot satisfy counted closure;
29. missing B SAM/pin/cache env fails before network startup;
30. ordinary product transit remains disabled.

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

Run every new Plan 262 focused test exactly by name.

After a passing receipt diagnostic, run two complete fresh-datadir external attempts on the same
implementation SHA. Closure also requires exact-head ordinary CI green on:

- Quality (ubuntu-latest);
- Quality (macos-latest);
- MSRV (Ubuntu);
- Dependency policy.

## 18. Acceptance criteria

Plan 262 closes only when all are directly evidenced:

1. Plan 260/261 retained evidence remains traceable.
2. Plan 261 B3 signature remains the starting boundary.
3. Exact-pinned self-loopback source lock holds.
4. Exact-pinned TunnelGateway-by-receive-id source lock holds.
5. Normative IBGW role semantics and exact-pinned reference gateway dispatch both establish
   receive-id routing without build-creator sender affinity.
6. `TransitGatewayData` no longer inherits participant previous-peer locking.
7. Build creator provenance remains retained for build admission/accounting/reply routing.
8. IBGW gateway processing asserts exact receive-id equality.
9. Valid third-party data sender can inject into a live IBGW registration.
10. Participant previous-peer lock remains unchanged.
11. Self-target OBEP TUNNEL enters local IBGW without synthetic peer state.
12. Unknown/zero/non-IBGW/expired/cancelled local target fails closed.
13. Non-self OBEP TUNNEL remote delivery is unchanged.
14. B SAM + B outbound `[i2pr]` establishment passes.
15. B LeaseSet resolution/addressing names counted A tuple.
16. Counted self-target action produces genuine IBGW ingress.
17. 1400-byte counted payload emits ≥2 TunnelData cells with zero failures.
18. Committed next tuple matches A creator-local inbound tunnel.
19. Pool-owned LOCAL dispatch occurs.
20. Receiver SAM socket receives exact payload/digest exactly once.
21. Full six-field tuple is bound.
22. Cardinality/bandwidth/code-30 rows pass.
23. Participant far-side, replay, expiry, cancellation, session-close, restart rows pass.
24. Fragment-id hardening stays green.
25. Complete external attempt 1 passes every mandatory row.
26. Complete external attempt 2 passes every mandatory row on the same SHA with fresh datadirs.
27. Full workspace verification passes.
28. Exact-head ordinary CI passes all four jobs.
29. README/registry/roadmap/support/conformance/dossier agree on Plan 262 as the sole
    dependency-ready M11 closure authority.
30. No product default/capability/version/public-network change.
31. No critical/high finding remains open.

Only then may the unblock audit mark the ADR 0026 one-family M11 experimental qualification
passed and consider M12 planning ready.

## 19. Closure evidence required

Write `plans/closure/transit-tunnels/262-status.md` with:

- implementation and closure SHAs;
- exact reference pin/version;
- both source-lock proofs;
- IBGW state-model change and peer-affinity rationale;
- exact receive-id binding proof;
- self-loop condition and no-synthetic-peer proof;
- third-party gateway-sender regression;
- B-sender build/LeaseSet/OBEP evidence;
- receiver IBGW ingress, multicell, local-tunnel/pool binding, exact socket receipt;
- flipped `b-sender-outcome`;
- complete carried-forward lifecycle matrix;
- both complete same-SHA manifests;
- mesh-sustainability timings/failures;
- full local verification;
- exact-head CI run;
- security/resource/concurrency/migration reviews;
- findings by severity;
- unblock audit.

If a new boundary appears, close retained/blocked and register only the narrow successor exposed
by the evidence.

## 20. Handoff order

1. add both exact-pinned source locks;
2. split IBGW state from Participant state;
3. correct exact receive-id ownership and remove IBGW creator-peer data affinity;
4. add third-party sender + Participant-lock regressions;
5. introduce the source-neutral IBGW service seam;
6. add the OBEP TUNNEL-to-self local branch without synthetic peer state;
7. prove non-self and negative self cases unchanged/fail-closed;
8. update checker/manifest authority to Plan 262;
9. run the B-sender receipt diagnostic once;
10. if receipt passes, run all remaining lifecycle rows receipt-first;
11. run the complete local verification floor;
12. run two complete fresh-datadir same-SHA external attempts;
13. obtain exact-head ordinary CI green;
14. close only if all 31 acceptance criteria are directly evidenced.

Do not create Plan 263 pre-emptively. Under the repository planning process, the next plan is
registered only if Plan 262 execution exposes a new bounded defect or qualification boundary.
