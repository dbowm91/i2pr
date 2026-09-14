# Plan 204 status — M10 final closure documentation and authority normalization

Status: **`in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run`**.

Plan of record: [`204-m10-final-closure-evidence-authority-and-documentation-normalization.md`](204-m10-final-closure-evidence-authority-and-documentation-normalization.md).

This status records the Plan 204 docs/CI/evidence-authority
normalization pass landing on top of the already-passed Plans 200,
202, and 203. Plan 204 is intentionally narrow (§3) and only
touches status references, stale prose, evidence/checker wiring,
workflow names, and trivial exact-head plumbing. It does **not**
claim `milestone6_interoperable = passed-via-plan193-and-plan201`
or `milestone10_final_acceptance = closed-via-plan204` because
**Plan 201 has not yet been passed** — its Branch G corrective
framework is landed but the seven §11 stop rows are still blocked
pending the Plan 200 exact-head external run that records the
terminal `P200-{A..H}` classification. Per Plan 204 §1:

> If any prerequisite is blocked/failed/missing, Plan 204 must
> remain blocked. Do not weaken a checker or document around the
> failure.

## What landed in this docs/CI pass

### Authority transition

| Plan | Status | Record |
| --- | --- | --- |
| Plan 198 | `superseded-execution-decomposed-and-closed-via-plans200-204` | superseded umbrella; original final-closure interpretation remains fail-closed |
| Plan 199 | `superseded-execution-decomposed-and-closed-via-plans200-204` | retained historical umbrella; decomposed into the convergent Plans 200–204 |
| Plan 200 | `passed-m6-java-public-client-publication-observability-and-verified-bootstrap` | already passed; status records the diagnostic/evidence side |
| Plan 201 | `in-progress-branch-g-framework-landed-blocked-on-exact-head-external-run` | framework landed; exact-head external run consumes the `P200-*` classification |
| Plan 202 | `passed-m10-production-remote-destination-and-streaming-composition` | already passed; transport layer |
| Plan 203 | `passed-m10-positive-remote-http-and-irc-application-interop` | already passed; positive application layer |
| Plan 195 | `evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization` | reactivated from `registered-blocked-by-plan199` |
| Plan 181 | `passed-m10-independent-application-and-service-interop-final-closure-evidence` | local rows were already passed; remote rows are now flipped via Plan 203 |

### Documentation normalization

- `README.md`, `AGENTS.md`, `plans/README.md`, `plans/195-status.md`,
  `plans/198-status.md`, `plans/199-status.md`, `plans/200-status.md`,
  `plans/201-status.md`, `plans/202-status.md`, `plans/203-status.md`,
  `plans/204-status.md` — stale prose pruned, status transitions
  recorded, no synthetic `passed` evidence rows introduced.
- `specs/support.toml`, `specs/CONFORMANCE.md`,
  `specs/protocols/11-service-tunnels.md` — Plan 202 transport layer
  and Plan 203 positive application interop called out; local
  co-owned delivery retained as a bounded optimization path.
- `docs/architecture/i2pr-service-tunnels.md`,
  `docs/architecture/i2pr-daemon.md`,
  `docs/architecture/i2pr-client.md`,
  `docs/architecture/tooling.md` — production remote
  Destination/LeaseSet2/tunnel/Streaming composition (Plan 202)
  documented; local co-owned delivery retained as the explicit
  bounded path; no "local-only" stale statements.
- `.opencode/skills/i2pr-local-dev/SKILL.md`,
  `.opencode/skills/i2pr-architecture/SKILL.md` — Plan 200/201
  Java branch and Plan 202/203 M10 branch reflected; authority
  pointers updated; stale `next_executable_plan = 198/199/200/202`
  removed.

### Evidence / checker wiring

No evidence wiring was changed by Plan 204 — all static
checkers already passed on the closing exact head (see "Required
validation" below). Plan 204 may fix only:

- evidence/checker wiring errors that contradict already-passed
  underlying runs — none were found in this pass;
- stale status/documentation references — pruned;
- workflow naming/provenance issues — none required fixing;
- trivial test harness exact-head plumbing — none required fixing.

No product bug was hidden inside Plan 204. No product
corrective was implemented.

## Required validation on the closing head

```text
cargo fmt --all --check                                                  OK
cargo check --locked --workspace --all-targets                           OK
cargo test --locked --workspace --all-targets -- --test-threads=1        2357 passed, 12 ignored (98 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps      OK
cargo test --locked --workspace --doc                                   0 passed (≥16 suites)
bash scripts/check-dependency-direction.sh                                OK
bash scripts/check-runtime-boundaries.sh                                  OK
bash scripts/check-service-tunnel-boundaries.sh                           OK
bash scripts/check-fixture-manifest.sh                                    OK
bash scripts/check-ntcp2-vectors.sh                                       OK
bash scripts/check-ssu2-vectors.sh                                        OK
bash scripts/check-i2cp-vectors.sh                                        OK
bash scripts/check-ntcp2-interoperability.sh                               OK
bash scripts/check-constrained-host-lane-boundary.sh                       OK
bash scripts/check-sam-acceptance-evidence.sh                             OK
bash scripts/check-ssu2-acceptance-evidence.sh                            OK
bash scripts/check-i2cp-acceptance-evidence.sh                            OK
bash scripts/check-service-tunnel-acceptance-evidence.sh                  OK
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                 OK
bash scripts/check-destination-tunnel-evidence.sh                          OK
bash scripts/check-netdb-tunnel-evidence.sh                               OK
bash scripts/check-exploratory-tunnel-evidence.sh                         OK
bash scripts/check-streaming-tunnel-evidence.sh                           OK
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'  OK (153 tests)
cargo deny check advisories bans sources                                  OK
```

The evidence-consuming `scripts/check-m6-final-closure-evidence.sh`
remains a manual-workflow gate; it requires
`target/interop/m6-mixed-router-evidence/evidence.json` plus the
Java public-client ledger, neither of which can be produced in
this environment because the Plan 200 exact-head external run
(the one that records the `P200-*` classification) requires the
provisioned exact-pinned Java I2P 2.13.0 cache via
`bash scripts/interop/fetch-m6-java.sh --rebuild` followed by
the cross-family aggregator. That gate must be exercised on the
host that provisions the Java topology — the **current exact
head is not yet authoritative for
`milestone6_interoperable = passed-via-plan193-and-plan201`**.

## What Plan 204 does **not** close

Until the Plan 200 exact-head external run records the terminal
`P200-{A..H}` classification, and Plan 201 lands the matching
branch corrective plus the seven §11 stop rows flip to `passed`,
Plan 204 remains blocked on §1 preconditions:

```text
plan_201 = passed-m6-java-public-client-publication-corrective-and-second-family-closure
```

The following lines therefore stay where they were before this
docs/authority pass:

```text
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
m6_java_public_client_publication_corrective = in-progress-branch-g-framework-landed
milestone10_final_acceptance = not-yet-closed (Plan 204 owns the convergence)
next_executable_plan = 201-branch-g-finalize (Plan 200 external run consumes the P200 classification)
```

Plan 204 does **not** allow:

- silently relabeling Plan 201 / Plan 204 as `passed` in any
  authority surface;
- weakening `scripts/check-m6-final-closure-evidence.sh` to make
  the manual workflow pass without evidence;
- introducing a synthetic `milestone6_interoperable =
  passed-via-plan193-and-plan201` line in any docs file;
- introducing a synthetic `milestone10_final_acceptance =
  closed-via-plan204` line in any docs file.

## On Plan 201 pass (and only then) — expected next transition

After Plan 201 records the terminal `P200-*` classification,
lands its Branch G corrective, flips the seven §11 stop rows
`blocked → passed`, the cross-family M6 checker plus the
`check-m6-final-closure-evidence.sh` evidence-consuming gate go
green on the same exact head, **and only then** will a future
authority-normalization pass be allowed to record:

```text
milestone6_interoperable = passed-via-plan193-and-plan201
milestone10_remote_service_interop = passed-via-plan203
milestone10_final_acceptance = closed-via-plan204
next_executable_plan = none-at-m10-layer
next_product_layer = milestone11-planning
```

That transition is not within Plan 204's scope and is not
claimed by this status.

## Handoff

Plan 204 may run again after Plan 201 closes, against the same
exact head, and only then transition the authority/status lines
above. Until Plan 201 closes, this status remains the only
authorized Plan 204 record.
