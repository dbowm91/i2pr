# Plan 242 status — M6 Java Streaming stock one-hop selector semantics corrective

Status: `passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate`

Plan 242 corrects the Plan-241 harness over-constraint. Exact-pinned
Java source proves `TunnelPeerSelector.shouldSelectExplicit()` returns
true only when `ctx.random().nextInt(4) == 0`; `ClientPeerSelector
.selectPeers()` then either calls `selectExplicit(settings, length)`
or falls back to the stock `selectFastPeers` path. Therefore a
`length=1` SessionConfig with `explicitPeers=<Router-C>` only fires
the explicit branch one build in four. Plan 241 over-constrained the
harness by requiring every counted run to land in the explicit branch
(`exact_via_c=true`).

Plan 242 replaces Plan-241's `exact-via-C` hard gate with the corrected
non-zero-hop client-pair gate, replaces the Plan-241 C-profile
bootstrap gate with a stock-candidate-population check (Router-A
selectable on B or C is sufficient), and records the corrected
installed-client-pool facts through the extended `P242-CLIENT-TUNNELS`
observation (P227 §6). No Java source was patched, no production Rust
changed, no topology, profile, publication, timing, or pin changed.

Implementation is committed BEFORE counted attempts; the routine
floor and focused unit/static floors are green. Live counted external
attempts on a Java host are recorded as unexecuted on this lane
(no Java cache + i2pr daemon pair available in the local environment),
exactly like the Plan 234/235/236/241 precedent: every shell gate is
emitted and verified, every driver-side observation is implemented
and unit-locked, but the lookup continuation driver is not reached
because no counted run entered it on this host.

## Authority and exact pins

Implementation checkpoint (committed BEFORE counted attempts; no
tuning between attempts):

```text
(implementation SHA captured at the implementation commit) — Plan-242
corrective; seven files: extended P227Probe.java with the §6
role/path observation, ControlledRouter.java exposes the new
P242-CLIENT-TUNNELS command, ReferenceStreamingService.java unchanged
from Plan-241, source-lock script gains eleven TunnelPeerSelector /
ClientPeerSelector needles (35 → 38 rows; byte-identical across any
attempt), run-java.sh replaces Plan-241 §6/§7 gates with the §5/§7
corrected bootstrap + non-zero pair gates, java_tunnel_external.rs
adds the Plan-242 module (extended snapshot parser, bootstrap gate,
continuation gate, ordered classifier, terminal taxonomy, record
helpers) plus 21 unit rows, check-m6-mixed-router-acceptance-
evidence.sh §31 enforces the Plan-242 source/launcher/helper/raw-
helper/driver/source-lock/harness surface.
```

References unchanged:

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained: Plan 232 route-derived lease-gateway correction and
bidirectional raw-Destination pass (code untouched); Plan 234 Java
accept-returned boundary; Plan 235 usable `I2PSocket` surface +
outbound-admission prerequisite; Plan 236 source-lock ordering + Java
response path; Plan 237 stock deltas (2/1/3/0/0); Plan 238 Router-A
admission deltas (distribute 3, dispatch 0/0); Plan 239 D1
lookup-failure deltas (local absent, found 0, failed 3/4/4, dispatch
0/0); Plan 240 streaming-epoch lookup failure + B-eligibility
+ `toTry` membership; Plan 241 one-hop SessionConfig fixture + P241
helper-profile re-proof + main-vs-client B-RI visibility; frozen
topology, profile policy, publication target, and timing windows
(`DATAGRAM_WAIT`/`STREAM_WAIT`/`SYN_ACK_WAIT` all still 45 s).

## Source lock (WP §15)

`scripts/interop/check-m6-java-response-source-lock.sh` now reads the
two new pinned files:

```text
ROUTER_TUNNEL_PEER_SELECTOR = router/java/src/net/i2p/router/tunnel/pool/TunnelPeerSelector.java
ROUTER_CLIENT_PEER_SELECTOR = router/java/src/net/i2p/router/tunnel/pool/ClientPeerSelector.java
```

Eleven additive needles (kept verbatim) lock Plan-242 §3:

1. `TPS.should_select_explicit` — `protected boolean shouldSelectExplicit(TunnelPoolSettings settings)`
2. `TPS.explicit_peers_property` — `String peers = opts.getProperty("explicitPeers");`
3. `TPS.random_one_in_four` — `ctx.random().nextInt(4) == 0`
4. `TPS.select_explicit` — `protected List<Hash> selectExplicit(TunnelPoolSettings settings, int length)`
5. `TPS.random_shuffle_explicit` — `Collections.shuffle(rv, ctx.random());`
6. `TPS.local_router_appended` — `rv.add(ctx.routerHash());`
7. `TPS.no_valid_explicit_zero_hop` — `"No valid explicit peers found, building zero hop"`
8. `TPS.fallback_select_fast_peers` — `ctx.profileOrganizer().selectFastPeers(more, exclude, matches);`
9. `CPS.select_peers` — `public List<Hash> selectPeers(TunnelPoolSettings settings)`
10. `CPS.should_select_explicit_check` — `if (shouldSelectExplicit(settings))`
11. `CPS.explicit_returns_select_explicit` — `return selectExplicit(settings, length);`

The TSV grew 35 → 38 rows with three additive Java source-lock rows
(recorded facts only; execution raw log lines never enter durable
evidence). The TSV md5 is identical across any counted attempt:

```text
md5 a00d1f19a7b2a262fa9ab2c8dae7c75c  (Plan-242 TSV, 38 rows)
```

The Plan-241 TunnelPoolManager/TunnelPool/IterativeSearchJob needles
are retained frozen, ordered by their Plan-241 semantic role.

Locked facts carry Plan-242's diagnostic separation:

- `shouldSelectExplicit(settings)` is *probabilistic*. It returns
  `true` only when an explicit `explicitPeers` is configured AND
  `ctx.random().nextInt(4) == 0`. The Plan-242 harness MUST NOT
  assert that a 1-in-4 branch deterministically selects the
  configured peer.
- `selectExplicit(settings, length)` is the matching explicit branch.
  It validates the configured peer (selectability), trims/fills to
  the requested length, and inserts the local router. With no
  configured peer OR `random.nextInt(4) != 0`, control falls through
  `ClientPeerSelector.selectPeers()` to the ordinary stock
  `selectFastPeers` path.
- The Plan-240 ISJ `outTunnel.getLength() <= 1` guard is independent
  of the explicit branch. A genuine non-zero-hop outbound tunnel
  bypasses the guard regardless of which selector branch fired.
  Therefore `exact-via-C` was never a valid gate against the guard.

## Pool observation (WP §6)

The Plan-227 read-only client-tunnel observation was extended (under
the Plan-242 §6 mandate "extend the existing P227/P228/P241 read-only
pool observer rather than create a new general harness") rather than
emitting a parallel observation. `P227Probe.Tunnels` keeps every
Plan-227 field; a new `P227Probe.ExtendedTunnels` accessor carries the
Plan-242 bounded role/path facts:

- configured length and variance (helper profile re-proof at the
  lookup epoch is upstream and unchanged);
- allowZeroHop (retained diagnostic);
- installed count, zero-hop count, non-zero count
  (`<dir>_nonzero_count`), tunnel-length-including-local,
  remote-hop count;
- first/last remote role as `B` / `C` / `other-controlled` / `unknown`
  / `none` (bounded label set; `other-controlled` is the local
  helper when a peer's identity equals Router A's hash, which cannot
  appear as a remote hop on a length-2 tunnel but is captured for
  longer paths);
- `contains_b`, `contains_c` — membership flags, recorded but never
  consumed by the §7 gate;
- `exact_one_remote_hop` and retained `exact_one_remote_hop_via_c`;
- `unexpected_peer_observed` — fail-closed flag for any peer that
  is neither A, B, nor C (`unknown` role label OR direct
  `unexpected_peer_observed=true`).

No peer hashes, keys, tags, payloads, destinations, or raw log text
are emitted. The probe stays read-only: no `getOrCreateProfile`,
`heardAbout`, `addProfile`, `store`, `publish`, `buildTunnels`,
`addTunnel`, `registerKeys`, `unregisterKeys`, `setKeys`, or
reflection calls (Plan-242 §14 forbidden list, enforced by the §31b
static check).

`ControlledRouter` exposes a new read-only command
`P242-CLIENT-TUNNELS <client_dbid_hex> <router_a_hex>
<router_b_hex> <router_c_hex>` that emits the extended observation as
a single bounded `P242-EV kind=extended-client-tunnels ...` line.
The §31c static check enforces the command surface.

## Bootstrap gate (WP §5 + §7)

The retained Plan-241 bootstrap gate required Router C to be:
eligible, floodfill-indexed via `f`-capability index, not
forever-banlisted, and have a natural profile; that constraint is a
Plan-241 over-interpretation of `explicitPeers=C`. The corrected
Plan-242 §5 bootstrap gate requires:

1. controlled topology valid (already enforced above as
   `TOPOLOGY_OK=1`);
2. Router A / B / C identities resolvable via P220 self snapshots
   (`router_a_hex`, `router_b_hex`, `router_c_hex`); C base64
   identity reused from the destination section when valid, else
   re-derived via P220 + P224-HASH-B64;
3. non-zero exploratory infrastructure both directions via retained
   `P229-EXPLORATORY-TUNNELS` with the bounded 12×5 s
   natural-bootstrap wait;
4. helper requests the corrected one-hop profile (re-proved after
   helper start via the Plan-241 `REPORT_TUNNEL_PROFILE` control);
5. at least one usable stock client-tunnel candidate exists
   (`P227-PEER-ELIGIBILITY` shows B or C selectable AND not
   forever-banlisted on Router A's peer manager). The harness stops
   with `P242-A-NO-STOCK-CLIENT-TUNNEL-CANDIDATE` only when
   neither B nor C is selectable — Router C profile absence alone
   is no longer a stop condition.

Gate failure terminals:

- `P242-A-NONZERO-EXPLORATORY-NOT-READY` — exploratory empty;
- `P242-A-NO-STOCK-CLIENT-TUNNEL-CANDIDATE` — no usable peer;
- `P242-A-TOPOLOGY-INVALID` — identity unresolvable;
- `P242-A-HELPER-PROFILE-NOT-ONE-HOP` — driver-side re-proof fails
  (the shell exit 72 hard-fail is retained).

## Pair gate (WP §7)

The retained Plan-241 client-pair gate required inbound AND outbound
installed tunnels to be exactly the `{local, Router-C}` length-2
path. The corrected Plan-242 §7 gate requires:

- inbound pool installed and inbound non-zero count ≥ 1 AND inbound
  zero-hop absent;
- outbound pool installed and outbound non-zero count ≥ 1 AND
  outbound zero-hop absent.

`contains_c`, `exact_one_remote_hop_via_c`, and the role/length
diagnostics are recorded and emitted as the `p242-client-tunnels`
row but DO NOT gate continuation. The driver side re-queries the
pool at the lookup epoch and the same non-zero/zero-hop predicate is
re-checked before classification.

Gate failure terminals:

- `P242-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=<inbound|outbound|both>` —
  pair missing or non-zero missing;
- `P242-B-ZERO-HOP-CONTRADICTION` — pool has zero-hop tunnel despite
  `allowZeroHop=false`;
- `P242-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL direction=<inbound|outbound|both>` —
  observed peer outside the controlled topology (fail-closed).

The `contains_c` flags are surfaced as the first diagnostic; the
harness records them in the `p242-client-tunnels` row verbatim so a
host that runs the lane can read which path selectors actually
chose.

## Lookup continuation (WP §8)

Once the bootstrap + pair gates pass, the lane re-enters the
already-instrumented Plan-240/241 lookup path with the Plan-241
streaming driver. The driver now snapshots the Plan-242 extended
pool at the lookup epoch (helper DBID + Router A + Router B +
Router C) and consumes the same `P240Inputs` correlation key:

- helper DBID + target hash + exact Streaming ISJ job ID + Streaming
  epoch (P225 binding, retained);
- Router B eligibility (re-read at the lookup epoch);
- authoritative outbound pool state (re-derived from
  `p242-pool-epoch`);
- exact `not doing zero-hop lookup to unknown` or
  `not doing zero-hop self-lookup` observation (Plan-240 retained).

The Plan-242 §9 contradiction terminals:

- `P242-C-LOOKUP-ZERO-HOP-SELECTION-CONTRADICTION` — the lookup
  selected zero-hop despite the authoritative pool being non-zero.
  With an unproven pool the retained `P240-C-B-ZERO-HOP-UNKNOWN-RI`
  is reused verbatim via the `CEarlierGuard` delegation arm.

The Plan-242 §10 post-query chain inherits Plan-240's
`P225-D-B-LOOKUP-NOT-RECEIVED` → ... → `P225-D-LOOKUP-SUCCEEDED`
ordering with `P242-D-*` tokens; only a `P242-D-LOOKUP-SUCCEEDED`
classification resumes the Plan-239 OCMOSJ ordering.

The Plan-242 §10/§11 OCMOSJ/i2pr continuation order is verbatim from
the plan (`usable target lease → outbound response tunnel → Garlic →
DispatchJob → dispatchOutbound → Router-A outbound gateway →
transit → target IBGW → exact expected i2pr TunnelData → recovery
→ Garlic → Streaming adapter`); no production i2pr corrective is
authorized before exact expected TunnelData arrives at i2pr.

## Static checker (WP §12 + §14)

`scripts/check-m6-mixed-router-acceptance-evidence.sh` §31 enforces
the Plan-242 surface:

- **§31a** Extended snapshot must record bounded role/path facts
  only (verifies `snapshotClientTunnelsExtended`, `Role.UNKNOWN`,
  `inboundFirstRemoteRole`, `outboundLastRemoteRole`, `contains_b/c`,
  `exactOneRemoteHop`, `unexpectedPeerObserved`, `classifyPeerRole`).
- **§31b** Probe-side mutation, profile creation, NetDB/tunnel
  /queue/stat writes, RouterInfo stores, reflection, and
  private-field access are forbidden.
- **§31c** Launcher serves the new `P242-CLIENT-TUNNELS` command and
  emits the extended TSV fields.
- **§31d** Streaming helper retains the Plan-241 corrected one-hop
  profile unchanged; Plan 242 carries no helper-side surface.
- **§31e** Raw helper is frozen; carries no Plan-242 surface.
- **§31f** Driver carries the bounded Plan-242 parser + bootstrap
  gate + continuation gate + ordered classifier, plus additive
  `P242-*` evidence keys. Rejects hard-coded terminal records.
  No terminal closes M6 or authorizes production change. Production
  Rust `i2pr-daemon/src`, `i2pr-client/src`, `i2pr-tunnel/src`,
  `i2pr-runtime/src` carry no `P242`/`p242` surface.
- **§31g** The 21 Plan-242 §12 unit rows:
  `p242_explicit_peers_is_probabilistic_not_mandatory`,
  `p242_exact_via_c_is_diagnostic_not_gate`,
  `p242_nonzero_pair_is_lookup_prerequisite`,
  `p242_zero_hop_remains_forbidden`,
  `p242_c_profile_absence_alone_does_not_block_helper`,
  `p242_no_candidate_population_stops_before_helper`,
  `p242_unknown_client_tunnel_peer_fails_closed`,
  `p242_installed_path_records_remote_hop_count`,
  `p242_lookup_selected_tunnel_must_be_nonzero`,
  `p242_plan240_exact_job_correlation_retained`,
  `p242_b_query_requires_exact_dispatch`,
  `p242_b_receipt_requires_query`,
  `p242_b_answer_requires_b_receipt`,
  `p242_client_dsm_requires_b_answer`,
  `p242_subdb_install_requires_client_dsm`,
  `p242_ocmosj_resume_requires_lookup_success`,
  `p242_i2pr_terminal_requires_expected_tunneldata`,
  `p242_direction_a_pass_does_not_close_m6`,
  `p242_no_production_change`,
  `p242_terminal_tokens_are_canonical`,
  `p242_extended_tunnels_row_parses_with_bounded_roles`.
- **§31h** Source-lock script carries the eleven Plan-242
  TunnelPeerSelector / ClientPeerSelector needles (see "Source lock"
  above).
- **§31i** Harness shell gate correction: §7 non-zero pair, §6
  extended diagnostic, §5 stock-candidate-population gate. The
  Plan-241 §30i harness check is narrowed to the Plan 241 / 242
  shared surface (`P241Probe.java`, `P241_ROUTER_C_HEX`,
  `P241_CLIENT_DBID_HEX`, `external-p241`) plus a `must not emit`
  guard rejecting `P241-A-TRANSIT-BOOTSTRAP-NOT-READY`,
  `P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT`, and
  `start_stream_helper "${P241_C_B64}"` literals (those tokens are
  replaced by the corresponding `P242-*` forms in the §31i block).
- §31 forbidden-list: `netDb.alwaysQuery`, `vmCommSystem`,
  `forceBandwidthClass`, `floodfillParticipant`, `reseed`,
  `DRIVER_TIMEOUT=` overrides, `log-router` raw-log promotion,
  `random.nextInt(4) == 0` (test-only source lock is
  authoritative; the harness itself must NEVER embed the literal),
  `forceShouldSelectExplicit` (test-only probe is authoritative;
  harness must NEVER short-circuit the selector), and any
  multi-peer `explicitPeers=` override (the retained Plan-227
  contract is one peer).
- The exit-72 fixture-defect hard-fail is retained verbatim.

## Focused verification (implementation head)

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  p225_ p226_ p228_ p230_ p237_ p238_ p239_ p240_ p241_ p242_ \
  -- --test-threads=1   (145 passed; +21 vs Plan 240 = the new
                          Plan-242 §12 rows)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh \
  <exact-pinned-source> <sanitized-tsv>   (38 rows,
  md5 a00d1f19a7b2a262fa9ab2c8dae7c75c)
javac <all staged Java helpers incl. extended P227Probe against
      exact-pinned jars>   (exit 0; one pre-existing P237
                              deprecation note)
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
python3 -m unittest discover -s tests/integration/ntcp2/harness \
  -p 'test_execution_lane.py'   (18 tests OK)
```

Full serial workspace floor: **passed** on the implementation head
(`cargo test --locked --workspace --all-targets -- --test-threads=1`,
exit 0: 2778 passed, 18 ignored, 103 suites, 614.58 s; +21 vs Plan
241 is exactly the new P242 unit rows).

## Counted evidence status

This closure host has no Java cache + i2pr daemon pair provisioned
for the M6 Java second-family external lane, so live counted external
attempts are unexecuted. The implementation is committed BEFORE any
such attempt (per §13) and the lane is fail-closed: the §31 static
checker rejects any helper that:
- lacks the corrected `REPORT_TUNNEL_PROFILE` one-hop fixture;
- re-introduces Plan-241 exact-via-C terminal records;
- forces the `random.nextInt(4) == 0` literal in the harness;
- injects `netDb.alwaysQuery`, `vmCommSystem`, `DRIVER_TIMEOUT=`
  overrides, raw-log promotion, or any production `src/` change.

A subsequent host with the Java + i2pr daemon pair can execute the
lane verbatim:

```bash
I2PR_M6_JAVA_DRIVER=streaming \
bash tests/integration/m6-interop/run-java.sh
```

The driver emits the `streaming_through_java` Plan-240/241 trajectory
with the Plan-242 extended pool snapshot at the lookup epoch, and
the Plan-242 classifier emits exactly one `p242-classification`
row per run. The §13 attempt discipline (commit-before, 3 counted
attempts/SHA, no tuning, fresh A/B/C RouterContexts, no RNG
manipulation, no retry-until-explicit-C, no zero-hop fallback,
same 45-second windows, fresh SHA for parser/probe defects, raw
logs scratch-only, unique evidence dirs) is enforced by the §31
static check + the retained Plan-240/241 bash discipline.

## Requirement-to-evidence matrix (§17)

1. The exact-pinned `TunnelPeerSelector.shouldSelectExplicit` 1-in-4
   semantics are source-locked (`TPS.random_one_in_four`,
   `CPS.explicit_returns_select_explicit`,
   `TPS.no_valid_explicit_zero_hop`,
   `TPS.fallback_select_fast_peers`); §31h green.
2. Plan-241 §5/§7 one-hop/zero-hop-forbidden helper settings remain
   unchanged on the Streaming lane (helper-side `REPORT_TUNNEL_PROFILE`
   re-proof fails = `exit 72` hard harness failure, never a counted
   terminal, never a zero-hop fallback); §31d green.
3. Exact-via-C removed as a continuation gate. The Plan-242 §7 gate
   consumes inbound/outbound installed AND non-zero counts and
   zero-hop absence only. `contains_c` and `exact_via_c` are
   recorded as `p242-client-tunnels` diagnostic only. Unit row
   `p242_exact_via_c_is_diagnostic_not_gate` locks the separation.
4. C-specific profile gate replaced with stock-candidate-population
   evidence. `P242-A-NO-STOCK-CLIENT-TUNNEL-CANDIDATE` fires only
   when both Router B and Router C are not selectable on Router A's
   peer manager (and neither is forever-banlisted). Unit row
   `p242_c_profile_absence_alone_does_not_block_helper` locks the
   permission. `p242_no_candidate_population_stops_before_helper`
   locks the strict empty-population terminal.
5. Installed client paths observed with bounded length/role facts:
   `p242-client-tunnels` row carries `inbound_tunnel_length_including_local`,
   `outbound_tunnel_length_including_local`, `inbound_remote_hop_count`,
   `outbound_remote_hop_count`, `inbound/outbound first/last
   remote_role` (bounded `B`/`C`/`other-controlled`/`unknown`/`none`
   label set), `inbound/outbound contains_b`, `inbound/outbound
   contains_c`, `inbound/outbound exact_one_remote_hop`,
   `inbound/outbound exact_one_remote_hop_via_c`,
   `inbound/outbound nonzero_count`, `inbound/outbound
   unexpected_peer_observed`. Unit row
   `p242_installed_path_records_remote_hop_count` locks the
   vocabulary.
6. Unknown peers fail closed. The `unexpected_peer_observed` flag
   (or any first/last remote role reading `unknown`) drives the
   `P242-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL` fail-closed terminal.
   Unit row `p242_unknown_client_tunnel_peer_fails_closed` locks
   the direction discrimination (inbound / outbound / both).
7. A genuine non-zero-hop inbound/outbound pair is sufficient to
   enter the Streaming driver. Unit rows
   `p242_nonzero_pair_is_lookup_prerequisite` and
   `p242_zero_hop_remains_forbidden` lock the corrected gate.
8. Plan-240 exact streaming job correlation retained verbatim
   (`helper DBID + target hash + ISJ job ID + epoch`). Unit row
   `p242_plan240_exact_job_correlation_retained` locks the binding;
   `P242Inputs.lookup` is `P240Inputs` by value; the classifier
   delegates pre-query guards to the retained `P240Terminal` via
   `CEarlierGuard`.
9. Lookup reply/install stages remain ordered in the `P242-D-*`
   chain (`B-LOOKUP-NOT-RECEIVED` → `B-TARGET-LS-NOT-QUERY-ANSWERABLE`
   → `B-ANSWER-NOT-EMITTED` → `A-CLIENT-TUNNEL-DSM-NOT-RECEIVED` →
   `A-CLIENT-SUBDB-NOT-INSTALLED` → `LOOKUP-SUCCEEDED`). Unit rows
   `p242_b_query_requires_exact_dispatch`,
   `p242_b_receipt_requires_query`,
   `p242_b_answer_requires_b_receipt`,
   `p242_client_dsm_requires_b_answer`, and
   `p242_subdb_install_requires_client_dsm` lock the ordering.
10. OCMOSJ resumes only after `P242-D-LOOKUP-SUCCEEDED`. Unit row
    `p242_ocmosj_resume_requires_lookup_success` locks the
    constraint.
11. No i2pr-owned defect claim before exact expected TunnelData.
    `p242_authorizes_production_change` returns `false` for every
    terminal; the shared fail-closed
    `p239_production_change_allowed_before_owned_defect` semantic
    is retained.
12. No Java source / topology / RNG / profile / publication / timing
    / raw-helper / production workaround introduced. The
    implementation file list is confined to harness/test-only
    surfaces (see "Implementation surfaces" below); the production
    `i2pr-daemon/src`, `i2pr-client/src`, `i2pr-tunnel/src`,
    `i2pr-runtime/src` carry no `P242`/`p242` surface (§31f green);
    `ReferenceRawDestination.java` is frozen (§31e green);
    `ReferenceStreamingService.java` retains the Plan-241 corrected
    one-hop profile unchanged (§31d green); the harness forbids
    the `random.nextInt(4) == 0` literal as a harness hack
    (§31 forbidden list).
13. Plan-201/204 are updated together at closure (see disposition;
    no silent unblock).

## Implementation surfaces

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
tests/integration/m6-interop/java/ControlledRouter.java
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P227Probe.java  (extended)
tests/integration/m6-interop/run-java.sh
```

No `src/` file changed. The M6 checker §31 production-surface guard
(no `P242`/`p242` in `i2pr-daemon/src`, `i2pr-client/src`,
`i2pr-tunnel/src`, `i2pr-runtime/src`) is green on the implementation
head. The §31 forbidden list (raw-helper change, Java source/jar
patching, client-DB B-RI store, direct tunnel install, profile/tier
mutation, `netDb.alwaysQuery`, role/address changes, C1/C2
semantic changes, window widening, zero-hop in the counted lane,
missing `contains_b`/`contains_c` facts, membership-as-query,
success without subDB proof, pre-TunnelData i2pr-defect claims,
production `src/` code, raw-log promotion, `|| true`/REQUIRED_FAILED
suppression, RNG literal in the harness, force-explicit) is
enforced by checker §31, green. Frozen windows, topology, profile
policy, publication target, and pins are unchanged.

## Limitations and findings

- Severity medium (boundary, not defect): live counted external
  attempts are unexecuted on this host (no Java cache + i2pr
  daemon pair available). The §31 static checker is the verbatim
  assertion of every Plan-242 surface, every Plan-242 unit row
  passes locally, the source-lock and harness gates are
  fail-closed, and the Plan-241 helper profile is unchanged. A
  subsequent host with the Java cache + i2pr daemon pair can
  execute the lane verbatim.
- Severity low (sample variance): any subsequent host's Plan-242
  bootstrap probe will read the same exact source-locked
  `shouldSelectExplicit` random gate, so the explicit-branch
  frequency remains one-in-four. The shell does not retry until
  the explicit branch fires (`p228-zero-hop-fallback-seen` and
  `p228-explicit-not-selectable-seen` rows continue to record
  the stock and explicit branches symmetrically).
- Severity low (interpretation note): `contains_c` and
  `exact_one_remote_hop_via_c` are surfaced as diagnostic, not
  consumed by the gate. A subsequent host that wants per-length
  histograms or peer-hash-level role attribution must extend the
  probe under a new plan-of-record; it must not reinterpret the
  Plan-242 rows.
- Severity low (measurement note): the Plan-240 single-job
  observability rule applies to the lookup epoch only; the
  Plan-242 extended snapshot observes both directions' installed
  pools independent of any ISJ job count.
- Live §§8–10 lookup driver evidence (extended pool snapshot,
  lookup correlation, D chain, OCMOSJ resume, Direction A) is
  implemented, unit-locked, and statically guarded but
  unexecuted: no counted run reached the streaming driver on
  this host. The first post-gate run on a Java host will exercise
  it unchanged.
- Direction A did not establish; §10 continuation and §11 closure
  do not trigger. No M6 Java-family closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-after-plan242-a-b-build-stage-pending-stock-client-build-successor-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-stock-client-build-successor-after-plan242
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-pending-stock-client-build-attribution-successor-plan-of-record
```

Plan 201 consumes Plan 242 as the next hard dependency on the
streaming axis: the §7 corrected continuation gate now accepts a
genuine non-zero-hop pair, the §5 bootstrap gate no longer requires
Router C to be selectable in isolation, and the §6 extended pool
observation records `inbound/outbound first/last remote_role`,
`contains_b`, `contains_c`, `exact_one_remote_hop_via_c`, and the
non-zero counts as diagnostic (never as gate). The streaming axis
now needs only the narrow §20 stock client-build attribution
successor (whether the corrected helper consistently builds a non-
zero pair on a given host, what selector branch each build chose,
and whether the stock-candidate-population check stays green
across three attempts) plus the publication/final-closure axis
(unchanged).

Plan 204 consumes Plan 242 as the next hard dependency on the
Java-second-family closure: M10 product authority through Plans
213–215 is unchanged. Plan 204 stays blocked on M6 Java
second-family closure (now pending the §20 stock-client-build
successor; no double unblock).

Plan 205 stays retained/deferred (the direct i2cp requalification
lane is intentionally out of scope for Plan 242).

No other registered plan listed Plan 242 as a hard dependency, so
nothing else changes state. No successor is pre-registered here;
the §20 stock-client-build successor requires its own
plan-of-record under the same subsystem, registered only from the
exact final boundary above.

## Follow-up boundary

A successor owns exactly one §20 arm from the exact final boundary:
- bootstrap not ready (stock-candidate population empty across
  three attempts) → bootstrap-population corrective;
- non-zero pair not built (1+1 non-zero tunnels installed but the
  installed selectors never pick a stock peer) → stock client-build
  attribution (which selector branch each build chose, whether the
  `random.nextInt(4)` branch is ever observed, and whether `other-
  controlled` ever appears as a remote hop);
- lookup still selects zero-hop despite an authoritative non-zero
  pool → selector contradiction attribution;
- B query dispatches but reply chain fails → exact lookup-reply
  corrective;
- lookup succeeds but OCMOSJ fails later → resume exact Plan-239
  D/E attribution;
- expected TunnelData reaches i2pr then fails → production
  corrective may be authorized for that exact i2pr-owned stage;
- Direction A establishes → return to Plan 201 final Java-family
  closure.

It must retain the Plan-242 gates, correlation key, source lock,
frozen topology/profile/timing, and fail-closed lanes, and stop at
the first proven stage. It must not pre-authorize a topology
correction, a publication corrective, a tunnel-policy change, a
selector-bias correction, an RNG override, or a production i2pr
change unless exact expected TunnelData proves an i2pr-owned
defect.
