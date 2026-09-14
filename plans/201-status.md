# Plan 201 status — Java publication corrective and M6 second-family closure

Status: **`in-progress-branch-g-framework-landed-blocked-on-exact-head-external-run`**.

Plan of record: [`201-m6-java-public-client-publication-corrective-and-second-family-closure.md`](201-m6-java-public-client-publication-corrective-and-second-family-closure.md).

Plan 200 has closed the diagnostic/evidence side of the lane (helper
`READY` decoupled from any `leaseset=published` claim; bounded
`REPORT_STATUS`; post-bootstrap RouterInfo lookup proofs in both
directions; sanitized Java client LeaseSet lifecycle / tunnel /
floodfill / store / ack keys; exactly one terminal `P200-{A..H}`
classification per run). Plan 201 has landed its **Branch G
(store-acked-remote-lookup-fails) corrective framework** so the
external driver can attribute a stuck Java second-family LeaseSet2
publication to a specific lookup-path boundary without weakening
the existing i2pd first-family invariants.

The Plan 200 §11 `P200-*` classification has not yet been recorded
against the exact-pinned Java I2P 2.13.0 cache (the full external
run requires the `bash scripts/interop/fetch-m6-java.sh --rebuild`
build + the cross-family external workflow). Plan 201 may pick a
specific corrective branch (G is the most likely candidate based
on the Plan 198/199 evidence — Java stores the LS2 in its client
subDB but the i2pr NetDB lookup never receives it) only after Plan
200 records an unambiguous terminal classification on the closing
exact head.

## What Plan 201 changed

```text
crates/i2pr-daemon/src/destination_tunnels.rs
  Plan 201 §G — eleven new sanitized observation counters on
  DestinationTunnelCounters (lookup_key_matches, lookup_key_mismatches,
  floodfill_candidates_present, floodfill_candidates_absent,
  reply_paths_derived, reply_paths_unresolved, ls2_records_decoded,
  ls2_records_decode_rejected, ls2_records_signature_rejected,
  inbound_cells_garlic_completed, inbound_cells_garlic_incomplete).
  The counters advance only on positive observations of the matching
  boundary so a Branch G probe can tell apart "decode succeeded but
  seam rejected" from "decode failed" from "validation rejected".
  begin_lease_lookup advances floodfill_candidates_* /
  reply_paths_*.  ingest_tunnel_lease_store peeks the response
  body, advances ls2_records_decoded + lookup_key_* when the body
  shape was a LeaseSet2, and advances ls2_records_signature_rejected
  only when a LeaseSet2 body Continue'd out of the seam. recover_garlic_bytes
  advances inbound_cells_garlic_completed / _incomplete.
  Public API note_lookup_boundary(label, value) exposes the same
  counter set as a typed observation surface for external drivers.
  Unknown labels and values are silently ignored so a future
  expansion of the documented set must update both the helper
  and the static checker.

crates/i2pr-daemon/tests/destination_tunnel_unit.rs
  Plan 201 §G — eight new unit rows exercising the Branch G
  observation surface (floodfill-candidates-absent,
  floodfill-candidates-present, lookup-key-match happy path,
  lookup-key-mismatch path, signature-rejected on tampered LS2,
  decode-rejected on RouterInfo body, note_lookup_boundary
  documented set, note_lookup_boundary unknown label rejection).
  The Branch G surface is now covered at the unit level so a future
  regression cannot silently advance a counter.

crates/i2pr-daemon/tests/java_tunnel_external.rs
  Plan 201 §G — the destination + Streaming drivers now emit the
  p201-lookup-boundary-* sanitized observation keys at every
  lookup-path boundary.  The Branch G counter snapshot is captured
  after begin_lease_lookup and after every ingest_tunnel_lease_store
  call so the harness can attribute a stuck Branch G lookup to a
  specific boundary without re-running the diagnostic.

tests/integration/m6-interop/run-java.sh
  Plan 201 §G — six new blocked_row entries that flip to passed
  when the corresponding p201-lookup-boundary-* key is emitted
  (external-p201-lookup-floodfill-present,
  external-p201-lookup-reply-gateway-derived,
  external-p201-lookup-ls2-key-match,
  external-p201-lookup-ls2-decoded,
  external-p201-lookup-ls2-signature-rejected,
  external-p201-inbound-garlic-completed).  The rows stay blocked
  with the documented Plan 198/199 stop provenance until a Plan 200
  external run produces the p201-* keys; the static checker below
  rejects a hard-coded passed row.

scripts/check-m6-mixed-router-acceptance-evidence.sh
  Plan 201 §G — new §11 + §12 invariants enforce the Branch G
  observation surface (note_lookup_boundary exists; documented
  label set is exhaustive; counter set is exhaustive; eight Plan 201
  §G unit rows exist).  The checker rejects a future regression
  that silently removes the observation surface.
```

## Implementation scope

Plan 201 landed Branch G only. Branches A-F stay open in case the
Plan 200 terminal classification exposes a different first-failing
boundary; each branch's corrective is bounded by the Plan 201 §3
table and cannot widen beyond the i2pd-first-family invariants
already proven by Plans 184/186/187/188/190/191/192/193.

```text
branch_a_router_a_missing_router_b = registered-corrective-not-implemented
branch_b_router_b_missing_router_a = registered-corrective-not-implemented
branch_c_client_ls2_not_created_or_current = registered-corrective-not-implemented
branch_d_client_tunnel_publication_path_unavailable = registered-corrective-not-implemented
branch_e_no_eligible_floodfill_candidate = registered-corrective-not-implemented
branch_f_store_sent_no_ack = registered-corrective-not-implemented
branch_g_store_acked_remote_lookup_fails = implemented-framework-landed-blocked-on-exact-head-external-run
branch_h_publication_path_passed = registered-no-corrective-needed
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
plan_198 = blocked-public-java-client-leaseset2-publication-superseded-into-plan199
plan_199 = blocked-execution-decomposed-into-plans-200-through-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = in-progress-branch-g-framework-landed-blocked-on-exact-head-external-run
plan_202 = registered-executable-m10-remote-router-composition (parallel-executable-with-plan200)
plan_203 = registered-blocked-by-plan202
plan_204 = registered-blocked-by-plan201-and-plan203

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (diagnostic-frame-landed-via-plan200; Branch-G-framework-landed-via-plan201; final closure blocked-on-exact-head-external-run)
milestone6_interoperable = not-yet-claimed
m6_java_publication_observability = landed-via-plan200
m6_java_publication_branch_g_framework = landed-via-plan201

next_executable_plan = 200-external-run (consume terminal P200 classification) or 202 (M10 remote transport composition; parallel-executable)
remaining_sequence = 200-external-run -> 201-branch-g-finalize-or-pivot-to-branch-{a..f} -> 204-convergence
```

## Required validation

```text
cargo fmt --all --check                                                OK
cargo check --locked --workspace --all-targets                         OK
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit     40 passed (Plan 187 32 + Plan 190 0 reused + Plan 201 8 new)
cargo test --locked -p i2pr-daemon --test java_tunnel_external        1 passed, 3 ignored (fail-closed ordinary invocation)
cargo test --locked --workspace --all-targets -- --test-threads=1    2341 passed, 9 ignored (the +8 from Plan 201 §G)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps   OK
cargo test --locked --workspace --doc                                0 passed (16 suites)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh            OK (11 guarded labels + Plan 197 §8 pq parser tolerance + Plan 201 §G observation surface)
bash tests/integration/m6-interop/run-java.sh                        external-session-established-java BLOCKED pending Plan 200 external run; the new Plan 201 rows stay blocked until the external run emits the p201-* keys
cargo deny check advisories bans sources                              OK
```

## Handoff rule

Plan 201 must not claim final closure until:
1. Plan 200 records exactly one unambiguous terminal
   `P200-{A..H}` classification against the exact-pinned Java I2P
   2.13.0 cache;
2. the matching Plan 201 branch land the narrowest corrective for
   that classification (Branch G is the most likely candidate;
   Branch G's framework is already landed and only requires the
   exact-head external run to confirm the seven §11 stop rows flip
   `blocked → passed`);
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

On Plan 201 pass, and only then, authority becomes:

```text
plan_201 = passed-m6-java-public-client-publication-corrective-and-second-family-closure
plan_204 = unblocked-convergence
plan_202 = unblocked-with-plan203
remaining_sequence = plan204-convergence -> closed-m6-via-plan193-and-plan201
```
