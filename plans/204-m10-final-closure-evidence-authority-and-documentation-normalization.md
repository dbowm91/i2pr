# Plan 204 — M10 final closure evidence, authority, and documentation normalization

Status at registration: **registered-blocked-by-plan201-and-plan203**.

Plan 204 is intentionally narrow. It is **not** another implementation plan. It closes the milestone only after the Java M6 branch and the M10 positive application branch are independently green.

Plan 199 remains historical umbrella context; Plans 200–203 are the executable decomposition that must satisfy its outstanding requirements.

## 1. Preconditions

Do not execute Plan 204 until all are true:

```text
plan_200 = passed
plan_201 = passed-m6-java-public-client-publication-corrective-and-second-family-closure
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
```

Required retained evidence:

```text
Plan 193 i2pd M6 family = all mandatory rows passed
Plan 201 Java M6 family = all mandatory rows passed
Plan 181/203 M10 local rows = 29/29 passed
Plan 203 M10 remote rows = 2/2 passed
```

If any prerequisite is blocked/failed/missing, Plan 204 must remain blocked. Do not weaken a checker or document around the failure.

## 2. Goal

On one exact repository head:

1. run the full workspace/static/dependency floor;
2. run exact-head M6 two-family external acceptance;
3. run exact-head M10 service-tunnel external acceptance;
4. verify all mandatory rows are command-derived and zero are blocked/failed/missing;
5. normalize plans, support/conformance, architecture, skills, and top-level status documentation;
6. close Milestone 10 and hand off to Milestone 11 planning.

## 3. No new product scope

Plan 204 may fix only:

- evidence/checker wiring errors that contradict already-passed underlying runs;
- stale status/documentation references;
- workflow naming/provenance issues;
- trivial test harness exact-head plumbing.

If Plan 204 discovers a product/protocol failure, stop and reopen the owning Plan 201/202/203 layer. Do not implement a hidden product corrective inside Plan 204.

## 4. Exact-head validation floor

Run on the final candidate commit:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo test --locked --doc --workspace

bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh

cargo deny check advisories bans sources
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

Then execute the actual external lanes:

```bash
bash tests/integration/m6-interop/run-m6-mixed-router.sh
bash tests/integration/service-tunnels/run-independent.sh
```

## 5. Workflow provenance

The final exact head must also have successful workflow evidence for:

```text
routine CI
.github/workflows/m6-mixed-router-external.yml
.github/workflows/service-tunnels-external.yml
```

Each external artifact must record:

- exact git SHA;
- exact reference pins;
- runner/workflow ID where applicable;
- mandatory row counts;
- zero blocked/failed/missing rows;
- sanitized evidence only.

If an external lane is flaky or requires a retry, require two complete successful runs on the same SHA. Do not merge evidence from different commits.

## 6. M6 final ledger

Required final M6 state:

```text
m6_i2pd_mandatory_rows = all passed
m6_java_mandatory_rows = all passed
m6_blocked = 0
m6_failed = 0
m6_missing = 0
milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = passed-via-plan201
milestone6_interoperable = passed-via-plan193-and-plan201
```

The Java publication diagnostics from Plan 200 remain retained evidence but are not themselves the final M6 claim.

## 7. M10 final ledger

Required final M10 state:

```text
m10_local_rows = 29/29 passed
m10_remote_rows = 2/2 passed
m10_blocked = 0
m10_failed = 0
m10_missing = 0
m10_remote_transport_core = passed-via-plan202
m10_remote_application_interop = passed-via-plan203
milestone10_final_acceptance = closed
```

Remote HTTP/IRC rows must be positive application executions, not blocker probes.

## 8. Authority/status normalization

Update all relevant status records consistently.

At minimum:

```text
plans/198-status.md
plans/199-status.md
plans/200-status.md
plans/201-status.md
plans/202-status.md
plans/203-status.md
plans/204-status.md
plans/195-status.md
plans/181-status.md
plans/README.md
```

Expected authority transition:

```text
plan_198 = passed-m6-java-public-client-complete-second-family-closure
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-java-publication-observability-and-boundary-classification
plan_201 = passed-m6-java-public-client-publication-corrective-and-second-family-closure
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_195 = passed-m10-remote-independent-service-final-closure
plan_181 = passed-m10-independent-application-service-interop-final-closure-evidence
plan_204 = passed-m10-final-closure-evidence-authority-and-documentation-normalization
```

## 9. Product/support documentation normalization

Update only after evidence passes:

```text
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
.opencode/skills/i2pr-architecture/SKILL.md
specs/support.toml
specs/CONFORMANCE.md
specs/protocols/11-service-tunnels.md
docs/architecture/i2pr-service-tunnels.md
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-client.md
docs/architecture/tooling.md
```

Required documentation corrections:

- remove stale statements that M10 delivery is local/co-owned only;
- document the production remote Destination/LeaseSet2/tunnel/Streaming path from Plan 202;
- document that local co-owned delivery remains a bounded optimization/path;
- record exact-pinned i2pd remote HTTP/IRC evidence;
- remove `next_executable_plan = 198/199/200/202` stale authority;
- preserve unsupported/deferred scope (no outproxy, no public-network claim, no floodfill/transit claim unless separately implemented).

## 10. Static consistency checks

Before closure, grep/check all authority surfaces for contradictory live status such as:

```text
next_executable_plan = 198
next_executable_plan = 199
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
m10 remote rows = blocked
```

Historical plan narratives may retain old states when clearly labeled historical. Current authority/status/docs must not.

## 11. Final closure criteria

Plan 204 passes only when:

1. Plans 200–203 are passed.
2. Exact-head workspace tests pass.
3. Exact-head clippy passes with `-D warnings`.
4. Exact-head docs/doc-tests pass.
5. Dependency/advisory checks pass.
6. All static evidence checkers pass.
7. i2pd M6 mandatory rows are all passed.
8. Java M6 mandatory rows are all passed.
9. M6 mandatory blocked count is zero.
10. M6 mandatory failed count is zero.
11. M6 mandatory missing count is zero.
12. M10 retained local rows are 29/29 passed.
13. Remote HTTP row is passed.
14. Remote IRC row is passed.
15. M10 blocked count is zero.
16. M10 failed count is zero.
17. M10 missing count is zero.
18. Routine CI succeeds on the exact closure head.
19. M6 external workflow succeeds on the exact closure head.
20. M10 external workflow succeeds on the exact closure head.
21. Evidence contains exact reference pins and exact SHA.
22. No evidence contains destination private keys/session secrets/raw sensitive payloads.
23. No counted row depends on public I2P network access.
24. No counted Java row patches the exact-pinned Java source.
25. No counted M10 row uses a co-owned local destination while labeled remote.
26. No clearnet/outproxy fallback is possible in counted remote HTTP/IRC rows.
27. Support/conformance docs match implementation.
28. Architecture docs describe remote and local delivery accurately.
29. Plan/status authority records agree.
30. `next_executable_plan` no longer points to M10 corrective work.
31. No product bug was hidden inside Plan 204.
32. The repository is ready to begin Milestone 11 planning without retained M10 acceptance debt.

## 12. Final transition

On closure:

```text
milestone6_interoperable = passed-via-plan193-and-plan201
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = passed-via-plan181-and-plan203
milestone10_remote_service_interop = passed-via-plan203
milestone10_final_acceptance = closed-via-plan204
next_executable_plan = none-at-m10-layer
next_product_layer = milestone11-planning
```
