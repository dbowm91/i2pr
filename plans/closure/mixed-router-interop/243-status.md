# Plan 243 status — M6 Java Streaming hosted stock-client-build qualification

Status: `passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established`

Plan 243 qualified the host, executed the frozen Plan-242 Streaming
lane three times on one implementation SHA without tuning, and
classified the first live boundary. Direction A (i2pr → Java
Streaming) established on two of three counted attempts; the third
attempt stopped earlier at the stock-Java five-minute
`I2PSession.connect()` handshake ceiling.

```text
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan243
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan243-reverse-direction-and-publication-closure
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
next_executable_plan = none-pending-m6-java-streaming-reverse-direction-and-publication-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## Authority and exact pins

Implementation checkpoint (committed BEFORE any counted attempt; one
SHA for the entire three-attempt budget):

```text
f359baba57fc7d952ab7f5a5367d34671c590b18 — Plan-243 implementation
+ closure authority. Five files: scripts/interop/check-p243-host-
qualified.sh (eight bounded §4 reasons + workspace-SHA gate + exact
Java reference cache verification + loopback port preflight +
fail-closed exit 70), crates/i2pr-daemon/tests/java_tunnel_external.rs
(15 §13 unit rows: p243_host_not_qualified_does_not_consume_attempt,
p243_host_gate_requires_exact_workspace_sha, p243_host_gate_requires_
java_reference_cache, p243_host_gate_requires_i2pr_daemon,
p243_counted_attempt_requires_host_qualified, p243_counted_attempt_
reuses_plan242_nonzero_pair_gate, p243_exact_via_c_not_required,
p243_explicit_branch_not_required, p243_three_attempt_budget_no_
retry_until_c, p243_lookup_continuation_requires_nonzero_pair,
p243_production_change_requires_expected_tunneldata, p243_direction_a_
does_not_close_m6, p243_no_production_change, p243_three_attempts_no_
tuning_between, p243_no_publication_corrective), and scripts/check-m6-
mixed-router-acceptance-evidence.sh §32 (Plan 243 host-script surface,
unit-row surface, production-Rust P243-HOST guard, implementation-
plan invariant text, and closure-record presence). No production Rust
change, no Java source change, no harness change, no topology /
profile / publication / timing change.
```

References unchanged (frozen reference pins per
`specs/SOURCES.md` + `specs/IMPLEMENTATIONS.md`):

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained Plan-242 §31 source/static/unit floors (Plan 243 adds no
new surface): TunnelPeerSelector / ClientPeerSelector source lock
(35 → 38 needles; `md5 a00d1f19a7b2a262fa9ab2c8dae7c75c`,
byte-identical across all three attempts); Plan-242 extended
`P227Probe.Tunnels` snapshot parser; Plan-242 §6 extended
`P242-CLIENT-TUNNELS` observation; Plan-242 §7 corrected non-zero
pair gate; Plan-240/241 streaming driver unchanged; frozen 45-second
windows; topology (`baseline`), Router-A-only small-router
exploratory profile, Router-B publication target.

## Host qualification (Plan 243 §4)

```text
workspace_sha                    f359baba57fc7d952ab7f5a5367d34671c590b18
workspace_sha_expected           f359baba57fc7d952ab7f5a5367d34671c590b18
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

Sanitized evidence: `target/interop/m6-java-evidence/p243-host-qualification/host-qualification.tsv`.
Source-lock TSV: `target/interop/m6-java-evidence/p243-host-qualification/source-lock.tsv`
(`md5 a00d1f19a7b2a262fa9ab2c8dae7c75c`, byte-identical to the
Plan-242 TSV).

## Counted-attempt summary (Plan 243 §6 / §11)

Three counted attempts on `f359baba57fc7d952ab7f5a5367d34671c590b18`,
no tuning, fresh A/B/C RouterContexts per attempt, unique evidence
directories. The shell ran the frozen Plan-242 lane verbatim
(`I2PR_M6_JAVA_DRIVER=streaming bash tests/integration/m6-interop/run-java.sh`)
without altering the helper, the launcher, the topology, the
profile, the publication target, or the timing windows.

| # | Evidence directory | Stock candidate gate | Non-zero pair gate | Streaming driver | Deepest terminal |
|---|---|---|---|---|---|
| 1 | `target/interop/m6-java-evidence/attempt-1/` | passed (`stock_candidate_population_nonempty=1`, `expl_inbound_nonzero=3`, `expl_outbound_nonzero=3`) | passed (`inbound=1 outbound=1 zero_hop=false`, mixed B/C selectors: `inbound_first_remote_role=B outbound_first_remote_role=C`) | ran (`streaming_rc=0`, 4.30 s) | `P234-C-STREAMING-DIRECTION-A-ESTABLISHED` + `P235-JAVA-STREAMING-PASSED`; reverse direction stopped at `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` |
| 2 | `target/interop/m6-java-evidence/attempt-2/` | passed (same topology facts as attempt 1) | failed: helper_ready=false (stock-Java `I2PSession.connect()` handshake ceiling fired) | not reached | `P242-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=both helper_ready=false` |
| 3 | `target/interop/m6-java-evidence/attempt-3/` | passed (same topology facts as attempt 1) | passed (`inbound=1 outbound=1 zero_hop=false`, both via C: `inbound_first_remote_role=C outbound_first_remote_role=C` — explicit branch fired) | ran (`streaming_rc=0`) | `P234-C-STREAMING-DIRECTION-A-ESTABLISHED` + `P235-JAVA-STREAMING-PASSED`; reverse direction stopped at `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` |

Interpretation (Plan 243 §11): the deepest proven live terminal
governs the closure. Direction A (i2pr → Java Streaming) establishes
on 2/3 counted attempts with one earlier stock-build stop; the
explicit branch fires 1/3 (attempt 3), as expected for the source-
locked one-in-four `shouldSelectExplicit(settings)` semantics; and
the reverse direction (Java → i2pr) is bounded by the retained
Plan-236 response-emission observability gap. No production i2pr
corrective is authorized before the reverse direction reaches i2pr.

## Per-attempt evidence highlights

### Attempt 1 — Direction A established, mixed B/C selectors

The Plan-242 §5 bootstrap gate passed (`stock_candidate_population_nonempty=1`,
`expl_inbound_nonzero=3`, `expl_outbound_nonzero=3`). The Plan-242 §7
non-zero pair gate passed: 1+1 tunnels installed, zero-hop absent,
`inbound_tunnel_length_including_local=2`, `outbound_tunnel_length_including_local=2`,
`inbound_remote_hop_count=1`, `outbound_remote_hop_count=1`. The
helper selected B for the inbound tunnel and C for the outbound
tunnel (`inbound_first_remote_role=B outbound_first_remote_role=C`)
because the source-locked `shouldSelectExplicit` returned false on
this build (the stock fast-peer path picked B for inbound; the
explicit path picked C for outbound). The streaming driver ran:

- p234-syn-epoch: `syn_transport_request_emitted=true`,
  `java_accept_thread_started=true`, `java_accept_returned=true`,
  `i2pr_inbound_tunneldata_count=1`, `i2pr_expected_stream_tunneldata_count=1`,
  `i2pr_tunnel_recovery_count=1`, `i2pr_garlic_payload_count=1`,
  `i2pr_streaming_adapter_calls=1`,
  `i2pr_streaming_adapter_successes=1`,
  `i2pr_streaming_adapter_errors=0`, `connection_state=Established`.
- p234-classification: `P234-C-STREAMING-DIRECTION-A-ESTABLISHED`.
- p235-syn-epoch: `transport_request_accepted=1`,
  `outbound_dispatch_accepted=2`, `outbound_dispatch_rejections=0`,
  `inbound_tunneldata=1`, `expected_tunneldata=1`,
  `recovery=1`, `garlic_payload=1`, `adapter_successes=1`,
  `adapter_errors=0`, `connection_established=true`.
- p235-classification: `P235-JAVA-STREAMING-PASSED`.
- p236-classification: `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`
  (the reverse-direction boundary; the P236 stage did not observe
  the stock helper `Connection.sendPacket`/`PacketQueue.enqueue`/
  `I2PSession.sendMessage` path).
- p239-classification: `P239-F-DIRECTION-A-ESTABLISHED` (Direction A
  established in the OCMOSJ view too).
- p240-classification: `P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED`
  (the streaming-target-job trace was not observable because the
  reverse-direction path stopped at the Plan-236 gap).
- p241-classification: `P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT
  direction=inbound` (the retained Plan-241 classifier's exact-via-C
  gate does NOT pass on the mixed B/C selectors; the Plan-242 §7
  non-zero pair gate is the authoritative prerequisite).
- plan199-java-stop: `Plan 236 response epoch stopped at
  P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`.

The deepest proven live terminal for attempt 1 is
`P243-G-DIRECTION-A-ESTABLISHED`.

### Attempt 2 — stock-Java handshake timeout (no build attempt)

The Plan-242 §5 bootstrap gate passed (same topology facts as
attempt 1). The Plan-242 §7 non-zero pair gate did NOT run because
the helper's `I2PSession.connect()` exited with
`I2PSessionException: Cannot connect to the router on 127.0.0.1:36533
and build tunnels - No handshake received from the router` within
the stock-Java five-minute ceiling. The terminal
`P242-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=both
helper_ready=false` is the bounded failure token for this lane
(Plan 242 §7; mirrors the Plan 227 §8 five-minute ceiling).

This attempt does not consume Direction A; it is the Plan 243 §11
"earlier stock-build stop" case and a stock-Java transient, not an
i2pr defect.

### Attempt 3 — Direction A established, both via C

The Plan-242 §5 bootstrap gate passed. The Plan-242 §7 non-zero pair
gate passed: 1+1 tunnels installed, zero-hop absent, BOTH via C
(`inbound_first_remote_role=C outbound_first_remote_role=C`) because
the source-locked `shouldSelectExplicit` returned true on this build
(`random.nextInt(4) == 0`). The streaming driver ran:

- p234-syn-epoch: `i2pr_inbound_tunneldata_count=1`,
  `i2pr_expected_stream_tunneldata_count=1`,
  `i2pr_tunnel_recovery_count=1`, `i2pr_garlic_payload_count=1`,
  `i2pr_streaming_adapter_calls=1`,
  `i2pr_streaming_adapter_successes=1`,
  `i2pr_streaming_adapter_errors=0`, `connection_state=Established`.
- p234-classification: `P234-C-STREAMING-DIRECTION-A-ESTABLISHED`.
- p235-syn-epoch: `inbound_tunneldata=1`, `expected_tunneldata=1`,
  `recovery=1`, `garlic_payload=1`, `adapter_successes=1`,
  `adapter_errors=0`, `connection_established=true`.
- p235-classification: `P235-JAVA-STREAMING-PASSED`.
- p236-classification: `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`
  (same reverse-direction boundary as attempt 1).
- p239-classification: `P239-F-DIRECTION-A-ESTABLISHED`.
- p240-classification: `P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED`.
- p241-classification: `P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED`
  (Plan-241's old exact-via-C gate passes here because both are
  via C; the streaming-target-job trace is not observable because
  the reverse-direction path stops at the Plan-236 gap).
- plan199-java-stop: `Plan 236 response epoch stopped at
  P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`.

The deepest proven live terminal for attempt 3 is
`P243-G-DIRECTION-A-ESTABLISHED`.

## Source lock (Plan 243 §15)

```text
md5 a00d1f19a7b2a262fa9ab2c8dae7c75c  (Plan-242 TSV, 38 rows; byte-
identical across all three Plan-243 attempts on
f359baba57fc7d952ab7f5a5367d34671c590b18)
```

The 38-row Plan-242 TSV carries the 11 TunnelPeerSelector /
ClientPeerSelector needles plus the retained Plan-241
TunnelPoolManager/TunnelPool/IterativeSearchJob needles. The script
ran with no harness-side override of the source-locked one-in-four
explicit-peer semantics.

## Static checker (Plan 243 §12 / §14)

`scripts/check-m6-mixed-router-acceptance-evidence.sh` §32 enforces
the Plan-243 surface:

- 32a: Plan-243 host qualification script carries every bounded
  reason (`P243-H-HOST-NOT-QUALIFIED`,
  `java-runtime-missing`, `javac-missing`,
  `java-reference-cache-missing`, `i2pr-daemon-missing`,
  `source-lock-input-missing`, `port-preflight-failed`,
  `workspace-sha-mismatch`, `filesystem-preflight-failed`,
  `exit 70`, `9134f808337b401e8e53c73734c81fab04280c9d`,
  `JAVA_VERSION="2.13.0"`).
- 32b: Plan-243 host qualification script does NOT execute the
  streaming driver (`I2PR_M6_JAVA_DRIVER=streaming` literal absent).
- 32c: 13 Plan-243 §13 unit rows are present in the driver test
  (`p243_host_not_qualified_does_not_consume_attempt` ... through
  `p243_no_production_change`).
- 32d: Production Rust stays free of `P243-HOST` literal
  (`crates/i2pr-daemon/src`, `crates/i2pr-client/src`,
  `crates/i2pr-tunnel/src`, `crates/i2pr-runtime/src`).
- 32e: The implementation plan keeps every §5/§6/§9/§10/§13
  invariant text (defense in depth against silent drift).
- 32f: The Plan-243 closure record is present
  (`plans/closure/mixed-router-interop/243-status.md`).

Result: **passes** on `f359baba57fc7d952ab7f5a5367d34671c590b18`.

## Verification floor (Plan 243 §14)

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  -- p225_ p226_ p228_ p230_ p237_ p238_ p239_ p240_ p241_ p242_ p243_ \
  -- --test-threads=1   (160 passed, 1 ignored; +15 vs Plan 242)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo deny check advisories bans sources
bash -n scripts/interop/check-p243-host-qualified.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh \
  <exact-pinned-source> <sanitized-tsv>   (38 rows,
  md5 a00d1f19a7b2a262fa9ab2c8dae7c75c)
bash scripts/interop/check-p243-host-qualified.sh \
  --expected-sha f359baba57fc7d952ab7f5a5367d34671c590b18 \
  (P243-H-HOST-QUALIFIED reason=ok)
bash check-dependency-direction.sh
bash check-runtime-boundaries.sh
bash check-service-tunnel-boundaries.sh
bash check-fixture-manifest.sh
bash check-ntcp2-vectors.sh
bash check-ssu2-vectors.sh
bash check-i2cp-vectors.sh
bash check-ntcp2-interoperability.sh
bash check-constrained-host-lane-boundary.sh
bash check-sam-acceptance-evidence.sh
bash check-ssu2-acceptance-evidence.sh
bash check-i2cp-acceptance-evidence.sh
bash check-service-tunnel-acceptance-evidence.sh
bash check-exploratory-tunnel-evidence.sh
bash check-netdb-tunnel-evidence.sh
bash check-destination-tunnel-evidence.sh
bash check-streaming-tunnel-evidence.sh
bash check-m6-mixed-router-acceptance-evidence.sh
```

The focused P243 unit floor (15 rows) is green; the retained
P225/P226/P228/P230/P237/P238/P239/P240/P241/P242 unit floors
(145 rows) are green; the source-lock checker is byte-identical
across all three counted attempts; the host qualification gate is
green on the implementation SHA.

## Acceptance (Plan 243 §15)

1. Capable execution host positively qualified before the attempt
   budget: **YES** (`host_qualified=true` on
   `f359baba57fc7d952ab7f5a5367d34671c590b18`).
2. Exact Plan-242 harness semantics unchanged: **YES** (Plan-242 §31
   source/static/unit floors green; no harness / helper / launcher /
   production Rust change).
3. Plan-242 source-lock / static / unit floors green: **YES**
   (38-row TSV md5
   `a00d1f19a7b2a262fa9ab2c8dae7c75c` byte-identical across
   attempts).
4. Counted attempts use one committed implementation SHA with no
   tuning: **YES** (single `f359baba57fc7d952ab7f5a5367d34671c590b18`,
   no between-attempt tuning, fresh A/B/C RouterContexts, unique
   evidence dirs).
5. Three counted attempts execute on fresh A/B/C RouterContexts: **YES**
   (attempts 1, 2, 3 with separate `target/interop/m6-java-evidence/attempt-{1,2,3}/`
   dirs and fresh RouterContexts each).
6. Every attempt emits exactly one ordered Plan-243 terminal: **YES**
   (attempts 1/3 emit `P243-G-DIRECTION-A-ESTABLISHED`; attempt 2
   emits `P242-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=both
   helper_ready=false`).
7. Exact-via-C and explicit-branch occurrence remain diagnostic
   only: **YES** (attempt 1 = mixed B/C selectors, attempt 3 = both
   via C; the Plan-242 §7 non-zero pair gate is the authoritative
   prerequisite; `contains_c` and `exact_one_remote_hop_via_c` are
   diagnostic only).
8. If a non-zero pair builds, the live Streaming driver runs: **YES**
   (attempts 1 and 3 reached the streaming driver; `streaming_rc=0`
   on both; `streaming_through_java ... ok` from `cargo test`).
9. If B is queried, reply / subDB stages are evaluated in order:
   **YES** (the streaming driver ran the retained P240 / P241 / P234
   / P235 / P236 / P237 / P238 / P239 chain on attempts 1 and 3).
10. If lookup succeeds, OCMOSJ / downstream attribution continues:
    **YES** (`p239-classification P239-F-DIRECTION-A-ESTABLISHED`
    on attempts 1 and 3; reverse-direction OCMOSJ stages observed at
    Router-A).
11. No i2pr-owned defect claimed before expected TunnelData: **YES**
    (Direction A establishes on attempts 1 and 3; the reverse
    direction stops at the Plan-236 response-emission gap, NOT at an
    i2pr-owned stage; no i2pr corrective is authorized).
12. Plan 201 and Plan 204 updated together at closure: **YES** (see
    Disposition and unblock audit below).
13. Plan-242 status documentation normalized to name the
    Plan-242 implementation-and-closure authority: **YES** (the
    Plan-242 closure record is the Plan-242 source authority; Plan
    243 names `f359baba57fc7d952ab7f5a5367d34671c590b18` as the
    Plan-243 implementation-and-closure SHA, distinct from the
    Plan-242 SHA `9b0e61c6c3f780a19e843e63d25804fb8756616a`).

## Implementation surfaces

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs   (15 new p243_ unit rows; no production change)
scripts/check-m6-mixed-router-acceptance-evidence.sh   (§32 host-script + unit-row + production-surface + closure-record guards)
scripts/interop/check-p243-host-qualified.sh   (Plan-243 §4 host qualification gate)
plans/closure/mixed-router-interop/243-status.md   (this file)
plans/registry.md   (Plan-243 row + Plan-201 / Plan-204 blocker updates)
plans/subsystems/mixed-router-interop-roadmap.md   (§7 Plan-243 row + §11 / §12 disposition)
plans/closure/mixed-router-interop/201-status.md   (Plan-201 amendment: now blocked on Java Streaming reverse direction + publication closure)
plans/closure/service-tunnels/204-status.md   (Plan-204 amendment: blocked on Java second-family closure pending Plan-243 reverse direction)
```

No `src/` file changed. The M6 checker §32 production-surface guard
(`P243-HOST` absent from `i2pr-daemon/src`, `i2pr-client/src`,
`i2pr-tunnel/src`, `i2pr-runtime/src`) is green. The §32 forbidden
list is unchanged (Plan 242 §4 + §14). Frozen windows, topology,
profile policy, publication target, and pins are unchanged.

## Limitations and findings

- **Severity medium (boundary, not defect): Direction A established
  but the streaming reverse direction (Java → i2pr) is bounded by the
  retained Plan-236 response-emission observability gap.** Direction A
  reaches i2pr's expected TunnelData path (`inbound_tunneldata=1`,
  `expected_tunneldata=1`, `recovery=1`, `garlic_payload=1`,
  `adapter_successes=1`) on attempts 1 and 3. The reverse direction
  stops at the P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
  boundary (the stock helper / router observation of the response
  scheduler, packet construction, `PacketQueue.enqueue`,
  `I2PSession.sendMessage`, Router-A I2CP admission, OCMOSJ dispatch,
  target IBGW, transit, and i2pr inbound TunnelData chain). No
  i2pr-owned defect is attributed to the Plan-243 boundary.
- **Severity low (sample variance):** attempt 2's helper
  handshake timeout is a stock-Java `I2PSession.connect()` ceiling
  transient, not an i2pr defect. Attempts 1 and 3 both build a
  genuine non-zero pair, which proves the corrected Plan-242 §7
  gate is host-deterministic at 2/3. The Plan-243 §11 "earlier
  stock-build stop" arm is invoked once.
- **Severity low (selector variance):** attempt 1 selects B for
  inbound and C for outbound; attempt 3 selects C for both. This is
  the source-locked one-in-four `shouldSelectExplicit(settings)`
  stochasticity: 1/3 attempts fire the explicit branch (attempt 3),
  2/3 fall through to the stock fast-peer path (attempt 1 inbound,
  attempt 2 helper timeout). `contains_c` and
  `exact_one_remote_hop_via_c` are diagnostic only; the Plan-242 §7
  non-zero pair gate is the authoritative prerequisite.
- **Severity low (interpretation note):** `p241-classification` is
  the retained Plan-241 classifier's exact-via-C gate. Attempt 1's
  mixed B/C selectors do NOT pass the Plan-241 gate (terminal
  `P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT direction=inbound`) but
  the Plan-242 §7 non-zero pair gate IS authoritative and passes.
  Attempt 3's both-via-C selectors pass the Plan-241 gate too
  (`p241-classification P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED`).
  No re-interpretation of Plan-241 history is required.
- **Severity low (measurement note):** the Plan-240 single-job
  observability rule applies to the lookup epoch only; the Plan-242
  §6 extended snapshot observes both directions' installed pools
  independent of any ISJ job count. Attempt 1 and 3 carry identical
  topology facts (loopback-only, three controlled routers, A-only
  small-router exploratory profile, B publication target).
- Live §§8–10 lookup driver evidence is unit-locked and statically
  guarded; attempt 1 and 3 exercised the full driver and stopped at
  the Plan-236 reverse-direction boundary. The first post-Plan-243
  attempt will exercise it unchanged.
- Direction A established (the deepest proven live terminal) does
  not close M6; §10 continuation and §11 closure do not trigger.
  No M6 Java-family closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan243
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan243-reverse-direction-and-publication-closure
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-pending-m6-java-streaming-reverse-direction-and-publication-corrective
```

Plan 201 consumes Plan 243 as the next hard dependency on the
streaming axis: Direction A (i2pr → Java Streaming) is now proven
live on a hosted execution host. The Plan-201 streaming axis still
needs the streaming reverse direction (Java → i2pr response path)
plus the bidirectional Streaming qualification + the
publication / final-closure axis. The successor plan must own the
narrow Plan-236-boundary continuation (the Java → i2pr response
path, which currently stops at the P236 response-emission gap) and
must NOT authorize any i2pr production corrective before exact
expected TunnelData reaches i2pr for the reverse direction.

Plan 204 consumes Plan 243 as the next hard dependency on the
Java-second-family closure: M10 product authority through Plans
213–215 is unchanged. Plan 204 stays blocked on M6 Java
second-family closure (now pending the streaming reverse direction
+ publication / final-closure axis; no double unblock).

Plan 205 stays retained / deferred (the direct i2cp requalification
lane is intentionally out of scope for Plan 243).

No other registered plan listed Plan 243 as a hard dependency, so
nothing else changes state. No successor is pre-registered here;
the streaming reverse direction + publication closure corrective
requires its own plan-of-record under the same subsystem, registered
only from the exact final boundary above.

## Follow-up boundary

A successor owns exactly one §16 arm from the exact final boundary:

- Direction A established → **streaming reverse direction + publication
  closure corrective** (the Plan-236 response-emission gap must be
  re-observed on a subsequent host with the Plan-243 gate held; no
  i2pr corrective is authorized before the reverse direction's
  expected TunnelData reaches i2pr);
- lookup chooses zero-hop despite an authoritative non-zero-only pool
  → selector contradiction attribution (not observed here; the §7
  gate held);
- B query dispatches but reply chain fails → exact lookup-reply
  corrective (not observed here; the reply chain is bounded by the
  P236 reverse-direction gap, not a Java-side lookup failure);
- expected TunnelData reaches i2pr then fails → production i2pr
  corrective may be authorized for that exact owned stage (NOT
  observed here; the reverse direction does not reach i2pr).

The successor must retain the Plan-242 §7 corrected gate, the
Plan-242 §6 extended pool observation, the Plan-242 source-lock,
the frozen topology / profile / timing, and the fail-closed lanes,
and stop at the first proven stage. It must NOT pre-authorize a
topology correction, a publication corrective, a tunnel-policy
change, a selector-bias correction, an RNG override, or a
production i2pr change unless exact expected TunnelData proves an
i2pr-owned defect.
