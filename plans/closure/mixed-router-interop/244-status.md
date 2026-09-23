# Plan 244 status — M6 Java Streaming reverse-direction continuous response attribution

Status: `passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary`

Plan 244 correlated the retained Plan-237/238/239/240 observers in one
continuous hosted response epoch per counted attempt and stopped at the
first real Java → i2pr boundary. Direction A (i2pr → Java Streaming)
established on three of three counted same-SHA attempts; every response
epoch bound fully (helper DBID + target hash + epoch id, Direction-A
prerequisite proven, provenance matched) and stopped identically at the
first missing live stage:

```text
P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
```

The historical `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` token
still prints on all three runs and did not govern any terminal: the
authoritative rule (earliest missing stage among the deepest trustworthy
same-epoch observers) produced the exact B terminal from live deltas on
every attempt.

```text
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-successor-after-plan244
plan_204 = blocked-on-m6-java-second-family-closure-pending-successor-after-plan244
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
next_executable_plan = none-pending-stock-response-corrective-plan-of-record
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## Authority and exact pins

Implementation checkpoint (committed BEFORE any counted attempt; one
SHA for the entire three-attempt budget):

```text
154e92d8436dbad0b020a0b19d848d2d227a22af — Plan-244 implementation
authority. Three files: crates/i2pr-daemon/tests/java_tunnel_external.rs
(P244 lane entry + same-epoch binding + Stage A–F ordered classifier
with the canonical P244-A..G terminals + milestone helpers +
production-change gate + read-only live correlation row reusing the
retained P237/238/239/240 same-epoch facts + 19 §16 unit rows),
tests/integration/m6-interop/run-java.sh (read-only external-p244
rows keyed on the driver TSV, no invented terminals), and
scripts/check-m6-mixed-router-acceptance-evidence.sh (§33 historical-
token/epoch/delta/correlation guards + unit-row + terminal-vocabulary
+ production-surface + plan-invariant + closure-record guards). No
production Rust change, no Java source change, no harness behavior
change, no topology / profile / publication / timing change.
```

References unchanged (frozen reference pins per
`specs/SOURCES.md` + `specs/IMPLEMENTATIONS.md`):

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained floors (Plan 244 adds correlation only): Plan-242 §31
source/static/unit floors, Plan-243 host qualification discipline,
frozen 45-second windows, frozen A/B/C topology, Router-A-only
small-router exploratory profile, Router-B publication target,
route-derived lease-gateway fixture.

## Host qualification (Plan 244 §4, reused Plan-243 gate unchanged)

```text
workspace_sha                    154e92d8436dbad0b020a0b19d848d2d227a22af
workspace_sha_expected           154e92d8436dbad0b020a0b19d848d2d227a22af
cargo_version                    cargo 1.95.0 (f2d3ce0bd 2026-03-21)
rustc_version                    rustc 1.95.0 (59807616e 2026-04-14)
python_version                   Python 3.12.3
java_runtime                     openjdk version "25.0.4.1" 2026-08-18
javac_version                    javac 25.0.4.1
java_cache_dir                   target/interop/cache/m6-java/9134f808337b401e8e53c73734c81fab04280c9d
java_cache_source_revision       9134f808337b401e8e53c73734c81fab04280c9d
java_cache_source_repository     https://github.com/i2p/i2p.i2p.git
java_cache_source_version        2.13.0
java_cache_lib_jar_count         34
java_source_root                 target/interop/m6-java-sources/i2p.i2p-9134f808337b401e8e53c73734c81fab04280c9d
java_source_head_sha             9134f808337b401e8e53c73734c81fab04280c9d
java_source_lock_inputs          ok
i2pr_daemon_path                 target/debug/i2pr
i2pr_daemon_present              true
port-preflight                   loopback_tcp_capacity=12 needed=12
host_qualified                   true
qualification_complete           ok
```

Result: `P243-H-HOST-QUALIFIED reason=ok` on the implementation SHA
(the Plan-243 script is reused verbatim per §4; no new host script).

## Counted-attempt summary (Plan 244 §17)

Three counted attempts on
`154e92d8436dbad0b020a0b19d848d2d227a22af`, no tuning, fresh A/B/C
RouterContexts per attempt (distinct per-run destinations prove
freshness: targets `391eae33…`, `54b12d5e…`, `57a3e89f…`), unique
evidence directories. The shell ran the frozen Plan-242 lane verbatim
(`I2PR_M6_JAVA_DRIVER=streaming bash
tests/integration/m6-interop/run-java.sh`) with only
`I2PR_M6_JAVA_EVIDENCE_DIR` varying per attempt.

| # | Evidence directory | Direction A (§5 prerequisite) | Response epoch binding (§6) | Deepest terminal |
|---|---|---|---|---|
| 1 | `target/interop/m6-java-evidence/p244-attempt-1/` | established (`java_accept_returned=true`, `expected_tunneldata=1`, `recovery=1`, `garlic_payload=1`, `adapter_successes=1`, `connection_state=Established`) | bound (`epoch_bound=true`, provenance matched) | `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` |
| 2 | `target/interop/m6-java-evidence/p244-attempt-2/` | established (same proof values as attempt 1) | bound (`epoch_bound=true`, provenance matched) | `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` |
| 3 | `target/interop/m6-java-evidence/p244-attempt-3/` | established (same proof values as attempt 1) | bound (`epoch_bound=true`, provenance matched) | `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` |

Interpretation (Plan 244 §§3/7): the deepest proven live terminal
governs the closure. All three epochs prove stock-Java scheduler
action (`scheduler_delta=1`) with zero response-packet construction
(`ack_constructed_delta=0`) in the frozen window, so attribution
stops at Stage A on every attempt — identically and repeatably
(3/3, stronger than Plan 243's 2/3 Direction-A rate; no
helper-handshake ceiling fired on this budget). Later same-epoch
facts (Router-A admission, lookup-found, authoritative pool) are
recorded but never override the earliest missing stage.

## Per-attempt evidence highlights

### Attempt 1 — Direction A established, response packet not constructed

Response-epoch binding:

```text
helper_dbid_hex=f41675d6b4b58b15cfb109d12b310aa1daad677d3e981f5c029eb49586b0d006
target_hash_hex=391eae33454b715a33694a2c6c2fc0532a3273abd050e6b8d0fffcd4d8fb5705
epoch_id=streaming-response direction_a_established=true
observers_same_epoch=true epoch_bound=true pool_dbid_provenance=matched
```

Stage facts (same-epoch deltas only):

```text
Stage A: isolatable=true scheduler_delta=1 ack_delta=0 sendmessage_delta=1
  failure_delta=0 exception_delta=0
Stage B: distribute_delta=Some(1) dispatch_delta=Some(1)
  dispatch_send_delta=Some(1) local_target_ls_path=true
  found_remote_delta=Some(1) failed_remote_delta=Some(0)
Stage C: streaming_job_correlated=false b_in_totry=None
  pool_authoritative_nonzero_only=true zero_hop_selected=false
  b_query_dispatched=false
Stage D: b_lookup_received=false b_target_answerable=Some(true)
  b_answered=false a_dsm_received=false a_subdb_installed=Some(true)
```

Retained observer chain on the same epoch: `P234-C-STREAMING-
DIRECTION-A-ESTABLISHED` + `P235-JAVA-STREAMING-PASSED` +
`P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` (historical only) +
`P237-B-SCHEDULER-OBSERVED-NO-ACK-CONSTRUCTION` +
`P239-F-DIRECTION-A-ESTABLISHED` +
`P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED` (+ retained P241
exact-via-C diagnostic `direction=both`). The P244 correlation reads
only these same-epoch rows and emits exactly one terminal:

```text
p244-classification  P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
```

### Attempt 2 — identical terminal on a fresh epoch

```text
helper_dbid_hex=bc51d7f300dc5e7339e22d1265107fa187ea6db23b7791d43bc4ad8599d01714
target_hash_hex=54b12d5effcc568b12014ed1b0bc4ee1a9ba4ac595bffbe90a97f59ef80343c5
epoch_bound=true pool_dbid_provenance=matched
Stage A: scheduler_delta=1 ack_delta=0 sendmessage_delta=1
  failure_delta=0 exception_delta=0
p244-classification  P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
```

Stage B/C/D facts match attempt 1 exactly (distribute 1,
dispatch 1/1, found 1/failed 0, job uncorrelated, no dispatch).
Retained chain identical except the P241 diagnostic
(`direction=outbound` pool churn — diagnostic only under the
Plan-242 §7 authority, and unreachable past Stage A in any case).

### Attempt 3 — identical terminal, lookup-epoch pool authoritative

```text
helper_dbid_hex=5278b8899a2f1fde69b7930f5e85ecca9e9398de47c67669b244d583ce881c2b
target_hash_hex=57a3e89f8b55a256191e9cb36e31e4e2ab56dd2cc783413af9c4cee7b8ec78ba
epoch_bound=true pool_dbid_provenance=matched
Stage A: scheduler_delta=1 ack_delta=0 sendmessage_delta=1
  failure_delta=0 exception_delta=0
p244-classification  P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
```

The lookup-epoch pool re-query on this attempt rebuilt an
authoritative non-zero pair (`pool_authoritative_nonzero_only=true`,
P241 `client_pair_built=true`), yet no exact Streaming ISJ correlated
and no query dispatched — consistent with the Stage-A stop: stock
Java never constructed the response whose lookup would follow.

## Source lock (Plan 244 §14, retained Plan-242 input)

```text
md5 a00d1f19a7b2a262fa9ab2c8dae7c75c  (38 rows; byte-identical on the
pre-lane check and across all three counted attempts on
154e92d8436dbad0b020a0b19d848d2d227a22af)
```

No Java source/jar change, no RNG/selector forcing, no
topology/profile/publication/timing change.

## Static checker (Plan 244 §15 / §33)

`scripts/check-m6-mixed-router-acceptance-evidence.sh` §33 enforces
the Plan-244 surface:

- 33a: all 19 Plan-244 §16 unit rows present in the driver test.
- 33b: all 34 canonical P244-A..G terminal literals present.
- 33c: the delimited P244 ranges (live correlation + module) carry
  none of the forbidden surfaces (`P236Terminal`, `p236_terminal`,
  `p236_state`, `record_p236`, `p236_classify`, `p236_parse`,
  `dispatch_outbound_tunnel`, `|| true`) and keep the full
  epoch/delta/correlation vocabulary (`scheduler_delta`,
  `distribute_delta`, `found_remote_delta`, `failed_remote_delta`,
  `streaming_job_correlated`, `b_query_dispatched`,
  `a_subdb_installed`, `observers_same_epoch`, `epoch_bound`).
- 33d: production Rust stays free of `P244`/`p244` surface
  (`i2pr-daemon/src`, `i2pr-client/src`, `i2pr-tunnel/src`,
  `i2pr-runtime/src`).
- 33e: the harness carries the read-only `external-p244-
  classification` / `p244-epoch-binding` / `p244-stage-summary`
  rows and invents no `record "<P244-"` terminal.
- 33f: the implementation plan keeps the §§3/6/12/17 invariants.
- Closure-record presence (this file).

Result: **passes** on
`154e92d8436dbad0b020a0b19d848d2d227a22af`. During development one
checker defect of ours was found and fixed before any counted
attempt: the 33c range scan piped a shell variable into `grep -q`
under `set -euo pipefail`, where an early `grep -q` match closes the
pipe and the resulting SIGPIPE fails the pipeline even on success;
the scan now uses herestrings. No lane, classifier, or plan text
changed for this fix.

## Verification floor (Plan 244 §18)

Passed (implementation head; re-verified at closure where the tree
changed only under `plans/`):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  -- p225_ p226_ p228_ p230_ p237_ p238_ p239_ p240_ p241_ p242_ p243_ p244_ \
  -- --test-threads=1   (179 passed, 1 ignored; +19 vs Plan 243)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh \
  <exact-pinned-source> <sanitized-tsv>   (38 rows,
  md5 a00d1f19a7b2a262fa9ab2c8dae7c75c, byte-identical x4)
bash scripts/interop/check-p243-host-qualified.sh \
  --expected-sha 154e92d8436dbad0b020a0b19d848d2d227a22af \
  (P243-H-HOST-QUALIFIED reason=ok)
javac <all 16 staged Java helpers/probes against exact-pinned jars> (ok)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py' (18 ok)
cargo test --locked --workspace --all-targets -- --test-threads=1 \
  (2812 passed, 18 ignored, 103 suites)
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

The lane-level `workspace-gates` slice passed on all three counted
attempts; the lane's overall nonzero exit is the retained
fail-closed classification (destination-lane/publication rows stay
failed on the streaming-driver path — byte-identical failed-label
sets to Plan 243 attempt 1), not a harness error. The three new
`external-p244-*` rows report `passed` on all three attempts.

## Acceptance (Plan 244 §19)

1. Plan-243 host qualification is green: **YES**
   (`P243-H-HOST-QUALIFIED reason=ok` on the implementation SHA).
2. Direction A proven before reverse attribution: **YES** (3/3
   attempts carry `P234-C-STREAMING-DIRECTION-A-ESTABLISHED` +
   `P235-JAVA-STREAMING-PASSED` with the full §5 proof values; the
   classifier gates every stage on it).
3. Plan-237 helper response observations correlated to the same
   response epoch: **YES** (`scheduler_delta=1`,
   `ack_constructed_delta=0`, `send_message_event_delta=1`,
   failures 0, isolatable, logger-enabled, bound by helper DBID +
   target + epoch id on all three).
4. Plan-238 Router-A admission correlated to that same epoch:
   **YES** (`distribute_delta=Some(1)` with dispatch corroboration
   on all three; recorded but not overriding Stage A).
5. Plan-239 lookup/dispatch facts correlated to that same epoch:
   **YES** (local path + `found_remote_delta=Some(1)` +
   prepare/dispatch deltas on all three; recorded, Stage C
   correlation still required and absent).
6. Plan-240 exact Streaming ISJ / Router-B facts correlated to that
   same epoch: **YES** — as negative facts (`streaming_job_-
   correlated=false`, `b_in_totry=None`, `b_query_dispatched=false`,
   trace unobservable on all three), which is exactly why the
   epoch cannot advance past Stage A/B.
7. Historical P236 output did not prematurely stop deeper
   attribution: **YES** (the classifier never reads it; the unit
   floor proves a fully proven epoch would reach
   `P244-G-REVERSE-DIRECTION-ESTABLISHED`; live stops at the exact
   B terminal, not the gap token).
8. The earliest missing live reverse stage identified without
   inference: **YES** (`P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED`
   3/3 — scheduler acts, `Connection.sendPacket` construction
   absent; the concurrent `sendMessage` lifetime event without
   construction never advances the epoch per the retained P237
   ordering).
9. No publication correction introduced: **YES** (no helper,
   launcher, profile, publication-target, or timing change;
   source-lock byte-identical).
10. No production i2pr change before exact reverse TunnelData:
    **YES** (no `src/` file changed; `p244_production_change_-
    allowed` unit-locked; checker 33d green; no reverse TunnelData
    exists to authorize anything).
11. Three counted same-SHA attempts with no tuning: **YES**
    (single `154e92d…`, fresh contexts, unique evidence dirs, no
    between-attempt change — `git log` shows no commit between
    implementation and closure).
12. Plan 201 and Plan 204 updated together at closure: **YES**
    (see Disposition and unblock audit below).

## Implementation surfaces

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs   (P244 lane entry /
epoch binding / Stage A–F classifier / milestone helpers /
production gate / read-only live correlation / 19 unit rows; one
behavior-preserving .clone() at the retained P239 record call so the
same-epoch facts stay available for correlation)
tests/integration/m6-interop/run-java.sh   (read-only external-p244
rows; no lane behavior change — failed-label sets byte-identical to
Plan 243 attempt 1)
scripts/check-m6-mixed-router-acceptance-evidence.sh   (§33
historical-token / epoch-mixing / absolute-counter / toTry-as-
dispatch / handoff-as-dispatch / premature-i2pr-defect / Java-source /
RNG / topology / production-Rust / fail-open guards + unit-row +
terminal-vocabulary + plan-invariant + closure-record guards)
plans/closure/mixed-router-interop/244-status.md   (this file)
plans/registry.md   (Plan-244 row + Plan-201 / Plan-204 blocker updates)
plans/subsystems/mixed-router-interop-roadmap.md   (§7 Plan-244 row + §11 / §12 disposition)
plans/closure/mixed-router-interop/201-status.md   (Plan-201 amendment: still blocked, successor owned by §20 arm)
plans/closure/service-tunnels/204-status.md   (Plan-204 amendment: still blocked, convergence unchanged)
```

No `src/` file changed. Frozen windows, topology, profile policy,
publication target, and pins are unchanged.

## Limitations and findings

- **Severity medium (boundary, not defect): stock-Java response
  scheduler acts but never constructs a response packet in the
  frozen window.** Three consecutive counted epochs prove
  `scheduler_delta=1` with `ack_constructed_delta=0`,
  `send_failure_delta=0`, `send_exception_delta=0`. The concurrent
  `send_message_event_delta=1` without construction is not response
  emission (retained P237 ordering: construction precedes
  sendMessage). The response path therefore never reaches Router-A
  admission *for the response* — the observed admission/lookup
  deltas in the window cannot be attributed to a Streaming response
  that was never constructed. No i2pr-owned defect is attributed.
- **Severity low (pool churn is diagnostic):** the lookup-epoch
  client-pool state varies across attempts (P241 `direction=both`
  not-built, `direction=outbound` not-built, then `client_pair_-
  built=true`), while the shell-gate pair passed every time. Under
  the Plan-242 §7 authority the shell gate stays the prerequisite
  and pool churn stays diagnostic; in any case Stage A governs
  before pool facts are consulted.
- **Severity low (measurement note):** `a_subdb_installed=Some(true)`
  appears as a post-epoch absolute on all three runs. Per §6 only
  same-epoch deltas and correlated IDs satisfy a stage, so the
  classifier never credits it; the exact Streaming ISJ correlation
  required to interpret it is absent (`target_job_count=0`).
- Live §§C–F arms beyond `TargetLookupNotCorrelated` are
  unit-locked, not live-proven. Stage E gateway/transit/IBGW facts
  and Stage F reverse-direction i2pr facts have no probe in this
  lane; the §20 successor owns them.
- Direction A established 3/3 does not close M6; §§13/20
  continuation and closure do not trigger. No M6 Java-family
  closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-successor-after-plan244
plan_204 = blocked-on-m6-java-second-family-closure-pending-successor-after-plan244
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-pending-stock-response-corrective-plan-of-record
```

Plan 201 consumes Plan 244 as the next hard dependency on the
streaming axis: the reverse-direction boundary is now exact and
repeatable (`P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` 3/3 with fully
bound epochs). The Plan-201 streaming axis still needs the narrow
stock-response corrective plus the bidirectional Streaming
qualification, and the publication / final-closure axis is
unchanged. No successor is pre-registered here; the corrective
requires its own plan-of-record under the same subsystem, owned by
the §20 `helper response not emitted` arm, and must NOT authorize
any i2pr production corrective before exact expected reverse
TunnelData reaches i2pr.

Plan 204 consumes Plan 244 as the next hard dependency on the
Java-second-family closure: M10 product authority through Plans
213–215 is unchanged. Plan 204 stays blocked on M6 Java
second-family closure (now pending the stock-response corrective +
publication / final-closure axis; no double unblock).

Plan 205 stays retained / deferred (the direct i2cp requalification
lane is intentionally out of scope for Plan 244).

No other registered plan listed Plan 244 as a hard dependency, so
nothing else changes state. Unblock audit complete: no plan becomes
dependency-ready at this closure; the only authorized forward motion
is the single §20 successor plan-of-record named above.

## Follow-up boundary

A successor owns exactly one §20 arm from the exact final boundary:

- helper response not emitted → **narrow stock-response
  corrective** (why the stock helper's response scheduler acts but
  never constructs a `Connection.sendPacket` response in the frozen
  window after Direction A establishes; no i2pr corrective is
  authorized before the reverse direction's expected TunnelData
  reaches i2pr) — **this is the observed arm**;
- Router-A admission missing → narrow I2CP admission corrective
  (not observed; admission deltas present);
- exact lookup not correlated → narrow lookup observer corrective
  (not reached; Stage A governs);
- zero-hop selected despite an authoritative non-zero pool →
  selector contradiction corrective (not observed);
- B query dispatches but reply chain fails → exact lookup-return
  corrective (not observed);
- lookup succeeds but OCMOSJ/tunnel dispatch fails → exact Java
  dispatch corrective (not observed);
- expected reverse TunnelData reaches i2pr then fails → exact
  production i2pr corrective for that owned stage (NOT observed;
  production change stays forbidden);
- reverse direction establishes → return to Plan 201
  publication/final Java second-family closure (not observed).

The successor must retain the Plan-242 §7 corrected gate, the
Plan-242 §6 extended pool observation, the Plan-242 source-lock
(38 needles, md5 `a00d1f19a7b2a262fa9ab2c8dae7c75c`), the
Plan-243 host discipline, the frozen topology / profile / timing,
and the fail-closed lanes, and stop at the first proven stage. It
must NOT pre-authorize a topology correction, a publication
corrective, a tunnel-policy change, a selector-bias correction, an
RNG override, or a production i2pr change unless exact expected
TunnelData proves an i2pr-owned defect.
