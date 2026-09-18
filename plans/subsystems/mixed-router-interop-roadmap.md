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

Historic/registered plans: 183–194, 196–198, 200, 201, 205, 217–221 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `milestone6_interoperable` claim; Java second-family stays `not-yet-passed`. No public I2P, no reference patching, no production wire change to go green.

## 4. Current state

Plan 217 closed the Java harness corrective. Plan 218 remains the trustworthy Java behavioral stop: the corrected destination lane passes through LS2 lookup/validation, publication and byte-exact i2pr→Java delivery, then Java→i2pr reverse delivery does not arrive.

Plan 219 added useful read-only diagnostics but its `J219-B-A-STORED-B-RI-NOT-F` attribution is superseded and refuted: with the protocol-correct hash at the post-bootstrap epoch, Router A holds Router B's current `f`-bearing RouterInfo.

Plan 220 passed the diagnostic/evidence corrective (`P220-OBSERVABILITY-GAP-CLIENT-NETDB` on exact head `a3d2571`): hash cross-check, exact stored-RI, PeerManager, and live-selector stages are all `Known(pass)`; the helper admits the reverse send; no TunnelData reaches i2pr. The first unobservable stage is the helper client-NetDB/OCMOSJ layer.

Plan 221 is dependency-ready and owns only the client-NetDB/OCMOSJ narrowing through WP-F-permitted evidence. It must produce a narrowed `P221-*` terminal (or narrower gap) before any topology/bootstrap/protocol corrective is registered. Plan 205 stays retained/deferred.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218 -> 219 -> 220 -> 221. Plan 205 is retained conditional fallback and is not part of the primary sequence.
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
| 201 | blocked | blocked-pending-plan221-client-netdb-narrowing | — | `plans/closure/mixed-router-interop/201-m6-java-public-client-publication-corrective-and-second-family-closure.md`; `plans/closure/mixed-router-interop/201-status.md` |
| 205 | retained | retained-deferred-conditional-after-plan218-direct-i2cp-requalification (Plan 218 inbound-delivery boundary is on Java's helper-side outbound tunnel endpoint; Plan 205's SAM-bridge pivot addresses the helper's local LeaseSet publication, not the inbound-delivery primitive, and would re-hit the same boundary) | `plans/implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md` | `plans/closure/mixed-router-interop/205-status.md` |
| 217 | closed | passed-m6-java-closure-harness-and-evidence-corrective (transfer-once invariant; positive/negative evidence split; relative Java NetDB dir; disjoint streaming build/tunnel/message-id namespace; `I2PR_M6_JAVA_DRIVER` selector; static-checker invariants) | `plans/implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md` | `plans/closure/mixed-router-interop/217-status.md` |
| 218 | stopped | stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary (corrected destination-only harness on commit `7762e13`; criteria 1–9 pass, Java→i2pr reverse raw Destination delivery fails; prior `java-floodfill-candidate=0` / J219-B root-cause interpretations are non-authoritative pending Plan 220 corrected diagnostics) | `plans/implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md` | `plans/closure/mixed-router-interop/218-status.md` |
| 219 | superseded | retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220 | `plans/implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md` | `plans/closure/mixed-router-interop/219-status.md` |
| 220 | closed | passed-m6-java-plan219-diagnostic-attribution-corrective | `plans/implementation/mixed-router-interop/220-m6-java-plan219-diagnostic-attribution-corrective.md` | `plans/closure/mixed-router-interop/220-status.md` |
| 221 | ready | registered-ready-m6-java-client-netdb-ocmosj-narrowing | `plans/implementation/mixed-router-interop/221-m6-java-client-netdb-ocmosj-narrowing.md` | — |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-m6-mixed-router-acceptance-evidence.sh`
(fail-closed; environment-gated). The checker now enforces a §14 block
of Plan 220 corrected-diagnostic invariants: no Plan-219-only
production surface, the driver-owned P220 tri-state classifier with
authoritative-epoch collection and hex-hash cross-check, the
controlled-launcher's `P220-*` hex-hash command set plus the
same-package selector probe, the harness's
`JAVA_DIAGNOSTIC_{A,B,C}_PORT` reservation with epoch-labeled
history only, and the driver's 11 `p220_*` unit rows. The Plan 219
`J219-{A..J}` surface and its 16 unit rows were removed in-commit.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

Plan 217 added the `plan217_outbound_role_transfer_once_invariant`
unit row in `destination_tunnel_unit.rs` to lock the destination
driver's transfer-once property locally, and the corrected harness
exposes an `I2PR_M6_JAVA_DRIVER=destination|streaming|both` selector
(`run-java.sh`) so the destination and Streaming sub-runs can be
invoked independently during diagnosis without duplicating the
Java-router topology. The `JAVA_DIAGNOSTIC_{A,B,C}_PORT`
controlled-launcher contract (Plan 219) now carries the
authoritative `P220-*` hex-hash commands (Plan 220); the
destination driver consumes them at its own post-bootstrap epoch
and the shell keeps epoch-labeled history only.

## 10. Risks and decision points

- Plan 220 passed its diagnostic corrective and refuted the old J219-B attribution: Router A holds Router B's current `f` RI at the authoritative epoch. No B→A bootstrap/topology corrective is authorized.
- The corrected terminal is `P220-OBSERVABILITY-GAP-CLIENT-NETDB`. Plan 221 owns the narrowing through WP-F-permitted evidence only.
- Missing evidence must be Unknown/observability-gap, never a negative protocol fact.
- Plan 205 SAM work remains off-path: the Java main-NetDB path fully passes, so the boundary is below the layer a SAM bridge would replace.
- Coarse Java log greps (e.g. `java-floodfill-candidate-empty`) MUST NOT override typed probe observation.

## 11. Completion definition

Closed: Plan 220 repaired the Plan 219 diagnostic and produced the corrected exact-clean-head reverse-delivery attribution (`P220-OBSERVABILITY-GAP-CLIENT-NETDB` on `a3d2571`).
Open: Plan 221 must narrow the client-NetDB/OCMOSJ gap to a `P221-*` terminal (or narrower gap) on an exact-clean-head run. A later plan, not Plan 221, owns whatever protocol corrective that result justifies. Plan 204 remains blocked until Java-family closure.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 217 closed the harness corrective; Plan 218 stopped at the corrected reverse-delivery boundary (narrowed by Plan 220 to the helper client-message layer); Plan 219 instrumentation is retained but its J219-B attribution is superseded and refuted; Plan 220 passed the diagnostic-attribution corrective; Plan 221 is dependency-ready for the client-NetDB/OCMOSJ narrowing; Plan 201 remains blocked pending that narrowing; Plan 205 remains retained-deferred. No topology/bootstrap corrective is currently registered.
