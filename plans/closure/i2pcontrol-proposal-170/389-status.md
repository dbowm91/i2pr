# Plan 389 closure — blocked

Status: **blocked-destination-role-registry-capacity-plan-390**.

Plan: `plans/implementation/i2pcontrol-proposal-170/389-post-start-inbound-build-completion-corrective.md`.

## Evidence and finding

Plan 389 added bounded per-Destination build-stage and registration-stage
counters, generation-pruned with the owning Destination, plus regression
coverage for fair scheduling and generation cancellation. The clean exact-pinned
i2pd 2.61.0 NONE diagnostic reached the reverse authority stage, but its
control-created server still failed to establish usable inbound ownership:

- `target/interop/els2-evidence-plan389-none-registration-stage-retry2-20261009`
- Result: authority payload passed; reverse server was committed; the test
  stopped at local DatabaseStore delivery admission after 185.72 seconds.
- The bounded Destination snapshot reported 67 inbound submissions, 61
  completed materials, 4 failed builds, and 61 registration failures. All 61
  failures were classified as coordinator role activation; pool admission,
  tunnel activation, coordinator lifetime, and missing-material failures were
  zero. No inbound registrations, usable lease, or LS2 were present.
- The source root is the shared `ExploratoryBuildCoordinator`'s
  `DataPlaneRegistry`: its inbound/outbound capacity is derived from the
  exploratory pool's small maxima, although post-start Destination pools now
  also register roles in that same registry. The fixed capacity is exhausted
  before the per-Destination pool targets can be met. The registry's historical
  documentation explicitly says no independent capacity exists, so changing
  it requires an explicit bounded aggregate-capacity and lifecycle design.

This is a product defect localized by Plan 389, not an i2pd process failure.
The clean lane did not return the reverse payload. No PSK/DH row or full floor
was run because NONE did not pass.

## Implementation and checks

Changed files include `service_product.rs` (bounded counters and registration
stage classification), `exploratory_build.rs` (generation-cancellation
regression), the Plan 389 checker and lane metadata, and the daemon architecture
deep-dive. Verification completed before closure:

- `rtk cargo fmt --all --check` — pass.
- `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — pass.
- Focused destination replenishment and coordinator cancellation tests — pass.
- ELS2 runner self-test and live-evidence checker self-test — pass.
- ELS2 checker mutation table — 18 detected, 0 missed, 2 controls.
- Live evidence checker, encrypted-service consumer caller guard, and `git diff --check` — pass.
- Exact-pinned NONE lane — failed at the product boundary described above.

The diagnostic artifact is retained verbatim. Raw i2pd logs are not closure
evidence. No support inventory, conformance, capability, or advertisement was
promoted. No dependency was added.

## Successor and unblock audit

Plan 390 is registered to define and implement a bounded aggregate role
registry capacity for the coordinator's mixed exploratory and Destination role
owners, including expiry/removal behavior and regression coverage, then rerun
the exact-pinned NONE lane. The correct bound must derive from explicit maximum
Destination/group counts and per-Destination pool limits, not an unbounded map
or a silently enlarged constant.

Plan 374 remains blocked on the live i2pd reverse matrix and is not unblocked.
Plan 375 remains independently blocked on Java source-lock and live evidence.
Plan 376 is already passed. Plan 377 still depends on 374 and 375; Plan 378
still depends on 377. No other downstream plan can be unblocked by this
diagnostic.
