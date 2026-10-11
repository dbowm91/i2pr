# Plan 444 — Configured shared bandwidth semantics and independent Java floodfill-selection controls

Status: **blocked** (2026-10-10), closure `plans/closure/floodfill/444-resume-status.md` (`blocked-java-selection-and-store-lookup-controls-not-run`). Plan 454 supplied the shared enforced governor and capacity permit, and this resume added the pinned Java floodfill-role minimum to capacity-bearing RouterInfo construction. The source-locked Java selector and real store/lookup controls remain unrun. The historical `plans/closure/floodfill/444-status.md` is unchanged. Plan 440 is closed. This remains bounded infrastructure/one-family admission work feeding Plan 437; full normal floodfill eligibility still requires Plans 433/436.

## Objective and baseline

Plan 306 proved Java recognizes and lists the controlled i2pr `caps=fR` but assigns bandwidth tier `Unknown`, never selects that floodfill and hence fails to build client tunnels. Plan 303 i2pd one-family floodfill matrix passed. Build a **standards-accurate** bandwidth class from configured, enforced shared participation budget and separately verify Java peer selection in a non-deadlocking stock topology. Avoid imposing the Plan 437 assumption that idle router must have a historical measured data-rate greater than the class before advertising it.

## Research/evidence and classification

Official `https://www.i2p.net/en/docs/overview/network-database/` specifies share-bandwidth classes K/L/M/N/O/P/X, with capability `O` specified at 128–256 KB/s shared bandwidth and the published floodfill minimum at 128 KBytes/sec. **Determine exact Java 2.13.0 units, threshold inclusivity and configured-share semantics against pinned source before implementation**; do not silently substitute Kbit/s or a binary-unit assumption. Floodfill automatic eligibility requires at least 128 KBytes/sec shared plus separate health signals. Emissary `emissary-core/src/subsystem/bandwidth.rs::BandwidthTracker::new` derives congestion-independent `Bandwidth::from(config.bandwidth × share_ratio)`; transport manager publishes that class and separately adds congestion flags from observed utilization. This behavior informs contract, not source to copy.

**Capability:** correctly classified controlled I2P RouterInfo recognized by stock Java with candidate selection. **Infrastructure:** normalized operator ingress/egress rate limits, share fraction, resource-governed transit reservation, a runtime-neutral class mapper and healthy eligibility source. **Invariant:** no manual `caps` injection, no fabrication of reachable/public endpoint, no class above enforceable configured capacity, no unconditional floodfill or version flag. **Polish:** documentation and unit-consistent diagnostics. Out: public floodfill opt-in, capacity proof by low/no traffic, patched reference clients, changing Plan 306 historical records.

## Ordered work packages

1. Source-lock current Java 2.13.0 router bandwidth-class and candidate eligibility branches, exact thresholds/units, and official docs. Decide whether the class derives from committed outbound share, bidirectional min, or another normative budget; name exact config fields and enforce the same policy in the daemon. Include initial low-tier default, no unexpected elevation by network fixture overrides.
2. Define `SharedBandwidth` and `CapacityEligibility` from explicit configured rate/share with checked math and strict upper bound by enforced global rate limit. Separate slow usage/health meter from advertised **commitment** and reject higher claim on insufficient configured/host provisioned capacity; hysteresis/withdrawal for congestion, out-of-date measurement and policy changes. No requiring sustained traffic to start earning the class.
3. Add deterministic class boundary fixtures at each documented transition, wrong units, zero share, overflow, rotation/restart, declined transit admission, contradictory operator rate limits, idle-but-configured, congestion and capped bandwidth tests; prohibit a manually supplied `O` for insufficient allocation.
4. Prepare three **separate** stock Java reference checks: (A) known-good floodfill and working exploratory/bootstrap peers; (B) deliberately low-class controlled i2pr candidate ranked behind a healthy candidate; (C) correctly classed controlled i2pr candidate preferred with otherwise identical topology. Pinned Java does not strictly reject every class-L remote candidate: its selector puts L in the last-resort group and can still return it if more candidates are needed. Observe signed RI parsed, genuine Java candidate/transport **selection**, and one real store/lookup once chosen; prevent Java's SAM startup from depending solely on an unselected target. Keep all reference components pristine and source-pin Java.
5. Preserve `R` reachability proof separate from class and `f`. If loopback controlled peer-test is allowed for test-only selection, the resulting `R`/class must be confined to that controlled profile; never allow public RouterInfo from it. Handle missing reference-selection prerequisites as fixture stop, not protocol failure. Compare i2pd controlled store/lookup regression with the Plan 303 matrix.
6. Hand only verified class and selection evidence to original Plan 437; require Plans 433+436 to qualify *normal floodfill service capacity* and Plan 438 for full two-family role, even if controlled Java admission succeeds here. Update docs and explicit role-health thresholds accordingly.

## Failure, migration, contention, verification

Shared rate budget must be enforced at actual transports/transit scheduling, with deterministic shed and all-path cleanup; early health loss withdraws floodfill eligibility without starving own-client traffic. No class from measurement of idle usage. Atomic config reconciliation; preserve existing default-off floodfill and local controlled behavior. Redact identities, reference logs; record only bounded classification/candidate-selection counts.

Commands: `cargo test --locked -p i2pr-core --all-targets`; `cargo test --locked -p i2pr-netdb --all-targets`; `cargo test --locked -p i2pr-daemon -- --test-threads=1`; `bash scripts/check-m12-floodfill-boundaries.sh`; source-verified `bash tests/integration/floodfill/run-i2pd.sh` and `run-java-floodfill.sh` with approved controlled reference topology; full AGENTS.md floor and class mapping fuzz/negative tests.

Acceptance only when the class is a truthful **enforced reservation**, stock Java genuinely prefers a correctly classed candidate over a low-class candidate with independent healthy bootstrap, and a real store/lookup is correlated to the selected candidate. Do not claim Java categorically rejects every remote L-class candidate; the pinned selector can return it as a last resort. No false caps leak. If an incapable host cannot supply documented configured share, record ineligible and stop; no fabricated tier. No normal public floodfill claim. Record the resume outcome separately from the historical `plans/closure/floodfill/444-status.md`; then Plan 437 owns deployment role eligibility and Plan 438 two-family full qualification.

## Resume outcome

The pinned Java source audit found that `FloodfillMonitorJob` admits its own
floodfill role only for class N/O/P/X, while `FloodfillPeerSelector` treats a
remote class-L candidate as last-resort and may still return it. The capacity
RouterInfo owner now refuses to combine `f` with a truthful class below N;
this is a local policy guard, not Java interoperability evidence. The exact
candidate preference and real store/lookup controls are still required before
this plan can pass.
