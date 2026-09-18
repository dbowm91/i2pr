# Plan 220 — M6 Java Plan 219 diagnostic-attribution corrective

Status: **registered-ready-m6-java-plan219-diagnostic-attribution-corrective**.

## 1. Objective

Repair the Plan 219 diagnostic harness so it can truthfully identify the first boundary on the Java → i2pr reverse-delivery path.

This plan supersedes the **Plan 219 attribution**, not the useful read-only diagnostic infrastructure and not the underlying Plan 218 observation that reverse delivery is absent.

Plan 220 MUST NOT implement a Java topology, bootstrap, NetDB, tunnel, SAM, or i2pr wire corrective. Its only capability outcome is:

> one exact-clean-head destination-only run produces either a correctly observed root-cause classification, or a typed observability-gap classification at the first stage that cannot yet be observed without violating the test constraints.

Only after this plan closes may a later plan target the resulting protocol/reference-topology boundary.

## 2. Why this corrective is ready

Plan 218 remains the last trustworthy external behavioral boundary:

- authenticated Java/i2pr sessions pass;
- RouterInfo bootstrap and real tunnel installs pass;
- Java LeaseSet2 lookup/validation passes;
- i2pr local LS2 publication passes;
- byte-exact i2pr → Java raw Destination delivery passes;
- Java helper reverse send is accepted;
- the Java → i2pr payload does not arrive.

Plan 219 then added useful infrastructure:

- loopback-only read-only Java diagnostic ports;
- five timed RouterInfo snapshots;
- a typed fact file;
- a classifier and unit/static guards.

However, review of commit `96a25cd96a3a4df6c2a9314c96930c2a917c382d` and follow-through authority commit `3b6f0b1bb3a6e797b5efbbdcc6aab34ae93d708c` found that the Plan 219 terminal attribution `J219-B-A-STORED-B-RI-NOT-F` is not supported by the implemented measurement path.

## 3. Defects Plan 220 must close

### D220-1 — load-bearing facts are sampled before bootstrap

`run-java.sh` derives the A-view-of-B / PeerManager / selector-like / client-db facts before the later `bootstrap_java_router_peers` invocation. Later timed snapshots are recorded, but the typed facts are not recomputed from post-bootstrap / pre-reverse-send state.

### D220-2 — Router B hash derivation is not protocol-correct

The harness hashes `router.info[16:386]` and converts the digest with Python standard Base64. Pinned Java `RouterInfo.readBytes()` starts with RouterIdentity at byte zero, and Java's `Hash.toBase64()` uses I2P Base64 semantics. The diagnostic may therefore query Router A with a hash that is not Router B.

### D220-3 — Plan 219 narrative contradicts executable bootstrap

`destination_message_plane_against_java()` already builds `service_wire` from Router B's signed RouterInfo and submits it to Router A (`service-to-publication-and-publication-to-service`). The Plan 219 closure statement that inverse B → A submission is not exercised is not consistent with executable source.

### D220-4 — `a-stored-b-ri-has-f` checks presence only

The derivation maps `present=true` directly to `j219-a-stored-b-ri-has-f=true`. It does not require the expected B identity/hash, `has_floodfill_capability=true`, or current/stale RouterInfo correlation.

### D220-5 — selector output is synthesized

`selector_result_count` is inferred from `peerManager().getPeersByCapability('f')`; it is not actual `FloodfillPeerSelector` output.

### D220-6 — client-NetDB / OCMOSJ facts use fabricated defaults

The harness hard-codes lookup-started and initializes lookup result, lease selected, outbound tunnel selected, and dispatch submitted to false without observing the per-message Java state.

### D220-7 — dispatch evidence is directionally wrong

`reference-received` proves the earlier i2pr → Java payload arrived. It does not prove the later Java → i2pr OCMOSJ dispatch.

### D220-8 — closure run is not an immutable implementation head

Plan 219 status records `9ce32a9c...` as implementation/run SHA even though the diagnostic implementation landed in `96a25cd...`. A dirty-tree run over the parent is not exact-head closure evidence.

### D220-9 — investigation-only machinery leaked into production source

Plan-specific J219 typed-fact/classifier machinery was added to `crates/i2pr-daemon/src/destination_tunnels.rs`. Unless production behavior genuinely needs an observation hook, this should move into the external test driver or testkit.

## 4. Authority disposition on registration

```text
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = registered-ready-m6-java-plan219-diagnostic-attribution-corrective

plan_201 = blocked-pending-plan220-corrected-java-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective
```

No J219-B bootstrap corrective is authorized by this registration.

## 5. Invariants

Plan 220 MUST preserve:

- exact stock Java I2P 2.13.0 pin `9134f808337b401e8e53c73734c81fab04280c9d`;
- exact i2pd 2.61.0 pin `635b013a612ff47278ef02acf8580a28e10e26c5`;
- no public I2P participation;
- no Java source patch;
- no reflection or private-state mutation;
- no NetDB/tunnel/LeaseSet injection;
- no VMComm;
- no direct LS2 copying;
- no SAM helper rewrite;
- no topology expansion;
- no new RouterInfo bootstrap behavior solely to make the test pass;
- no i2pr production wire change;
- no support-claim expansion;
- loopback-only bounded diagnostics;
- Plan 193 i2pd authority untouched;
- Plan 215 M10 authority untouched.

Most importantly: **unknown / unobserved state MUST remain unknown. It MUST NOT be converted to false, zero, or a root-cause classification.**

## 6. Scope

### In scope

- `tests/integration/m6-interop/run-java.sh`;
- `tests/integration/m6-interop/java/ControlledRouter.java`;
- a narrow clean-room same-package Java probe under `tests/integration/m6-interop/java/` if package-private selector APIs require it;
- `crates/i2pr-daemon/tests/java_tunnel_external.rs`;
- `crates/i2pr-daemon/tests/destination_tunnel_unit.rs` as needed for obsolete Plan 219 classifier tests;
- `scripts/check-m6-mixed-router-acceptance-evidence.sh`;
- removal/relocation of Plan 219-only diagnostic structures from `crates/i2pr-daemon/src/destination_tunnels.rs`;
- Plan 219/220 authority files.

### Explicitly out of scope

- adding a B → A bootstrap corrective;
- changing floodfill policy or RouterInfo publication timing;
- changing Java client tunnel pool settings;
- changing i2pr NetDB/Garlic/Streaming/SSU2/tunnel/LS2 wire behavior;
- Streaming requalification;
- final Java-family qualification;
- Plan 204 convergence;
- M11.

## 7. Work package A — authoritative observation epoch

Move load-bearing Java-state queries into the destination external driver at the point the state matters:

```text
fresh RouterContexts
  -> shell bootstrap probe
  -> destination driver authenticated sessions
  -> destination driver's ordinary A/B RouterInfo DatabaseStore bootstrap
  -> bounded settle/pump
  -> Java A-view-of-B diagnostic query
  -> Java PeerManager/selector diagnostic query
  -> helper reverse SEND begin
  -> client-NetDB / OCMOSJ observations
  -> reverse SEND terminal state
  -> classification
```

Early snapshots may remain for history, but no pre-bootstrap fact may feed the terminal classifier.

Every observation row must carry an epoch such as:

```text
pre-bootstrap
post-shell-bootstrap
post-driver-bootstrap
pre-reverse-send
post-reverse-send
```

Required guards:

- fail if authoritative facts are derived before bootstrap;
- fail if the destination driver does not take a fresh A-view-of-B snapshot after its own bootstrap and before reverse send;
- fail if the classifier consumes an older epoch when a newer authoritative epoch exists.

## 8. Work package B — protocol-correct RouterHash identity

Remove all raw byte slicing and generic Python Base64 from the RouterHash path.

The destination driver already validates/decode Router B's RouterInfo and has its protocol-derived `java_hash`. Use that value.

Preferred diagnostic contract:

- commands accept a 32-byte RouterHash as lowercase hex;
- responses include `router_hash_hex` and Java `Hash.toBase64()` for human correlation.

Before any A-view-of-B fact is accepted:

```text
rust_decoded_B_router_hash
    == Java_B_self_snapshot_router_hash
    == query_target_hash_used_against_A
```

Mismatch is a **diagnostic failure**, not "A does not store B".

Static guards must reject `data[16:386]`, Python `base64.b64encode` for RouterHash construction, and any hard-coded RouterIdentity byte offset.

## 9. Work package C — exact stored-RouterInfo evidence

For Router A's view of B, observe separately:

```text
target_hash_matches_B
present
stored_identity_hash_matches_B
stored_routerinfo_sha256
stored_published_timestamp
stored_capabilities
stored_has_f
B_live_routerinfo_sha256
B_live_published_timestamp
B_live_capabilities
B_live_has_f
stored_matches_current_live_record
```

A "stored B RI lacks f" classification is allowed only if B's identity is validated, B live RI has `f`, A is queried for that exact B hash, A returns that identity, and the returned record lacks `f` or is demonstrably stale/pre-`f`.

If A returns no exact B record, classify **A lacks B RI**, not "stored B RI not f".

## 10. Work package D — separate PeerManager from selector output

Observe separately:

1. Router A PeerManager `f` membership;
2. whether B is in that set;
3. B permanent-banlist / explicit-ignore state;
4. actual `FloodfillPeerSelector` selected peers for the relevant lookup routing key.

Do not synthesize selector output from PeerManager membership.

Because `FloodfillPeerSelector` is package-private in the exact Java source, preferred implementation is a newly authored test-only same-package probe under `tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/`. It may call package-visible selector APIs read-only and return only sanitized counts/target-membership booleans. It MUST NOT patch or replace any Java I2P class.

Fallback: exact-pinned Java DEBUG/INFO output only if unambiguously correlated to the reverse lookup.

If neither is possible without mutation/reflection, emit an observability gap and stop.

## 11. Work package E — tri-state facts

Replace default-false Plan 219 classification with explicit observation state:

```text
Observed<T> =
  Known(T)
  Unknown(reason)
```

or equivalent.

Rules:

- absent key never defaults false;
- parse failure never defaults zero;
- missing output never becomes a protocol failure;
- root-cause classification may fire only when every earlier boundary is `Known(pass)` and current boundary is `Known(fail)`;
- earlier Unknown emits `P220-OBSERVABILITY-GAP-<stage>`.

Terminal classes:

```text
P220-CORRECTED-ATTRIBUTION <validated boundary>
P220-OBSERVABILITY-GAP-HASH
P220-OBSERVABILITY-GAP-A-STORED-RI
P220-OBSERVABILITY-GAP-PEERMANAGER
P220-OBSERVABILITY-GAP-SELECTOR
P220-OBSERVABILITY-GAP-CLIENT-NETDB
P220-OBSERVABILITY-GAP-OCMOSJ
P220-REVERSE-DELIVERY-PASSED
```

The old J219-B result remains historical/superseded.

## 12. Work package F — client-NetDB / OCMOSJ only if earlier stages pass

If execution reaches the client-message path, correlate the **reverse send** specifically.

Required tri-state facts:

```text
reverse_send_admitted
client_netdb_lookup_started
client_netdb_lookup_peer_selected
client_netdb_lookup_succeeded
target_leaseset_present
target_lease_selected
outbound_client_tunnel_selected
garlic_constructed
tunnel_dispatch_submitted
i2pr_inbound_tunneldata_observed
i2pr_destination_payload_recovered
```

Permitted evidence: exact-pinned class-specific logs in the bounded send window, read-only Java stats/counters with unambiguous fresh-run deltas, read-only public RouterContext diagnostics, or helper status events.

Forbidden: treating `sendMessage()==true` as lookup/dispatch success; using forward `reference-received` as reverse proof; defaulting OCMOSJ facts false; reflection/private fields.

If a stage is not observable within constraints, stop at the corresponding observability gap.

## 13. Work package G — correct directional dispatch evidence

Use separate evidence names:

```text
forward_i2pr_to_java_received
reverse_java_send_admitted
reverse_java_dispatch_submitted
reverse_i2pr_tunneldata_observed
reverse_i2pr_destination_payload_recovered
```

Remove the old `java_dispatch_observed = reference-received` mapping.

## 14. Work package H — remove Plan-219-only production diagnostic machinery

Review `DestinationTunnelCounters`, `J219TypedFacts`, `J219Terminal`, and `note_j219_typed_fact`.

If they exist only for external interoperability classification, move them into `java_tunnel_external.rs` or a focused testkit helper. Production destination/tunnel code should retain only genuine runtime observability needed by the product.

Do not remove existing Plan 201/product counters with independent runtime value.

Acceptance requires either removal of the Plan-219-only production surface or a concrete non-test runtime consumer documented in closure.

## 15. Work package I — regression guards

Update `scripts/check-m6-mixed-router-acceptance-evidence.sh` to reject:

- `[16:386]` RouterInfo identity slicing;
- standard Python Base64 RouterHash construction;
- classifier input derived before post-bootstrap/pre-send epoch;
- `present=true`-only "stored RI has f";
- selector-result computation solely from `getPeersByCapability('f')`;
- hard-coded `lookup-started=true`;
- default-false lookup/OCMOSJ terminal facts;
- forward `reference-received` → reverse dispatch mapping;
- dirty-tree/parent-SHA closure wording;
- any classifier without explicit Unknown/observability-gap behavior.

Static checks protect structure; they do not replace the fresh external run.

## 16. Work package J — exact-clean-head external rerun

Commit the diagnostic implementation before the authoritative run.

Record:

```bash
git rev-parse HEAD
git status --porcelain=v1
```

The porcelain result must be empty.

Then run:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

Evidence must record the exact implementation SHA and Java/i2pd pins. A later status-only closure commit is expected.

Do not run Streaming unless raw Destination reverse delivery passes; even then Streaming qualification remains later work.

## 17. Failure / cancellation / restart semantics

- fresh scratch Java RouterContexts for every authoritative run;
- no reuse of typed-fact files;
- loopback-only bounded diagnostic ports;
- missing response = Unknown;
- parse failure = Unknown;
- router exit = environment/runtime failure, not protocol classification;
- preserve Plan 217 process cleanup;
- one baseline run is sufficient if complete;
- at most one exact-head repeat to assess nondeterminism;
- no topology tuning between repeats.

## 18. Compatibility and migration

No user-facing compatibility or support change.

Evidence authority changes only:

- Plan 219 J219-B attribution becomes superseded historical evidence;
- Plan 220 governs corrected diagnostic attribution;
- downstream work consumes Plan 220, not J219-B.

## 19. Required tests

Focused tests must cover:

- protocol-derived RouterHash equality against Java self snapshot;
- mismatched hash yields diagnostic failure, not A-missing-B;
- stored RI presence and `f` represented independently;
- stale/current RI SHA and published timestamp;
- PeerManager and selector fields independent;
- Unknown → observability-gap;
- earliest Known(fail) wins only after earlier Known(pass);
- missing fact cannot yield root-cause;
- forward receive cannot satisfy reverse dispatch;
- exact epoch ordering.

If production-only J219 structures are removed, delete/relocate their tests in the same commit.

## 20. Verification commands

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh

cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run

test -z "$(git status --porcelain=v1)"
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

The M6 final closure checker is expected to remain blocked unless a later qualification plan closes Java-family interop.

## 21. Acceptance criteria

Plan 220 closes only when:

1. Plan 219 J219-B is explicitly superseded, not erased.
2. useful read-only diagnostics are retained/replaced.
3. no terminal fact is sourced pre-bootstrap.
4. driver samples A view after its own A/B bootstrap and before reverse send.
5. B hash derives from validated RouterInfo identity / Java RouterHash, not byte slicing.
6. Rust and Java B RouterHash views are byte-equal.
7. no standard/I2P Base64 ambiguity remains.
8. stored-B fact proves exact identity.
9. stored-B-`f` proves presence and `f`.
10. stored/live RI SHA/published/capabilities are explicit.
11. PeerManager and selector are distinct observations.
12. selector is not synthesized from PeerManager.
13. client-NetDB/OCMOSJ facts are observed or Unknown.
14. forward evidence cannot satisfy reverse dispatch.
15. classifier uses Known/Unknown semantics.
16. unknown earlier stage produces observability-gap.
17. Plan-219-only production classifier surface is removed or justified by a real runtime consumer.
18. static guards cover D220-1..D220-9.
19. implementation is committed before external run.
20. authoritative run starts clean.
21. exact implementation SHA is recorded correctly.
22. one corrected terminal outcome is emitted.
23. no topology/bootstrap/protocol corrective is implemented because of old J219-B.
24. Plan 193 and Plan 215 authority remain unchanged.
25. closure names the smallest next corrective implied by corrected evidence.

## 22. Stop conditions

Stop with an observability gap if a required stage cannot be observed without Java patching, reflection/private-state access, NetDB/tunnel mutation, public-network participation, or i2pr production-wire change.

If corrected evidence truly reproduces the old logical J219-B boundary, stop and register **Plan 221** for the actual bootstrap/topology corrective. Do not implement it here.

If the boundary moves to PeerManager, selector, client-NetDB, OCMOSJ, or i2pr inbound handling, stop and register the correspondingly narrow Plan 221.

If reverse raw Destination delivery passes, stop and register a fresh Java final-qualification plan.

## 23. Closure evidence required

`plans/closure/mixed-router-interop/220-status.md` must include:

- implementation commit SHA;
- clean-worktree proof;
- Java/i2pd pins and cache fingerprints;
- D220-1..D220-9 remediation matrix;
- RouterHash cross-check;
- epoch timeline;
- exact A-stored-B evidence;
- PeerManager vs selector evidence;
- client-NetDB/OCMOSJ evidence or observability gap;
- corrected reverse-dispatch evidence;
- production-surface cleanup disposition;
- terminal Plan 220 classification;
- verification commands/outcomes;
- findings by severity;
- unblock audit for Plans 201, 204, 205, 218, 219 and M6 roadmap;
- exact next-plan scope.

## 24. Handoff

```text
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = registered-ready-m6-java-plan219-diagnostic-attribution-corrective

plan_201 = blocked-pending-plan220-corrected-java-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective

next_executable_plan = 220-m6-java-plan219-diagnostic-attribution-corrective
```

No bootstrap/topology corrective is registered by this plan.
