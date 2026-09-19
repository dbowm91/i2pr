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

Historic/registered plans: 183–194, 196–198, 200, 201, 205, 217–224 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `milestone6_interoperable` claim; Java second-family stays `not-yet-passed`. No public I2P, no reference patching, no production wire change to go green.

## 4. Current state

Plan 217 closed the Java harness corrective. Plan 218 remains the trustworthy behavioral stop: the destination lane passes the established forward/publication evidence, Java admits a reverse raw-Destination send, and no matching reverse payload reaches i2pr inside the bounded window.

Plan 219's J219-B attribution is superseded; Plan 220 refuted it. Plan 222 reproduced the exact client-NetDB selector and nonce-correlated send path and narrowed the reverse failure to status 17. Plan 223 corrected the generated Destination identity / LS2 separation: new router-owned Destinations are legacy type-0/256 identity shape while Standard LS2 remains X25519/type-4/32. Status 17 disappeared and the same tracked send now emits:

```text
P223-NEXT-BOUNDARY ordered_statuses=[1, 21]
```

The open boundary is therefore OCMOSJ `NO_LEASESET (21)` after ACCEPTED.

Exact-pinned Java 2.13.0 research shows status 21 is only the lookup-failure result, not a root cause. The current harness publishes the i2pr target LS2 specifically toward Router B but does not prove that Router B's main NetDB actually holds a current `receivedAsPublished` copy that Java would answer from. If B is answerable, the remaining exact path is helper client lookup A→B, B's LS reply, A's inbound client-tunnel DSM receipt, and installation into the helper client sub-DB. Pinned `InNetMessagePool` stores a matching DSM inline before running the lookup-success job, ruling out a store-vs-success scheduling race.

Plan 224 is dependency-ready and attribution-only. It must prove the earliest failing stage and stop; Plan 225 owns the resulting correction. No bootstrap/floodfill/SAM/tunnel/topology corrective is authorized.

Plan 205 remains retained/deferred.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218 -> 219 -> 220 -> 221 -> 222 -> 223 -> 224. Plan 205 is retained conditional fallback and is not part of the primary sequence.
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
| 201 | blocked | blocked-pending-plan224-no-leaseset-lookup-path-attribution | — | `plans/closure/mixed-router-interop/201-m6-java-public-client-publication-corrective-and-second-family-closure.md`; `plans/closure/mixed-router-interop/201-status.md` |
| 205 | retained | retained-deferred-conditional-after-plan218-direct-i2cp-requalification (J219-B refuted; Plan 224 attributes the direct client-NetDB lookup path below the SAM API, so SAM remains off-path) | `plans/implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md` | `plans/closure/mixed-router-interop/205-status.md` |
| 217 | closed | passed-m6-java-closure-harness-and-evidence-corrective (transfer-once invariant; positive/negative evidence split; relative Java NetDB dir; disjoint streaming build/tunnel/message-id namespace; `I2PR_M6_JAVA_DRIVER` selector; static-checker invariants) | `plans/implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md` | `plans/closure/mixed-router-interop/217-status.md` |
| 218 | stopped | stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary (criteria 1–9 pass on the corrected lane; Java helper admits the reverse raw-Destination send, but no matching reverse payload reaches i2pr inside the bounded acceptance window; J219-B is refuted; Plan 223 moved the exact boundary to ACCEPTED→NO_LEASESET and Plan 224 owns lookup-path attribution) | `plans/implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md` | `plans/closure/mixed-router-interop/218-status.md` |
| 219 | superseded | retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220 | `plans/implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md` | `plans/closure/mixed-router-interop/219-status.md` |
| 220 | closed | passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required | `plans/implementation/mixed-router-interop/220-m6-java-plan219-diagnostic-attribution-corrective.md` | `plans/closure/mixed-router-interop/220-status.md` |
| 221 | superseded | superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/221-m6-java-client-netdb-ocmosj-narrowing.md` | `plans/closure/mixed-router-interop/221-status.md` |
| 222 | closed | passed-m6-java-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/222-m6-java-client-netdb-ocmosj-narrowing-corrective.md` | `plans/closure/mixed-router-interop/222-status.md` |
| 223 | closed | passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset (type-0/256 + LS2 type-4/32; status 17 gone → P223-NEXT-BOUNDARY [1,21] on 0755dc1) | `plans/implementation/mixed-router-interop/223-m6-java-destination-identity-crypto-separation-corrective.md` | `plans/closure/mixed-router-interop/223-status.md` |
| 224 | ready | registered-ready-m6-java-no-leaseset-lookup-path-attribution | `plans/implementation/mixed-router-interop/224-m6-java-no-leaseset-lookup-path-attribution.md` | `plans/closure/mixed-router-interop/224-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-m6-mixed-router-acceptance-evidence.sh` (fail-closed; environment-gated).

Plan 224 retains every Plan-222/223 invariant and adds lookup-path attribution:

- read-only Router-B main-NetDB and Router-A helper-client-subDB target-LS snapshots;
- explicit `receivedAsPublished` / current-state proof before calling Router B query-answerable;
- scratch-only class-specific Java lookup logging with default ERROR and whitelist-only sanitized booleans/counts;
- exact distinction between selector membership, actual A→B query, B lookup receipt, B published-LS answer, A inbound-client-tunnel DSM receipt, and A client-subDB post-send presence;
- the pinned Java DSM store-before-success ordering is treated as source authority and the speculative race explanation is forbidden.

Raw Java logs never become evidence. The Plan-223 45-second reverse-payload acceptance window stays frozen. External attempts remain limited to three per committed implementation SHA with no tuning.

## 10. Risks and decision points

- `NO_LEASESET (21)` is not evidence that publication failed; it is the bounded OCMOSJ lookup-failure result.
- Router B's exact main-NetDB LS state is the first missing discriminator because i2pr explicitly publishes the target LS2 toward B.
- A LeaseSet in B's main DB is query-answerable only when current/valid and `receivedAsPublished` according to pinned Java.
- Selector membership does not prove an actual query was sent to B.
- B answering does not prove the encrypted DSM reached A's helper client tunnel.
- A client-tunnel DSM receipt should route to the helper client sub-DB, and pinned Java stores matching DSMs inline before lookup-success callback; do not resurrect a store-race hypothesis.
- Targeted DEBUG/INFO raw logs may contain sensitive reply key/tag material. They remain scratch-only; evidence is whitelist-sanitized typed facts only.
- Plan 224 cannot implement the discovered fix. Plan 225 owns it.
- Plan 205 SAM work remains off-path.

## 11. Completion definition

Closed: Plan 223 removed the Destination encryption guard and moved the tracked reverse send to ACCEPTED→NO_LEASESET.

Open: Plan 224 must identify the earliest missing stage among Router-B publication/answerability, actual helper client lookup dispatch, B lookup handling/answer, A inbound-client DSM receipt, and helper client-subDB installation. It performs attribution only. A new Plan 225 owns the evidence-supported correction.

Plan 201 and Plan 204 remain blocked until the resulting Java-family work closes.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 217 closed the harness corrective; Plan 218 retains the reverse-delivery behavioral stop; Plan 219 attribution is superseded; Plan 220 refuted J219-B; Plan 221 is superseded-before-execution; Plan 222 narrowed OCMOSJ to status 17; Plan 223 corrected identity/LS2 separation and moved the tracked send to ACCEPTED→NO_LEASESET; Plan 224 is the sole dependency-ready attribution pass; Plan 201 remains blocked pending Plan 224 and its successor; Plan 205 remains retained-deferred. No production corrective for NO_LEASESET is registered.
