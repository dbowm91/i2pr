# M6 Mixed-Router Interop Roadmap

Status: active

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/02-i2np.md` (authenticated router-I2NP dispatch)
- `specs/SOURCES.md` + `specs/IMPLEMENTATIONS.md` (exact-pinned references: i2pd 2.61.0, Java I2P 2.13.0)

Related ADRs:

- `docs/adr/0021-minimal-java-support-topology.md`; see index for others.

## 1. Purpose and ownership boundary

Authenticated I2NP preflight, one-hop exploratory tunnels, NetDB lookup/publication, destination/garlic routing, short-build reply + NetDB reply-path + wire-format correctives, i2pd Streaming qualification (33/33), Java second-family qualification (controlled topology, pq tolerance, public-client observability, Branch G corrective framework, harness/evidence corrective).

Historic/registered plans: 183–194, 196–198, 200, 201, 205, 217–222 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `milestone6_interoperable` claim; Java second-family stays `not-yet-passed`. No public I2P, no reference patching, no production wire change to go green.

## 4. Current state

Plan 217 closed the Java harness corrective. Plan 218 remains the trustworthy behavioral stop: the destination lane passes through the established forward-direction and publication evidence, the Java helper admits a reverse raw-Destination send, and no matching reverse payload reaches i2pr inside the bounded acceptance window.

Plan 219's J219-B RouterInfo-bootstrap attribution is superseded. Plan 220 correctly refuted that hypothesis: Router A stores Router B's exact current `f`-bearing RouterInfo and PeerManager indexes B under `f`.

Post-closure exact-pinned Java 2.13.0 source review found one remaining Plan 220 evidence defect: its selector probe used the raw Destination hash and hard-coded N=3, while production `IterativeSearchJob` derives a daily routing key and selects `_totalSearchLimit + EXTRA_PEERS` through the helper's client-NetDB path. Therefore Plan 220's selector-pass row and its "earliest gap = CLIENT-NETDB" interpretation require selector-equivalence follow-up; the RI/PeerManager/J219-B-refutation facts remain authoritative.

Plan 221 was registered but not executed and is superseded by Plan 222. Plan 222 is dependency-ready. It first reproduces the actual client lookup selector using the helper client DBID, Java-derived routing key and effective search width, then uses the public nonce-correlated `SendMessageStatusListener` path as the primary OCMOSJ discriminator. No topology/bootstrap/protocol corrective is currently authorized. Plan 205 remains retained/deferred.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218 -> 219 -> 220 -> 221 -> 222. Plan 205 is retained conditional fallback and is not part of the primary sequence.
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 183 | ready | registered-m6-mixed-router-streaming-interop-program | `plans/implementation/mixed-router-interop/183-m6-mixed-router-streaming-interop-program.md` | `plans/closure/mixed-router-interop/183-status.md` |
| 184 | closed | passed-m6-authenticated-i2np-runtime-and-reference-preflight | `plans/implementation/mixed-router-interop/184-m6-authenticated-i2np-runtime-and-reference-preflight.md` | `plans/closure/mixed-router-interop/184-status.md` |
| 185 | closed | passed-m6-live-one-hop-exploratory-tunnels-and-liveness | `plans/implementation/mixed-router-interop/185-m6-live-one-hop-exploratory-tunnels-and-liveness.md` | `plans/closure/mixed-router-interop/185-status.md` |
| 186 | closed | passed-m6-mixed-router-netdb-lookup-and-publication | `plans/implementation/mixed-router-interop/186-m6-mixed-router-netdb-lookup-and-publication.md` | `plans/closure/mixed-router-interop/186-status.md` |
| 187 | blocked | blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correct... | `plans/implementation/mixed-router-interop/187-m6-remote-leaseset2-and-destination-garlic-routing.md` | `plans/closure/mixed-router-interop/187-status.md` |
| 188 | blocked | blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-p... | `plans/implementation/mixed-router-interop/188-m6-mixed-router-streaming-with-i2pd.md`; `plans/implementation/mixed-router-interop/188-m6-short-build-reply-interop-corrective.md` | `plans/closure/mixed-router-interop/188-status.md` |
| 189 | blocked | registered-m6-java-second-family-qualification-and-closure (blocked-by-plan188-plan190-plan191-plan192) | `plans/implementation/mixed-router-interop/189-m6-java-i2p-second-family-qualification-and-closure.md` | `plans/closure/mixed-router-interop/189-status.md` |
| 190 | closed | passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed; remote lane no... | `plans/implementation/mixed-router-interop/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md` | `plans/closure/mixed-router-interop/190-status.md` |
| 191 | blocked | stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked; 2 ordering rows ... | `plans/implementation/mixed-router-interop/191-m6-inbound-destination-delivery-boundary.md` | `plans/closure/mixed-router-interop/191-status.md` |
| 192 | closed | passed-m6-i2cp-wire-format-corrective (i2pd-compatible I2CP-style Data body wire-format; inbound-delivery boundary E ... | `plans/implementation/mixed-router-interop/192-m6-i2cp-wire-format-corrective.md` | `plans/closure/mixed-router-interop/192-status.md` |
| 193 | closed | passed-m6-i2pd-mixed-router-streaming-qualification (local rows passed; 33/33 external rows twice on exact head 36871... | `plans/implementation/mixed-router-interop/193-m6-i2pd-mixed-router-streaming-qualification.md` | `plans/closure/mixed-router-interop/193-status.md`; `plans/closure/mixed-router-interop/193-streaming-status.md` |
| 194 | closed | passed-m6-java-second-family-mixed-router-closure-with-sam-ls2-gap (status-file token; re-scoped by Plans 198/200/201... | — | `plans/closure/mixed-router-interop/194-m6-java-second-family-mixed-router-closure.md`; `plans/closure/mixed-router-interop/194-status.md` |
| 196 | closed | passed-m6-java-controlled-first-run-topology-corrective (out-of-tree ControlledRouter.java test-only launcher + rewri... | `plans/implementation/mixed-router-interop/196-m6-java-controlled-first-run-topology-corrective.md` | `plans/closure/mixed-router-interop/196-status.md` |
| 197 | closed | passed-m6-pq-ssu2-option-support-corrective (parser-only tolerance of the SSU2 `pq` KEM-scheme option Java I2P 2.13.0... | `plans/implementation/mixed-router-interop/197-m6-pq-ssu2-option-support-corrective.md` | `plans/closure/mixed-router-interop/197-status.md` |
| 198 | superseded | superseded-execution-decomposed-and-closed-via-plans200-204. | — | `plans/closure/mixed-router-interop/198-m6-java-public-client-final-closure-corrective.md`; `plans/closure/mixed-router-interop/198-status.md` |
| 200 | closed | passed-m6-java-public-client-publication-observability-and-verified-bootstrap (Java helpers decoupled `leaseset=publi... | `plans/implementation/mixed-router-interop/200-m6-java-public-client-publication-observability-and-verified-bootstrap.md` | `plans/closure/mixed-router-interop/200-status.md` |
| 201 | blocked | blocked-pending-plan222-client-netdb-ocmosj-narrowing | — | `plans/closure/mixed-router-interop/201-m6-java-public-client-publication-corrective-and-second-family-closure.md`; `plans/closure/mixed-router-interop/201-status.md` |
| 205 | retained | retained-deferred-conditional-after-plan218-direct-i2cp-requalification (Plan 218 inbound-delivery boundary is on Java's helper-side outbound tunnel endpoint; Plan 205's SAM-bridge pivot addresses the helper's local LeaseSet publication, not the inbound-delivery primitive, and would re-hit the same boundary) | `plans/implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md` | `plans/closure/mixed-router-interop/205-status.md` |
| 217 | closed | passed-m6-java-closure-harness-and-evidence-corrective (transfer-once invariant; positive/negative evidence split; relative Java NetDB dir; disjoint streaming build/tunnel/message-id namespace; `I2PR_M6_JAVA_DRIVER` selector; static-checker invariants) | `plans/implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md` | `plans/closure/mixed-router-interop/217-status.md` |
| 218 | stopped | stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary (corrected destination-only harness on commit `7762e13`; criteria 1–9 pass, Java→i2pr reverse raw Destination delivery fails; prior `java-floodfill-candidate=0` / J219-B root-cause interpretations are non-authoritative pending Plan 220 corrected diagnostics) | `plans/implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md` | `plans/closure/mixed-router-interop/218-status.md` |
| 219 | superseded | retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220 | `plans/implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md` | `plans/closure/mixed-router-interop/219-status.md` |
| 220 | closed | passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required | `plans/implementation/mixed-router-interop/220-m6-java-plan219-diagnostic-attribution-corrective.md` | `plans/closure/mixed-router-interop/220-status.md` |
| 221 | superseded | superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/221-m6-java-client-netdb-ocmosj-narrowing.md` | `plans/closure/mixed-router-interop/221-status.md` |
| 222 | ready | registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/222-m6-java-client-netdb-ocmosj-narrowing-corrective.md` | `plans/closure/mixed-router-interop/222-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-m6-mixed-router-acceptance-evidence.sh` (fail-closed; environment-gated).

Plan 222 extends the corrected diagnostic guards with two exactness requirements:

- the selector preflight must use the helper client DBID, Java's own daily routing key, and the effective `netdb.searchLimit + EXTRA_PEERS` width through the production-equivalent selector overload; and
- the reverse helper send must use the public listener-enabled `I2PSession.sendMessage(..., SendMessageStatusListener)` API so admission/failure/success statuses are correlated by the returned nonce.

The historical P220 selector row cannot satisfy a P222 terminal. The existing 45-second i2pr reverse-payload acceptance window remains frozen; any later listener polling is status-only diagnostic observation.

Environment-gated lanes stay `#[ignore]`-gated and fail closed. Plan 222 allows at most three exact-head external attempts on one committed implementation SHA, with fresh scratch RouterContexts and no tuning between attempts.

## 10. Risks and decision points

- J219-B remains refuted; do not return to a B→A RouterInfo-bootstrap corrective.
- Plan 220's exact RouterHash, stored-RI and PeerManager findings remain authoritative; its selector-pass row is historical pending Plan 222 production-equivalent selector replay.
- Plan 221 MUST NOT be executed; Plan 222 supersedes it before implementation.
- A nonempty selector that omits Router B is not itself a root cause because another floodfill may be selected. An empty exact selector is materially stronger because pinned client-DB `getAllRouters()` returns empty.
- `STATUS_SEND_ACCEPTED` proves admission only. Missing listener status stays Unknown. Failure/success codes must be interpreted no more strongly than exact pinned OCMOSJ source permits.
- The 45-second payload acceptance window cannot be extended by the later status-observation deadline.
- Plan 205 SAM work remains off-path until Plan 222 identifies a boundary that SAM would actually change.

## 11. Completion definition

Closed: Plan 220 corrected the Plan 219 evidence path and refuted J219-B, with selector-equivalence follow-up required.

Superseded before execution: Plan 221.

Open: Plan 222 must reproduce the exact client lookup selector and correlate one reverse helper send through public nonce-bearing status evidence, yielding one `P222-*` terminal or an explicit narrower observability gap. A later plan, not Plan 222, owns any protocol/topology corrective. Plan 204 remains blocked until Java-family closure.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 217 closed the harness corrective; Plan 218 retains the reverse-delivery behavioral stop; Plan 219 attribution is superseded; Plan 220 refuted J219-B and retains exact RI/PeerManager evidence but needs selector-equivalence follow-up; Plan 221 is superseded-before-execution; Plan 222 is dependency-ready for corrected client-NetDB/OCMOSJ narrowing; Plan 201 remains blocked pending Plan 222; Plan 205 remains retained-deferred. No topology/bootstrap/protocol corrective is registered.
