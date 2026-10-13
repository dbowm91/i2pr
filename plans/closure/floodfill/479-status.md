# Plan 479 status — Java candidate and NetDB product controls remain open

**Status token:** `in-progress-java-candidate-comparator-and-product-lane-not-implemented`  
**Disposition:** local floodfill regressions and evidence guards pass. The Plan 479 Java comparator profiles and operation-correlated store/lookup lane have not been implemented or run. No source-pinned Java selection claim is made here.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Retain truthful class-capacity enforcement | Plan 454 and Plan 444 authoritative closures | Passed by predecessors; no new class override introduced |
| Distinguish Java local floodfill-role eligibility from remote candidate ranking | Plan 444 source audit: local role requires N or higher; lower-tier remote candidate may remain last-resort | Confirmed by predecessor evidence; no new comparator run |
| Healthy-stock baseline, low-tier preference and eligible i2pr candidate profiles | Existing Plan 470 diagnostic provides read-only candidate/transport preflight; Plan 279 is a different frozen six-router floodfill lane | Not implemented as Plan 479 matrix |
| Actual Java-owned selection, store/lookup product effects and correlated response | No Plan 479 selector trigger or operation-owned attribution was run | Not established |
| Public floodfill capability | No support inventory or normal RouterInfo changes | Unchanged; no claim |

## Verification

| Command | Outcome |
| --- | --- |
| `cargo test --locked -p i2pr-netdb --all-targets` | Passed (282 tests) |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon tests |
| `cargo test --locked -p i2pr-daemon --lib floodfill -- --test-threads=1` | Passed (30 tests) |
| `bash scripts/check-m12-floodfill-boundaries.sh` | Passed |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh` | Passed integrity check (10 i2pd + 12 Java command-derived rows) |
| Plan 475 Java two-role NTCP2 lane | Retained predecessor evidence; not repeated |

No Java floodfill process was started for Plan 479, because the existing frozen Plan 279 lane does not implement the required candidate-preference controls and repeating it would not establish Plan 479's acceptance criteria.

## Invariants and next work

Keep `f`/`fR` default-off in normal startup. Do not invent a bandwidth class, patch stock Java, call private stock selection methods from an external helper, or interpret a bid/preflight result as an actual selected send. The next step is a new owner in the existing Plan 470 same-package diagnostic fixture that observes normal Java-owned candidate selection without triggering selector or throttler methods directly, then drives stock-owned NetDB requests against the three truthful comparator profiles.

**Roadmap disposition:** active; Plans 437/438 remain gated.
