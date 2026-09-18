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

Authenticated I2NP preflight, one-hop exploratory tunnels, NetDB lookup/publication, destination/garlic routing, short-build reply + NetDB reply-path + wire-format correctives, i2pd Streaming qualification (33/33), Java second-family qualification (controlled topology, pq tolerance, public-client observability), Branch G corrective framework.

Historic plans: 183–194, 196–198, 200, 201, 205 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `milestone6_interoperable` claim; Java second-family stays `not-yet-passed`. No public I2P, no reference patching, no production wire change to go green.

## 4. Current state

Plan 201 (in-progress Branch C/D, blocked on Java loopback peer-profile scoring); next executable after 201 is Plan 205.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 205 (see legacy registry `m6_sequence`).
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
| 201 | active | in-progress-branch-c-d-attempt-blocked-on-java-loopback-peer-profile-scoring (this run consumed the first Plan 200 / ... | — | `plans/closure/mixed-router-interop/201-m6-java-public-client-publication-corrective-and-second-family-closure.md`; `plans/closure/mixed-router-interop/201-status.md` |
| 205 | active | in-progress-registered-blocked-on-plan198-publication-boundary (the documented next executable plan after Plan 201; r... | `plans/implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md` | `plans/closure/mixed-router-interop/205-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-m6-mixed-router-acceptance-evidence.sh` (fail-closed; environment-gated).

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- Java-side LeaseSet2 publication gap gates the seven §11 stop rows; three sequential exact-head runs classified `P200-B-router-b-missing-router-a`.

## 11. Completion definition

Open: exact-head external run records the terminal `P200-{A..H}` classification and closes Plan 201; then Plan 205 (SAM-bridge pivot).

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 201 (in-progress Branch C/D, blocked on Java loopback peer-profile scoring); next executable after 201 is Plan 205.
