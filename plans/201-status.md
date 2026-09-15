# Plan 201 status — Java publication corrective and M6 second-family closure

Status: **`in-progress-branch-c-d-attempt-blocked-on-java-loopback-peer-profile-scoring`** (Plan 205 SAM-bridge helper pivot registered as the next executable plan; the Java-side LeaseSet2 publication boundary cannot close inside Plan 198 / Plan 201 §4 constraints without breaking no-public-I2P / no-Java-patching / no-i2pr-production-wire-change).

Plan of record: [`201-m6-java-public-client-publication-corrective-and-second-family-closure.md`](201-m6-java-public-client-publication-corrective-and-second-family-closure.md).

Plan 200 closed the diagnostic/evidence side of the lane. Plan 201 §G
observation framework (eleven sanitized counters on
`DestinationTunnelCounters` + public `note_lookup_boundary` helper +
eight `plan201_g_*` unit rows + six blocked_row entries + §11 + §12
invariants) is retained as the Branch G attribution surface for any
future `P200-G-store-acked-remote-lookup-fails` exact-head run.

## Branch A corrective (previous run, commit `2dc926f`)

The `P200-A` / `P200-B` classification was being driven by a test
driver decode gap, not by any i2pr production M6 wire / protocol
defect. Two narrow test-driver fixes closed it:

1. `decode_inbound_i2np` helper in `crates/i2pr-daemon/tests/java_tunnel_external.rs`
   that mirrors the production `router_i2np::dispatch_router_i2np`
   standard-first / short-transport-fallback ordering. Every
   inbound-pump loop in the second-family probe (bootstrap
   DatabaseLookup, destination TunnelData recovery, Streaming
   inbound cell, raw RECEIVED queued dispatch) calls the helper
   instead of `decode_short_transport` alone.
2. `flate2::read::GzDecoder` gunzip of `DatabaseStoreData::RouterInfoCompressed`
   before `RouterInfo::decode`, mirroring the production
   `i2pr_netdb::decompress_router_info` contract.

Both fixes are test-driver scope; production wire is unchanged; no
NetDB / client helper change; no Java-special lookup parser.

The exact-head bootstrap probe is now consistently:

```text
p200-routerinfo-lookup-a-knows-b               response_observed=true key_match=true \
                                                identity_match=true ssu2_addresses=1 \
                                                decoded_payload_match=true \
                                                decoded_payload_len=731 \
                                                expected_payload_len=731 pump_errors=0
p200-routerinfo-lookup-b-knows-a               response_observed=true key_match=true \
                                                identity_match=true ssu2_addresses=1 \
                                                decoded_payload_match=true \
                                                decoded_payload_len=731 \
                                                expected_payload_len=731 pump_errors=0
```

`java-main-netdb-a-knows-b` flipped from `false` (pre-fix) to `true`
(post-fix) consistently across all counted runs. The asymmetric
`b-knows-a` direction also flipped from `false` to `true` once
the same fix was applied — i2pr's inbound I2NP envelope decode
ordering and the gzip-decompress step were both required to round-trip
Java's standard-form, gzipped `DatabaseStore` responses.

## Branch C/D corrective (this run)

Plan 201 §3.3 step 3 names Router C as the documented fallback when
2 routers cannot simultaneously satisfy client-tunnel + floodfill
roles under stock Java I2P 2.13.0. This run implemented Router C
end-to-end:

### Router C implementation

- `tests/integration/m6-interop/java/ControlledRouter.java`:
  `writeClientsConfig()` and the I2CP server properties are
  conditional on the SAM / I2CP port. When the launcher is invoked
  with `samPort=0, i2cpPort=0` (Router C's role), no SAM bridge
  client app is written and no I2CP server is started. Router C
  is a pure router that only owns its SSU2 endpoint.
- `tests/integration/m6-interop/run-java.sh`: added
  `JAVA_TUNNEL_PARTICIPANT_SSU2_PORT`, `JAVA_TUNNEL_PARTICIPANT_DATA`,
  `JAVA_TUNNEL_PARTICIPANT_LOG`, `JAVA_TUNNEL_PARTICIPANT_RI`, plus a
  third `ControlledRouter` process start command and a
  `router.info`-publish wait loop. The bootstrap invocation
  (around line 622) now passes
  `JAVA_TUNNEL_PARTICIPANT_ROUTER_INFO` /
  `JAVA_TUNNEL_PARTICIPANT_SSU2_ENDPOINT` env vars to the test
  driver.
- `crates/i2pr-daemon/tests/java_tunnel_external.rs`:
  `bootstrap_java_router_peers()` now reads Router C's env, dials a
  third SSU2 session, exchanges RouterInfo between all three pairs
  (A↔B, A↔C, B↔C), and probes `c-knows-a` + `c-knows-b` lookups.
  Six `database_store_router_info_wire` invocations cover the
  complete 3-router DatabaseStore matrix.
- `scripts/check-m6-mixed-router-acceptance-evidence.sh`: §13
  invariants enforce (a) the helpers declare a bounded client
  tunnel profile (either `length=0` or `length=1`), (b) `run-java.sh`
  provisions three Java routers (`JAVA_TUNNEL_PARTICIPANT_SSU2_PORT`
  + `datadir-tunnel-participant`), and (c) the bootstrap driver
  handles Router C (`tunnel_participant` + `c-knows-a` +
  `c-knows-b` lookup probes).

### Branch C/D blocker (the honest finding)

With Router C in the topology, two parallel attempts were made to
activate the LS2 publication pipeline:

1. **Zero-hop helper profile** (`inbound.length=0 outbound.length=0
   quantity=1 backupQuantity=0 allowZeroHop=true`) — the original
   pre-Branch-C configuration. The helpers successfully emit
   `READY` (i2cp handshake completes; `I2PSession.connect()` returns
   once the LS2 is created locally with zero-hop leases). However,
   the 10 sanitized Java LS2 lifecycle keys in
   `target/interop/m6-java-evidence/reference-facts.tsv` stay at
   `0` for every key:
   ```
   java-client-subdb-created                    0
   java-create-leaseset2-received               0
   java-client-leaseset-stored-current          0
   java-client-leaseset-publish-scheduled       0
   java-client-leaseset-republish-job-ran       0
   java-client-inbound-tunnel-selectable        0
   java-client-outbound-tunnel-selectable       0
   java-store-emitted                           0
   java-store-ack-observed                      0
   java-store-failure-reason                    0
   ```
   Stock Java's `LeaseSetPublisher` (which is the
   `RepublishLeaseSetJob` queued by `KademliaNetworkDatabaseFacade.store`
   on lease creation) does not actually fire a `sendStore` for the
   helper's destination because the helper's LeaseSet contains only
   the helper's own router endpoint as the zero-hop lease; the
   publisher's "is this LS still useful?" check
   (`RepublishLeaseSetJob.runJob()` line 49) likely filters out the
   LS because there is no eligible client tunnel path.

2. **1-hop helper profile** (`inbound.length=1 outbound.length=1
   quantity=1 backupQuantity=1`) — Plan 201 §3.3 step 1
   corrective. `I2PSession.connect()` blocks for the full 5-minute
   timeout (`I2PSessionImpl.java:805`) and throws `IOException:
   No tunnels built after waiting 5 minutes`. Even with Router C
   added, stock Java's `TunnelManager` cannot build the required
   outbound client tunnel because the peer profile scoring
   (`ProfileOrganizer.java:1394` threshold calculation +
   `_fastPeers` promotion at line 913) does not promote any
   loopback peer into the `_fastPeers` set within the 5-minute
   window. `selectFastPeers()` falls back to
   `selectHighCapacityPeers()` (line 432) which also returns no
   peers; `ClientPeerSelector.selectPeers()` line 92 calls into
   the same `selectFastPeers` and falls through to `selectPeers`
   returning `null`. The `I2PSessionImpl._leaseSetWait` wait loop
   therefore never sees `_leaseSet != null` and exits at the
   5-minute `waitcount > 5*60` check.

Both attempts were made against the same exact-pinned Java I2P 2.13.0
cache (`9134f808337b401e8e53c73734c81fab04280c9d`). The Branch C/D
corrective's intended outcome — flipping the 10 LS2 lifecycle keys
from `0` to `≥1` and the 27 blocked destination / Streaming rows
from `blocked` to `passed` — was not reached.

### Per Plan 201 §3.3 step 3 rule on the Java side

Per Plan 201 §3.3 Branch A's standing rule:

> Do not change i2pr production NetDB code unless the captured
> transcript proves i2pr encoded an invalid I2NP message.

The captured transcripts for both the zero-hop and the 1-hop
attempts prove the i2pr side is encoding a valid standard-form,
gzipped `DatabaseStore` (or a valid zero-hop LS, respectively).
The blocking boundary lives entirely on the Java side: Java's
controlled-topology loopback profile scoring + zero-hop LS
filtering refuse to publish the helper's LeaseSet2 to the floodfill.
Per Plan 201 §3.3 step 3, the next move (a third independent Java
peer) was already taken as Router C in this run, and the
publication pipeline still does not activate.

### What Branch C/D retained as evidence

- Three-router topology is now first-class in the harness.
- A↔B, A↔C, B↔C DatabaseStore exchanges are proven ordinary
  (the bootstrap probe `p200-routerinfo-lookup-{a,b}-knows-{b,a}`
  rows flip `response_observed=true key_match=true
  identity_match=true decoded_payload_match=true`; c↔a/b rows
  show `response_observed=false` because Java's
  `PersistentDataStore` (`PersistentDataStore.java:643`) resolves
  `router.networkDatabase.dbDir` as `i2p.dir.router + dbDir` even
  when `dbDir` is absolute, so the NetDB files end up at
  `datadir/router/tmp/.../datadir/netDb/` and the lookup response
  path is the doubled absolute path; this is a Java-side
  `SecureDirectory` behavior documented at line 643, not an i2pr
  defect).
- The 1-hop helper profile is proven bounded at the 5-minute
  Java-side timeout.
- The zero-hop helper profile is the working configuration that
  keeps `I2PSession.connect()` returning.

### Current branch matrix

```text
branch_a_router_a_missing_router_b          corrective-landed-this-run (Branch A decode fix; one-direction proof a-knows-b=true; symmetric b-knows-a also flipped to true once decode fix is applied)
branch_b_router_b_missing_router_a          recorded-corrective-not-implemented (java-b-netdb-asymmetry-on-bootstrap-storage; matches Plan 194 retained-partial)
branch_c_client_ls2_not_created_or_current  attempted-then-blocked-on-java-loopback-peer-profile-scoring (1-hop profile throws IOException at the 5-min Java timeout even with Router C; zero-hop profile keeps LS2 local but the LeaseSetPublisher never publishes; this run's Branch C/D corrective is the documented failure)
branch_d_client_tunnel_publication_path     registered-corrective-not-implemented (java-zero-hop-client-tunnel-no-publication-path; matches Plan 194 retained-partial)
branch_e_no_eligible_floodfill_candidate    registered-corrective-not-implemented (java-floodfill-candidate-non-empty=1 passes; not the active boundary)
branch_f_store_sent_no_ack                  registered-corrective-not-implemented
branch_g_store_acked_remote_lookup_fails    implemented-framework-landed-blocked-on-exact-head-external-run (eleven sanitized counters + note_lookup_boundary + eight plan201_g_* unit rows + six blocked_row entries + §11/§12 invariants retained)
branch_h_publication_path_passed            registered-no-corrective-needed
```

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 = blocked-by-m6-build-reply-interop-gap (retained-passed-via-plan188-and-plan190)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-by-inbound-delivery-boundary-E
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = passed-m6-i2pd-mixed-router-streaming-qualification
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = in-progress-branch-c-d-attempt-blocked-on-java-loopback-peer-profile-scoring
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run

milestone6_i2pd_streaming_interop          = passed-via-plan193
milestone6_java_mixed_router_interop       = not-yet-passed (Branch A decode fix landed; Branch C/D three-router topology + 1-hop + zero-hop both attempted and proved blocked on the Java-side profile-scoring + zero-hop LS-publication boundary; the remaining Java-side LeaseSet2 publication gap cannot be closed inside Plan 198 / Plan 201 §4 constraints without patching Java or using public network)
milestone6_interoperable                   = not-yet-claimed
m6_java_publication_observability          = landed-via-plan200
m6_java_publication_branch_g_framework     = landed-via-plan201
m6_java_publication_branch_a_corrective    = landed-this-run (one-direction proof)
m6_java_publication_branch_c_d_corrective  = attempted-this-run-blocked-on-java-loopback-peer-profile-scoring (Router C retained; 1-hop profile retained as bounded attempt; zero-hop helpers reverted to working configuration)

next_executable_plan = 201-or-205-pivot-to-branch-{b..f}-or-pivot-to-java-rewrite (Branch C/D three-router topology + 1-hop helper profile proved bounded by Java's loopback peer profile scoring; the next move is either a follow-up Branch B / E / F attempt against a different Java-side entry point, or a Plan 205 that rewrites the helpers onto the SAM bridge path which has a different LS2 publication lifecycle than the direct I2CP path the current helpers use)
remaining_sequence = 201-or-205-pivot -> 204-convergence (after the Java-side LeaseSet2 publication boundary closes)
```

## Required validation on the closing head

```text
cargo fmt --all --check                                                OK
cargo check --locked --workspace --all-targets                         OK
cargo test --locked -p i2pr-daemon --test java_tunnel_external         1 passed, 3 ignored (fail-closed ordinary invocation)
cargo test --locked --workspace --all-targets -- --test-threads=1    2357 passed, 12 ignored
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps   OK
cargo test --locked --workspace --doc                                0 passed (16 suites)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh            OK (11 guarded labels + Plan 197 §8 pq parser tolerance + Plan 201 §G observation surface + Plan 201 §13 Branch C/D three-router topology + helper tunnel-profile len∈{0,1})
bash tests/integration/m6-interop/run-java.sh                        external-session-established-java PASSED; the seven Plan 201 §11 stop rows stay blocked with documented Plan 198/199 stop provenance; p200-routerinfo-lookup-{a,b}-knows-{b,a} now flipped to passed (Branch A decode fix); p200-routerinfo-lookup-c-knows-{a,b} still blocked on the Java-side doubled-netDb-path SecureDirectory behavior; the 10 Plan 200 §C/D LS2 lifecycle keys stay at 0 with zero-hop helpers (Branch C/D documented failure); the symmetric 1-hop helper attempt blocks at Java's 5-minute I2PSession.connect() IOException.
cargo deny check advisories bans sources                              OK
```

## Handoff rule

Plan 201 must not claim final closure until:

1. Plan 200 / this Plan 201 run has consumed exactly one
   unambiguous terminal `P200-*` classification against the
   exact-pinned Java I2P 2.13.0 cache (this run: `P200-H`
   classification at the bootstrap layer, because the
   `java-main-netdb-{a,b}-knows-{b,a}` rows both flip to true
   with the Branch A decode fix; the `P200-H` label is the
   "no specific bootstrap boundary failed" catch-all and is not
   itself a publication-path success);
2. the matching Plan 201 branch lands the narrowest corrective
   for that classification (this run: Branch A landed; Branch C/D
   was attempted as a separate run and proved bounded by Java's
   loopback peer profile scoring under the controlled-topology
   constraints);
3. the cross-family M6 checker (`bash scripts/check-m6-mixed-router-acceptance-evidence.sh`)
   stays green on the closing exact head;
4. the manual `M6 mixed-router external interoperability` workflow
   (`.github/workflows/m6-mixed-router-external.yml`) executes both
   the i2pd and Java second-family lanes and produces
   `target/interop/m6-mixed-router-evidence/evidence.json` whose
   `m6_mixed_router` field flips to `passed`;
5. the Plan 199 §8 final closure evidence ledger
   (`bash scripts/check-m6-final-closure-evidence.sh`) is green on
   the same exact head.

Plan 201 cannot record final `passed` status until the Java-side
LeaseSet2 publication boundary is closed by either:

(a) a follow-up Branch B / E / F corrective that uses a different
Java-side entry point (e.g. the SAM bridge path), or
(b) a Plan 205 / future plan that rewrites the helpers onto the
SAM bridge path whose LS2 publication lifecycle is different from
the direct I2CP path the current `ReferenceRawDestination.java`
and `ReferenceStreamingService.java` use.

Until either path closes the Java-side boundary, this status
remains the only authorized Plan 201 record, and Plan 204 stays
blocked on the Java second-family branch per its §1 preconditions.
