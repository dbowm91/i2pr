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

Historic/registered plans: 183–194, 196–198, 200, 201, 205, 217–223 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `milestone6_interoperable` claim; Java second-family stays `not-yet-passed`. No public I2P, no reference patching, no production wire change to go green.

## 4. Current state

Plan 217 closed the Java harness corrective. Plan 218 remains the trustworthy behavioral stop: the destination lane passes the established forward/publication evidence, Java admits a reverse raw-Destination send, and no matching reverse payload reaches i2pr inside the bounded window.

Plan 219's J219-B RouterInfo-bootstrap attribution is superseded. Plan 220 refuted it. Plan 222 then reproduced the exact client-NetDB selector and nonce-correlated send path and closed with:

```text
P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
```

Post-Plan-222 exact-pinned Java 2.13.0 source review identified an earlier status-17 branch than the LS2 key-intersection branch: `OutboundClientMessageOneShotJob.runJob()` immediately rejects a target Destination whose `getEncType()` is not `ELGAMAL_2048`. Current i2pr router-owned Destination generation advertises X25519/type 4 in the Destination identity/key certificate itself.

This conflicts with Java's own modern identity model: `I2PClientImpl.createDestination()` keeps the legacy 256-byte Destination public-encryption slot in its ElGamal/type-0 identity shape, while `i2cp.leaseSetEncType=4` controls the independent Standard-LS2 X25519 key. i2pr already separates imported legacy Destination bytes from its static X25519 secret, and `build_signed_lease_set2()` independently emits type-4 X25519.

Plan 223 is therefore dependency-ready. It must first prove the early Java target-Destination guard using the exact external-lane bytes. Only after that proof may it change router-owned Destination generation so the legacy Destination field is type 0 / 256 bytes while LS2 remains type 4 / X25519. If status 17 persists after that correction, Plan 223 records the source `LeaseSetKeys`, target LS2 key types, and exact key intersection, then stops for a successor. No bootstrap/floodfill/SAM/tunnel/topology corrective is authorized.

Plan 205 remains retained/deferred.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218 -> 219 -> 220 -> 221 -> 222 -> 223. Plan 205 is retained conditional fallback and is not part of the primary sequence.
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
| 201 | blocked | blocked-pending-plan223-destination-identity-crypto-separation-corrective | — | `plans/closure/mixed-router-interop/201-m6-java-public-client-publication-corrective-and-second-family-closure.md`; `plans/closure/mixed-router-interop/201-status.md` |
| 205 | retained | retained-deferred-conditional-after-plan218-direct-i2cp-requalification (J219-B refuted; Plan 223 targets the Java target-Destination/LS2 crypto boundary below the SAM API, so SAM remains off-path) | `plans/implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md` | `plans/closure/mixed-router-interop/205-status.md` |
| 217 | closed | passed-m6-java-closure-harness-and-evidence-corrective (transfer-once invariant; positive/negative evidence split; relative Java NetDB dir; disjoint streaming build/tunnel/message-id namespace; `I2PR_M6_JAVA_DRIVER` selector; static-checker invariants) | `plans/implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md` | `plans/closure/mixed-router-interop/217-status.md` |
| 218 | stopped | stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary (criteria 1–9 pass on the corrected lane; Java helper admits the reverse raw-Destination send, but no matching reverse payload reaches i2pr inside the bounded acceptance window; J219-B is refuted, while exact selector/client-message attribution is pending Plan 222) | `plans/implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md` | `plans/closure/mixed-router-interop/218-status.md` |
| 219 | superseded | retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220 | `plans/implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md` | `plans/closure/mixed-router-interop/219-status.md` |
| 220 | closed | passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required | `plans/implementation/mixed-router-interop/220-m6-java-plan219-diagnostic-attribution-corrective.md` | `plans/closure/mixed-router-interop/220-status.md` |
| 221 | superseded | superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/221-m6-java-client-netdb-ocmosj-narrowing.md` | `plans/closure/mixed-router-interop/221-status.md` |
| 222 | closed | passed-m6-java-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/222-m6-java-client-netdb-ocmosj-narrowing-corrective.md` | `plans/closure/mixed-router-interop/222-status.md` |
| 223 | ready | registered-ready-m6-java-destination-identity-crypto-separation-corrective | `plans/implementation/mixed-router-interop/223-m6-java-destination-identity-crypto-separation-corrective.md` | `plans/closure/mixed-router-interop/223-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-m6-mixed-router-acceptance-evidence.sh` (fail-closed; environment-gated).

Plan 223 adds three exactness requirements on top of retained Plan-222 evidence:

- the exact i2pr target Destination bytes must be parsed by both Rust and Java with matching hashes and explicit encryption-type/public-key-length facts before any corrective;
- post-fix generated router-owned Destinations must use the Java-compatible legacy Destination identity shape (type 0 / 256-byte public slot), while the Standard LS2 remains X25519/type 4/32;
- if status 17 persists, source `LeaseSetKeys`, Java-parsed target LS2 keys, and the exact `getEncryptionKey(supported)` intersection must be recorded read-only before any successor is planned.

The Plan-222 45-second reverse-payload acceptance window remains frozen. External attempts remain limited to three on one committed implementation SHA with fresh scratch RouterContexts and no tuning.

## 10. Risks and decision points

- Status 17 is not uniquely an LS2 key-selection failure; pinned OCMOSJ also returns it immediately for a non-ElGamal target Destination key certificate.
- Current i2pr generated Destinations advertise X25519/type 4 in the Destination itself, making the early guard the primary Plan-223 hypothesis.
- Router identity X25519 is not in scope and must not be changed.
- LS2 X25519 is not in scope for downgrade and must remain type 4.
- The legacy Destination public-key slot is public identity-format material; do not derive its filler from private X25519 or signing secrets.
- If correcting generation implies a persistent identity-format migration, stop for a dedicated migration plan rather than silently changing existing identities.
- Plan 205 SAM work remains off-path.

## 11. Completion definition

Closed: Plan 222 narrowed the Java reverse failure to nonce-correlated status 17.

Open: Plan 223 must prove whether the exact i2pr target Destination triggers Java's early non-ElGamal guard, perform only the evidence-supported Destination/LS2 crypto-separation corrective, and rerun the exact destination-only Java lane. If reverse delivery passes, a later final qualification owns Java-family closure. If status 17 or another boundary remains, Plan 223 records it and stops for a narrow successor.

Plan 204 remains blocked until Java-family closure.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 217 closed the harness corrective; Plan 218 retains the reverse-delivery behavioral stop; Plan 219 attribution is superseded; Plan 220 refuted J219-B; Plan 221 is superseded-before-execution; Plan 222 closed the exact client-NetDB/OCMOSJ narrowing with status 17; Plan 223 is the sole dependency-ready corrective and owns the target-Destination identity / LS2 crypto-separation proof-and-fix; Plan 201 remains blocked pending Plan 223; Plan 205 remains retained-deferred. No bootstrap/floodfill/SAM/tunnel/topology corrective is registered.
