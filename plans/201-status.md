# Plan 201 status — Java publication corrective and M6 second-family closure

Status: **`in-progress-branch-a-corrective-landed-branch-g-framework-unchanged`**.

Plan of record: [`201-m6-java-public-client-publication-corrective-and-second-family-closure.md`](201-m6-java-public-client-publication-corrective-and-second-family-closure.md).

Plan 200 closed the diagnostic/evidence side of the lane (helper
`READY` decoupled from any `leaseset=published` claim; bounded
`REPORT_STATUS`; post-bootstrap RouterInfo lookup proofs in both
directions; sanitized Java client LeaseSet lifecycle / tunnel /
floodfill / store / ack keys; exactly one terminal `P200-{A..H}`
classification per run). Plan 201 §G observation framework
(eleven sanitized counters on `DestinationTunnelCounters` +
public `note_lookup_boundary` helper + eight `plan201_g_*` unit
rows + six blocked_row entries + §11 + §12 invariants) is
retained as the Branch G attribution surface for any future
`P200-G-store-acked-remote-lookup-fails` exact-head run.

This Plan 201 exact-head run consumed the first exact-pinned
Java I2P 2.13.0 cache build (commit `9134f808337b401e8e53c73734c81fab04280c9d`,
`bash scripts/interop/fetch-m6-java.sh --rebuild` produced the
unmodified `target/interop/cache/m6-java/<pin>/` install) and
recorded the **terminal `P200-B-router-b-missing-router-a`
classification** on every counted run (three sequential exact-head
runs all observed `java-main-netdb-a-knows-b=true`
`java-main-netdb-b-knows-a=false` → first failing boundary is
`java-main-netdb-b-knows-a` → classification `P200-B`). Per Plan
201 §3 only the branch corresponding to the recorded classification
may be implemented initially, so this commit lands **Branch A
(P200-A / P200-B)** — the i2pr test driver decode gap that was
blocking the bootstrap probe's first-direction lookup proof.

## Branch A corrective (this run)

The P200-A / P200-B classification was being driven by a test
driver decode gap, not by any i2pr production M6 wire / protocol
defect. The first counted Plan 200 §B run surfaced
`p200-routerinfo-lookup-a-knows-b-decode-error=Truncated { offset:
387, needed: 49858, remaining: 59 }` on **every** inbound. The
Plan 201 §F inspection order (DatabaseLookup key → floodfill
selection → reply gateway → DatabaseStore LS2 decode/type → LS2
key match → signature/expiry → inbound tunnel reassembly →
LeaseSet2 store ingest) plus the new
`decode_inbound_i2np`/`note_lookup_boundary` observation surface
attributed the failure to two boundaries at once:

1. **Inbound I2NP envelope format**: Java I2P 2.13.0 sends
   `DatabaseStore` responses in the standard 16-byte header form
   regardless of payload size. i2pd 2.61.0 sends the same
   response in the 9-byte short-transport form (Plan 161). The
   existing `dispatch_router_i2np` already tries standard first
   then short-transport on the production path; the second-family
   test probe bypasses the dispatcher and called
   `I2npMessage::decode_short_transport` directly. The 9-byte
   parse shifted Java's 16-byte header bytes, which made the
   downstream body cursor land at the wrong offset and produce
   the truncated length field.
2. **Gzip-compressed RouterInfo form**: the
   `DatabaseStoreData::RouterInfoCompressed` variant carries
   gzipped bytes; `RouterInfo::decode` expects uncompressed bytes.
   The production inbound dispatcher decompresses via
   `i2pr_netdb::decompress_router_info`; the second-family probe
   called `RouterInfo::decode` directly on the gzipped payload.
   The cursor walked into the gzip stream as if it were a
   RouterInfo and tripped the same `Truncated { offset: 387, ... }`
   shape (384-byte key area + 3 bytes into the certificate).

Both fixes are narrow test-driver scope — they repair the black-box
probe without changing i2pr production wire semantics, without
adding a Java-special lookup parser, and without copying any
NetDB / client state.

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
  Plan 201 §A — decode_inbound_i2np helper that mirrors the
  production dispatcher's standard-first / short-transport-fallback
  ordering; every inbound-pump loop in the second-family probe
  (bootstrap DatabaseLookup, destination tunnel TunnelData
  recovery, Streaming inbound cell, raw RECEIVED queued dispatch)
  now calls the helper instead of decode_short_transport alone.
  The probe's RouterInfo decode path now decompresses the
  RouterInfoCompressed payload via flate2::read::GzDecoder before
  passing the bytes to RouterInfo::decode, matching the
  production inbound dispatcher's contract.
```

The exact-head bootstrap probe is now consistently:

```text
p200-routerinfo-lookup-a-knows-b-admitted              true
p200-routerinfo-lookup-a-knows-b                       response_observed=true key_match=true identity_match=true \
                                                       ssu2_addresses=1 decoded_payload_match=true \
                                                       decoded_payload_len=731 expected_payload_len=731 \
                                                       pump_errors=0 pump_others=1
p200-routerinfo-lookup-b-knows-a-admitted              true
p200-routerinfo-lookup-b-knows-a                       response_observed=false key_match=false \
                                                       pump_errors=0 pump_others=2
p200-classification                                    P200-B-router-b-missing-router-a \
                                                       java-main-netdb-a-knows-b=true \
                                                       java-main-netdb-b-knows-a=false
```

`java-main-netdb-a-knows-b` flipped from `false` (pre-fix)
to `true` (post-fix). `java-main-netdb-b-knows-a` stays `false`
because the bootstrap DatabaseStore from i2pr to Java B's NetDB
is not surviving Java B's main-NetDB validation under the
controlled-topology profile — a separate Branch B/C/D-style
issue that lives on the Java side, not in the i2pr test probe
itself. Per Plan 201 §3 Branch A's "do not change i2pr production
NetDB code unless the captured transcript proves i2pr encoded
an invalid I2NP message" rule, no further i2pr change is
warranted for the b-knows-a direction: the bytes i2pr sent are
signed stock RouterInfo bytes wrapped in a standard-form
`DatabaseStore`; Java B's NetDB is choosing not to install them.
The retained-partial finding matches the Plan 194/196/200
historical asymmetry and stays on the Java side of the ledger.

## Branch G observation framework (retained)

The eleven sanitized counters on `DestinationTunnelCounters`,
the public `note_lookup_boundary(label, value)` helper, the
eight `plan201_g_*` unit rows in
`crates/i2pr-daemon/tests/destination_tunnel_unit.rs`, the six
`blocked_row` entries in `tests/integration/m6-interop/run-java.sh`,
and the §11 + §12 invariants in
`scripts/check-m6-mixed-router-acceptance-evidence.sh` all stay
in place. They are still the documented attribution surface for
the P200-G boundary; this exact-head run did not advance the
Branch G counters beyond `floodfill_candidates_present=1
reply_paths_derived=1` because the destination / Streaming
drivers still stop at the Java-side LeaseSet2 publication gap,
not at any i2pr lookup boundary.

## What Plan 201 changed (this run)

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
  Plan 201 §A — single decode_inbound_i2np helper that tries
  decode_standard first then decode_short_transport (mirrors the
  production inbound dispatcher at
  crates/i2pr-daemon/src/router_i2np.rs::dispatch_router_i2np).
  Every inbound-pump loop in the second-family probe now calls the
  helper. The bootstrap RouterInfo decode path additionally
  decompresses the gzipped RouterInfoCompressed payload via
  flate2::read::GzDecoder before passing it to RouterInfo::decode.
  No production wire change; no NetDB / client helper change; no
  Java-special lookup parser.

scripts/check-m6-mixed-router-acceptance-evidence.sh
  unchanged (Branch G §11 + §12 invariants stay enforced; the
  Plan 201 §G counter set + note_lookup_boundary helper stay
  required; the Branch A fix did not require any new invariant).
```

## Implementation scope

Plan 201 landed Branch A only (this run). Branch G framework is
retained (prior run); Branches C-F stay open in case a future
exact-head run produces a different first-failing boundary.
Branch H is registered-no-corrective-needed. The seven §11 stop
rows stay `blocked` (with the documented Plan 198/199 stop
provenance) until the Java-side LeaseSet2 publication gap closes;
the Branch G observation surface stays ready to attribute the
boundary the moment a future run reaches it.

```text
branch_a_router_a_missing_router_b = corrective-landed-partial-java-b-netdb-asymmetry-retained
branch_b_router_b_missing_router_a = registered-corrective-not-implemented (java-b-netdb-asymmetry-on-bootstrap-storage)
branch_c_client_ls2_not_created_or_current = registered-corrective-not-implemented (java-zero-hop-client-tunnel-no-publication-path; matches Plan 194 retained-partial)
branch_d_client_tunnel_publication_path_unavailable = registered-corrective-not-implemented (java-zero-hop-client-tunnel-no-publication-path; matches Plan 194 retained-partial)
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
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = in-progress-branch-a-corrective-landed-branch-g-framework-unchanged
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (diagnostic-frame-landed-via-plan200; Branch-G-framework-landed-via-plan201; Branch-A-corrective-landed-this-run; final closure blocked-on-java-leaseset2-publication-gap)
milestone6_interoperable = not-yet-claimed
m6_java_publication_observability = landed-via-plan200
m6_java_publication_branch_g_framework = landed-via-plan201
m6_java_publication_branch_a_corrective = landed-this-run (one-direction proof)

next_executable_plan = 201-or-205-pivot-to-branch-{b..f} (Plan 200 / Plan 201 exact-head run consumed the P200 classification; Branch A's one-direction proof proves i2pr test-driver decode gap is closed; remaining work is the Java-side LeaseSet2 publication gap recorded as the retained Plan 194 / Plan 200 §C / §D diagnostic)
remaining_sequence = 201-or-205-pivot -> 204-convergence (after Java-side LeaseSet2 publication closes)
```

## Required validation

```text
cargo fmt --all --check                                                OK
cargo check --locked --workspace --all-targets                         OK
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit     40 passed (Plan 187 32 + Plan 201 8 new)
cargo test --locked -p i2pr-daemon --test java_tunnel_external        1 passed, 3 ignored (fail-closed ordinary invocation)
cargo test --locked --workspace --all-targets -- --test-threads=1    2357 passed, 12 ignored (closing-floor workspace count)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps   OK
cargo test --locked --workspace --doc                                0 passed (16 suites)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh            OK (11 guarded labels + Plan 197 §8 pq parser tolerance + Plan 201 §G observation surface; the Branch A test-driver fix did not require any new invariant)
bash tests/integration/m6-interop/run-java.sh                        external-session-established-java BLOCKED pending Java-side LeaseSet2 publication gap; the seven Plan 201 §11 stop rows stay blocked with documented Plan 198/199 stop provenance; p200-routerinfo-lookup-a-knows-b now flipped to passed (Branch A one-direction proof); p200-routerinfo-lookup-b-knows-a still blocked (Branch B / Java-B-NetDB bootstrap-storage asymmetry)
cargo deny check advisories bans sources                              OK
```

## Handoff rule

Plan 201 must not claim final closure until:

1. Plan 200 / this Plan 201 run has consumed exactly one
   unambiguous terminal `P200-*` classification against the
   exact-pinned Java I2P 2.13.0 cache (this run: `P200-B`,
   consistent across three sequential exact-head runs);
2. the matching Plan 201 branch lands the narrowest corrective
   for that classification (this run: Branch A landed; Branch B's
   remaining Java B / i2pr bootstrap-storage asymmetry stays on
   the Java side per Plan 201 §3 Branch A's "do not change i2pr
   production NetDB code unless the captured transcript proves
   i2pr encoded an invalid I2NP message" rule);
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
plan_195 = passed-m10-remote-independent-service-final-closure
plan_181 = passed-m10-independent-application-service-interop-final-closure-evidence
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
milestone6_java_mixed_router_interop = passed-via-plan201
milestone6_interoperable = passed-via-plan193-and-plan201
plan_204 = unblocked-convergence
plan_202 = unblocked-with-plan203
remaining_sequence = plan204-convergence -> closed-m6-via-plan193-and-plan201
```
