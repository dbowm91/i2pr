# Plan 240 status — M6 Java Streaming target-LeaseSet lookup-failure attribution

Status: `passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary`

Plan 240 attributed the exact reason the Plan-239 Streaming response
cannot resolve the i2pr target LeaseSet from Router A's helper client
NetDB. On three counted same-SHA executions, the exact streaming target
ISJ was correlated (helper DBID + target hash + job IDs), Router B
proved lookup-candidate eligible (RI present, floodfill capability in
RI, peer-manager `f`-indexed, not forever-banlisted, fresh RI, profile
present, comm established), B appeared in the initial `toTry` of both
streaming jobs, and the first B-specific boundary fired inside pinned
`sendQuery()`: the selected outbound tunnel was zero-hop while Router
A held no validated RouterInfo for B, so the target DLM was never
dispatched to B and the search exhausted. The terminal on all three
runs:

```text
P240-C-B-ZERO-HOP-UNKNOWN-RI
```

No Java source was patched, no production Rust changed, no topology,
profile, publication, timing, or pin changed, and no M6 Java-family
closure is claimed. The retained Plan-232 raw-Destination authority,
Plan-234/235/236/237/238 baselines, and Plan-239 D1 lookup-failure
proof are reproduced intact.

## Authority and exact pins

Implementation checkpoint:

```text
37025f9 (Plan-240 lookup-failure attribution surfaces; three counted
         full-lane attempts 1/2/3 with identical terminal and no
         between-attempt tuning)
```

References unchanged:

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained: Plan 232 route-derived lease-gateway correction and
bidirectional raw-Destination pass (code untouched), Plan 234 Java
accept-returned boundary, Plan 235 usable `I2PSocket` surface +
outbound-admission prerequisite, Plan 236 source-lock ordering, Plan
237 stock deltas (2/1/3/0/0) and logger proof, Plan 238 Router-A
admission deltas (distribute 3, dispatch 0/0), Plan 239 D1
lookup-failure deltas (local absent, found 0, failed 3/4, dispatch
0/0), frozen topology, profile policy, publication target, and timing
windows (`DATAGRAM_WAIT`/`STREAM_WAIT`/`SYN_ACK_WAIT` all still 45 s).

## Source lock (WP1 + §14)

Exact-pinned source review established the streaming-epoch lookup
sequence in pinned order: OCMOSJ constructor local lookup, then
`IterativeSearchJob.runJob()` negative-cache check, floodfill selection
through `selectFloodfillParticipants(_rkey, _totalSearchLimit +
EXTRA_PEERS, ks)`, self/target removal, job registration, the
source-locked `New ISJ ... toTry:` row, `retry()` routing-key-order
pick with `IP_CLOSE_BYTES = 3` diversity, and the `sendQuery()`
pre-dispatch guards (old-router via `StoreJob.shouldStoreTo(ri)`,
outbound/client tunnels, reply-encryption compatibility, zero-hop
self/unknown, encrypted-prep) before the authoritative
`Encrypted DLM for <target> to <peer>` dispatch line. The
`FloodfillPeerSelector` candidate source
(`getPeersByCapability(CAPABILITY_FLOODFILL)` minus
`isBanlistedForever`) and the DEBUG ranking family (Same /16, Old,
Bad country, Slow, Bad (new), Good, OK, Bad (DB), Bad (no hist), Bad
(no prof)) are locked as source facts for §7 interpretation.

The source lock
(`scripts/interop/check-m6-java-response-source-lock.sh`) is extended
with the twenty-five P240 needles above (ISJ/FloodfillPeerSelector
files newly consumed; StoreJob read for provenance); the TSV gains
seven additive rows (32 rows total, 25 retained shapes frozen). The
TSV is byte-identical across the three counted attempts:

```text
md5 def49e9ea6c180a069665dae237aecff  (attempts 1/2/3, 32 rows)
```

## Observation mechanism (WP2–WP3 + §5/§6)

One tiny public Router-A command (`P240-ROUTER-B <router-b-hex>` in
the out-of-tree `ControlledRouter.java`, observed read-only through
the new out-of-tree `P240Probe.java` compiled against the
exact-pinned jars):

- Router-B readiness via retained public-API patterns (RI presence,
  floodfill capability in RI, peer-manager `f`-index membership, the
  exact pinned selector source, forever-banlist state, RI age bucket,
  bandwidth tier, read-only profile presence plus 30-minute
  send-failure recency, comm-system established). No profile creation
  (`getProfileNonblocking` only, never `getOrCreate*`/`heardAbout`),
  no rate creation, no NetDB/tunnel/banlist/queue/stat writes;
- exact streaming target/job correlation reusing the retained
  P224/P225/P226 surfaces inside the streaming epoch: JVM Base64
  renderings, helper Base32 label, effective logger configs
  (file-installed plus in-process levels), whitelist-only scratch-log
  scan with three additive bounded counters (`toTry` B-membership on
  the exact-target `New ISJ` line, negative-cache, zero-hop-self),
  and the ordered P224 lookup trace plus P226 exact-job trace for the
  streaming target;
- `-1`/Unknown discipline retained: unreachable diagnostics yield
  `None` (Unknown, never zero-as-fact); the classifier maps unknown
  readiness or unknown `toTry` to the pre-query gap, never to a
  B-specific selection claim.

The Rust driver (inline in `streaming_through_java`, the P239
precedent) snapshots Router-B readiness and logger configs before the
Direction-A SYN, then at the end of the frozen 45 s response window
collects post readiness, B main-LS and A client-subDB snapshots for
the streaming target, scans both routers' scratch logs, and maps the
exact-job facts onto the ordered A/B/C/D classifier (never
absolutes, never generic search-wide counts without B correlation).

Router A is long-lived across the destination + streaming sub-runs
(Plan 217 §6.D). Epoch isolation holds at the attribution layer
because the streaming target hash is fresh per run and differs from
any destination-lane target: stale destination-lane jobs can never
enter the exact streaming job set, and the Plan-239 4-vs-3
failed-remote mismatch is excluded by construction (the classifier
consumes only job-correlated facts, never global
`leaseSetFailedRemoteTime` deltas).

A deliberate divergence from P226 single-job observability is
recorded here: each counted attempt created exactly two streaming
target jobs (three helper sends collapsing onto two ISJ epochs), so
the retained `p226-target-job-trace observable=false` (single-job
rule) while P240 correlation holds (`target_job_count=2`,
`target_job_id_overflow=false`, both jobs with B in `toTry`). The
P240 classifier uses any-job semantics over the exact streaming set
for guard/query facts; generic search-wide failures without B
correlation still cannot satisfy a B-specific terminal (unit-locked).

## Stat/logger enablement proof

All three counted attempts prove lookup loggers effective pre and
post on both routers, Router-B readiness observable pre and post
(identical facts: RI present, floodfill-indexed, not banlisted, fresh
RI, tier L, profile present, no recent send failure, comm
established), target context fully rendered (target/router-B/helper
Base64 plus helper Base32), B main-LS post answerable (validated,
current, received-as-published, 1 lease, key type 4), and A client
sub-DB post empty (validated false):

```text
p240-logger-config-a-pre/post effective=true (ERROR/INFO/DEBUG/INFO)
p240-logger-config-b-pre/post effective=true (ERROR/INFO/DEBUG/INFO)
p240-router-b-pre == p240-router-b-post on every run (stable epoch)
p240-b-main-after validated=true current=true received_as_published=true
p240-a-client-after validated_present=false (client facade proven)
p240-lookup-trace observable=true query_started=true query_to_b=false
  search_failed=true b_lookup_received=false b_dlm_proven_live=true
```

`b_dlm_proven_live=true` makes B non-receipt authoritative, but the
classifier stops earlier at the proven zero-hop-unknown guard, so no
D terminal is claimed.

## Counted evidence (SHA 37025f9, no tuning between attempts)

| Attempt | Evidence dir | P239 D1 (found/failed) | Streaming jobs (B in toTry) | Terminal |
| --- | --- | --- | --- | --- |
| 1 | `target/interop/m6-java-evidence-plan240-attempt1` | 0/4 | 2/2 | `P240-C-B-ZERO-HOP-UNKNOWN-RI` |
| 2 | `target/interop/m6-java-evidence-plan240-attempt2` | 0/3 | 2/2 | `P240-C-B-ZERO-HOP-UNKNOWN-RI` |
| 3 | `target/interop/m6-java-evidence-plan240-attempt3` | 0/3 | 2/2 | `P240-C-B-ZERO-HOP-UNKNOWN-RI` |

Full per-attempt P240 classification rows (all three):

```text
attempt 1 target 8381b66d...: correlated=true negative=false ri=true indexed=true banlisted=false totry=Some(true) ip=false old=false no_ob=false no_ib=false no_crypto=false zh_self=false zh_unknown=true enc_prep=false query=false trace_obs=true failed=true -> P240-C-B-ZERO-HOP-UNKNOWN-RI
attempt 2 target a5f057c6...: identical shape -> P240-C-B-ZERO-HOP-UNKNOWN-RI
attempt 3 target f7260866...: identical shape -> P240-C-B-ZERO-HOP-UNKNOWN-RI
```

Reading: Router A admitted all three response client messages
(`distributeTime` +3, matching helper sendMessage delta 3), found no
local target LS, failed the remote lookup (found 0, failed 3/4),
created two exact streaming search jobs per attempt with Router B in
both initial `toTry` sets, then the pinned `sendQuery()` guard
rejected B with `not doing zero-hop lookup to unknown <B>` (the
selected outbound tunnel was zero-hop while A's main NetDB held no
validated RI for B), so no target DLM was ever dispatched to B
(`Encrypted DLM` 0, `ISJ try` peer count 0) and the search exhausted.
B was answerable throughout (post B main-LS validated/current/RAP),
so the failure is purely the pre-query dispatch guard, not B state,
not publication, not the reply path.

Retained baselines reproduced identically on all three attempts:

```text
P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
P237 stock post 2/1/3/0/0 with all loggers enabled, deltas 2/1/3/0/0
P237-D-CLIENT-MESSAGE-NOT-ADMITTED (distribute 3, dispatch 0/0)
P239-D-TARGET-LEASESET-LOOKUP-FAILED (local absent, found 0, failed 3/4, dispatch 0/0)
p237-router-stages router_i2cp_observed=true, all later stages false
```

Destination-lane context (retained not re-litigated): attempt 1 took
the retained P230 early-stop path (`P230-D-ELIGIBLE-BUT-NO-PROFILE`,
`P231-A-OBSERVABILITY-GAP reason=destination-lane-not-entered`);
attempts 2/3 reproduced the Plan-232 shape
(`P231-REVERSE-DELIVERY-PASSED` role=A, digest-matched) with the
streaming route-parity rows intact. The destination-lane path varies
across attempts while the streaming P240 terminal is identical, so
destination variability cannot explain the streaming attribution;
the streaming-epoch deltas and exact-job correlation isolate the
response epoch regardless.

## Proof of no production change

`git status` on the implementation head is clean apart from the six
harness surfaces (§13 + §14); no `src/` file changed:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
tests/integration/m6-interop/java/ControlledRouter.java
tests/integration/m6-interop/run-java.sh
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P240Probe.java  (new)
```

The M6 checker §29 production-surface guard (no `P240`/`p240` in
`i2pr-daemon/src`, `i2pr-client/src`, `i2pr-tunnel/src`,
`i2pr-runtime/src`) is green. The streaming helper
(`ReferenceStreamingService.java`) carries no P240 surface by checker
guard — the observer lives on Router A, never in the client helper.
Frozen windows (`DATAGRAM_WAIT`/`STREAM_WAIT`/`SYN_ACK_WAIT` = 45 s),
topology, profile policy, publication target, and pins are unchanged;
the §12 forbidden list (`netDb.alwaysQuery`, LS seeding, forced
`toTry`, search-limit/timing changes, topology/floodfill/profile
mutation, republication, Java patching, reflection, production Rust,
window widening, membership-as-query, status-as-stage) is enforced by
checker §29, green.

## Focused verification (implementation head 37025f9)

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p225_ -- --test-threads=1   (4 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1   (7 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p239_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p240_ -- --test-threads=1   (17 passed: 16 §15 rows + token lock)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>   (32 rows, md5 def49e9ea6c180a069665dae237aecff)
javac <all staged Java helpers incl. P240Probe against exact-pinned jars>   (exit 0)
all 18 boundary/vector/evidence scripts in AGENTS.md
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   (18 tests OK)
```

Full serial workspace floor: **passed** on the implementation head
(`cargo test --locked --workspace --all-targets -- --test-threads=1`,
exit 0: 2739 passed, 18 ignored, 103 suites, 618.67 s; +17 vs Plan
239 is exactly the new P240 unit rows). No
`P240-V-WORKSPACE-FLOOR-INCOMPLETE` token was needed.

## Requirement-to-evidence matrix (§18)

1. The exact Plan-239 Streaming response lookup job is correlated by
   helper DBID + target hash + job IDs (2 jobs/attempt, both with B
   in `toTry`; `p240-target-context` + `p240-target-job-trace` +
   `p240-totry` rows on all runs).
2. Plan-239 admission and local-target-LS absence remain reproduced
   (distribute +3, local absent pre/post, found 0, failed 3/4,
   dispatch 0/0 on all runs).
3. Router B's lookup-candidate readiness is measured read-only
   (`p240-router-b-pre/post` identical per run; RI present,
   floodfill-indexed, not banlisted, fresh, tier L, profile present,
   comm established).
4. Initial `toTry` membership for B is known for the exact jobs
   (2/2 both attempts' jobs carry B; `toTry:` line correlation, never
   selector inference).
5. B is queried on no run; the earliest supported sendQuery reason is
   identified without inference (exact-job `not doing zero-hop
   lookup to unknown <B>`; IP-close false so no topology claim;
   old-router/tunnel/crypto guards false).
6. B is never queried, so the retained P225 post-query chain is
   correctly not evaluated (post-query facts recorded as context:
   B answerable, B non-receipt authoritative via `b_dlm_proven_live`,
   A sub-DB empty — none claimed as the terminal).
7. Stale destination-lane lookup failures cannot satisfy the P240
   classifier (target-hash binding; unit rows lock the exclusion;
   live proof: attempt 1 destination early-stop vs attempts 2/3
   reverse-passed with identical P240 terminal).
8. No Java source, production Rust, topology, profile, publication,
   or timing corrective is introduced (clean status + §29 guard
   green + §12 checker green).
9. The same terminal is reproduced on counted execution
   (`P240-C-B-ZERO-HOP-UNKNOWN-RI` on all three same-SHA runs, no
   tuning, unique evidence dirs, byte-identical source lock).
10. Plan-201/204 state is updated together at closure (see
    disposition; no silent unblock).

## Limitations and findings

- Severity medium (boundary, not defect): the attribution stops at
  the first B-specific pre-query guard. Why the selected outbound
  tunnel is zero-hop in the streaming epoch, and whether B's RI
  absence from A's main DB at that instant is a propagation/timing
  effect or a structural bootstrap gap, remain unobserved. A
  successor owns that narrow zero-hop-tunnel/RI-availability
  attribution; no i2pr defect is claimed.
- Severity low (measurement note): each attempt created two exact
  streaming jobs for three helper sends. The P226 single-job
  observability rule therefore reads `observable=false` while P240
  correlation holds over the two-job set. Any successor that needs
  single-job isolation must snapshot tighter (e.g. per-send log
  offsets); Plan 240 does not.
- Severity low (interpretation note): the zero-hop-unknown guard
  matches Plan 226's historical destination-lane baseline
  (`P226-BASELINE-B-ZERO-HOP-UNKNOWN`). Convergence across lanes
  strengthens the boundary but, exactly as in Plan 226, admits no
  topology correction on its own: IP-close is false on all runs, so
  the distinct-loopback /24 correction stays unadmitted.
- The live `P240-C-B-NO-OUTBOUND-LOOKUP-TUNNEL` arm is unreachable by
  construction (the pinned ISJ null-tunnel branch logs nothing); it
  stays unit-locked only and was never claimed live.
- Direction A did not establish; §9 continuation and §19
  OCMOSJ-resume do not trigger. No M6 Java-family closure is
  claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-after-plan240-b-zero-hop-unknown-ri-pending-zero-hop-successor-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-zero-hop-successor-after-plan240
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_239 = passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-pending-zero-hop-successor-plan-of-record
```

Plan 201's streaming axis consumed its `pending-plan240` hard
dependency (the exact streaming-epoch lookup-failure attribution
executed; the first B-specific missing stage is proven with B
eligibility, `toTry` membership, and selection-guard ordering all
measured). But Plan 201 stays blocked: the streaming axis now needs
the narrow zero-hop-tunnel/RI-availability successor (no
plan-of-record yet), and the publication/final-closure axis is
unchanged. Plan 204 stays blocked on M6 Java second-family closure
(now pending that successor instead of Plan 240); M10 product
authority through Plans 213–215 is unchanged. Plan 205 stays
retained/deferred. No other registered plan listed Plan 240 as a
hard dependency, so nothing else changes state. No new plan is
registered here; the successor requires its own plan-of-record under
the same subsystem.

## Follow-up boundary

A successor owns only the narrow zero-hop-tunnel/RI-availability
attribution for the streaming response epoch: which outbound tunnel
the exact streaming ISJ selected (client vs exploratory, hop count),
why it was zero-hop, whether B's RI was genuinely absent from A's
main DB at that instant (read-only main-DB timeline, never a store),
and whether the guard clears when a non-zero-hop tunnel is selected.
It must retain the Plan-240 deltas, correlation key, source lock,
frozen topology/profile/timing, and fail-closed lanes, and stop at
the first proven stage. It must not pre-authorize a topology
correction (IP-close remains unproven), a publication corrective, a
tunnel-policy change, or a production i2pr change unless exact
expected TunnelData proves an i2pr-owned defect.
