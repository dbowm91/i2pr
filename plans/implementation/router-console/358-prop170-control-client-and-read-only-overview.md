# Plan 358 — Proposal-170 control client and read-only router overview

Status: **blocked-router-console-prop170-overview-on-plans356-357**.

Classification: **capability + infrastructure**.

Roadmap:
- `plans/subsystems/router-console-roadmap.md`

Hard dependencies:
- Plan 356 must close the EggServe/Axum console/application substrate.
- Plan 357 must close the browser-security and optional-authentication boundary.

Interface dependency:
- the active I2PControl / Proposal 170 line remains authoritative for exact wire/domain semantics and support state. This plan consumes only fields/methods proven at its implementation baseline and does not gate on full Proposal 170 closure.

## Objective

Deliver the first truthful end-to-end router-console vertical slice:

- a narrow `ControlClient` abstraction used by `i2pr-console`;
- a daemon-owned local control adapter that executes only canonical I2PControl / Proposal 170 methods through the same validated dispatch authorities as the external I2PControl service;
- no requirement to enable or expose the external I2PControl TCP/TLS listener merely to use the built-in console;
- differential evidence proving the local adapter and the loopback I2PControl wire endpoint produce equivalent method semantics for the covered read-only requests;
- a bounded read-only overview page/API showing currently supported router, network, NetDB, tunnel, service, log/news/rate information;
- explicit `unsupported`, `unavailable`, or `not currently published` UI states instead of fabricated zero/empty values;
- no direct router-owner handles in `i2pr-console`.

This plan does not add missing I2PControl methods or router mutations.

## Why this plan is blocked but well-defined

The browser-facing side must first have:

- Plan 356's real EggServe/Axum application boundary;
- Plan 357's Host/Origin/auth/session/CSP hardening.

The router-side control implementation is already sufficiently mature to plan this consumer. The current daemon's `I2pControlServiceState::dispatch_body` is I/O-independent after the HTTP body is obtained, and method-specific dispatch already converges on canonical `i2pr-i2pcontrol` validation before reaching narrow owner adapters.

That makes it feasible to extract an authenticated/authorized local dispatch seam without creating another RouterInfo/TunnelManager/AddressBook implementation.

## Current implementation evidence

At registration:

- `i2pr-i2pcontrol` owns canonical JSON-RPC/I2PControl/Proposal-170 vocabulary and bounds.
- `i2pr-daemon::I2pControlServiceState` owns token authentication, source-IP throttle, live source adapters, and method dispatch.
- `dispatch_body` is already pure with respect to socket I/O once it receives bounded request bytes and request metadata.
- Protected methods currently perform token validation before their method-specific dispatch.
- The external I2PControl listener is disabled by default and has its own password/TLS/token lifecycle.
- Requiring the console to enable that listener would couple one local product feature to another optional network endpoint and would force internal browser-backend traffic through managed loopback TLS solely as an implementation artifact.
- Directly handing `i2pr-console` the service state or router owner handles would violate the console/control-plane boundary.

The plan therefore separates **control authorization origin** from **control method semantics** while preserving one validation/dispatch implementation.

## Architecture decisions frozen by this plan

### 1. `i2pr-console` sees a narrow control client, never daemon/router internals

Define a console-facing async control abstraction with operations expressed in canonical control-domain terms.

The exact Rust shape may be a trait or a small concrete client facade, but it must not expose:

- `I2pControlServiceState`;
- token tables;
- password material;
- NetDB/tunnel/address-book owner types;
- daemon composition structs;
- arbitrary closures into daemon state.

A conceptual shape is:

~~~rust
trait ControlClient {
    async fn router_info(&self, selection: RouterInfoSelection)
        -> Result<RouterInfoResult, ControlClientError>;

    async fn client_services(&self)
        -> Result<ClientServicesResult, ControlClientError>;
}
~~~

Use exact existing contract/result types where practical. Do not invent a second selector vocabulary.

### 2. Built-in console uses a trusted local control principal, not an I2PControl bearer token

Refactor daemon control dispatch so authentication/authorization is a narrow gate in front of one canonical protected-method dispatcher.

At minimum distinguish:

- external I2PControl request authenticated by API-1 token;
- built-in local console request authorized by daemon composition.

The local principal must be unforgeable from an HTTP/browser request. It is created only by daemon composition and is not serializable over the console HTTP API.

The local principal bypasses only the **bearer-token transport requirement**. It does not bypass:

- method-name validation;
- parameter validation;
- selector/action/value bounds;
- source availability rules;
- result bounds;
- mutation transaction rules;
- secret redaction;
- capability/support semantics.

There must be no "console-only router state getter."

### 3. The local adapter uses canonical request/domain dispatch

Prefer extracting a method such as an authorized decoded-request dispatcher from the existing I2PControl service rather than serializing JSON just to parse it again in-process.

The canonical chain should remain equivalent to:

~~~text
external wire:
HTTPS JSON-RPC
 -> bounded decode
 -> token gate
 -> canonical protected dispatcher
 -> method adapter
 -> router owner

built-in console:
daemon-issued LocalConsole principal
 -> canonical typed request/selection
 -> canonical protected dispatcher
 -> method adapter
 -> router owner
~~~

The protected dispatcher must remain one implementation.

If avoiding JSON decode causes contract validation to diverge, keep a canonical encode/decode boundary or add shared validation helpers in `i2pr-i2pcontrol`; do not fork validation.

### 4. Keep external wire parity as executable evidence

Add differential tests that issue the same read-only selection through:

1. the local console adapter; and
2. the real loopback I2PControl endpoint after normal API-1 authentication.

Normalize only transport envelope details such as request IDs/tokens. Method result shape, errors, unavailable-source behavior, ordering where specified, bounds, and redaction must agree.

This prevents the local principal from silently becoming a richer private API.

### 5. The console does not require `[i2pcontrol].enabled = true`

When the console is enabled and the external I2PControl listener remains disabled:

- the daemon still constructs only the internal control dispatch/source composition required by the console;
- no I2PControl TCP/TLS listener is bound;
- no I2PControl password is required solely for console operation;
- no bearer token is minted for normal local console reads.

When both are enabled, both origins share the same canonical method/source authorities but retain separate authentication surfaces.

If the current `I2pControlServiceState` construction is inseparable from listener/TLS/password configuration, refactor composition into:
- a listener-independent control dispatcher/source owner; and
- the optional external I2PControl listener/auth wrapper.

Do not fake-enable the external service on an ephemeral port.

### 6. Start with read-only methods only

Plan 358 authorizes the local console principal only for:

- `RouterInfo`;
- `ClientServicesInfo`.

Even though `AddressBook` and `TunnelManager` exist, their mutating forms remain future console plans after the browser security boundary has been proven in a real control-backed vertical slice.

The local-principal authorization model should be extensible to exact methods/actions later, but Plan 358's allow-set is closed and read-only.

### 7. Overview state is a presentation model, not a second telemetry owner

Create bounded view models derived from control results.

The console may format, group, label, and calculate presentation-only quantities from returned values. It may not:

- maintain alternative peer/tunnel counters;
- scrape daemon logs directly;
- read transport/runtime metrics directly;
- infer unavailable fields from neighboring state;
- turn a failed selector into zero/empty.

### 8. Capability/unavailability is explicit

At each refresh, the console must distinguish at least:

- supported and returned;
- standard selector/method known but currently unavailable/publish-gated;
- standard selector/method unsupported by this implementation/revision;
- request/control failure.

Do not use router version sniffing to infer support.

For the initial i2pr-only built-in adapter, a bounded method/selector support snapshot may be derived from the canonical contract/source matrices already maintained by `i2pr-i2pcontrol`.

If a router-neutral external console would require a new generic capability-discovery wire method, record that as a separate I2PControl/Proposal-170 plan. Do not add a private browser endpoint that pretends to solve the standard protocol gap.

### 9. Browser refresh is bounded polling first

Use same-origin `fetch` from the bundled JavaScript to one bounded read-only console API endpoint or page fragment.

Initial refresh semantics:

- one in-flight refresh per browser page;
- abort/ignore stale requests;
- finite refresh interval with a conservative lower bound;
- pause/back off on hidden/error state where practical;
- no WebSocket or SSE requirement;
- no browser direct connection to I2PControl.

The server must remain correct if JavaScript is disabled; the initial page render should contain a useful point-in-time snapshot.

## Invariants that must not regress

1. `i2pr-console` has no router-owner dependencies.
2. Local console authorization cannot be constructed from browser input.
3. Local console bypasses only external bearer-token transport authentication.
4. Canonical I2PControl method/parameter/source validation remains shared.
5. External I2PControl behavior is unchanged.
6. Console enablement does not auto-enable/bind the external I2PControl listener.
7. External I2PControl enablement does not automatically enable the console.
8. Plan 358 local principal is read-only and permits only RouterInfo/ClientServicesInfo.
9. Unavailable/unsupported values are never fabricated.
10. Browser never receives an I2PControl password/token.
11. No browser request can select arbitrary daemon methods by raw name unless the console route's closed allow-set permits them.
12. Every result/list/log/news payload remains bounded by control and console ceilings.
13. Refresh work is bounded per page/client and cancellable.
14. No router support/conformance claim is promoted.
15. No direct eepsite/I2P-facing listener gains administrative access.

## Scope

### In scope

- listener-independent daemon control dispatcher/source composition if required;
- local trusted control principal/origin;
- read-only local control client for RouterInfo + ClientServicesInfo;
- exact method allow-set;
- wire/local differential tests;
- bounded console view-model builder;
- read-only overview HTML;
- same-origin refresh endpoint/fragment;
- explicit capability/unavailability presentation;
- sections for useful currently supported data;
- observability/errors suitable for an operator without secret leakage;
- documentation of missing standard admin methods for future work.

### Out of scope

- `AddressBook` mutation UI;
- `TunnelManager` mutation UI;
- RouterManager/restart/shutdown/reseed;
- GetRate implementation if not already standard-supported at execution time;
- NetworkSetting;
- AdvancedSettings;
- I2PControl password mutation;
- peer ban/unban;
- app management;
- direct router-owner reads;
- generic new Proposal-170 capability method;
- external/router-neutral standalone console packaging;
- WebSockets/SSE.

## Required production changes

### A. Separate control dispatcher from external listener authentication where necessary

Refactor the current daemon I2PControl composition only enough to expose one supervised/listener-independent protected-method dispatch authority.

The external listener remains:

~~~text
HTTP/TLS -> JSON-RPC decode -> Authenticate/token gate -> protected dispatcher
~~~

The console adapter becomes:

~~~text
LocalConsole authority -> typed/canonical request -> protected dispatcher
~~~

Keep token tables/throttles/TLS listener state outside the reusable protected dispatcher when possible.

The dispatcher must not become a global `RouterContext`.

### B. Add closed local-principal authorization

Define a daemon-local authority enum/capability structure with an exact method/action allow-set.

For Plan 358, local console grants:
- RouterInfo read;
- ClientServicesInfo read.

Anything else rejects before method side effects.

Do not grant "all current I2PControl."

### C. Add console control-client boundary

`i2pr-console` owns/consumes only a narrow abstract client supplied at construction.

Avoid making `i2pr-console` depend on `i2pr-daemon`; dependency direction should remain daemon -> console, not cyclic.

If async trait machinery would add an unnecessary dependency, use an explicit boxed-future service interface or a concrete runtime-neutral request/response port. Keep it small.

### D. Build a point-in-time overview model

At minimum, where available from the canonical control baseline, include:

- router version/status/uptime/identity/public network state;
- bandwidth/total transfer values;
- known/active peers and reachability;
- exploratory/client/participating tunnel counts and build health;
- I2P tunnel/service summary;
- client service status;
- bounded recent logs and router news where available;
- explicit unavailable/support indicators.

Do not require every candidate field to exist. The view model should be sparse/typed and render missing capability honestly.

### E. Add bounded refresh API

Expose a fixed same-origin endpoint, for example `/api/overview`, returning a typed bounded JSON document or HTML fragment.

The endpoint has no arbitrary selector passthrough. The server owns the exact selection set.

Add cache policy suitable for live administrative state (`no-store`).

### F. Add a control-gap report for planning, not runtime fallback

During closure, produce a small documented matrix of desired Java/I2P+-class console functions vs current standard control support.

Expected gaps may include base I2PControl methods such as:
- `GetRate`;
- `RouterManager`;
- `NetworkSetting`;
- `I2PControl`;
- `AdvancedSettings`;
and standardized peer mutation/capability discovery if still absent.

Do not implement them under Plan 358. Register follow-up control-plane plans only after exact need/scope is confirmed by the working overview.

## Work packages

### WP1 — Freeze listener-independent control-dispatch boundary

Refactor/extract the shared protected dispatcher and local authority without changing external wire behavior.

### WP2 — Local read-only ControlClient

Implement RouterInfo/ClientServicesInfo methods through the canonical dispatcher and enforce the exact Plan-358 allow-set.

### WP3 — Local-vs-wire differential harness

Run representative success, unavailable, malformed/invalid selection, and bounded-result cases through both origins.

### WP4 — Overview view model and initial render

Build the server-rendered point-in-time dashboard from control results only.

### WP5 — Bounded same-origin refresh

Add one refresh endpoint and the small bundled client logic with single-in-flight/backoff behavior.

### WP6 — Control-gap accounting and docs

Record what later UI work needs from standard I2PControl/Proposal 170 and keep missing functionality out of private console APIs.

## Failure, cancellation, restart, and contention semantics

- local control request cancellation does not cancel/poison router owners beyond normal method semantics;
- one failed selector/group renders an explicit bounded error state according to the canonical whole-request semantics; the console may issue multiple intentionally grouped requests so one unavailable optional panel does not erase unrelated overview state;
- no automatic retry loop runs unboundedly;
- browser refresh permits are bounded and released on disconnect/cancellation;
- control dispatch concurrency remains under existing router-wide/service budgets;
- no token-table work is performed for local-console requests;
- console disable/shutdown drops local client handles and refresh work;
- external I2PControl listener shutdown remains independent;
- daemon restart reconstructs the local dispatcher from current router composition and has no console telemetry persistence.

## Compatibility and migration

External I2PControl wire behavior and authentication remain unchanged.

The internal refactor is not a public Rust API promise.

Existing configurations with I2PControl disabled remain valid; console operation does not require adding an `[i2pcontrol]` password.

If the implementation cannot separate protected dispatch from listener/auth without a broad daemon-global context, stop and register an I2PControl architecture corrective rather than giving the console router-owner handles.

## Required tests

At minimum:

### Local authority/dispatch

- local console principal can call RouterInfo;
- local console principal can call ClientServicesInfo;
- local console principal cannot call AddressBook mutation;
- local console principal cannot call TunnelManager mutation;
- arbitrary method names cannot be passed from browser route to dispatcher;
- local principal cannot be deserialized/constructed from request data;
- unavailable source behavior matches canonical contract;
- selector/value/max+1 validation remains canonical.

### Listener independence

- console + I2PControl disabled: console overview works, no I2PControl listener bound;
- console disabled + I2PControl enabled: external service works unchanged;
- both enabled: both work and share method semantics, not credentials;
- both disabled: neither listener exists;
- console startup does not require an I2PControl password/certificate.

### Differential

For representative RouterInfo selections and ClientServicesInfo:
- local result equals wire result after transport-envelope normalization;
- canonical error code/detail class agrees for invalid/unavailable selection;
- bounds/truncation behavior agrees;
- redaction agrees;
- repeated tests do not mutate token/auth state through local calls.

### Overview/browser

- first render works without JavaScript;
- refresh endpoint returns same typed view-model shape;
- unsupported/unavailable panels are labeled, not zero-filled;
- log/news strings are escaped;
- no private/control secret appears in HTML/JSON;
- one-in-flight refresh behavior;
- hidden/error backoff where implemented;
- response body/list ceilings.

### Structural

- `i2pr-console` has no dependency edge to daemon/router owner crates;
- protected dispatcher remains single implementation;
- no new console-private router-state endpoint below the control layer;
- support inventory unchanged.

## Exact verification commands

Focused commands should include:

~~~text
cargo fmt --all --check
cargo check --locked -p i2pr-i2pcontrol -p i2pr-console -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-i2pcontrol --all-targets -- --test-threads=1
cargo test --locked -p i2pr-console --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon i2pcontrol -- --test-threads=1
cargo test --locked -p i2pr-daemon console -- --test-threads=1
cargo clippy --locked -p i2pr-i2pcontrol -p i2pr-console -p i2pr-daemon --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-i2pcontrol -p i2pr-console -p i2pr-daemon --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-i2pcontrol-acceptance-evidence.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
~~~

Run the Plan-356 resource/boundary guard and Plan-357 browser-security suite.

Closure must also run the full routine floor from `AGENTS.md`.

## Documentation updates

Required:

- `docs/architecture/i2pr-i2pcontrol.md`;
- `docs/architecture/i2pr-console.md`;
- `docs/architecture/i2pr-daemon.md`;
- router-console roadmap;
- registry;
- console operator documentation;
- `plans/closure/router-console/358-status.md` at closure.

Add a concise console/control feature-gap matrix referencing the standard method owner rather than promising private workarounds.

## Acceptance criteria

Plan 358 passes only when:

1. `i2pr-console` consumes a narrow control-client abstraction and no router owners;
2. daemon composition provides an unforgeable local-console control authority;
3. local authority enters the same canonical protected method dispatcher used by external I2PControl after token authentication;
4. local authorization is read-only and closed to RouterInfo/ClientServicesInfo;
5. console operation does not enable/bind the external I2PControl listener or require its password;
6. representative local-vs-wire differential cases agree;
7. external I2PControl wire/auth behavior remains unchanged;
8. the initial overview derives all router data from canonical control results;
9. unavailable/unsupported data is explicit and never fabricated;
10. initial HTML works without JavaScript;
11. refresh is same-origin, bounded, single-in-flight, and cancellable;
12. browser responses contain no I2PControl/browser-auth secrets;
13. missing standard admin capability is documented as control-plane work, not bypassed;
14. no router support/conformance claim changes;
15. full routine floor passes.

## Stop conditions

Stop and register a separate corrective/architecture plan if:

- the only way to build the local client is to hand console code direct router-owner handles;
- local dispatch cannot share canonical validation/method implementation with the external endpoint;
- the external I2PControl wire behavior must change merely to accommodate the console;
- console enablement would have to silently enable an external listener;
- a desired overview panel requires fabricating a missing source;
- a generic capability-discovery problem cannot be solved truthfully from the current canonical contract/source matrices and needs a standard wire extension;
- the Plan-358 local read-only authority can reach a mutation path.

## Closure evidence required

The closure record must include:

- before/after I2PControl dispatch/auth ownership diagram;
- exact local-console authority type and allow-set;
- proof no external I2PControl listener/password is required;
- local-vs-wire differential corpus/results;
- `i2pr-console` dependency graph;
- overview field/source/support matrix;
- explicit unavailable/unsupported rendering evidence;
- browser no-secret/escaping/refresh evidence;
- full routine-floor results;
- current standard control-gap matrix;
- unblock audit for later console administration planning.

## Handoff notes

Do not attempt Java/I2P+ feature parity in this milestone.

The purpose is to prove the architectural contract: a useful browser view can be built entirely on the router's canonical control plane, while a built-in console avoids unnecessary loopback TLS/token plumbing and still cannot grow an implementation-private management API.
