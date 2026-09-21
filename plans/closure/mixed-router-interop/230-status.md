# Plan 230 status — M6 Java reachability-capability/profile-bootstrap corrective and continuation

Status: **`passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary`**.

Plan of record:
[`230-m6-java-profile-population-path-attribution.md`](../../implementation/mixed-router-interop/230-m6-java-profile-population-path-attribution.md).

The filename is retained to preserve the registered global Plan-230 path; the
plan body was replaced in place by the narrower reachability/capability
corrective before execution, and this record closes that corrective.

## Closure result

Plan 230 is closed as a completed, fail-closed reference-topology
corrective with an exact downstream boundary. Every Plan-230-owned gate
passed:

```text
WP A  observation contract (capability/predicate/self-view, no isFailing)
WP B  baseline predicate outcome recorded before any correction
WP C  conditional stock controlled-topology correction, exactly as authorized
WP D  natural profile bootstrap through the ordinary authenticated RI path
WP E  genuine non-zero exploratory + one-hop client continuation gates
```

The WP F frozen-destination re-entry ran once end-to-end (counted attempt
1): one-hop client tunnels through C installed in both directions, the
target Standard LS2 resolved through the real tunnel NetDB path, the
forward i2pr→Java payload delivered digest-matched — and the reverse
Java→i2pr payload never arrived inside the frozen 45-second window. The
reverse stop reproduces the exact retained Plan-218 behavioral signature
(send admitted by Java, no payload at i2pr, no terminal status), now with
every upstream Java-internal prerequisite proven rather than hypothesized.
No new i2pr-visible wire/protocol boundary was observed, so no new
protocol corrective is registered here; the reverse-delivery lane stays
owned by Plan 201.

Authoritative per-attempt facts (fresh disposable A/B/C RouterContexts,
`I2PR_M6_JAVA_DRIVER=destination`, exact pins throughout):

```text
baseline  sha=09a2f6e router_c_hex=169c348a88d4019d1365469cbcb46b9ebe9a98d05df97172817d6110fb8cf4a3
  Plan-229 topology, no reachability/bandwidth overrides
  caps r=false u=false f=false l=true e=false g=false tier=L
  local ff=true share=46694 comm=UNKNOWN probe_eligible=false computed=0
  self-view C sha=ca03dec61d1590626f7bb3d7adbe96c8dfdde786f08bf35c04934219e20c4b2d (matches observed)
  terminal=P230-A-PREDICATE-INELIGIBLE reason=compound
  (missing-r + low-bandwidth-l-on-floodfill-observer; authorizes C1+C2)

attempt=1 sha=3ef5bac router_c_hex=a38ea65d1fe08d3549f4acd3a74d681182669053268e16ff1e77498fdf0eeb9f
  corrected topology (C1+C2)
  baseline=P230-A-PREDICATE-ELIGIBLE (r=true l=false tier=N local comm=OK)
  profile=P230-D-PROFILE-BOOTSTRAP-PASSED (present=true count=1 not_failing=1
    selectable banlisted=false caps_f=false c-sha=self-sha=3d6dcad45e1269adfb8716b6215ed7b35984ed494edb71674e23e3b860f9b8e6)
  E=P230-E-TUNNEL-CONTINUATION-PASSED
  client tunnels 1/1 exact-one-hop-via-C both directions, zero-hop absent
  lookup lane executed (p230-f-tunnel-continuation)
  outbound+inbound installed (1/1); lease-lookup-completed (leases=1)
  destination-outbound-delivered cells=1 payload_len=27
  reference-received payload_len=27 match=true digest=ec61e08da98b76b02ee2268d544b90da0c3b7cace6339d627cd3d40b85048ceb
  reverse: destination-inbound-send-failed send_status=public-send-accepted
    ordered_statuses=[1] (ACCEPTED, no NO_LEASESET, no further status)
    reverse_i2pr_payload_recovered_45s=false
  stop=P230 reverse-delivery boundary (retained Plan-218 signature)

attempt=2 sha=3ef5bac router_c_hex=a5b9e551ec6c52017ccad9b9e0ca158fbf9917afb839acd2de1693e24ebd9a56
  baseline=P230-A-PREDICATE-ELIGIBLE
  profile=P230-D-PROFILE-BOOTSTRAP-PASSED (present=true count=2 not_failing=2
    c-sha=self-sha=0267003019a05c439ab23615ecece243614cc13644c5e9b8a7b7e9fcf8680767)
  exploratory 5/5 non-zero both directions, C present both directions
  client inbound exact-via-C=true, outbound exact-via-C=false
  terminal=P230-E-CLIENT-NOT-BUILT direction=outbound (lookup lane not executed)

attempt=3 sha=3ef5bac VOID (infra)
  Router A died before SAM listen (exit 2, no Java log emitted);
  no Plan-230 gate reached, no terminal emitted, no tuning performed.

attempt=3b sha=3ef5bac router_c_hex=4dabc333cb78381a2de9ecbdfa2850910840a1a044a7a22a3f0b2c17938a85ba
  baseline=P230-A-PREDICATE-ELIGIBLE (3/3 eligible on this SHA)
  D poll (6x5 s) exhausted with profile_present=false count=1 not_failing=1
    c-sha=self-sha=01e103d7bed67e7ae06438f1f4a86c1d2aaa6d8d2dd812cc28ae7b2d1317873e
  terminal=P230-D-ELIGIBLE-BUT-NO-PROFILE (lookup lane not executed)
```

Attempt discipline (§11): the baseline ran on the observation-contract SHA
as WP-B evidence (not a closure run). The WP-C correction committed as the
second implementation SHA (`3ef5bac`); attempts 1, 2, 3b are that SHA's
three counted attempts (attempt 3 void on infra, retried without any code
change). No between-attempt tuning; fresh RouterContexts after the
configuration change; no second correction inside any counted SHA.

The harness evidence directory retains only the latest run's TSVs
(attempt-3b state); the per-attempt facts above were transcribed from live
observation at each attempt's aggregation epoch (the aggregation consumes
the LAST `p230-classification`, so each attempt's terminal below is the
final P230 word of its own run). This matches the Plan-229 recording
practice for a single-evidence-dir lane.

## Implementation commits and pinned inputs

Implementation was committed before the counted attempts (Plan 230 §11):

```text
09a2f6e interop: implement Plan 230 reachability-capability/profile-bootstrap observation contract
3ef5bac interop: apply Plan 230 conditional stock controlled-topology correction
```

Full SHAs:

```text
09a2f6ed56364044140a8c31b20232e764479f32
3ef5bac9972a96618e5c496909058009b57e82d4
```

Reference inputs remained frozen: Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; the i2pd reference pin
remained `2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`. No
dependency, fixture, production protocol, or Java reference source
changed. No Java source patch, reflection, profile/tier mutation,
`heardAbout()` probe call, client-NetDB RI store, direct tunnel install,
exploratory/client tunnel setting change beyond the authorized C1/C2
stock properties, paired-tunnel policy override, VMComm,
`netDb.alwaysQuery`, public I2P, distinct topology, build/timeout change,
streaming-helper change, or reverse-window change.

Test-only deltas (no production Rust code changed):

```text
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P230Probe.java (new)
tests/integration/m6-interop/java/ControlledRouter.java (P230-CAPABILITY/P230-SELF-VIEW commands + WP-C props)
tests/integration/m6-interop/run-java.sh (WP-B/D/E/F gates, P230 aggregation)
crates/i2pr-daemon/tests/java_tunnel_external.rs (predicate/parser/classifier/terminals, 21 unit rows)
scripts/check-m6-mixed-router-acceptance-evidence.sh (Plan 230 §23 invariants)
```

## Requirement-to-evidence matrix

| Plan-230 requirement | Evidence / result |
|---|---|
| Reference pins unchanged (§4) | Java `9134f80…` on all attempts; i2pd pin retained by checker; static guard pins both. |
| No production Rust changes (§4) | `git diff` touches only the five test-only files above; dependency/runtime boundary scripts pass. |
| `isFailing` retired as reachability authority (§4.11/§5) | P230Probe never calls it; parser + harness + checker reject any P230 row carrying `unreachable`/`isFailing` (unit-locked). |
| Capability bits + comm status recorded (§5) | `p230-capability` rows carry `caps_has_r/u/f/l/e/g`, tier, RI-SHA, counts, `local_floodfill_enabled`, `local_max_share_bandwidth`, `local_comm_status` on every attempt. |
| Exact predicate reconstructable (§5) | Probe-derived `heard_about_creation_eligible` must agree with the exact recomputation or the run maps to `OBSERVABILITY-GAP` (shell + Rust + unit-locked); checker pins the predicate shape. |
| Baseline before correction (§6) | Baseline attempt on `09a2f6e` recorded `P230-A-PREDICATE-INELIGIBLE reason=compound` with Plan-229 topology unchanged. |
| `i2np.udp.status=ok` only if proven (§7 C1) | Baseline proved missing-`R` (`comm=UNKNOWN` both routers); C1 applied to all controlled roles; loopback-only + reseed-disabled proven by retained topology invariants. |
| C bandwidth only if `L` blocks (§7 C2) | Baseline proved `low-bandwidth-l-on-floodfill-observer`; transit-C-only `128/128` stock properties; forced-class override never used (checker-forbidden + unit-locked). |
| Post-correction RI via normal bootstrap (§8) | All three counted attempts show C self-RI SHA == A-observed C-RI SHA (`3d6dca…`, `026700…`, `01e103…` respectively). |
| Predicate true + natural membership (§8) | Attempts 1–2: eligible + `selectAllPeers`/`getProfileNonblocking` membership + `not_failing_count > 0` + selectable/non-banned/non-floodfill; no probe creation call exists (static guard + javac). |
| Non-zero exploratory both directions (§9) | Attempt 1 (P229 gate passed) and attempt 2 (5/5 nonzero, C present both directions). |
| One-hop client tunnels through C both directions (§9) | Attempt 1: 1/1 exact-one-hop-via-C, zero-hop absent (shell + driver epochs). |
| P228 boundary disposition (§9) | Attempt 1 entered F with paired infrastructure live; the retained `NO-PAIRED` attribution never fired on the corrected topology. |
| Frozen destination lane re-entered (§10) | Attempt 1: 6/7 WP-F items (topology, tunnels, answerable LS2 on B, client session, nonce-tracked send ACCEPTED, ordinary lookup path with `search_success=true` + client-tunnel receipt); reverse payload missing at 45 s. |
| No bootstrap successor ( §10/§15) | E/F ran inside this plan; no new plan registered to repeat a gate. |
| Exactly one terminal per run (§11/§15) | Exactly one `p230-classification` per counted run (last-occurrence aggregation; verified per attempt). |
| Raw logs scratch-only (§4) | Static guard rejects `log-router` promotion on the P230 path; parsers reject secret-bearing lines before they can satisfy any fact. |
| Focused/routine verification (§14) | See Verification below. |
| Closure/registry/roadmap/unblock audit (§15/§17) | This record plus registry/roadmap/201/204 updates in the same commit. |

## External attempt history

| Attempt | Implementation SHA | P230 result |
|---|---|---|
| baseline | `09a2f6e` | `P230-A-PREDICATE-INELIGIBLE reason=compound` (evidence for C1+C2, not a closure run) |
| 1 | `3ef5bac` | `P230-E-TUNNEL-CONTINUATION-PASSED` → WP-F reverse-delivery boundary (forward digest-matched, reverse admitted-but-unarrived) |
| 2 | `3ef5bac` | `P230-E-CLIENT-NOT-BUILT direction=outbound` (D passed with count=2; inbound client tunnel built, outbound not within run) |
| 3 | `3ef5bac` | VOID — Router A died before SAM listen (exit 2); no gate reached |
| 3b | `3ef5bac` | `P230-D-ELIGIBLE-BUT-NO-PROFILE` (eligible 3/3 on this SHA; 30 s D poll exhausted with count=1, C absent) |

The enclosing legacy Plan-199 Java wrapper exited nonzero on attempts 1,
2, 3b because install/lookup/streaming rows remain unqualified beyond the
forward path. The Plan-230 diagnostic rows
(`external-p230-classification`, `external-p230-capability`,
`external-p230-self-view-c`, `external-p230-baseline`,
`external-p230-profile`, `workspace-gates`) passed; raw Java logs remained
scratch-only.

## Verification

Successful verification on closing SHA `3ef5bac` (local truth, not CI).
The full workspace floor ran on `09a2f6e` (2611 passed, 18 ignored);
`3ef5bac` differs from it only by the Java test-resource launcher (no
Rust production or test code), re-verified below by the focused rows plus
the full static floor:

```text
cargo fmt --all --check                                      PASS (closing SHA)
cargo check --locked --workspace --all-targets                PASS (closing SHA)
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                                  PASS on 09a2f6e (2611 passed, 18 ignored)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                                  PASS (closing SHA)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                                  PASS (closing SHA)
cargo test --locked --workspace --doc                         PASS (0 doc tests, closing SHA)
cargo deny check advisories bans sources                     PASS (closing SHA)
bash scripts/check-dependency-direction.sh                   PASS (closing SHA)
bash scripts/check-runtime-boundaries.sh                     PASS (closing SHA)
bash scripts/check-service-tunnel-boundaries.sh              PASS (closing SHA)
bash scripts/check-fixture-manifest.sh                       PASS (closing SHA)
bash scripts/check-ntcp2-vectors.sh                          PASS (closing SHA)
bash scripts/check-ssu2-vectors.sh                           PASS (closing SHA)
bash scripts/check-i2cp-vectors.sh                           PASS (closing SHA)
bash scripts/check-ntcp2-interoperability.sh                 PASS (closing SHA)
bash scripts/check-constrained-host-lane-boundary.sh         PASS (closing SHA)
bash scripts/check-sam-acceptance-evidence.sh                PASS (closing SHA)
bash scripts/check-ssu2-acceptance-evidence.sh               PASS (closing SHA)
bash scripts/check-i2cp-acceptance-evidence.sh               PASS (closing SHA)
bash scripts/check-service-tunnel-acceptance-evidence.sh     PASS (closing SHA)
bash scripts/check-exploratory-tunnel-evidence.sh            PASS (closing SHA)
bash scripts/check-netdb-tunnel-evidence.sh                  PASS (closing SHA)
bash scripts/check-destination-tunnel-evidence.sh            PASS (closing SHA)
bash scripts/check-streaming-tunnel-evidence.sh              PASS (closing SHA)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh    PASS (§23 invariants, closing SHA)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
                                                                  PASS (18 tests, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
                                                                  PASS (21 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
                                                                  PASS (25 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
                                                                  PASS (21 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
                                                                  PASS (14 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
                                                                  PASS (closing SHA)
bash -n tests/integration/m6-interop/run-java.sh             PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh PASS
javac (all probes + launcher + helpers vs staged jars)       PASS (closing SHA)
```

Focused Plan-230 unit rows (21, all green on closing SHA):

```text
p230_isfailing_never_authoritative_for_reachability
p230_selectable_alone_does_not_imply_creation_eligible
p230_missing_r_is_ineligible
p230_u_without_r_is_ineligible
p230_nonff_l_peer_on_floodfill_observer_is_ineligible
p230_nonff_r_non_l_non_e_non_g_is_eligible
p230_floodfill_peer_with_r_is_eligible
p230_e_is_ineligible_for_nonff_peer
p230_g_is_ineligible_for_nonff_peer
p230_baseline_ineligible_authorizes_only_matching_fixture_correction
p230_reachability_override_is_loopback_fixture_only
p230_bandwidth_correction_applies_to_transit_c_only
p230_force_bandwidth_class_is_forbidden
p230_stale_pre_correction_ri_cannot_pass_post_correction_gate
p230_eligible_without_profile_maps_to_d_boundary
p230_profile_gate_requires_natural_organizer_membership
p230_no_probe_profile_creation_calls
p230_profile_pass_continues_into_exploratory_gate
p230_tunnel_pass_continues_into_frozen_destination_lane
p230_exactly_one_earliest_terminal
p230_secret_or_unrelated_rows_rejected
```

## Security, compatibility, and operational decisions

- No production crate or protocol behavior changed; all deltas are in
  the external Java driver (`P230Probe.java`, `ControlledRouter.java`
  read-only commands + startup properties), the `run-java.sh` harness
  gates, the `java_tunnel_external.rs` gates/classifier/unit rows, and
  the static evidence checker.
- The P230 probe reads through public accessors only
  (`lookupLocallyWithoutValidation`, `lookupRouterInfoLocally`,
  `selectAllPeers` + `getProfileNonblocking`, `isSelectable`,
  `countNotFailingPeers`, `isBanlisted`, `floodfillEnabled`,
  `getMaxShareBandwidth`, `commSystem().getStatus`,
  `RouterInfo.getCapabilities()/getBandwidthTier()`). No reflection,
  no private-field access, no `addProfile`/`getOrCreateProfile*`/
  `heardAbout` call from any probe path (checker-enforced, including
  substring guards that also cover comments).
- No Java source patching, NetDB key/tunnel injection, publication
  retry, paired-tunnel policy override, timeout change (helper
  five-minute ceiling, 45-second payload window, and 30-second D poll
  all frozen), or `netDb.alwaysQuery` override was used.
- The `i2np.udp.status=ok` override is fixture-only: all SSU2 hosts
  remain `127.0.0.1` baseline (retained topology invariants), reseed
  stays disabled, VMComm stays absent. The transit bandwidth change
  uses real configured stock bandwidth, never the forced-class lie.
- Durable evidence contains only bounded booleans, counts, tier/status
  tokens, hex hashes, and RI SHA-256 identities. Raw Java logs, peer
  path lists, keys, tags, SessionConfig contents, request records, and
  payloads remain scratch-only or are redacted; the parsers reject
  secret-bearing lines (including bare-token shape violations) before
  they can satisfy any fact.
- Java and i2pd pins, SAM/I2CP/diagnostic loopback policy, and frozen
  45-second reverse-payload authority remain unchanged.
- A transit-peer profile is never considered proven from `isSelectable`
  presence alone; only tier-map membership plus a non-failing,
  non-banned, eligible, non-floodfill record with a fresh RI identity
  qualifies (unit-locked). Installed tunnels are proven only from
  installed-pool snapshots, never from logs.

## Findings and limitations

No security finding was introduced. Severity: no critical/high findings.

Medium: the corrected topology's downstream Java timing is stochastic
across fresh contexts — attempt 2 built only the inbound client tunnel
(`P230-E-CLIENT-NOT-BUILT direction=outbound`) and attempt 3b exhausted
the frozen 30-second D poll with C still outside the organizer
(`P230-D-ELIGIBLE-BUT-NO-PROFILE`, count=1). Both are earliest-stage
stops under the frozen bounds, not contradictions: eligibility itself
was 3/3 deterministic, bootstrap 2/3 within the poll, and the deepest
run (attempt 1) passed every gate through the forward delivery. No
timing inflation is authorized to chase them; a future reverse-delivery
corrective re-runs the same frozen gates.

Medium: the WP-F reverse lane stops at the retained Plan-218 signature
on the corrected topology — Java admits the reverse raw-Destination
send (`ACCEPTED`, `ordered_statuses=[1]`, no `NO_LEASESET`), but no
payload reaches i2pr in 45 s and no terminal status follows. Because no
i2pr-visible wire event occurs at all, this is a Java-side dispatch
boundary below i2pr's observation surface, not a new i2pr protocol
defect. Owned by Plan 201; a narrow reverse-delivery corrective may be
registered separately.

Low: the P229 WP-E driver (run after helper teardown on attempt 1)
emitted `P229-OBSERVABILITY-GAP` because post-teardown pools are empty;
the P230-E terminal is authoritative for the E stage on the corrected
topology, so no contradiction exists.

Low: the harness evidence directory retains only the latest run's TSVs
(attempt-3b state at closure); earlier attempts' facts are transcribed
in this record at their aggregation epochs, as in Plan 229.

## Roadmap and unblock audit

Plan 230 is formally closed at the bounded reverse-delivery stop. The
unblock audit examined every registered plan listing Plan 230 as a
dependency:

```text
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_201 = blocked-pending-reverse-delivery-corrective-after-plan230-profile-bootstrap
plan_204 = blocked-on-m6-java-second-family-closure-pending-reverse-delivery-corrective
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
active_plan = none
next_executable_plan = none (a narrow reverse-delivery corrective may be registered separately)
```

- Plan 201 remains blocked: its Plan-230 hard dependency is closed
  (predicate proven, correction applied, natural bootstrap proven,
  forward path fully passing), but Java second-family closure still
  requires the reverse-delivery boundary, which reproduces retained
  Plan 218 and has no registered corrective. Its blocker is narrowed
  from generic Plan-230 pendency to that exact boundary. Re-running
  201's lane without a corrective would only reproduce the stop, so no
  successor is registered here.
- Plan 204 remains blocked on that same independent M6 closure; its M10
  authority is unchanged. Plan 230 is reference-topology-only and does
  not alter M10 authority.
- Plan 205 remains retained/deferred: the observed boundary is below
  the SAM bridge (at Java-side reverse dispatch, below exploratory
  establishment and below client-tunnel paired selection).
- No successor is registered here: the next step is a narrow
  reverse-delivery corrective plan-of-record, owned by the Plan-201
  destination lane when its trigger (new Java-side dispatch evidence
  or a bounded harness observation) is identified.

## Registration basis (retained)

Plan 229 closed with:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE
profile_present=false
profile_count=0
not_failing_count=0
```

on all four counted attempts (SHAs `00dc368` / `10d1015`).

Exact-pinned Java I2P 2.13.0 source review after that closure changed the
interpretation of the stop:

1. `P229Probe` used `ProfileOrganizer.isFailing(C)` as its
   `unreachable` signal, but the exact-pinned method is deprecated and
   unconditionally returns `false`. Historical `unreachable=false` is therefore
   not authoritative reachability evidence.
2. `ProfileManagerImpl.heardAbout(peer, caps)` is the main profile-creation
   vector and creates a new profile only when its private
   `shouldCreate(caps)` predicate accepts the peer's RouterInfo.
3. That predicate requires `R` and, for a non-floodfill peer observed by the
   floodfill service router, rejects the normal low-bandwidth `L` class as well
   as `E`/`G`.
4. Plan 229 observed `f` but did not record `R/L/E/G/U` or reconstruct the
   creation predicate.
5. The controlled router uses Java's default 60 KiB/s outbound bandwidth unless
   explicitly configured; with the default 80% share the pinned
   `Router.getBandwidthClass()` normally places it at the `L` boundary.
6. The isolated loopback fixture also cannot depend on public-network peer
   testing to derive reachability. Pinned `UDPTransport` supports the stock
   `i2np.udp.status=ok` override, and an OK communication-system status causes
   `Router.getCapabilities()` to advertise `R`.

The open question was therefore not "how many minutes until a profile
appears?" but whether the exact RouterInfo is eligible for ordinary stock
profile creation at all. This record answers it: it was not (compound
missing-`R` + `L` blocker), the two matching stock corrections repaired
exactly that, and ordinary creation proceeded naturally.

## Current authority (superseded by closure above)

```text
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-reachability-capability-profile-bootstrap-corrective

plan_201 = blocked-pending-plan230-reachability-capability-profile-bootstrap-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 230-m6-java-reachability-capability-profile-bootstrap-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## Authorized execution (executed; retained for traceability)

Plan 230 first added read-only capability/predicate evidence and performed
one baseline run with the Plan-229 topology unchanged.

Only baseline evidence authorized these stock controlled-fixture changes:

- `i2np.udp.status=ok` for A/B/C when missing/incorrect reachability capability
  is proven;
- `i2np.bandwidth.outboundKBytesPerSecond=128` and
  `i2np.bandwidth.outboundBurstKBytesPerSecond=128` for transit Router C only
  when C's `L` class is the exact creation-predicate blocker.

`router.forceBandwidthClass` remained forbidden.

After a newly published corrected C RouterInfo traversed the ordinary
authenticated DatabaseStore bootstrap and C entered A's organizer population
naturally, this same plan continued directly through the existing non-zero
exploratory, client-tunnel, and frozen Plan-201 destination qualification gates.

## Prohibited shortcuts (all honored; retained for traceability)

Plan 230 did not authorize:

- production Rust changes merely to satisfy the Java fixture;
- Java source patches;
- direct profile creation, tier promotion, score mutation, or profile files;
- probe-side `heardAbout()` calls;
- direct Java NetDB store/publish;
- direct tunnel installation;
- VMComm, public I2P, or reseed;
- `netDb.alwaysQuery`;
- Plan-226 distinct-/24 topology;
- SAM pivot;
- timeout inflation;
- client-NetDB RI injection;
- using historical P229 `isFailing()` evidence as a reachability gate.

Closure replaced the registration token with the Plan-230
`passed-...-with-reverse-delivery-boundary` terminal above and updated the
Plan-201/204 unblock audit.
