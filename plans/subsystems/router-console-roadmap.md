# Router Console Roadmap

Status: parallel — **Plans 356, 357, and 358 are all passed.** The initial console foundation milestone is implemented. This line establishes a localhost-only, self-contained web console over I2PControl / Proposal 170 without changing router protocol support or exposing router internals. The console remains experimental, loopback-only, disabled by default, and non-advertised, and is not reachable through `i2pr run` while that pre-existing startup defect stands.

Long-term references:
- `GUARDRAILS.md`
- `specs/CONFORMANCE.md`
- `specs/support.toml`
- `docs/security-model.md`
- `docs/architecture/overview.md`
- `docs/architecture/dependency-graph.md`
- `docs/architecture/i2pr-i2pcontrol.md`
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`

Related ADRs:
- `docs/adr/0028-i2pcontrol-proposal-170-control-plane.md`
- `docs/adr/0034-eggserve-axum-router-console-http-substrate.md`

## 1. Purpose and ownership boundary

Build a first-class browser-based router administration console comparable in functional category to the Java I2P/I2P+ router consoles while preserving i2pr's control-plane and runtime boundaries.

The console is:

- a localhost-only administrative application;
- optionally browser-authenticated;
- backed by I2PControl API 1 and Proposal 170 rather than direct router-owner access;
- packaged with the router;
- fully self-contained in the browser, with no remote JavaScript, CSS, font, image, analytics, or telemetry dependency;
- theme-compatible with the Halloy TOML vocabulary and the 50-theme set currently shipped by EggPool, subject to explicit provenance/license verification;
- allowed to share EggServe as a library substrate with future eepsite HTTP serving, but never a listener/router/authentication trust zone.

The intended ownership split is:

~~~text
browser
  |
  | loopback HTTP
  v
i2pr-daemon
  |
  | owns console listener + EggServe lifecycle
  v
eggserve-server
  |
  v
TowerToEggserve
  |
  v
i2pr-console / Axum
  |
  | narrow control-client boundary
  v
I2PControl API 1 + Proposal 170
  |
  v
existing router owners
~~~

No console handler receives NetDB stores, tunnel pools, router identity material, service-tunnel managers, or a global mutable router context.

## 2. Work classification

- **Invariant:** loopback-only listener, browser/control credential separation, self-contained assets, no console-to-eepsite route sharing, no router-internal state access, bounded browser/session/control state.
- **Infrastructure:** EggServe/Axum composition, console crate boundary, embedded assets, Halloy-compatible theme adapter, security middleware, control-client abstraction.
- **Capability:** authenticated browser shell, read-only router overview, later tunnel/address-book/network/lifecycle management as the standard control plane supports it.
- **Polish:** richer graphs, responsive layouts, keyboard navigation, accessibility, theme previews, optional convenience pages, and visual convergence with EggPool follow only after correctness/security closure.

## 3. Non-goals

This line does not:

- expose the console on LAN, wildcard, public, I2P, or service-tunnel listeners;
- use a reverse proxy as the console security boundary;
- build an eepsite server in the same listener/application tree;
- replace or fork I2PControl / Proposal 170;
- add private i2pr-only browser endpoints for router state that bypass the control plane;
- add Java-specific JVM/Jetty/plugin controls;
- add a SPA framework, Node.js build/runtime dependency, external CDN, remote font, or remote analytics;
- require H2/H3/QUIC;
- make Proposal 170 conformance claims not already proven by its own roadmap;
- make anonymity/privacy claims.

Future eepsite serving is a separate product line even if it uses the same EggServe crates.

## 4. Current state

### Router control plane

The active i2pr Proposal 170 line already provides:

- API-1 `Authenticate`;
- canonical `RouterInfo`;
- `AddressBook`;
- `TunnelManager`;
- `ClientServicesInfo`;
- bounded source matrices and explicit unavailable/publish-gated dispositions;
- loopback-default secured I2PControl listener ownership in the daemon.

The console may begin against this implemented subset. It must treat unsupported or temporarily unavailable fields as capability/state, not synthesize values.

Base I2PControl administrative methods such as `RouterManager`, `GetRate`, `NetworkSetting`, `I2PControl`, and `AdvancedSettings` are not part of the current five-method i2pr surface. The console line must not invent replacements. Later console pages that need them require separate control-plane plans.

### HTTP substrate

EggServe currently provides a direct H1 application-server runtime in `eggserve-server 0.4.0` plus an optional Tower bridge. Its repository has a dedicated Axum 0.8 qualification proving streaming request/response behavior, middleware execution, disconnect cancellation, duplicate headers, and supervised shutdown through the direct server.

EggServe's own application-server benchmark campaign retained the Tower/Axum path. The measured adapter/binary overhead is small enough for a localhost administrative UI and avoids recreating framework concerns inside i2pr.

Axum 0.8 remains a current Tower-based routing/request-handling library. The console therefore uses EggServe for transport and Axum for application routing.

### Theme substrate

EggPool currently ships 50 Halloy-format TOML themes and a Rust adapter that converts Halloy palette roles into semantic CSS variables. `Cyber Red` is its default.

The console will adopt the same data model and naming semantics. Theme data must enter i2pr through an explicit source-revision/license/provenance record. Halloy upstream is GPL-licensed; no direct copy from that repository is authorized by this roadmap.

## 5. Target architecture

### Console server

~~~text
127.0.0.1 / ::1
      |
      v
daemon-owned TcpListener
      |
      v
eggserve-server (H1)
      |
      v
TowerToEggserve
      |
      v
Axum Router
  /             \
HTML/assets      console API/actions
  |                    |
  +-------- i2pr-console+
                       |
                       v
              bounded ControlClient
                       |
                       v
             I2PControl/Prop 170
~~~

The browser never talks to I2PControl directly and never receives its password/token.

### Future HTTP role separation

~~~text
loopback browser
  -> console EggServe instance
  -> console Axum router
  -> administrative control client

I2P service tunnel
  -> distinct eepsite EggServe instance
  -> eepsite/static/application service
~~~

Shared code may include HTTP primitives, response helpers, or theme-independent presentation utilities. Runtime listeners, route trees, authentication state, cookies, CSRF state, and administrative app state remain distinct.

### Browser packaging

The normal console response graph is entirely local:

~~~text
binary/package
  +-- HTML
  +-- CSS
  +-- small vanilla JS
  +-- SVG icons
  +-- theme TOML / generated palette data
~~~

No web build step is required at runtime.

## 6. Dependency graph

~~~text
356 HTTP/application substrate + embedded theme/assets foundation
  ->
357 localhost browser security + optional authentication/session boundary
  ->
358 Proposal-170 read-only control adapter + overview vertical slice
  ->
future typed administration pages
  |-- TunnelManager / AddressBook
  |-- GetRate graphs after base API support
  |-- RouterManager lifecycle/reseed after base API support
  |-- NetworkSetting after base API support
  |-- peer actions only after a standard control-plane owner exists
  +-- managed-app/AppManager integration after that subsystem exposes its admin API
~~~

Plans 356–358 do not depend on full Proposal 170 closure. Plan 358 consumes only contract fields/methods already proven at its implementation baseline and must degrade explicitly when a selector/method is unavailable.

If implementation demonstrates that generic capability discovery is required, register a separate I2PControl/Proposal-170 plan rather than adding an i2pr-private console endpoint.

## 7. Milestones

| Plan | State | i2pr token | Classification | Implementation | Closure |
|---|---|---|---|---|---|
| 356 | passed | `passed-eggserve-axum-self-contained-console-foundation` | infrastructure + invariant | `plans/implementation/router-console/356-eggserve-axum-self-contained-console-foundation.md` | `plans/closure/router-console/356-status.md` (implementation `73506b38`) |
| 357 | passed | `passed-loopback-browser-security-and-optional-authentication` | invariant + infrastructure | `plans/implementation/router-console/357-loopback-browser-security-and-optional-authentication.md` | `plans/closure/router-console/357-status.md` (implementation `73506b38`) |
| 358 | passed | `passed-prop170-control-client-and-read-only-overview` | capability + infrastructure | `plans/implementation/router-console/358-prop170-control-client-and-read-only-overview.md` | `plans/closure/router-console/358-status.md` (implementation `73506b38`) |

Later functional pages remain unnumbered. The first vertical slice has now
proved the browser/control boundary; the control-gap matrix in
`plans/closure/router-console/358-status.md` identifies the exact missing
standard control methods and is the input to that next registration.

## 8. Cross-cutting requirements

1. Console bind validation accepts only loopback addresses. There is no remote-mode override.
2. Console and future eepsite listeners are separate runtime instances and trust zones.
3. EggServe owns the HTTP connection/runtime boundary; Axum does not start its own server.
4. The console application never imports router-owner state merely for convenience.
5. All router administration goes through standard I2PControl/Proposal-170 vocabulary or an explicitly planned standard extension.
6. Browser auth credentials, session identifiers, CSRF material, and I2PControl credentials/tokens are separate secret classes.
7. No browser response reflects control tokens, passwords, destination private keys, LeaseSet client-auth secrets, proxy secrets, or raw sensitive config.
8. Every request body, form, query, route parameter, cookie, session table, login throttle, response, control result, log view, peer list, graph series, and asset has an explicit bound.
9. No CDN or other external origin is required for normal rendering.
10. No `@font-face`; use platform/system font stacks.
11. Theme conversion is deterministic and bounded. Invalid/untrusted theme content falls back safely.
12. Theme provenance is explicit; no ambiguous upstream copying.
13. Console disablement must leave no listener/task/asset server active.
14. Adding the console does not alter router advertisement, transport activation, or protocol support claims.
15. Java/I2P+ are UX/functional references, not source-code donors.

## 9. Verification strategy

### Static/dependency evidence

- dependency-direction and runtime-boundary checks;
- no `axum::serve`, direct Hyper server, or second HTTP parser path in console code;
- no console dependency on NetDB/tunnel/router-owner crates except the approved control contract/client boundary;
- no remote asset URLs, external script/style/font tags, `@font-face`, analytics, or telemetry;
- no wildcard/non-loopback bind option;
- exact theme source manifest/hash/provenance.

### HTTP/application evidence

- real loopback TCP through EggServe -> Tower adapter -> Axum route;
- bounded request bodies and predictable rejection;
- graceful cancellation/shutdown;
- response headers and CSP;
- static embedded asset integrity/cache behavior;
- no filesystem asset dependency in normal operation.

### Browser-security evidence

- Host and Origin rejection;
- no permissive CORS;
- session expiry/eviction;
- login throttle;
- CSRF on mutation;
- cookie policy;
- clickjacking/CSP/referrer/content-type hardening;
- unauthenticated mode has no hidden authentication bypass state.

### Control-plane evidence

- canonical JSON-RPC/I2PControl requests;
- authentication/token lifecycle remains backend-only;
- unavailable methods/selectors render explicit unsupported/unavailable state;
- read-only dashboard data derives exclusively from control responses;
- fixture comparison against standard reference responses where feasible.

## 10. Risks and decision points

### Framework drift

The EggServe Tower adapter and Axum request-body type must remain compatible. Pin compatible versions and retain one focused consumer test in i2pr so upgrades fail locally.

### Trust-zone collapse

Because EggServe may later serve eepsites, the greatest architectural risk is reusing a route tree/listener to "save a port". This is forbidden even on localhost. Code sharing must stop below the application/listener trust boundary.

### Theme licensing/provenance

Halloy upstream's GPL license makes direct copying a legal/product decision rather than a mere technical task. The implementation should consume EggPool's separately licensed assets only if provenance supports that use. Otherwise ship the compatible schema/adapter and independently licensed palettes first.

### Control-plane incompleteness

A Java/I2P+-class console needs some base API-1 administrative methods not currently implemented by i2pr. The console must expose only what the standard control plane truthfully supports and drive missing functionality back into the I2PControl roadmap.

### Localhost does not eliminate browser attacks

DNS rebinding, cross-site requests, clickjacking, stale sessions, XSS, and secret reflection remain relevant even for loopback applications. Plan 357 is therefore required before any mutating console page.

## 11. Completion definition

The router-console line reaches its initial foundation milestone when:

- a disabled-by-default console can be enabled on a loopback-only listener;
- EggServe is the HTTP transport owner and Axum is only the application router;
- the UI ships and renders with zero external browser dependencies;
- the Halloy-compatible theme adapter is proven with a provenance-cleared theme set;
- optional browser authentication and mutation protections are closed;
- the browser backend consumes I2PControl/Proposal 170 rather than router internals;
- a read-only overview presents truthful router/control state and explicit capability gaps;
- console disable/shutdown is clean and bounded;
- no public/I2P/LAN route can reach the console;
- support/conformance documentation remains unchanged except for documenting the console itself.

Full Java/I2P+ functional parity is not part of this initial completion definition. It requires later standard control-plane method support and separately planned UI pages.

## 12. Milestone status summary

**Plans 356, 357, and 358 are passed.** Implementation landed together in
`73506b38` because each plan is only meaningful with the others present and
because Plan 358's `ControlDispatcher` extraction is a precondition for the
console's data path; the closure records separate what each plan contributed.

Plan 356 established the reusable HTTP/application boundary: direct EggServe H1
transport, Axum routing through `TowerToEggserve`, the `i2pr-console` crate
(zero workspace dependencies, no socket, `axum` built without `tokio`),
compile-time browser assets, and a bounded Halloy-compatible theme adapter.
Three original i2pr palettes ship; **the 50-theme EggPool import was not
performed** because its licence could not be established, and that blocked
follow-up is recorded in `crates/i2pr-console/assets/themes/PROVENANCE.md`.

Plan 357 hardened the loopback browser boundary with an exact `Host` allow-list
derived from the bound address and resolved port, Origin validation for unsafe
methods, centralized CSP/hardening headers with no `unsafe-inline` and no
`Server` header, optional Argon2id authentication with bounded concurrency,
bounded opaque sessions with revocation, CSRF, and a console-wide login throttle
(the substrate exposes no unforgeable per-client identity, so the throttle bounds
delay rather than attributing it per client).

Plan 358 supplied the first end-to-end control-plane consumer. `ControlDispatcher`
was extracted from `I2pControlServiceState` so the console and the external
listener share **one** implementation of every handler; `LocalConsolePrincipal` is
a daemon-issuable closed allow-set limited to `RouterInfo` and `ClientServicesInfo`
that bypasses only the external bearer-token step. Local-vs-wire parity is proved
by test. Enabling the console requires **no** external I2PControl listener,
password, or token. Missing standard administrative methods remain recorded as
control-plane work rather than bypassed with console-private router access.

**Two consequences outside this subsystem.** First, the workspace MSRV moved
`1.88` → `1.89`, because every published `eggserve-server` release (0.2.0–0.4.0)
and `eggserve-primitives` 0.2.2 declares `rust-version = "1.89"` and the rest of
the locked graph tops out at exactly 1.88.0. Second, Plan 356 closed a
pre-existing gap in `check-dependency-direction.sh`: the map now has one
`expected` key per workspace member (22 for 22), and `check-console-boundaries.sh`
rule 7 asserts that, so the gap cannot silently reopen.

**Honest current posture.** The console has **no product-reachable path**, because
`i2pr run` does not open any listener — the pre-existing Essential-`lifecycle`
readiness defect recorded in `README.md` and `AGENTS.md`. The console is
reachable only through tests and direct `ConsoleServiceState` use. Closing that
defect needs its own plan-of-record and is not part of this line.
