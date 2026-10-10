# Plan 440 — Emissary-informed router/test architecture and evidence-contract reconciliation

Status: **registered / ready**. Owner core-router-recovery; evidence plan for the new corrective sequence; predecessor Plan 430 passed, Plan 432 passed. Base: recovery branch `work/plans-430-439-core-router-recovery` with Plan 424 terminal. Roadmap `plans/subsystems/core-router-recovery-roadmap.md`.

## Objective and readiness

Reconcile the actual mixed-router test *topology, permissions, evidence oracle and source-owned router lifecycle* before another NTCP2 micro-corrective. Formally allow a **controlled integration** progression that is independent of external/non-loopback SSU2 qualification, without qualifying public network claims. The Emissary read-only comparison is recorded in `plans/diagnostics/2026-10-10-emissary-router-qualification-comparison.md`, independently pinned fork `6885a945` and upstream `9b43484a`. No source reuse except the separate Proposal 170 exception.

## Classification, invariants, out of scope

**Capability:** no public exposure; a tested plan contract for accurate router viability. **Infrastructure:** test-profile taxonomy, independent positive/negative control oracles, correlation and evidence-lifecycle rules, separate local product and external qualification gates. **Invariant:** AGENTS.md external-router restrictions remain; no second transport owner, no fake RI, no caps override, no secret/identity-bearing persistent traces, two-family conformance unchanged. **Polish:** comparison/diagnostic documentation and anti-vacuity checkers. Out: direct protocol fix; public-network access; patching stock Java/i2pd/Emissary; blanket conclusion that loopback is invalid.

## Ordered work packages

1. Pin and independently trace Emissary (router initialization, two transport registrations, NetDB/tunnel/router lifecycle, allow_local, test cases, publication, bandwidth). Verify with specs and i2pr's current source; classify adoptable behavioral **contracts** vs rejected design choices (e.g. operator caps override, global router service locator, broad copy).
2. Freeze test matrix and meanings: `unit` / `loopback-transport` / `controlled-multi-router` / `independent-private-LAN` / `normal-network`. Local loopback is allowed solely as transport and controlled product evidence; no use for R/direct public address claims. Clarify that a stock reference router's **candidate selection, tunnel-building prerequisites, network ID 2 and unique router identity** are part of test validity. Require stock-to-stock reference positive control on suspect test topologies.
3. Define finite evidence oracle per protocol with phase gates and direct consumption: deterministic exact stage before connection; per-session in-memory correlation without persisting raw peer hash/endpoints/packet; signed RI accepted; bidirectional authenticated link; decoded I2NP; normal router integration; controlled product acceptance. Persist allowlisted phase status and bounded counts only.
4. Define a **separate explicit qualification lane policy** for owner-authorized two-host private LAN, without changing routine AGENTS.md prohibitions. When environment unavailable, classify `environment-blocked` and allow independent production work under separate closure; no false pass or silent downgrade.
5. Amend forward successor plans and roadmap/registry: allow Plan 443 controlled-router integration once Plan 440 and prior Plan 432 are closed; keep original Plan 433 external operational gate on Plan 431. Require 441 NTCP2 runner positive control before any additional reverse-only live budgets. Seed independent Java exploratory references before testing target floodfill selection.
6. Add a source checker/self-test that fails if a test uses a fake success event, ambiguous process-wide count as session acceptance, failed reference control as i2pr protocol blame, or turns loopback into public reachability. No new status claim.

## Failure, cancellation, contention, migration

Plans are evidence only. Fixture processes must have bounded startup/handshake/shutdown, port reservation and explicit cleanup; never retain unredacted raw logs. Separate phase-unavailable from protocol-rejected. Historical plan 410–424 closures unchanged; newly registered successors have their own closure. No config migration.

## Verification and closure

Commands: `python3 scripts/check-global-plan-number-uniqueness.py`, `python3 scripts/check-adr-number-uniqueness.py`, `python3 scripts/check-router-readiness-contract.py --self-test`, `python3 scripts/check-current-pin-ntcp2-runner.py --self-test`, `cargo test --locked -p i2pr-interop --all-targets`, `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets`, full AGENTS.md floor if production/test code changes. Check source existence at execution HEAD; unknown commands must be corrected before a closure attempt.

Acceptance: owner-reviewed test/profile decision, valid stock positive controls and negative controls that can actually falsify a result, immutable historical evidence, and 441–444 dependencies set precisely. STOP when source differences from Emissary cannot be reconciled with GUARDRAILS/protocol specs; record decision rather than silently relaxing. Closure `plans/closure/core-router-recovery/440-status.md` must contain commits, reference pins, exact check outputs, findings, security review and which successors are ready.
