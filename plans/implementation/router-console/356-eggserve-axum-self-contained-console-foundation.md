# Plan 356 — EggServe/Axum self-contained router-console foundation

Status: **registered-router-console-http-theme-foundation**.

Classification: **infrastructure + invariant**.

Roadmap:
- `plans/subsystems/router-console-roadmap.md`

ADR:
- `docs/adr/0034-eggserve-axum-router-console-http-substrate.md`

Hard dependencies:
- none beyond the current workspace baseline;
- the existing Proposal 170 / I2PControl contract remains an interface reference but is not modified by this plan.

Successor:
- Plan 357 hardens browser security/authentication over this server/application foundation.
- Plan 358 later attaches the first read-only Proposal-170 control consumer after Plans 356–357 close.

## Objective

Add the minimum production foundation for a disabled-by-default, localhost-only router console using:

1. `eggserve-server` as the sole HTTP/1 transport, connection-lifecycle, admission, and shutdown authority;
2. Axum as the application router through EggServe's `TowerToEggserve` adapter;
3. a new `i2pr-console` crate for browser-facing application logic and embedded resources;
4. compile-time bundled HTML/CSS/JavaScript/SVG/theme data with no runtime web dependency;
5. a Halloy-compatible theme translation layer modeled on EggPool;
6. hard architectural separation between the administrative console and any future eepsite HTTP server that also uses EggServe.

This plan ends with a static/local console shell and theme system. It does **not** expose router state or mutations yet.

## Why this plan is ready

The server/framework choice is no longer speculative:

- EggServe 0.4.0 documents `eggserve-server` as the direct H1 application-server substrate.
- EggServe's `tower` feature exposes `TowerToEggserve`.
- Its `axum_tower_qualification` test composes an Axum 0.8 `Router` through the direct EggServe server and proves incremental uploads/responses, middleware, duplicate headers, disconnect cancellation, and supervised shutdown.
- EggServe Plans 294–297 measured the direct Tower/Axum profile and retained it. The cost relative to native service is acceptable for a localhost administration UI; avoiding a local routing/middleware framework is more valuable than chasing maximum loopback request throughput.
- Axum 0.8 is built around Tower services and does not need to own the network server in this architecture.
- EggPool already demonstrates a Halloy-TOML-to-semantic-CSS adapter and currently carries 50 theme files.

The remaining work is i2pr composition and invariants, not HTTP-framework research.

## Current implementation evidence

At registration:

- i2pr has no router-console crate or console listener;
- `i2pr-daemon` already owns local administrative/service listeners and runtime composition;
- `i2pr-i2pcontrol` owns the runtime-neutral control contract;
- the Proposal 170 roadmap explicitly excluded a frontend;
- EggServe is not currently a workspace dependency;
- no console assets exist;
- no eepsite HTTP server is implemented in this line;
- EggPool's current theme directory contains 50 TOML files and its theme adapter maps Halloy roles to CSS variables;
- Halloy upstream is GPL-licensed while EggPool carries an MIT license, so exact theme-file provenance must be recorded before copying assets.

## Architecture decisions frozen by this plan

### 1. Direct EggServe H1, not `eggserve-core`

Add a workspace dependency on the direct server line, initially:

~~~toml
eggserve-server = { version = "0.4", default-features = false, features = ["tower"] }
~~~

Use the exact compatible patch resolved by the lockfile and record it in closure evidence.

Do not pull `eggserve-core`, H2/H3, QUIC, forward-proxy support, TLS composition, or filesystem static serving merely for the console.

### 2. Axum is an application dependency only

Add Axum 0.8 with the narrowest feature set that supports the required `Router`/routing/state behavior.

`i2pr-console` owns the Axum router. The daemon must not call `axum::serve`.

The transport composition is:

~~~text
daemon TcpListener
  -> eggserve-server::Server
  -> TowerToEggserve<axum::Router>
  -> i2pr-console handlers
~~~

Retain one focused real-socket consumer test in i2pr so dependency upgrades fail at the actual adapter boundary.

### 3. `i2pr-console` owns browser/application concerns, not runtime sockets

Create `crates/i2pr-console`.

It may own:

- route construction;
- typed console application state that contains only console/control abstractions;
- embedded resource tables;
- theme parsing/translation;
- HTML rendering helpers/templates;
- browser-facing response metadata;
- tests for route/resource/theme behavior.

It must not own:

- `tokio::net::TcpListener`;
- router protocol state;
- router identity/private key access;
- NetDB stores;
- tunnel managers;
- destination runtimes;
- service-tunnel managers;
- I2PControl service internals.

Listener/supervisor ownership remains in `i2pr-daemon`.

### 4. Console bind policy is loopback-only from the first commit

Add a disabled-by-default `[console]` configuration block with a loopback default, provisionally `127.0.0.1:7657`.

Only IPv4/IPv6 loopback addresses are valid. Reject non-loopback configuration during config normalization before bind.

Do not add LAN/public/reverse-proxy escape hatches.

Authentication itself is Plan 357 scope, but the unauthenticated Plan-356 shell is still loopback-only.

### 5. Console assets are embedded application responses

Normal operation must not read a UI directory from disk.

Bundle at least:

- base HTML shell;
- base stylesheet;
- small vanilla JavaScript bootstrap if needed;
- favicon/logo SVG;
- theme data or generated theme palette table.

Use `include_bytes!`, `include_str!`, generated Rust tables, or an equivalently auditable compile-time mechanism.

Do not add a Node/npm/pnpm/yarn runtime/build dependency.

Do not add Webpack/Vite or another frontend bundler merely to concatenate local assets.

### 6. Theme adapter is semantic, not page-specific

Introduce a bounded `ThemePalette` / equivalent containing semantic web roles such as:

- page background/text/border;
- topbar/nav surfaces;
- card/panel surfaces;
- table headers/borders;
- primary/secondary/muted text;
- success/warning/error/info;
- button background/text/hover/active;
- link/accent;
- chips/tags;
- graph/heatmap roles where already justified.

Parse the Halloy-compatible TOML schema and derive missing web-specific roles deterministically from palette colors. Invalid data falls back to a compiled default (`Cyber Red` if provenance clears, otherwise an original i2pr default).

The browser consumes CSS custom properties; page CSS must not contain one-off theme-specific selectors.

### 7. Theme provenance is a gate

Before importing the EggPool 50-theme set, add a small provenance manifest containing:

- source repository;
- exact source commit;
- source path;
- license observed at that revision;
- file count;
- content hashes or an aggregate manifest hash;
- note distinguishing EggPool assets from Halloy upstream.

Do not copy directly from Halloy upstream.

If review cannot establish that EggPool's copies may be redistributed under the intended i2pr licensing posture, stop the bulk import. The Plan may still close only if the Halloy-compatible parser/adapter and a provenance-clean default/original test set are complete, with the 50-theme import recorded as blocked follow-up rather than silently included.

### 8. Future eepsite reuse stops below the trust-zone boundary

Document and statically guard that a future eepsite HTTP server may reuse `eggserve-server` but must instantiate a different listener/server/application tree.

No common router may mount both administrative and eepsite routes.

No path/Host/auth middleware branch may be the sole boundary between the two.

## Invariants that must not regress

1. Console listener is disabled by default.
2. When enabled, only loopback binds validate.
3. EggServe is the only console HTTP transport runtime.
4. Axum does not own a second server.
5. `i2pr-console` does not access router owners directly.
6. Console and future eepsite HTTP roles are distinct server instances.
7. Browser resources require no external origin.
8. No `@font-face`; use system font stacks.
9. No remote script/style/font/image/analytics/telemetry request is needed to render.
10. Every embedded asset has a compile-time size bound.
11. Theme parsing has explicit input/file/count/value bounds.
12. Invalid themes cannot inject raw CSS.
13. No router capability/support advertisement changes.
14. No I2PControl/Proposal-170 wire change is introduced by this plan.
15. Console failure cannot become a router-core availability dependency when disabled.

## Scope

### In scope

- ADR 0034 enforcement;
- `i2pr-console` crate skeleton;
- workspace dependency additions/review;
- daemon `[console]` config normalization;
- daemon-owned loopback EggServe startup/shutdown wiring;
- Axum router composition through `TowerToEggserve`;
- `/` shell plus embedded `/assets/...` and generated theme CSS endpoint(s);
- Halloy-compatible theme parser/adapter;
- provenance gate for EggPool theme assets;
- basic responsive shell inspired by EggPool's dashboard structure;
- system-font styling;
- static checks/tests for no external asset dependencies;
- architecture/dependency docs.

### Explicitly out of scope

- I2PControl client/authentication;
- router status data;
- tunnel/address-book/network controls;
- browser login/session/CSRF;
- remote console access;
- eepsite serving;
- framework-driven SPA hydration;
- Chart.js or other large chart dependencies;
- service workers;
- WebSockets;
- H2/H3;
- managed-app UI embedding.

## Required production changes

### A. Workspace and dependency review

Add the minimal dependencies with centralized versions.

Record for EggServe/Axum and any template/theme helper:

- purpose;
- enabled features;
- transitive graph delta;
- unsafe exposure;
- license;
- MSRV compatibility with i2pr;
- duplicate Hyper/Tokio/http-body versions;
- advisory status.

Prefer no template-engine dependency in this plan unless it materially reduces unsafe/manual HTML construction. If a template engine is selected, it must compile templates into the binary and escape values by default.

### B. New console application crate

Add `crates/i2pr-console` with `#![forbid(unsafe_code)]`.

Expose a narrow constructor such as `router(state) -> axum::Router` or equivalent. Initial state contains only theme/resource configuration and placeholders; no router owner handles.

Keep assets under the crate and compile them in.

### C. Daemon-owned console service

Add config and a supervised service lifecycle.

The daemon:

1. validates loopback bind;
2. creates/binds the listener;
3. constructs the `i2pr-console` router;
4. wraps it with `TowerToEggserve`;
5. starts EggServe under existing supervisor/cancellation ownership;
6. reports readiness only after bind/server readiness;
7. drains/shuts down within bounded time.

No detached task.

### D. Embedded resource policy

Define a fixed resource map with exact content type, cache policy, and maximum bytes.

HTML and dynamic theme CSS should normally be `no-store` or appropriately short-lived. Content-addressed immutable assets may receive immutable caching later if filenames actually carry hashes.

Reject unknown asset paths with a small fixed 404.

### E. Theme translation

Implement the Halloy-compatible parser using bounded TOML input.

Only recognized color/value fields affect the palette. Never splice arbitrary source strings into CSS declarations. Validate hex colors exactly (6/8 hex digits where supported), discard alpha where opaque UI surfaces require it, and derive roles through typed color functions.

Add automated contrast/readability checks for core text/background/control pairings. A theme that cannot meet the console's minimum readability rules must fall back/adjust deterministically rather than rendering invisible controls.

### F. Self-contained-resource guard

Add a test/script that fails on console asset references requiring external network access.

At minimum inspect HTML/CSS/JS/resource manifests for:

- `http://`;
- `https://`;
- protocol-relative `//...` URLs in fetchable contexts;
- external module/script imports;
- `@import url(...)` to remote origins;
- `@font-face`;
- telemetry/analytics endpoints.

Do not make the guard a brittle generic repository grep; scope it to console browser resources and parse/allow known inert text where necessary.

## Work packages

### WP1 — Dependency and topology qualification

Freeze exact EggServe/Axum versions/features and prove the direct adapter on i2pr's toolchain/MSRV.

### WP2 — Console crate and embedded-resource foundation

Create `i2pr-console`, shell routes, resource table, base semantic CSS, and small vanilla JS only where needed.

### WP3 — Theme adapter and provenance gate

Implement bounded Halloy-compatible parsing and semantic CSS-variable generation. Import the EggPool 50-theme set only after provenance passes.

### WP4 — Daemon service composition

Add disabled-by-default loopback config and supervised EggServe startup through `TowerToEggserve`.

### WP5 — Boundary/static guards and docs

Freeze no-external-assets, no-second-server, no-router-owner dependency, and console-vs-eepsite trust-zone invariants.

## Failure, cancellation, restart, and contention semantics

- invalid/non-loopback console config fails before bind;
- bind failure fails only console startup according to the feature's declared optional-service policy and must not partially start an asset server;
- request admission/body limits are bounded by EggServe;
- handler failure returns sanitized browser errors and must not leak Rust/internal/control details;
- daemon cancellation stops accept, drains within a finite timeout, then terminates remaining console work;
- console restart creates fresh application/theme state; no session persistence exists yet;
- theme parse failure selects the safe default and emits bounded diagnostics without raw untrusted content;
- asset lookup is O(1)/bounded over a fixed table;
- no unbounded request queue or background refresh loop is introduced.

## Compatibility and migration

The console is a new disabled-by-default feature, so no existing wire/config behavior changes when it remains disabled.

Adding `[console]` must preserve old config files through defaults.

No SAM/I2CP/I2PControl/service-tunnel behavior changes.

If EggServe or Axum cannot satisfy i2pr's MSRV/dependency gates at implementation time, stop rather than silently switching to a second HTTP runtime. Re-research and amend ADR 0034 through a new decision record.

## Required tests

At minimum:

### Dependency/adapter

- `eggserve-server` Tower feature builds on pinned and MSRV toolchains;
- an Axum `Router` serves a real request through EggServe on loopback;
- shutdown/control completion works through the chosen API;
- a deliberately streaming response is not collected before write.

### Config/runtime

- defaults disabled;
- default bind is loopback;
- IPv4 loopback accepted;
- IPv6 loopback accepted;
- wildcard/LAN/public addresses rejected;
- enable -> bind/readiness -> request -> graceful shutdown works;
- disabled mode binds nothing.

### Assets

- every declared asset returns exact content type/body;
- unknown asset 404 is bounded;
- no runtime filesystem dependency;
- self-contained-resource guard passes;
- no external fonts/scripts/styles;
- CSP-compatible layout: no required inline script/style.

### Themes

- bounded parser max/max+1;
- all provenance-cleared bundled themes parse;
- invalid color/value types fail/fallback safely;
- semantic CSS variables are deterministic;
- malicious TOML strings cannot emit arbitrary CSS;
- core contrast/readability tests pass or trigger deterministic adjustment/fallback;
- selected theme name is bounded and must match the compiled inventory.

### Structural

- `i2pr-console` dependency graph contains no NetDB/tunnel/router-owner crates;
- no `axum::serve`;
- no console Hyper server;
- no `eggserve-core` unless a separately documented requirement appears;
- no shared console/eepsite route tree exists;
- support inventory unchanged.

## Exact verification commands

Focused commands should include:

~~~text
cargo fmt --all --check
cargo check --locked -p i2pr-console --all-targets
cargo test --locked -p i2pr-console --all-targets -- --test-threads=1
cargo check --locked -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-daemon console -- --test-threads=1
cargo clippy --locked -p i2pr-console -p i2pr-daemon --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-console -p i2pr-daemon --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
~~~

Add and run the new scoped console-resource/boundary checker.

Closure must also run the repository routine floor from `AGENTS.md`.

## Documentation updates

Required:

- `docs/architecture/overview.md`;
- `docs/architecture/dependency-graph.md`;
- new `docs/architecture/i2pr-console.md`;
- daemon config/architecture docs;
- `plans/subsystems/router-console-roadmap.md`;
- `plans/registry.md`;
- `plans/closure/router-console/356-status.md` at closure;
- dependency/license/provenance documentation for EggServe/Axum/theme assets.

Do not edit `specs/support.toml` except to clarify that console availability is not protocol support; no support value should be promoted.

## Acceptance criteria

Plan 356 passes only when:

1. `i2pr-console` exists as a distinct browser/application crate;
2. the daemon owns a disabled-by-default loopback-only console listener;
3. EggServe direct H1 is the sole console server runtime;
4. Axum routing executes through `TowerToEggserve`;
5. there is no `axum::serve`/second Hyper server;
6. the shell and all browser assets are compiled into the package;
7. normal rendering makes zero external-resource requests;
8. no custom font dependency exists;
9. the Halloy-compatible theme adapter is bounded and deterministic;
10. imported EggPool themes, if present, have exact provenance/license evidence;
11. theme CSS cannot be arbitrary source-controlled string injection;
12. console and future eepsite server boundaries are statically/documentarily distinct;
13. startup/shutdown/admission are supervised and bounded;
14. no router state/control access is introduced yet;
15. no router support claim changes;
16. the full routine floor passes.

## Stop conditions

Stop and register a corrective/decision follow-up if:

- the current published EggServe direct server cannot compose with current Axum on i2pr's toolchain;
- the only workable integration requires `eggserve-core` broad features without a concrete reason;
- the console would need a non-loopback bind;
- router-owner handles are required to render the shell;
- runtime assets require a filesystem directory or external origin;
- theme licensing/provenance is ambiguous and implementation is about to copy Halloy upstream;
- the implementation attempts to share a listener/router with an I2P-facing eepsite;
- dependency changes violate MSRV, supply-chain, or unsafe policy.

## Closure evidence required

The closure record must include:

- exact EggServe/Axum resolved versions/features and dependency review;
- direct adapter real-socket test evidence;
- binary/package asset inventory and external-resource scan;
- console bind validation matrix;
- startup/readiness/shutdown evidence;
- `i2pr-console` dependency graph;
- theme provenance manifest result and bundled-theme count;
- theme parse/contrast/fallback evidence;
- console-vs-eepsite trust-zone architecture assertion;
- full routine-floor results;
- support/config diff;
- unblock audit for Plan 357.

## Handoff notes

Do not attach router state in Plan 356. A visually complete static shell is enough.

The implementation should look structurally like the eventual console, but correctness of the HTTP/application/theme boundary is the milestone. Plan 357 owns browser authentication and web-security hardening; Plan 358 owns the first Proposal-170-backed data.
