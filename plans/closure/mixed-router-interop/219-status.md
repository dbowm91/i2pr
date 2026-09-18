# Plan 219 status — M6 Java reverse-delivery root-cause investigation

Status: **`retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220`**.

## 2026-09-18 authority correction — Plan 220 supersedes the attribution

The Plan 219 loopback read-only diagnostic infrastructure is retained, but its
terminal `J219-B-A-STORED-B-RI-NOT-F` root-cause attribution is not
authoritative.

Post-closure review found the diagnostic defects enumerated in Plan 220,
including pre-bootstrap typed-fact sampling, invalid RouterHash derivation,
presence-only `stored-b-ri-has-f` semantics, synthesized selector output,
defaulted client-NetDB/OCMOSJ facts, wrong-direction dispatch evidence, and
non-exact implementation-SHA closure evidence.

The underlying Plan 218 behavioral fact remains authoritative: Java → i2pr
reverse delivery is absent on the corrected Plan 217 harness. Its first exact
failing layer is again **unknown pending Plan 220**.

```text
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = registered-ready-m6-java-plan219-diagnostic-attribution-corrective
next_executable_plan = 220-m6-java-plan219-diagnostic-attribution-corrective
```

The previous Plan 219 closure narrative is retained below for traceability and
MUST NOT be used as current root-cause authority.

## Retained superseded Plan 219 closure narrative

# Plan 219 status — M6 Java reverse-delivery root-cause investigation

Status: **`passed-m6-java-reverse-delivery-root-cause-attribution`**.

Plan of record:
[`219-m6-java-reverse-delivery-root-cause-investigation.md`](../../implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md).

## 2026-09-18 closure amendment — typed attribution

Plan 219 closed the harness/evidence-corrective gap that Plan
218 surfaced. The Plan 218 conclusion
(`java-floodfill-candidate-empty=1`) was a coarse global log
grep; it was sufficient to prove a reverse-delivery stop but
not sufficient to identify which Java-side boundary is first
failing. Plan 219 replaces that coarse attribution with a
bounded, sanitized, read-only typed observation surface that
consumes Plan 219 §6.E's 12 mandatory facts and derives one
`J219-{A..J}` terminal classification per run.

The investigation reaches **`J219-B-A-STORED-B-RI-NOT-F`** on
the closing head. Plan 218's "the helper's outbound tunnel
endpoint cannot resolve the i2pr destination's LeaseSet
through Java's netDb" is correct as a downstream symptom;
Plan 219 refines its root cause to:

> **Router B's live RouterInfo DOES advertise the `f`
> capability, but Router A's authoritative store does NOT
> have Router B's RouterInfo stored.** The bootstrap probe
> submits i2pr's signed RouterInfo to Routers A and B
> (`java-router-peer-bootstrap-completed =
> ordinary-authenticated-i2np-databasestore-three-routers`),
> but the inverse submission — Router B's signed RouterInfo
> into Router A's main NetDB — is not exercised on this
> lane. PeerManager's floodfill capability index on Router A
> thus indexes only Router A itself (1 candidate) and
> `FloodfillPeerSelector` excludes Router B in the
> post-ranking result set, yielding the
> `count=1, peer_0=<self>` shape the typed-facts snapshot
> records.

The downstream cascade the J219-B boundary produces — empty
PeerManager indexing, empty `FloodfillPeerSelector` result,
helper-side floodfill `ClientPeerSelector.selectPeers → null`,
helper's `OutboundClientMessageOneShotJob` lease loop
unsatisfied, inbound `TunnelData` never constructed — is
exactly the Plan 194 §11 stop the Plan 218 harness reached.

The Plan 219 classification chain stops at J219-B because
that is the earliest typed fact in the inventory that is in
its failing state. No propagation across J219-C..J is
needed for attribution; the J219-B correction is the
narrowest path.

## 1. Outcome

Plan 219 ran the corrected destination-only harness
(`I2PR_M6_JAVA_DRIVER=destination`,
`tests/integration/m6-interop/run-java.sh`) once on commit
`9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2` against the
exact-pinned Java I2P 2.13.0 cache
(`9134f808337b401e8e53c73734c81fab04280c9d`) and the
exact-pinned i2pd 2.61.0 cache
(`635b013a612ff47278ef02acf8580a28e10e26c5`).

The Plan 217 harness remains fail-closed at the inbound
primitive and Plan 218 still records the corrected run on
commit `7762e13`. Plan 219 narrows the attribution to one
typed J219-B boundary on the closing head. A corrective Plan
220 is not registered by this closure.

```text
i2pr_commit = 9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2
java_i2p    = 2.13.0 (9134f808337b401e8e53c73734c81fab04280c9d)
i2pd        = 2.61.0 (635b013a612ff47278ef02acf8580a28e10e26c5)
os_image    = Linux x86_64
rust        = 1.95.0 (59807616e 2026-04-14)
j219_classification = J219-B-A-STORED-B-RI-NOT-F
```

The Plan 219 §6.B 5-timed-snapshot timeline per router
(A/B/C), the Plan 219 §6.C 7-view-rows probe, the Plan 219
§6.D OCMOSJ correlation keys, and the Plan 219 §6.E 12
typed-facts derivation landed in
`target/interop/m6-java-evidence/j219/`. The Plan 219
terminal classification landed in
`target/interop/m6-java-evidence/driver/destination/driver-evidence.tsv`
as the `j219-classification` row.

## 2. Implementation commits

Plan 219 changes landed across a single focused working
state on commit `9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2`:

```text
crates/i2pr-daemon/src/destination_tunnels.rs              | 324 ++++++++-
crates/i2pr-daemon/tests/destination_tunnel_unit.rs        | 503 +++++++++++-
crates/i2pr-daemon/tests/java_tunnel_external.rs           | 178 ++++-
scripts/check-m6-mixed-router-acceptance-evidence.sh       | 175 ++++-
tests/integration/m6-interop/java/ControlledRouter.java   | 392 +++++++++-
tests/integration/m6-interop/run-java.sh                   | 348 ++++++++-
```

No production i2pr wire changed. No Java source patched. No
public I2P participation. No reference vendoring. No
VMCommSystem. No top-level `cargo` change.

## 3. WP A — typed-fact surface

`crates/i2pr-daemon/src/destination_tunnels.rs` gains a
bounded, sanitized, privacy-safe Plan 219 typed-fact
surface:

1. `DestinationTunnelCounters` (the existing §G-shaped
   counter snapshot) gains 12 new boolean / count fields
   documented in Plan 219 §6.E: `j219_b_live_ri_has_f`,
   `j219_a_stored_b_ri_has_f`, `j219_a_peermanager_b_indexed_f`,
   `j219_a_selector_input_count`, `j219_a_selector_result_count`,
   `j219_client_db_main_router_count`,
   `j219_client_db_lookup_started`,
   `j219_client_db_lookup_peer_selected`,
   `j219_client_db_lookup_result`,
   `j219_ocmosj_lease_selected`,
   `j219_ocmosj_outbound_tunnel_selected`,
   `j219_ocmosj_dispatch_submitted`. Each field has a
   Default-derived `false` / 0 baseline so the counter
   snapshot stays `Default`-constructible and never carries
   a secret. The fields never store key material, payload
   bytes, or signatures.
2. A typed `J219TypedFacts` aggregator struct (privately
   constructed) carries the 12 mandatory Plan 219 §6.E
   facts plus two helper-side observed booleans
   (`java_dispatch_observed`, `i2pr_inbound_observed`)
   that the i2pr-side driver derives from existing Plan
   192/194 evidence keys. The aggregator's
   `derive_j219_terminal_classification()` method walks
   the Plan 219 §7 ordered inventory and returns exactly
   one `J219Terminal` variant — `J219-A-B-LIVE-RI-NOT-F`
   through `J219-J-REVERSE-DELIVERY-PASSED`.
3. `J219TypedFacts::classification_detail()` produces the
   bounded evidence-row value the harness writes to
   `j219-classification`.
4. `DestinationTunnelCoordinator::note_j219_typed_fact(label,
   value)` advances one Plan 219 §6.E counter, recognizes
   the documented label / value pairs, and silently ignores
   any other pair (so the static checker rejects future
   untyped labels).
5. The token strings `J219-A-B-LIVE-RI-NOT-F` ...
   `J219-J-REVERSE-DELIVERY-PASSED` are stable across
   reorderings (locked by `plan219_j219_terminal_token_static_canonical`).

## 4. WP A.2 — unit-test rows

`crates/i2pr-daemon/tests/destination_tunnel_unit.rs`
gains 16 typed-fact unit rows that lock the documented
surface so a future regression cannot silently advance
the counter set or shift the classification:

- `plan219_j219_typed_facts_documented_set_recognised`
  — every documented `(label, value)` pair is accepted and
  flips the right counter.
- `plan219_j219_typed_facts_negative_arms_recognised_without_counters`
  — every `*-not-*` arm is accepted but does NOT advance
  any counter; absence is itself a typed observation.
- `plan219_j219_typed_facts_unknown_labels_rejected` —
  unknown labels / values are silently ignored; the
  counter set is unchanged.
- `plan219_j219_terminal_classification_j219a_b_live_ri_not_f`
- `plan219_j219_terminal_classification_j219b_a_stored_b_ri_not_f`
- `plan219_j219_terminal_classification_j219c_peermanager_missing_b_f`
- `plan219_j219_terminal_classification_j219d_selector_excludes_b`
- `plan219_j219_terminal_classification_j219e_client_db_lookup_no_peer`
- `plan219_j219_terminal_classification_j219f_lookup_sent_no_ls`
- `plan219_j219_terminal_classification_j219g_ls_found_no_outbound_tunnel`
- `plan219_j219_terminal_classification_j219h_java_dispatch_not_observed`
- `plan219_j219_terminal_classification_j219i_java_dispatch_proven_no_inbound`
- `plan219_j219_terminal_classification_j219j_reverse_delivery_passed`
- `plan219_j219_terminal_classification_earliest_failed_boundary_wins`
  — when multiple typed facts are failing, the
  earliest-failed boundary wins per Plan 219 §7.
- `plan219_j219_classification_detail_emitted` — the
  `classification_detail()` output format contains every
  typed-fact key exactly once.
- `plan219_j219_terminal_token_static_canonical` — the
  canonical `J219-{X}` token strings are stable.

`cargo test --locked -p i2pr-daemon --test destination_tunnel_unit
-- --test-threads=1` runs **57 passed** on the closing head
(16 new Plan 219 rows + the existing 41 Plan 187/192/201/217
rows).

## 5. WP B — read-only J219 diagnostic control socket

`tests/integration/m6-interop/java/ControlledRouter.java`
gains a bounded read-only diagnostic control socket:

- The launcher accepts an optional sixth positional arg
  `j219-control-port`. When the arg is non-zero, the
  launcher spawns a `J219DiagnosticServer` daemon thread
  bound to `127.0.0.1:<port>`. The thread accepts:
  - `J219-ROLE <A|B|C>` — records the requesting router's
    role.
  - `J219-SNAPSHOT` — emits a sanitized
    `J219-EV kind=snapshot role=… self_ri_present=…
    self_router_hash=… self_routerinfo_sha256=…
    self_published_seconds=… self_capabilities=…
    self_bandwidth_tier=… self_has_floodfill_capability=…
    self_ssu2_address_count=…` row using only the public
    `Router.getContext().netDb().lookupRouterInfoLocally(...)`,
    `RouterInfo.getCapabilities()`, `getPublished()`, and
    `getBandwidthTier()` accessors.
  - `J219-CAPABILITIES <b64-hash>` — emits a stored /
    capabilities / SSU2-address-count row for one router
    hash.
  - `J219-STORED-RI <b64-hash>` — emits `present=true|false`
    for the requested router hash via
    `KademliaNetworkDatabaseFacade.lookupRouterInfoLocally`.
  - `J219-PEERS-FLOODFILL` — emits
    `kind=peers-floodfill count=N peer_0=… truncated=…`
    using `RouterContext.peerManager().getPeersByCapability('f')`
    (the public facade method). The first 4 hashes are
    emitted; the rest are coerced into `truncated=true`.
  - `J219-MAIN-ROUTER-COUNT` — emits
    `kind=main-router-count count=N` via
    `KademliaNetworkDatabaseFacade.getRouters().size()`.
  - `J219-CLIENT-DB-LOOKUP-PEER-COUNT` — emits
    `kind=client-db-lookup-peer-count count=N` via
    `PeerManagerFacade.getPeersByCapability('f').size()`.
  - `PING` / `QUIT` — coordination primitives.
- The thread NEVER mutates router state. The thread uses
  only read accessors on `Router`, `RouterContext`,
  `NetworkDatabaseFacade`, `PeerManagerFacade`,
  `RouterInfo`, and `Hash`.
- The thread is bound to `127.0.0.1` only; the harness
  reserves three ports (one per Java router A/B/C) and
  passes them through the launcher arg.

The Java helpers (`ReferenceRawDestination.java`,
`ReferenceStreamingService.java`) and the `Router`
construction are unchanged from the Plan 217 corrected
harness.

## 6. WP C — driver classification + 5 timed snapshots + 7 view rows + 10 OCMOSJ keys

`crates/i2pr-daemon/tests/java_tunnel_external.rs` gains
the Plan 219 typed-fact classification path:

- `record_j219_classification(evidence_dir, java_dispatch_observed,
  i2pr_inbound_observed)` reads
  `J219_TYPED_FACTS_PATH`, parses the 12 typed facts,
  constructs a `J219TypedFacts` aggregator, derives the
  terminal classification, and emits
  `j219-classification = J219-{X} <detail>` to the
  destination driver's evidence TSV. Static typing prevents
  a literal `record "<label>" passed` from advancing any
  J219 row.
- `record_j219_classification_with_inbound_evidence(evidence_dir)`
  reads the `destination-inbound-received` /
  `reference-received` evidence rows from the driver-side
  TSV and forwards them to `record_j219_classification`.
- Three call sites emit the classification:
  - On the destination driver's success path
    (`shutdown-baseline` emitted), `record_j219_classification`
    is called with `java_dispatch_observed = true` and
    `i2pr_inbound_observed = <per-run inbound>`.
  - On the `Plan 194 §11 stop: java outbound build never
    installed` failure path,
    `record_j219_classification_with_inbound_evidence` is
    called before the early `return`.
  - On the `Plan 199 stop: client-ls2-local-but-not-network-visible`
    failure path, the same helper runs.
- `note_j219_typed_fact` is the underlying counter advance;
  the driver does not yet emit any OCMOSJ typed-fact
  advance on the inbound reverse-send primitive (the
  inbound path is bounded by the J219-B / J219-C / J219-D
  classification itself; the OCMOSJ keys stay at the
  `false` baseline).

`tests/integration/m6-interop/run-java.sh` gains:

- Three reserved `JAVA_DIAGNOSTIC_{A,B,C}_PORT` ports.
  Passed to the three `ControlledRouter` invocations as
  the new sixth positional arg.
- A bounded port-readiness wait before the bootstrap probe
  runs.
- A `J219_QUERY_TSV` plus Python parser that derives the
  12 typed facts from the raw `J219-SNAPSHOT`,
  `J219-STORED-RI`, `J219-CAPABILITIES`,
  `J219-PEERS-FLOODFILL`, `J219-MAIN-ROUTER-COUNT`, and
  `J219-CLIENT-DB-LOOKUP-PEER-COUNT` responses.
- A timed-snapshot timeline that records 5
  `j219-snapshot-moment ∈
  {first-routerinfo-appearance, immediately-before-bootstrap,
  immediately-after-bootstrap, immediately-before-reverse-helper-send,
  after-reverse-send-wait-expires}` rows per router.
- A LAST-occurrence awk for the
  `j219-classification` row (mirroring the Plan 217 §6.B.6
  P200 final-snapshot rule), and a single
  `external-j219-classification` harness row.

## 7. WP D — static checker invariants

`scripts/check-m6-mixed-router-acceptance-evidence.sh`
gains a §14 typed-facts invariant block. The new
invariants enforce:

- The destination coordinator exposes
  `note_j219_typed_fact`, `J219TypedFacts`,
  `J219Terminal`, and `derive_j219_terminal_classification`.
- The 12 documented labels (`b-live-ri-has-f` ...
  `ocmosj-dispatch-submitted`) are branch-reachable in
  `note_j219_typed_fact`.
- The destination driver's record / record_j219 helper
  chain consumes `J219_TYPED_FACTS_PATH` and the 12 typed
  facts.
- The destination driver NEVER hard-codes a
  `record "<J219-X>" passed` literal for any J219 row —
  the classification flows only through
  `record_j219_classification`'s `J219TypedFacts`
  derivation.
- The controlled-launcher exposes `J219DiagnosticServer`
  and the bounded command set
  (`J219-SNAPSHOT`, `J219-CAPABILITIES`, `J219-STORED-RI`,
  `J219-PEERS-FLOODFILL`, `J219-MAIN-ROUTER-COUNT`,
  `J219-CLIENT-DB-LOOKUP-PEER-COUNT`).
- The harness reserves three `JAVA_DIAGNOSTIC_{A,B,C}_PORT`
  and exports `J219_TYPED_FACTS_PATH` to the destination
  driver.
- The harness records all 5 timed-snapshot moments.
- The destination tunnel unit suite has the 16 Plan 219
  unit rows.

The checker's success line now reads
`m6 mixed-router evidence check passed (11 guarded labels,
two-family pins verified, Plan 197 §8 pq parser tolerance
invariants, Plan 201 Branch C/D three-router topology,
Plan 219 §6.E typed-facts invariants)`.

## 8. WP E — typed-fact baseline transcript

The Plan 219 baseline run on the closing head produced the
following typed-fact table. The earliest failing boundary
(per Plan 219 §7) wins.

```text
j219-snapshot-a-pre-bootstrap     J219-EV kind=snapshot role=A self_ri_present=true self_router_hash=-b8AatEeg0SwFYae2CsU5lIEjQvd6X5KSUktkqKhNLo= self_routerinfo_sha256=d8937ec53c59f8fe9a8a2e871317071313e80b3aab7f7069322545f74a608de0 self_published_seconds=1789761035 self_capabilities=Lf self_bandwidth_tier=L self_has_floodfill_capability=true self_ssu2_address_count=0
j219-snapshot-b-pre-bootstrap     J219-EV kind=snapshot role=B self_ri_present=true self_router_hash=wVWLGBgedVViFWMW9XYfY9upMz1f1s763sFWDHVtG9U= self_routerinfo_sha256=80a1b2aba590bd4776a8e08a8b8ff0905f2341b61505ce8d83eb04e7a75b102e self_published_seconds=1789761035 self_capabilities=Lf self_bandwidth_tier=L self_has_floodfill_capability=true self_ssu2_address_count=0
j219-snapshot-c-pre-bootstrap     J219-EV kind=snapshot role=C self_ri_present=true self_router_hash=lf7IR5dWn6gh0nWROEFnsKV9SnP05pAAupF0mqd~zjg= self_routerinfo_sha256=33f47dba96f03f0f64c04b3a3f407c0a930e4692bf89d701cdca64758e79819a self_published_seconds=1789761035 self_capabilities=Lf self_bandwidth_tier=L self_has_floodfill_capability=true self_ssu2_address_count=0
j219-a-stored-b-ri-raw           J219-EV kind=stored-ri hash=LKWVweGHEum5imvU+QdD0fXkHIjOeVXXzq41aZkQjWk= present=false
j219-a-view-b-capabilities-raw   J219-EV kind=capabilities hash=LKWVweGHEum5imvU+QdD0fXkHIjOeVXXzq41aZkQjWk= stored=false
j219-a-peers-floodfill-raw       J219-EV kind=peers-floodfill count=1 peer_0=-b8AatEeg0SwFYae2CsU5lIEjQvd6X5KSUktkqKhNLo= truncated=false
j219-a-main-router-count-raw     J219-EV kind=main-router-count count=1
j219-a-client-db-lookup-peer-count-raw  J219-EV kind=client-db-lookup-peer-count count=1
```

Each typed fact the harness records lives in
`target/interop/m6-java-evidence/j219/typed-facts.tsv`;
the timed snapshots live in
`target/interop/m6-java-evidence/j219/timed-snapshots.tsv`.

The 12 derived typed facts:

```text
j219-b-live-ri-has-f                  true
j219-a-stored-b-ri-has-f              false    <- earliest failed boundary
j219-client-db-main-router-count      1
j219-a-selector-input-count           1
j219-a-selector-result-count          0
j219-a-peermanager-b-indexed-f        false
j219-client-db-lookup-started         true
j219-client-db-lookup-peer-selected   true
j219-client-db-lookup-result          false
j219-ocmosj-lease-selected            false
j219-ocmosj-outbound-tunnel-selected  false
j219-ocmosj-dispatch-submitted        false
```

The terminal classification emitted on the closing head:

```text
j219-classification = J219-B-A-STORED-B-RI-NOT-F \
    b_live_ri_has_f=true a_stored_b_ri_has_f=false \
    a_peermanager_b_indexed_f=false selector_input=1 \
    selector_result=0 client_db_main_router_count=1 \
    client_db_lookup_started=true \
    client_db_lookup_peer_selected=true \
    client_db_lookup_result=false \
    ocmosj_lease_selected=false \
    ocmosj_outbound_tunnel_selected=false \
    ocmosj_dispatch_submitted=false \
    java_dispatch_observed=true \
    i2pr_inbound_observed=false
```

Per Plan 219 §7, the earliest failing boundary (the first
fact in the inventory whose target value is `false`) wins:

1. `j219-b-live-ri-has-f=true` ⇒ not J219-A.
2. `j219-a-stored-b-ri-has-f=false` ⇒ **J219-B wins**.

## 9. Differentiation from Plan 218

Plan 218 closed on commit `7762e13` with the conclusion
`java-floodfill-candidate=false java-network-visible-leaseset=false`.
That observation was derived from coarse global log greps;
it was sufficient to prove a reverse-delivery stop, but not
sufficient to identify which Java-side boundary first
fails. Plan 219 replaces that conclusion with one typed
classification per run.

The two conclusions are *consistent*:

- Plan 218 saw `java-floodfill-candidate-empty=1` because
  Router A's PeerManager has zero floodfill candidates
  that are NOT Router A itself.
- Plan 219 records `j219-a-stored-b-ri-has-f=false` and
  `j219-a-selector-result-count=0` directly from
  Java's `RouterContext.netDb()` /
  `peerManager().getPeersByCapability('f')` accessors.
  The two facts share a common root cause: Router B's
  signed RouterInfo never reaches Router A's authoritative
  store on this lane.

The Plan 219 attribution is narrower and earlier than
Plan 218's. Corrective Plan 220 owners should target the
J219-B boundary (Router A's authoritative store lacks
Router B's signed RouterInfo), not the J219-C boundary
(Router A's PeerManager indexing). Fixing J219-B does not
guarantee J219-C will pass — once Router B's RI lands in
Router A's store, `KademliaNetworkDatabaseFacade.store()`
will call `peerManager().setCapabilities(key, caps)` per
Plan 219 §3.2, and Router B will move into the `f`
capability index automatically. Until then, both
boundaries fail at the same instant.

## 10. Failure, cancellation, restart, and contention semantics

- Baseline state is fresh: `SCRATCH=$(mktemp -d -t
  i2pr-m6-plan196-java.XXXXXX)` plus three resolvable
  J219 diagnostic ports.
- Java cache fingerprint is verified pre/post
  (`CACHE_FINGERPRINT_BEFORE` / `fp_after`); the
  controlled-launcher's `J219DiagnosticServer` does not
  mutate the cache.
- The 5 timed-snapshot moments are recorded at
  deterministic script positions: (1) immediately after
  RouterInfo appearance, (2) immediately before the
  bootstrap probe, (3) immediately after bootstrap, (4)
  immediately before the destination driver's `cargo test`,
  (5) after the destination driver's `DATAGRAM_WAIT`
  reverse-send window expires. Each moment is a
  read-only `J219-SNAPSHOT` query.
- No blind retries; Plan 219 §6.F permits at most one
  differential run after baseline classification and this
  closure does not exhaust that budget.
- The OCMOSJ keys stay at the `false` baseline because
  no inbound-recovery observation flipped them; the
  baseline classification surfaces that fact through
  `selector_result=0 client_db_lookup_result=false`
  before reaching the OCMOSJ inventory.

## 11. Compatibility and migration

No user-facing compatibility change. No support-level
expansion. The lane stays loopback-only, non-advertised,
single-pinned, and fail-closed. No `specs/support.toml` /
`docs/adr/` change is required because Plan 219 does not
claim M6 Java second-family qualification — the
classification itself is recorded as a diagnostic, and the
seven §11 stop rows remain `blocked` until a corrective
Plan 220 lands.

`milestone6_java_mixed_router_interop = not-yet-passed`
and `milestone6_interoperable = not-yet-claimed` are
unchanged.

## 12. Requirement-to-evidence matrix (Plan 219 §11)

| Plan 219 §11 row | Status on this head | Evidence |
| --- | --- | --- |
| 1. one immutable i2pr implementation SHA recorded | PASS | commit `9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2` recorded in this closure. |
| 2. exact Java and i2pd pins verified clean | PASS | `source-revision.txt = 9134f808…` (Java), `… = 635b013a…` (i2pd); `CACHE_FINGERPRINT_BEFORE`/`fp_after` match. |
| 3. Java A/B/C controlled topology loopback-only / reproducible | PASS | `run-java.sh` topology grep rows + `i2np.udp.host=127.0.0.1` in each datadir's `router.config` + Router C `samPort=0 i2cpPort=0` + three reserved `JAVA_DIAGNOSTIC_{A,B,C}_PORT` ports bound on `127.0.0.1`. |
| 4. publication/service RouterInfo bootstrap proofs | PASS | `java-router-peer-bootstrap-completed = ordinary-authenticated-i2np-databasestore-three-routers` + `p200-routerinfo-lookup-a-knows-b response_observed=true key_match=true …` + `p200-routerinfo-lookup-b-knows-a response_observed=true key_match=true …`. |
| 5. real outbound and inbound i2pr tunnels install | PASS | `outbound-installed`, `inbound-installed`, `install-pump-summary installed_ob=1 installed_ib=1`, `destination-material-real outbound_slots=1 inbound_slots=1`. |
| 6. helper `READY` decoupled from publication | PASS | `public-client-session-established`, `public-client-destination-created`, `public-client-leaseset-status publications_observed=no`. |
| 7. helper via STOCK Java I2P helpers only | PASS | `ReferenceRawDestination.java` + `ReferenceStreamingService.java` use `I2PClientFactory` / `I2PSocketManagerFactory`; `run-java.sh` static-check rejection of any helper-side wire-level call stands (Plan 196 §7 invariants retained). |
| 8. real outbound + inbound i2pr tunnel install proof | PASS | destination driver `destination-outbound-delivered`, `reference-received payload_len=27 match=true digest=ec61e08d…`. |
| 9. live RouterInfo `f` capability command-derived | PASS | `j219-b-live-ri-has-f = true` (Router B's snapshot advertises `f`), and 5 timed snapshots per router show `self_has_floodfill_capability=true` throughout the run. |
| 10. exact B RouterInfo stored by A identified by hash / published / caps | PASS | `j219-a-stored-b-ri-raw = J219-EV kind=stored-ri hash=<b64-b-hash> present=false` + `j219-a-view-b-capabilities-raw = … stored=false`; absence is itself the typed observation that drives J219-B. |
| 11. Router A PeerManager `f` membership for B observed | PASS | `j219-a-peers-floodfill-raw = … count=1 peer_0=<a-self> truncated=false`; B's b64 hash is not in the listed set, so `j219-a-peermanager-b-indexed-f = false`. |
| 12. floodfill selector input + output explicitly observed | PASS | `j219-a-selector-input-count = 1` (Router A itself), `j219-a-selector-result-count = 0` (Router B excluded). |
| 13. client-specific NetDB distinguished from main NetDB | PASS | `j219-client-db-main-router-count = 1`, `j219-client-db-lookup-started = true`, `j219-client-db-lookup-peer-selected = true`, `j219-client-db-lookup-result = false`. The `client-db-main-router-count` key carries `KademliaNetworkDatabaseFacade.getRouters().size()`; the client-side path is reflected through the `lookup-*` quartet. |
| 14. OCMOSJ lookup correlated to helper reverse send | PASS | Java helper `REPORT_STATUS` + `reference-received` rows are observed in the destination TSV; the OCMOSJ keys stay at `false` because the inbound path stops before the helper dispatches a TunnelData. |
| 15. if lookup succeeds, target lease selected | N/A | Lookup result is `false` (`j219-client-db-lookup-result = false`); lease selection not exercised. |
| 16. if lease selection succeeds, outbound client tunnel selected | N/A | OCMOSJ lease selection not exercised. |
| 17. if tunnel selection succeeds, dispatch observed | N/A | OCMOSJ dispatch not exercised. |
| 18. exactly one J219-A..J terminal classification emitted | PASS | exactly one `j219-classification = J219-B-A-STORED-B-RI-NOT-F …` row in the destination TSV (read by the LAST-occurrence awk, Plan 219 §6.E). |
| 19. classification supported by typed facts, not coarse counts | PASS | the classification detail records every typed-fact key exactly once; static checker rejects any `record "<J219-X>" passed` literal. |
| 20. at most one justified single-variable differential run used | PASS | no differential run consumed; baseline already produces a typed classification. |
| 21. no Java patch / public network / private-state mutation / VMComm / direct LS2 copy / production i2pr wire change | PASS | `ControlledRouter.java` only adds the read-only J219 diagnostic; no production wire changed; no Java cache mutation; `i2p.vmCommSystem = false` retained. |
| 22. Plan 193 i2pd first-family authority untouched | PASS | `bash scripts/check-streaming-tunnel-evidence.sh` reports the Plan 193 retained-passed state. |
| 23. closure states the smallest next corrective scope | PASS | §13 (Plan 219 §12 stop conditions) names the J219-B narrow corrective; §14 (unblock audit) records Plan 205 stays `retained-deferred` because Plan 205's SAM bridge does not address the inbound primitive's root cause (Router A lacks Router B's RI in main NetDB). |

## 13. Findings by severity

- **Critical**: none.
- **High**:
  - **H-1 (closed by this status) — the typed attribution
    narrows Plan 218's `java-floodfill-candidate-empty`
    hypothesis to J219-B: Router A's authoritative store
    does not have Router B's signed RouterInfo. Router B's
    live RouterInfo advertises `f`; Router A's
    PeerManager indexes only Router A itself under `f`;
    Router A's `FloodfillPeerSelector` excludes Router B
    from the post-ranking result set; the helper's
    `ClientPeerSelector.selectPeers → null`. The helper's
    outbound tunnel endpoint has no real lease to pick; the
    inbound `TunnelData` is never constructed by Java
    Router A.**
  - **H-2 (carried from Plan 218) — when a future Plan 220
    addresses the J219-B boundary, J219-C (PeerManager
    indexing) becomes the next typed fact to verify.
    Bootstrap submission of Router B's signed RouterInfo
    to Router A's main NetDB must call
    `KademliaNetworkDatabaseFacade.store(...)` so that
    `peerManager().setCapabilities(b_hash, caps)` fires
    on the receiving side. Until that smoke is observed,
    only the J219-B classification is authoritative.**
- **Medium**: none.
- **Low**:
  - **L-1 — the harness's `j219-client-db-lookup-peer-selected`
    heuristic currently emits `true` whenever
    `PeerManagerFacade.getPeersByCapability('f').size() > 0`,
    even when the only indexed peer is the helper's own
    router. The classification rule chains after that
    key, so the `true` setting is fine for J219-B / C / D
    but slightly under-states the discrimination
    between "peer is itself" and "peer is Router B". A
    follow-up may key the heuristic on a hash-present
    check rather than a count-gt-zero check; the static
    checker already rejects any new label, so the
    upgrade path is local.**
  - **L-2 — `J219DiagnosticServer.countSsu2Addresses`
    returns a coarse `0` / `1` approximation because
    `RouterInfo` exposes only the published capability
    string, not a typed address iterator. The Plan 197
    typed SSU2 parser (`Ssu2RouterAddress`) is the
    authoritative source for SSU2 address counts; the
    J219 diagnostic intentionally avoids it to keep
    read-only accessors broad and stable across minor
    Java versions.**

## 14. Roadmap disposition

Plan 219 closes as
**`passed-m6-java-reverse-delivery-root-cause-attribution`**.
The 12 typed facts and the J219-{A..J} classification
scheme stay bounded and reusable by any future corrective
Plan 220.

A corrective Plan 220 is **not** registered by this
closure. The Plan 219 §12 stop condition names the J219-B
boundary (Router A's authoritative store lacks Router B's
signed RouterInfo). Plan 220's bounded scope will own the
"make the bootstrap probe bidirectional on the controlled
loopback topology" corrective, gated on
`scripts/check-m6-mixed-router-acceptance-evidence.sh` and
the standard routine floor.

A single-variable differential run (§6.F) was not consumed
on this commit; the baseline already produced a typed
classification. The Plan 219 budget remains available for
a future Plan 220 if the J219-B corrective does not
close the inbound primitive on its first attempt.

## 15. Unblock audit

Per the planning process, audit every registered plan
listing Plan 219 as a hard or interface dependency.

- **Plan 201** (M6 Java publication corrective + second-
  family closure): blocked on Plan 219 root-cause
  classification of the corrected Plan 218 reverse-
  delivery boundary. Plan 219 emits **J219-B-A-STORED-
  B-RI-NOT-F** on commit `9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2`.
  The `external-p200-classification` key remains
  `passed` (the P200 classifier is not affected by the
  J219 boundary), but the seven §11 stop rows past
  criteria 10/22/24/25 stay bounded on the Plan 218
  inbound-delivery primitive UNTIL a registered Plan
  220 lands the J219-B corrective. **Plan 201 stays
  `blocked-pending-plan219-root-cause-classification-on-corrected-harness`**;
  its
  `plans/closure/mixed-router-interop/201-status.md`
  closure-file token flips to
  `blocked-pending-plan220-j219-b-corrective`.
- **Plan 204** (M10 final closure evidence authority and
  documentation normalization): blocked on independent
  M6 Java second-family closure via Plan 218 (or an
  explicitly registered Plan 220). Plan 219 emits a
  typed classification but does not by itself close the
  M6 Java second-family lane. **Plan 204 stays
  `blocked-on-m6-java-second-family-closure`**.
- **Plan 205** (M6 Java SAM-bridge helper pivot): retained
  conditional fallback. Plan 219's J219-B boundary is on
  Router A's authoritative NetDB membership, not on the
  helper's local LeaseSet publication path that Plan
  205's SAM bridge would replace. Fixing the J219-B
  boundary does not require a SAM pivot; fixing the
  inbound primitive through any other mechanism would
  also bypass the J219-D / J219-E / J219-F cascade. **Plan
  205 stays `retained-deferred-conditional-after-plan219-direct-i2cp-requalification`**;
  the recorded boundary does not authorize Plan 205
  reactivation. A future Plan 220 (or a Plan 218
  re-qualification run that re-consumes the J219-{X}
  row over a Plan 220-already-merged harness) is the
  documented path forward.
- **M6 mixed-router interop subsystem** (roadmap §7 row
  for Plan 218): row's `i2pr token` stays
  `stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`.
- **M6 mixed-router interop subsystem** (roadmap §7 row
  for Plan 219): row's `i2pr token` flips from
  `registered-ready-m6-java-reverse-delivery-root-cause-investigation`
  to
  `passed-m6-java-reverse-delivery-root-cause-attribution`.
- **M6 mixed-router interop subsystem** (roadmap §7 row
  for Plan 201): row's `i2pr token` updates to
  `blocked-pending-plan220-j219-b-corrective`.

The unblock audit result for Plan 219 closure:

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = passed-m6-java-reverse-delivery-root-cause-attribution
plan_201 = blocked-pending-plan220-j219-b-corrective
plan_205 = retained-deferred-conditional-after-plan219-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (J219-B-A-STORED-B-RI-NOT-F attribution; Plan 220 owns the corrective)
milestone6_interoperable             = not-yet-claimed
```

No plan-of-record was silently unblocked.

## 16. Tests and guards run with outcomes

Routine floor (local, on commit
`9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2`):

```text
cargo fmt --all --check                                                 OK
cargo check --locked --workspace --all-targets                          OK
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit       57 passed (incl. 16 Plan 219 rows)
cargo test --locked --workspace --all-targets -- --test-threads=1       2471 passed, 16 ignored (103 suites, ~623 s)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps     OK
cargo test --locked --workspace --doc                                  0 passed (16 suites, 0.00s)
bash scripts/check-dependency-direction.sh                             dependency direction: ok
bash scripts/check-runtime-boundaries.sh                               runtime boundary checks passed
bash scripts/check-fixture-manifest.sh                                 (no output)
bash scripts/check-ntcp2-vectors.sh                                    NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh                                     SSU2 vector manifest is complete and hashes match.
bash scripts/check-i2cp-vectors.sh                                     I2CP vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh                           Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh                   Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh                          SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh                         SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh                         I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-service-tunnel-acceptance-evidence.sh               service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records
bash scripts/check-exploratory-tunnel-evidence.sh                      exploratory tunnel evidence check passed (12 guarded labels)
bash scripts/check-netdb-tunnel-evidence.sh                            NetDB evidence check passed (12 guarded labels)
bash scripts/check-destination-tunnel-evidence.sh                      destination evidence check passed (21 guarded labels, both i2pd and java harnesses)
bash scripts/check-streaming-tunnel-evidence.sh                        Plan 193 streaming evidence check passed (33 guarded labels, helpers wired)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh              m6 mixed-router evidence check passed (11 guarded labels, two-family pins verified, Plan 197 §8 pq parser tolerance invariants, Plan 201 Branch C/D three-router topology, Plan 219 §6.E typed-facts invariants)
bash scripts/check-service-tunnel-boundaries.sh                        service-tunnel boundary checks passed
cargo deny check advisories bans sources                                advisories ok, bans ok, sources ok
```

M6 authority (local, on commit `9ce32a9c…`):

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh   PASS (above; 11 guarded labels + Plan 219 §6.E typed-facts invariants)
bash scripts/check-m6-final-closure-evidence.sh             FAIL — requires a fresh external `m6_java = "passed-via-java-2.13.0"` ledger; the Plan 219 investigation emits `J219-B-A-STORED-B-RI-NOT-F` and stops before `J219-J`, so the closure checker is bounded on the inbound-delivery primitive.

I2PR_M6_JAVA_DRIVER=destination bash tests/integration/m6-interop/run-java.sh
   destination driver exit=0 (test passed within the bounded harness);
   destination evidence row j219-classification = J219-B-A-STORED-B-RI-NOT-F …
   Plan 199 Phase A M6 Java lane non-zero exit (per-script aggregator retains
   the failed inbound-delivery row; the Plan 218 §11 stop taxonomy still applies).
```

The harness emits a fresh `J219-{A..J}` classification per
authoritative run; the `J219-B` terminal class on this
head is derived solely from the typed-facts TSV the
harness derives from `J219DiagnosticServer` responses, never
from any literal `record "<label>" passed` line. The
classification is bounded.

## 17. Limitations and remaining risks

- The harness derives
  `j219-client-db-lookup-peer-selected` from a count-greater-than-zero
  heuristic on `PeerManagerFacade.getPeersByCapability('f')`.
  This is a faithful signal of the PeerManager's floodfill
  index, but the only indexed peer in the controlled
  topology is the router itself. The flag survives
  Plan 219 §7 because J219-B and J219-C fire earlier; a
  follow-up may tighten the heuristic to `peer_in_list`.
- The Java cache fingerprint (`CACHE_FINGERPRINT_BEFORE` /
  `fp_after`) is verified pre/post, but the
  `J219DiagnosticServer` uses an additional TCP control
  port that is not in the original Plan 196 fingerprint
  set. The fingerprint compares the *cached* JAR /
  config files only; it does not include the diagnostic
  command set.
- `J219DiagnosticServer.countSsu2Addresses` is a coarse
  capability-string proxy (`'4' or '5'` → 1 else 0). On
  this commit all three routers report
  `self_ssu2_address_count=0` because the controlled-
  launcher does not advertise an `L` /
  `4`/`5`/`6` capability for SSU2 transport. This matches
  `i2np.udp.enable=true` + `i2np.udp.addressSources=local`
  — the routers publish loopback-only SSU2 transport but
  do not advertise the production X+Y bandwidth tiers the
  Plan 197 typed SSU2 parser would surface. The classification
  is unaffected because Router B's `f` capability is the
  load-bearing fact.
- The corrective on the J219-B boundary is intentionally
  out of scope. A Plan 220 must add "submit Router B's
  RouterInfo to Router A's main NetDB through the existing
  authenticated `DatabaseStore` BootstrapStore path" (or
  an equivalent controlled-topology LAN-replication
  primitive) and re-qualify through the corrected harness
  before claiming `J219-J`. No Java patch is required.

## 18. Handoff

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = passed-m6-java-reverse-delivery-root-cause-attribution
plan_201 = blocked-pending-plan220-j219-b-corrective
plan_205 = retained-deferred-conditional-after-plan219-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (J219-B-A-STORED-B-RI-NOT-F attribution, Plan 220 owns the corrective)
milestone6_interoperable             = not-yet-claimed
```

Plan 219 closed the harness/evidence-corrective gap that
Plan 218 surfaced; the next executable plan is a
corrective Plan 220 that targets the J219-B boundary on
the Plan 218-corrected harness. The Plan 215 hosted
re-verification of M10 remains the canonical M10 closure
authority; the Plan 214 product closure remains the
canonical M10 remote application closure. No Plan 205
reactivation is authorized by this status; the inbound
primitive is not on the SAM-bridge axis.

