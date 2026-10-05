# Plan 345 — managed native app runtime foundation and capability contract

Status: **in-progress-managed-native-app-runtime-contract-foundation**.

Classification: **invariant + infrastructure**. This plan establishes a security/ownership boundary and a runtime-neutral application ABI. It does **not** establish a user-visible application capability, process sandbox, firewall, package installer, console, or anonymity claim.

Hard dependencies: none. The plan deliberately does not depend on Proposal 170's present completion state. It defines a scoped control-capability vocabulary and adapter boundary only; a later runtime plan may bind that vocabulary to the canonical completed Proposal 170 implementation.

Roadmap: `plans/subsystems/managed-native-app-runtime-roadmap.md`.

Canonical references:
- `GUARDRAILS.md`
- `specs/CONFORMANCE.md`
- `specs/support.toml`
- `docs/security-model.md`
- `docs/architecture/overview.md`
- `docs/architecture/dependency-graph.md`
- `docs/architecture/i2pr-api.md`
- `docs/architecture/i2pr-i2pcontrol.md`
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`

## Objective

Land one bounded foundation that downstream application-runtime and application developers can target without importing router internals:

1. freeze the native-app trust/process/permission architecture in durable ADR authority;
2. add a runtime-neutral `i2pr-app-proto` crate containing the versioned application/admin protocol vocabulary, principals, capabilities, bounded framing/control messages, manifest/UI descriptors, network-policy model, resource ceilings, and sandbox-attestation vocabulary;
3. enforce by static dependency/runtime checks that the contract crate cannot open sockets, spawn processes, touch the filesystem, own Tokio, or reach router internals;
4. prove the default-deny and privilege-separation semantics in deterministic tests.

The outcome is an API/contract foundation only. No process is launched and no application is usable at Plan 345 closure.

## Why this plan is ready

The router already has stable architectural seams that the future runtime can adapt rather than duplicate:

- `i2pr-api` owns SAM/I2CP protocol/state without owning sockets.
- `i2pr-i2pcontrol` owns Proposal 170 wire/domain semantics without owning live router state.
- `i2pr-daemon` is the sole composition layer that sees those contracts plus live router owners.
- `i2pr-runtime` already has a strict router-runtime ownership boundary that should not absorb a desktop/application sandbox.
- the planning system supports a parallel workstream that does not gate M12/mainline.

No OS sandbox decision is required to implement a portable contract describing the properties a future backend must attest. OS-specific mechanism selection is explicitly out of scope and remains a later milestone.

## Current implementation evidence

At registration:
- no `i2pr-app-proto` workspace crate exists;
- there is no `AppId`, `AppInstanceId`, `AppPrincipal`, or app-owned resource vocabulary;
- there is no application capability/grant model;
- there is no app-manager administrative protocol;
- there is no managed-app package manifest;
- there is no managed-app network policy;
- there is no sandbox attestation vocabulary;
- SAM/I2CP/Proposal 170 are reachable only through their existing product surfaces, not an inherited app capability channel;
- no application runtime or native app process is part of the product.

These absences are the baseline. Do not treat existing loopback listeners as the managed-app transport.

## Invariants that must not regress

1. **Router/user-space separation.** No managed-app contract type exposes `RouterContext`, NetDB stores, transport managers, tunnel pools, destination secret owners, `ServiceProduct`, or other router internals.
2. **Runtime neutrality.** `i2pr-app-proto` owns no Tokio, socket, timer, process, filesystem, DNS, TLS, dynamic-loader, or OS sandbox implementation.
3. **Principal separation.** Persistent app identity, launch-instance identity, publisher identity, router identity, and I2P Destination identity are distinct types/domains. No conversion reuses router/destination private material as a publisher/app identity.
4. **Requested != granted.** Package/app permission requests are inert data. Only an administrator principal can create/revoke effective grants.
5. **No application self-promotion.** The application-channel message vocabulary contains no operation that grants permissions, disables network policy, switches to direct host networking, installs packages, or mutates another app.
6. **Default direct network deny.** The pure policy default denies direct public, LAN/private, link-local, multicast, unspecified, loopback, and DNS/network resolver access. I2P router services are separate capabilities, not native network exceptions.
7. **Loopback is not trusted.** Nothing in the contract treats `127.0.0.0/8`, `::1`, local Unix sockets, named pipes, or localhost services as an implicit safe networking path.
8. **Scoped control.** The future app control surface never implies possession of the administrator Proposal-170 credential; resource-owning mutations are scoped to the app principal.
9. **UI is not networking.** UI descriptors refer only to package-relative static resources. No URL/host/port field is a valid UI entrypoint in v1.
10. **Bounded hostile input.** Every identifier, frame, list, map, manifest field, capability set, rule set, resource request, UI message, and diagnostic string has an explicit ceiling and max+1 test.
11. **Feature isolation.** Merely adding the contract crate changes no listener, route, router advertisement, startup behavior, persisted router state, or public protocol claim.

## Scope

### In scope

- durable ADR(s) for the managed native-app architecture and security boundary;
- new runtime-neutral `crates/i2pr-app-proto`;
- exact v1 protocol/reference document committed before implementation code that depends on its spellings/shapes;
- app/admin handshake roles and protocol version negotiation;
- app/publisher/instance identity value types and validation;
- capability/request/grant vocabulary;
- app principal and app-owned resource reference vocabulary;
- bounded multiplexed frame envelope suitable for opaque SAM/I2CP/application data plus typed control messages;
- distinct application-channel and AppManager/admin message enums;
- manifest v1 structural schema for downstream packages;
- package-relative UI descriptor and bounded UI command/event envelope;
- deterministic network-policy model for future broker evaluation;
- launch-profile/sandbox-requirement and sandbox-attestation vocabulary;
- resource-request/ceiling vocabulary;
- strict typed errors;
- dependency/runtime guard updates;
- focused architecture/docs updates;
- contract tests, malformed/boundary tests, and decoder fuzz coverage consistent with existing repository practice.

### Explicitly out of scope

- native process creation;
- Linux namespaces/seccomp/Landlock/cgroups implementation;
- Windows AppContainer/Job Object/WFP implementation;
- macOS App Sandbox/XPC/Seatbelt implementation;
- DNS resolution or clearnet socket opening;
- firewall mutation;
- SAM or I2CP transport adaptation;
- Proposal-170 request dispatch/adaptation;
- package signature verification or online repository/update protocol;
- package extraction/installation;
- application data directories;
- console implementation or webview selection;
- application SDK convenience wrappers beyond the protocol crate itself;
- first-party or third-party application implementation;
- public-network or anonymity qualification.

If implementation requires any of the above to make the contract tests pass, stop and register the appropriate successor instead of smuggling runtime behavior into Plan 345.

## Required production changes

### 1. Freeze the architecture in ADR authority

Before adding production contract code, write the next available ADR(s) that decide:

- managed apps are separate processes, never in-process Rust plugins/dylibs in the router or console security domain;
- `i2pr-daemon` remains the router-side authority/composition point for router capabilities;
- OS application lifecycle/sandbox/network-broker ownership is a separate trusted user-space component boundary rather than new `i2pr-runtime` responsibility;
- the normal secured launch profile has no direct host networking, including loopback;
- I2P networking is provided through an inherited capability channel using adapters over the existing SAM/I2CP owners;
- Proposal 170 is administrative/control semantics and future app exposure is scoped/proxied rather than handing out the administrator credential;
- clearnet access, when later implemented, is brokered under user-owned default-deny policy;
- app-side permission requests cannot modify effective policy;
- direct-host networking is a distinct operator-selected relaunch profile, not an app-controlled dynamic transition;
- first-party apps use the same trust model as third-party apps by default;
- embedded app UI runs under a distinct untrusted principal/origin and cannot inherit console/router administrative authority.

The ADR must state the non-guarantee: preventing unauthorized direct clearnet/LAN/loopback egress does not make arbitrary malicious application code anonymous; an app may still encode identifying information into traffic it is legitimately allowed to send.

### 2. Add `i2pr-app-proto` as a bottom-layer contract crate

Add the workspace member with:
- `#![forbid(unsafe_code)]`;
- no `i2pr-*` production dependencies;
- only already-reviewed workspace dependencies where possible (expected: `serde`, `serde_json`, `thiserror`; justify any additional dependency before adding it);
- no default feature that opens runtime/OS functionality.

Update `scripts/check-dependency-direction.sh`, `scripts/check-runtime-boundaries.sh`, architecture inventory, and `AGENTS.md` so the boundary is executable rather than prose-only.

Expected modules may be reorganized if the public contract remains equivalent, but ownership should cover:

```text
i2pr-app-proto/
  ids
  version
  frame
  capability
  principal
  admin
  app_channel
  manifest
  network_policy
  ui
  sandbox
  resources
  limits
  errors
```

### 3. Freeze the v1 wire/control contract before implementation

Add a normative repository reference document defining the exact v1 public contract before code depends on it.

The transport is a bounded multiplexed byte stream. It must support:
- one connection/session version handshake;
- typed control frames;
- opaque data frames keyed by bounded `StreamId`;
- explicit open/accept/close/reset semantics;
- bounded frame payload size;
- bounded concurrent logical streams;
- bounded control in-flight requests;
- deterministic rejection of unknown/case-drifted v1 literals;
- reserved version/extension mechanism without accepting ambiguous unknown behavior.

The contract must be language-neutral. Do not make Rust enum serialization itself the compatibility specification.

The framing must allow future SAM/I2CP byte streams without base64/JSON inflation of application payload bytes. Control payload encoding may be JSON if strictly bounded and frozen; raw stream payload remains opaque bytes.

### 4. Define identity and principal domains

At minimum define validated/bounded:

- `AppId`: stable package/application namespace identifier;
- `PublisherId`: code provenance namespace, opaque to router identity;
- `AppVersion`;
- `AppInstanceId`: one launch instance; contract type only, generation policy belongs to runtime;
- `AppPrincipal`: app + instance (+ publisher attribution where required);
- `OwnedResourceId`: opaque resource handle that is always associated with its principal at the runtime boundary.

No type may imply that package publisher identity is an I2P Destination or router signing identity.

### 5. Define capabilities, requests, and grants as different types

Create closed v1 capability families sufficient for future composition without pretending the runtime exists. At minimum cover:

- I2P SAM access;
- I2P I2CP access;
- scoped Proposal-170/control access;
- brokered clearnet TCP request ability;
- UI bridge;
- health/lifecycle reporting;
- reserved filesystem/resource capabilities only where their semantics are frozen.

Separate:
- `RequestedCapability` (manifest/app asks);
- `GrantedCapability` (administrator-owned effective permission);
- `EffectiveCapabilities` (bounded read-only view for the app).

The application protocol may carry `PermissionRequest`/status. It may not carry `Grant`, `Revoke`, `DisableFirewall`, `SetDirectNetwork`, `Install`, `Update`, or equivalent administrator mutations.

### 6. Define two non-upgradable protocol roles

The v1 handshake must select exactly one role:

- `Application`;
- `Administrator` / AppManager.

They use disjoint message enums/surfaces. An application session cannot send an admin message by choosing a numeric tag or changing a field. Role mismatch is a typed protocol error and closes/refuses the request without side effect.

The administrator vocabulary may describe future lifecycle/policy operations structurally, but Plan 345 implements no owner and no side effect.

### 7. Define the manifest v1 structural contract

The manifest must be versioned, bounded, and strict. It must carry enough for downstream apps to develop against the future platform:

- schema version;
- `AppId`, human display metadata with ceilings;
- `PublisherId` attribution slot;
- app version and compatible host/app-protocol range;
- per-target executable entrypoint descriptors using package-relative paths;
- requested capabilities;
- bounded resource requests;
- optional static UI descriptor;
- restart/autostart preference as a request, not authority.

The v1 schema must not contain:
- installer/post-install/pre-install scripts;
- arbitrary shell commands;
- executable hooks beyond the declared application entrypoint;
- remote UI URLs;
- localhost UI URLs;
- an embedded effective-grant/firewall policy;
- router/admin credentials.

Package signature verification, package archive format, updater/TUF integration, extraction, and installation are later runtime work. Preserve raw/canonical manifest bytes or define a stable serialization seam so a later signature envelope can authenticate the manifest without changing its semantics.

### 8. Define pure default-deny network policy

Implement only deterministic policy data/evaluation — no socket/DNS I/O.

The default state is deny. Model at minimum:
- protocol: TCP now; reserve future UDP without claiming it;
- destination selector: exact hostname, exact IP, CIDR;
- bounded single port/range;
- address scopes: public, loopback, private/LAN, link-local, multicast, unspecified;
- action: allow/deny with deny taking precedence where applicable;
- direct-network launch profile as administrator-owned state outside app self-grants.

Requirements:
- loopback and LAN/private are denied unless explicitly allowed by administrator policy;
- an exact hostname rule does not imply permission for any resolved address scope that policy forbids;
- policy exposes a pure `evaluate_requested_target` and a pure post-resolution `evaluate_resolved_address` (names flexible) so a future broker must pass both gates;
- applications cannot add rules through the app protocol;
- DNS itself is not an app capability in the secured profile; future broker resolution belongs to the trusted runtime.

Do not create a generic host packet-filter DSL in this milestone.

### 9. Define UI bridge/resource contract

A manifest UI entrypoint is a validated package-relative resource path.

Reject:
- `http:`, `https:`, `file:`, `data:`, and other URL schemes;
- absolute paths;
- `..` traversal;
- host/port forms.

Define bounded typed `UiCommand` / `UiEvent` envelopes or an equivalent bounded app-defined message channel. This is a logical bridge only. Plan 345 does not choose Tauri/WebView/WebKit/CEF or implement rendering.

Document that the future console host must treat UI code as the same untrusted app principal: no admin cookie/token, no console DOM authority, no independent direct networking.

### 10. Define sandbox requirement and attestation vocabulary

Describe security properties, not OS mechanisms.

At minimum represent:
- direct network denied;
- loopback denied;
- private filesystem boundary;
- host process inspection denied/contained where supported;
- child-process tree containment;
- resource limits installed;
- environment sanitized;
- inherited broker channel installed;
- backend kind/version/evidence generation.

Define a `Secured` launch requirement whose mandatory properties cannot be satisfied by an attestation missing any required property. Define an explicit `UnsafeDirect`/equivalent operator profile separately; do not represent it as a partially passing secure attestation.

No platform backend implementation belongs in this plan.

### 11. Define resource ceilings

Freeze hard contract ceilings for:
- identifier lengths;
- manifest bytes/fields/list counts;
- capability count;
- network rules;
- frame bytes;
- logical streams;
- in-flight requests;
- UI message bytes;
- resource request values;
- attestation fields;
- diagnostic strings.

Requested limits are never effective limits. The future runtime clamps/denies them under host/operator maxima.

## Ordered work packages

### A. Architecture and normative freeze

1. Write ADR(s).
2. Add `specs/references/managed-native-app-runtime-v1.md` (or equivalent) with the exact v1 vocabulary, frame/control shapes, manifest schema, policy semantics, and security/non-claim boundary.
3. Record that Proposal 170 binding is an interface dependency for a later plan, not a Plan-345 capability.

No production code should precede the contract freeze commit.

### B. Crate and static boundary

1. Add `i2pr-app-proto`.
2. Add dependency/runtime allowlist rules.
3. Add architecture index/dependency-graph documentation.
4. Add a static guard that fails on forbidden runtime/OS primitives in this crate.

### C. Identity/capability/admin split

Implement the validated identifier/principal/capability types and the two role-specific message vocabularies. Add exhaustive tests proving application-role decoding cannot produce an administrator mutation.

### D. Bounded framing and control codec

Implement incremental/strict frame/control decode/encode with exact-consumption and max+1 behavior. Add golden vectors and malformed fixtures. No socket adapter.

### E. Manifest/UI contract

Implement strict manifest and package-relative UI descriptors plus negative tests for scripts/URLs/traversal/oversize/unknown fields.

### F. Pure policy + sandbox/resource vocabulary

Implement default-deny network decisions, post-resolution scope checks, launch requirement/attestation validation, and resource request/ceiling models. No DNS/socket/process call.

### G. Documentation and evidence

Update architecture docs, roadmap/registry status at closure, and the subsystem evidence record. Add fuzz/property coverage appropriate for every hostile decoder.

## Failure, cancellation, restart, and contention semantics

Plan 345 has no tasks, processes, sockets, or persistent mutable owner, so runtime cancellation/restart semantics are deliberately absent.

Contract requirements for future owners must nevertheless be explicit:

- truncated/malformed/oversized frame or manifest input returns typed error without panic or partial side effect;
- role/capability violation is rejected before a future owner is invoked;
- unknown v1 capability/message literals are fail-closed, not ignored into privilege;
- duplicate manifest/control fields are rejected where ambiguity could affect authority;
- resource-limit arithmetic uses checked/saturating policy as appropriate and cannot wrap;
- a future restart must be able to distinguish package request, persisted administrator grant, and current effective capability; Plan 345 must model these as separate types now;
- concurrent logical stream identifiers must be unique within one session; duplicate open is an error;
- stream close/reset is idempotently representable and bounded; no retry loop exists in this crate.

## Compatibility and migration

This is v1 and has no deployed compatibility obligation.

Still:
- include explicit protocol and manifest schema versions from the first commit;
- reject unsupported major versions;
- avoid exposing Rust layout/enum discriminants as wire ABI;
- reserve extension/version negotiation deliberately rather than "ignore unknown";
- package manifest and app wire versions are distinct so either can evolve independently;
- publisher/app IDs are stable textual identifiers while launch-instance/resource IDs are opaque runtime values;
- no router on-disk identity/storage format changes are authorized.

A later package-signature plan must authenticate a stable manifest representation or raw manifest bytes without requiring semantic reinterpretation of v1 fields.

## Required tests

At minimum:

### Identifier/principal
- min/max/max+1;
- ASCII/normalization/case rules exactly per frozen contract;
- invalid separator/control/unicode cases according to chosen grammar;
- app/publisher/instance types cannot be confused through generic parsing.

### Role/capability separation
- app role accepts only app messages;
- admin role accepts only admin messages;
- numeric/tag substitution cannot decode an admin operation on app role;
- permission request is inert;
- no effective grant can be constructed from untrusted manifest/app bytes without explicit administrator-origin constructor/owner seam.

### Frame/control
- exact golden encoding;
- fragmented incremental decode;
- multiple frames;
- zero/max/max+1 payload;
- truncation at every header boundary;
- unknown version/kind/flags;
- duplicate stream open;
- bounded stream IDs/in-flight requests;
- arbitrary bytes never panic.

### Manifest/UI
- valid minimal/maximal manifest;
- unknown/duplicate fields;
- max+1 every bounded collection/string;
- target entrypoint traversal/absolute/scheme rejection;
- UI remote URL/localhost/file/data URL rejection;
- manifest cannot encode effective grant/network policy/admin credential/install script.

### Network policy
- empty/default denies every address scope;
- IPv4 and IPv6 loopback denied;
- RFC1918/ULA/link-local/multicast/unspecified denied;
- exact public IP/CIDR allow positive/negative boundaries;
- explicit loopback/LAN grant works only when administrator policy contains it;
- hostname request gate plus post-resolution public address passes when allowed;
- same allowed hostname resolving to loopback/private/link-local is denied unless that scope is separately authorized;
- deny precedence and port/range boundaries;
- app message cannot mutate rules or direct-network profile.

### Sandbox/resource contract
- secured requirement rejects each missing required property one at a time;
- unsafe/direct profile cannot serialize/report itself as secured;
- resource requests clamp/fail according to frozen semantics and never become grants;
- attestation strings/counts max+1.

### Static boundaries
- `i2pr-app-proto` has no forbidden `i2pr-*` production edge;
- guard catches inserted socket/process/filesystem/Tokio/dynamic-loader spellings;
- positive-control fixture proves the guard itself is not vacuous.

## Exact verification commands

Focused:

```text
cargo fmt --all --check
cargo check --locked -p i2pr-app-proto --all-targets
cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1
cargo clippy --locked -p i2pr-app-proto --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-proto --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Routine closure floor:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
```

If the plan adds fuzz targets under the repository fuzz workspace, compile/run the corresponding existing fuzz-smoke mechanism and record whether the host/toolchain supports it. Missing optional fuzz tooling may not be relabeled as a fuzz pass.

## Documentation updates

At implementation/closure:
- new ADR(s);
- `specs/references/managed-native-app-runtime-v1.md`;
- `docs/architecture/overview.md`;
- `docs/architecture/dependency-graph.md`;
- new `docs/architecture/i2pr-app-proto.md`;
- `AGENTS.md`;
- this roadmap §7/§12;
- `plans/registry.md`;
- closure record.

Do not update `specs/support.toml` with a user-visible application capability. Plan 345 is infrastructure only.

## Acceptance criteria

Plan 345 passes only when all of the following are true:

1. durable ADR authority records the process/trust/network/admin/UI split and explicit non-guarantees;
2. the v1 app protocol/manifest/policy contract is frozen before dependent production code;
3. `i2pr-app-proto` is in the workspace and has no `i2pr-*` production dependency;
4. dependency/runtime guards mechanically preserve its no-I/O/no-process/no-Tokio boundary;
5. app and administrator roles are different protocol surfaces and an app session cannot decode/construct an administrator policy mutation;
6. requested, granted, and effective capabilities are distinct types with tests proving untrusted package/app bytes cannot self-grant;
7. default network policy denies public/LAN/link-local/multicast/unspecified/loopback and models hostname post-resolution scope re-checks;
8. no application API exists to disable firewall/direct-network policy;
9. manifests are strict/bounded and cannot contain executable install hooks, remote/localhost UI endpoints, or effective grants;
10. UI resources are package-relative and the UI message bridge is bounded;
11. secured sandbox requirements/attestations are represented without claiming an OS backend exists;
12. frame/control decoders pass golden, malformed, truncation, max+1, and no-panic tests;
13. the full routine floor is green;
14. no socket, listener, process launch, filesystem owner, DNS resolver, clearnet connection, router advertisement, app implementation, or sandbox-capability claim lands.

Closure of Plan 345 makes successor planning dependency-ready; it does not by itself make any native application safe to execute.

## Stop conditions

Stop and record a blocker/corrective rather than broadening scope if:

- a portable contract cannot represent a required OS security property without embedding one OS mechanism;
- SAM/I2CP must be reimplemented rather than adapted in a later layer;
- the contract would require handing a general Proposal-170 administrator token to an app;
- a capability/grant distinction cannot be made mechanically enforceable;
- package manifest semantics require choosing/signing/verifying a package archive format in this plan;
- the only way to support UI is a localhost HTTP server;
- an external dependency with new `unsafe`/license/transitive risk is needed and has not received the repository dependency review;
- an implementation change would alter current router protocol support/advertisement.

## Closure evidence required

The closure record must include:
- implementation commits;
- ADR/spec freeze commit preceding dependent production code;
- requirement-to-evidence matrix covering every acceptance criterion;
- exact focused and routine-floor command results;
- dependency diff/review;
- static-guard teeth tests including a positive control;
- manifest/frame golden fixture identifiers/hashes where applicable;
- security review of authority escalation, confused-deputy, loopback/LAN/DNS policy, UI-origin, and identity-correlation boundaries;
- compatibility/migration review;
- explicit statement that no app process/sandbox/network containment capability exists yet;
- unblock audit identifying which successor class is genuinely ready.

## Handoff notes

Keep this pass contract-first and small enough to review. Do not implement the obvious next layers "while here." In particular, no `Command::spawn`, socketpair/pipe creation, network namespace/AppContainer/App Sandbox code, DNS, or clearnet connect belongs in Plan 345.

The most important review question is not whether the API is convenient. It is whether an actively hostile downstream application can ever turn application-controlled bytes into an increase of its own authority. Design every v1 type and message around making that transition impossible without the separate administrator principal.
