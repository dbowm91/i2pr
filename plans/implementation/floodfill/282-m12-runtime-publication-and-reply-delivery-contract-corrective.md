# Plan 282 — M12 runtime publication and reply-delivery contract corrective

Status at registration:
**registered-m12-runtime-publication-delivery-corrective-ready**

Classification: narrow corrective / daemon-runtime integration / protocol route correctness.

Predecessor evidence:
- Plan 276 passed the persistence, maintenance, and resource-governance foundation.
- Plan 277 is retained as a stopped partial implementation at
  `stopped-m12-daemon-runtime-publication-and-reply-adapter-contract-required`.
- Plan 277's partial coordinator, role controller, bounded effect queue, controlled
  `FloodfillAdvertisementPermit`, and authenticated I2NP dispatch are retained.
- Plans 278 and 279 remain blocked.

## 1. Objective

Complete the local M12 daemon path without violating the repository dependency direction.

This corrective must provide the runtime-owned SSU2 publication and delivery contract that Plan
277 could not safely obtain by depending directly on `i2pr-transport-ssu2`. It must then wire the
existing floodfill coordinator effects into the existing daemon/runtime transport and tunnel
delivery seams, correct two route-semantics defects discovered during the Plan 277 source review,
and demonstrate the complete controlled local lifecycle needed by Plan 278.

A successful Plan 282 is the completion authority for the stopped Plan 277 scope. Do not retry or
rewrite Plan 277 as though its original execution passed.

## 2. Research findings that define this corrective

### 2.1 Runtime is the correct SSU2 publication owner

`i2pr-runtime` already depends on `i2pr-transport-ssu2` and owns the live SSU2 sockets,
bound addresses, transport static/intro material, reachability tracker, active sessions, and
current SessionConfirmed RouterInfo bytes. `i2pr-daemon` intentionally does not depend on the
SSU2 implementation crate.

Therefore the publication bridge belongs in `i2pr-runtime`: runtime should consume the
transport implementation's deterministic publication snapshot internally and expose a narrow
transport-neutral factual view to the daemon.

The daemon must not add `i2pr-transport-ssu2` to its dependencies.

### 2.2 Existing direct delivery should be reused

`i2pr-daemon::router_i2np::RouterDeliveryService` already wraps
`Ssu2RuntimeService::send_i2np` and therefore preserves TransportManager admission, per-link
message/byte bounds, deadlines, and current-session ownership.

Plan 282 must reuse this path for established direct replies/floods rather than adding a second
SSU2 queue or raw UDP send path.

### 2.3 Existing tunnel machinery supplies the opposite direction

The daemon already has canonical outbound-tunnel cell composition and RouterDeliveryService
dispatch. Floodfill lookup replies to a requester-supplied inbound tunnel are not local outbound
tunnel traffic. They are sent to the request's reply gateway as a `TunnelGateway` I2NP message
naming the requested tunnel id.

For supplied-key ECIES replies, the Plan 274 Existing Session bytes are the payload of an I2NP
`Garlic` message; that Garlic message is then nested inside `TunnelGateway` and delivered
directly to the reply gateway. Do not run the reply through `OutboundGatewayRole`.

### 2.4 DatabaseLookup.from is a reply route, not authenticated-source identity

The current I2NP specification defines `DatabaseLookup.from` as:

- the router to which a direct reply is sent when deliveryFlag == 0; or
- the inbound-tunnel gateway when deliveryFlag == 1.

It is not required to equal the authenticated transport peer that delivered the lookup. Java I2P
likewise replies to `message.getFrom()` / `message.getReplyTunnel()` independently of the
immediate sender.

The partial Plan 277 check:

~~~text
lookup.from == authenticated_peer
~~~

is therefore invalid for normal tunnel-originated lookup traffic and must be removed. The
authenticated peer remains provenance/throttle evidence only; `from` remains an untrusted
request-supplied reply destination validated by route policy.

Normative source:
https://i2p.net/en/docs/specs/i2np/

Pinned reference:
`i2p/i2p.i2p@011ab635dac22fe0e63288b933e180f2d0f2f18e`,
`router/java/src/net/i2p/router/networkdb/HandleDatabaseLookupMessageJob.java`.

### 2.5 DatabaseStore acknowledgement currently carries the wrong message id at the daemon boundary

The I2NP DatabaseStore contract says a nonzero reply token requests a DeliveryStatus whose body
message id is the **reply token**. The current `FloodfillAck` separately retains the token but
constructs its `DeliveryStatusMessage` from the inbound I2NP envelope message id.

Correct this so:

~~~text
DeliveryStatus.msg_id == DatabaseStore.reply_token
~~~

and route the acknowledgement using `reply_gateway` / `reply_tunnel_id`, not the immediate
authenticated peer/link. Missing or invalid reply route fields fail closed. The immediate peer
continues to supply provenance and flood-fanout exclusion only.

Normative source:
https://i2p.net/en/docs/specs/i2np/#msg-DatabaseStore

## 3. Required ownership model

The final controlled path should be:

~~~text
i2pr-transport-ssu2
    protocol/address/publication implementation
             |
             v
i2pr-runtime
    live sockets + reachability + bound endpoints + current local RI
    runtime-owned publication material / local-RI update authority
             |
             v
i2pr-daemon
    FloodfillCoordinator
    LocalRouterInfoBuilder
    RouterDeliveryService
    reply/flood effect adapter
    persistence + maintenance lifecycle
             |
       +-----+-------------------+
       |                         |
       v                         v
direct SSU2                reply TunnelGateway
RouterDeliveryService      -> gateway -> requested tunnel
~~~

No NetDB/tunnel/client crate receives a global runtime context.

## 4. Hard invariants

1. `i2pr-daemon` must not depend directly on `i2pr-transport-ssu2`.
2. `i2pr-netdb` remains socket/Tokio/runtime-neutral.
3. Normal daemon configuration still cannot construct the controlled floodfill permit.
4. `caps=f` remains unavailable outside the controlled qualification path.
5. A direct flood action can never fall back to a tunnel route.
6. `DatabaseLookup.from` is never treated as proof of authenticated sender identity.
7. Direct/tunnel reply routing is derived from the I2NP request fields and validated independently
   from authenticated-source provenance.
8. DatabaseStore acknowledgement DeliveryStatus body id equals the reply token exactly.
9. A requested supplied-key ECIES tunnel reply is never downgraded to plaintext and is wrapped
   exactly once as Garlic before TunnelGateway delivery.
10. Runtime publication material is derived from the actual bound SSU2 owner and current
    reachability evidence; no config placeholder or fabricated endpoint may become advertised
    material.
11. Floodfill role activation cannot precede a qualified publishable SSU2 address and successful
    installation of the matching signed local RouterInfo into the runtime.
12. Health loss stops new floodfill admission immediately and requires a non-`f` signed local
    RouterInfo to replace the active floodfill RouterInfo before drain completes.
13. Local RouterInfo replacement preserves the RouterIdentity, SSU2 static-key/intro-key binding,
    network id, size/freshness bounds, and signature validation.
14. No task per packet or unbounded task per effect; all dial/effect concurrency is explicitly
    bounded and supervised.
15. Type 5 remains deferred under Plan 281. Plan 282 must not reopen Red25519/provider work.

## 5. Work package A — correct NetDB reply-route semantics

### A1. Store acknowledgement correctness

Change `FloodfillAck` construction so its DeliveryStatus body carries
`message.reply_token`, not the inbound envelope id.

Either remove the duplicate `reply_token` field after proving it is redundant, or lock an
invariant test that `ack.reply_token == ack.message.message_id`.

Define one typed store-ack reply intent from:

~~~text
reply_token > 0
reply_gateway
reply_tunnel_id
DeliveryStatus(reply_token, now)
~~~

Rules:

- `reply_tunnel_id == 0/None` -> direct reply to `reply_gateway`;
- nonzero tunnel id -> TunnelGateway delivery to `reply_gateway`;
- absent/malformed gateway or inconsistent route -> typed no-response/failure;
- never route the ack merely to the immediate transport peer because it happens to be available.

### A2. Lookup route correction

Remove the Plan 277 `lookup.from == peer.hash()` requirement.

Keep authenticated peer/link identity for:
- provenance;
- request-rate accounting;
- diagnostic categorical outcomes;
- direct-fanout exclusion where relevant.

Use `lookup.from` only as the request's reply router/gateway after validating the delivery flag
and tunnel-id combination.

Add a mandatory regression where:

~~~text
authenticated OBEP peer != lookup.from reply gateway
~~~

and both direct and tunnel reply forms route correctly.

### A3. Route abuse bounds

Do not create an unbounded arbitrary-dial primitive. Every direct reply target must resolve through
the bounded validated main-router NetDB and the controlled SSU2 address policy unless an already
authenticated active session to that exact target exists.

Unknown/unvalidated reply targets produce a typed failure. No clearnet/LAN target material may be
derived from untrusted message fields.

## 6. Work package B — runtime-owned SSU2 publication material

Add a narrow runtime API whose exact name may vary, equivalent to:

~~~text
Ssu2PublicationMaterial {
    address: RouterAddress,
    evidence_expires_at,
    reachability,
    bound_family,
}
~~~

The value must be constructed inside `i2pr-runtime` from:
- the runtime's actual bound nonzero socket;
- current `ReachabilityTracker` state/evidence;
- runtime-owned SSU2 static public key;
- runtime-owned intro key;
- runtime MTU/capabilities;
- the existing `i2pr-transport-ssu2::publication` builder.

The daemon receives only the validated public `RouterAddress` and categorical/expiry metadata.
Private static key bytes never cross the bridge.

Required negative cases:
- no bound socket;
- port zero after bind should be impossible and test-locked;
- expired reachability evidence;
- unknown/unreachable state when direct publication is required;
- invalid MTU/key/options;
- snapshot address differs from the actual bound endpoint;
- runtime shutdown.

The controlled M12 path may publish loopback endpoints only inside its isolated qualification
profile. This plan does not enable non-loopback/public SSU2 advertisement.

## 7. Work package C — live local RouterInfo rotation

The SSU2 runtime currently retains one immutable `local_router_info` byte vector from
construction. That cannot satisfy health-based `f` withdrawal.

Add a bounded runtime-owned local-RouterInfo update API.

The update must:
1. decode and validate the supplied RouterInfo;
2. require the same local RouterIdentity/hash;
3. require a compatible SSU2 address whose static/intro binding matches the runtime owner;
4. enforce size/freshness/network-id/router.version policy from ADR 0027;
5. atomically replace the bytes future SessionConfirmed handshakes use;
6. expose only generation/count metadata in diagnostics.

No active session is silently re-authenticated or mutated by a RouterInfo rotation.

Daemon activation sequence:

~~~text
runtime publication material ready
-> eligibility ReadyToActivate
-> begin activation
-> build signed RI with qualified SSU2 address + controlled caps=f
-> runtime validates/installs RI
-> register/publish the same signed bytes through existing NetDB publication authority
-> complete activation / accept floodfill work
~~~

Withdrawal sequence:

~~~text
eligibility lost
-> stop new floodfill admission
-> build a new signed RI with the same qualified SSU2 address but without caps=f
-> runtime validates/installs replacement
-> register/publish replacement through existing NetDB publication authority
-> bounded effect drain
-> Disabled
~~~

If preserving the SSU2 address while removing `f` requires a new
`LocalRouterInfoBuilder` entry point, add a narrowly named controlled SSU2-address builder. The
ordinary `build()` path must continue to forbid arbitrary transport addresses/caps. Production
call sites must source the address only from Work Package B.

Strengthen the existing weak style-only floodfill-address check in production composition: a
caller-created `RouterAddress { style = "SSU2" }` with missing/invalid SSU2 options must not
qualify merely because its style string matches.

## 8. Work package D — encode and drain daemon effects

Add one daemon-owned floodfill delivery adapter. It consumes
`FloodfillDaemonEffect` and returns typed bounded outcomes.

### D1. Direct lookup reply

For `FloodfillReplyIntent::Direct { peer, body }`:

- build a canonical I2NP message with a fresh bounded message id/expiration;
- deliver through `RouterDeliveryService` when an authenticated session exists;
- if no active session exists, resolve a validated SSU2 RouterAddress from `ServerNetDb`, create
  the existing controlled `Ssu2DialTarget`, perform a bounded supervised dial, then deliver;
- no tunnel fallback.

### D2. Tunnel lookup reply

For `FloodfillReplyIntent::Tunnel`:

- if `ReplyProtection::SuppliedKeyEcies`, treat the Plan 274 payload as exactly one Existing
  Session wire payload;
- wrap it as exactly one I2NP `Garlic` body;
- wrap that complete I2NP message in `TunnelGateway(tunnel_id)`;
- deliver the TunnelGateway message directly to the requested gateway through the same bounded
  direct delivery/dial policy.

Do not call `OutboundGatewayRole`; this is injection into the requester's inbound tunnel, not
traffic originating through a local outbound tunnel.

### D3. DatabaseStore acknowledgement

Use the Work Package A typed route. Direct and TunnelGateway variants use the same bounded delivery
adapter. The DeliveryStatus body id must remain the original DatabaseStore reply token.

### D4. Direct replication

For `DirectFloodAction`:

- resolve only the action's peer;
- use an existing authenticated session or bounded SSU2 dial derived from that peer's validated
  RouterInfo;
- send its zero-token DatabaseStore directly;
- never use a tunnel, even when direct delivery fails;
- no more than the Plan 275 fanout/candidate/dial ceilings.

### D5. Concurrency and backpressure

No detached task per effect. Use a fixed supervised worker set or one bounded owner with an
explicit maximum number of in-flight dials/effects. The coordinator's existing effect count/byte
leases remain authoritative; an effect is released exactly once on success, typed failure,
cancellation, or shutdown.

## 9. Work package E — daemon lifecycle, persistence, and maintenance completion

Compose the retained Plan 277 coordinator into the actual SSU2 service owner.

Startup order:
1. persistent identity;
2. runtime service construction/bind;
3. Plan 276 floodfill record load and mandatory revalidation;
4. coordinator creation;
5. maintenance/resource readiness;
6. runtime publication-material readiness;
7. controlled eligibility evaluation;
8. local RouterInfo install;
9. role activation.

Run bounded maintenance ticks under one owner. Persist only Plan 276-authorized records.

Shutdown/health-loss order must stop admission, rotate/withdraw `f`, cancel/detach pending dials,
drain bounded queued effects to the existing shutdown deadline, persist allowed state, release all
leases, and join supervised tasks.

A forced shutdown may skip remote publication completion but restart must begin without an active
floodfill permit and must not reuse stale `f` authority.

## 10. Work package F — local end-to-end acceptance

Plan 282 must prove the complete local path before Plan 278 is unblocked.

Required rows:

1. **Publication material** — actual bound SSU2 endpoint and runtime static/intro keys produce the
   exact RouterAddress placed in the signed local RouterInfo.
2. **Activation** — controlled eligibility installs an RI containing the qualified SSU2 address
   and `caps=f`; ordinary/default daemon remains unable to do so.
3. **Lookup direct route** — authenticated delivery peer differs from `lookup.from`; the reply
   reaches `lookup.from`, not the immediate peer.
4. **Lookup tunnel route** — ECIES Existing Session payload -> one Garlic -> one TunnelGateway ->
   requested gateway/tunnel; decrypts to the expected DSM/DSRM.
5. **Store direct ack** — DeliveryStatus body id equals the nonzero reply token and is sent to the
   requested reply gateway.
6. **Store tunnel ack** — DeliveryStatus is nested in TunnelGateway with the exact requested
   tunnel id; no immediate-peer shortcut.
7. **Replication established** — zero-token direct flood reaches an established target.
8. **Replication dial** — no-active-session target is resolved from validated RouterInfo, dialed
   under deadline/concurrency bounds, and receives the zero-token store.
9. **No tunnel fallback** — failed direct flood terminates with typed failure.
10. **Persistence restart** — allowed records revalidate before role eligibility; restored
    provenance remains conservative.
11. **Health withdrawal** — loss of headroom/reachability/supervision removes `f`, prevents new
    floodfill admission, drains bounded effects, and reaches Disabled.
12. **Resource baseline** — queued-effect bytes/count, direct dial permits, and coordinator
    resources return to baseline after cancel/shutdown.
13. **RouterInfo generation** — future SSU2 handshakes use the latest installed RI; pre-existing
    sessions are not silently rewritten.
14. **Default profile** — no controlled permit, no transport address publication, no `caps=f`.

Prefer real loopback i2pr↔i2pr runtime tests using existing SSU2 test infrastructure. Do not build
a new external harness for this plan.

## 11. Scope

In scope:
- the route correctness fixes above;
- narrow `i2pr-runtime` SSU2 publication/local-RI update surface;
- daemon floodfill effect adapter;
- bounded direct dial composition;
- TunnelGateway reply injection;
- Plan 276 persistence/maintenance ownership;
- local lifecycle tests and static boundary guards;
- planning/support reconciliation.

Out of scope:
- exact-pinned i2pd execution (Plan 278);
- Java-family execution (Plan 279);
- normal user floodfill config;
- public/non-loopback floodfill participation;
- type-5/Red25519;
- new peer-scoring system;
- new transport;
- changes to M11 transit behavior.

## 12. Failure, cancellation, restart, and contention semantics

All delivery outcomes are typed: accepted, no active/validated route, dial rejected, deadline,
queue full, resource denied, too large, cancelled, or runtime closed.

A direct-dial attempt owns one bounded permit and one deadline. Cancellation releases both the
runtime dial state and coordinator effect lease. Duplicate target effects may share an existing
session but must not create unbounded concurrent dials.

A RouterInfo update is atomic: either the validated new signed bytes become current or the
previous bytes remain current. A failed attempted floodfill activation must leave the runtime on
the previous non-`f` RouterInfo and keep the role non-Active.

## 13. Compatibility and migration

No normal config migration. No wire-format change. Persistence remains Plan 276 format v1 unless
implementation proves a version bump is required; if so, stop and document the migration rather
than silently changing it.

This plan may change internal daemon/runtime APIs and the exact Plan 277 partial effect types.

## 14. Static guards

Extend `scripts/check-m12-floodfill-boundaries.sh` to enforce at minimum:

- daemon Cargo.toml contains no direct `i2pr-transport-ssu2` dependency;
- NetDB remains runtime-neutral;
- direct flood type contains no tunnel route;
- normal config cannot construct `FloodfillAdvertisementPermit`;
- production `build_floodfill` address source is the runtime publication bridge;
- lookup handler contains no `lookup.from == peer.hash()` identity assertion;
- store ack body id is sourced from the reply token;
- Tunnel reply adapter contains the Garlic -> TunnelGateway shape;
- type 5 stays denied/deferred.

## 15. Required verification

Focused first:

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked -p i2pr-netdb -p i2pr-runtime -p i2pr-daemon --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
git diff --check
~~~

Then the repository floor:

~~~bash
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo test --locked --doc
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo doc --locked --workspace --no-deps
~~~

If the full floor stalls, retain the exact test/process boundary and investigate it; do not claim
Plan 282 passed from focused tests only.

Ordinary exact-head CI must be green before closure.

## 16. Documentation and support updates

On implementation/closure update:
- `plans/closure/floodfill/282-status.md`;
- `plans/subsystems/floodfill-roadmap.md`;
- `plans/registry.md`;
- `docs/architecture/i2pr-netdb.md`;
- SSU2 runtime/daemon architecture docs touched by the new bridge;
- `specs/protocols/02-i2np.md` for corrected ack/reply routing if its local dossier is stale;
- `specs/support.toml`;
- README current-work line.

Do not state external floodfill interoperability until Plan 278 passes.

## 17. Acceptance criteria

Plan 282 closes only when:

1. the daemon has no forbidden direct dependency on `i2pr-transport-ssu2`;
2. runtime exposes factual bounded publication material from the live SSU2 owner;
3. local RouterInfo can be atomically rotated between controlled non-`f` and `f` forms while
   preserving identity/address binding;
4. future SSU2 handshakes use the updated signed RouterInfo;
5. DatabaseStore DeliveryStatus body id equals the reply token;
6. DatabaseStore acknowledgements follow reply gateway/tunnel fields;
7. DatabaseLookup replies do not require `from == authenticated peer`;
8. direct lookup replies target `lookup.from`;
9. supplied-key tunnel replies are exactly one Existing Session payload inside one Garlic inside
   one TunnelGateway and are successfully opened by the requester fixture;
10. direct replication uses existing session or bounded validated-RI dial only and never tunnels;
11. persistence/restart, maintenance, cancellation, drain, and health withdrawal are demonstrated;
12. normal/default daemon remains floodfill-disabled and cannot advertise `f`;
13. the complete local acceptance matrix in §10 passes;
14. the full workspace verification floor and exact-head ordinary CI are green;
15. no critical/high finding remains open.

## 18. Stop conditions

Stop and register a narrower architecture corrective if:
- publication requires daemon -> transport-ssu2 dependency;
- a local RouterInfo cannot be safely rotated without invalidating live SSU2 identity semantics;
- the requested TunnelGateway reply requires a missing protocol primitive beyond existing Garlic /
  TunnelGateway framing;
- bounded direct dial requires a new global connection manager;
- the existing runtime cannot expose factual reachability/address evidence without fabricating
  publication state;
- local acceptance reveals a Plan 273/274/275 semantic defect beyond the route corrections
  explicitly scoped here.

Do not compensate by broadening network access, patching a reference router, increasing unbounded
budgets, or weakening route/authentication checks.

## 19. Closure and unblock audit

Plan 277 remains historical stopped evidence. A passing Plan 282 records that the missing Plan 277
runtime integration has been corrected and becomes the M12 local daemon-composition authority.

On pass:
- mark Plan 282 passed;
- retain Plan 277 as stopped/corrected-via-282 rather than rewriting it;
- move Plan 278 to ready;
- keep Plan 279 blocked on Plan 278;
- keep Plan 280 stopped and type 5 deferred under Plan 281;
- keep normal/public `caps=f` unclaimed.

## 20. Handoff

Plan 278 then exercises exactly this completed controlled daemon path against unmodified
exact-pinned i2pd. No additional local architecture tranche should be inserted unless Plan 282
closure evidence identifies a concrete remaining defect.
