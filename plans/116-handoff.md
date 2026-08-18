# Plan 116 handoff

- Status: **terminal-cleanup-pending**
- Date: 2026-08-18
- Original plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Prior correction: [`116-completion-correction.md`](116-completion-correction.md)
- Prior final-closure pass: [`116-final-closure.md`](116-final-closure.md)
- **Execute now:** [`116-terminal-cleanup.md`](116-terminal-cleanup.md)
- Current status authority: [`116-status.md`](116-status.md)
- Predecessor: [`115-status.md`](115-status.md)
- Plan 117: **blocked until this terminal cleanup succeeds**.

## Start here

Do not restart Plan 116 from the beginning.

The major local data-plane implementation is already present and should be retained. The only remaining work is the terminal cleanup described in [`116-terminal-cleanup.md`](116-terminal-cleanup.md).

Current substantive implementation:

```text
0330fb2e9e64dd0877472c930606ab4219ac18a9
```

Current state:

```text
plan_115_Q0                       = passed-emissary-native-consumer
plan_116_material_transfer       = passed-local
plan_116_pool_material_only      = passed-local
plan_116_fragment_boundaries     = passed-local
plan_116_exact_byte_cross_tunnel = passed-ordered-local
plan_116                         = terminal-cleanup-pending
plan_117                         = blocked-on-plan116-terminal-cleanup
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
```

Expected production changes are narrowly concentrated in:

```text
fragment.rs     duplicate accounting / expiry / delivery metadata integrity
roles.rs        out-of-order fragmented cross-tunnel acceptance test
```

## Remaining defects

### T1 — exact duplicates are not resource-accounting no-ops

Current behavior can logically retain no new fragment bytes while still incrementing `aggregate_bytes` by `fragment.body_len()`.

The duplicate is also processed after `last_touched_ms` is refreshed, allowing duplicate traffic to extend the life of an incomplete partial.

Required behavior for an exact duplicate:

```text
retained bytes unchanged
partial count unchanged
aggregate_bytes unchanged
last_touched_ms unchanged
no aggregate-limit rejection merely for being a duplicate
```

Classify duplicate vs new unique fragment before applying added-byte accounting.

### T2 — first-fragment delivery metadata must participate in duplicate identity

For a fragmented message, the first fragment owns the delivery instruction.

These are not equivalent duplicates:

```text
same first body + ROUTER A
same first body + ROUTER B
```

or:

```text
same first body + TUNNEL(router A, id X)
same first body + TUNNEL(router A, id Y)
```

A metadata conflict must invalidate only that partial and release its retained-byte accounting.

An exact first duplicate requires both:

```text
same body
same delivery instruction
```

### T3 — add the missing out-of-order full cross-tunnel proof

The existing fragmented cross-tunnel test now executes all roles and returns exact bytes, but endpoint delivery is still canonical-order.

Extend it or add a new test so that after IBGW + inbound-participant processing, valid endpoint TunnelData cells are reordered.

At minimum:

```text
follow-on arrives before first
last follow-on may arrive before first
first fragment arrives later with delivery metadata
endpoint emits exactly once
final bytes == original standard I2NP bytes
```

Do not call `BoundedReassembler` directly in this terminal role-level proof.

## Required implementation order

```text
1. duplicate classification + exact accounting delta
2. duplicate expiry no-refresh
3. first-delivery metadata conflict detection
4. targeted fragment/reassembly tests
5. out-of-order full cross-tunnel exact-byte test
6. full workspace validation
7. synchronize status/handoff/current documentation
```

## Mandatory tests

The implementation must include active tests covering at least:

```text
exact duplicate first retained bytes unchanged
exact duplicate follow-on retained bytes unchanged
exact duplicate accepted at aggregate budget ceiling
exact duplicate does not refresh expiry
completion after duplicates leaves retained_bytes == 0
exact first duplicate + same delivery is idempotent
first duplicate + different ROUTER target invalidates partial
first duplicate + different TUNNEL target invalidates partial
out-of-order full fragmented outbound -> inbound exact-byte trajectory
```

The detailed plan gives preferred names and exact semantics.

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

## Terminal state

Do not close until:

```text
exact_duplicate_accounting             = passed-noop
exact_duplicate_budget_behavior        = passed-noop-at-limit
exact_duplicate_expiry                 = passed-no-refresh
completion_accounting                  = passed-zero-after-complete
first_delivery_duplicate_identity      = passed
first_delivery_conflict_invalidation   = passed
fragmented_cross_tunnel_out_of_order   = passed-exact-bytes
current_test_names_recorded             = exact
status_handoff_authority                = synchronized
workspace_validation                    = passed-or-preexisting-env-only
plan_116                                = passed-final-local-closure
plan_117                                = unblocked-ready-for-planning
```

Only after that should planning move to Plan 117.