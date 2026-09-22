# Plan 241 status — M6 Java Streaming one-hop client-tunnel fixture corrective

Status: `passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary`

Plan 241 removed the exact Plan-240 controlled-fixture cause — the
Streaming helper's forced zero-hop client profile — using only
ordinary public I2CP SessionConfig options mirrored from the proven
raw-helper contract, then continued the already-instrumented
lookup/response path until the next real boundary. On three counted
same-SHA executions with no between-attempt tuning, the lane stopped
at exactly one of two typed build-stage terminals per run: the
pre-helper transit bootstrap gate (`P241-A-TRANSIT-BOOTSTRAP-NOT-READY`,
attempts 2–3, known Plan-230 profile-bootstrap stochasticity) or the
installed-pair gate (`P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT
direction=both`, attempt 1: corrected one-hop settings proven active,
1+1 client tunnels installed with zero-hop absent in both directions,
but not exact-via-C in either direction). The old Plan-240
zero-hop-unknown terminal can no longer fire on this lane by
construction (zero-hop is forbidden and the profile mismatch is a
harness defect, never a fallback). No Java source was patched, no
production Rust changed, no topology, profile, publication, timing, or
pin changed, and no M6 Java-family closure is claimed. The retained
Plan-232 raw-Destination authority and Plans 237–240 baselines are
reproduced intact by the unit/static floor; live §§8–11 continuation
was not reached on any counted run and remains owned by the §20
successor.

## Authority and exact pins

Implementation checkpoint (committed BEFORE counted execution, no
tuning between attempts):

```text
33ebdc4ca3d9d1059b0f768d5021c5130cfb99af (Plan-241 fixture corrective;
          seven files: driver P241 module + 18 unit rows, run-java.sh
          P241-A/B gates, ReferenceStreamingService one-hop contract +
          profile report, ControlledRouter P241-B-RI dispatch, new
          out-of-tree P241Probe.java, source-lock §15 rows, M6 checker
          §30 + superseded-freeze re-scope)
```

References unchanged:

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained: Plan 240 exact-ISJ correlation key, source-lock rows, and
terminal taxonomy (reused verbatim for non-zero-hop pre-query
guards); Plan 239 D1 lookup-failure deltas; Plan 238 admission
deltas; Plan 237 stock deltas (2/1/3/0/0); Plan 232
route-derived lease-gateway correction and bidirectional
raw-Destination pass (code untouched); frozen topology, profile
policy, publication target, and timing windows
(`DATAGRAM_WAIT`/`STREAM_WAIT`/`SYN_ACK_WAIT` all still 45 s).

## Fixture corrective (WP §5)

`ReferenceStreamingService.java` mirrors the already-proven optional
explicit-peer contract in `ReferenceRawDestination` verbatim:

- before: hard-coded `inbound.length=0`, `outbound.length=0`,
  quantity 1, backups 0, `allowZeroHop=true` both directions;
- after: with an explicit Router-C peer (5th argument or
  `I2PR_M6_JAVA_EXPLICIT_PEER_B64` env, strict 43/44-char I2P Base64
  validated through Java's own decoder + 32-byte `Hash` shape):
  `inbound.length=1`, `outbound.length=1`, quantity 1, backups 0,
  `allowZeroHop=false` both directions,
  `inbound/outbound.explicitPeers=<Router-C>`; without it the legacy
  zero-hop profile is retained (non-counted diagnosis only).
- lease-set type 3, enc type 4, publication flags, `fastReceive`,
  `BestEffort` reliability, Destination generation, Streaming
  behavior, and response scheduling are byte-identical between
  branches (checker §30d locks every retained key).
- new bounded `REPORT_TUNNEL_PROFILE` control command reports only
  lengths/booleans (`TUNNEL_PROFILE inbound_length=1
  outbound_length=1 inbound_allow_zero_hop=false
  outbound_allow_zero_hop=false explicit_peers_set=true`); never the
  peer value, keys, tags, or payloads.

Proof the raw helper is unchanged: `ReferenceRawDestination.java`
does not appear in `33ebdc4 --stat`; checker §30e locks its retained
Plan-227 contract and rejects any `P241`/`REPORT_TUNNEL_PROFILE`
surface in it (green).

## Source lock (WP §15)

`scripts/interop/check-m6-java-response-source-lock.sh` gains two
pinned source files (`TunnelPoolManager.java`, `TunnelPool.java`)
and eleven additive needles (all verified present on the exact pin;
retained 32 rows frozen). The TSV grows 32 → 35 rows and is
byte-identical across the three counted attempts:

```text
md5 e1c35ffe1c40633f08fd5a2baf94c7a1  (attempts 1/2/3, 35 rows)
```

Locked facts: `selectOutboundTunnel(destination, closestTo)` serves
`_clientOutboundPools.get(destination)` via `pool.selectTunnel`
(no exploratory fallback when a client pool exists);
`selectTunnel(Hash closestTo)` sorts with
`TunnelInfoComparator(closestTo, avoidZeroHop)` where
`avoidZeroHop = !getAllowZeroHop()` (zero-hop-last rule);
`sendQuery()` reads `ctx.netDb().lookupRouterInfoLocally(peer)` for
send preparation but guards on
`_facade.lookupLocallyWithoutValidation(peer)` with
`outTunnel.getLength() <= 1`, dispatching only via
`dispatchOutbound(outMsg, outTunnel.getSendTunnelId(0), peer)`.

## Observation mechanism (WP §6/§7/§8 + driver)

- Shell P241-A gate (read-only retained probes, streaming section):
  Router-C identity reused from the destination section when valid
  else derived via P220 self snapshot + P224-HASH-B64 render;
  P227-PEER-ELIGIBILITY (main raw/valid, selectable, banlist);
  P230-CAPABILITY with the exact-pinned `shouldCreate` predicate
  mirror plus the retained 6×5 s natural-bootstrap wait; C self-view
  RI match; P229-EXPLORATORY-TUNNELS non-zero both directions
  (12×5 s poll). Failure emits exactly
  `P241-A-TRANSIT-BOOTSTRAP-NOT-READY` and stops: no helper start,
  no zero-hop fallback, no tuning.
- Corrected helper start passes the explicit Router-C B64 as the 5th
  argument; the `p241-helper-profile` row must equal the exact
  one-hop shape or the shell exits 72 (deterministic fixture
  defect, never a counted terminal, never a fallback).
- Shell P241-B gate reuses retained `P227-CLIENT-TUNNELS` with the
  streaming helper client DBID (via `P223-DEST-INSPECT`): requires
  inbound+outbound exact-via-C with zero-hop absent both directions,
  else exactly `P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT
  direction=<inbound|outbound|both>` and the driver is skipped.
- Driver (only runs past both gates): re-proves the helper profile
  pre-SYN (`REPORT_TUNNEL_PROFILE`, stops the epoch on mismatch);
  records `p241-lane-context`; collects `P241-B-RI` pre/post through
  the new `P241Probe` (main raw/valid + client resolved/raw/valid,
  local reads only, never a client-DB store); re-queries the
  authoritative pool at the lookup epoch; emits exactly one
  `p241-classification` via the ordered P241 classifier (gates →
  pool-zero-hop → retained P240 ordering with the zero-hop-unknown
  split on proven pool state → P241-D chain in P225 order).
- OCMOSJ resume is locked to `P241-D-LOOKUP-SUCCEEDED`
  (`p241_ocmosj_resume_allowed`); M6 closure is locked to never
  (`p241_m6_closure_claimable` is false for all terminals — Plan 201
  owns final closure); production change is locked to never
  (`p241_authorizes_production_change` is false for all terminals).

## Counted evidence (SHA 33ebdc4, no tuning between attempts)

| Attempt | Evidence dir | Bootstrap gate | Helper profile | Client pair | Terminal |
| --- | --- | --- | --- | --- | --- |
| 1 | `target/interop/m6-java-evidence-plan241-attempt1` | pass (eligible=1, probe true, profile true, RI match, expl 3/3) | one-hop proven | 1+1 tunnels, zero absent both, exact false both | `P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT direction=both` |
| 2 | `target/interop/m6-java-evidence-plan241-attempt2` | fail (`profile_present=false`; all else pass incl. RI match, expl 4/6) | helper never started | — | `P241-A-TRANSIT-BOOTSTRAP-NOT-READY` |
| 3 | `target/interop/m6-java-evidence-plan241-attempt3` | fail (identical shape: `profile_present=false`; RI match, expl 3/3) | helper never started | — | `P241-A-TRANSIT-BOOTSTRAP-NOT-READY` |

Attempt-1 pair row (authoritative installed state):

```text
p241-client-tunnels client_resolved=true inbound_tunnel_count=1 outbound_tunnel_count=1 inbound_exact_one_remote_hop_via_c=false outbound_exact_one_remote_hop_via_c=false inbound_zero_hop_present=false outbound_zero_hop_present=false
```

Reading: the corrected one-hop SessionConfig is proven active, one
client tunnel is installed per direction, no zero-hop tunnel remains
in either pool — but neither installed tunnel is the exact
length-2 local+Router-C path the retained probe defines. The old
Plan-240 zero-hop-unknown guard therefore cannot fire on this lane;
the lane correctly stops at the pair gate without running the
driver, without any zero-hop fallback, and without inferring why the
installed tunnels are not exact-via-C (that attribution is owned by
the §20 stock client-build successor, not this run).

Attempts 2–3 reproduce the known Plan-230 natural-bootstrap
stochasticity (profile 1/3 in this sample vs 2/3 in Plan 230; Plan
240 attempt 1 took the same early-stop shape in its destination
lane): every other bootstrap fact passes (eligibility, selectability,
RI match, non-zero exploratory both directions) while the helper
client profile never appears within the retained 6×5 s bound. The
plan explicitly requires this stochasticity to be a typed terminal,
not a mid-budget correction — the lane obeys it.

Lane exit code is 1 on all three runs (fail-closed Java-family
verdict with the terminal row as authority, precedented by Plans
227–229 build-stage closures). Driver-dependent observation rows
(`p241-lane-context`, `p241-b-ri-pre`, `p241-pool-epoch`) correctly
report absent on pre-driver terminals; the authoritative
`p241-classification` / `external-p241-classification` rows pass on
all three runs. Raw reference logs are scratch-only; evidence holds
only sanitized counts/booleans/hashes.

Not reached on any counted run (honestly unexecuted, unit/static
only): §8 live B-RI visibility rows, exact Streaming ISJ
correlation, the §9 lookup continuation, the §10 D chain, §11
OCMOSJ/i2pr continuation, Direction A. No B query was dispatched, no
lookup success is claimed, no i2pr defect is claimed.

## Proof of no production change

`git show 33ebdc4 --stat` touches exactly seven harness/test-only
files; no `src/` file changed:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
tests/integration/m6-interop/java/ControlledRouter.java
tests/integration/m6-interop/java/ReferenceStreamingService.java
tests/integration/m6-interop/run-java.sh
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P241Probe.java (new)
```

The M6 checker §29/§30 production-surface guards (no `P240`/`p240`
or `P241`/`p241` in `i2pr-daemon/src`, `i2pr-client/src`,
`i2pr-tunnel/src`, `i2pr-runtime/src`) are green on the
implementation head. The §14 forbidden list (raw-helper change,
Java source/jar patching, client-DB B-RI store, direct tunnel
install, profile/tier mutation, `netDb.alwaysQuery`, role/address
changes, C1/C2 semantic changes, window widening, zero-hop in the
counted lane, missing explicit C, membership-as-query, success
without subDB proof, pre-TunnelData i2pr-defect claims, production
`src/` code, raw-log promotion, `|| true` forgiveness) is enforced
by checker §30, green. Frozen windows, topology, profile policy,
publication target, and pins are unchanged.

## Focused verification (implementation head 33ebdc4)

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p225_ -- --test-threads=1   (4 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1   (7 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1   (21 passed, 1 ignored)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1   (21 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p239_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p240_ -- --test-threads=1   (17 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p241_ -- --test-threads=1   (18 passed: 17 §16 rows + token lock)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>   (35 rows, md5 e1c35ffe1c40633f08fd5a2baf94c7a1)
javac <all staged Java helpers incl. P241Probe against exact-pinned jars>   (exit 0; one pre-existing P237 deprecation note)
all 18 boundary/vector/evidence scripts in AGENTS.md
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   (18 tests OK)
```

Full serial workspace floor: **passed** on the implementation head
(`cargo test --locked --workspace --all-targets -- --test-threads=1`,
exit 0: 2757 passed, 18 ignored, 103 suites, 638.77 s; +18 vs Plan
240 is exactly the new P241 unit rows).

## Requirement-to-evidence matrix (§19)

1. Plan-240 authority retained (exact correlation key, source-lock
   rows, terminal taxonomy reused; frozen topology/profile/timing;
   `P240-C-B-ZERO-HOP-UNKNOWN-RI` dispositioned below).
2. Streaming zero-hop fixture removed for this lane (one-hop
   SessionConfig proven active live on attempt 1 via
   `p241-helper-profile`; shell exit-72 hard-fail + driver epoch
   stop forbid any non-one-hop profile; zero-hop fallback never
   taken on any run).
3. Raw helper unchanged (absent from implementation commit; checker
   §30e green).
4. C bootstrap proven before helper start (attempt-1 gate row with
   eligibility/profile/RI-match/exploratory; attempts 2–3 prove the
   gate fires honestly when the profile never bootstraps).
5. Genuine one-hop pair through C installed OR exact build-stage
   terminal closes the run: attempt 1 installs 1+1 non-zero-hop
   tunnels but not exact-via-C → exact `P241-B` terminal; attempts
   2–3 → exact `P241-A` terminal. No run inferred past its gate.
6. Zero-hop tunnels absent from continuation (attempt-1 pool row:
   zero-hop false both directions; pool-epoch re-query armed in the
   driver for post-gate runs).
7. B-RI visibility recorded separately for main/client facades
   (P241-B-RI probe + driver pre/post rows exist and are unit-locked;
   live rows unexecuted — honestly recorded as absent, never
   zero-as-fact).
8. Exact Streaming ISJ correlated (machinery retained and
   unit-locked; live correlation unexecuted — never claimed).
9. Zero-hop-unknown terminal disposition: cannot fire on this lane
   by construction (profile gate + pool gate precede the lookup);
   the classifier retains the contradiction-vs-reuse split
   unit-locked for post-gate runs.
10. B-query/reply/client-subDB chain evaluated in order when reached
    (unit-locked P241-D ordering; live chain unreached — never
    claimed).
11. No production/Java-source/topology/profile/publication/timing
    workaround introduced (commit file list + §29/§30 guards green).
12. Counted attempts obey same-SHA/no-tuning discipline (33ebdc4
    committed before attempt 1; `git status` clean throughout;
    fresh disposable A/B/C datadirs per attempt; unique evidence
    dirs; raw logs scratch-only).
13. Plan-201/204 updated together at closure (see disposition; no
    silent unblock).

## Limitations and findings

- Severity medium (boundary, not defect): attempt 1 proves the
  corrected settings install 1+1 non-zero-hop client tunnels that
  are not the exact local+Router-C path, but not WHY. Candidates
  (longer path including C, length-2 via a non-C peer, probe
  length-accounting edge) are unobserved from the retained probe,
  which reports only counts + exact + zero-hop booleans. The §20
  stock client-build successor owns this attribution; no i2pr
  defect is claimed.
- Severity low (evidence precision): the shell `p241-client-tunnels`
  `*_nonzero_implied` fields encode only the exact⇒non-zero
  direction. On attempt 1 the stronger reading holds (count=1 ∧
  zero-hop absent ⇒ the installed tunnel is non-zero; exact=false
  ⇒ it is not via C). The gate itself consumes exact/zero directly
  and is unaffected. A successor that needs per-length histograms
  must extend the probe under a new plan-of-record; it must not
  reinterpret these rows.
- Severity low (sample variance): the bootstrap profile gate passed
  1/3 in this sample (vs 2/3 natural bootstrap in Plan 230). Both
  outcomes are exact typed terminals; neither was tuned mid-budget.
  A successor needing tighter bootstrap repeatability must own a
  repeatability plan per §20 (bootstrap-repeatability arm), not
  widen bounds here.
- Live §§8–11 driver evidence (B-RI split, ISJ correlation, D
  chain, OCMOSJ resume, Direction A) is implemented, unit-locked,
  and statically guarded but unexecuted: no counted run passed the
  B gate. The first post-gate run will exercise it unchanged.
- Direction A did not establish; §11 continuation and §12 closure
  do not trigger. No M6 Java-family closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-after-plan241-a-b-build-stage-pending-successor-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-successor-after-plan241
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-pending-section-20-successor-plan-of-record
```

Plan 201's streaming axis consumed its `pending-plan241` hard
dependency (the fixture corrective executed; the zero-hop cause is
removed on this lane; the lane now stops at exact A/B build-stage
terminals with the old zero-hop terminal structurally unreachable).
But Plan 201 stays blocked: the streaming axis now needs the narrow
§20 successor (bootstrap-repeatability arm for the A variance;
stock client-build corrective arm for the attempt-1 not-exact-via-C
finding), and the publication/final-closure axis is unchanged. Plan
204 stays blocked on M6 Java second-family closure (now pending
that successor instead of Plan 241); M10 product authority through
Plans 213–215 is unchanged. Plan 205 stays retained/deferred. No
other registered plan listed Plan 241 as a hard dependency, so
nothing else changes state. No successor is pre-registered here;
the §20 successor requires its own plan-of-record under the same
subsystem, registered only from the exact final boundary above.

## Follow-up boundary

A successor owns exactly one §20 arm from the exact final boundary:
bootstrap not ready → bootstrap repeatability; one-hop pair not
built (1+1 non-zero-hop tunnels installed but not exact-via-C) →
stock client-build corrective (which outbound/inbound tunnel the
exact streaming ISJ would select, why the installed pair is not the
exact local+C path, and whether the guard clears when it is);
B query sent but reply chain fails → B-answer/A-DSM corrective;
lookup succeeds but OCMOSJ fails later → resume exact Plan-239 D/E
attribution; expected TunnelData reaches i2pr then fails →
production corrective may be authorized for that exact i2pr-owned
stage; Direction A establishes → final bidirectional/publication
closure remains with Plan 201. It must retain the Plan-241 gates,
correlation key, source lock, frozen topology/profile/timing, and
fail-closed lanes, and stop at the first proven stage. It must not
pre-authorize a topology correction, a publication corrective, a
tunnel-policy change, or a production i2pr change unless exact
expected TunnelData proves an i2pr-owned defect.
