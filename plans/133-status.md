# Plan 133 status — Milestone 6 final evidence and authority closure

Status: **`passed-milestone6-final-evidence-authority-closure`**.

- Registered: **2026-08-27**; executed from source floor
  `af0f07a0037a639afc2c03af31a1266b99273564`.
- Plan of record:
  [`plans/133-m6-final-evidence-authority-closure.md`](133-m6-final-evidence-authority-closure.md).
- This file records successful historical evidence for the replay,
  Elligator, transactional-send, and authority corrections it targeted.
  A later concrete receive-window defect was corrected by Plan 134,
  which is now the current Milestone 6 local closure authority.
  `plans/131-status.md` and `plans/132-status.md` are retained as
  historical implementation/evidence records and are marked
  `superseded-by-plan133-final-evidence-authority-gate` for the
  authoritative closure interpretation.
- Plan 132's substantive implementation corrections are retained
  unchanged: strict Elligator receive validation in
  `crates/i2pr-crypto/src/ecies.rs`; Plan 131 production Elligator
  branch/high-bit randomization; transactional `send_data()` wire
  construction before send-window commit; transactional
  CLOSE/RESET build/sign before state mutation; exact tunnel-cell
  replay B1 evidence; connection-owned I2P ports and source-port
  zero behavior; corrected Streaming sequence/ACK/NACK behavior;
  Plan 124 encrypted Garlic/tunnel composition invariant.

## Final classification

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = implementation-landed-evidence-superseded-by-plan133
plan_133 = passed-evidence-authority-superseded-by-plan134-streaming-ack-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

## What Plan 133 corrected

### Phase A — B2 evidence correction
(`crates/i2pr-client/tests/plan132_trajectory.rs`)

`plan132_consumed_es_ciphertext_rewrapped_in_fresh_tunnel_is_rejected_by_ecies`
now directly proves the integrated ECIES/session rejection reaches
the production dispatcher:

- A1 captures the first delivered Existing Session ciphertext and
  tag from the real first plan, decodes it as
  `ExistingSessionMessage`, and decodes the Garlic carrier payload
  that the OBEP produces. The tunnel-decrypted bytes equal the
  Garlic carrier `garlic_i2np_bytes` (tunnel framing wraps the
  carrier) and the Garlic carrier payload equals the ECIES ES
  ciphertext.
- A2 rewraps the same `retained_es_bytes` and `retained_es_tag`
  under a fresh IBGW RNG; the new cells survive the tunnel
  duplicate window because the inbound IBGW cell bytes are
  freshly sampled, and the inner Garlic carrier payload is
  byte-for-byte equal to the first envelope.
- A3 asserts the integrated rejection variant the production
  dispatcher currently emits. The narrowest stable variant is
  `Rejected(Session(Ecies(AuthenticationFailed)))` because the
  dispatcher's `classify()` removes the consumed tag from the
  inbound window before the replay arrives; the dispatcher routes
  the replay through the New Session path and the ECIES AEAD
  inside fails authentication. The earlier
  `Rejected(Session(UnknownSessionTag))` variant is preserved as
  the direct `accept_existing_session` proof
  (`plan132_ecies_session_layer_rejects_consumed_tag_directly`),
  which exercises the session-layer rejection explicitly without
  the dispatcher's New Session pre-classification.
- A5 holds the dispatcher queued-payloads, established-sessions,
  pending-handshake, provisional-responder, and Streaming
  delivered-byte counters constant across the replay.

### Phase B — B3 evidence correction
(`crates/i2pr-client/tests/plan132_trajectory.rs`)

`plan132_fresh_es_seal_of_same_streaming_sequence_reaches_streaming_and_deduplicates`
now retains the actual first delivered ES envelope rather than
re-synthesizing a comparison artifact:

- B1 calls `send_data()` exactly once and captures sequence `N`
  and `application_payload` bytes verbatim.
- B2 seals the request through the adapter, decodes the real
  first ES envelope as `ExistingSessionMessage`, and drives that
  exact first plan through the receiver via the new
  `pipe_through_first_plan` helper (which surfaces both the
  dispatcher outcome and the inbound Streaming outcome from a
  single delivery). The dispatcher emits
  `ExistingSessionProcessed` and the adapter surfaces the
  original application bytes exactly once.
- B3 freshly reseals the exact same `first_request` under a
  different RNG, asserts the second ES tag/ciphertext differ
  from the first, asserts the second plan's cell count equals
  the first plan's cell count, and asserts the second plan's
  `OutboundDeliveryPlan.cells` produce a distinct inner I2NP
  Garlic payload. The hard-coded `plan132_plan_cell_count`
  helper is removed.
- B4 drives the second plan through the receiver; the lower
  tunnel accepts the fresh wrapping, the dispatcher reaches
  ECIES authentication, and the dispatcher emits exactly one
  `ExistingSessionProcessed` outcome.
- B5 proves Streaming is the only layer that suppresses the
  duplicate sequence: `drain_delivered` yields no second copy
  and `pop_payload` returns `None`.

### Phase C — Elligator reference-note correction
(`specs/references/elligator2-production-representation.md`,
`crates/i2pr-crypto/src/ecies.rs`)

The reference note now distinguishes **executable acceptance
domains** from **source comments** explicitly:

- Java I2P `Elligator2.decode()`: masks byte 31 with `0x3f` and
  rejects `r >= (p - 1) / 2` (strict `<`). **Equality rejected.**
- Pinned i2pd `Elligator.cpp`: source comment reads `r < (p - 1) /
  2`, but the executable check enters the decode branch on
  `BN_cmp(r, p12) <= 0` (i.e., `r <= (p - 1) / 2`).
  **Equality accepted.** The encoder's `SquareRoot()` produces
  the equality value only through a single pre-image point per
  generator and that case is not reached on the OR'd high-bit
  variants or the alternative branch in normal production
  traffic.
- i2pr enforces the stricter Java-style boundary as a deliberate
  safer subset; the rejection is structurally inert for
  compliant i2pd traffic.
- `decode_representative` doc comment and three Plan 132 boundary
  tests (`masked_representative_equal_to_threshold_is_rejected`,
  `masked_representative_just_above_threshold_is_rejected`,
  `maximum_masked_representative_is_rejected`) are updated to
  reflect the Java/i2pd executable distinction.

### Phase D — Targeted regression
(`cargo +1.95.0 test --locked -p i2pr-crypto`, `-p i2pr-client`)

All Plan 132 trajectories pass after the Phase A/B rewrites:

- `i2pr-crypto`: 51 unit tests pass (including the Elligator
  boundary tests, the production generator randomness tests, and
  the frozen Plan 126 ECIES vector reproductions).
- `i2pr-client`: every integration test binary passes, including
  the 10-test `plan132_trajectory` suite and the 7-test
  `plan131_trajectory` suite.

### Phase E — Full local validation gate

```text
cargo +1.95.0 fmt --all --check                                                pass
cargo +1.95.0 check --locked --workspace --all-targets                         pass
cargo +1.95.0 test --locked --workspace                                        all green
cargo +1.95.0 clippy --locked --workspace --all-targets
       --all-features -- -D warnings                                            pass (0 warnings)
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked
       --workspace --no-deps                                                    pass
bash scripts/check-dependency-direction.sh                                     ok
bash scripts/check-runtime-boundaries.sh                                       passed
```

### Phase F — Status and documentation synchronization

- `plans/131-status.md`: marked historical; final classification
  reclassified to `superseded-by-plan132-and-plan133-final-gates`.
- `plans/132-status.md`: implementation retained unchanged;
  closure status reclassified to
  `implementation-landed-evidence-superseded-by-plan133`.
- `plans/133-status.md`: retained as successful historical evidence;
  its authority is superseded by Plan 134's receive-window closure.
- `README.md`, `AGENTS.md`, `architecture/` (`overview.md`,
  `i2pr-crypto.md`, `i2pr-client.md`, `i2pr-proto.md`,
  `i2pr-tunnel.md`, `i2pr-daemon.md`, `i2pr-netdb.md`),
  `docs/protocol-support.md`, `specs/support.toml`, and the
  `i2pr-ntcp2-interop` skill were synchronized to point at Plan 133
  as the then-current Milestone 6 closure authority; Plan 134 now
  supersedes that authority for the receive-window correction.

## Validation record

Exact commands (pinned toolchain 1.95.0, `--locked`):

```text
cargo +1.95.0 fmt --all --check                                                pass
cargo +1.95.0 check --locked --workspace --all-targets                         pass
cargo +1.95.0 test --locked -p i2pr-crypto --all-targets                       51 tests pass
cargo +1.95.0 test --locked -p i2pr-proto --all-targets                        pass
cargo +1.95.0 test --locked -p i2pr-client --all-targets                       pass
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets                       pass
cargo +1.95.0 test --locked --workspace                                        all green
cargo +1.95.0 clippy --locked --workspace --all-targets
       --all-features -- -D warnings                                            pass
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked
       --workspace --no-deps                                                    pass
bash scripts/check-dependency-direction.sh                                     ok
bash scripts/check-runtime-boundaries.sh                                       passed
```

New focused evidence (Plan 133 closure surface):

- `crates/i2pr-client/tests/plan132_trajectory.rs` — 10 tests
  pass: B1 (`plan132_exact_same_tunnel_cell_is_rejected_by_live_duplicate_window`),
  B2 (`plan132_consumed_es_ciphertext_rewrapped_in_fresh_tunnel_is_rejected_by_ecies`
  rewritten to retain the real first ES envelope and assert the
  dispatcher-stable rejection variant), B3
  (`plan132_fresh_es_seal_of_same_streaming_sequence_reaches_streaming_and_deduplicates`
  rewritten to retain the actual first plan), the 4 Plan 132
  transactional precommit tests, the direct session-layer
  consumed-tag rejection test
  (`plan132_ecies_session_layer_rejects_consumed_tag_directly`),
  and the sender port-tuple assert tests for `send_data`,
  `send_close`, and `send_reset`.
- `crates/i2pr-crypto/src/ecies.rs` — 51 unit tests pass,
  including the Elligator boundary tests, the Plan 131
  production-generator randomness tests, and the frozen Plan 126
  KDF/Noise vector reproductions.

## Subsequent Plan 134 correction

Plan 134 corrected a concrete receive-window defect discovered after
this closure: `TooFarAhead` packets previously advanced
`highest_received` before rejection. The corrected policy performs
admission before ACK-state mutation, and focused policy plus manager
regressions prove rejected sequences remain absent from later ACK/NACK
views and production piggyback ACKs. Plan 134 is the final local
Milestone 6 authority.

## Explicitly not claimed

Mixed-router interoperability, destination ECIES/Streaming/tunnel
interoperability against an independent router, NTCP2 activation,
and any public-network behavior remain outside this closure. NTCP2
stays experimental and non-advertised. External acceptance debt is
retained separately.

## Handoff

Plan 133's handoff is superseded by the narrow Plan 134 correction.
After Plan 134 passed, corrective Milestone 6 planning stops and the
next product work is **Milestone 7 / SAM baseline planning**.
Do not reopen external transport validation as a prerequisite for
SAM. Do not create another Milestone 6 plan unless a new concrete
protocol defect is discovered.
