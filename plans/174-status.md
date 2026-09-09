# Plan 174 status — M10 service-tunnel foundation and shared stream runtime

Status: **`passed-m10-service-tunnel-foundation-and-shared-stream-runtime`**.

Plan of record:
[`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md).

Roadmap authority:
[`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md)
([`plans/173-status.md`](173-status.md)).

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = not-yet-passed
milestone10_http_proxy = not-yet-passed
milestone10_socks5 = not-yet-passed
milestone10_irc_client = not-yet-passed
milestone10_irc_server = not-yet-passed
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 175
next_product_layer = milestone10-service-tunnels
```

## What landed

```text
crates/i2pr-service-tunnels/ (new, runtime-neutral)
  Cargo.toml (i2pr-proto + thiserror only; no Tokio/sockets)
  src/lib.rs
  src/config.rs (kinds, policy, listener/target, limits, timeouts, set)
  src/destination.rs (Base32/alias/configured + alias table)
  src/errors.rs
  src/events.rs

crates/i2pr-daemon/src/destination_streaming.rs (new)
  generic bounded socket<->Streaming pump (AsyncRead+AsyncWrite)
  + 5 deterministic mock-endpoint tests

crates/i2pr-daemon/src/sam/raw_stream.rs (adapted)
  thin SamPumpEndpoint over the shared primitive; no second pump

crates/i2pr-daemon/src/config.rs (+ [service_tunnels])
  strict disabled-by-default loopback-only surface;
  enabled tunnels rejected as not-yet-available

crates/i2pr-daemon/tests/service_tunnels_foundation.rs (new, 4 tests)
specs/protocols/11-service-tunnels.md (new M10 dossier)
docs/architecture/i2pr-service-tunnels.md (new deep-dive)
```

No generic/HTTP/SOCKS/IRC listener is active. No I2P wire-format
or destination-routing semantic change was introduced.

## Acceptance checklist (Plan 174 §13)

1. `i2pr-service-tunnels` exists and is runtime-neutral — **passed**
   (`#![forbid(unsafe_code)]`, boundary script proves no Tokio or
   listener ownership).
2. Every count/length/deadline has a hard typed ceiling — **passed**
   (32 services, 64 aliases, 64-byte IDs, 128/1024 conns,
   1024–1048576 buffered bytes, 8 targets, connect/read/write/
   shutdown ranges).
3. Base32/static alias parsing strict + malformed tests — **passed**
   (52-char `a-z2-7` + `.b32.i2p`, lower-case `.i2p`, negative
   matrix in `destination.rs`).
4. Daemon config strict, disabled by default, loopback-only — **passed**
   (`deny_unknown_fields`, loopback checks, duplicate binds,
   router-wide budget guards).
5. Shared pump no longer depends on SAM state — **passed**
   (`destination_streaming.rs` depends only on the
   `StreamPumpEndpoint` trait + Tokio IO + cancellation).
6. SAM reuses the shared primitive — **passed** (`raw_stream.rs`
   delegates to `run_stream_pump`; duplicate loop removed).
7. Bidirectional/segmentation/backpressure/isolation/EOF/cancel
   tests — **passed** (5 pump tests + crate/daemon focused tests).
8. No generic/HTTP/SOCKS/IRC listener active — **passed** (graph
   contains only lifecycle/netdb-bootstrap (+ sam/i2cp when
   enabled); foundation test asserts no service/http/socks/irc
   service).
9. No wire/routing semantic change — **passed** (SAM/I2CP
   regressions green; no proto/client wire edits).
10. Plan 151/152 SAM + Plan 172 I2CP regressions green — **passed**
    (see evidence below).
11. Dependency/runtime boundary scripts prove the graph — **passed**
    (`check-dependency-direction.sh` explicit edge,
    `check-runtime-boundaries.sh` runtime-neutral enforcement).
12. Full workspace floor + routine CI — **passed** (1781 passed,
    1 ignored across 71 suites; see evidence).
13. Status advances `next_executable_plan = 175` — **this record**.

## Evidence (exact commands, closing head)

Closing source floor: implementation commit (see git log); routine
CI must be green on the exact closing head before Plan 175 begins.

Full workspace (single-threaded, as required):

```text
cargo test --locked --workspace --all-targets --offline -- --test-threads=1
# 1781 passed, 1 ignored (71 suites)
# the single ignored test is the Plan 162-gated
# ssu2_independent_ipv4_interop external test (routine lane ignores,
# dedicated lane selects with --ignored --exact)
```

Focused crate:

```text
cargo test -p i2pr-service-tunnels --all-targets --offline
# 19 passed
```

Shared pump:

```text
cargo test -p i2pr-daemon --lib destination_streaming --offline
# 5 passed: small bidirectional, multi-segment, backpressure,
# sibling isolation, cancel/half-close
```

Daemon config:

```text
cargo test -p i2pr-daemon --lib config --offline
# 51 passed (44 retained + 7 new service-tunnel rows)
```

Foundation black-box:

```text
cargo test -p i2pr-daemon --test service_tunnels_foundation --offline
# 4 passed: disabled-by-default graph unchanged, non-loopback
# listener rejected, non-loopback target rejected, enabled
# rejected as not-yet-available
```

SAM retained:

```text
cargo test -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
# 4 passed
cargo test -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
# 10 passed
```

I2CP retained:

```text
cargo test -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
# 12 passed
```

Static boundaries:

```text
bash scripts/check-dependency-direction.sh
# dependency direction: ok
bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed
```

Full floor (run from repository root before handoff; see handoff
for the exact closing-head results):

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

## Handoff

Execute Plan **175** next (generic client/server tunnels +
persistent server identities). Do not begin Plan 176 until Plan
175 has an explicit passing status record. Do not implement HTTP,
SOCKS5, or IRC profiles early.

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
milestone10_foundation = passed-via-plan174
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 175
next_product_layer = milestone10-service-tunnels
```
