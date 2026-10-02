# Plan 302 — M12 floodfill-reply wire form and replication-planning corrective

Status at registration: **registered-m12-floodfill-reply-wire-form-and-replication-corrective**

Classification: narrow protocol corrective (one proven wire defect + one localized
open question). Follows the Plan 278 second bounded attempt (§11–§12 of
`plans/closure/floodfill/278-status.md`).

Hard dependencies: none beyond the Plan 278 stop record. Plans 284/285 stay retained;
Plan 278 stays stopped with its budget spent; Plan 279 stays blocked.

## 1. Objective

Fix the proven floodfill-reply wire-form defect so the exact-pinned reference consumes
i2pr floodfill replies (DeliveryStatus acks, lookup DSM/DSRM answers, direct zero-token
stores), determine why no replication was planned for the accepted publisher store,
and re-observe both under one new bounded exact-pinned attempt with its own frozen
budget. This is a corrective, not a widening of Plan 278's spent budget.

## 2. Proven defect (why-ready)

The Plan 278 second attempt (frozen budget 1, spent) produced reference-side proof:

- i2pr sent 1 DeliveryStatus ack + 18 DSRM answers; the reference dropped all 19 with
  `SSU2: Message <id> expired` (`SSU2Session.cpp:2766`). Counts match exactly
  (1 × 28-byte + 18 × 145-byte messages).
- Cause: `encode_standard` (`crates/i2pr-daemon/src/floodfill.rs:277-284`) writes the
  16-byte standard header (u64 millisecond expiration at bytes 5–12, body from byte 16),
  but `queue_i2np_message`
  (`crates/i2pr-transport-ssu2/src/session.rs:835-839`) reads all outbound bytes as the
  9-byte short-transport form (`expiration_secs = raw[5..9]`, body from byte 9). The
  reference therefore reads ~417 (high 4 bytes of ms time = 1970) as seconds and drops
  every reply; the body offset is wrong by 7 bytes regardless.
- Every other daemon send path uses `new_short_transport` +
  `encode_short_transport_to_vec` with checked ms→s conversion, which is why Plans
  161/186 passed. The i2pr↔i2pr lifecycle tests are blind to it (symmetric standard
  codec both ends).

Open question: no `DirectFlood` effect was planned for the accepted store
(`direct_store_replicas=0`, no failures, no dispatch errors). Bounded hypotheses are
`Idempotent` insert against the seeded replica vs an empty replication plan; the
driver records neither. See 278-status §12.2.

## 3. Evidence principles (same as Plan 278)

- Exact pin i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`, verified before
  execution; loopback-only; reseed disabled; no public network.
- No reference patching/vendoring/fake peers/production relaxation.
- New attempt runs under a FRESH frozen budget of 1 (`MAX_ATTEMPTS=1` in a Plan 302
  runner invocation, recorded before execution). Plan 278's spent budget is not widened.
- External rows `#[ignore]`-gated, fail-closed without env. Raw reference logs are
  diagnostic only; evidence is sanitized counts/hashes/categories.
- No retry-until-green.

## 4. Required work packages

1. **Wire-form fix (production, narrow).** Replace the outer encoding in
   `encode_standard` (`crates/i2pr-daemon/src/floodfill.rs`) with the short-transport
   form: `expiration_seconds = u32::try_from(expiration_ms / 1000)` (fail closed on
   overflow, mirroring `outbound_lookup.rs:206`), `new_short_transport`,
   `encode_short_transport_to_vec`. Applies to StoreAck, direct LookupReply, outer
   TunnelGateway reply, and DirectFlood stores. The tunnel-wrapped INNER garlic clove
   stays standard-encoded (consumed by `decode_standard` in the same function).
2. **Wire-form pin test.** A unit test that encodes each floodfill reply shape and
   asserts the bytes parse as the 9-byte short-transport layout with a sane
   seconds expiration (within a bounded horizon of the supplied `now_ms`, never a
   millisecond value, never year-1970). Must fail on the pre-fix encoder.
3. **Reference-derived cross-check.** A unit test feeding a reference-shaped
   short-transport DeliveryStatus/DSRM byte vector through the session-layer parse
   (`raw[5..9]` seconds) to lock the assumed layout against the pinned source
   (`I2NPProtocol.h` short-header offsets + `SSU2Session::HandleI2NPMsg`).
4. **Replication observability (driver + coordinator, sanitized counts only).**
   Record the store insert outcome class (`Inserted`/`Replaced`/`Idempotent`/
   `Stale`/`Conflict`/rejected) and replication-plan statistics (candidate offered?
   actions planned?) as TSV counts. No payloads, no keys.
5. **Seeded-replica idempotency test (local).** Seed a coordinator NetDB with a flood
   replica, then `handle_store` the same key with a newer timestamp via direct-peer
   ingress: assert the outcome class and whether replication is offered. This decides
   the §12.2 hypotheses without spending external budget.
6. **One bounded attempt.** Fresh frozen budget 1 through the existing
   `run-i2pd.sh` lane (plus the committed driver declaration fix already on head).
   Success bar: reference logs `Publishing confirmed`; i2pr observes a planned
   `DirectFlood`. Full matrix pass is NOT required to close; whatever the attempt
   shows is recorded exactly.
7. **No-guarantee clause.** If the attempt surfaces a further defect, stop and register
   the next narrow corrective. Do not iterate the budget.

## 5. Production changes allowed

- `crates/i2pr-daemon/src/floodfill.rs`: the `encode_standard` wire-form fix only.
  No behavior, policy, budget, or guard change.
- Tests + the `#[ignore]`-gated external driver (observability rows only).

Forbidden: touching the install guard, `controlled_router_options`, caps/version
declaration, replication policy thresholds, attempt budgets of other plans, or any
reference artifact.

## 6. Failure / cancellation / restart

Same bounds as Plan 278 §6: hard per-step deadlines, timeout is a retained categorical
failure, cleanup kills owned reference processes and removes only the fresh attempt
datadir.

## 7. Local verification before the external attempt

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
```

Plus the new wire-form pin tests and the idempotency test, green.

## 8. Acceptance criteria

- Floodfill replies arrive in short-transport form with sane seconds expiration
  (unit-pinned + observed consumed by the reference: zero `Message <id> expired`
  drops for i2pr-sent messages, or the exact remaining drops recorded).
- The §12.2 question is answered with evidence (outcome class + plan stats), whether
  the answer is a second defect (then fixed or handed off) or a lane artifact
  (then documented with the local test as proof).
- Exact-head ordinary CI green. Evidence checker passes on the retained root.
- Zero matrix rows claimed beyond what the attempt actually shows; no capability or
  version claim changes.

## 9. Stop conditions

Stop on any further reproducible i2pr protocol defect and register the next narrow
corrective. If the reference lane itself is unavailable, record the exact boundary.
Never increase the fresh budget of 1, patch i2pd, or broaden network access. A pass
on the attempt's success bar does not pass Plan 278 (whose budget is spent and whose
matrix is unexecuted); it unblocks registering the matrix-execution successor.

## 10. Closure evidence required

Fix commit(s), wire-form pin tests, idempotency test result, the one bounded attempt:
pin metadata, frozen budget record, sanitized evidence (counts incl. reference
`expired`-drop count for i2pr-sent messages, `Publishing confirmed` presence/absence,
insert-outcome class, plan stats, matrix rows observed), CI run, security/resource
review, unblock audit.

## 11. Handoff

If the attempt meets the success bar, register the matrix-execution successor (fresh
budget, Plan 278's matrix A–I) — do not resurrect Plan 278 itself. If it stops at a
new defect, the next corrective owns it. Plan 279 stays blocked throughout.
