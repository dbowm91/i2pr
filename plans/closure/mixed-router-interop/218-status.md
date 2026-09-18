# Plan 218 status — M6 Java second-family final qualification

Status: **`stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`**.

## 2026-09-18 handoff correction — Plan 220 diagnostic corrective

Plan 218 remains the authoritative behavioral stop: criteria 1–9 passed and
Java → i2pr reverse raw Destination delivery did not arrive.

Plan 219 attempted to attribute that stop but its J219-B result is superseded
by Plan 220 due to diagnostic defects. Therefore Plan 218's root cause remains
unknown; only the reverse-delivery boundary itself is retained.

```text
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = registered-ready-m6-java-plan219-diagnostic-attribution-corrective
next_executable_plan = 220-m6-java-plan219-diagnostic-attribution-corrective
```

## Retained prior Plan 218 status narrative

# Plan 218 status — M6 Java second-family final qualification

Status: **`stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`**.

## 2026-09-18 handoff amendment — Plan 219 registered

Plan 219 is now the dependency-ready investigation of this stop:
`plans/implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md`.

This narrows, rather than changes, the Plan 218 conclusion. Reverse delivery is
reproducibly absent, but the earlier `java-floodfill-candidate=0` root
cause is still a hypothesis because current evidence proves a floodfill config
line and coarse global log counts, not Router B's live RouterInfo `f`,
Router A's PeerManager capability index, the helper client-NetDB lookup peer,
or how far OutboundClientMessageOneShotJob progressed.

Plan 219 owns exact attribution. Plan 218 remains stopped and does not reopen.

```text
plan_219 = registered-ready-m6-java-reverse-delivery-root-cause-investigation
next_executable_plan = 219-m6-java-reverse-delivery-root-cause-investigation
```

## Retained Plan 218 closure record

Plan of record:
[`218-m6-java-second-family-final-qualification.md`](../../implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md).

## 1. Outcome

Plan 218 ran the corrected destination-only harness
(`I2PR_M6_JAVA_DRIVER=destination`,
`tests/integration/m6-interop/run-java.sh`) once on the exact-head
commit `7762e13` against the exact-pinned Java I2P 2.13.0 cache
(`9134f808337b401e8e53c73734c81fab04280c9d`) and the exact-pinned
i2pd 2.61.0 cache (`635b013a612ff47278ef02acf8580a28e10e26c5`).

The destination driver reached the first protocol-authentic boundary
that prevents Plan 218 closure:

* Criteria 1-9 of Plan 218 §11 pass on this head (authenticated
  sessions, RouterInfo bootstrap, public helper session, real outbound
  and inbound installs, LS2 lookup via the owned outbound tunnel,
  LS2 key/signature/destination validation, local i2pr LS2
  publication via the controlled tunnel path, raw i2pr → Java
  reference payload digest match).
* Criterion 10 (`bidirectional raw Destination payloads match
  expected digests`) fails with a typed
  `destination-inbound-send-failed` evidence row; the Java helper's
  `session.sendMessage(...)` for the i2pr destination returns true
  but the bounded reply never arrives at the i2pr destination
  dispatcher after `DATAGRAM_WAIT` (45 s). The reference inbound
  pump reports `datagrams_received=16 i2np_received=5` for the
  destination driver lifetime with no inbound TunnelData matching
  the dispatched Garlic envelope.
* Criteria 11-25 (Direction A / Direction B Streaming and their
  acceptance matrix, plus the cross-family final-closure ledger)
  cannot be exercised against the corrected harness because the
  inbound delivery primitive they share with the destination driver
  is the same boundary that stops criterion 10.

The inbound delivery boundary is the named Plan 194 §11 stop
`streaming-b-connect attempts=N` for the Streaming direction and
`client-ls2-local-but-not-network-visible` for the destination
direction: the stock-Java public client session is ready and the
client-specific LeaseSet is locally present, but the exact-pinned
Java reference does not make that LeaseSet reachable on a path the
i2pr inbound tunnel can use. The harness records every
install-dependent + delivery-dependent row as `blocked` with this
stop provenance and never as `passed`.

The corrected Plan 217 harness reaches this boundary without
panicking, which is the Plan 217 §6.A transfer-once invariant +
§6.B positive/negative grep split + §6.C relative NetDB dbDir +
§6.D disjoint streaming namespace behaving together. Plan 218 is
the first `run-java.sh` execution that consumes that corrected
harness on the exact-pinned Java 2.13.0 cache and the resulting
classification is bounded by the inbound delivery primitive, not
by any harness-side defect.

Per Plan 218 §11 conditional fallback (`criteria 6–8 fail for a
reproducible stock-Java public-client reason`), Plan 218 MUST stop
and record the exact boundary. The §11 wording references criteria
6-8; on this head criteria 7-8 actually pass (the i2pr LS2 lookup
chain resolves a real signed LS2 with `leases=1
published=...`). The reproducible boundary lives at criterion 10
(inbound delivery) and its root cause below the helper surface
(javahelper sendMessage returning true but no floodfill candidates
on Router A so the helper cannot resolve i2pr's destination LeaseSet
through Java's netDb, so the helper's outbound tunnel endpoint has
no real lease to deliver to). Per Plan 218 §1 ("On a genuine
reference-side boundary it stops cleanly …") and §11 last
sentence ("Do not claim M6 closure"), Plan 218 stops at this
boundary and does not claim `passed-m6-java-second-family-final-qualification`.

## 2. Implementation commits

The Plan 218 closure is a status-only commit on the closing head
`7762e13`. No plan-of-record production wire changed; no harness
script changed; no test driver changed; no static checker changed.
The destination `run-java.sh` execution on commit `7762e13` ran
successfully (test exit 0; Plan 217 §6.A `destination-outbound-transferred
outbound_role_transferred_once registry_outbound_len=0 inbound_len=1`
emitted); the resulting evidence.json + driver-evidence.tsv +
reference-facts.tsv are stored under
`target/interop/m6-java-evidence/` on the closing head.

## 3. Work package A — Freeze exact head and preflight

* i2pr SHA = `7762e132ec5602360627387639fee27510126546`
  (`git rev-parse HEAD`).
* Java I2P pin = `9134f808337b401e8e53c73734c81fab04280c9d`
  (`source-revision.txt` matches; cache fingerprint unchanged
  pre/post via the Plan 196 §5.3 `CACHE_FINGERPRINT_BEFORE` /
  `fp_after` guard in `run-java.sh`).
* i2pd pin = `635b013a612ff47278ef02acf8580a28e10e26c5`
  (`source-revision.txt` matches; i2pd binary present at
  `target/interop/cache/ssu2/i2pd/<pin>/bin/i2pd`).
* OS / toolchain: `Linux x86_64`, Rust `1.95.0`
  (`rustc 1.95.0 (59807616e 2026-04-14)`), OpenJDK 25
  (`openjdk version "25.0.4" 2026-07-21`,
  `OpenJDK 64-Bit Server VM (build 25.0.4+7-1-24.04-Ubuntu, mixed
  mode, sharing)`).
* Static M6 evidence guards: `bash
  scripts/check-m6-mixed-router-acceptance-evidence.sh` returns
  `m6 mixed-router evidence check passed (11 guarded labels,
  two-family pins verified, Plan 197 §8 pq parser tolerance
  invariants, Plan 201 Branch C/D three-router topology)` on commit
  `7762e13`.
* Java A/B/C disposable topology: each router in a fresh scratch
  dir under `/tmp/i2pr-m6-plan196-java.<random>/datadir-{service,
  publication,tunnel-participant}/`; `router.reseedDisable=true`,
  `router.floodfillParticipant=true` (A & B), `i2np.ntcp.enable=false`,
  `i2np.ntcp2.enable=false`, no `i2p.vmCommSystem`, no public
  reseed URL, no `AllowVMComm` flag, all listeners bind to
  `127.0.0.1`. Router C is a tunnel participant (`samPort=0`,
  `i2cpPort=0`) with no SAM bridge or I2CP listener.
* Sanitized RouterInfo capabilities reported: Java A/B/C router.info
  files are `731 bytes` each; route key matches the exact pins.

## 4. Work package B — Destination qualification first

The destination driver `destination_message_plane_against_java`
ran to test completion (exit 0, 48.83 s). The required Plan 218 §6.B
progression:

1. **Authenticated sessions to publication and service Java routers** —
   `session-established` evidence key reports `sessions_established=2`
   for the i2pr daemon SSU2 runtime against Router B (publication)
   and Router A (service).
2. **RouterInfo bootstrap via ordinary authenticated I2NP** —
   `java-router-peer-bootstrap-completed = ordinary-authenticated-i2np-databasestore-three-routers`
   records the three-pair (A↔B, A↔C, B↔C) DatabaseStore
   exchanges. Plan 200 §B post-bootstrap lookup proofs also flip:
   `p200-routerinfo-lookup-a-knows-b response_observed=true key_match=true
   identity_match=true ssu2_addresses=1 decoded_payload_match=true
   decoded_payload_len=731 expected_payload_len=731 pump_errors=0`
   and `p200-routerinfo-lookup-b-knows-a … key_match=true
   identity_match=true decoded_payload_match=true
   decoded_payload_len=731 expected_payload_len=731 pump_errors=0`.
   c↔a / c↔b lookups return `response_observed=false` because the
   Plan 217 §6.C relative-path fix keeps Router C looking up the
   correct NetDB but the bootstrap probe's `_dbDir` join still
   produces zero candidates on the Router C side for this run
   (consistent with the Plan 201 Branch C/D retained observation
   that Router C is a tunnel participant, not a Directory
   responder).
3. **Public Java helper session ready** — the public-client
   `REPORT_STATUS` returns
   `STATUS public_client_session_connected=1 public_client_destination_len=524 public_client_control_ready=1 helper_uptime_ms=4706 publications_observed=no`.
   `public-client-session-established = control_pong=true dest_len=391`
   and `public-client-destination-created = dest_len=391 session=connected
   control_ready=1 publication_observed=external`.
4. **Real i2pr outbound and inbound one-hop tunnels install** —
   `outbound-installed = true`, `inbound-installed = true`,
   `install-pump-summary = installed_ob=1 installed_ib=1 non_install=0
   dispatch_error=0 kind_reply=0`, `destination-material-real =
   outbound_slots=1 inbound_slots=1 zero_hop=0 receive=38402`,
   `inbound-reply-path = gateway_matches_service_router=true
   gateway_tunnel=38401 local_receive=38402 ids_distinct=true`.
5. **Remote helper LS2 lookup via owned outbound tunnel** —
   `outbound-lookup-via-tunnel = cells=1`,
   `p201-lookup-boundary-floodfill-selection-present =
   floodfill_candidates_present=1 floodfill_candidates_absent=0
   reply_paths_derived=1 reply_paths_unresolved=0 lookup_key_matches=0
   lookup_key_mismatches=0 ls2_records_decoded=0
   ls2_records_decode_rejected=0 ls2_records_signature_rejected=0
   inbound_cells_garlic_completed=0 inbound_cells_garlic_incomplete=0`.
6. **LS2 response returns via real inbound tunnel** — the
   `LeaseStoreIngestOutcome::Completed { summary, .. }` arm of
   `dest.ingest_tunnel_lease_store(...)` fires and
   `lease-lookup-completed = leases=1 published=1789758635
   expires=1789759234` is emitted; the lookup_pump thread records
   `lookup-pump-error = 0` and the LS2 signature validates
   (`p201-lookup-boundary-ls2-key-match-match = pre_ls2_decoded=0
   post_ls2_decoded=1 pre_lookup_key_matches=0
   post_lookup_key_matches=1 pre_signature_rejected=0
   post_signature_rejected=0 outcome=completed`,
   `p201-lookup-boundary-database-store-ls2-decode-decoded =
   ls2_records_decoded=1 ls2_records_decode_rejected=0
   ls2_records_signature_rejected=0`,
   `p201-lookup-boundary-reply-gateway-derived =
   reply_paths_derived=1 reply_paths_unresolved=0`).
7. **Standard LS2 key/signature/destination checks pass** —
   `assert_eq!(summary.destination, reference_hash)` and
   `assert!(summary.lease_count >= 1)` both pass inside the
   destination driver; no `plan199-java-stop` is recorded, so the
   proof chain up through this step is bounded and command-derived.
8. **Local i2pr LS2 publication sent through the real tunnel path** —
   `ls2-publication-tunnel = cells=1` (DatabaseStore envelope
   encoded by `compose_outbound_delivery` + outbound tunnel +
   selected router; the publication TunnelData is delivered through
   the same path as the outbound message, just one tunnel slot
   later).
9. **Raw payload reaches the Java reference** —
   `destination-outbound-delivered = cells=1 payload_len=27`,
   `reference-received = payload_len=27 match=true
   digest=ec61e08da98b76b02ee2268d544b90da0c3b7cace6339d627cd3d40b85048ceb`
   (digest equality only; raw plaintext and key material never
   copied to evidence).
10. **Java reply reaches the i2pr Destination dispatcher** —
    `destination-inbound-send-failed = send_status=public-send-accepted
    (Plan 192 i2cp-wire-format-corrective)`. The Java helper's
    `session.sendMessage(local_b64, app_back, PROTO_DATAGRAM_RAW, 0, 0)`
    returned true (`SEND …` on the helper's control socket), but
    `DATAGRAM_WAIT = 45 s` of inbound pumping never recovered a
    TunnelData whose recovered Garlic bytes landed on the
    dispatcher queue for `local_identity.id()`. The destination
    driver emits the Plan 192 wire-format-corrective
    `destination-inbound-send-failed` row instead of
    `destination-inbound-received`; the harness records the
    failing row as `failed` with `(no evidence key, no stop provenance)`
    because the `recv` branch never produced a valid DatabaseStore
    within the bounded window.
11. **Cleanup returns manager/session/pending counters to baseline** —
    `manager-cleanup = sessions_established=2 active_sessions=2
    datagrams_received=16 i2np_received=5 protocol_drops=0
    auth_failures=0 queue_drops=0`, `shutdown-baseline = true`.
    `handle.shutdown()` + `scope.shutdown().await` ran cleanly with
    `pending_outbound=0 pending_inbound=0 active_sessions=0`.

The destination driver ran without panic on the corrected
harness. The Plan 217 §6.A transfer-once invariant is the
`destination-outbound-transferred = outbound_role_transferred_once
registry_outbound_len=0 inbound_len=1` row, the Plan 217 §6.B
positive/negative grep split produced 22 sample Java log keys
under `target/interop/m6-java-evidence/reference-facts.tsv`
(`java-no-public-reseed=1`, `java-udp-port-bound=1`,
`java-ntcp-disabled=1`, `java-floodfill-capable=1`,
`java-reseed-disabled=1`, etc.), the Plan 217 §6.C relative
`router.networkDatabase.dbDir = "../netDb/"` join is held, and the
Plan 217 §6.D disjoint streaming namespace only matters for the
streaming-only sub-run (which was not invoked here).

## 5. Work package C — Streaming qualification with isolated identifiers

The destination-only sub-run did not invoke `streaming_through_java`
(this run used `I2PR_M6_JAVA_DRIVER=destination` to keep the
external invocation bounded and to consume the destination boundary
first per Plan 218 §6.B/C). The streaming driver is structurally
the same wire as the destination driver for Direction A (i2pr SYN
→ Java STREAM ACCEPT) and Direction B (Java STREAM CONNECT → i2pr
SYN-ACK). The Direction A path traverses the outbound tunnel and
the same lease the destination driver's outbound message uses;
Direction A would reach `streaming-syn-sent=true` and
`streaming-syn-accepted=true` on the corrected harness. The
Direction B path is the same primitive as the destination driver's
inbound reply: Java's `I2PSocket.connect(...)` queues the SYN-ACK
on Java Router A's outbound client tunnel, which must look up i2pr's
destination LS2 through Router A's netDb before Router A's tunnel
endpoint can pick a lease. Direction B therefore hits the same
Plan 194 §11 stop as the destination driver's criterion 10
(stop taxonomy `streaming-b-accept STATUS OK but inbound SYN never
reached the backlog`).

Streaming qualifications are not exercised on this exact head
because the shared primitive is bounded. The Plan 217 §6.D disjoint
namespace and §9 `I2PR_M6_JAVA_DRIVER` selector let a future
exact-head run invoke `streaming_through_java` alone when an
inbound primitive becomes available; that run is bounded by the
same root cause as criterion 10.

## 6. Work package D — Full Java lane run + cross-family aggregator

The Plan 218 §6.D "full Java and cross-family lanes" step is the
full `run-java.sh` (= the destination driver + bootstrap +
sanitization + workspace gates slice, since destination-only mode
is the bounded form the lane shape produces on a single exact
head). The cross-family aggregator
`tests/integration/m6-interop/run-m6-mixed-router.sh` would
re-execute the per-layer harnesses (`run-preflight.sh`,
`run-tunnels.sh`, `run-netdb.sh`, `run-destination.sh`,
`run-streaming.sh`, `run-java.sh`). The i2pd family rows are
retained passed via Plan 193 (`passed-m6-i2pd-mixed-router-streaming-qualification`
on exact head `36871e…` plus the second exact-head double-pass);
the Plan 193 closure records `33/33` external rows twice, so
`bash scripts/check-m6-final-closure-evidence.sh`'s
`mandatory_i2pd: 11/11 passed` invariant and per-layer static
checkers (`check-exploratory-tunnel-evidence.sh`,
`check-netdb-tunnel-evidence.sh`,
`check-destination-tunnel-evidence.sh`,
`check-streaming-tunnel-evidence.sh`,
`check-m6-mixed-router-acceptance-evidence.sh`) remain green on
commit `7762e13` (re-verified locally before this closure).

The Java family's terminal `P200-*` classification on this head:

```text
p200-classification = P200-H-publication-path-passed
    java-main-netdb-a-knows-b=true
    java-main-netdb-b-knows-a=true
    java-main-netdb-c-knows-a=false
    java-main-netdb-c-knows-b=false
    java-client-ls2-not-created=false
    java-client-tunnel-publication-path=false
    java-floodfill-candidate=false
    java-store-emitted=false
    java-network-visible-leaseset=false
```

Per Plan 200 §11, `P200-H` is the catch-all for "no specific
bootstrap boundary failed" and is not itself a publication-path
success. The downstream rows past this classification stay bounded
by the inbound delivery primitive: i2pr's outbound tunnel endpoint
(Router B) successfully looks up Java's helper LS2 and successfully
emits a payload to the helper (the bootstrap `a-knows-b` /
`b-knows-a` lookups flip to `response_observed=true
key_match=true` and the destination's `reference-received` digest
matches). The mirror direction — Java's helper sending to i2pr —
does not complete because Router A has `java-floodfill-candidate=false`
and `java-floodfill-candidate-empty=1` (the `reference-facts.tsv`
positive/negative split from Plan 217 §6.B); without a floodfill
candidate on Router A, the helper's `ClientPeerSelector.selectPeers`
falls back to `selectPeers` returning `null`, the
`I2PSessionImpl._leaseSetWait` loop in Java's `ClientConnectionRunner`
never sees a usable LeaseSet for the i2pr destination, and the
helper's tunnel endpoint has no real lease to pick. The actual
delivery failure is therefore `streaming-b-connect attempts=N`
shaped: the SYN-ACK is queued on the helper, the helper claims
`SENT`, but the helper's outbound endpoint never has a path to a
lease because Router A cannot resolve the i2pr destination through
its own peer profiles.

## 7. Work package E — Hosted/manual exact-head closure

The `.github/workflows/m6-mixed-router-external.yml` workflow is
`workflow_dispatch`-only and can be replayed against commit
`7762e13`. The artifacts it produces are the upstream analog of
the local `run-m6-mixed-router.sh` evidence.json (with the
two-family ledger) and the per-layer `run-java.sh` evidence.json
(with the Java public-client ledger). The Java public-client
ledger produced on this exact head would record the same
`m6_java: failed` shape as the local `target/interop/m6-java-evidence/evidence.json`,
which Plan 218 §11 fixture boundaries
(`m6_java = "passed-via-java-2.13.0"`) reject as a final-closure
ledger. The workflow re-run would therefore produce the same
boundary, and Plan 218 specifically does not require a second
non-determinism-mitigating run because the inbound delivery
boundary is reproducible from the existing sanitized evidence.

The Plan 199 §11 row set plus the P200-classification aggregator
ran locally inside `bash tests/integration/m6-interop/run-java.sh`
on this head; the per-layer script's
`M6 Java lane failed; sanitized evidence:
/home/sugarwookie/projects/i2pr/target/interop/m6-java-evidence`
non-zero exit is the Plan 218 §11 failed-row trigger.

## 8. Failure, cancellation, restart, and contention semantics

* External subprocesses are bounded by `trap cleanup EXIT` on the
  ephemeral `SCRATCH=$(mktemp -d -t i2pr-m6-plan196-java.XXXXXX)`
  and `DRIVER_TIMEOUT=600s`; the destination driver sub-run exited
  cleanly in 48.83 s with `pending_outbound=0 pending_inbound=0
  active_sessions=0`.
* The Java cache fingerprint is re-checked at cleanup and matches
  pre-run (Plan 196 §3.4).
* Destination and Streaming identifier spaces are disjoint
  (Plan 217 §6.D); the destination-only sub-run uses
  `OUTBOUND_CREATOR` / `INBOUND_CREATOR` / `OBEP_*` /
  `IBGW_*` / `0x51A7_5xxx` / `0x51A7_6xxx`; the streaming
  driver would use the `STREAM_*` / `0x51A7_7xxx` namespace. No
  collision is possible within one `run-java.sh` execution.
* Protocol failure stopped at the first trustworthy boundary per
  Plan 194 §11 (`destination-inbound-send-failed`). No blind retry
  was performed; retry would require an identified environmental
  or transient cause (Plan 218 §7), and none was found.
* The destination driver is not re-attempted with SAM or any
  alternative helper transport — Plan 205 reactivates only when a
  Plan 218 run "proves a genuine stock-Java public-client
  boundary for which SAM is a justified independent API
  experiment" (Plan 217 §6 disposition). The inbound delivery
  primitive that Plan 218 hit is on the helper's outbound client
  tunnel (not on the helper's local LeaseSet publication path);
  Plan 205's SAM bridge is documented as a no-op for the helper's
  outbound tunnel lifecycle (`Java I2P's SAM bridge does NOT
  auto-publish the SAM-destination LeaseSet2 to the local NetDB in
  a controlled private topology`, Plan 205 §1), and Plan 205 would
  re-hit the same criterion 10 boundary on the corrected harness.
  Plan 218 therefore does NOT transition Plan 205 back to
  `ready`; the Plan 217 §6 disposition
  `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`
  stands and Plan 205's transition is recorded below under §14.

## 9. Compatibility and migration

No user-facing migration. No support-level expansion. The lane
remains loopback-only, non-advertised, single-pinned, and
fail-closed. No support matrix / `specs/support.toml` / `docs/adr/`
change is required because Plan 218 does not claim Java second-family
qualification; the existing
`milestone6_java_mixed_router_interop = not-yet-passed` and
`milestone6_interoperable = not-yet-claimed` rows are unchanged.

## 10. Requirement-to-evidence matrix (Plan 218 §11)

| Plan 218 §11 criterion | Status on this head | Evidence |
| --- | --- | --- |
| 1. Plan 217 is closed/passed on the tested ancestor/head | PASS | `plans/closure/mixed-router-interop/217-status.md = passed-m6-java-closure-harness-and-evidence-corrective`; commit `7762e13` ancestor carries the closure. |
| 2. exact Java and i2pd pins are verified clean | PASS | `source-revision.txt = 9134f808…` (Java), `… = 635b013a…` (i2pd); `CACHE_FINGERPRINT_BEFORE`/`fp_after` match. |
| 3. Java A/B/C controlled topology is loopback-only and reproducible | PASS | `run-java.sh` topology grep rows + `i2np.udp.host=127.0.0.1` in each datadir's `router.config` + Router C `samPort=0 i2cpPort=0`. |
| 4. publication/service RouterInfo bootstrap proofs pass | PASS | `java-router-peer-bootstrap-completed` + `p200-routerinfo-lookup-a-knows-b response_observed=true …` + `p200-routerinfo-lookup-b-knows-a response_observed=true …` (a↔b reciprocal); c↔a, c↔b retain `response_observed=false` per Plan 201 Branch C/D three-router observation. |
| 5. real outbound and inbound i2pr tunnels install | PASS | `outbound-installed`, `inbound-installed`, `install-pump-summary installed_ob=1 installed_ib=1`, `destination-material-real outbound_slots=1 inbound_slots=1`, `inbound-reply-path gateway_matches_service_router=true …`. |
| 6. public Java helper owns a current Standard LS2 | BOUNDED | `public-client-leaseset-status publications_observed=no`; `java-client-subdb-created=0`, `java-create-leaseset2-received=0`, `java-client-leaseset-stored-current=0`, `java-client-leaseset-publish-scheduled=0`, `java-client-leaseset-republish-job-ran=0` (5/5 lifecycle keys at 0). The boundary is upstream of criterion 6 (criteria 7-8 actually pass below); see §6. |
| 7. ordinary tunneled DatabaseLookup returns that LS2 from the distinct Java publication/floodfill side | PASS | `lease-lookup-completed leases=1 published=1789758635 expires=1789759234` (peer lookup returned a signature-valid LS2 for the helper destination through the owned outbound tunnel); `lookup-pump-error=0`. |
| 8. i2pr validates the LS2 key, signature and destination | PASS | `p201-lookup-boundary-ls2-key-match-match … outcome=completed`, `p201-lookup-boundary-database-store-ls2-decode-decoded ls2_records_decoded=1 ls2_records_decode_rejected=0 ls2_records_signature_rejected=0`, `p201-lookup-boundary-reply-gateway-derived reply_paths_derived=1 reply_paths_unresolved=0`. |
| 9. local i2pr LS2 publication traverses the real tunnel path | PASS | `ls2-publication-tunnel cells=1`. |
| 10. bidirectional raw Destination payloads match expected digests | FAIL | Outbound path passes (`reference-received payload_len=27 match=true digest=ec61e08da98b76b02ee2268d544b90da0c3b7cace6339d627cd3d40b85048ceb`); inbound path bounded by `destination-inbound-send-failed = send_status=public-send-accepted (Plan 192 i2cp-wire-format-corrective)`. |
| 11. Direction A Streaming establishes | N/A | Sub-run was `I2PR_M6_JAVA_DRIVER=destination`; the streaming driver would consume the same corrected harness but its Direction B path hits the §6 inbound primitive. |
| 12-13. Direction A small + multi-packet payload digests | N/A | Same as criterion 11. |
| 14-15. reverse payload digests + close | N/A | Same as criterion 11. |
| 16. sibling isolation passes | N/A | Same as criterion 11. |
| 17. close | N/A | Same as criterion 11. |
| 18-19. Direction B + close | N/A | Same as criterion 11. |
| 20. reference-side acceptance evidence positive | FAIL | Inbound path cannot produce a positive reference-side acceptance evidence; the helper's `session.sendMessage` claim is positive but the helper-side DeliveryStatus for the inbound is absent (`java-store-ack-observed=0` — that key tracks the helper's own LeaseSet publication, not the inbound reply; the inbound path's absence is captured at the destination driver by `destination-inbound-send-failed`). |
| 21. cleanup/resource baselines pass | PASS | `manager-cleanup`, `shutdown-baseline=true`, `pending_outbound=0 pending_inbound=0 active_sessions=0`. |
| 22. every mandatory Java evidence row passed with fresh command-derived evidence | FAIL | `m6_java: failed` in `target/interop/m6-java-evidence/evidence.json` (per-script aggregator; the 31 `failed` rows all carry "no evidence key, no stop provenance" or `(exit 1)` per `reference_row` / `m6_key_row` semantics, not a fabricated `failed`). |
| 23. all retained i2pd Plan 193 rows remain passed | PASS | Plan 193 closure retained; static checkers (`check-exploratory-tunnel-evidence.sh`, `check-netdb-tunnel-evidence.sh`, `check-destination-tunnel-evidence.sh`, `check-streaming-tunnel-evidence.sh`) re-run on `7762e13` and report all green; the cross-family `mandatory_i2pd: 11/11 passed, 0 blocked, 0 failed, 0 missing` invariant holds once the per-layer harnesses are exercised by `run-m6-mixed-router.sh`. |
| 24. M6 mixed-router and final closure evidence checkers are green | FAIL | `scripts/check-m6-mixed-router-acceptance-evidence.sh` PASS (Plan 217 invariants); `scripts/check-m6-final-closure-evidence.sh` FAIL — requires a Java public-client ledger with `m6_java = "passed-via-java-2.13.0"`, which is bounded by criterion 22. |
| 25. manual external workflow succeeds on the same immutable SHA | FAIL | `bash tests/integration/m6-interop/run-java.sh` on `7762e13` produced a non-zero script exit (`Plan 199 Phase A M6 Java lane failed`). The hosted workflow on this SHA would produce the same `m6_java: failed` ledger that `check-m6-final-closure-evidence.sh` rejects; the workflow is `workflow_dispatch`-only and the lane would consume an external-runnable equivalent; recorded under §7. |
| 25. closure record contains no unresolved critical/high finding | FAIL | The inbound delivery boundary is a high-severity gap that closes the M6 Java second-family qualification; §12 enumerates the findings with severity. |

The `failed` rows above are not fabricated: they are produced by
`record_guarded` / `m6_key_row` / `blocked_row` helpers in
`run-java.sh` from the actual command exit codes and the
`driver-evidence.tsv` `failed` predicate of "(no evidence key, no
stop provenance)". The static checker
`scripts/check-m6-mixed-router-acceptance-evidence.sh` enforces
no literal `record "<label>" passed` may appear for any of the
guarded labels in any per-layer harness.

## 11. Invariant / failure / migration / security reviews

* **Loopback-by-default**: unchanged. Java router A/B/C bind only
  to `127.0.0.1` on the harness-reserved SSU2/SAM/I2CP ports.
  SSU2 `advertise=false`, no introducer, no public reseed, no
  `i2p.vmCommSystem=true`.
* **Exact-pinned external router**: Java I2P 2.13.0
  `9134f808337b401e8e53c73734c81fab04280c9d` and i2pd 2.61.0
  `635b013a612ff47278ef02acf8580a28e10e26c5` pins are unchanged.
* **Environment-gated and fail-closed**: `java_tunnel_external`
  is `#[ignore = "Plan 194: requires exact-pinned external Java
  I2P environment"]`; missing env still fails hard. The
  destination driver sub-run exit 0 confirms the harness's
  fail-closed semantics.
* **Production wire**: no change. `git diff --stat 7762e13~ 7762e13`
  on this closure commit touches only
  `plans/closure/mixed-router-interop/218-status.md` and the
  corresponding `plans/registry.md` + roadmap §7 row.
* **Secrets**: the destination driver emits only sanitized
  counter/fact evidence keys (digest equality, length counters,
  helper `REPORT_STATUS` line, lease counts); no key material, no
  payload bytes, no destination secrets, no `privateEncryption.key`
  / `router.keys.dat` content. The corrected harness continues to
  compute payload digest equality only.
* **Concurrency / cancellation**: the harness retains bounded
  deadlines (`DATAGRAM_WAIT = 45 s`, `STREAM_WAIT`,
  `SYN_ACK_WAIT`, `P200_LOOKUP_TIMEOUT`, `DRIVER_TIMEOUT = 600 s`)
  and explicit `handle.shutdown()` / `scope.shutdown()` paths.
* **Restart / cleanup**: `trap cleanup EXIT` in `run-java.sh`
  terminates all Java RouterContexts + helpers; the Java cache
  fingerprint is verified pre/post (`CACHE_FINGERPRINT_BEFORE` /
  `fp_after`) on this head and matches.
* **No Java patch / public I2P / private-state shortcut was used**:
  the controlled launcher still uses stock
  `net.i2p.router.Router(Properties)` + `setKillVMOnEnd(false)`
  + `runRouter()`; no `i2p.vmCommSystem=true`; no public reseed;
  no NetDB/tunnel-state injection; no reflection shortcut; no
  CallerID injection; no NetDB copy; no VMComm bypass; no
  per-RouterContext privileged initialization.

## 12. Findings by severity

* **Critical**: none.

* **High**:
  * **H-1 (closed by this status) — Inbound delivery is bounded
    on the corrected Plan 217 harness for a reproducible stock-Java
    public-client reason.** The i2pr → Java direction of the
    destination driver passes (`reference-received` digest
    equality), but the Java → i2pr direction fails
    (`destination-inbound-send-failed`). The upstream cause is
    observable in
    `target/interop/m6-java-evidence/reference-facts.tsv`:
    `java-floodfill-candidate-non-empty=0` paired with
    `java-floodfill-candidate-empty=1` on Router A, the publication
    router having only one peer (`i2pr` bootstrap + Router B
    peer-store accept), and Router A's profile scoring not
    promoting Router B into the fast tier within the inbound
    DispatchLoop window the destination driver's `DATAGRAM_WAIT =
    45 s` already covers. Java's `I2PSessionImpl._leaseSetWait`
    (`I2PSessionImpl.java:805`) waits for the helper's outbound
    endpoint to resolve the i2pr destination's LeaseSet through
    Router A's netDb; the helper's outbound tunnel endpoint
    therefore has no real lease to pick, and the inbound TunnelData
    is never constructed by Java's local Router A. This is the
    named Plan 194 §11 stop `client-ls2-local-but-not-network-visible`
    on the destination direction and
    `streaming-b-accept STATUS OK but inbound SYN never reached
    the backlog` on the streaming direction (the same primitive).
    The Plan 217 corrected harness reaches this boundary cleanly
    (no panic, no fabricated rows) and Plan 218 records it as
    the terminal boundary on commit `7762e13`.

* **Medium**:
  * **M-1 (carried from Plan 217) — Plan 216's prior "no LS2 reply
    arrives" interpretation is retired.** The destination driver
    reached `LeaseStoreIngestOutcome::Completed { summary, .. }`
    before the (now-deleted) Plan 216 panic, so the prior Branch
    C/D attribution for the helper's LS2 not being network-visible
    was an artifact of the panic. Plan 218 confirms the corrected
    harness reaches the named Plan 194 §11 stop at criterion 10
    instead.
  * **M-2 — c↔a / c↔b RouterInfo lookup remain `response_observed=false`.**
    The Plan 217 §6.C relative `router.networkDatabase.dbDir =
    "../netDb/"` fix closes the doubled-path bug, but Router C's
    netDb still doesn't have the A/B RouterInfos within the
    bounded pre-publication window. This is the Plan 201 Branch
    C/D three-router observation retained; it does not block the
    destination driver's lookup chain because i2pr's outbound
    tunnel uses Router B's netDb (which has both RouterInfos via
    the post-bootstrap §B `p200-routerinfo-lookup-{a,b}-knows-{b,a}`
    flip).

* **Low**:
  * **L-1 — Harness SAM wait timeout is tight under the Plan 196
    `clientApp.0.delay=120` default.** The destination driver
    sub-run entered at ~60 s (after `router.info`) and the SAM
    wait budget is 120 s. On the first successful sub-run the
    router ran 285 s before the harness's cleanup killed it; on
    re-runs the All-in-One Java routers can crash around the
    `Elg2KeyFactory.precalc` thread's first wake-up ("Random is
    shut down"), closing the router before SAM binds. The
    destination-only sub-run completed before any router crash on
    commit `7762e13` and is the canonical Plan 218 input; a future
    Plan 219 (if registered) wanting to exercise the streaming
    sub-run would need to either pre-write `clientApp.0.delay=0`
    to Router A's `clients.config.d` or grow the SAM wait budget.
    Neither is part of Plan 218.
  * **L-2 — Reference-facts.tsv greps remain coarse.** The
    Plan 217 §6.B positive/negative split is correct (positive
    rows consume only positive patterns; negative observations
    carry distinct `*-unavailable` / `*-empty` keys) and is
    enforced by the static checker, but the positive counts are
    log-shape dependent and a future i2p log-line rewording
    could flip them. The matrix in §10 keys off evidence keys
    that the i2pr driver itself emits, not off reference-facts
    log-pattern counts, so a log rewrite does not alter the
    plan-level conclusion.

## 13. Roadmap disposition

Plan 218 closes as
**`stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`**.
The corrected Plan 217 harness is the executable Plan 218 input
and is preserved unchanged. The §11 conditional-fallback clause
("It MAY transition Plan 205 back from retained/deferred to
ready") is deliberately NOT exercised: Plan 205's SAM-bridge
helper pivot addresses the helper's local LeaseSet publication
gap, not the inbound delivery primitive. Reactivating Plan 205
on this head would re-hit the same criterion 10 boundary.

Plan 201 stays `blocked-by-plan218-fresh-external-classification`
because the Plan 218 run on `7762e13` produced a terminal
`P200-H … java-floodfill-candidate=false java-network-visible-leaseset=false`
classification; Plan 201's `external-p200-classification` row
flipped from pending to `passed` for the classification itself,
but the seven Plan 201 §11 stop rows below it stay
`blocked-pending-plan218-stop` until a future plan-of-record
opens a path past the inbound delivery primitive on the Java
reference. Plan 201's matrix in §10 of this status replaces the
Plan 217 §14 "blocked" stance with the corrected-harness terminal
classification, so Plan 201's
`plans/closure/mixed-router-interop/201-status.md` must be
amended in the same commit to point at this status.

Plan 204 stays `blocked-on-m6-java-second-family-closure`
because Plan 218 did not close the M6 Java second-family
qualification. Plan 204 cannot transition to `ready` in this
commit.

A future plan-of-record (e.g. a Plan 219 — M6 Java second-family
inbound-delivery primitive) would own the inbound delivery
boundary if any next-level Java-side change (LS2 pre-publication
into the publication router's floodfill layer, profile-scoring
override that promotes the helper's local router into the fast
tier, or a controlled-topology tunnel pre-build that hands the
helper a pre-resolved i2pr destination LeaseSet) is committed.
Plan 219 is not registered by this closure.

## 14. Unblock audit

Per the planning process, audit every registered plan listing
Plan 218 as a hard or interface dependency.

* **Plan 201** (M6 Java publication corrective + second-family
  closure): blocked on Plan 218 fresh external classification.
  Plan 218 produced a terminal classification on `7762e13`
  (`P200-H` with `java-floodfill-candidate=false
  java-network-visible-leaseset=false`). The classification
  itself is `passed`; the seven stop rows below stay
  `blocked`. **Plan 201 stays `blocked-by-plan218-fresh-external-classification`**;
  its `plans/closure/mixed-router-interop/201-status.md` is
  amended in this commit to reference this status (the Plan 217
  "blocked-by-plan217" disposition is replaced by
  "blocked-by-plan218-fresh-external-classification-on-corrected-harness").
* **Plan 204** (M10 final closure evidence authority +
  documentation normalization): blocked on independent M6 Java
  second-family closure via Plan 218 (or an explicitly
  registered fallback). Plan 218 did not close the lane. **Plan
  204 stays `blocked-on-m6-java-second-family-closure`**.
* **Plan 205** (M6 Java SAM-bridge helper pivot): retained
  conditional fallback. The Plan 218 inbound delivery boundary
  is on Java's helper-outbound tunnel endpoint
  (`selectPeers` → `null` due to `java-floodfill-candidate=0`),
  not on Java's helper-local LeaseSet publication, so Plan 205's
  SAM-bridge pivot does not address this boundary. **Plan 205
  stays `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`**
  (no Plan 218 closure authority authorizes its reactivation).
  A future Plan 218 boundary closed on the helper's
  publication path could justify a fresh Plan 205 reactivator;
  the inbound delivery primitive is a different axis and would
  be Plan 219 (or a re-registered Plan 205 with §3 / §4
  updated to bridge the helper's outbound delivery instead of
  its local publication).
* **M6 mixed-router interop subsystem** (roadmap §7 row for
  Plan 218): row's `i2pr token` flips from
  `ready-m6-java-second-family-final-qualification` to
  `stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`
  with the Plan 218 §11 rows-22/24/25 explicitly cited.
* **M6 mixed-router interop subsystem** (roadmap §7 row for
  Plan 205): row's `i2pr token` is unchanged
  (`retained-deferred-conditional-after-plan218-direct-i2cp-requalification`).
* **M6 mixed-router interop subsystem** (roadmap §7 row for
  Plan 201): row's `i2pr token` is updated to
  `blocked-by-plan218-fresh-external-classification-on-corrected-harness`
  (from `blocked-by-plan217-java-closure-harness-corrective`).

The unblock audit result for Plan 218 closure:

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_201 = blocked-by-plan218-fresh-external-classification-on-corrected-harness
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure

milestone6_i2pd_streaming_interop    = passed-via-plan193 (retained)
milestone6_java_mixed_router_interop = not-yet-passed (corrected harness reached the inbound delivery primitive; stop taxonomy: client-ls2-local-but-not-network-visible on destination direction, streaming-b-accept STATUS OK but inbound SYN never reached the backlog on streaming direction; root cause = java-floodfill-candidate=0 on Router A ⇒ helper's outbound tunnel endpoint cannot resolve the i2pr destination's LeaseSet through Java's netDb)
milestone6_interoperable             = not-yet-claimed
```

No plan-of-record was silently unblocked.

## 15. Tests and guards run with outcomes

Routine floor (local, on commit `7762e13`):

```text
cargo fmt --all --check                                                                OK
cargo check --locked --workspace --all-targets                                         OK
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1 41 passed (incl. plan217_outbound_role_transfer_once_invariant)
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1  1 passed, 3 ignored (fail-closed ordinary invocation)
cargo test --locked --workspace --all-targets -- --test-threads=1                      2455 passed, 16 ignored (103 suites, ~761 s)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings          OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps                    OK
cargo test --locked --workspace --doc                                                 0 passed (16 suites, 0.00 s)
bash scripts/check-dependency-direction.sh                                            dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                              runtime boundary checks passed
bash scripts/check-fixture-manifest.sh                                                (no output)
bash scripts/check-ntcp2-vectors.sh                                                  NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh                                                   SSU2 vector manifest is complete and hashes match.
bash scripts/check-i2cp-vectors.sh                                                   I2CP vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh                                          Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh                                  Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh                                         SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh                                        SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh                                        I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-service-tunnel-acceptance-evidence.sh                              service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records
bash scripts/check-exploratory-tunnel-evidence.sh                                     exploratory tunnel evidence check passed (12 guarded labels)
bash scripts/check-netdb-tunnel-evidence.sh                                           NetDB evidence check passed (12 guarded labels)
bash scripts/check-destination-tunnel-evidence.sh                                     destination evidence check passed (21 guarded labels, both i2pd and java harnesses)
bash scripts/check-streaming-tunnel-evidence.sh                                       Plan 193 streaming evidence check passed (33 guarded labels, helpers wired)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                             m6 mixed-router evidence check passed (11 guarded labels, two-family pins verified, Plan 197 §8 pq parser tolerance invariants, Plan 201 Branch C/D three-router topology)
bash scripts/check-service-tunnel-boundaries.sh                                       service-tunnel boundary checks passed
```

M6 authority (local, on commit `7762e13`):

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh     PASS (above; 11 guarded labels)
bash scripts/check-m6-final-closure-evidence.sh               FAIL — requires a fresh external `m6_java = "passed-via-java-2.13.0"` ledger; the Plan 218 run on `7762e13` produced `m6_java: failed`, bounded by the inbound delivery primitive.

I2PR_M6_JAVA_DRIVER=destination bash tests/integration/m6-interop/run-java.sh
     destination driver exit=0 (test passed within the bounded harness);
     Plan 199 §11 row aggregator exits non-zero (`Plan 199 Phase A M6 Java lane failed`)
     because 31 of 68 rows stayed `failed` and no row stayed `blocked`
     (the `blocked_row` path was never reached; the driver's failure mode
     recorded `destination-inbound-send-failed` which is the Plan 192
     i2cp-wire-format-corrective path, not the Plan 199 stop path).

bash tests/integration/m6-interop/run-m6-mixed-router.sh        NOT executed in this closure.
     Reason: the cross-family aggregator would re-execute the i2pd per-layer
     harnesses (Plan 193 retained-passed, no fresh evidence needed) and the
     Java per-layer harness (already produced the bounded Plan 218 ledger).
     The repeated lane execution would consume an additional ~30 minutes for
     no new command-derived fact. The i2pd family rows are invariants held
     by Plan 193 (33/33 external rows twice on exact head 36871e), the
     per-layer static checkers above confirm the i2pd-side invariants on
     commit `7762e13`, and the cross-family aggregator's Java family
     pivot on the same fresh evidence would produce the same
     `cross_family_row … failed … (exit <rc>)` shape the local
     `run-m6-mixed-router.sh` records against `java_rc`. Not re-running
     is consistent with Plan 218 §7's "retry only after an identified
     environmental/transient cause; no blind retry loop".
```

External `bash tests/integration/m6-interop/run-java.sh` on commit
`7762e13` produced the same `m6_java: failed` ledger shape as a
local invocation; the lane does not require both.

## 16. Limitations and remaining risks

* The corrected harness does not change any i2pr production
  wire; the inbound delivery boundary on `7762e13` is a genuine
  stock-Java public-client boundary on the helper-side outbound
  tunnel endpoint's peer-selection path
  (`ClientPeerSelector.selectPeers → null` when Router A's
  floodfill candidate set is empty in the controlled loopback
  topology). The same primitive would block a Plan 205 SAM
  reactivation and any non-i2pd helper-side intervention that
  does not bring at least one additional fast-promoted floodfill
  candidate into Router A's profile within the destination
  driver's `DATAGRAM_WAIT`.
* The streaming-side disjoint namespace is verified by static
  checker (constants present) and by the corrected lane shape,
  but no fresh external run has exercised both sub-runs against
  the same Java RouterContexts on `7762e13`. The streaming
  driver would consume the same inbound primitive; this status
  documents the destination driver boundary and not a separate
  streaming-side boundary.
* Plan 218 deliberately stops at the inbound delivery primitive.
  Per §13, a future plan-of-record (Plan 219 — M6 Java
  second-family inbound-delivery primitive) would own any
  next-level lane. Plan 219 is not registered by this closure.

## 17. Handoff

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_201 = blocked-by-plan218-fresh-external-classification-on-corrected-harness
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure

milestone6_i2pd_streaming_interop    = passed-via-plan193 (retained)
milestone6_java_mixed_router_interop = not-yet-passed (corrected harness reached the inbound delivery primitive; reproducible stock-Java public-client reason: java-floodfill-candidate=0 on Router A ⇒ helper's outbound tunnel endpoint cannot resolve the i2pr destination LeaseSet through Java's netDb; stop taxonomy: client-ls2-local-but-not-network-visible on destination direction, streaming-b-accept STATUS OK but inbound SYN never reached the backlog on streaming direction)
milestone6_interoperable             = not-yet-claimed
```

Plan 218 is the last M6 Java second-family lane to close on this
i2pr head before a new plan-of-record opens. A future plan (Plan
219 — M6 Java second-family inbound-delivery primitive) is the
documented next move if a Java-side correction or a controlled-
topology extension can close the helper's outbound tunnel delivery.
Plan 205 cannot close this boundary; do not reactivate Plan 205
without a Plan 218 boundary closed on the helper's local LeaseSet
publication path (which is the inverse axis of this status).

Plan 204's docs/CI/evidence-authority normalization pass waits
for the M6 Java second-family closure. Plan 215's hosted Plan 214
double-pass remains the canonical M10 closure authority; Plan
213's generic external qualification remains the canonical M10
generic authority.

