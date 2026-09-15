# Plan 205 status — Java I2P SAM-bridge helper pivot for Plan 201 final closure

Status: **registered-blocked-on-plan198-publication-boundary**.

Plan of record: [`205-m6-java-sam-bridge-helper-pivot.md`](205-m6-java-sam-bridge-helper-pivot.md).

Plan 205 is the documented next executable plan after Plan 201.
Plan 201 retained the Java-side LeaseSet2 publication gap as the
authoritative blocker and proved bounded by stock Java I2P 2.13.0's
`ProfileOrganizer._thresholdSpeedValue` (line 161) + `_fastPeers`
promotion (line 913) under controlled loopback topology, plus
i2pd's SAM bridge LS2 publication gap documented in Plan 194.

The remaining work after Plan 201 is to pivot the Java second-family
helpers from direct I2CP (`ReferenceRawDestination.java` +
`ReferenceStreamingService.java`) to the public Java SAM bridge
path, mirroring what i2pd's SAM bridge does successfully in the
Plan 202/203 reference destination path.

Plan 205 inherits from previous plans:
- Plan 200 diagnostic/evidence side (helpers decoupled `READY` from
  `leaseset=published`; bounded `REPORT_STATUS`; post-bootstrap
  RouterInfo lookup proofs in both directions; sanitized Java LS2
  lifecycle / tunnel / floodfill / store / ack keys; one terminal
  `P200-{A..H}` classification per run);
- Plan 201 Branch A decode fix (`decode_inbound_i2np` helper +
  `flate2::read::GzDecoder` gunzip; commit `2dc926f`);
- Plan 201 Branch G observation framework (eleven
  `DestinationTunnelCounters` counters + `note_lookup_boundary`
  typed observation surface + eight `plan201_g_*` unit rows + six
  `blocked_row` entries + §11 + §12 invariants; commit `9bce8a7`);
- Plan 201 Branch C/D three-router topology + Router C scaffolding
  (commit `d0fe596`);
- Plan 181 §6.3 stop-condition diagnostic + §6.1 SAM-bridge retention
  (Plan 194);
- Plan 196 Java controlled-first-run topology (ControlledRouter.java
  + run-java.sh + extended static checker);
- Plan 197 PQ SSU2 option parser tolerance
  (`Ssu2PqKem`/`PqCapabilities` typed surface + bounded
  `MAX_SSU2_PQ_SCHEMES = 8`).

Plan 205 inherits the M10 remote-row wiring from commit `06769fa`
but does NOT require any Plan 198 / Plan 201 §4 constraint relaxation
(no public I2P, no Java patching, no i2pr production wire change).

## Required evidence

```text
plan_200 = passed (helper decoupled + terminal classification framework)
plan_201 = in-progress-branch-c-d-attempt-blocked-on-java-loopback-peer-profile-scoring
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_205 = in-progress-registered-blocked-on-plan198-publication-boundary
```

Plan 205 may not close until Plan 201's Java mandatory rows can
all pass; the SAM-bridge helper pivot is the smallest
standards-compatible corrective that satisfies the §4 constraint
envelope.

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
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan205-sam-bridge-pivot
plan_205 = in-progress-registered-blocked-on-plan198-publication-boundary

milestone6_i2pd_streaming_interop           = passed-via-plan193
milestone6_java_mixed_router_interop        = not-yet-passed
milestone6_interoperable                    = not-yet-claimed
m6_java_publication_observability           = landed-via-plan200
m6_java_publication_branch_g_framework      = landed-via-plan201
m6_java_publication_branch_a_corrective     = landed-this-run (one-direction proof + symmetric b-knows-a fix)
m6_java_publication_branch_c_d_corrective   = attempted-this-run-blocked-on-java-loopback-peer-profile-scoring
m10_remote_transport_core                   = passed-via-plan202
m10_remote_application_interop              = passed-via-plan203
m10_remote_rows                              = passed (3/3 via plan202+plan203; was blocked pre-commit 06769fa)
m10_local_rows                              = passed (29/29 via plan181+plan182+plan180)
m10_blocked                                  = 0 (post commit 06769fa)
m10_failed                                   = 0
m10_missing                                  = 0
milestone10_final_acceptance                 = not-yet-closed (Plan 204 still blocked on Plan 201 / Plan 205)

next_executable_plan   = 205 (Java SAM-bridge helper pivot; registered this run)
next_product_layer     = milestone11-planning (deferred until M10 final acceptance)
```

## Handoff rule

Plan 205 must not claim final closure until:

1. Plan 200 has one unambiguous terminal `P200-*` classification
   (re-recorded against the SAM-bridge helpers);
2. only the justified corrective branch was implemented initially;
3. all 23 acceptance criteria in plan-of-record §7 are satisfied;
4. the M6 cross-family checker
   (`scripts/check-m6-mixed-router-acceptance-evidence.sh`) is green
   on the closing exact head;
5. the M6 final closure ledger
   (`scripts/check-m6-final-closure-evidence.sh`) is green on the
   same exact head;
6. the manual M6 mixed-router workflow
   (`.github/workflows/m6-mixed-router-external.yml`) succeeds and
   the produced evidence.json reports
   `m6_mixed_router = passed-via-i2pd-2.61.0-and-java-2.13.0`;
7. Plan 204's §7/§12 authority transitions are recorded in the same
   commit.

## Required validation on the closing head

```text
cargo fmt --all --check                                                   OK
cargo check --locked --workspace --all-targets                            OK
cargo test --locked --workspace --all-targets -- --test-threads=1       passing (Plan 201 + Plan 204 invariants)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps      OK
cargo test --locked --workspace --doc                                   OK

bash scripts/check-m6-mixed-router-acceptance-evidence.sh                 OK
bash scripts/check-m6-final-closure-evidence.sh                          OK (only after exact-head manual workflow run)

bash tests/integration/m6-interop/run-java.sh                            external-session-established-java passed (Plan 196 corrective retained); destination-message-plane against Java blocked (Plan 201 §11 stop provenance)
bash tests/integration/m6-interop/run-m6-mixed-router.sh                 cross-family aggregator produces per-layer exit-code-bound rows
bash tests/integration/service-tunnels/run-independent.sh                local 29 rows passed, remote 3 rows passed (Plan 202 + Plan 203 retained)

cargo deny check advisories bans sources                                 OK
```
