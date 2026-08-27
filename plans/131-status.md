# Plan 131 status — Milestone 6 final local correctness closure

Status: **`superseded-by-plan132-and-plan133-final-gates`** (historical
implementation/evidence record; not the current Milestone 6 closure
authority).

- Registered: **2026-08-26**; executed from source floor
  `8f2b3dfe44b480beb7f411613b8d7089aaeb7a19`.
- Plan of record:
  [`plans/131-m6-final-local-correctness-closure.md`](131-m6-final-local-correctness-closure.md).
- Plan 131's substantive implementation is retained unchanged. This
  file is retained as the historical record of the
  `passed-milestone6-final-local-correctness-closure` surface Plan
  131 closed at the Plan 130 → Plan 131 transition. Plan 132 then
  reopened the Plan 131 closure with three narrow evidence/transactional
  corrections (strict Elligator receive validation, real three-layer
  replay separation, transactional established sends) and closed as
  `implementation-landed-evidence-superseded-by-plan133`. Plan 133
  then closed the remaining B2/B3 evidence gaps and the Elligator
  reference-note correction, and is the **current Milestone 6
  local closure authority**. See `plans/133-status.md`.
- The current authoritative token for the Milestone 6 local closure
  is therefore `plan_133 = passed-milestone6-final-evidence-authority-closure`,
  not the Plan 131 token recorded below.

## Final classification

The classification below reflects Plan 131's local closure
transition (Plan 130 → Plan 131). The current Milestone 6 local
closure authority is the Plan 133 token
`passed-milestone6-final-evidence-authority-closure` recorded in
`plans/133-status.md`. Plans 131 and 132 are historical
implementation/evidence records for that authority.

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-streaming-wire-local
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-line
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = superseded-by-plan130-final-gate
plan_130 = superseded-by-plan131-final-local-correctness-gate
plan_131 = superseded-by-plan132-and-plan133-final-gates (this file)
plan_132 = implementation-landed-evidence-superseded-by-plan133
plan_133 = passed-milestone6-final-evidence-authority-closure (current authority)
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

## What was corrected

### Phase A+B — production Elligator branch randomization
(`crates/i2pr-crypto/src/ecies.rs`, `Cargo.toml`)

- The ECIES ephemeral representative generator now uses the
  `elligator2 = 0.1.0` reviewed primitive (`to_representative(point,
  tweak)`). The 8-bit tweak drives both deployed-reference
  randomization degrees: `tweak & 0x01` selects between `u` and
  `u + A` as the inverse-map base; `tweak & 0xc0` populates the two
  free representation bits per `ENCODE_ELG2`.
- `curve25519-elligator2` was retired from `i2pr-crypto`'s
  dependency list. Its `RFC9380::to_representative` derived the
  inverse-map branch deterministically from the public key; its
  `Randomized` mode randomized the branch but also rotated the
  Diffie-Hellman public key away from what a standard X25519
  decoder would recover. The reviewed primitive is the explicit
  preferred implementation route and matches the deployed Java
  I2P / i2pd encoder semantics byte-for-byte.
- `from_seed_bytes(seed)` remains the deterministic test/vector
  constructor (tweak `0` → branch 0, high bits `00`). Every frozen
  Plan 126 KDF/Noise vector continues to reproduce byte-for-byte.
- `from_seed_bytes_with_tweak(seed, tweak)` is the explicit-tweak
  deterministic constructor for tests and frozen fixtures.
- Reference note: [`specs/references/elligator2-production-representation.md`](../specs/references/elligator2-production-representation.md).

### Phase C — independent three-layer replay separation
(`crates/i2pr-client/tests/plan131_trajectory.rs`,
`crates/i2pr-crypto/src/ecies.rs`)

- `plan131_exact_cell_replay_hits_tunnel_duplicate_window` —
  an exact cell replay hits the persistent inbound tunnel
  duplicate window before ECIES is ever invoked.
- `plan131_consumed_es_session_tag_replay_rejected_by_session_layer`
  — the integrated ECIES consumed-tag replay leaves the receiver
  state untouched. No Data plaintext crosses the seam; the
  receiver's recv-window delivered count never advances past
  the original first delivery.
- `plan131_fresh_ecies_reseal_is_deduplicated_by_streaming` — a
  freshly resealed identical Streaming sequence is deduplicated
  at the sequence level; the application bytes surface exactly
  once.
- The new ECIES unit tests
  (`production_generator_randomizes_the_inverse_map_branch_bit`,
  `from_seed_bytes_with_tweak_produces_distinct_but_decoding_invariant_branches`)
  pin the branch randomization against the frozen fixtures.

### Phase D — connection-owned I2P ports
(`crates/i2pr-client/src/streaming/manager.rs`,
`crates/i2pr-client/tests/plan131_trajectory.rs`)

- `StreamingManager::send_data`, `send_close`, and `send_reset`
  now read the wire ClientPayload ports from the stored
  `StreamingConnection` (`local_port` / `remote_port`); the
  caller-supplied port arguments are treated only as
  **assertions** and fail closed with typed `PortTupleMismatch`
  before any state mutation. The established tuple is the
  authority for every post-handshake wire packet.
- `process_inbound_packet` validates the wire destination /
  source ports against the stored tuple on every inbound
  delivery, including the SYN response branch. A wrong-port
  response leaves the outbound connection in `OutboundSynSent`
  and never transitions to `Established`.
- `StreamingDestinationAdapter::receive` (Plan 129 §8 surface)
  remains the only inbound dispatch boundary; no caller-supplied
  listener override is exposed.
- New unit tests:
  `plan131_established_data_uses_connection_ports_in_wire_envelope`,
  `plan131_send_data_with_caller_port_mismatch_fails_closed_before_mutation`,
  `plan131_send_close_with_caller_port_mismatch_fails_closed`,
  `plan131_send_reset_with_caller_port_mismatch_fails_closed`,
  `plan131_source_port_zero_works_end_to_end`.

### Phase E — transactional oversized writes
(`crates/i2pr-client/src/streaming/manager.rs`,
`crates/i2pr-client/tests/plan131_trajectory.rs`)

- `send_data` validates the negotiated maximum payload size
  **before** allocating the send-window sequence number and
  before any send-window mutation. A rejected oversized write
  changes no sequence, no window state, no retransmit record,
  and no outbound queue. The next valid packet receives the
  exact contiguous sequence number that would have been assigned
  before the rejected call.
- Send-window backpressure (the second failure path) is also
  evaluated before sequence allocation; the same snapshot
  invariants hold.
- New unit test:
  `plan131_oversized_send_data_is_side_effect_free`.

### Phase F — retained Plan 130 surface

- `plan131_full_stack_fixture_still_composes` smoke-tests the
  Plan 130 `Side` / `establish_stream` / `pipe_through_stack`
  fixture surface after every Phase D/E refactor.
- All eleven `plan130_trajectory.rs` test cases remain green
  unmodified.

## Validation record

Exact commands (pinned toolchain 1.95.0, `--locked`):

```text
cargo +1.95.0 fmt --all --check                                   pass
cargo +1.95.0 check --locked --workspace --all-targets            pass
cargo +1.95.0 test --locked -p i2pr-crypto                        41 tests ok
cargo +1.95.0 test --locked -p i2pr-proto                         pass
cargo +1.95.0 test --locked -p i2pr-client                        pass (60 lib + plan120..plan131 suites)
cargo +1.95.0 test --locked -p i2pr-tunnel                        pass
cargo +1.95.0 test --locked --workspace                           all green
cargo +1.95.0 clippy --locked --workspace --all-targets
       --all-features -- -D warnings                               pass (0 warnings)
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked
       --workspace --no-deps                                       pass
bash scripts/check-dependency-direction.sh                        ok
bash scripts/check-runtime-boundaries.sh                          passed
bash scripts/check-fixture-manifest.sh                            exit 0
```

New focused evidence:

- `crates/i2pr-client/tests/plan131_trajectory.rs` — 7
  deterministic tests (3 replay-separation, source-port-zero,
  port-ownership, oversized-rollback, full-stack fixture smoke).
- `crates/i2pr-crypto/src/ecies.rs` Plan 131 tests:
  `production_generator_randomizes_the_inverse_map_branch_bit`,
  `from_seed_bytes_with_tweak_produces_distinct_but_decoding_invariant_branches`
  plus the retained Plan 130 high-bit / decode tests.

## Explicitly not claimed

Mixed-router interoperability, destination ECIES/Streaming/tunnel
interoperability against an independent router, NTCP2 activation,
and any public-network behavior remain outside this closure. NTCP2
stays experimental and non-advertised. External acceptance debt is
retained separately.

## Handoff

Per the plan-of-record §13: stop corrective Milestone 6 planning.
The next product work is **Milestone 7 / SAM baseline planning**.
Do not reopen external transport validation as a prerequisite for
SAM.