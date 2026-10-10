# Plan 430 — Core-router executable baseline and public-role safety gates

Status: registered / **ready**. Date: 2026-10-10. Baseline: \`main@8fd5eb10824909ddebbd99c8b3800dac71eae211\`; recheck exact working HEAD before writing production code. Roadmap: \`plans/subsystems/core-router-recovery-roadmap.md\`.

## Objective and readiness

Establish one executable, source-verified baseline and explicit capability/readiness contracts shared by SSU2, reseed, NTCP2, NetDB, transit and floodfill. It is the **only immediately executable recovery plan**; no transport/public role is activated by this plan. Existing Plans 099/101, 161, 268, 303/306 and 360 are closed baseline evidence; no new prerequisite is assumed.

## Current implementation evidence

\`crates/i2pr-daemon/src/config.rs::normalize_ssu2\` currently rejects non-loopback SSU2, \`advertise=true\` and introducer service; \`normalize_ntcp2\` rejects enabled NTCP2. \`bootstrap_daemon\` uses offline reseed only. \`specs/support.toml\` retains experimental transport status; M11 transit off; M12 normal floodfill off. Confirm actual code versus stale prose (\`plans/registry.md\` and older deep dives can lag closure records). Reference exact pins in \`specs/SOURCES.md\`.

## Invariants, scope, and exclusions

**Invariant:** all unsupported normal profiles remain forbidden and unadvertised; one signed RouterInfo owner, durable identity separation, no public/clearnet traffic in tests, no default-on network capability. **Capability:** none; this pass is evidence infrastructure. **Infrastructure:** typed service readiness/advertisement decision contracts and a read-only, versioned capability/gate ledger with deterministic inputs. **Polish:** operator diagnostics/plan authority consistency. Out: reimplement transport, change NTCP2/SSU2 cryptography, activate transit or floodfill, edit prior closure records, manufacture claims from a test runner.

## Ordered work packages

1. **Source census.** On an exact SHA, locate and document concrete source entry points for identity loading, SSU2/NTCP2 config normalization/listener creation, RouterInfo construction/publication/withdrawal, reseed ingestion, NetDB client/store, tunnel admission, M11 and M12 role selectors. Compare test claims vs source behavior; record all unresolved contradictions with file and line evidence.
2. **Frozen profile matrix.** Distinguish \`isolated_test\` (non-I2P network ID, controlled peer endpoints), \`controlled_interop\` (isolated exact-reference peers), \`normal_router\` (real I2P network ID, qualified authenticated transports) and \`optional_network_role\` (transit/floodfill opt-in). Define readiness stages \`ProcessServing\`, \`PeerDatabaseUsable\`, \`TransportUsable\`, \`TunnelPoolsUsable\`, \`ApplicationRoutingUsable\` and \`Degraded\`. No serving-state bit can impersonate network readiness.
3. **No-false-claim gate.** Introduce a small runtime-neutral eligibility/claim contract consumed only at daemon/router-info composition: verified owner health, address reachability, independently qualified protocol/profile, operator authorization, withdrawal generation, and explicit typed reason for rejection. Avoid hardcoded trust based on config or one loopback probe. Keep old strict posture for every unsupported profile; never allow a new flag to bypass conformance.
4. **Executable negative controls.** Add integration tests and a mutation-tested static checker proving the baseline rejects all unauthorized NTCP2/SSU2/transit/floodfill public exposures, false readiness, forged eligibility evidence and unverified RouterInfo records; demonstrate that a test actually exercises each guard.
5. **Planning handoff.** Record exact contracts needed by 431/432/434; reconcile only changed *planning* and scope text, not legacy closure/history.

## Failure, restart, and contention policy

On restart, unverified reachability/health and prior in-memory test qualification expire; persisted cryptographic identity is not itself an advertisement permit. All transitions are monotone per owner generation, fail-closed during incomplete start/reconfigure and require bounded resource accounting. No network side effects in dry-run or disabled profiles.

## Compatibility/migration

Existing TOML and disabled defaults remain accepted. Any future normal/public configuration additions must be opt-in, schema-validated before binding; this plan must not normalize \`enabled=true\` into \`supported\`. No on-disk migrations.

## Verification commands and evidence

\`\`\`sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-m12-floodfill-boundaries.sh
# plus the full AGENTS.md floor, new anti-vacuity checker, and CI
\`\`\`

Acceptance: one checked-in truthful source/evidence inventory, reviewed profile/readiness/eligibility contract, negative tests that detect deliberate forbidden activation and false-ready mutation, and deterministic status projection with no current \`specs/support.toml\` promotion. Stop if branch HEAD changed incompatible owner contracts or the proposed claim gate requires unapproved architectural changes; file a new ADR or corrective. Closure \`plans/closure/core-router-recovery/430-status.md\` must report exact commits, command outcomes, tests not run, guard-mutation results, unresolved risk severities, and explicit 431/432/434 readiness disposition. Handoff lists source files, migrations (none expected), and deviations.
