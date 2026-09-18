# Plan 139 status — SAM 3.1 forward/naming hardening

Status: **passed-m7-sam31-forward-naming-hardening**

Plan 139 is locally closed. It keeps SAM experimental, loopback-only, and
non-advertised. Plan 140 remains responsible for independent-client
interoperability and the remaining Milestone 7 external acceptance evidence.

## Delivered behavior

- `STREAM FORWARD` requires a valid session ID and port, accepts only
  loopback numeric hosts or `localhost`, rejects `SSL`, and returns
  `STREAM STATUS RESULT=OK` while keeping the control socket as owner.
- Forward ownership is removed on control-socket EOF, session teardown, or
  daemon shutdown. `InboundMode` atomically excludes pending `ACCEPT` from
  `FORWARD` and vice versa.
- Forwarded local TCP connections have a three-second connect deadline and a
  cancellation-aware, read-then-write bridge with one bounded chunk per
  direction. Non-silent forwarding emits one peer `DESTINATION=` line;
  silent forwarding emits application bytes as the first target bytes.
- `NAMING LOOKUP` supports session-scoped `ME`, strict canonical full public
  Destinations, and locally-owned `.b32.i2p` hashes through the existing SAM
  session registry. Unknown b32 and human-readable `.i2p` names do not use
  DNS and return `KEY_NOT_FOUND`; malformed values return `INVALID_KEY`.
- Recognized unsupported command families, session styles, stream port
  options, forwarding SSL, and naming OPTIONS receive typed unsupported
  outcomes and deterministic SAM replies.
- SAM aggregate client/task and stream-buffer ceilings are checked against
  router-wide budgets during configuration normalization.

## Resource ownership table

| Resource | Owner | Ceiling | Released on |
| --- | --- | --- | --- |
| Accepted SAM TCP clients | `SamServiceState` | `sam.max_clients` | client task/socket end |
| SAM sessions | `SamSessionRegistry` | `sam.max_sessions` | control socket/session teardown |
| STREAM attachments | `SamStreamRegistry` | `max_stream_sockets_per_session` | stream end or session teardown |
| Pending ACCEPT waiters | `SamStreamRegistry` | `max_pending_accepts_per_session` | claim, cancel, or teardown |
| Active FORWARD | `InboundMode` + daemon owner map | one per session | owner EOF, session teardown, shutdown |
| Forward bridge buffers | bridge stream task | `max_buffered_bytes_per_stream_direction` | write/close/cancel |
| SAM input lines | `LineReader` | `MAX_SAM_LINE_BYTES` | command completion or connection close |
| Naming waits | daemon request path | no network wait in M7 | immediate reply or connection close |

## Unsupported matrix

| Input | Result |
| --- | --- |
| `SESSION CREATE STYLE=DATAGRAM`, `RAW`, `DATAGRAM2`, `DATAGRAM3`, `PRIMARY` | typed unsupported style |
| `SESSION ADD`, `SESSION REMOVE` | typed unsupported command family |
| `STREAM CONNECT FROM_PORT` / `TO_PORT` | typed unsupported option |
| `STREAM FORWARD SSL=true` | typed unsupported feature / `NOT_IMPLEMENTED` |
| `NAMING LOOKUP OPTIONS=true` | typed unsupported feature / `NOT_IMPLEMENTED` |
| `AUTH`, `DATAGRAM`, `RAW` | typed unsupported command family / `NOT_IMPLEMENTED` |

## Verification

The local CI-equivalent gate passed on 2026-08-28:

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | passed |
| `cargo check --locked --workspace --all-targets` | passed |
| `cargo test --locked --workspace` | 1,241 passed across 59 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | passed |
| `RUSTDOCFLAGS=-D warnings cargo doc --locked --workspace --no-deps` | passed |
| dependency/runtime/fixture/vector/NTCP2/constrained-host boundary checks | passed |
| constrained-host Python contract tests | 18 passed |
| `cargo deny check advisories bans sources` | passed; existing duplicate-version warnings only |

Focused Plan 139 coverage is in `crates/i2pr-api/tests/sam_plan139.rs` and
`crates/i2pr-daemon/tests/sam_forward_naming.rs`.

Remote CI verification passed for the final implementation commit
[`de5781b`](https://github.com/dbowm91/i2pr/commit/de5781b):
[CI run #553](https://github.com/dbowm91/i2pr/actions/runs/33205078363)
completed successfully across the quality matrix, MSRV, and dependency-policy
jobs. The macOS quality runner executes the complete test-binary set serially;
this avoids the runner-specific loopback contention observed during the first
Plan 139 runs.

The implementation deliberately does not claim a complete same-socket raw
SAM CONNECT/ACCEPT handoff or independent-router SAM interoperability; those
remain explicit Plan 140 work and acceptance debt.

`next_executable_plan = 140`
