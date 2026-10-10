# Plan 448 — Enforced global bandwidth governor and honest RouterInfo bandwidth class

Status: **registered / ready**. Date: 2026-10-10. Owner: floodfill / router capacity policy with daemon/runtime transport adapters. New production-infrastructure corrective to blocked Plan 444 (\`plans/closure/floodfill/444-status.md\`) and predecessor to the Java candidate-selection remainder of Plan 444 and original Plan 437. Roadmaps: \`plans/subsystems/floodfill-roadmap.md\` and \`plans/subsystems/core-router-recovery-roadmap.md\`. Baseline: \`work/plans-440-444-emissary-evidence-recovery\`; recheck exact working commit.

## Objective and why this is the blocker

Plan 444 audited pinned Java I2P 2.13.0 and verified that it obtains the RouterInfo bandwidth tier from **configured in/out byte rates and share**, with class boundaries \`<12 K\`, \`<=48 L\`, \`<=64 M\`, \`<=128 N\`, \`<=256 O\`, \`<=2000 P\`, otherwise \`X\` after integer KB/s share normalization, subject to hidden/insufficient-tunnels rules. i2pr presently has bounded queues/transit counters but **no global operator-configured ingress/egress throughput enforcement**; setting \`O\` manually or measuring synthetic traffic would lie about committed capacity. Implement an end-to-end capacity owner and derive the class from the same *enforced* configuration. Do not make availability dependent on sustained historical traffic when idle.

Emissary's \`BandwidthTracker::new\` takes configured bandwidth × share ratio to derive class and measures observed congestion separately. Use it as a **read-only behavioral reference**; derive all i2pr source independently and audit full reference behavior against I2P specs. The ordinary router class is useful before floodfill; setting \`f\` is a separate later role gate.

## Classification, ownership and boundaries

**Infrastructure:** normalized operator throughput policy; checked-rate arithmetic; shared inbound/outbound byte accounting; runtime/transport-owner pacing/admission; class/health projection. **Invariant:** never advertise more shared bandwidth than configured *and enforced*, never create public \`R\` or \`f\` from throughput values, no unbounded queue/backpressure, no adverse priority inversion under load, no operator-supplied cap-string override, no unit ambiguities. **Capability:** eventually honest bandwidth class in signed local RouterInfo and checked Java candidate admission; not a floodfill service or two-family conformance on its own. **Polish:** redacted local usage/capacity diagnostics and operator docs. Out: floodfill-serving activation, implementation of transit M11, new protocols, full network-wide performance calibration, enforced throughput proof from claimed host NIC speed.

## Bounded implementation

1. **Owner/contract audit:** Inventory \`crates/i2pr-daemon/src/config.rs\`, SSU2/NTCP2 runtime shared link resources, existing transport budgets and M11 transit accounting; choose one runtime-neutral typed capacity policy and one daemon/runtime shaper owner. Preserve i2pr runtime's exclusive async I/O ownership and transport-neutral per-link admission. Map existing unused capacity fields carefully; no double-limiting or tenant side channel.
2. **Operator policy/schema:** Introduce explicit, versioned \`inbound_bytes_per_second\`, \`outbound_bytes_per_second\`, \`share_percent\` (range 0–100) plus a conservative disabled/unconfigured compatibility mode. Specify whether a zero means explicitly no transit or invalid config; never treat missing as unlimited-but-high-tier. Reserve room for local-service traffic, shared/transit and control packets. Add strict \`deny_unknown_fields\`/normalization, bounded max rates, checked integer math and actionable typed preflight errors **before** socket creation. No silent automatic high-tier defaults.
3. **Real enforcement:** Implement token-bucket or equivalent bounded clock-backed pacing for *actual network bytes* at common transport ownership boundaries, distinct incoming and outgoing ceilings, per-peer fairness and a global budget; enforce outbound pacing including framed/encrypted transport overhead and inbound processing/backpressure so bounded oversized/malicious ingress cannot bypass caps. Explicitly distinguish a physical NIC's unthrottleable inbound wire traffic from i2pr's bounded accepted/processed ingress; configure truthful advertised **share** using demonstrable capacity, and do not claim a hard physical inbound cap where OS limitations prevent it. Distinguish inbound protocol and application traffic. All pending byte queues must remain resource bounded and cancellable.
4. **Priority/fairness:** Allocate bounded local client, NetDB/transport control, transit and floodfill reservations; transit/floodfill shed first under pressure; preserve minimum control-flow budget to prevent deadlock. Never prioritize one unauthenticated peer enough to starve real clients. State the exact policy for default-disabled transit. Release allocations on timeout, socket close, restart and peer migration. All transitions version/generation tracked; stale metrics cannot authorize claims.
5. **Classification and RouterInfo:** Derive \`min(configured inbound, configured outbound) × share_percent\` with verified pinned-Java integer rounding and binary/decimal KB conversion; name which byte-rate units are canonical. Test exact \`N\` vs \`O\` threshold (**at 128 KB/s Java says N; above 128 it says O**) and K/L/M/O/P/X boundaries, hidden mode and tunnel-capacity restrictions as applicable. Class must be conservative on insufficient headroom, restart and enabled limit differences. Only sign a changed \`caps\` record through the Plan 430 health/eligibility gate, never direct operator text or a fixture override. Firewall/reachability \`R\`, floodfill \`f\` and congestion \`D\`/\`E\` have independent sources.
6. **Measured behavior:** Deterministic fake-clock and local socket tests under burst, sustained traffic, simultaneous peers, asymmetric in/out, graceful shutdown, cancelled outstanding sends, disabled profile and peer restart; assert released permits, bounded buffering and multi-transport shared ceiling. Measure actual emitted socket bytes or controlled observed rate, not merely counters. Include negative tests for overflow, 0 share, underconfigured O, accidental percent/fraction interpretation and a source mutation that would bypass the scheduler.
7. **Follow-on selection handoff:** Provide a safely generated signed controlled RouterInfo class and an independent known-good Java 2.13.0 candidate-selection test fixture; **full Plan 444 still requires** stock Java (healthy bootstrap, ineligible negative, eligible positive) and Plan 437 later requires normal qualified reachability/transit/role health. This plan is not passed merely because the class mapper compiles; exact live enforcement + truthful class evidence is mandatory.

## Failure, cancellation, restart, migration

No raw identity or peer hash in labels; finite task scopes, disabled queues released on config replacement, generation-aware atomic shaper update and RouterInfo withdrawal/downgrade on loss of reservation or service health. Existing configs load without silently advertising higher classes; feature needs explicit operator selection. If a bandwidth limit cannot be met or a global pathway evades shaping, **stop** before publishing an unsupported class and record a narrow owner-specific defect.

## Verification

\`\`\`sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon -- --test-threads=1
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-netdb --all-targets
bash scripts/check-m12-floodfill-boundaries.sh
python3 scripts/check-core-router-recovery-contract.py --self-test
python3 scripts/check-global-plan-number-uniqueness.py
# new shaper/class exact-threshold, contention, outbound/inbound and restart tests;
# full AGENTS.md routine floor on implementation HEAD
\`\`\`

**Pass** on real common ingress processing/outbound byte pacing in both transport-owner profiles, deterministic safety/fairness/restart evidence, class calculated from that enforced reservation, mutation-tested inability to advertise a too-high class, and no \`f\`/\`R\` promotion. **Do not close Plan 444** until its Java candidate-selection positive and negative references run; this plan closes only the capacity prerequisite. Closure \`plans/closure/floodfill/448-status.md\` must include owner paths, actual bytes/rates/units, commands/CI, resource baseline, migration/security review and Plan 444/437 unblock audit. The Plan 444/306 historical stopped/blocked evidence stays immutable.
