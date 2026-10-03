# Plan 296 — Tunnel pool shaping residuals and reply-bundling primitive

Status: registered-prop170-pool-shaping-residuals-blocked-on-plan292

Classification: capability.

Hard dependency: Plan 292 closed with the exact `CorrectivePending{plan: 296}` residual set.

## Objective

Close the Plan 292 matrix cells that need real tunnel-pool or
destination-stack semantics: backup quantity, length variance,
multihoming, and reply bundling. Plan 292 implements the
dependency-ready shaping cells (length/quantity, per-direction
projection) and records these four as named corrective residuals with
failing-before-allocation boundary behavior; this plan provides the
missing primitives and flips them to apply.

## Scope (Plan 292 residual identities)

- `tunnel_backup_quantity` (all types): standby tunnel semantics in the
  pool owner (`i2pr-tunnel` pool config plus destination projection):
  how many extra tunnels build and hold ready, when standby tunnels
  promote on primary failure, and how standby interacts with
  `failure_threshold` and build budgets. Proposal bound 0–3.
- `tunnel_variance` (all types): per-build length randomization around
  the configured length within Proposal bound −2..+2, floored at the
  pool minimum hop policy, with deterministic-test hooks (injected
  randomness, never ambient RNG in unit paths).
- `multihoming` (server families): exact semantic after the Plan 292
  finding that the pinned PR6 reference maps `MultiHoming` onto the
  I2CP session reply-info flag rather than target selection. This plan
  resolves the conflict explicitly: either a session reply-info
  primitive in the destination stack, or a documented target-selection
  semantic for `targets[1..]` (today first-only), or both with the
  precedence stated. No silent reinterpretation.
- `reply_bundling` (all types): garlic reply-bundling primitive in the
  delivery owner, or a proven statement that the delivery path already
  bundles and the flag maps onto it. New delivery behavior needs the
  same bounded-queue and typed-error discipline as the rest of the
  data plane.

## Non-goals

No signature agility, LeaseSet security, or outproxy provider work
(Plan 293). No TLS identity work (Plan 297). No new wire keys.

## Evidence

- Per-cell positive tests (each flag changes its owner's behavior)
  and negative tests (bounds, contradictory combinations).
- Pool-level tests for standby promotion and variance distribution
  with injected randomness.
- Matrix re-evaluation: the 39 Plan 296 cells flip to apply with
  named owners; `CORRECTIVE_296_CELLS` goes to zero.
- No regression of Plans 289–294 control and product paths.

## Acceptance criteria

Plan 296 closes when every residual cell it owns is consumed by a
real owner, the matrix leaves no `CorrectivePending{plan: 296}` cell,
and the routine floor (including the 292 boundary census tests) is
green on the closing commit.
