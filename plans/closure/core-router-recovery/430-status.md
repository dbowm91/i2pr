# Plan 430 status: passed

Closure token: `passed-core-router-baseline-and-public-role-safety-gates`

Plan: `plans/implementation/core-router-recovery/430-executable-baseline-and-public-role-gates.md`

## Implementation commits

- `9e7926cc` — activate Plan 430.
- `a16f0794` — add the runtime-neutral router readiness and advertisement decision contract, source inventory, and CI mutation guard.
- `92978da2` — extend the guard to cover daemon/config/RouterInfo owners and mutation controls.
- `fd6b41ea` — format the profile predicate to satisfy workspace Clippy; align the source guard with formatted Rust.

The final implementation source SHA before this closure record is `fd6b41ead53fdfad86f230d105e0e2977f92830a`. This plan changed no transport activation, network exposure, `specs/support.toml`, or advertised capability.

## Requirement-to-evidence matrix

| Requirement | Result and evidence |
| --- | --- |
| Exact-head source census | The implementation plan now records the identity, SSU2/NTCP2 config and socket, local RouterInfo, reseed/cache, NetDB/tunnel, transit, and floodfill owners. It corrects the registration-era descriptions where source has since gained guarded floodfill and transit infrastructure. |
| Frozen profiles and readiness | `i2pr-core::router_readiness` defines `IsolatedTest`, `ControlledInterop`, `NormalRouter`, and `OptionalNetworkRole`; ordered network-readiness stages plus `Degraded`; and typed rejection reasons. Tests prove process serving does not satisfy transport readiness and test profiles cannot publish claims even when all input facts are true. |
| No-false-claim decision | The evaluator requires current readiness, owner health, address reachability, independent protocol qualification, operator authorization, and current generation. It returns a decision only, never an authorization token. Existing opaque floodfill permits and the normal local RouterInfo builder remain authoritative for actual publication. |
| Negative controls | `scripts/check-router-readiness-contract.py --self-test` mutation-tests the profile/readiness/evidence gates and checks the current SSU2 bind/advertisement restrictions, NTCP2 activation refusal, RouterInfo signature/freshness/identity/endpoint checks, default-off transit/floodfill, and corresponding negative regressions. The M12 floodfill checker self-test passed 11/11 mutations. |
| Planning handoff | Plans 431, 432, and 434 were unblocked to `ready`; 433, 435–439 remain gated on their documented successors. The floodfill source-comment discrepancy below is assigned to Plan 438 before any promotion. |

## Commands and outcomes

All outcomes below are local Linux results; no hosted CI result is claimed.

- `rtk cargo fmt --all --check` — passed after formatting the `matches!` predicate.
- `rtk cargo check --locked --workspace --all-targets` — passed.
- `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed.
- Linux secured-fixture preparation (`cargo rustc ... +crt-static`, copy to `i2pr-app-fixture-secured`, then ordinary fixture rebuild) — passed.
- `rtk cargo test --locked -p i2pr-daemon -- --test-threads=1` — passed after fixture preparation: 1,960 passed, 37 ignored, 86 suites.
- `rtk cargo test --locked -p i2pr-netdb --all-targets` — passed: 276 passed, 7 suites.
- `rtk cargo test --locked -p i2pr-core --all-targets` — passed on the final contract implementation: 19 passed, 1 suite.
- `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed after the profile predicate was changed to `matches!`.
- Full serial workspace tests — 4,751 passed, 38 ignored across 186 suites. This run preceded only the semantics-preserving `match` to `matches!` Clippy correction; the final core tests and whole-workspace Clippy passed afterward.
- The 62-command AGENTS routine floor was executed. Documentation, doc tests, boundary/evidence checkers, planning tests, tooling/license/workflow inventory, vector checkers, and `cargo deny` all passed. The first Clippy attempt exposed the corrected style lint; the readiness source checker also needed its pattern updated after rustfmt, then both standard and mutation modes passed. The remaining floor commands passed on resumption. The floor's Plan 193 streaming evidence checker emitted its existing `WARN` diagnostics and returned success as designed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `rtk python3 scripts/check-adr-number-uniqueness.py` — passed.
- `rtk bash scripts/check-ntcp2-interoperability.sh` — passed as a static historical-boundary check only; no NTCP2 interop lane ran.
- `rtk bash scripts/check-m12-floodfill-boundaries.sh` and `--self-test` — passed; 11/11 negative mutations rejected.
- `rtk python3 scripts/check-router-readiness-contract.py` and `--self-test` — passed.

No external I2P network was contacted, and no reference router was started for Plan 430.

## Compatibility, security, and operations

- No configuration schema or persistent-data format changed. Existing default-off behavior and Plan 101 NTCP2 refusal remain in force.
- No secrets or private identity material were added to the new contract or checker.
- `AdvertisementEvidence` carries factual inputs from owning services; callers must not derive qualification or reachability from config intent or a loopback probe. The returned `Result` is not a permit. Plan 431 must wire generation-bound owner evidence before any non-loopback address publication.
- Shutdown, restart, and resource behavior are unchanged; this plan owns no task, socket, timer, or persistent state.
- `specs/support.toml` and protocol conformance claims are unchanged.

## Findings and limitations

- **Medium — floodfill qualification comment/source drift.** `crates/i2pr-daemon/src/floodfill.rs` describes the normal eligibility path as having passed two-family qualification via Plan 279. The authoritative `plans/closure/floodfill/279-status.md` says Java stopped before any matrix row; Plan 306 records the later bandwidth-tier boundary. The comment is not evidence and does not change this plan's result. Plan 438 must reconcile the source description and qualification contract before any normal `caps=f` promotion. Earlier closure records are not rewritten.
- **Low — future evidence producer ownership.** The generic contract is decision infrastructure, not a standalone security boundary. The daemon composition must feed it owner-derived, generation-current evidence and retain existing owner-specific publication permits. This is an explicit Plan 431 handoff condition.
- Full two-family transport, online reseed, independent-router multihop, NTCP2, normal transit, and two-family floodfill remain unqualified and unadvertised.

No migration or corrective successor is required for Plan 430. Its source-comment finding is owned by the already registered Plan 438.

## Unblock audit and roadmap disposition

The hard dependency on Plan 430 is now closed. Plans 431 (SSU2), 432 (HTTPS SU3 reseed), and 434 (NTCP2 authenticated-link diagnosis) have no other registered hard dependency and their interface inputs are documented, so all three move from `blocked` to `ready`. Plan 433 remains blocked on 431 and 432; Plan 435 on 433 and 434; Plan 436 on 433; Plan 437 on 433 and 436; Plan 438 on 437; Plan 439 on 433, 435, 436, and 438. No other registered plan was unblocked by this closure.

Updated `plans/registry.md` and `plans/subsystems/core-router-recovery-roadmap.md`. `next_executable_plan = 431, 432, 434 (parallel; continue in registry order)`.
