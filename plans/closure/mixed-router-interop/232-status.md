# Plan 232 status — M6 Java route-derived lease-gateway fixture corrective and second-family closure

Status: **`passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary`**.

Plan of record:
[`232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure.md`](../../implementation/mixed-router-interop/232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure.md).

## Closure result

Plan 232 is closed as Outcome B (`P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY`).
The fixture defect proven by Plan 231 is corrected at all three Java-driver
local lease sites, route parity is proven live on both lanes, raw-Destination
reverse delivery flips to digest-matched pass, and the retained Java
Streaming continuation stops at the first new exact post-correction boundary
on two consecutive counted runs:

```text
corrected local LS2 (all 3 sites)
  -> lease gateway == installed inbound route gateway (Router A)
  -> lease tunnel   == installed inbound route gateway tunnel
  -> publication target unchanged (Router B, independent NetDB role)
  -> raw-Destination forward digest-matched (27 B)
  -> corrected-route preflight: published gateway/tunnel match route,
     target router (role=A) exposes the exact IBGW
  -> tracked Java -> i2pr reverse send ACCEPTED
  -> i2pr exact inbound TunnelData observed, recovery completes,
     Garlic decodes, dispatcher queues, digest matches in 45 s
  -> terminal P232-D-REVERSE-DELIVERY-PASSED (attempt 1)
  -> Streaming initial lease route parity proven live (attempts 2+3)
  -> Streaming SYN sent through the corrected path (attempts 2+3)
  -> SYN-ACK never established, twice identically, no tuning
  -> terminal P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY
     stage=streaming-syn-ack-never-established
```

No `P232-JAVA-SECOND-FAMILY-PASSED`. No production Rust behavior change.
The Streaming refresh lease path is corrected in code and proven by unit
contract tests; it was never reached live because both Streaming runs stop
before Direction B republication.

## Implementation commit and pinned inputs

Implementation was committed before counted execution (Plan 232 §14); each
counted run below executed on that exact SHA with a clean tree and no
between-attempt tuning:

```text
236ccb6 interop: implement Plan 232 route-derived lease-gateway fixture corrective
```

Full SHA:

```text
236ccb63c056349bf784a19edf8a1c1d69064582
```

Reference inputs remained frozen: Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; the i2pd reference pin
remained `2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`. No
dependency, fixture, production protocol, or Java reference source
changed. No Java source patch, reflection, private-field access,
direct queue/tunnel/NetDB/profile injection, paired-tunnel policy
override, VMComm, `netDb.alwaysQuery`, public I2P, distinct topology,
build/timeout change, or reverse-window change (30-second install poll,
five-minute helper/tunnel ceiling, 45-second payload acceptance, and
70-second status-only observation frozen; checker-pinned).

Test-only deltas (no production Rust code changed; enforced by the new
checker rule that rejects any `p232`/`P232` surface under
`crates/i2pr-daemon/src`, `crates/i2pr-client/src`,
`crates/i2pr-tunnel/src`, `crates/i2pr-runtime/src`):

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs (route-derived helper +
  parity/publication/preflight/classifier surface, 3 corrected lease
  sites, destination preflight + reverse outcome, streaming completion
  row, 18 unit rows)
scripts/check-m6-mixed-router-acceptance-evidence.sh (Plan 232 §25 invariants)
```

## Requirement-to-evidence matrix

| Plan-232 requirement | Evidence / result |
|---|---|
| Route-derived helper/contract, never hardcoded A/B (§6) | `p232_route_derived_lease_source(slot, local_receive, route, registry_slot, …)` derives gateway/tunnel ONLY from the `InboundGatewayRoute` object; helper body contains no `java_hash`/`service_hash` (checker §25c). `InboundLeaseSource::from_parts` appears exactly once in the driver (the helper body; checker §25b). |
| All three lease sites corrected (§7 B1–B3) | Destination LS2, initial Streaming LS2, refreshed Streaming LS2 each fetch `coord.registry().inbound_gateway_route(local_receive)`, assert the controlled-fixture route (`service_hash` / `IBGW_RECEIVE` / `IBGW_NEXT`, streaming `0x9801` / `0x9802` namespace), revalidate the registry slot binding, and call the helper. Helper call count ≥ 4 (def + 3 sites; checker §25d). Refresh re-derives from the live registry before rebuilding (no blind copy). |
| Publication semantics preserved (§8) | `begin_ls2_publication` / republication targets remain `RouterHash(java_hash)` (checker §25e). `p232-publication-separation` rows prove `lease_from_installed_route=true` with `publication_distinct_from_gateway=true` on all three sites. |
| Raw-Destination requalification (§9) | Attempt 1 (destination-only) re-proves the retained prerequisites (capability eligible, natural bootstrap, non-zero exploratory, exact one-hop client tunnels through C, target LS2 lookup, digest-matched forward) then the corrected preflight (`published gateway/tunnel match route`, `target_router_has_exact_ibgw=true`, role=A) and the digest-matched reverse inside 45 s → `P232-D-REVERSE-DELIVERY-PASSED`. |
| Streaming continuation (§10) | Attempts 2+3 (streaming-only, same SHA, no tuning) prove initial Streaming route parity live, publish through the corrected path, and send the Direction-A SYN; both stop identically at `plan199-java-stop: SYN-ACK never established (syn_accepted=false established=false pump_error=0)`. Outcome B recorded; refresh parity stays unit-proven. |
| Route-parity evidence contract (§11) | `p232-destination-lease-route` / `p232-streaming-lease-route` rows carry lane/stage/slot/local-receive/route-hash/route-tunnel/lease-hash/lease-tunnel/publication-hash plus the three booleans; construction fails closed (helper `Err` + record `assert!`) before publication on any mismatch. |
| Required focused tests (§12) | 18 `p232_*` unit rows, all green on closing SHA (see Verification). |
| Static evidence guards (§13) | Checker §25 (12 invariants: surface presence, single `from_parts`, helper purity, 3-site coverage, no hardcoded terminal, publication targets unchanged, frozen 30 s/45 s/70 s windows, no production surface, scratch-only logs, 18 unit rows). Green on closing SHA. |
| Attempt discipline (§14) | 1 implementation SHA, 3 counted external attempts, no tuning (see history). |
| Closure/registry/roadmap/unblock audit (§18) | This record plus registry/roadmap/201/204 updates plus Plan 233 registration in the same commit (see below). |

## External attempt history

All attempts on implementation SHA `236ccb6` (clean tree, exact pins,
frozen topology/windows, no tuning between attempts).

### Attempt 1 — `I2PR_M6_JAVA_DRIVER=destination` — raw reverse PASSED

Authoritative per-stage facts (fresh disposable A/B/C RouterContexts):

```text
route parity (destination/initial):
  registration_slot=0 local_receive_tunnel=38402 (0x9602 = IBGW_NEXT)
  route_gateway_hash=58788a83… (Router A / service router)
  route_gateway_tunnel=38401 (0x9601 = IBGW_RECEIVE)
  lease_gateway_hash=58788a83… (== route)
  lease_gateway_tunnel=38401 (== route)
  publication_target_hash=31ae66ef… (Router B, distinct)
  gateway_route_match=true tunnel_route_match=true
  publication_distinct_from_gateway=true
  gateway_matches_service_router=true
forward: reference-received payload_len=27 match=true
  digest=ec61e08da98b76b02ee2268d544b90da0c3b7cace6339d627cd3d40b85048ceb
preflight: published gateway/tunnel match route (true/true)
  target_router_has_exact_ibgw=true role=A lease_tunnel=38401
reverse: destination-inbound-received payload_len=27 match=true pump_error=0
  p231-classification P231-REVERSE-DELIVERY-PASSED nonce=1 role=A
    lease_tunnel=38401 send_id=3239805366 expected_seen=true
    digest_match=true ordered_statuses=[1]
  p232-classification P232-D-REVERSE-DELIVERY-PASSED nonce=1 role=A
    lease_tunnel=38401 send_id=3239805366 parity_ok=true
    expected_seen=true digest_match=true ordered_statuses=[1]
wrapper: destination rows passed (inbound/outbound/tunnels/lookup/
  publication/forward/reverse + p220/p222/p223/p224/p225 terminals);
  streaming rows absent (sub-run not executed); legacy Plan-200 §C/D
  Java client-LS2 rows remain failed with stop provenance (the known
  public-client publication gap owned by Plan 201, unchanged by this
  fixture corrective).
```

The role flip `B → A` against the identical Plan-231 epoch shape is the
regression proof: the same reverse send that stopped at
`P231-C-TARGET-IBGW-NOT-INSTALLED` (advertised gateway B, exact IBGW
absent twice) now addresses Router A (exact IBGW present) and delivers
digest-matched.

### Attempt 2 — `I2PR_M6_JAVA_DRIVER=streaming` — new Streaming boundary

```text
route parity (streaming/initial):
  registration_slot=1 local_receive_tunnel=38914 (0x9802)
  route_gateway_hash=d74bd11d… (Router A)
  route_gateway_tunnel=38913 (0x9801)
  lease gateway/tunnel == route; publication 7878a26e… (Router B, distinct)
  all four booleans true
progress: lease-lookup-completed leases=1; ls2-publication-tunnel cells=1
  streaming-syn-sent true
stop: plan199-java-stop SYN-ACK never established
  (syn_accepted=false established=false pump_error=0)
```

### Attempt 3 — `I2PR_M6_JAVA_DRIVER=streaming` — boundary reproduced

```text
route parity (streaming/initial):
  registration_slot=0 local_receive_tunnel=38914
  route_gateway_hash=8174c236… (Router A)
  route_gateway_tunnel=38913; lease == route; publication 0fef1281…
  all four booleans true
progress: lease-lookup-completed leases=1; ls2-publication-tunnel cells=1
  streaming-syn-sent true
stop: plan199-java-stop SYN-ACK never established
  (syn_accepted=false established=false pump_error=0)
```

Two consecutive identical stops on the same SHA with no tuning: the
Streaming SYN-ACK boundary is the earliest new exact post-correction
stage. Terminal for both runs:

```text
P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY stage=streaming-syn-ack-never-established
```

The enclosing legacy Plan-199 Java wrapper exits nonzero on all three
runs (uncounted Streaming rows stay unqualified; Plan-200 §C/D rows
unchanged). Raw Java logs remained scratch-only (bounded counts only).

## Verification

Successful verification on closing implementation SHA `236ccb6` (local
truth; the tree was clean, so the tested tree is the committed tree):

```text
cargo fmt --all --check                                      PASS (closing SHA)
cargo check --locked --workspace --all-targets                PASS (closing SHA)
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                                   PASS (2647 passed, 18 ignored; +18 are the P232 unit rows)
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
bash scripts/check-m6-mixed-router-acceptance-evidence.sh    PASS (§25 invariants, closing SHA)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
                                                                   PASS (18 tests, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p232_ -- --test-threads=1
                                                                   PASS (18 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p231_ -- --test-threads=1
                                                                   PASS (18 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
                                                                   PASS (21 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
                                                                   PASS (closing SHA)
bash -n tests/integration/m6-interop/run-java.sh             PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh PASS
javac (all probes + launcher + helpers vs staged jars)       PASS (closing SHA)
```

The §25 integrity script additionally pins: the 15-symbol P232 driver
surface, the 11 terminal tokens, the single-`from_parts` rule, the
helper purity rule (no `java_hash`/`service_hash` in the helper body),
the ≥4 helper-call rule, the four lane/stage literals, the retained
publication targets, the frozen 30 s/45 s/70 s windows, no production
`p232`/`P232` surface, scratch-only logs, and the 18 unit rows.

Focused Plan-232 unit rows (18, all green on closing SHA):

```text
p232_destination_lease_gateway_matches_installed_inbound_route
p232_destination_lease_tunnel_matches_installed_inbound_route
p232_destination_publication_target_is_not_lease_gateway_source
p232_streaming_initial_gateway_matches_installed_inbound_route
p232_streaming_initial_tunnel_matches_installed_inbound_route
p232_streaming_refresh_revalidates_installed_inbound_route
p232_streaming_refresh_gateway_matches_installed_inbound_route
p232_streaming_refresh_tunnel_matches_installed_inbound_route
p232_route_derived_helper_rejects_missing_inbound_route
p232_route_derived_helper_rejects_slot_route_mismatch
p232_java_hash_cannot_be_hardcoded_as_local_lease_gateway
p232_all_java_local_lease_sites_use_route_derived_contract
p232_corrected_target_router_must_have_exact_ibgw_before_send
p232_reverse_pass_requires_exact_tunneldata_and_digest
p232_raw_reverse_pass_continues_to_streaming
p232_streaming_pass_can_close_java_second_family
p232_timeout_windows_remain_frozen
p232_no_production_surface_change
```

## Security, compatibility, and operational decisions

- No production crate or protocol behavior changed; all deltas are in
  the external Java driver lease sites/evidence/classifier/unit rows
  and the static evidence checker. The new checker rule fails the
  build if any `p232`/`P232` token ever appears under production
  `src/` directories.
- The correction redirects the test's own advertised lease to the
  router that actually owns the inbound gateway. No Java
  NetDB/publication policy changed: Router B remains the floodfill
  publication target; only the lease gateway/tunnel fields moved
  (B → A) to match installed tunnel material.
- The helper rejects a missing route and any slot/selector mismatch
  before publication; the record layer asserts gateway+tunnel parity
  before the LS2 is built. A future lease site that bypasses the
  helper fails the static check (single-`from_parts` rule).
- Durable evidence contains only bounded booleans, counts, tunnel
  ids, hex hashes, lengths, digests, and enumerated stage tokens. Raw
  Java logs, peer paths, keys, tags, SessionConfig contents, and
  payloads remain scratch-only.
- Java and i2pd pins, SAM/I2CP/diagnostic loopback policy, and all
  frozen windows remain unchanged. Plan-230 C1+C2 controlled-topology
  corrections and Plan-231 observability remain intact.
- M10 product authority (Plans 213–215) is untouched and stays
  closed; this M6-only corrective neither reopens nor downgrades it.
  Plan 205 SAM/helper pivot remains off-path.

## Findings and limitations

No security finding was introduced. Severity: no critical/high
findings.

High (new exact post-correction boundary — owned by the registered
successor, not by this corrective): Java Streaming Direction-A SYN
traverses the corrected path (`streaming-syn-sent=true`) but the SYN-ACK
never establishes (`syn_accepted=false established=false pump_error=0`)
on two consecutive counted runs. Route parity, publication separation,
lease lookup, LS2 publication, and SYN emission are all proven on the
same runs, so the stop is downstream of the lease fixture — in Java-side
SYN receipt/dispatch, the helper streaming service destination, or the
i2pr Streaming responder path under the Java second-family topology.
Raw-Destination reverse delivery passing on the same fixture proves the
destination/garlic/tunnel path is sound; the Streaming handshake layer
needs its own narrow attribution. Registered as Plan 233; no production
change authorized by this record.

Medium: the Streaming refresh lease path never executed live (both
Streaming runs stop before Direction B republication). Its correction
is code-identical to the proven initial path and is locked by four
dedicated unit rows (revalidation, gateway, tunnel, plus the
three-site contract row), but live refresh parity awaits Plan 233.

Medium: profile-bootstrap stochasticity persists across fresh contexts
(the destination lane still depends on natural Router-C profile
formation inside the frozen 30-second poll). Attempt 1 bootstrapped;
no timing inflation was authorized to chase the stochastic tail.

Low: the legacy Plan-199 wrapper still exits nonzero (uncounted
Streaming rows + Plan-200 §C/D Java client-LS2 rows). Those rows are
outside Plan-232 scope: Streaming rows belong to Plan 233, and the
§C/D public-client publication gap belongs to Plan 201.

Low: counted evidence TSVs live under `target/interop/` (gitignored
scratch-evidence by lane design). This record quotes the terminal rows
verbatim; re-running the exact SHA reproduces them subject to the
documented bootstrap stochasticity.

## Roadmap and unblock audit

Plan 232 is formally closed at Outcome B
(`P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY
stage=streaming-syn-ack-never-established`) with the raw-Destination
reverse pass retained. The unblock audit examined every registered plan
listing Plan 232 as a dependency:

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = registered-ready-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix
plan_201 = blocked-pending-plan233-streaming-syn-ack-corrective-after-plan232-raw-reverse-pass
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan233
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 233-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

- Plan 201 remains blocked: its Plan-232 hard dependency is closed
  (fixture corrected, raw reverse passed), but Java second-family
  closure still requires the Streaming pass, which stopped at the new
  SYN-ACK boundary. Its blocker is narrowed from generic Plan-232
  pendency to the Plan-233 corrective. No 201-lane rerun without Plan
  233 would be interpretable.
- Plan 233 is registered in the same commit as the narrow Streaming
  SYN-ACK corrective justified by Outcome B. It inherits the corrected
  fixture, frozen topology/windows, and the §25 checker invariants,
  and must not relitigate the closed lease-gateway correction.
- Plan 204 remains blocked on that same independent M6 closure
  (now pending Plan 233); its M10 product/application authority
  (Plans 213–215) is unchanged and closed.
- Plan 205 remains retained/deferred: the observed boundary is at the
  Java Streaming handshake layer above the now-passing destination
  path — still below any SAM bridge.
- No plan is unblocked to ready except Plan 233 (registered-ready):
  every other ready-gated item still has an unclosed hard dependency
  (Streaming qualification itself).

## Registration basis (retained)

Plan 231 closed as:

```text
passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
```

Its deepest counted run proved Java A enqueue, Router-C OBEP
processing, exact target-IBGW absence on Router B (twice), and honestly
zero i2pr wire — root-caused to the test-driver lease fixture
advertising gateway B for an inbound tunnel terminating at Router A.
Plan 232 corrected exactly that fixture at all three lease sites and
re-qualified: the role flips B → A, the exact IBGW is present, and the
tracked reverse payload delivers digest-matched.

## Current authority (superseded by closure above)

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
plan_232 = registered-ready-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure

plan_201 = blocked-pending-plan232-route-derived-lease-gateway-fixture-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan232
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## Authorized work (executed; retained for traceability)

Plan 232 refactored all local LS2 lease construction onto one test-only
route-derived contract, proved gateway+tunnel parity before every
publication while preserving Router B as the independent publication
target, re-ran the destination lane to a digest-matched reverse pass,
continued directly into Streaming on the same implementation, and
stopped at the first new exact boundary with bounded evidence. It
changed no production behavior.
