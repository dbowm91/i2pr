# Plan 437 — Truthful bandwidth class, reachability and Java floodfill selection

Status: registered / **blocked on Plans 433 and 436**. Roadmap: plans/subsystems/floodfill-roadmap.md and core-router-recovery-roadmap.md. Explicit successor for Plan 306, not a revision of its stopped record.

## Objective and historical issue

Close the **Java floodfill candidate-selection** boundary recorded in Plan 306 without false RouterInfo capabilities. Controlled i2pd accepted full floodfill matrix in Plan 303; Java 2.13.0 loaded, verified and listed the `fR` RouterInfo but did not select it because available bandwidth tier was Unknown. A synthetic `L`/`M`/`N` letter solely to obtain a pass is forbidden. Establish an operator-reproducible, actual-bandwidth-backed eligibility class and cause independent Java to select i2pr only if honest classification meets its policy. Do not reopen historical type-5 gaps closed by Proposal 170 Plans 350/351.

## Readiness and classification

Plan 433 must demonstrate healthy real-router transport/NetDB routing, Plan 436 a fair bounded transit capacity profile. **Capability:** Java-recognized eligible floodfill RouterInfo and controlled NetDB store/lookup exchange. **Infrastructure:** measured/configured throughput ceiling with truthful accounting and eligibility state, per-role bandwidth allocation and RouterInfo capability generation. **Invariant:** no unmeasured capability, no version bump beyond verified protocol support, no conflation of reachable with high capacity, no floodfill default activation. **Polish:** reasoned eligibility diagnostics, metric confidence/measurement methodology. Out: forcing Java selection by patching the reference, changing Java policy, public role exposure, unconditional declaration based on port open.

## Ordered work packages

1. Freeze official NetDB/RouterInfo bandwidth capability semantics and actual pin source: Java I2P 2.13.0 `RouterInfo` floodfill policy, `RouterProfile` selection and underlying bandwidth scheduler. Identify exact candidate eligibility predicates and required throughput/capacity units and thresholds. Document any network-version requirements separately.
2. Define a local **truthful capacity policy**: configure owner-acknowledged bandwidth share/ceiling, derive available floodfill headroom from runtime budgets and observed rolling throughput, clamp to actual service rate; require conservative expiry/recalibration and actual resource governance. No hard-coded high capacity for lab identity and no environment-variable bypass.
3. Implement runtime-neutral `BandwidthEligibility`/reasoned decision model and daemon projection into allowed RouterInfo bandwidth class letters only when the measured configured capacity satisfies the normative class. One peer-test result is never adequate evidence of capacity. Reject config requests for a class above actual measured/configured capacity.
4. Separate direct reachability proof (`R`) from capacity tier and floodfill (`f`); require all independently. Update floodfill role health and withdrawal lifecycle. If Java requires a minimum honest class this router cannot provide, classify the router **ineligible** and do not force a positive test.
5. Exercise one controlled i2pd regression of Plan 303 and a Java source-pinned test containing eligible and intentionally ineligible profiles, with stock Java making its own candidate selection and issuing actual DatabaseStore/DatabaseLookup. Record whether a tested host really meets capacity; vary only legitimate quota/bandwidth configurations.
6. Add unit/property tests for thresholds, overflow, depleted capacity, restarted snapshots, hostile peer-published values, cap downgrade and dynamic withdrawal. No reliance on instantaneous artificially generated I2P traffic to claim nominal throughput.

## Failure / restart / contention / migration

Measured bandwidth windows do not persist as unconditional capacity claims; use bounded window data and cold-start conservative tier until enough corroboration. Enforce floodfill serving headroom before admitting work. Test accounting when own-client and transit workloads saturate budgets. No downgrade of consent/role gates; existing `fR` controlled fixtures remain reproducible with explicit classification.

## Acceptance, verification, stop

Require a documented evidence-backed class mapping; exact-pinned Java **actual candidate selection** when demonstrably eligible, genuine request/response bytes, negative ineligibility controls, correct caps withdrawal and i2pd regression. The minimum verified capacity must be achieved, not asserted. Commands: cargo test --locked -p i2pr-netdb --all-targets; cargo test --locked -p i2pr-daemon --test floodfill_controlled_lifecycle -- --test-threads=1; bash scripts/check-m12-floodfill-boundaries.sh; bash tests/integration/floodfill/run-i2pd.sh; bash tests/integration/floodfill/run-java-floodfill.sh; full AGENTS.md floor, plus new measurement calibration suite. Freeze budgets before runs; distinguish runner errors.

If required class is not legitimately achievable on the test host, **stop with measured evidence** and document a qualified higher-capacity host/test profile; do not fake capability. If this is a genuine Java protocol/policy mismatch, record the exact mismatch and a narrow successor. Closure plans/closure/floodfill/437-status.md must include actual throughput units, ceilings, Java observation and 438 readiness. No normal-daemon public floodfill advertised by this pass.
