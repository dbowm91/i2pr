# Plan 225 corrective closure — M6 Java NO_LEASESET lookup-path observability

Status: **`passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution`**.

Plan of record:
[`225-m6-java-no-leaseset-lookup-path-observability-corrective.md`](../../implementation/mixed-router-interop/225-m6-java-no-leaseset-lookup-path-observability-corrective.md).

Authoritative status:
[`225-status.md`](225-status.md).

## Closure result

Plan 225 closed the Plan-224 observability gap with one authoritative terminal:

```text
P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
```

The exact destination lane proved, with effective in-process logger levels and
target-correlated bounded diagnostics, that Router A's helper-client lookup
search started and exhausted without dispatching the target lookup to Router B.
The corresponding sanitized trace retained the other stages as unknown rather
than inferring them from selector membership or status 21.

This plan is an observability corrective, not a Java or Rust protocol fix. It
does not claim M6 Java interoperability, does not change the 45-second
acceptance window, and does not authorize a production correction. Any product
change belongs to a later plan of record after the Java-family closure owner
accepts this attribution.

## Implementation commits and pinned inputs

The implementation was committed before the counted external lane:

```text
001c2723dd6ce9292ba03924959a52385c620ce3  Implement Plan 225 lookup observability corrective
e7a51372741b464cd1a43c21c3a7aa8006d77e3b  Fix Plan 225 diagnostic error response
a753f2f8d445b850ae2f4c64ff0ade19c063342d  Normalize Plan 225 Java Base32 label
9258e26400065521b911879fed9c75810a6741a8  Satisfy Plan 225 clippy floor
```

The Java reference remained pinned to I2P `2.13.0`, commit
`9134f808337b401e8e53c73734c81fab04280c9d`. No dependency, reference source,
or fixture bytes changed.

## Requirement-to-evidence matrix

| Plan-225 requirement | Evidence / result |
|---|---|
| Effective logger activation is proven before the tracked send | `p225-logger-config-a` and `p225-logger-config-b`: `observable=true`, `effective=true`, `default_level=ERROR`, `isj_level=INFO`, `dlm_level=DEBUG`, `ibmd_level=INFO` |
| Diagnostics are bounded and target-correlated | `p224-target-context` carries the exact target hash and helper DBID; nested Java logs are reduced to whitelisted counts/booleans and the exact lookup target |
| Java helper-client identity uses the actual pinned Base32 behavior | `helper_dbid_b32` is the exact 52-character lower Base32 label derived from Java's full `.b32.i2p` hostname |
| The lookup path distinguishes search, dispatch, receipt, answer, DSM receipt, and install | `p224-lookup-trace` separates `query_started`, `query_to_b`, B liveness/answer, A client-tunnel receipt, and client-subDB installation; no selector-membership inference is used |
| Exactly one authoritative terminal is emitted | `p225-classification` emits exactly `P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B` |
| Pre-epoch/early-stop paths are fail-closed | The Rust unit row `p225_record_emits_one_terminal_for_pre_epoch_gap` and shell/static checker cover the single `P225-OBSERVABILITY-GAP-LOOKUP-PATH` fallback |
| No production behavior or Java state mutation is introduced | Changes are confined to the external Java launcher, test-only diagnostic scanner/aggregator, and static evidence guard; the checker rejects setters, reflection, standalone lookup, topology, and timing changes |

## External evidence

The exact required command was run from a clean committed implementation head:

```text
I2PR_M6_JAVA_DRIVER=destination bash tests/integration/m6-interop/run-java.sh
```

The authoritative Plan-225 rows passed and included:

```text
p225-logger-config-a ... observable=true effective=true ...
p225-logger-config-b ... observable=true effective=true ...
p224-lookup-trace observable=true ... query_started=true query_to_b=false ... search_failed=true ... b_dlm_proven_live=true ...
p225-classification P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
external-p225-classification | passed | ... trace_observable=true
workspace-gates | passed
```

The enclosing legacy Plan-199/M6 Java process exited nonzero because its
pre-existing destination/streaming acceptance rows remain failed. That exit is
not a Plan-225 failure: the Plan-225 diagnostic row and workspace gates passed,
and the raw scratch Java logs were deleted rather than promoted to evidence.
The external run was executed on `a753f2f`; `9258e26` is the subsequent
clippy-only spelling correction for the same parser behavior and was covered by
focused and full local verification.

## Verification and guards

The following completed successfully after the final implementation commit,
unless noted:

```text
cargo fmt --all --check                                      PASS
cargo check --locked --workspace --all-targets                PASS
cargo test --locked -p i2pr-daemon --test java_tunnel_external p225_ -- --test-threads=1
                                                               PASS (4 passed, 62 filtered)
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                               PASS (all tests; Java harness 63 passed, 3 ignored)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                               PASS
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps PASS
cargo test --locked --workspace --doc                         PASS
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
                                                               PASS (18 tests)
cargo deny check advisories bans sources                       PASS
```

The dependency-direction, runtime-boundary, service-tunnel-boundary,
fixture/vector, NTCP2, constrained-host, acceptance-evidence, and
`check-m6-mixed-router-acceptance-evidence.sh` guards all passed. The acceptance
checker emitted only its existing unrelated warning labels. `cargo deny`
reported only existing duplicate-dependency warnings.

## Compatibility, security, and operational evidence

- No Cargo dependency or feature changed.
- Java I2P `2.13.0` and i2pd pins were unchanged.
- Diagnostics run only in disposable loopback Java router contexts.
- Java source inspection is read-only; there is no reflection, setter, router
  mutation, standalone lookup, publication retry, or topology/timing change.
- Raw Java logs and possible key/tag material remain scratch-only. Durable
  evidence contains whitelisted bounded facts and sanitized hashes/labels.
- The existing `ACCEPTED -> NO_LEASESET (21)` send and frozen 45-second window
  remain unchanged.

## Findings and limitations

No critical, high, medium, or low security findings were introduced by this
plan. The remaining functional limitation is the known unclosed Java
second-family qualification: the exact lookup path is now attributed, but no
production correction or end-to-end Java reverse payload claim follows from
that attribution.

The full external wrapper remains nonzero on legacy Plan-199 destination and
streaming rows. No remote/rootless lane was required or run for this
diagnostic-only plan; environment-gated external suites remain ignored unless
their explicit environments are supplied.

## Roadmap and unblock disposition

Plan 225 is formally closed as a passed diagnostic/infrastructure milestone.
The unblock audit found no plan eligible to resume:

```text
plan_201 = blocked-pending-m6-java-second-family-closure-after-plan225-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan201-after-plan225-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
next_executable_plan = none
```

Plan 201 cannot be unblocked by diagnostic attribution alone: its Java
second-family closure and any evidence-supported corrective remain outstanding.
Plan 204 is convergence-only and remains blocked on that independent closure;
M10 authority through Plans 213–215 is unchanged. Plan 205 remains off-path
because the failure is below the SAM bridge. No future plan status was flipped
to unblocked.
