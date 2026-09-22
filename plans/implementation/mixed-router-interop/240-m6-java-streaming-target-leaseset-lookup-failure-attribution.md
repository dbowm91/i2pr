# Plan 240 — M6 Java Streaming target-LeaseSet lookup-failure attribution

Status: **registered-ready-m6-java-streaming-target-leaseset-lookup-failure-attribution**

## 1. Objective

Attribute the exact reason the Plan-239 Streaming response cannot resolve the
i2pr target LeaseSet from Router A's helper client NetDB.

Plan 239 proved, on three counted executions of `d82cf06`:

```text
helper response sendMessage delta = 3
Router-A client.distributeTime delta = 3
Router-A local target LS = absent
Router-A remote LS lookup success delta = 0
Router-A remote LS lookup failure delta = 3 / 4 / 4
Router-A dispatchTime / dispatchSendTime delta = 0 / 0
installed helper client outbound/inbound tunnels = 1 / 1
```

and closed at:

```text
P239-D-TARGET-LEASESET-LOOKUP-FAILED
```

Plan 240 must explain that lookup failure. It must not change routing behavior
until the first failing lookup stage is proven.

## 2. Why one plan is sufficient for this round

The repository already contains the required lookup-path observability from
Plans 224–226:

- Plan 224: Router-B main-NetDB target LS2 and Router-A helper client-subDB
  snapshots;
- Plan 225: exact target-correlated search lifecycle and
  `A query dispatch -> B receipt -> B answer -> A client-tunnel DSM ->
  client-subDB install` tracing;
- Plan 226: exact target-job correlation, Router-B correlation, and IP-diversity
  skip attribution.

Plan 225 previously closed the destination lane with:

```text
P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
```

Plan 226 then proved the historical target job did **not** justify the proposed
IP-diversity topology correction.

Plan 240 therefore reuses those surfaces inside the now-proven Streaming
response epoch. Do not add another broad lookup framework.

A production or topology corrective is deliberately **not** pre-registered.
The exact Plan-240 terminal must authorize any successor.

## 3. Exact pins and retained authority

Keep unchanged:

```text
Java I2P 2.13.0
9134f808337b401e8e53c73734c81fab04280c9d

i2pd 2.61.0
635b013a612ff47278ef02acf8580a28e10e26c5
```

Retain unchanged:

- Plan 232 route-derived lease-gateway correction and raw bidirectional pass;
- Plan 237 helper response deltas / source lock;
- Plan 238 Router-A I2CP admission proof;
- Plan 239 post-admission OCMOSJ / target-LS failure proof;
- Plan 225/P226 lookup trace semantics;
- Router A/B/C roles and controlled topology;
- publication target Router B;
- helper client DBID / target hash derivation;
- 45-second Streaming response window;
- fail-closed evidence semantics.

## 4. Exact-pinned source ordering

The exact Java source establishes this lookup sequence.

### 4.1 OCMOSJ enters client NetDB lookup

After Router-A admission, `OutboundClientMessageOneShotJob` first checks the
helper client DB locally. If the target LS is absent, it calls the helper
client NetDB lookup path.

Plan 239 already proves this local miss plus eventual remote lookup failure.

### 4.2 IterativeSearchJob startup

For the exact helper DBID and target hash, `IterativeSearchJob.runJob()`
performs, in order:

1. negative-cache check;
2. floodfill peer selection through the main DB's
   `FloodfillPeerSelector`;
3. removal of self and target from the initial candidate set;
4. registration of the exact search job;
5. INFO `New ISJ ... toTry: ...`;
6. `retry()`.

### 4.3 retry() peer selection

`retry()`:

- chooses from `_toTry` in routing-key order;
- may skip a peer for `IP_CLOSE_BYTES = 3` diversity;
- tracks skipped/failed/unheard peers;
- stops when limits / available peers are exhausted;
- calls `sendQuery(peer, previouslyTried)` only after the peer passes that
  stage.

### 4.4 sendQuery() pre-dispatch guards

For a client LeaseSet lookup, `sendQuery()` may stop before creating /
dispatching the DLM because:

- RouterInfo fails `StoreJob.shouldStoreTo()`;
- no usable outbound client/exploratory tunnel is available;
- no inbound **client** reply tunnel is available;
- the helper destination lacks compatible reply-encryption keys;
- Router B RI does not support the required encrypted reply mode;
- zero-hop target/unknown-RI guards reject the query;
- encrypted-reply session generation / wrapping fails.

Only after these guards does the exact-source INFO row:

```text
ISJ try ... for LS <target> to <peer>
```

become valid query-dispatch preparation evidence.

Plan 225's `query_to_b` remains the authoritative actual-target-query
criterion; selector membership alone is insufficient.

### 4.5 Post-query path

If Router B is actually queried:

```text
A DLM dispatch
-> B receives exact target DLM
-> B has current query-answerable target LS2
-> B emits answer
-> A helper inbound-client tunnel receives encrypted DSM
-> helper client NetDB stores target LS
-> OCMOSJ lookup success callback / client.leaseSetFoundRemoteTime
```

Plan 240 may continue only as far as evidence supports and must stop at the
first missing stage.

## 5. Router-B selection facts to capture

Before the exact Streaming lookup epoch, collect bounded read-only Router-A
facts for Router B:

```text
b_ri_present
b_floodfill_capability_in_ri
b_peer_manager_f_capability_indexed
b_banlisted_forever
b_ri_age_bucket
b_bandwidth_tier
b_profile_present
b_last_send_failed_recent
b_comm_established
```

These are explanatory facts only. They do not themselves prove that B was or
was not selected for the exact target search.

Prefer existing public/read-only P220/P226 probe patterns. Do not create
profiles, capabilities, rates, or RouterInfo entries.

## 6. Reuse of Plan-225 / Plan-226 observability

The Plan-240 implementation should first attempt to reuse, unchanged:

```text
p225 logger configuration
p224 target context
p224 lookup trace
p225 Base32/DBID correlation
p226 exact target-job ID correlation
p226 exact Router-B peer correlation
p226 IP-close skip detection
```

Only additive changes needed to correlate those facts to the
**Plan-239 Streaming epoch** are allowed.

Expected implementation approach:

1. enable the already-approved class-scoped lookup loggers before the
   Streaming SYN;
2. snapshot Plan-239 admission/lookup counters;
3. capture the exact target ISJ job created by the response send;
4. bind all P225/P226 log facts to that exact job ID + target + helper DBID;
5. record Router-B preflight facts;
6. run the existing ordered lookup-path trace;
7. emit one Plan-240 terminal.

Do not run an independent standalone lookup to obtain evidence.

## 7. Initial-candidate and selector interpretation

Use the source-locked `New ISJ ... toTry: ...` row for the exact target job.

If Router B is absent from `toTry`, distinguish as far as supported:

### 7.1 B not eligible as a floodfill candidate

If exact read-only facts prove Router B is absent from the peer-manager
floodfill capability index or forever-banlisted:

```text
P240-B-B-NOT-FLOODFILL-CANDIDATE
```

Record the exact reason.

### 7.2 B floodfill-eligible but not returned in initial selection

If B is floodfill-indexed, not forever-banlisted, and its RI is present, but B
is absent from the exact job's `toTry` list:

```text
P240-B-B-ELIGIBLE-NOT-IN-INITIAL-SELECTION
```

Capture source-locked FloodfillPeerSelector DEBUG classification for B where
available:

```text
Same /16, family, or port
Old
Bad country
Slow
Bad (new)
Good
OK
Bad (DB)
Bad (no hist)
Bad (no prof)
```

These categories rank/deprioritize peers; do not assume any one category
necessarily excludes B unless the exact returned list proves it.

If selection-limit/ranking is the only supported explanation, record that
rather than inventing a stronger root cause.

### 7.3 B present in initial selection

If B appears in the exact `toTry`, continue to §8.

## 8. Exact-job pre-query classifier

For an exact target ISJ with B in or later added to its candidate set, classify
the first proven B-specific boundary:

```text
P240-C-B-IP-DIVERSITY-SKIPPED
P240-C-B-NOT-REACHED-BEFORE-SEARCH-EXHAUSTION
P240-C-B-OLD-OR-UNSUPPORTED-ROUTER
P240-C-B-NO-OUTBOUND-LOOKUP-TUNNEL
P240-C-B-NO-INBOUND-CLIENT-REPLY-TUNNEL
P240-C-B-NO-COMPATIBLE-REPLY-ENCRYPTION
P240-C-B-ZERO-HOP-SELF-LOOKUP
P240-C-B-ZERO-HOP-UNKNOWN-RI
P240-C-B-ENCRYPTED-LOOKUP-PREP-FAILED
P240-C-B-QUERY-DISPATCHED
P240-C-B-PREQUERY-OBSERVABILITY-GAP
```

Rules:

- retain Plan 226's exact peer/job correlation;
- do not infer IP-close from topology shape;
- do not infer query dispatch from B being in `toTry`;
- do not infer a tunnel failure from installed-tunnel counts alone;
- a generic search failure without B-specific correlation is not enough for a
  B-specific terminal.

## 9. Post-query continuation

Only after:

```text
P240-C-B-QUERY-DISPATCHED
```

may Plan 240 classify:

```text
P240-D-B-LOOKUP-NOT-RECEIVED
P240-D-B-TARGET-LS-NOT-QUERY-ANSWERABLE
P240-D-B-ANSWER-NOT-EMITTED
P240-D-A-CLIENT-TUNNEL-DSM-NOT-RECEIVED
P240-D-A-CLIENT-SUBDB-NOT-INSTALLED
P240-D-LOOKUP-SUCCEEDED
```

Use the retained P225 semantic ordering.

If `P240-D-LOOKUP-SUCCEEDED` is reached, the Plan-239 OCMOSJ classifier may
resume and must identify the next D/E boundary. Do not close M6 from lookup
success alone.

## 10. Negative-cache guard

Because exact `IterativeSearchJob.runJob()` checks negative cache before
selection, Plan 240 must explicitly distinguish:

```text
P240-B-TARGET-NEGATIVE-CACHED
```

from an actual search that starts and later fails.

A positive `client.leaseSetFailedRemoteTime` delta does not by itself prove
an IterativeSearchJob was created or queried any peer.

## 11. Epoch isolation

The Plan-239 closure recorded `failed_remote_delta` 3/4/4 while
`client.distributeTime` was exactly +3, because stale destination-lane
timeout jobs can fire inside the Streaming window.

Plan 240 must eliminate that ambiguity at the attribution layer by correlating
the **exact Streaming response search jobs**, not by counting global
`leaseSetFailedRemoteTime` events.

Required key:

```text
helper DBID + target hash + ISJ job ID + Streaming epoch
```

Uncorrelated stale destination-lane job output must be ignored.

If exact job correlation cannot be established:

```text
P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED
```

and stop.

## 12. Forbidden shortcuts

Plan 240 must not:

- set `netDb.alwaysQuery`;
- directly seed/copy the target LS into A's helper client DB;
- force Router B into `_toTry`;
- change `netdb.searchLimit`, `netdb.maxConcurrent`, or search timing;
- alter A/B/C IP topology;
- alter floodfill roles/capabilities/profile state;
- republish the target LS as a corrective;
- patch Java source;
- use reflection/private mutation;
- modify i2pr production Rust;
- widen the 45-second response window;
- interpret selector membership as a query;
- interpret a status code as a lookup-path stage.

## 13. Expected implementation surfaces

Prefer changes only in existing test/diagnostic surfaces:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
tests/integration/m6-interop/run-java.sh
tests/integration/m6-interop/java/ControlledRouter.java
existing P224/P225/P226 probe/helper source if an additive read-only fact is needed
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
```

Do not add a new helper class if an existing P224/P225/P226 probe can expose
the required fact safely.

## 14. Source-lock requirements

Extend the exact Java source lock only for facts actually consumed by Plan 240.
At minimum lock:

```text
IterativeSearchJob negative-cache check
selectFloodfillParticipants(...)
New ISJ ... toTry
Skipping query w/ router too close to others
StoreJob.shouldStoreTo(ri)
failed, no IB client tunnel to receive reply
skipped, no ratchet/elg support
not doing zero-hop self-lookup
not doing zero-hop lookup to unknown
ISJ try ... for LS ... to ...
FloodfillPeerSelector floodfill capability source
FloodfillPeerSelector forever-banlist exclusion
FloodfillPeerSelector B classification log family
```

Retain all prior Plan-236–239 source-lock rows.

## 15. Required focused tests

Add at least equivalent unit rows:

```text
p240_requires_exact_streaming_isj_correlation
p240_negative_cache_precedes_candidate_selection
p240_candidate_membership_does_not_equal_query
p240_b_absent_from_totry_requires_candidate_readiness_facts
p240_b_eligible_not_selected_is_distinct_from_not_floodfill
p240_ip_close_requires_exact_b_job_correlation
p240_old_router_guard_precedes_query_dispatch
p240_no_client_reply_tunnel_precedes_query_dispatch
p240_reply_encryption_guard_precedes_query_dispatch
p240_zero_hop_unknown_guard_precedes_query_dispatch
p240_query_dispatch_required_before_b_receipt
p240_b_receipt_required_before_b_answer
p240_b_answer_required_before_a_dsm
p240_a_dsm_required_before_client_subdb_install
p240_stale_destination_lookup_job_does_not_classify_streaming
p240_no_production_change
```

Retain full P225/P226/P237/P238/P239 focused suites.

## 16. Attempt discipline

- implementation committed before counted runs;
- maximum three counted attempts per implementation SHA;
- no between-attempt tuning;
- no topology/profile/timing changes;
- fresh scratch/log evidence dirs;
- parser/correlation defect requires a new SHA and restarted attempt budget;
- raw logs remain scratch-only;
- durable evidence contains bounded typed facts only.

## 17. Verification floor

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets

cargo test --locked -p i2pr-daemon --test java_tunnel_external p225_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p239_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p240_ -- --test-threads=1

cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>

javac <all staged Java helpers against exact-pinned jars>
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
```

Attempt the full serial workspace floor:

```bash
cargo test --locked --workspace --all-targets -- --test-threads=1
```

If an unrelated historical SAM stall reappears, record it exactly and do not
modify SAM under Plan 240.

## 18. Acceptance criteria

Plan 240 closes correctly when all of the following hold:

1. The exact Plan-239 Streaming response lookup job is correlated by helper
   DBID + target hash + job ID.
2. Plan-239 admission and local-target-LS absence remain reproduced.
3. Router B's lookup-candidate readiness is measured read-only.
4. Initial `toTry` membership for B is known for the exact job.
5. If B is not queried, the earliest supported selector/retry/sendQuery reason
   is identified without inference.
6. If B is queried, the retained P225 post-query chain is evaluated in order.
7. Stale destination-lane lookup failures cannot satisfy the Plan-240
   classifier.
8. No Java source, production Rust, topology, profile, publication, or timing
   corrective is introduced.
9. The same terminal is reproduced on counted execution or variability is
   explicitly recorded.
10. Plan-201/204 state is updated together at closure.

## 19. Successor authorization

Plan 240 itself is diagnostic.

Register a corrective successor only for the proven terminal:

- B not floodfill-indexed / selector readiness defect -> narrow controlled
  fixture/bootstrap corrective;
- B eligible but not selected -> selection/routing-key/ranking corrective only
  if the controlled fixture is proven non-representative;
- IP-close -> topology corrective only if Plan 226's prior contrary evidence
  is superseded by exact current evidence;
- missing client reply tunnel -> client-tunnel corrective;
- reply-encryption incompatibility -> exact key/capability fixture corrective;
- B query dispatched but reply path fails -> narrow B-answer/A-client-DSM
  corrective;
- lookup succeeds -> resume Plan-239 OCMOSJ/tunnel attribution;
- observability gap -> observability-only successor.

Do not pre-authorize a production i2pr change.

## 20. Closure evidence

`plans/closure/mixed-router-interop/240-status.md` must record:

- implementation SHA(s);
- exact pins;
- exact Streaming target/job correlation;
- Router-B readiness facts;
- initial candidate / `toTry` facts;
- exact B-specific skip/sendQuery facts;
- P225 post-query facts if reached;
- per-attempt terminal;
- proof stale destination jobs were excluded;
- retained Plan-232/237/238/239 authority;
- no-production-change proof;
- focused/full verification;
- Plan-201/204 unblock audit;
- whether a corrective successor is authorized and why.

## 21. Registration disposition

```text
plan_239 = passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary
plan_240 = registered-ready-m6-java-streaming-target-leaseset-lookup-failure-attribution

plan_201 = blocked-after-plan239-target-leaseset-lookup-failed-pending-plan240-attribution-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan240
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 240-m6-java-streaming-target-leaseset-lookup-failure-attribution
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 22. Smaller-model handoff

Do not change the network.

Start with the exact Streaming response lookup that Plan 239 proved failed.

Answer these questions in order:

```text
1. Can the exact Streaming target ISJ be correlated?
2. Is the target negative-cached?
3. Is Router B floodfill-indexed, not forever-banlisted, and known by RI?
4. Is Router B in that exact ISJ's initial toTry set?
5. If present, is B skipped before sendQuery? Why exactly?
6. If sendQuery starts, does a client tunnel / reply-encryption / zero-hop guard reject it?
7. Is a target DLM actually dispatched to B?
8. If yes, where does the retained P225 reply/install chain first stop?
```

Stop at the first proven missing stage.
