# Plan 217 — M6 Java closure harness and evidence corrective

Status: **registered-ready-m6-java-closure-harness-corrective**.

## 1. Objective

Correct the Java second-family test harness and its evidence authority after the
Plan 216 diagnostic exposed contradictions between the Rust control flow, the
sanitized Java log counters, and the recorded blocker narrative.

This is a **test/harness corrective only**. It must establish a trustworthy
closure lane before any new Java helper architecture is attempted.

The bounded outcome is:

1. repair tunnel ownership/lifetime assertions in
   `crates/i2pr-daemon/tests/java_tunnel_external.rs`;
2. make the Java lifecycle evidence describe actual Java I2P 2.13.0 events
   rather than stale/negative grep aliases;
3. normalize the disposable Java test topology enough that peer/tunnel
   eligibility is observable and deterministic;
4. isolate destination and Streaming tunnel identifiers so one driver cannot
   poison the next driver on the same long-lived Java RouterContexts; and
5. obtain one fresh exact-head Java run whose terminal result is a real
   protocol/reference boundary or a clean continuation into Plan 218.

Plan 217 MUST NOT claim M6 Java interoperability. Plan 218 owns final
qualification and authority promotion.

## 2. Why this plan is ready

Hard dependencies are already available:

- i2pd first-family M6 Streaming is closed by Plan 193;
- Java controlled topology is established by Plans 196/200/201;
- exact Java I2P 2.13.0 remains pinned at
  `9134f808337b401e8e53c73734c81fab04280c9d`;
- Plan 201 Branch A standard-I2NP/gzip decode corrective is landed;
- Plan 201 Branch G lookup-boundary counters are landed;
- Plan 216 captured a reproducible destination-driver panic and Streaming stop.

Plan 216 itself is diagnostic-only and remains unchanged. This plan corrects the
interpretation and machinery around it.

## 3. Current implementation evidence and discovered defects

### 3.1 Outbound tunnel ownership assertion is stale

In `destination_message_plane_against_java`, the driver first proves:

```text
coord.registry().outbound_len() == 1
coord.registry().inbound_len() == 1
```

It then intentionally transfers ownership:

```rust
let gateway_role = coord
    .registry_mut()
    .remove_outbound(outbound_slot)
    .expect("real outbound role");

let destination_outbound =
    DestinationOutboundRole::from_role(gateway_role, ...);
```

The transferred `DestinationOutboundRole` is then used by
`compose_lookup_via_tunnel`. The coordinator registry is therefore expected to
contain **zero** outbound roles after the transfer.

The current driver later executes:

```rust
assert_eq!(coord.registry().outbound_len(), 1);
```

and then repeats registration/ownership setup, including another
`remove_outbound(outbound_slot)`.

Plan 216 reported the first stale assertion as a new registry disappearance.
That diagnosis is incompatible with the test's ownership transfer.

### 3.2 Reaching the stale assertion implies the LS2 lookup completed

Immediately before the stale assertion the driver has:

```rust
let summary = if let Some(s) = lease_summary {
    s
} else {
    record_stop(... "client-ls2-local-but-not-network-visible" ...);
    ...
    return;
};
```

The only successful assignment is
`LeaseStoreIngestOutcome::Completed { summary, .. }`.

Therefore the Plan 216 execution could not reach the stale
`outbound_len() == 1` assertion unless the Java Destination lookup had already
returned a DatabaseStore that i2pr decoded and accepted as a completed LS2
lookup. This does **not** authorize a passed M6 claim without a corrected rerun,
but it invalidates the previous assertion that Plan 216 proved "no LS2 reply
ever arrives."

### 3.3 Java lifecycle grep counters are not closure-quality evidence

`tests/integration/m6-interop/run-java.sh` derives lifecycle counters from
patterns that do not consistently match exact-pinned Java 2.13.0 log strings.
Relevant pinned-source messages include forms such as:

- `Publishing: ...`
- `New lease set granted for destination ...`
- `Queueing to publish at ...`
- `Publishing LS for ...`
- `sending encrypted store through client tunnel to ...`

The existing counter surface also mixes positive labels with negative strings
such as "No outbound tunnels available" / "No floodfill peers". A count of a
negative diagnostic must never satisfy a positive acceptance row.

Protocol evidence is stronger than inferred log evidence. A matching,
signature-valid LS2 returned through a real inbound tunnel and accepted by
`ingest_tunnel_lease_store()` is authoritative for remote visibility.

### 3.4 Peer-selection attribution is incomplete

Pinned Java 2.13.0 `ProfileOrganizer.selectFastPeers()` cascades from fast
peers to high-capacity peers, active not-failing peers, and not-failing peers.
An empty `_fastPeers` set alone is not proof of the terminal blocker.

`TunnelPeerSelector.shouldExclude()` and `ProfileOrganizer.isSelectable()`
also consider RouterInfo availability, hidden/unreachable state, bandwidth and
congestion capability, key types, version, transport reachability, and
closest-hop restrictions.

The controlled lane therefore needs sanitized RouterInfo capability and
eligibility evidence for each Java router before assigning a failure to
ProfileOrganizer scoring.

### 3.5 Controlled NetDB path is malformed

The controlled launcher supplies an absolute
`router.networkDatabase.dbDir`, while pinned Java constructs the path relative
to `i2p.dir.router`. This produced the already-recorded doubled path. Use a
relative test NetDB directory consistent with Java's own testnet/MultiRouter
usage.

### 3.6 Destination and Streaming runs reuse tunnel/build identifiers

The destination and Streaming drivers run against the same long-lived Java
RouterContexts and reuse fixed tunnel/build identifiers. Java retains tunnel
state for the normal tunnel lifetime and has duplicate build/tunnel-ID
rejection machinery. Plan 216's Streaming stop therefore must be tested after
identifier isolation before it can be attributed to Java protocol behavior.

## 4. Invariants that must not regress

- No public I2P network participation.
- Exact stock Java I2P 2.13.0 pin; no source patching or vendoring changes.
- No private Java NetDB/tunnel-state injection, reflection shortcut, VMComm, or
  direct LS2 copy.
- i2pr production wire/code MUST NOT change merely to make the external lane
  green. A production change requires captured protocol evidence of an i2pr
  defect and a separate plan-of-record if it expands this corrective.
- i2pr SSU2 remains loopback-only/non-advertised in this lane.
- Environment-gated tests remain ignored by ordinary workspace runs and fail
  closed when required external environment variables are absent.
- Raw reference logs remain non-authoritative; only sanitized facts may enter
  evidence artifacts.
- Plan 193 i2pd first-family evidence must not regress.

## 5. Scope

### In scope

- `crates/i2pr-daemon/tests/java_tunnel_external.rs`
- `tests/integration/m6-interop/run-java.sh`
- `tests/integration/m6-interop/java/ControlledRouter.java`
- M6 evidence/static checker scripts directly governing this lane
- manual workflow wiring only if needed to preserve lane isolation
- targeted tests for ownership transfer, evidence classification, and unique
  tunnel/build ID namespaces

### Explicitly out of scope

- rewriting Java helpers to SAM;
- changing i2pr production NetDB, Streaming, tunnel, SSU2, Garlic, or crypto
  behavior absent new protocol evidence;
- patching Java I2P;
- joining the public I2P network;
- broad refactors of the M6 harness;
- M11 planning;
- Plan 204 documentation convergence.

## 6. Required changes / ordered work packages

### A. Repair destination-driver ownership flow

1. Treat `remove_outbound(outbound_slot)` as an ownership transfer.
2. After transfer, assert the owned `DestinationOutboundRole` is usable rather
   than asserting the coordinator still contains that outbound slot.
3. Remove the duplicated post-lookup registration/second-removal/second-lookup
   block.
4. After the first completed LS2 lookup, continue directly with:
   - summary destination + lease validation;
   - local LS2 publication;
   - outbound raw Destination delivery;
   - reference receipt;
   - inbound reply;
   - cleanup.
5. Add a regression assertion/test ensuring one installed outbound tunnel is
   transferred exactly once.

### B. Repair evidence semantics

1. Audit every Java lifecycle grep against the exact pinned Java source.
2. Split positive observations from negative/failure observations.
3. No positive row may be satisfied by a string beginning with or semantically
   equivalent to "No ...".
4. Prefer protocol-derived driver rows for:
   - remote LS2 visibility;
   - LS2 decode/signature/key match;
   - real outbound/inbound tunnel installation;
   - application delivery.
5. If log-derived lifecycle counters are retained, label them diagnostic and
   document the exact pinned Java string(s) that increment them.
6. Replace first-occurrence `awk` terminal classification with one final
   bounded snapshot after bootstrap/helper readiness. One run produces one
   terminal classification.

### C. Normalize controlled Java topology

1. Use a relative Java NetDB directory under each disposable router data dir.
2. Preserve distinct roles:
   - service/client router;
   - publication/floodfill router;
   - pure tunnel participant.
3. Do not blanket-enable floodfill on the pure tunnel participant.
4. Adopt only the relevant safe settings from Java's supplied testnet config:
   loopback/local transport allowance, deterministic non-firewalled local
   posture where required, blocklist/reseed isolation, explicit share/bandwidth
   settings sufficient to avoid accidental lowest-tier exclusion.
5. Do **not** change the network ID unless the entire i2pr controlled identity
   path is deliberately migrated under a separately justified change.
6. Record sanitized RouterInfo capabilities for A/B/C and a derived
   `tunnel-peer-selectable=true|false reason=...` fact.

### D. Isolate build/tunnel identifiers across drivers

Use deterministic disjoint identifier namespaces (preferred) or fresh Java
RouterContexts per driver. At minimum:

- destination and Streaming drivers must not reuse receive/send/creator tunnel
  IDs;
- short-build message IDs must not collide across the two drivers;
- add sanitized Java duplicate-build/rejection observations;
- preserve reproducibility; do not depend on unlogged randomness.

### E. Focused requalification

Run the destination driver first. It must terminate in exactly one of:

- clean completion through raw bidirectional delivery; or
- a named protocol/reference stop supported by command-derived evidence.

It must never terminate because of stale ownership assertions, duplicated setup,
misclassified log counters, or test-to-test identifier collisions.

Then run Streaming in an isolated identifier namespace and classify the first
real boundary if it does not complete.

## 7. Failure, cancellation, restart, and contention semantics

- Every external process retains bounded startup/ready/cleanup deadlines.
- A failed helper/router must cause the lane to fail/record a named stop, never
  silently continue as passed.
- Cleanup must terminate all disposable Java routers/helpers and leave no
  listener/process that can influence a rerun.
- Evidence files are recreated per run; stale evidence must not be consumed.
- Destination and Streaming sub-runs must have disjoint tunnel/build
  identifiers even when the same Java processes remain alive.
- If identifier isolation cannot be proven, restart Java RouterContexts between
  sub-runs rather than lengthening sleeps.

## 8. Compatibility and migration

No product migration. No public config contract changes. Changes are confined to
the external interoperability harness and its evidence schema.

If evidence labels are renamed, update all M6 checkers and final evidence
aggregation in the same commit; do not retain aliases that can make stale
artifacts pass.

## 9. Required tests and exact verification

Focused local/static floor:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc

bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh
cargo deny check advisories bans sources
```

External corrective lane on the exact implementation head:

```bash
bash tests/integration/m6-interop/run-java.sh
```

The implementation agent SHOULD add a bounded selector to
`run-java.sh` (or equivalent direct ignored-test invocation) so the destination
and Streaming sub-runs can be executed independently during diagnosis without
duplicating the topology setup.

## 10. Documentation updates

On implementation:

- write `plans/closure/mixed-router-interop/217-status.md` with actual command
  outcomes and the requirement-to-evidence matrix;
- update the mixed-router roadmap and registry;
- do not edit Plan 216's point-in-time diagnostic;
- if corrected evidence disproves a Plan 201 blocker, update Plan 201 status
  authority without deleting its historical execution narrative.

## 11. Acceptance criteria and stop conditions

Plan 217 passes only when all are true:

1. no code path asserts `outbound_len() == 1` after that slot was transferred
   out of the coordinator;
2. each real outbound role is removed/transferred exactly once;
3. the duplicate post-lookup setup/second-removal block is gone;
4. `lease-lookup-completed` can only be emitted after
   `LeaseStoreIngestOutcome::Completed`;
5. a successful completed lookup is authoritative evidence that the remote LS2
   was network-visible to the queried Java router;
6. positive/negative Java lifecycle evidence is semantically separated;
7. every retained log-derived counter names an exact pinned-source log pattern;
8. final P200 classification is taken once from a final snapshot, not the first
   line in an evolving stream;
9. Java NetDB directory construction no longer produces the doubled absolute
   path;
10. Java router roles and capability strings are recorded and tunnel
    eligibility has a reasoned sanitized result;
11. destination and Streaming build/tunnel/message identifiers cannot collide
    within one `run-java.sh` execution;
12. ordinary workspace invocation remains fail-closed/ignored as designed;
13. static M6 evidence checkers are updated for the corrected schema and pass;
14. a fresh exact-head Java run reaches a genuine protocol/reference boundary
    or completes the corrected destination/Streaming probes without harness
    panic;
15. no Java patch/public-network/private-state shortcut was introduced;
16. no i2pr production-wire change was made solely to satisfy the lane.

**Stop condition:** if a corrected run exposes a new production i2pr protocol
defect, stop Plan 217 after preserving the minimal transcript. Do not broaden
this harness corrective into product implementation.

## 12. Closure evidence required and handoff

The Plan 217 closure record must include:

- implementation commit(s);
- before/after control-flow explanation for the outbound ownership bug;
- exact evidence-schema changes;
- Java capability/eligibility facts from the corrected run;
- destination and Streaming terminal outcomes;
- all commands run and whether local or CI/hosted;
- unresolved findings by severity;
- unblock audit.

On Plan 217 success:

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = ready-m6-java-second-family-final-qualification
plan_205 = retained-deferred-conditional-fallback
```

Proceed to Plan 218. Do not execute Plan 205 unless Plan 218 records a genuine
direct-I2CP Java/reference boundary after the corrected harness is in place.
