# Plan 171 — Milestone 9 I2CP invalid-preamble close and exact-head CI corrective

Status: **active corrective; blocks Plan 170**.

Registered: **2026-09-08**.

Depends on:

- Plan 169 `passed-m9-i2cp-self-composed-local-product-and-hardening`;
- current `main` head `4274b8a6b8198c1daad6caea020fdbffa530840a`;
- failing routine CI run `34202991918` on that exact head.

Resume after closure: **Plan 170**.

## 1. Trigger and exact defect

Plan 169's implementation head `2fecc64f6f7143ac6ef560ee31264b2cfa78b841` had a green routine run (`34201791743`), but the subsequent authoritative status commit `4274b8a6b8198c1daad6caea020fdbffa530840a` is not green.

Routine run `34202991918` passes Ubuntu quality, MSRV, and dependency policy, but fails `Quality (macos-latest)` in:

```text
crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs
  wrong_protocol_byte_is_closed

thread ... panicked ...:
expected close, got timeout
```

The failure occurs after the client writes one byte `0x00` instead of the required I2CP protocol byte `0x2a` and waits for EOF/reset.

This is not an I2CP codec failure. `handle_connection_inner()` correctly classifies the byte and returns `I2cpConnectionError::InvalidProtocolByte`. The corrective concerns the runtime's observable TCP termination/lifecycle contract and its host-independent evidence.

## 2. Why the previous macOS correction is insufficient

Commit `2fecc64` is titled `plan169: accept immediate-close or timeout in adversarial tests`, but its actual patch changes only:

- `oversized_frame_length_rejected_before_body_allocation`;
- `destroy_session_before_create_session_is_rejected`.

It does **not** change `wrong_protocol_byte_is_closed`, whose contract remains strict: an invalid I2CP preamble must result in an observable peer close/reset within a bounded interval.

Do not broaden the `2fecc64` relaxation to this test. A timeout here means the peer-facing close contract is not proven.

## 3. Correct product invariant

For every terminal pre-session protocol rejection, especially an invalid first byte:

```text
classify terminal protocol error
        ↓
stop further frame processing
        ↓
explicitly terminate the TCP stream
        ↓
release connection/session/destination bookkeeping
        ↓
return admission/resource baseline
```

The client must observe EOF or reset. Silence with a still-open socket until an arbitrary timeout is not an acceptable successful outcome for `wrong_protocol_byte_is_closed`.

The implementation must remain loopback-only and must not expose new remote-I2CP behavior.

## 4. Required investigation before editing

Inspect at minimum:

```text
crates/i2pr-daemon/src/i2cp.rs
  serve
  handle_connection
  handle_connection_inner
  read_protocol_byte
  teardown_connection
  drop_connection

crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs
  start_listener
  wrong_protocol_byte_is_closed
  truncated_frame_does_not_panic_listener
  unknown_message_type_in_active_state_closes_session
```

Confirm whether the host-sensitive behavior is caused by relying on `TcpStream` drop after `handle_connection_inner()` returns, by task/scope teardown ordering, or by the paused-time test harness. Record the finding in `plans/171-status.md`.

## 5. Preferred runtime correction

Prefer an explicit terminal socket shutdown in the per-connection owner instead of relying on destructor timing.

Expected shape:

```rust
let result = handle_connection_inner(...).await;

// Terminal connection result: explicitly close the socket before
// releasing router-side ownership/bookkeeping.
let _ = stream.shutdown().await;

state.teardown_connection(connection_id);
state.drop_connection(connection_id);
```

Exact placement may differ after inspection, but preserve these rules:

- `i2pr-daemon` remains the only TCP owner;
- shutdown happens once on the common terminal path, not duplicated across every protocol error;
- shutdown failure must not block resource cleanup;
- do not write a protocol frame for an invalid first byte unless the I2CP specification explicitly requires one;
- no sleeps;
- no detached cleanup task;
- no unbounded retry;
- no panic on already-closed/reset sockets.

If explicit `AsyncWriteExt::shutdown()` is not the correct primitive after direct investigation, use the smallest equivalent Tokio-owned close mechanism and document why.

## 6. Test correction requirements

Keep `wrong_protocol_byte_is_closed` strict.

It must prove:

1. client writes a wrong first byte;
2. server never sends an application/I2CP reply;
3. client observes `read() == 0` or a connection-reset/broken-pipe class error within a bounded deadline;
4. `I2cpServiceSnapshot.connection_count` returns to baseline;
5. session/destination counts remain zero;
6. listener remains usable by a subsequent valid I2CP client.

Add a repeated trajectory (suggested 16–32 iterations) proving invalid-preamble connects do not monotonically retain connection/admission state.

A valid sibling or immediately subsequent client must complete GetDate/SetDate after the rejected peer is closed.

### Forbidden test-only fixes

Do **not**:

- accept `Err(Elapsed)` as success for the wrong-preamble row;
- increase the timeout as the primary fix;
- remove the assertion that the peer closes;
- skip/ignore the test on macOS;
- branch expected behavior by OS;
- serialize the entire workspace solely to hide the race;
- change `start_paused` without proving that the product close contract itself is correct.

If Tokio paused-time semantics materially contribute, separate product-close evidence from timer behavior with an additional non-paused focused test rather than weakening the close assertion.

## 7. Scope audit of other terminal errors

After fixing the common terminal path, rerun the other connection-ending adversarial rows and classify them explicitly:

- invalid protocol byte — **must close observably**;
- unknown/illegal message in terminal state — must close/reset per declared M9 profile;
- oversized frame — bounded rejection; observable close preferred, but retain the existing declared acceptance shape unless the common close path makes it deterministic;
- DestroySession before session creation — protocol/state rejection according to the declared profile;
- truncated/stalled peer — bounded by timeout/cancellation policy, not necessarily immediate close.

Do not accidentally convert nonterminal protocol errors that intentionally return `SessionStatus`/`MessageStatus` into connection-closing errors.

## 8. Focused validation

Run at minimum:

```text
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix \
  wrong_protocol_byte_is_closed -- --exact --test-threads=1 --nocapture

cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix \
  -- --test-threads=1

cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
```

If practical, run the focused invalid-preamble test repeatedly locally:

```text
for i in $(seq 1 32); do
  cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix \
    wrong_protocol_byte_is_closed -- --exact --test-threads=1 || exit 1
done
```

Do not add this loop to routine CI unless needed; the deterministic single-run test plus exact-head hosted macOS evidence is the primary gate.

## 9. Full repository floor

Before closure run:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
cargo deny check advisories bans sources
```

No Plan 170 external-I2CP evidence is required to close Plan 171; Plan 170 remains the next gate.

## 10. Hosted acceptance

Plan 171 closes only on a commit whose routine CI passes all existing jobs:

```text
Quality (ubuntu-latest) = success
Quality (macos-latest)  = success
MSRV (Ubuntu)           = success
Dependency policy       = success
```

The macOS job must execute `wrong_protocol_byte_is_closed`; do not special-case it out of the job.

Because the trigger is an exact-head red status, the closure record must contain the exact green hosted run ID and exact closing SHA.

## 11. Acceptance criteria

Plan 171 passes only if all are true:

1. The current-head failure `34202991918` is recorded accurately.
2. The root cause of the missing observable close is identified and documented.
3. Invalid preamble still fails before any I2CP frame/body processing.
4. The common per-connection terminal path explicitly or equivalently closes the TCP stream deterministically.
5. Cleanup proceeds even if socket shutdown itself errors.
6. `wrong_protocol_byte_is_closed` still treats timeout as failure.
7. The wrong-preamble row observes EOF/reset on Linux and macOS hosted CI.
8. Connection/session/destination resource baselines return to zero after rejection.
9. Repeated wrong-preamble rejection shows no retained resource growth.
10. A valid sibling/subsequent I2CP client remains usable.
11. Nonterminal SessionStatus/MessageStatus behavior is unchanged.
12. Plans 164–169 focused regressions remain green.
13. M6/M7/M8 retained local regressions remain green under the normal workspace floor.
14. I2CP remains disabled by default and loopback-only.
15. No external-client or public-network claim is introduced.
16. Routine Ubuntu/macOS, MSRV, and dependency-policy jobs pass on the exact closing SHA.
17. `plans/171-status.md` records exact commands, failure diagnosis, closing SHA, and hosted run ID.
18. Planning authority returns to Plan 170 after closure.

## 12. Planning/authority updates at closure

Update coherently:

```text
plans/171-status.md
plans/README.md
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
specs/support.toml
```

Expected classification:

```text
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective
plan_170 = ready-m9-i2cp-independent-clients-and-final-closure
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 170
```

Do not mark Milestone 9 closed in this corrective.

## 13. Stop conditions

Stop and write a narrower follow-up if investigation shows any of the following:

- the server continues processing frames after invalid preamble;
- resource state remains live after the client sees EOF/reset;
- fixing close semantics breaks valid clients or SAM;
- macOS still intermittently fails after explicit common-path shutdown and repeated focused testing;
- the defect is actually in `ChildScope`/runtime task lifecycle shared by non-I2CP services.

If the defect is shared infrastructure, do not bury it in an I2CP-only test workaround.

## 14. Handoff

Execute Plan **171** now. On explicit passing closure, resume the already-registered Plan **170** final independent-client gate. Do not begin Milestone 10 planning until Plan 170 closes Milestone 9.