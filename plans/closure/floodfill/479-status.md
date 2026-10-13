# Plan 479 status — Java candidate and NetDB product controls remain open

**Status token:** `in-progress-java-candidate-comparator-and-product-lane-partial`
**Disposition:** local floodfill regressions and evidence guards pass. One exact-pinned Java run establishes a real selected-candidate Store path; the Plan 479 Java comparator profiles and full operation-to-response attribution remain open. No comparative selection claim is made here.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Retain truthful class-capacity enforcement | Plan 454 and Plan 444 authoritative closures | Passed by predecessors; no new class override introduced |
| Distinguish Java local floodfill-role eligibility from remote candidate ranking | Plan 444 source audit: local role requires N or higher; lower-tier remote candidate may remain last-resort | Confirmed by predecessor evidence; no new comparator run |
| Healthy-stock baseline, low-tier preference and eligible i2pr candidate profiles | One exact-pinned Plan 470 controlled run below observed the eligible i2pr candidate in Java's live selector and normal store path. The healthy-stock and low-tier comparison profiles remain absent. | Partial; Plan 479 matrix not implemented |
| Actual Java-owned selection, store/lookup product effects and correlated response | The same run observed Java select the i2pr floodfill candidate for its local RouterInfo, submit and complete its normal floodfill Store job, and the daemon accept the authenticated RouterInfo Store with a response owed. | One real product operation established; comparative recipient matrix and full response correlation remain open |
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
| `rtk python3 tests/integration/ntcp2/plan470_java_ntcp2.py` | Exact Java 2.13.0 pin passed the candidate-loaded/admitted/selected, stock Store submission/success, authenticated daemon Store effect, and daemon lookup-correlation stages. Java's normal selector included the truthful i2pr candidate for its local RouterInfo. The lane exited 1 because its separate `daemon_outbound_link_promoted` stage was zero; the sanitized receipt also recorded one Java NTCP connect timeout. This is a useful eligible-candidate/product profile, not Plan 479 closure or a complete Plan 470 pass. |

No separate Java floodfill process or low-tier comparison profile was started for Plan 479. The exact-pinned Plan 470 run above reused the existing Java owner and produced real selected-candidate Store evidence, so it narrows the remaining work without duplicating the frozen Plan 279 lane.

## Invariants and next work

Keep `f`/`fR` default-off in normal startup. Do not invent a bandwidth class, patch stock Java, or interpret a bid/preflight result as an actual selected send. The next step is to extend the existing Plan 470 controlled fixture with multiple independently healthy floodfills, record actual stock selection among eligible and last-resort candidates, and correlate each stock-owned NetDB operation with its selected peer and response. Repair the distinct outbound-link-promotion attribution in the Plan 470 lane before treating its outbound lookup result as fully sourced evidence.

**Roadmap disposition:** active; Plans 437/438 remain gated.
