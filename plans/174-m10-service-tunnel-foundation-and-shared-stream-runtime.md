# Plan 174 — Milestone 10 service-tunnel foundation and shared Streaming runtime

Status: **blocked until Plan 173 is registered; first executable M10 implementation pass**.

## 1. Goal

Land the architectural foundation required by every M10 application tunnel without adding an HTTP, SOCKS, IRC, or generic service listener yet.

Required result:

```text
runtime-neutral i2pr-service-tunnels crate
+ strict bounded M10 config model
+ shared daemon destination/Streaming runtime capability
+ generic bounded socket<->Streaming pump primitive
+ SAM requalified on the shared primitive
```

This plan is a refactor/foundation pass. It must not change I2P wire semantics or broaden listener exposure.

## 2. Read first

- `plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`
- `plans/172-status.md`
- `GUARDRAILS.md`
- `crates/i2pr-client/src/streaming/mod.rs`
- `crates/i2pr-client/src/streaming_adapter.rs`
- `crates/i2pr-daemon/src/sam.rs`
- `crates/i2pr-daemon/src/sam/raw_stream.rs`
- `crates/i2pr-daemon/src/config.rs`
- `scripts/check-dependency-direction.sh`
- `scripts/check-runtime-boundaries.sh`

## 3. Create `i2pr-service-tunnels`

Add the workspace member:

```text
crates/i2pr-service-tunnels/
  Cargo.toml
  src/lib.rs
  src/config.rs
  src/destination.rs
  src/errors.rs
  src/events.rs
```

The crate must use `#![forbid(unsafe_code)]` and remain runtime-neutral.

Allowed dependencies should be minimal and follow the existing graph. Prefer `i2pr-client` public destination/Streaming-facing types, `i2pr-proto` public Destination/Hash types, and `thiserror`. Do not add Tokio, socket libraries, HTTP frameworks, SOCKS libraries, or parser frameworks in this plan.

Update the dependency-direction checker so the intended edge is explicit, not accidentally allowed by omission.

## 4. Runtime-neutral service model

Implement bounded typed identifiers/configuration primitives, at minimum:

```text
ServiceTunnelId
ServiceTunnelKind
  GenericClient
  GenericServer
  HttpClient
  Socks5Client
  IrcClient
  IrcServer

DestinationPolicy
  Dedicated
  SharedClientGroup(ServiceClientGroupId)

DestinationRef
  Base32Hash
  StaticAlias
  ConfiguredDestination

LocalListenerSpec
ServerTarget
  LoopbackTcp(SocketAddr-like runtime-neutral data)
  UnixPath(public path representation; Unix validation occurs in daemon)

ServiceResourceLimits
ServiceTimeouts
ServiceTunnelSpec
ServiceTunnelSet
```

Hard ceilings must live centrally and include:

- service count;
- alias count;
- service/group ID length;
- active connections per service;
- aggregate active connections;
- buffered bytes per direction;
- connect/read/write/shutdown deadline ranges;
- maximum configured target list count.

`ServiceTunnelSet::validate()` must reject duplicate IDs and internally contradictory options before daemon state changes.

## 5. Destination reference policy

Implement structural parsing/validation only:

- canonical 52-character I2P Base32 host label plus `.b32.i2p` suffix;
- bounded lower-case `.i2p` static alias names;
- bounded static alias table with duplicate/conflict rejection;
- no DNS lookup;
- no filesystem lookup;
- no network lookup;
- no implicit clearnet fallback.

Resolution to a LeaseSet/remote runtime remains a daemon/client composition operation.

Negative tests:

- wrong Base32 alphabet/length;
- mixed suffix tricks such as `.i2p.example.com`;
- embedded NUL/control/whitespace;
- overlong alias;
- duplicate aliases;
- alias pointing to malformed destination reference;
- IP literals rejected for I2P destination references.

## 6. Daemon configuration surface

Extend strict configuration with a disabled-by-default service-tunnel section. Use a structure that can grow without protocol-specific untyped option bags.

Suggested shape:

```toml
[service_tunnels]
enabled = false
max_active_connections = 128

[[service_tunnels.tunnel]]
id = "example"
kind = "generic-client"
enabled = false
# kind-specific fields...
```

Requirements:

- `serde(deny_unknown_fields)` throughout;
- loopback-only local listeners in M10;
- numeric and duration ceilings enforced during semantic normalization;
- duplicate bind endpoints detected before listener creation;
- generic server TCP target must be loopback;
- Unix target allowed only as a normalized path value; actual platform/socket validation later;
- no raw private destination material in TOML.

No service listener starts in this plan even if config contains a future enabled entry. Until Plan 175, semantic validation should reject/mark executable service kinds as not-yet-available rather than silently ignoring them.

## 7. Extract shared daemon Streaming runtime

The main engineering work is to separate the proven SAM-specific socket pump from SAM protocol state.

Introduce a daemon module with an explicit narrow capability, e.g.:

```text
crates/i2pr-daemon/src/destination_streaming.rs
```

The exact names may differ, but the ownership boundary must be clear.

Required reusable components:

### 7.1 Destination Streaming handle

A handle/capability that owns or references:

- one router-owned destination runtime;
- one `StreamingManager`;
- existing `StreamingDestinationAdapter` routing seam;
- per-destination driver cancellation/notification;
- established-state notification;
- bounded delivery accounting.

Do not expose an `Arc<RouterContext>` service locator.

### 7.2 Generic raw byte pump

Extract the protocol-agnostic part of `sam/raw_stream.rs` into a reusable function/type that is not SAM-aware.

Inputs should be capabilities, not `SamServiceState`. It should receive enough to:

- write local bytes into exactly one Streaming connection;
- drain bytes for exactly one Streaming connection;
- observe terminal state;
- wake destination delivery driver;
- honor cancellation.

Socket ownership remains daemon-side. A helper may be generic over Tokio `AsyncRead + AsyncWrite` if that makes TCP and Unix targets share the same tested pump without unsafe code.

Retain these Plan 147/151 properties:

- bounded per-read chunk;
- negotiated Streaming segmentation;
- send-window backpressure without busy spin;
- sibling-stream isolation;
- fair opportunity for ACK/delivery driver progress;
- cancellation/EOF/remote terminal close convergence;
- no command parser retaining the raw socket after handoff.

### 7.3 SAM adaptation

Refactor SAM to use the shared primitive while preserving:

- existing session/stream ownership;
- SILENT semantics;
- ACCEPT Destination metadata;
- FORWARD behavior;
- same-read command+raw bytes;
- Plan 151 fault tests;
- Plan 152 ACK/receiver-window corrections.

Do not move SAM protocol parsing into the new service-tunnel crate.

## 8. No new product listener yet

This plan must not add:

- generic service listeners;
- HTTP proxy listener;
- SOCKS listener;
- IRC listener;
- server destination persistence;
- public-network activity.

The only listeners exercised are retained SAM/I2CP regressions.

## 9. Tests

Add focused tests for:

1. all service kinds parse as typed enum values;
2. strict bounds/duplicate validation;
3. Base32/static alias validation and negative matrix;
4. non-loopback local listener rejected;
5. non-loopback server TCP target rejected;
6. disabled-by-default service-tunnel config leaves daemon graph unchanged;
7. shared raw pump exact-once small and multi-segment bidirectional bytes using a deterministic/mock destination Streaming capability or existing local bridge seam;
8. pump backpressure with a stalled local reader;
9. sibling stream A cannot drain B;
10. cancellation and half-close release resources;
11. SAM canonical self-composed test passes unchanged;
12. SAM final acceptance suite passes unchanged;
13. I2CP Plan 172 focused lifecycle remains green.

Do not validate the pump only by calling the original SAM wrapper if that would leave the extracted primitive itself untested.

## 10. Static boundaries

Extend `check-runtime-boundaries.sh` / dependency checker as appropriate to enforce:

- no `tokio` dependency in `i2pr-service-tunnels`;
- no `std::net`/`tokio::net` listener ownership in that crate;
- no transport-specific crate dependency;
- no production dependency on `i2pr-testkit`;
- daemon is the only new M10 socket/task owner.

Prefer structural dependency checks over brittle grep where Cargo metadata can prove the invariant.

## 11. Documentation

Update at closure:

- `docs/architecture/overview.md` / relevant daemon/client docs;
- create `docs/architecture/i2pr-service-tunnels.md`;
- create or initialize `specs/protocols/11-service-tunnels.md` with Plan 173 reference basis and explicit not-yet-implemented product rows;
- `specs/support.toml` with Plan 173/174 authority only after evidence passes;
- `plans/174-status.md`.

## 12. Validation commands

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-client --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
cargo deny check advisories bans sources
```

## 13. Acceptance criteria

Plan 174 passes only when:

1. `i2pr-service-tunnels` exists and is runtime-neutral.
2. Every service-tunnel count/length/deadline has a hard typed ceiling.
3. Base32/static alias parsing is strict and has malformed tests.
4. Daemon config is strict, disabled by default, and loopback-only for local listeners.
5. The shared raw byte pump no longer depends on SAM protocol state.
6. SAM uses/reuses the shared primitive rather than retaining a second duplicate byte pump.
7. Bidirectional, segmentation, backpressure, sibling isolation, EOF/cancel cleanup tests pass.
8. No generic/HTTP/SOCKS/IRC listener is active yet.
9. No I2P wire-format or destination-routing semantic change is introduced.
10. Plan 151/152 SAM and Plan 172 I2CP focused regressions remain green.
11. Dependency/runtime boundary scripts prove the intended graph.
12. Full workspace floor and exact-head routine CI pass.
13. `plans/174-status.md` records exact evidence and advances `next_executable_plan = 175`.

## 14. Stop conditions

Stop and write a narrow corrective instead of forcing closure if:

- extracting the pump exposes a real M6 Streaming correctness defect;
- SAM can only remain green by duplicating the pump or retaining a hidden private delivery bypass;
- the new service crate requires Tokio/socket ownership;
- destination/runtime access would require a global unrestricted context object;
- a new dependency with significant unreviewed unsafe/untrusted parsing is proposed.

## 15. Handoff

Expected passing authority:

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
milestone10_foundation = passed-via-plan174
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 175
next_product_layer = milestone10-service-tunnels
```