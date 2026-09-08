# Plan 171 status — Milestone 9 I2CP invalid-preamble close and exact-head CI corrective

Status: **`passed-m9-i2cp-invalid-preamble-close-and-ci-corrective`**.

Registered: **2026-09-08**.

Plan of record:
[`plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md`](171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md).

## Current authority

```text
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
plan_167 = passed-m9-i2cp-loopback-server-runtime
plan_168 = passed-m9-i2cp-message-data-plane
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective
plan_170 = ready-m9-i2cp-independent-clients-and-final-closure

milestone9_i2cp_local_product = passed-via-plan169
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 170
resume_after_plan171 = 170
next_product_layer = milestone9-i2cp
```

## Trigger evidence

Current head when Plan 171 was registered:

```text
4274b8a6b8198c1daad6caea020fdbffa530840a
```

Routine CI run:

```text
34202991918
Quality (ubuntu-latest) = success
Quality (macos-latest)  = failure
MSRV (Ubuntu)           = success
Dependency policy       = success
```

The macOS failure is:

```text
crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs
wrong_protocol_byte_is_closed ... FAILED
expected close, got timeout
```

The test sends `0x00` instead of the required I2CP protocol byte `0x2a`. The server's inner connection state correctly returns `InvalidProtocolByte`; the missing proof is deterministic peer-visible TCP termination and cleanup on the common terminal path.

## Prior correction audit

Plan 169 closing commit `2fecc64f6f7143ac6ef560ee31264b2cfa78b841` was titled `accept immediate-close or timeout in adversarial tests`, but its patch only changed:

- `oversized_frame_length_rejected_before_body_allocation`;
- `destroy_session_before_create_session_is_rejected`.

It did not change `wrong_protocol_byte_is_closed`. The current failure is therefore not contradicted by that patch; the wrong-preamble close contract remains unresolved on the current authoritative head.

## Handoff

Plan 171 has passed. Execute **Plan 170** next, the final
independent-client gate. Do not begin Milestone 10 planning until
Plan 170 closes Milestone 9.

Expected post-closure authority:

```text
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective
plan_170 = ready-m9-i2cp-independent-clients-and-final-closure
next_executable_plan = 170
milestone9_final_acceptance = not-yet-closed
```

## Root-cause diagnosis

Inspection of `crates/i2pr-daemon/src/i2cp.rs` (`serve`,
`handle_connection`, `handle_connection_inner`,
`read_protocol_byte`, `teardown_connection`, `drop_connection`)
confirmed the plan's hypothesis: the invalid-preamble rejection
was classified correctly (`I2cpConnectionError::InvalidProtocolByte`
before any frame/body processing), but the runtime relied on
`TcpStream` drop at spawned-task exit for the peer-visible close.
`teardown_connection` + `drop_connection` run while the
`handle_connection` frame still owns the stream, so the FIN/RST
was ordered after bookkeeping release and task-completion
scheduling. On the macOS hosted runner that ordering surfaced as
a still-open socket for the full 2 s client deadline
(`expected close, got timeout`). No codec, state-machine, or
paused-clock defect: the contract gap was destructor-timing
dependence on the common terminal path.

## Corrective

`handle_connection` now explicitly terminates the TCP stream on
the single common terminal path before releasing router-side
bookkeeping:

```rust
let _ = stream.shutdown().await;
state.teardown_connection(connection_id);
state.drop_connection(connection_id);
```

Rules preserved: `i2pr-daemon` remains the only TCP owner;
shutdown happens once on the common path, not per error;
shutdown failure never blocks cleanup; no protocol frame is
written for an invalid first byte; no sleeps, no detached
cleanup task, no unbounded retry, no panic on
already-closed/reset sockets. Nonterminal
`SessionStatus`/`MessageStatus` replies still return
`FrameOutcome::Reply` and never touch this path.

## Test evidence

`wrong_protocol_byte_is_closed` stays strict (timeout is
failure) and now proves, per Plan 171 §6: wrong first byte, no
reply frame (any readable byte panics), EOF/reset within 2 s,
`connection_count`/`session_count`/`destination_count`
baselines at zero after a 24-iteration rejection trajectory, and
a subsequent valid client completing GetDate/SetDate. A new
non-paused companion `wrong_protocol_byte_is_closed_real_time`
(5 s wall-clock deadline, same strict assertions) separates
product-close evidence from `start_paused` timer behavior.

## Local validation (pre-push)

```text
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix \
  wrong_protocol_byte_is_closed -- --exact --test-threads=1 --nocapture
# 2 passed (paused 24-iteration + real-time), 18 filtered out

cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
# 20 passed

cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
# 12 passed

cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
# 18 passed

cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
# 5 passed

cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
# 6 passed

# 32x focused repeat loop: 32/32 passed

cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
# 1729 passed, 1 ignored (routine SSU2 external lane)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# 153 tests OK
cargo deny check advisories bans sources
# all green locally before push
```

## Scope audit of other terminal rows (Plan 171 §7)

After fixing the common terminal path, the full 20-test
adversarial matrix passes unchanged in shape:

```text
invalid protocol byte — observably closes (strict row, Plan 171 fix)
unknown/illegal message in terminal state — closes per declared M9 profile
oversized frame — bounded rejection, existing declared acceptance shape retained
DestroySession before CreateSession — protocol/state rejection per declared profile
truncated/stalled peer — bounded by timeout/cancellation policy
```

No nonterminal `SessionStatus`/`MessageStatus` path was
converted into a connection-closing error.

## Hosted acceptance

Routine CI on the closing SHA:

```text
CLOSING_SHA = c3bf8419c8578c7aa157aa522421a9c5882af419
HOSTED_RUN_ID = 34272024453
Quality (ubuntu-latest) = success
Quality (macos-latest)  = success
MSRV (Ubuntu)           = success
Dependency policy       = success
```

The macOS job log executes the strict row on hosted hardware:

```text
test wrong_protocol_byte_is_closed ... ok
test wrong_protocol_byte_is_closed_real_time ... ok
```

Plan 171 is closed on this run. Planning authority returns to
Plan 170.