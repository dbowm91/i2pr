# Plan 342 — I2P-routed outproxy provider and canonical proxy field completion

Status: **passed-loopback-wire-lane-landed-live-failover-rotation-unproven**

Closure record: [`342-status.md`](../../closure/i2pcontrol-proposal-170/342-status.md).

**Superseded token, retained:** `in-progress-option-surface-and-request-paths-landed-awaits-self-composed-wire-lane`
(commit `5c3263b`, 2026-10-05). It is kept here rather than deleted because it
described the request-path integration as *landed* when it was in fact
unreachable, and the repo rule is that a predecessor's record is superseded and
never rewritten. The `5c3263b` commit remains in history for the same reason.

Progress (2026-10-05). Recorded in the order the work happened, including that
token turning out to describe something broken.

1. **Steps 1-3, landed as `e9ffe40` and `cf13e1b`.** Step 1 closed the two
   structural gaps this plan named: `RouterIdentityBundle` gained a
   closure-based signing-seed accessor, and the outbound secret store is derived
   once at the composition root and threaded as a single `Arc`. `OutproxyRoute`
   gained `Refused(OutproxyFailure)`, because the enum previously had no way to
   say "no" and an empty proxy list made it claim `DirectI2p` for a clearnet
   target. Steps 2 and 3 landed **together**, deliberately: the plan's own
   sequencing rule -- *"nothing in the option surface is accepted until the
   route behind it exists"* -- means splitting them would have put an
   egress-looking surface on a tree that could not carry a request. All seven
   canonical fields were admitted as one all-or-none block with
   `OutproxyPassword` sealed at normalize time; one `classify_client_target`
   made the Direct / ViaOutproxy / Refused decision; the handshake prefix was
   carried into the pump's **inbound** direction.
2. **That token was wrong, and writing the wire lane is what proved it.** Every
   request-target grammar in the tree hard-required a `.i2p` suffix and refused
   a clearnet authority **before** `classify_client_target` was reached:
   `CONNECT example.com:443` was answered 403 by the parser, and a SOCKS5
   clearnet target was answered `HostUnreachable` from inside the negotiator.
   So `ClientTargetClass::ViaOutproxy` was dead in production while its unit
   rows -- which call the classifier directly -- passed. This plan's own
   inert-acceptance failure, one level deeper than it anticipated: not an option
   surface with no route behind it, but a route with no reachable input. Fixed
   by `TargetPolicy`, whose default is the strict value and whose relaxed value
   a caller must name, plus `ServiceTunnelManager::target_policy` reading the
   provider registry so the parser and the classifier cannot disagree.
3. **Step 4 landed, and it settled the open scoping question.** `handle_proxy_request`
   **is** in step 3's scope: the block is admitted on `httpclient`, so a forward
   path that ignored it would be inert acceptance at sub-path granularity. After
   `build_attempt` a session is a byte pipe to the origin, not a forward proxy,
   so it carries the clearnet authority in `Host:` and never the `b32.i2p`
   substitution. Running the lane also found two further production defects: the
   outproxy opener never kicked the delivery driver, so every route would have
   failed with `TargetUnreachable`; and a control commit drops manager-only
   specs, so an outproxy endpoint must be startup-owned.

   **Evidence:** `crates/i2pr-daemon/tests/outproxy_loopback_wire.rs`, 8/8 rows.
   The outproxy is reached through Streaming exactly as production reaches one,
   and the fixture never resolves a name, so the test process holds no clearnet
   capability. Guards: `check-outproxy-request-path.sh` extended 24 -> 39/39
   mutations; new `check-outproxy-wire-lane-evidence.sh` 7/7 with 2/2 controls.

   **Plan 327 stays blocked.** The live failover rotation between two configured
   outproxies, and a live restart carrying a request, are unproven. Two rows were
   written for them and removed rather than left ungreen, with their names
   machine-checked in `DOCUMENTED_ABSENCES`. No interoperability evidence; the
   Java 2.13.0 and i2pd 2.61.0 pins are untouched.

Full routine floor 39/39 PASS and 4190 workspace tests green, all local.

Progress (2026-10-05, commit `cf13e1b`): the ordered work below is recorded in
reverse order because steps 2 and 3 landed together, deliberately. The plan's
own sequencing rule -- *"Nothing in the option surface is accepted until the
route behind it exists"* -- means splitting them across commits would put an
egress-looking option surface on a tree that could not carry a request.

1. **Step 1, landed as `e9ffe40`.** `RouterIdentityBundle::with_signing_seed`
   (a closure, not a getter) and `RouterBoundOutboundSecrets::from_router_identity`;
   `build_outbound_secret_store` derives one store at the composition root and
   threads a single `Arc`; `OutproxyRoute::Refused(OutproxyFailure)` added so
   the enum can say "no" instead of claiming `DirectI2p` for a clearnet target.
2. **Steps 2 and 3, landed as `cf13e1b`.** All seven canonical fields admitted
   as one all-or-none block with `OutproxyPassword` sealed at normalize time;
   `classify_client_target` made the single Direct / ViaOutproxy / Refused
   decision for the HTTP, CONNECT, and SOCKS5 request paths, each matching it
   exhaustively before opening anything; the handshake prefix carried into the
   pump's **inbound** direction. Pinned by `scripts/check-outproxy-request-path.sh`
   (24/24 mutations) and by classifier unit rows.
3. **Step 4, NOT DONE.** The self-composed loopback outproxy wire lane, its
   evidence checker, and this plan's closure record remain. No live route has
   been exercised end to end, so the loopback evidence the plan asks for does
   not exist and no capability is claimed. Plan 327 stays blocked.

Full routine floor 39/39 PASS and 4184 tests green at `cf13e1b`, all local.

Classification: capability + security boundary (completes Plan 327).

Hard dependencies: Plan 343 passed (the provider policy and route owner);
Plan 341 passed (the outbound secret owner); Plan 323 closed; Plan 327 closed
blocked.

Subsystem: `i2pcontrol-proposal-170`.

## Design decisions settled by the Plan 343/344 groundwork (2026-10-05)

These were worked out against the real code, not assumed. Recording them here
so the next pass starts from decisions rather than re-deriving them, and
because two of them rule out the obvious implementation order.

### The secret store has to reach the request path, and that is the gate

`RouterBoundOutboundSecrets` (Plan 341) is currently constructed nowhere. Two
structural problems stand between it and a real route:

- `normalize_definition_with_filter_root(name, tunnel_type, options,
  start_on_load, filter_root)` is a free function with no store parameter, and
  every caller would need one.
- The store is keyed by the router's persisted signing seed, which lives inside
  `RouterIdentityBundle`. **`RouterIdentityBundle` exposes no accessor for it
  today.** It needs a *closure-based* one — `with_signing_seed(|seed| ...)` —
  not a getter, because the seed is the root of every router identity
  credential and a returned value can be copied, printed, or stored. The
  closure bounds the exposure to a single expression, which is enough to
  derive a domain-separated key and nothing more.

So the store must be derived once at the composition root and threaded as
`Arc<dyn OutboundSecretStore>` into both the definition builder (to seal
`OutproxyPassword` into its stored form) and the per-tunnel runtime (to open it
when a request is actually going upstream). **This is the first thing to
build**, because everything else depends on it.

### The policy goes on the options value; the credential does not

`HttpClientOptions`, `ConnectClientOptions`, and `Socks5ClientOptions` each gain
`outproxy: Option<OutproxyConfig>` — the route policy only. The credential
stays in Plan 341's owner and is opened only while a header is being built, so
an options value remains safe to `Clone` into a snapshot and to `Debug`. The
sealed stored form is scrubbed into the definition's options map exactly the
way Plan 292 scrubs `proxy_password`, so it round-trips through an edit
untouched.

### The route decision is a pure function

The load-bearing property — a clearnet target has exactly **one** route, and
that route is an outproxy — is decidable without a socket, so it must be
decidable without one. `classify_connect_target` returns an enum
(`Direct` | `ViaOutproxy` | `Refused`) and the order is the guarantee:
`.i2p` authority first and no provider consulted, then the outproxy grammar,
then a refusal. There is no fourth branch, and no branch that opens a clearnet
socket. This is the row to write first, because it is what makes "no direct
fallback" checkable rather than asserted.

### The pump hand-off has a concrete requirement

Plan 343's `open_via_outproxy` returns a `connection_id` **and** a
`tunnel_prefix`: bytes a proxy pushed in the same burst as its `CONNECT` 200.
`run_stream_pump` and `ServicePumpEndpoint` must be able to adopt that
connection **and prepend the prefix to the pump's first read**, or the first
request on any outproxy that pipelines is corrupted. Adopting the connection
alone is not enough.

### All seven fields or none — a subset is the inert-acceptance trap

The obvious sequencing is "accept the three non-secret route fields now, the
four credential fields later". **That must not be done.** Accepting
`ProxyList` / `UseOutproxyPlugin` / `OutproxyType` alone produces a tunnel that
looks egress-capable and is not: the options parse, the policy validates, the
policy is stored — and nothing ever opens a route. That is precisely the failure
Plan 344 found in the LeaseSet mode table, where
`encrypted with per-user key (psk)` passed on parser acceptance alone, and it
is what Plan 326 means by "no mode may pass from parser acceptance or inert
storage". A control client that sets `ProxyList` and gets an accepted response
would reasonably conclude it has egress.

So the ordering is forced: **store plumbing first, then all seven fields, then
the request path, then the wire lane.** Nothing in the option surface is
accepted until the route behind it exists.

## Classify the work

Give Plan 170's outproxy-related TunnelManager fields real semantics, on top of
the secret owner Plan 341 landed. This is Plan 327's remaining scope.

**Partly implemented; the rest is not.** Plan 343 delivered the provider itself
in both halves — the runtime-neutral policy layer
(`crates/i2pr-service-tunnels/src/outproxy.rs`) and the daemon route owner
(`crates/i2pr-daemon/src/outproxy_route.rs`), plus static guards on the
no-direct-clearnet invariant. See
[`343-status.md`](../../closure/i2pcontrol-proposal-170/343-status.md).

Still outstanding, and still the point of this plan:

- the seven canonical option fields and their transactional reconfiguration;
- the HTTP and CONNECT request-path integration, and the Proposal-applicable
  SOCKS families;
- the self-composed loopback outproxy wire lane and its evidence checker.

**The provider is not reachable from any request path**, so this document
remains a plan of record rather than a claim, and Plan 327 remains blocked.

An implementation attempt on 2026-10-05 was **started and reverted in full**,
with nothing landed. It reached the request-path hand-off and found that the
credential store is not yet reachable from there, and that the pump cannot yet
adopt an already-negotiated connection. Rather than land an option surface that
would parse and store a policy with no route behind it, the attempt was reverted
and the five design decisions above were recorded. The tree is unchanged by it.

## Provider architecture

A small typed outproxy-provider interface owned by the service-tunnel layer, with
one shipped implementation: a **static, configured, I2P-routed provider**.

- Selects from configured I2P outproxy destinations, validated as I2P names or
  destinations. A `ProxyList` entry is never a clearnet host.
- Opens every route through the existing Destination/Streaming path.
- **Never** opens a direct clearnet socket, under any failure. There is no
  fallback branch to remove later, because there is never a fallback.
- Returns typed unavailable / auth / resolve / connect outcomes.
- Uses bounded selection, retry, and backoff with a fixed ceiling.

`UseOutproxyPlugin` is the Java provider mechanism by wire name. i2pr does not
load plugins; `true` selects the configured provider path, and if none is
configured then creation and edit fail **before** any listener or destination is
allocated. No dynamic library loading and no command execution from I2PControl
input, ever.

## Canonical fields

Real semantics for `ProxyList`, `UseOutproxyPlugin`, `OutproxyAuth`,
`OutproxyUsername`, `OutproxyPassword`, `OutproxyType`, `SSLProxies`, and the
relevant `JumpList` behaviour.

- `OutproxyType` is a typed finite vocabulary. It is **not** an arbitrary
  executable or provider name, and it is not read from a config file as a command.
- `OutproxyPassword` is persisted only through Plan 341's sealed form, and is
  recovered only at header construction. It is never echoed into control output,
  even where Java's `rawConfig` would return it.
- Changing a provider or proxy list is a transactional reconfiguration with
  explicit drain and rebuild semantics, matching the existing
  `rollback_state` behaviour.

## Request paths

Integrate provider selection into the HTTP and CONNECT client paths and the
Proposal-applicable SOCKS families, preserving:

- direct `.i2p` routing, which bypasses the outproxy;
- no DNS leak;
- no direct-clearnet fallback;
- proxy auth redaction;
- bounded HTTP/CONNECT parsing;
- existing TLS semantics from service-tunnel policy.

## Evidence

The self-composed in-tree loopback outproxy fixture, decided with Plan 339 and
already on record: it is **loopback evidence, not interoperability**, matching the
existing `sam_stream_self_composed` precedent. Nothing vendored, patched, or
reused from an external router.

Required rows: `.i2p` direct path bypasses the outproxy; a clearnet target
succeeds through the controlled loopback outproxy; a clearnet target **without** a
provider fails and opens no direct socket; failover and retry ceiling; auth
success and failure with no secret echo; malformed `ProxyList` and `OutproxyType`;
HTTP CONNECT and applicable SOCKS coverage; restart and persistence; and proof
that no dynamic library loading or unrestricted execution exists.

## Invariants

1. No direct clearnet capability is introduced, in any path, including failures.
2. Every outbound credential moves through Plan 341's owner and nowhere else.
3. `UseOutproxyPlugin` with no configured provider fails before allocation.
4. Provider selection and retry are bounded.
5. No plugin loading and no command execution from control input.
6. A transaction that fails leaves no partial provider state.

## Out of scope

Encrypted LeaseSet2 and Red25519 (Plans 326/335), the type-11 ecosystem split, the
Plan 328 full-conformance gate, and any advertisement change.

## Stop conditions

Stop and re-audit if any design step would require a clearnet socket, a plaintext
password outside Plan 341's owner, an unbounded retry, or a plugin/exec
capability.
