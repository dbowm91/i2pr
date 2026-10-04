# Plan 322 — RouterInfo canonical sources and signed news disposition

Status: **blocked-prop170-production-transit-and-ipv6-source-owners**

Implementation commits: prior Plan 322 implementation commits are recorded in repository history and the implementation plan. This disposition adds no implementation commit.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Canonical RouterInfo selector matrix has all 43 Proposal additions, exact output shapes, bounds, sensitivities, and source status. | `crates/i2pr-i2pcontrol/src/source_matrix.rs`; `cargo test --locked -p i2pr-i2pcontrol --test contract plan322_source_matrix_covers_all_canonical_additions_and_marks_gaps -- --test-threads=1` | Passed for inventory integrity; eight rows still lack production owners. |
| Implemented RouterInfo, AddressBook, transport, peer, tunnel, queue, logs, and signed-news sources are truthful and bounded. | `plans/implementation/i2pcontrol-proposal-170/322-routerinfo-canonical-source-and-news-completion.md`; daemon inspection/news owners and focused test evidence listed there; full workspace suite on this branch | Passed for the implemented subset. Signed NEWS verifies bounded SU3 before parsing and retains the last verified feed on transient failure. |
| Transit bytes, 15-second transit bandwidth, and tunnel share ratio come from production transit owners. | Source matrix gap assignments; `crates/i2pr-daemon/src/transit_compose.rs`; production profile composition | Blocked. `TransitOwner` is currently an experimental/controlled qualification owner and is not constructed by production profiles. |
| IPv6 status, IPv4/IPv6 error codes, and IPv4/IPv6 testing state come from maintained per-family lifecycle owners. | Source matrix gap assignments; runtime/daemon transport composition | Blocked. No production IPv6 lifecycle snapshot, typed per-family error owner, or peer-test state owner exists. |
| No unavailable field is fabricated as zero, false, or empty. | `proposal_unavailable_sources_fail_closed_with_field_and_plan_over_wire`; canonical source matrix contract test | Passed: unavailable selections fail closed without partial RouterInfo results. |

## Verification and compatibility

The current branch's local workspace run passed 3,775 tests with 35 ignored across 133 suites; formatting, workspace check, Clippy, rustdoc, doctests, dependency/runtime/service-tunnel boundaries, and the I2PControl acceptance evidence guard passed. Focused source-gap evidence is identified in the implementation plan. This is local evidence, not CI truth.

No migration or protocol advertisement change is introduced by this blocked disposition. Existing canonical sources and their bounded snapshots remain in place. No private data or fabricated network values are exposed. No new runtime task, queue, or lock is added.

## Findings and limitations

- Critical: 0; high: 0; medium: 0; low: 0.
- Eight RouterInfo fields remain unavailable: transit bytes, transit bandwidth, tunnel share ratio, IPv6 status, IPv4/IPv6 error codes, and IPv4/IPv6 testing state.
- Implementing these sources requires production transit instrumentation and typed IPv4/IPv6 lifecycle/peer-test owners. Enabling the qualification-only transit owner or synthesizing values would violate the current product and truthfulness boundaries.

## Roadmap disposition

Plan 322 is closed as blocked, not passed. Plan 328 remains blocked on Plans 322, 326, and 327. Reopen only after separately reviewed production source owners exist and can be sampled without activating unqualified transit participation or implying unsupported IPv6 capability. No M12/mainline readiness or full-Proposal claim changes.
