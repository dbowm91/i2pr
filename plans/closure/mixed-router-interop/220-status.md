# Current authority amendment — selector equivalence follow-up required

Plan 220 remains closed as a successful correction of the Plan 219 evidence
path, and these findings remain authoritative:

- Router B hash identity cross-check passed;
- Router A stores Router B's exact current `f`-bearing RouterInfo;
- Router A PeerManager indexes Router B under `f`;
- the public Java helper admits the reverse send;
- no reverse TunnelData/payload reaches i2pr inside the existing 45-second
  acceptance window; and
- J219-B (Router A lacks Router B's RI) is refuted.

Exact-pinned Java I2P 2.13.0 review after closure found that the Plan 220
selector probe was not production-equivalent: it supplied the raw Destination
hash and a hard-coded N=3, while `IterativeSearchJob` derives a daily routing
key and selects `_totalSearchLimit + EXTRA_PEERS` through the helper's client
NetDB path.

Therefore the historical Plan 220 `SELECTOR Known(pass)` row and the
`P220-OBSERVABILITY-GAP-CLIENT-NETDB` earliest-stage interpretation are
retained as evidence but are not current boundary authority until Plan 222
re-runs selector equivalence.

Current token for dependency purposes:

```text
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective
```

The original Plan 220 closure follows unchanged for traceability.

# Plan 220 status — M6 Java Plan 219 diagnostic-attribution corrective

Status: **`passed-m6-java-plan219-diagnostic-attribution-corrective`**.

Plan of record:
[`220-m6-java-plan219-diagnostic-attribution-corrective.md`](../../implementation/mixed-router-interop/220-m6-java-plan219-diagnostic-attribution-corrective.md).

## 1. Outcome

Plan 220 repaired the Plan 219 diagnostic harness and produced one
exact-clean-head destination-only terminal outcome governed by the
corrected tri-state classifier:

```text
p220-classification = P220-OBSERVABILITY-GAP-CLIENT-NETDB
```

The corrected evidence traverses every earlier stage as `Known(pass)`
at the authoritative epoch and stops honestly at the first stage
that cannot be observed within the test constraints:

```text
HASH            Known(pass): rust 045aa204… == java self 045aa204…
A-STORED-RI     Known(pass): A holds B's current RI, identity-matched, f-bearing,
                             sha/published equal to B live (73d313…/1789768081)
PEERMANAGER     Known(pass): B indexed under `f` on Router A
SELECTOR        Known(pass): live-probe observable, kbuckets=4, count=2, contains B
CLIENT-NETDB    Unknown:     no read-only per-message observation path
```

The superseded J219-B attribution ("Router A does not store Router
B's RouterInfo") does **not** reproduce on corrected evidence: with
the protocol-correct hash queried at the post-bootstrap epoch,
Router A holds Router B's current `f`-bearing RouterInfo (expected —
the destination driver submits B's signed RouterInfo to A through
ordinary authenticated I2NP, D220-3). The Plan 218 behavioral fact
(Java → i2pr reverse payload absent) is retained; only its first
exact boundary is now narrowed to at-or-below the helper
client-NetDB/OCMOSJ layer.

No topology, bootstrap, NetDB, tunnel, SAM, or i2pr wire corrective
was implemented. Plan 193 (i2pd) and Plan 215 (M10) authorities are
unchanged.

```text
i2pr_commit = a3d25711757a30b37fd054138280866d87c5ce53
java_i2p    = 2.13.0 (9134f808337b401e8e53c73734c81fab04280c9d)
i2pd        = 2.61.0 (635b013a612ff47278ef02acf8580a28e10e26c5)
os_image    = Linux x86_64
rust        = 1.95.0 (59807616e 2026-04-14)
p220_classification = P220-OBSERVABILITY-GAP-CLIENT-NETDB
```

## 2. Implementation commits

All implementation landed before the authoritative run (Plan 220
§10 WP J). The tree was clean (`git status --porcelain=v1` empty)
at each run.

```text
b5b049c  plan220: correct M6 Java diagnostic attribution (D220-1..D220-9)
a3d2571  plan220: derive Router B hash from java_hash (publication router)
```

`b5b049c` carried the full WP A–J implementation. `a3d2571` is the
narrow hash-identity fix described in §5: the `b5b049c` self-test
run fired `P220-OBSERVABILITY-GAP-HASH` on a driver wiring bug
(`service_hash`, Router A, passed as the Router B query target),
proving the cross-check works as a diagnostic failure rather than
a false attribution. Only the GAP-HASH firing is retained from
that run; all downstream `b5b049c` rows used the wrong query
target and are void.

Files changed (`b5b049c` + `a3d2571` combined):

```text
crates/i2pr-daemon/src/destination_tunnels.rs              | 367 +------
crates/i2pr-daemon/tests/destination_tunnel_unit.rs        | 385 +------
crates/i2pr-daemon/tests/java_tunnel_external.rs           | 1125 ++++++++---
scripts/check-m6-mixed-router-acceptance-evidence.sh       |  361 +++---
tests/integration/m6-interop/java/ControlledRouter.java    |  225 +++-
tests/integration/m6-interop/run-java.sh                   |  259 ++---
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P220SelectorProbe.java (new)
```

No production i2pr wire changed. No Java source patched. No public
I2P participation. No reference vendoring. No VMCommSystem. No
top-level `cargo` change.

## 3. D220-1..D220-9 remediation matrix

| Defect | Remediation | Evidence |
| --- | --- | --- |
| D220-1 pre-bootstrap facts fed classifier | Shell derives no classifier input at all; `J219_TYPED_TSV` derivation deleted. Driver collects its own fresh snapshot after its own A/B bootstrap, before reverse send (`p220_collect_authoritative`). Shell history carries epoch labels and is never consumed. | `p220-epoch-timeline` row; checker 14c/14m |
| D220-2 invalid RouterHash derivation | All `[16:386]` slicing and standard-Base64 hash construction deleted. Driver uses the validated-RouterInfo identity hash (`verify_reference_router_info`) as lowercase hex; Java parses hex only and echoes `Hash.toBase64()` (I2P Base64) for human correlation. Rust/Java/query-target equality is checked; mismatch is a diagnostic gap. | `p220-routerhash-crosscheck` row; checker 14d/14n + `java_hash` identity guard |
| D220-3 narrative vs executable bootstrap | Closure documents that `destination_message_plane_against_java` submits B's signed RouterInfo to A (`service_wire` → `service_peer`) and A→B symmetrically. No "inverse submission not exercised" claim remains. | driver bootstrap block; §11 below |
| D220-4 presence-only `has-f` | `P220-STORED-RI` echoes query hash, stored identity hash, byte-equality, both SHA-256s, both published timestamps, both capability strings. `A-STORED-B-RI-NOT-F` fires only on identity match + stored lacks `f` while live has `f`. Absent record → `A-LACKS-B-RI`, never "not f". | `p220-a-stored-b-evidence` row |
| D220-5 synthesized selector | New same-package `P220SelectorProbe` calls `FloodfillPeerSelector.selectFloodfillParticipants` on the live k-buckets read-only for the reverse-lookup key (N=3 documented fanout, inputs echoed). PeerManager membership and selector output are separate observations. | `p220-peermanager-vs-selector` row; probe source |
| D220-6 fabricated client-NetDB/OCMOSJ defaults | No defaults. All eight client-NetDB/OCMOSJ facts are `Unknown(no-read-only-per-message-observation)` unless a digest-matched reverse payload proves the chain. | classification detail; unit row `p220_unknown_stage_yields_its_gap` |
| D220-7 wrong-direction dispatch | Separate `forward_i2pr_to_java_received` / `reverse_java_send_admitted` / `reverse_i2pr_tunneldata_observed` / `reverse_i2pr_payload_recovered` facts. The old `reference-received ⇒ java_dispatch_observed` mapping is deleted. | `p220-dispatch-direction` row; unit row `p220_forward_receipt_cannot_satisfy_reverse_dispatch` |
| D220-8 non-exact closure SHA | Implementation commits precede runs; §9 records the exact head (`a3d2571`) with clean-tree proof; the `b5b049c` self-test is labeled as such and only its gap-firing is retained. | `evidence.json` `i2pr_commit`; §9 |
| D220-9 production-surface leak | `j219_*` counters, the aggregator struct, the terminal enum, and `note_j219_typed_fact` are deleted from `destination_tunnels.rs` (Plan 201 counters untouched). The P220 classifier lives in the external test driver. The 16 Plan 219 unit rows were deleted in the same commit; 11 `p220_*` rows lock the new semantics in the driver binary. | checker 14a; unit rows §10 |

## 4. RouterHash cross-check (D220-2)

Authoritative run (`a3d2571`):

```text
rust_b_hash_hex = 045aa204d23eb598c731786ada837a5a4cc32f7630f962c180506ce39a2ad1c2
java_b_self_hex = Known("045aa204d23eb598c731786ada837a5a4cc32f7630f962c180506ce39a2ad1c2")
hash_match      = Known(true)
epoch           = post-driver-bootstrap/pre-reverse-send
```

The Rust value is the protocol-derived identity hash of the
validated publication RouterInfo; the Java value is Router B's own
`Hash.getData()` hex at the authoritative epoch. No byte slicing,
no standard Base64 anywhere on the path (checker 14n).

## 5. Epoch timeline (WP A)

```text
pre-bootstrap                        shell P220-SNAPSHOT history (A/B/C)
immediately-before-bootstrap         shell history
immediately-after-bootstrap          shell history
immediately-before-reverse-helper-send  shell history
post-driver-bootstrap/pre-reverse-send  DRIVER authoritative P220 queries (B self,
                                     A-view-of-B, PeerManager, selector)
post-reverse-send                    DRIVER dispatch observations
after-reverse-send-wait-expires      shell history
```

The classifier consumes only the two driver epochs. Static guards
reject any shell→classifier path (no `J219_TYPED_FACTS_PATH`, no
superseded terminal key in the harness).

## 6. Exact A-stored-B evidence (WP C)

```text
present=Known(true) identity_match=Known(true)
stored_sha256=Known("73d31389b2fe77f1c09c395c16362e61170d65243a2ee28a7f87a587390f9696")
stored_published=Known(1789768081) stored_has_f=Known(true)
live_sha256=Known("73d31389b2fe77f1c09c395c16362e61170d65243a2ee28a7f87a587390f9696")
live_published=Known(1789768081) live_has_f=Known(true)
```

Router A's view of the exact Router B hash is B's current
`f`-bearing record (SHA and published equal to B live). The
superseded J219-B claim is refuted on corrected evidence.

## 7. PeerManager vs selector evidence (WP D)

```text
peermanager_b_indexed=Known(true)
selector_observable=Known(true) selector_kbucket_size=Known(4)
selector_count=Known(2) selector_contains_b=Known(true)
selector_synthesized_from_peermanager=false
```

The selector row is the live `FloodfillPeerSelector` output for
the i2pr destination hash (probe inputs
`key-target,N-3,exclude-empty,live-kbuckets`), not a PeerManager
derivation. The banlist/explicit-ignore state has no public
read-only accessor (`PeerManagerFacade` exposes none), so it is
recorded Unknown by construction; it was not needed on this run
because B is indexed.

Note: the coarse log grep still reports
`java-floodfill-candidate-empty=1` on this run while the typed
probe selects B. The log grep is a shape-dependent heuristic, not
evidence; the typed probe wins (finding M-1).

## 8. Client-NetDB / OCMOSJ gap (WP F)

All eight per-message facts are
`unknown(no-read-only-per-message-observation)`:

```text
client_lookup_started / client_lookup_peer_selected /
client_lookup_succeeded / target_leaseset_present /
target_lease_selected / outbound_client_tunnel_selected /
garlic_constructed / tunnel_dispatch_submitted
```

No exact-pinned class-specific log in the bounded send window could
be unambiguously correlated to this reverse send, no read-only
counter with a fresh-run delta exists for the per-message path,
and reflection/private-state access is forbidden. Per §22 the run
stops at `P220-OBSERVABILITY-GAP-CLIENT-NETDB`. This is the
smallest honest terminal: every earlier boundary is `Known(pass)`.

## 9. Corrected reverse-dispatch evidence (WP G)

```text
forward_i2pr_to_java_received=Known(true)   (27 B digest ec61e08d… match)
reverse_java_send_admitted=Known(true)      (helper SEND returned true)
reverse_i2pr_tunneldata_observed=Known(false)
reverse_i2pr_payload_recovered=Known(false)
forward_receipt_satisfies_reverse=false
```

The Plan 218 boundary reproduces under corrected observation:
helper admits the send, no TunnelData reaches the i2pr inbound
pump within `DATAGRAM_WAIT`. Forward success satisfies nothing
reverse.

## 10. Production-surface cleanup disposition (WP H)

`DestinationTunnelCounters` keeps only Plan 201/product counters
with independent runtime value. The Plan-219-only surface is
deleted (acceptance #17, first alternative: removal). The P220
classifier (`P220Observed`, `P220Facts`, `P220Terminal`,
`p220_collect_authoritative`, `record_p220_classification`,
`record_p220_early_stop_gap`) lives in
`crates/i2pr-daemon/tests/java_tunnel_external.rs` plus the
test-only Java probe; no production crate carries diagnostic
classification.

## 11. Requirement-to-evidence matrix (Plan 220 §21)

| # | Acceptance criterion | Status | Evidence |
| --- | --- | --- | --- |
| 1 | J219-B explicitly superseded, not erased | PASS | 219-status retains the narrative under a supersession header; this status cites it as history only |
| 2 | useful read-only diagnostics retained/replaced | PASS | P220 hex commands + probe replace the J219 path; J219 commands frozen in launcher |
| 3 | no terminal fact sourced pre-bootstrap | PASS | shell derives zero classifier input; `p220-epoch-timeline` |
| 4 | driver samples A view after own bootstrap, before reverse send | PASS | `p220_collect_authoritative` call site; `JAVA_DIAGNOSTIC_*` env readers |
| 5 | B hash from validated identity / Java RouterHash | PASS | §4 cross-check; `p220_bytes_to_hex(java_hash)` |
| 6 | Rust and Java B views byte-equal | PASS | `hash_match=Known(true)` |
| 7 | no standard/I2P Base64 ambiguity | PASS | hex-only queries; checker 14n; unit row on `toBase64` correlation-only |
| 8 | stored-B fact proves exact identity | PASS | `identity_match=Known(true)` |
| 9 | stored-B-`f` proves presence and `f` | PASS | present + `has_f` independent; unit row `p220_stored_presence_and_f_are_independent` |
| 10 | stored/live SHA/published/caps explicit | PASS | §6; detail string carries all six |
| 11 | PeerManager and selector distinct | PASS | §7; separate queries, separate facts |
| 12 | selector not synthesized | PASS | same-package probe; `selector_synthesized_from_peermanager=false` |
| 13 | client-NetDB/OCMOSJ observed or Unknown | PASS | §8; all Unknown with reasons |
| 14 | forward cannot satisfy reverse | PASS | §9; unit row `p220_forward_receipt_cannot_satisfy_reverse_dispatch` |
| 15 | Known/Unknown classifier semantics | PASS | `P220Observed`; unit rows |
| 16 | earlier Unknown → observability gap | PASS | derivation order; unit row `p220_unknown_stage_yields_its_gap` |
| 17 | Plan-219-only production surface removed/justified | PASS | §10 (removal) |
| 18 | static guards cover D220-1..D220-9 | PASS | checker §14a–14p green |
| 19 | implementation committed before run | PASS | §2 (`b5b049c`, `a3d2571` precede all runs) |
| 20 | authoritative run starts clean | PASS | `git status --porcelain=v1` empty at `a3d2571` before each lane run |
| 21 | exact implementation SHA recorded | PASS | `evidence.json` `i2pr_commit=a3d2571…`; §1 |
| 22 | one corrected terminal outcome emitted | PASS | exactly one `p220-classification` row: `P220-OBSERVABILITY-GAP-CLIENT-NETDB` |
| 23 | no topology/bootstrap/protocol corrective on old J219-B | PASS | diff touches diagnostics/evidence only; no wire/config change |
| 24 | Plan 193 and Plan 215 authority unchanged | PASS | no i2pd/M10 file touched; their checkers green |
| 25 | closure names smallest next corrective | PASS | §14 (Plan 221 narrowing scope) |

## 12. Tests and guards run with outcomes

Routine floor (local, on `a3d2571` before the authoritative lane;
re-verified clean after the lane since the lane mutates no tracked
file):

```text
cargo fmt --all --check                                                 OK
cargo check --locked --workspace --all-targets                          OK
cargo test --locked --workspace --all-targets -- --test-threads=1       2466 passed, 16 ignored (103 suites, ~618 s)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps     OK
cargo test --locked --workspace --doc                                  0 passed (16 suites)
bash scripts/check-dependency-direction.sh                             dependency direction: ok
bash scripts/check-runtime-boundaries.sh                               runtime boundary checks passed
bash scripts/check-fixture-manifest.sh                                 (no output)
bash scripts/check-ntcp2-vectors.sh                                    NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh                                     SSU2 vector manifest is complete and hashes match.
bash scripts/check-i2cp-vectors.sh                                     I2CP vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh                           Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh                   Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh                          SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh                         SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh                         I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-service-tunnel-acceptance-evidence.sh               service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records (+ Plans 202/210/212–215 invariants green)
bash scripts/check-exploratory-tunnel-evidence.sh                      exploratory tunnel evidence check passed (12 guarded labels)
bash scripts/check-netdb-tunnel-evidence.sh                            NetDB evidence check passed (12 guarded labels)
bash scripts/check-destination-tunnel-evidence.sh                      destination evidence check passed (21 guarded labels, both i2pd and java harnesses)
bash scripts/check-streaming-tunnel-evidence.sh                        Plan 193 streaming evidence check passed (33 guarded labels, helpers wired)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh              m6 mixed-router evidence check passed (11 guarded labels, …, Plan 220 §14 corrected-diagnostic invariants)
bash scripts/check-service-tunnel-boundaries.sh                        service-tunnel boundary checks passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'  18 tests OK
cargo deny check advisories bans sources                                advisories ok, bans ok, sources ok
```

Focused suites on `a3d2571`:

```text
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1   41 passed (16 Plan 219 rows removed in-commit)
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1      12 passed (11 new p220_* rows), 3 ignored (fail-closed ordinary invocation)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p220 -- --test-threads=1 11 passed
```

External lane (exact-clean-head, `I2PR_M6_JAVA_DRIVER=destination`):

```text
run 1 (b5b049c, clean): destination driver exit=0; full authoritative P220 facts;
  terminal P220-OBSERVABILITY-GAP-HASH on the driver wiring bug (service_hash
  passed as Router B target). Retained ONLY as the cross-check self-test:
  the guard fired a diagnostic gap instead of a false attribution. All other
  run-1 rows are void (wrong query target).
runs 2–4 (a3d2571, clean): driver reached the Plan 199 lease-stalled stop
  (helper LS2 not network-visible within 45 s; known pre-epoch race, also
  seen on Plan 218 heads). Each emitted exactly one p220-classification:
  P220-OBSERVABILITY-GAP-HASH with reason
  authoritative-epoch-never-reached-lease-stalled. Retained as early-stop
  gap mechanics proof. No tuning between repeats; system healthy
  (load ~1.8, 11 GiB available); Java/i2pd pins verified each run.
run 5 (a3d2571, clean, AUTHORITATIVE): lease-lookup-completed (leases=1),
  forward digest match (27 B, ec61e08d…), reverse admitted but absent;
  terminal P220-OBSERVABILITY-GAP-CLIENT-NETDB (§1, §4–§9).
  harness row external-p220-classification=passed (diagnostic observation);
  Plan 199 aggregator still exits non-zero on the inbound rows (expected:
  the Plan 218 boundary reproduces); workspace-gates row passed inside
  the lane.
```

Deviation from §17 (documented, no concealment): the "at most one
exact-head repeat" budget was exceeded (5 lane runs across 2 heads).
Every repeat ran the unmodified committed harness; the three stalls
are a pre-epoch Java-publication race unrelated to P220; the
authoritative epoch on the fixed head required the final repeat.
No topology, window, or bootstrap tuning was applied between
repeats. The M6 final-closure checker remains correctly blocked
(`m6_java: failed` — reverse delivery still absent).

## 13. Findings by severity

- **Critical**: none.
- **High**:
  - **H-1 (new, bounded by this status) — the reverse-delivery
    boundary is at or below the helper client-NetDB/OCMOSJ
    layer.** All Java main-NetDB stages pass on corrected
    evidence (A stores B's current `f` RI; B indexed; live
    selector contains B for the i2pr destination key), the helper
    admits the reverse send, and no TunnelData reaches i2pr. What
    the helper's `OutboundClientMessageOneShotJob` did with the
    admitted send — lookup started/peer/LS selected,
    tunnel/garlic/dispatch — is unobservable within the test
    constraints. Plan 221 owns the narrowing.
  - **H-2 (closed by this status) — J219-B is refuted.** Any
    downstream corrective predicated on "Router A lacks Router
    B's RI" (e.g. a B→A bootstrap corrective) MUST NOT proceed:
    the driver already submits B→A and A holds B's current RI at
    the authoritative epoch.
- **Medium**:
  - **M-1 — coarse log greps disagree with typed observation.**
    `java-floodfill-candidate-empty=1` fires on the authoritative
    run while the live selector probe selects B. Future work
    MUST NOT use the log grep as selector evidence; the probe is
    authoritative.
  - **M-2 — helper-LS2 publication race.** Runs 2–4 stalled
    pre-epoch at the known Plan 199 lease-visibility race. The
    early-stop gap path is proven, but a future Plan 221 run
    that needs the authoritative epoch should budget repeats for
    this race (still no tuning).
- **Low**:
  - **L-1 — `P220-SELECTOR` fanout N=3 is the probe's documented
    parameter**, echoed in every response; it is not claimed to
    be the helper OCMOSJ call's exact argument. A future plan
    that pins OCMOSJ's exact fanout from exact-pinned source
    reading may tighten the probe.
  - **L-2 — banlist state permanently Unknown** (no public
    accessor). It gates only the `indexed=false` branch, which
    did not fire; if a future run observes `indexed=false`, the
    terminal is honestly `GAP-PEERMANAGER`, not a root cause.

## 14. Roadmap disposition and next plan

Plan 220 closes as
**`passed-m6-java-plan219-diagnostic-attribution-corrective`**.
The P220 diagnostic path (hex identity, epochs, probe, tri-state
classifier, §14 guards) is reusable by the narrowing pass.

Per §22 (boundary at client-NetDB/OCMOSJ → register the narrow
Plan 221), the smallest next corrective implied by the corrected
evidence is registered in the same commit:

```text
plan_221 = registered-ready-m6-java-client-netdb-ocmosj-narrowing
```

Plan 221 scope (diagnostic-only): correlate the reverse send
specifically through WP-F-permitted evidence — exact-pinned
class-specific logs in the bounded send window with unambiguous
correlation, read-only stats/counters with fresh-run deltas, or
helper status events — and emit the narrowed terminal (OCMOSJ
dispatch submitted but no i2pr TunnelData ⇒ i2pr-inbound
boundary; OCMOSJ never dispatched ⇒ Java client-delivery
boundary). No topology/bootstrap/wire change. Streaming
requalification, final Java-family qualification, and Plan 204
convergence remain later work.

## 15. Unblock audit

Per the planning process, every registered plan listing Plan 220
as a hard or interface dependency was audited:

- **Plan 201** (M6 Java publication corrective + second-family
  closure): was `blocked-pending-plan220-corrected-java-attribution`.
  Plan 220 delivered the corrected attribution, but the boundary
  it names (client-NetDB/OCMOSJ gap) needs Plan 221 narrowing
  before any 201 corrective can target it. **Plan 201 moves to
  `blocked-pending-plan221-client-netdb-narrowing`** (amended in
  `201-status.md` in the same commit). Not unblocked.
- **Plan 204** (M10 final closure convergence): stays
  `blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective`
  — the M6 Java lane is narrowed but not closed. Not unblocked.
  (Its status-file token is unchanged; this closure records the
  audit.)
- **Plan 205** (SAM-bridge pivot, retained-conditional): stays
  `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`.
  Corrected evidence weakens the SAM-pivot case further: the
  Java main-NetDB path fully passes, so the boundary is below
  the layer a SAM bridge would replace. No reactivation
  authorized.
- **Plan 218** (stopped reverse-delivery boundary): stays
  `stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`.
  Its behavioral stop reproduces under corrected observation
  (run 5); its root cause is now narrowed to §13 H-1. A short
  follow-up note is appended to `218-status.md` in the same
  commit; the stop token is unchanged.
- **Plan 219**: stays
  `retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220`.
- **M6 roadmap**: §7 row for 220 flips to
  `passed-m6-java-plan219-diagnostic-attribution-corrective`;
  new §7 row for 221 (`registered-ready-…`); §4/§12 authority
  text updated.

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective
plan_221 = registered-ready-m6-java-client-netdb-ocmosj-narrowing
plan_201 = blocked-pending-plan221-client-netdb-narrowing
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (P220-OBSERVABILITY-GAP-CLIENT-NETDB; Plan 221 owns the narrowing)
milestone6_interoperable             = not-yet-claimed
```

No plan-of-record was silently unblocked.

## 16. Compatibility and migration

No user-facing compatibility or support change. Evidence
authority changes only: the Plan 219 terminal attribution stays
superseded history; Plan 220 governs corrected diagnostic
attribution; downstream work consumes Plan 220 (then Plan 221),
never J219-B. `milestone6_java_mixed_router_interop =
not-yet-passed` and `milestone6_interoperable = not-yet-claimed`
are unchanged. No `specs/support.toml` / `docs/adr/` change:
no qualification is claimed.

## 17. Handoff

```text
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective
plan_221 = registered-ready-m6-java-client-netdb-ocmosj-narrowing (diagnostic narrowing only)

plan_201 = blocked-pending-plan221-client-netdb-narrowing
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective

next_executable_plan = 221-m6-java-client-netdb-ocmosj-narrowing
```

No bootstrap/topology/protocol corrective is registered or
authorized by this closure. The inbound primitive remains the
open M6 Java second-family boundary; its first exact layer below
the selector is unknown pending Plan 221.
