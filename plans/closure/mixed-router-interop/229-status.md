# Plan 229 status — M6 Java non-zero exploratory paired-tunnel bootstrap corrective

Status: **`passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary`**.

Plan of record:
[`229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective.md`](../../implementation/mixed-router-interop/229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective.md).

## Closure result

Plan 229 is closed as a completed, fail-closed reference-topology
corrective with an exact stop terminal. Both authorized topology
corrections were applied and proven live on every counted attempt:

```text
A role=service floodfill=true + stock small-router exploratory 1/1/1/1
B role=publication floodfill=true + ordinary exploratory settings
C role=transit floodfill=false + ordinary exploratory settings
```

but the run stops at the WP C transit-peer gate on all four counted
attempts with the single authoritative terminal:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE
```

Authoritative per-attempt facts (fresh disposable A/B/C RouterContexts,
baseline loopback topology, destination-only lane,
`I2PR_M6_JAVA_DRIVER=destination`):

```text
attempt=1 sha=00dc368 router_c_hex=2e19669c0fbf3187eb38c67330b1a01d03beb91535f94de4584fca6ab54fb5bf
  roles=A-service/B-publication/C-transit floodfill=true/true/false
  settings: inbound_length=1 inbound_variance=1 outbound_length=1 outbound_variance=1
  main_raw_present=true main_valid_present=true profile_present=false
  selectable=true banlisted=false unreachable=false caps_has_f=false
  profile_count=0 not_failing_count=0 helper_started=false
  terminal=P229-C-NOT-EXPLORATORY-ELIGIBLE

attempt=2 sha=10d1015 router_c_hex=c81d5d1dabde601414233a19159c56a48528b8d79a6d4533e8b5412171082c38
  roles + settings identical to attempt 1 (live re-proven)
  main_raw_present=true main_valid_present=true profile_present=false
  selectable=true banlisted=false unreachable=false caps_has_f=false
  profile_count=0 not_failing_count=0 helper_started=false
  transit-gate 60 s bounded wait exhausted without a profile
  terminal=P229-C-NOT-EXPLORATORY-ELIGIBLE

attempt=3 sha=10d1015 router_c_hex=d48fb54f82fceb63fcb34b60ebddb78c09c8e935831aceb1866c04e26bae8368
  identical gate facts to attempt 2 (profile_count=0, 60 s wait exhausted)
  terminal=P229-C-NOT-EXPLORATORY-ELIGIBLE

attempt=4 sha=10d1015 router_c_hex=c8ee26ed0f3594cb0572332ac750b75d37060ee1749bf866b4de46e9103ac784
  identical gate facts to attempt 2 (profile_count=0, 60 s wait exhausted)
  terminal=P229-C-NOT-EXPLORATORY-ELIGIBLE
```

Attempt 1 ran on `00dc368` (1 of that SHA's 3-attempt budget); attempts
2–4 ran on `10d1015` (that SHA's full 3-attempt budget) after a
plan-authorized refinement (bounded 60 s transit-gate wait for in-flight
DatabaseStore processing, no profile injection). No between-attempt
tuning; no second correction inside any counted SHA.

Because the transit-peer gate never passes, the raw helper never starts,
no exploratory install snapshot is taken, the Plan-228 attribution never
runs, and the target lookup / reverse-delivery lane is never entered.
Each attempt emits exactly one `p229-classification` row and zero
`p228-classification` rows. No M6 Java interoperability is claimed.

## Exact-pinned mechanism (why `selectable=true` with `profile=false`)

Bytecode review of the exact-pinned `router.jar`
(`ProfileOrganizer.isSelectable` / `selectAllPeers`) shows the two
probes measure different populations by construction:

- `isSelectable(Hash)` (the retained Plan-227/228 signal) checks
  banlist membership, main-NetDB presence, the hidden flag, and
  `TunnelPeerSelector.shouldExclude` — then returns true. It never
  consults the tier maps.
- `selectAllPeers()` (the Plan-229 `profile_present` / `profile_count`
  signal) returns the union of the `_fastPeers`, `_highCapacityPeers`,
  and `_notFailingPeers` tier maps.

On all four attempts `profile_count=0` and `not_failing_count=0`: Router
A holds zero tier-map profiles at all (not merely a missing Router-C
entry), while the ordinary authenticated DatabaseStore bootstrap passes
(`P200-H-publication-path-passed` on every attempt) and C is present,
valid, selectable, non-banned, non-failing, and non-floodfill. The
existing wire bootstrap therefore populates NetDB entries without
populating the organizer tier population that pinned
`ExploratoryPeerSelector` (via `selectNotFailingPeers`) selects from.
This is the precise upstream-of-i2pr precondition failure Plan 229 §3.4
anticipated, now proven rather than hypothesized.

## Implementation commits and pinned inputs

Implementation was committed before the counted attempts (Plan 229 §16):

```text
00dc368 interop: implement Plan 229 non-zero exploratory paired-tunnel bootstrap corrective
10d1015 interop: Plan 229 bounded transit-gate wait for in-flight bootstrap processing
```

Full SHAs:

```text
00dc36806f63f52169d1a6948334712c6bbd2771
10d101593c61b50034d2f5e1bd397abb19f739dd
```

Reference inputs remained frozen: Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; the i2pd reference pin
remained `2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`. No
dependency, fixture, production protocol, or Java reference source
changed. No Java source patch, reflection, profile/tier mutation,
client-NetDB RI store, direct tunnel install, exploratory/client tunnel
setting change beyond the authorized A-only small-router profile,
paired-tunnel policy override, VMComm, `netDb.alwaysQuery`, public I2P,
distinct topology, build/timeout change, streaming-helper change, or
reverse-window change.

Test-only deltas (no production Rust code changed):

```text
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P229Probe.java (new)
tests/integration/m6-interop/java/ControlledRouter.java (role arg, A-only small exploratory profile, P229 commands)
tests/integration/m6-interop/run-java.sh (roles, proofs, gates, bounded polls, P229 driver, lookup-lane disable)
crates/i2pr-daemon/tests/java_tunnel_external.rs (P229 gates/classifier/driver, 25 unit rows)
scripts/check-m6-mixed-router-acceptance-evidence.sh (Plan 229 §22 invariants)
```

## Requirement-to-evidence matrix

| Plan-229 requirement | Evidence / result |
|---|---|
| Reference pins unchanged (§18.1) | Java `9134f80…` on all 4 attempts; i2pd pin retained by checker; static guard pins both. |
| No production Rust changes (§18.2) | `git diff --stat` touches only the five test-only files above; dependency/runtime boundary scripts pass. |
| Counted roles explicit, C proven non-floodfill (§18.3) | `p229-roles` rows + live `router.config` enforcement (A/B `floodfillParticipant=true`, C `false`) + `p229-role-proof role_ok=1 caps_has_f=false` on all attempts. |
| A effective settings match small-router profile (§18.4) | `p229-exploratory-settings observable=true 1/1/1/1` (quantities 2/2 diagnostic) on all attempts; B/C proven A-only by config-file absence. |
| Ordinary wire bootstrap is the only profile path (§18.5) | `P200-H-publication-path-passed` retained on all attempts; no `addProfile`/`getOrCreateProfile`/store/publish call exists in probe/launcher (static guard + javac). |
| C proven present/profiled/selectable/reachable/non-floodfill (§18.6) | Present/valid/selectable/non-banned/reachable/non-floodfill proven; `profile_present=false` with `profile_count=0` is the exact stop (see terminal row). |
| Non-zero inbound exploratory installed (§18.7) | Gate not entered: helper never started, no install snapshot taken. Honestly unproven, never inferred. |
| Non-zero outbound exploratory installed (§18.8) | Same as §18.7. |
| Plan-227 helper profile unchanged (§18.9) | `ReferenceRawDestination.java` untouched; helper never started so no profile ran; static guard retains 1/1/false/explicit-C requirements. |
| NO-PAIRED disappears or is classified contradiction (§18.10) | Not reached: P228 attribution never ran (zero `p228-classification` rows); no contradiction invented. |
| Paired selection proven when reached (§18.11) | Not reached; helper never started. |
| Build stages recorded independently when reached (§18.12) | Not reached; none inferred. |
| Exactly one terminal per run (§18.13) | Exactly one `p229-classification` row per attempt (verified by count on all 4 evidence TSVs). |
| No lookup/reverse-delivery claim (§18.14) | Lookup driver disabled by default-0 gate (`P229_LOOKUP_DRIVER_ENABLED:-0`); `p229-lookup-lane executed=false` on all attempts; driver never touches the frozen 45 s window. |
| Raw logs scratch-only (§18.15) | Static guard rejects `log-router` promotion on the P229 path; secret-bearing rows rejected by unit-locked parsers. |
| Focused/routine verification (§18.16) | See Verification below. |
| Closure/registry/roadmap/unblock audit (§18.17) | This record plus registry/roadmap/201/204/230 updates in the same commit. |

## External attempt history

Each attempt used fresh disposable Java RouterContexts, the baseline
loopback topology, A=service, B=publication, C=transit, exact A
small-router exploratory settings, the existing authenticated RouterInfo
bootstrap, unchanged pins, unchanged timeouts, and no tuning between
retries, per §16.

| Attempt | Implementation SHA | P229 result |
|---|---|---|
| 1 | `00dc368` | `P229-C-NOT-EXPLORATORY-ELIGIBLE` profile=false (single snapshot) |
| 2 | `10d1015` | `P229-C-NOT-EXPLORATORY-ELIGIBLE` profile=false profile_count=0 (60 s wait exhausted) |
| 3 | `10d1015` | `P229-C-NOT-EXPLORATORY-ELIGIBLE` profile=false profile_count=0 (60 s wait exhausted) |
| 4 | `10d1015` | `P229-C-NOT-EXPLORATORY-ELIGIBLE` profile=false profile_count=0 (60 s wait exhausted) |

The enclosing legacy Plan-199 Java wrapper exited nonzero on all attempts
because install-dependent destination/streaming rows remain unqualified.
The Plan-229 diagnostic rows (`external-p229-classification`,
`external-p229-roles`, `external-p229-exploratory-settings`,
`external-p229-transit-peer`, retained P227/P228 supporting rows,
`workspace-gates`) passed; the install/lookup/streaming rows stay
blocked with stop provenance; raw Java logs remained scratch-only.

## Verification

Successful verification on final SHA `10d1015` (local truth, not CI).
The full workspace floor ran on `00dc368` (2590 passed) with only the
bounded transit-gate wait plus checker row afterwards, re-verified by
the focused rows below plus clippy/checker/javac; the counted attempts
above ran on the final SHA:

```text
cargo fmt --all --check                                      PASS
cargo check --locked --workspace --all-targets                PASS
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                                 PASS on 00dc368 (2590 passed, 18 ignored)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                                 PASS (final SHA)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                                 PASS (final SHA)
cargo test --locked --workspace --doc                         PASS (0 doc tests, final SHA)
cargo deny check advisories bans sources                     PASS (final SHA)
bash scripts/check-dependency-direction.sh                   PASS (final SHA)
bash scripts/check-runtime-boundaries.sh                     PASS (final SHA)
bash scripts/check-service-tunnel-boundaries.sh              PASS (final SHA)
bash scripts/check-fixture-manifest.sh                       PASS (final SHA)
bash scripts/check-ntcp2-vectors.sh                          PASS (final SHA)
bash scripts/check-ssu2-vectors.sh                           PASS (final SHA)
bash scripts/check-i2cp-vectors.sh                           PASS (final SHA)
bash scripts/check-ntcp2-interoperability.sh                 PASS (final SHA)
bash scripts/check-constrained-host-lane-boundary.sh         PASS (final SHA)
bash scripts/check-sam-acceptance-evidence.sh                PASS (final SHA)
bash scripts/check-ssu2-acceptance-evidence.sh               PASS (final SHA)
bash scripts/check-i2cp-acceptance-evidence.sh               PASS (final SHA)
bash scripts/check-service-tunnel-acceptance-evidence.sh     PASS (final SHA)
bash scripts/check-exploratory-tunnel-evidence.sh            PASS (final SHA)
bash scripts/check-netdb-tunnel-evidence.sh                  PASS (final SHA)
bash scripts/check-destination-tunnel-evidence.sh            PASS (final SHA)
bash scripts/check-streaming-tunnel-evidence.sh              PASS (final SHA)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh    PASS (§22 invariants, final SHA)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
                                                                 PASS (18 tests, final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
                                                                 PASS (25 passed, final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
                                                                 PASS (21 passed, final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
                                                                 PASS (14 passed, final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
                                                                 PASS (final SHA)
bash -n tests/integration/m6-interop/run-java.sh             PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh PASS
javac (all probes + launcher + helpers vs staged jars)       PASS (final SHA)
```

Focused Plan-229 unit rows (25, all green locally):

```text
p229_unknown_launcher_role_fails_closed
p229_service_role_keeps_floodfill_true
p229_publication_role_keeps_floodfill_true
p229_transit_role_sets_floodfill_false
p229_only_service_role_receives_small_exploratory_profile
p229_no_explicit_peers_on_exploratory_settings
p229_no_exploratory_quantity_backup_timeout_override
p229_c_advertising_f_maps_to_role_mismatch
p229_c_no_profile_maps_to_not_eligible
p229_c_profile_but_not_selectable_maps_to_not_eligible
p229_zero_hop_only_exploratory_is_insufficient
p229_inbound_only_nonzero_is_insufficient
p229_outbound_only_nonzero_is_insufficient
p229_both_nonzero_admits_helper_start
p229_nonzero_plus_no_paired_maps_to_contradiction
p229_paired_without_create_maps_to_create_boundary
p229_create_without_dispatch_maps_to_dispatch_boundary
p229_dispatch_without_c_receive_maps_to_first_hop_boundary
p229_c_reject_retained_with_bounded_code
p229_reply_decrypt_join_failures_retain_earliest_ordering
p229_both_client_tunnels_install_maps_only_to_built
p229_no_lookup_or_reverse_send_after_terminal
p229_exactly_one_terminal_per_attempt
p229_unrelated_logs_cannot_satisfy_facts
p229_secret_raw_log_lines_rejected
```

## Security, compatibility, and operational decisions

- No production crate or protocol behavior changed; all deltas are in
  the external Java driver (`P229Probe.java`, `ControlledRouter.java`
  read-only commands + startup properties), the `run-java.sh` harness
  gates, the `java_tunnel_external.rs` gates/classifier/unit rows, and
  the static evidence checker.
- The `unreachable` probe fact is read through the public
  `ProfileOrganizer.isFailing(Hash)` facade accessor.
  `PeerProfile.wasUnreachable()` is package-private and unavailable to
  the out-of-tree probe; the mapping is documented in the probe header
  and the fail-closed direction is preserved (a failing flag stops the
  run). No reflection was used to reach the private method.
- No Java source patching, reflection, private-state mutation, NetDB
  key/tunnel injection, publication retry, paired-tunnel policy
  override, timeout change (helper five-minute ceiling and 45-second
  payload window frozen), or `netDb.alwaysQuery` override was used.
- All SSU2 hosts remain `127.0.0.1` baseline; the distinct topology path
  stays rejected by construction for counted runs.
- Durable evidence contains only bounded booleans, counts, direction,
  hashes, response/status codes, and elapsed facts. Raw Java logs, peer
  path lists, keys, tags, SessionConfig contents, request records, and
  payloads remain scratch-only or are redacted; the parsers reject
  secret-bearing lines before they can satisfy any fact.
- Java and i2pd pins, SAM/I2CP/diagnostic loopback policy, and frozen
  45-second reverse-payload authority remain unchanged.
- A transit-peer profile is never considered proven from `isSelectable`
  presence alone; only tier-map membership plus a non-failing,
  non-banned, reachable, non-floodfill record qualifies (unit-locked).
  Installed tunnels are proven only from installed-pool snapshots, never
  from logs. The counted lookup driver is disabled by a default-0 gate
  so Plan 229 cannot enter the successor's qualification lane.

## Findings and limitations

No security finding was introduced. The plan proved its corrective
applied (roles + A-only small-router profile live on all attempts) and
localized the next precondition failure exactly: stock Java's ordinary
authenticated RouterInfo DatabaseStore bootstrap does not populate
Router A's profile-organizer tier population (`profile_count=0` after a
bounded 60 s in-flight wait on three of four attempts), so Router C —
though present, valid, selectable, non-banned, reachable, and
non-floodfill — is not exploratory-eligible under the pinned
`ExploratoryPeerSelector` → `selectNotFailingPeers` path. Four
consecutive fresh-context attempts across two implementation SHAs each
stopped at the same earliest-stage gate without starting the helper,
dispatching a build, or entering the lookup lane.

Severity: no critical/high findings. Medium: the controlled topology's
profile population path is unproven — owned by the Plan 230 successor,
not by profile manufacturing (which stays forbidden). Low: the WP D
exploratory poll and WP E build-path continuation never executed, so
their shell/Rust paths are exercised only by the 25 unit rows and the
`--no-run` compilation — the successor's counted run is their first
live execution.

## Roadmap and unblock audit

Plan 229 is formally closed at the bounded transit-peer stop. The
unblock audit examined every registered plan listing Plan 229 as a
dependency:

```text
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-profile-population-path-attribution
plan_201 = blocked-pending-plan230-profile-population-path-attribution-after-plan229-not-eligible
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
active_plan = none
next_executable_plan = 230-m6-java-profile-population-path-attribution
```

- Plan 201 remains blocked: Plan 229 applied its topology correction
  but produced a stop, not installed tunnels or second-family closure.
  Its blocker is narrowed from generic Plan-229 pendency to the
  specific profile-population corrective Plan 230 owns.
- Plan 204 remains blocked on that same independent M6 closure; its M10
  authority is unchanged. Plan 229 is reference-topology-only and does
  not alter M10 authority.
- Plan 205 remains retained/deferred: the observed boundary is below
  the SAM bridge (at profile population, below exploratory
  establishment and below client-tunnel paired selection).
- Exactly one successor is registered here, tied to the exact stop
  stage per Plan 229 §19: Plan 230 owns the profile-population-path
  attribution (how ordinary stock-Java activity populates the organizer
  tier population for controlled peers, without profile injection).

## Registration basis (retained)

Plan 228 closed with:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both
```

on all four counted attempts at final implementation SHAs
`8f2167d` / `8990849`.

Retained facts:

- Router C is present and valid in Router A's main NetDB;
- `ProfileOrganizer.isSelectable(C)=true`;
- the Plan-227 raw helper requests one-hop inbound/outbound client
  tunnels;
- both client creator configs contain Router C;
- `BuildExecutor` reaches the client build path;
- neither direction obtains a paired tunnel;
- only zero-hop exploratory tunnels are installed;
- no client build message is dispatched;
- Router C therefore never receives a client tunnel build request.

Exact-pinned Java source shows the client builder requires non-zero
paired infrastructure while the exploratory pools sit on zero-hop
fallbacks; Java ships a stock one-hop small-router exploratory profile;
and the launcher incorrectly made Router C floodfill despite its
documented transit role. Plan 229 restored the roles, applied the
small-router profile to A, and re-proved the bootstrap — stopping at
the newly visible profile-population precondition rather than the
paired-tunnel boundary.

## Registration constraints (retained)

Plan 229 did not authorize:

- i2pr production changes;
- Java source patches;
- profile/tier manipulation;
- NetDB mutation;
- direct tunnel installation;
- exploratory/client tunnel setting changes beyond the A-only
  small-router profile;
- paired-tunnel policy overrides;
- VMComm;
- `netDb.alwaysQuery`;
- timeout increases;
- topology changes;
- Streaming-helper execution or changes;
- target lookup / reverse-delivery qualification.

Closure replaced the registration token with the single Plan-229
`P229-C-NOT-EXPLORATORY-ELIGIBLE` terminal above and updated the
dependency/unblock audit.
