# Plan 116 handoff

- Status: **passed-final-local-closure**
- Date: 2026-08-18
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Completion/correction pass: [`116-completion-correction.md`](116-completion-correction.md)
- Final closure pass: [`116-final-closure.md`](116-final-closure.md)
- Terminal cleanup pass: [`116-terminal-cleanup.md`](116-terminal-cleanup.md)
- Current status authority: [`116-status.md`](116-status.md)
- Predecessor: [`115-status.md`](115-status.md)
- Plan 117: **unblocked — ready for planning or execution**.

## Start here

Do not restart Plan 116 from the beginning. The terminal cleanup
pass landed the last four closure defects (`T1`–`T4`) and
synchronized the status / handoff / evidence authority.

Current substantive implementation:

```text
0330fb2e9e64dd0877472c930606ab4219ac18a9  (data plane)
+ this terminal cleanup commit              (T1-T4 corrections)
```

Current state:

```text
plan_115_Q0                       = passed-emissary-native-consumer
plan_116_material_transfer       = passed-local
plan_116_pool_material_only      = passed-local
plan_116_fragment_boundaries     = passed-local
plan_116_exact_byte_cross_tunnel = passed-ordered-local
plan_116_out_of_order_cross_tunnel = passed-local
plan_116_duplicate_accounting    = passed-noop-exact-duplicates
plan_116_duplicate_expiry        = passed-no-refresh
plan_116_first_delivery_identity = passed-conflict-detected
plan_116                         = passed-final-local-closure
plan_117                         = unblocked-ready-for-planning
Q1_authenticated_transport       = deferred
Q2_external_return_established   = deferred
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
```

## Do not rewrite

Retain the existing implementations of:

```text
short.rs        real EstablishedMaterial extraction
short_state.rs  canonical registrar -> pool seam
established.rs  established secret/path ownership
pool.rs         material-bearing production entries
data.rs         Tunnel Message framing + exact fragment sizing
layer.rs        AES layer transforms + duplicate token window
roles.rs        runtime-neutral outbound/inbound role composition
fragment.rs     bounded reassembler (T1+T2 corrections applied)
```

## What the terminal cleanup changed

### T1 — exact duplicates are resource-accounting no-ops

`PartialMessage::classify()` returns a `FragmentInsertDisposition`
that distinguishes `Inserted { added_bytes }` from
`ExactDuplicate`. The reassembler classifies before applying the
aggregate budget check; exact duplicates are accepted as a no-op
even when the budget is already full. `last_touched_ms` is not
refreshed on duplicates. `self.partials.get(&key).cloned()` was
removed; classification borrows the partial immutably.

### T2 — first-fragment delivery metadata participates in duplicate identity

Two first fragments are exact duplicates only when both their body
bytes and their delivery instruction match. A same-body first
fragment with a different delivery instruction is rejected with
`ConflictingFirstMetadata`; only the affected partial is dropped.
A follow-on fragment that carries a delivery instruction is
rejected with `UnexpectedFollowOnDeliveryInstruction`.

### T3 — out-of-order full cross-tunnel proof

`outbound_to_inbound_fragmented_out_of_order_trajectory_exact_bytes`
runs the same outbound -> OBEP -> TunnelGateway -> IBGW -> inbound
participant trajectory as the ordered test, but feeds the local
endpoint with the first-fragment cell moved to the end of the
delivery order. The endpoint does not emit before all unique
fragments are present, emits exactly once, and the recovered
standard I2NP bytes equal the original encoded bytes.

### T4 — status/handoff/evidence authority

Both `plans/116-status.md` and `plans/116-handoff.md` agree on
closure state and Plan 117 successor state. The recorded test
names match the actual implemented identifiers.

## Mandatory tests

The terminal cleanup tests are recorded under the existing
`cargo test` surface. Exact identifiers:

```text
fragment::tests::exact_duplicate_first_does_not_increase_retained_bytes
fragment::tests::exact_duplicate_follow_on_does_not_increase_retained_bytes
fragment::tests::exact_duplicate_at_aggregate_limit_is_accepted_as_noop
fragment::tests::exact_duplicate_does_not_refresh_expiry
fragment::tests::reassembly_completion_returns_aggregate_bytes_to_zero_after_duplicates
fragment::tests::exact_duplicate_first_with_same_delivery_is_idempotent
fragment::tests::conflicting_first_router_target_invalidates_partial
fragment::tests::conflicting_first_tunnel_id_invalidates_partial
fragment::tests::conflicting_first_tunnel_gateway_invalidates_partial
fragment::tests::unexpected_follow_on_delivery_fails_closed
roles::tests::outbound_to_inbound_fragmented_out_of_order_trajectory_exact_bytes
```

## Validation bar

Run targeted tests first, then at minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --lib
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Do not modify historical interoperability harnesses to make this pass green.

## Fixed scope boundary

Forbidden during this pass:

```text
Emissary/i2pd/Java validation
NTCP2 correction/activation
SSU2
Q1/Q2
rootless namespaces
Docker / Multipass / VM work
Python interoperability harnesses
public I2P network
NetDB live integration
new generic router dispatcher
garlic / LeaseSet / streaming / SAM / I2CP
Plan 117 execution
```

There is no environment blocker for this work.

## Terminal state

The terminal cleanup is complete:

```text
exact_duplicate_accounting             = passed-noop-exact-duplicates
exact_duplicate_budget_behavior        = passed-noop-at-limit
exact_duplicate_expiry                 = passed-no-refresh
completion_accounting                  = passed-zero-after-complete
first_delivery_duplicate_identity      = passed-conflict-detected
first_delivery_conflict_invalidation   = passed
fragmented_cross_tunnel_out_of_order   = passed-out-of-order-exact-bytes
current_test_names_recorded             = exact
status_handoff_authority                = synchronized
workspace_validation                    = passed-or-preexisting-env-only
plan_116                                = passed-final-local-closure
plan_117                                = unblocked-ready-for-planning
```

Planning may now move to Plan 117.
