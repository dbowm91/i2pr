# ADR 0034 — EggServe transport with Axum application routing for the router console

Status: Accepted  
Date: 2026-10-05

## Context

i2pr needs a web-based router console comparable in scope to the Java I2P and I2P+ consoles. The console is an administrative surface, not an eepsite: it is intended to bind only to loopback, may optionally require browser authentication, must consume the I2PControl / Proposal 170 control plane rather than router internals, and must ship all browser assets in the router package with no CDN, remote script, remote font, or other runtime web dependency.

The router is also expected to need an HTTP server substrate for future eepsite/service-tunnel serving. Reusing one hardened HTTP transport implementation is desirable, but the administrative console and an I2P-facing eepsite listener are different trust zones and must never share routes, authentication state, listener policy, or application state merely because they use the same HTTP library.

The current EggServe line is directly relevant:

- `eggserve-server 0.4.0` is the supported direct H1 application-server substrate;
- it owns strict H1 parsing/framing, request/response limits, lifecycle/cancellation, admission, shutdown, and response normalization;
- its optional `tower` feature exposes `TowerToEggserve`;
- EggServe carries a first-class Axum 0.8 qualification suite proving `axum::Router` composition, incremental request/response bodies, middleware execution, duplicate headers, disconnect cancellation, and server supervision;
- EggServe's application-server benchmark campaign found measurable but small adapter cost relative to the native service path and retained the Tower/Axum path after optimization;
- EggServe itself explicitly does not attempt to be an application framework.

Axum 0.8 remains a current, maintained routing/request-handling library built around Tower's `Service` abstraction. Its routing, extractors, typed state, response conversion, and middleware composition are useful for a multi-page administrative application. Using only EggServe's native `Service` would require i2pr to reimplement a subset of that framework functionality.

## Decision

### 1. EggServe owns HTTP transport and connection lifecycle

The router console will use the direct `eggserve-server` H1 runtime as its HTTP transport substrate.

The initial dependency target is:

~~~toml
eggserve-server = { version = "0.4", default-features = false, features = ["tower"] }
~~~

The console must not use `axum::serve`, a second Hyper server, or a hand-written HTTP parser/connection loop.

The initial console needs H1 only. H2, H3, QUIC, generic tunnels, forward-proxy behavior, and EggServe's compatibility umbrella are out of scope unless a later plan establishes a concrete console requirement.

### 2. Axum owns application routing, extraction, and middleware composition

The console application layer will use Axum 0.8 with a minimal feature set and run it through EggServe's `TowerToEggserve` adapter.

The intended composition is:

~~~text
loopback TCP listener
        |
        v
eggserve-server
  H1 parsing / bounds / lifecycle / shutdown
        |
        v
TowerToEggserve
        |
        v
axum::Router
  routes / extractors / typed app state / UI middleware
        |
        v
console handlers
~~~

Axum is not the network server in this design. EggServe remains the connection/runtime authority.

### 3. The console application lives behind a distinct crate boundary

A new `i2pr-console` application crate will own the Axum router, browser-facing request/response policy, bundled UI resources, theme translation, and console-specific view models.

It must not own a TCP listener, Tokio runtime, router protocol state, NetDB state, tunnel pools, destination identities, or I2PControl implementation state.

The daemon remains the composition root. It owns the loopback listener/server lifecycle and supplies the console application plus its narrow control client/state.

### 4. Browser assets are compile-time bundled

The console will not use `eggserve-static` for its own UI assets because the required console package is self-contained rather than directory-backed.

HTML, CSS, JavaScript, SVG icons, and theme data are compiled into the program/package and returned as bounded application responses. No runtime filesystem asset root is required for normal console operation.

The browser surface must support a CSP with no external script/style/font origins. Inline script/style should be avoided so the normal policy can use `script-src 'self'` and `style-src 'self'` without `unsafe-inline`.

### 5. Halloy-compatible theming follows EggPool's adapter model, not a frontend dependency

The console will implement the Halloy TOML theme vocabulary as data and translate it to semantic CSS custom properties, following the proven EggPool pattern.

Theme identity is a stable string so a user may choose the same named theme in Halloy, EggPool, and i2pr-console.

The initial target is the 50-theme set currently shipped by EggPool. Theme files or conversion code may be copied from EggPool only after recording the exact source revision and license/provenance. Do not copy files directly from the GPL-licensed Halloy repository into i2pr without a separate license decision. If the 50-file provenance cannot be established cleanly, the implementation must stop at the compatible theme parser/adapter plus independently licensed/original themes rather than silently importing ambiguous assets.

### 6. Console and eepsite HTTP servers share only the transport substrate

A future eepsite/service-tunnel HTTP server may also use EggServe, but it must be a separate `Server`/listener/application instance.

The following composition is forbidden:

~~~text
I2P service tunnel ----+
                       +--> one shared Router ----> /console
loopback administrator-+
~~~

The intended isolation is:

~~~text
browser on loopback
    -> console EggServe instance
    -> console Axum router

I2P service tunnel
    -> separate eepsite EggServe instance
    -> eepsite application/static service
~~~

No host header, path prefix, middleware branch, or authentication check may be the sole boundary between an I2P-facing request and router administration.

### 7. Loopback-only is an invariant, not a default

The console listener accepts only IPv4 or IPv6 loopback addresses. Configuration requesting a non-loopback bind must fail validation.

There is no `--public`, LAN mode, wildcard bind, reverse-proxy mode, or "unsafe allow remote" escape hatch in this console line.

Optional browser authentication is implemented separately from the I2PControl credential. The browser never receives the router's I2PControl token or password.

## Alternatives considered

### EggServe native `Service` without a framework

This is technically viable and is the smallest dependency/adapter path. It was rejected for the console because route matching, path/query extraction, typed state, forms, response conversion, and middleware would become i2pr-owned application-framework code. EggServe deliberately does not own those concerns.

The native service path remains appropriate for very small fixed HTTP services and may be used by future eepsite components where a framework is unnecessary.

### Axum as both application framework and HTTP server

Using `axum::serve` would be conventional and somewhat simpler. It was rejected because i2pr already intends to standardize on EggServe for hardened HTTP serving, EggServe is already qualified against Axum, and using a second server runtime would duplicate connection policy/lifecycle ownership.

### `eggserve-core` compatibility umbrella

Rejected for the initial console. The console does not need H2/H3, proxy, generic tunnel, or compatibility orchestration. The direct `eggserve-server` + `tower` path is narrower and is the EggServe project's supported application-server direction.

### A larger frontend framework or WASM SPA

Rejected for the initial console. Server-rendered HTML plus bounded vanilla JavaScript is sufficient, easier to make fully self-contained, and produces a smaller browser attack/dependency surface.

## Consequences

Positive:

- one hardened HTTP transport substrate can serve the console now and other router HTTP roles later without sharing trust zones;
- routing/application ergonomics come from a mature framework instead of local reinvention;
- the EggServe/Axum integration is already qualified upstream;
- the console can remain H1-only, loopback-only, and self-contained;
- the browser layer remains independent of router implementation internals.

Costs:

- the Tower/Axum bridge adds a small amount of per-request adaptation and binary/dependency footprint compared with a native EggServe service;
- i2pr must track compatibility across EggServe and Axum upgrades;
- the daemon must keep console listener ownership separate from future eepsite listener ownership;
- Halloy-compatible theme assets require explicit provenance accounting.

EggServe's retained application-server measurements make the adapter overhead acceptable for a localhost administrative UI, where correctness, isolation, and maintainability dominate raw request throughput.

## Verification implications

The router-console plans must add static and runtime evidence for:

- one EggServe runtime authority and no `axum::serve`/second Hyper server;
- loopback-only bind validation;
- distinct console vs eepsite/service-tunnel server instances when both eventually exist;
- no external browser asset origins;
- bounded request/response/session state;
- EggServe/Axum incremental-body and cancellation behavior remaining covered by dependency qualification or focused consumer tests;
- exact theme-source provenance and contrast/readability checks;
- no router capability claim introduced merely by adding the console.

## Related work

- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`
- `plans/subsystems/router-console-roadmap.md`
- `docs/adr/0028-i2pcontrol-proposal-170-control-plane.md`
- EggServe `docs/downstream-app-server.md` and its Axum/Tower qualification
