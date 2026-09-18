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

Historic/registered plans: 183–194, 196–198, 200, 201, 205, 217–218 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `milestone6_interoperable` claim; Java second-family stays `not-yet-passed`. No public I2P, no reference patching, no production wire change to go green.

## 4. Current state

Plan 217 closed the Java closure harness and evidence corrective after Plan 216 exposed contradictions in the prior Java publication diagnosis. Plan 218 ran the corrected destination-only harness once on commit `7762e13` and stopped at the inbound-delivery primitive: criteria 1-9 of Plan 218 §11 pass (authenticated sessions, RouterInfo bootstrap, public helper session, real outbound and inbound installs, LS2 lookup via the owned outbound tunnel, LS2 key/signature/destination validation, local i2pr LS2 publication via the controlled tunnel path, raw i2pr → Java reference payload digest match), but criterion 10 (bidirectional raw Destination payloads match expected digests) fails on the Java → i2pr direction at the helper's outbound tunnel endpoint's peer-selection path; the underlying cause is Java Router A's `java-floodfill-candidate=0` (`reference-facts.tsv`: `java-floodfill-candidate-empty=1`), so the helper cannot resolve the i2pr destination's LeaseSet through Java's netDb and has no real lease to deliver to. Plan 201 stays blocked on Plan 218's fresh external classification (now on the corrected harness); the seven §11 stop rows remain blocked on the inbound-delivery primitive until a fresh plan-of-record opens a path past it. Plan 205 is retained as a conditional fallback only and does not address the inbound-delivery axis that Plan 218 hit.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218. Plan 205 is a retained conditional fallback branch after Plan 218, not part of the primary sequence.
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
| 201 | blocked | blocked-by-plan218-fresh-external-classification-on-corrected-harness (Plan 217 closed the harness/evidence corrective; Plan 218 ran the corrected harness once on commit `7762e13` and recorded a fresh terminal `P200-H … java-floodfill-candidate=false java-network-visible-leaseset=false` classification; the seven §11 stop rows below stay blocked on the inbound-delivery boundary until a fresh plan-of-record opens a path past that primitive) | — | `plans/closure/mixed-router-interop/201-m6-java-public-client-publication-corrective-and-second-family-closure.md`; `plans/closure/mixed-router-interop/201-status.md` |
| 205 | retained | retained-deferred-conditional-after-plan218-direct-i2cp-requalification (Plan 218 inbound-delivery boundary is on Java's helper-side outbound tunnel endpoint; Plan 205's SAM-bridge pivot addresses the helper's local LeaseSet publication, not the inbound-delivery primitive, and would re-hit the same boundary) | `plans/implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md` | `plans/closure/mixed-router-interop/205-status.md` |
| 217 | closed | passed-m6-java-closure-harness-and-evidence-corrective (transfer-once invariant; positive/negative evidence split; relative Java NetDB dir; disjoint streaming build/tunnel/message-id namespace; `I2PR_M6_JAVA_DRIVER` selector; static-checker invariants) | `plans/implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md` | `plans/closure/mixed-router-interop/217-status.md` |
| 218 | stopped | stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary (ran the corrected destination-only harness on commit `7762e13`; recorded terminal `P200-H … java-floodfill-candidate=false java-network-visible-leaseset=false`; inbound delivery primitive bounded by Plan 194 §11 stop `client-ls2-local-but-not-network-visible` on the destination direction and `streaming-b-accept STATUS OK but inbound SYN never reached the backlog` on the streaming direction; root cause = `java-floodfill-candidate=0` on Router A ⇒ helper's outbound tunnel endpoint cannot resolve the i2pr destination's LeaseSet through Java's netDb; Plan 205 stays retained because its SAM-bridge pivot addresses a different axis) | `plans/implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md` | `plans/closure/mixed-router-interop/218-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-m6-mixed-router-acceptance-evidence.sh` (fail-closed; environment-gated).

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

Plan 217 added the `plan217_outbound_role_transfer_once_invariant`
unit row in `destination_tunnel_unit.rs` to lock the destination
driver's transfer-once property locally, and the corrected harness
exposes an `I2PR_M6_JAVA_DRIVER=destination|streaming|both` selector
(`run-java.sh`) so the destination and Streaming sub-runs can be
invoked independently during diagnosis without duplicating the
Java-router topology.

## 10. Risks and decision points

- Plan 217 closed the harness/evidence corrective; the prior Plan 216
  "no LS2 reply arrives" interpretation is no longer authoritative
  because the destination driver reached `LeaseStoreIngestOutcome::Completed`
  before the panic. Plan 218 ran the corrected harness on commit
  `7762e13` and recorded a fresh terminal `P200-H … java-floodfill-candidate=false
  java-network-visible-leaseset=false` classification; the seven §11
  stop rows past criteria 10/22/24/25 stay bounded on the inbound-delivery
  primitive. The corrected Plan 218 status names this as a
  reference-side boundary on Java's helper-side outbound tunnel endpoint
  (Plan 194 §11 stop `client-ls2-local-but-not-network-visible` on the
  destination direction; the streaming driver would consume the same
  primitive via `streaming-b-accept STATUS OK but inbound SYN never
  reached the backlog`). No downstream row can flip past blocked
  until a fresh plan-of-record opens a path past the inbound-delivery
  primitive.
- Plan 205 SAM work is intentionally off the critical path; reactivate
  only if a future plan-of-record (e.g. Plan 219 — M6 Java
  second-family inbound-delivery primitive) records a genuine
  stock-Java boundary that SAM-bridge helper API can cross.

## 11. Completion definition

Open: Plan 218 stopped at the inbound-delivery boundary on commit
`7762e13`. The executable next move is a fresh plan-of-record that
addresses the helper-side outbound tunnel delivery primitive (Plan
219 — M6 Java second-family inbound-delivery primitive is the
documented next move; not registered by Plan 218). On its success,
M6 Java closes and Plan 204 becomes dependency-ready. Plan 205's
SAM-bridge pivot addresses the helper's local LeaseSet publication,
not the inbound-delivery primitive, and would re-hit the same Plan
218 boundary on the corrected harness; do not reactivate Plan 205
without a new boundary on a different axis.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 217 is closed
(harness/evidence corrective); Plan 218 is closed as
`stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`
on commit `7762e13`; Plan 201 stays blocked on Plan 218's fresh
external classification (now on the corrected harness); Plan 205
stays retained-deferred-conditional; Plan 218's seven §11 stop
rows past criteria 10/22/24/25 stay bounded on the inbound-delivery
primitive. The M6 mixed-router Java second-family qualification
remains not-yet-passed; the executable next move is a fresh
plan-of-record.
