# Service Tunnels Roadmap

Status: active

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/10-i2cp-service-tunnels.md` (service-tunnel material is M10 scope)
- `specs/protocols/11-service-tunnels.md` (M10 service-tunnel dossier, plan-linked)

Related ADRs:

- See `docs/adr/` index for service-tunnel decisions.

## 1. Purpose and ownership boundary

Generic client/server service tunnels, HTTP/SOCKS5/IRC profiles, composition reconcile + hardening, local delivery corrective, production remote delivery composition (Plans 202/206/208), router-backed destination material + canonical inbound Streaming (210/212), generic external qualification (213), product HTTP/IRC closure (214/215).

Historic plans: 173–182, 195, 199, 202–204, 206–215 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No capability advertisement beyond `specs/CONFORMANCE.md`; service tunnels stay disabled by default, loopback-only listeners.

## 4. Current state

Plan 215 (`passed-m10-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification`) is the closed M10 product authority. Plan 204 is convergence-only and remains blocked on independent M6 Java second-family closure. Plan 219 is the registered investigation of the Plan 218 reverse-delivery boundary; it does not itself unblock Plan 204.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
173 -> 174 -> 175 -> 176 -> 177 -> 178 -> 179 -> 180 -> 182 -> 181 -> 195 -> 202 -> 203 -> 206 -> 208 -> 210 -> 211 -> 212 -> 213 -> 214 -> 204 (see legacy registry `m10_sequence`; 199/207/209 retained-superseded scaffolds).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 173 | ready | registered-m10-service-tunnels-roadmap | `plans/implementation/service-tunnels/173-m10-service-tunnels-http-socks5-irc-roadmap.md` | `plans/closure/service-tunnels/173-status.md` |
| 174 | closed | passed-m10-service-tunnel-foundation-and-shared-stream-runtime | `plans/implementation/service-tunnels/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md` | `plans/closure/service-tunnels/174-status.md` |
| 175 | closed | passed-m10-generic-client-server-service-tunnels | `plans/implementation/service-tunnels/175-m10-generic-client-server-service-tunnels.md` | `plans/closure/service-tunnels/175-status.md` |
| 176 | closed | passed-m10-http-i2p-proxy-and-connect | `plans/implementation/service-tunnels/176-m10-http-i2p-proxy-and-connect.md` | `plans/closure/service-tunnels/176-status.md` |
| 177 | closed | passed-m10-socks5-i2p-connect-proxy | `plans/implementation/service-tunnels/177-m10-socks5-i2p-connect-proxy.md` | `plans/closure/service-tunnels/177-status.md` |
| 178 | closed | passed-m10-irc-client-profile-and-privacy-filtering | `plans/implementation/service-tunnels/178-m10-irc-client-profile-and-privacy-filtering.md` | `plans/closure/service-tunnels/178-status.md` |
| 179 | closed | passed-m10-irc-server-profile-and-authenticated-peer-hostname | `plans/implementation/service-tunnels/179-m10-irc-server-profile-and-authenticated-peer-hostname.md` | `plans/closure/service-tunnels/179-status.md` |
| 180 | closed | passed-m10-service-tunnel-composition-reconcile-and-hardening | `plans/implementation/service-tunnels/180-m10-service-tunnel-composition-reconcile-and-hardening.md` | `plans/closure/service-tunnels/180-status.md` |
| 181 | closed | passed-local-matrix-remote-rows-blocked-pending-plan213-and-plan214 (status-file token; the retained local 29-row mat... | — | `plans/closure/service-tunnels/181-m10-independent-application-and-service-interop-final-closure.md`; `plans/closure/service-tunnels/181-status.md` |
| 182 | closed | passed-m10-local-delivery-corrective | `plans/implementation/service-tunnels/182-m10-local-delivery-corrective.md` | `plans/closure/service-tunnels/182-status.md` |
| 195 | blocked | blocked-m10-remote-independent-service-pending-plan213-and-plan214. | — | `plans/closure/service-tunnels/195-m10-remote-independent-service-final-closure.md`; `plans/closure/service-tunnels/195-status.md` |
| 199 | superseded | superseded-execution-decomposed-and-closed-via-plans200-204. | — | `plans/closure/service-tunnels/199-m10-unified-final-closure.md`; `plans/closure/service-tunnels/199-status.md` |
| 202 | superseded | partial-m10-remote-routing-capability-surface-superseded-by-plan206 (Plan 206 §5 promoted the marker/counter capabili... | `plans/implementation/service-tunnels/202-m10-production-remote-destination-and-streaming-composition.md` | `plans/closure/service-tunnels/202-status.md` |
| 203 | superseded | retained-partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207 (the `m10_positive_remote_http_and... | `plans/implementation/service-tunnels/203-m10-positive-remote-http-and-irc-application-interop.md` | `plans/closure/service-tunnels/203-status.md` |
| 204 | blocked | blocked-on-m6-java-second-family-plan218-for-final-convergence | — | `plans/closure/service-tunnels/204-m10-final-closure-evidence-authority-and-documentation-normalization.md`; `plans/closure/service-tunnels/204-status.md` |
| 206 | superseded | retained-partial-executable-backend-seams-superseded-by-plan208 (the executable `ServiceDestinationDelivery` backend ... | `plans/implementation/service-tunnels/206-m10-production-remote-delivery-composition-corrective.md` | `plans/closure/service-tunnels/206-status.md` |
| 207 | superseded | passed-m10-genuine-remote-http-and-irc-application-interop-superseded-by-plan209 (introduced real system `curl` + exa... | `plans/implementation/service-tunnels/207-m10-genuine-remote-http-and-irc-application-interop-corrective.md` | `plans/closure/service-tunnels/207-status.md` |
| 208 | closed | passed-m10-production-delivery-driver-remote-route-integration (`crates/i2pr-daemon/src/service_tunnels.rs::deliver_o... | `plans/implementation/service-tunnels/208-m10-production-delivery-driver-remote-route-integration-corrective.md` | `plans/closure/service-tunnels/208-status.md` |
| 209 | superseded | retained-partial-black-box-composition-harness-superseded-by-plan211 (the new `crates/i2pr-daemon/src/service_product... | `plans/implementation/service-tunnels/209-m10-product-only-remote-http-and-irc-application-acceptance-corrective.md` | `plans/closure/service-tunnels/209-status.md` |
| 210 | superseded | retained-partial-structural-corrective-superseded-by-plan212 (retained owner table, placeholder removal, explicit des... | `plans/implementation/service-tunnels/210-m10-real-service-destination-tunnel-material-and-inbound-streaming-corrective.md` | `plans/closure/service-tunnels/210-status.md` |
| 211 | retained | retained-source-harness-superseded-for-final-evidence-by-plan214 (source harness retained; final counted evidence own... | `plans/implementation/service-tunnels/211-m10-product-only-remote-http-irc-final-acceptance-corrective.md` | `plans/closure/service-tunnels/211-status.md` |
| 212 | closed | passed-source-and-generic-external-qualification-via-plan213 (router-backed state + per-service provisioning + canoni... | `plans/implementation/service-tunnels/212-m10-router-backed-service-destination-material-and-canonical-inbound-streaming-corrective.md` | `plans/closure/service-tunnels/212-status.md` |
| 213 | closed | passed-m10-router-backed-generic-external-qualification (generic A/B application driver with real TCP I/O + per-direc... | `plans/implementation/service-tunnels/213-m10-router-backed-generic-external-qualification-and-evidence-corrective.md` | `plans/closure/service-tunnels/213-status.md` |
| 214 | closed | passed-m10-product-only-remote-http-and-irc-application-closure (see plans/214-status.md) | — | `plans/closure/service-tunnels/214-m10-http-irc-product-only-external-requalification-evidence-hardening-and-final-closure.md`; `plans/closure/service-tunnels/214-status.md` |
| 215 | closed | passed-m10-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification (see plans/215-status.md) | `plans/implementation/service-tunnels/215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md` | `plans/closure/service-tunnels/215-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-service-tunnel-acceptance-evidence.sh` and `scripts/check-service-tunnel-boundaries.sh`.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- Plan 204 convergence is deferred until independent M6 Java second-family closure; current M6 work is Plan 219 root-cause investigation after stopped Plan 218. This does not reopen or downgrade M10 product closure.
- Remote branch `origin/plan-m10-closure` (Plan 199 executable-registration era) is superseded
  by the 200–204 decomposition and the 210–215 closures — do not merge (see
  `plans/registry.md` "Superseded remote branches").

## 11. Completion definition

Open: Plan 204 docs/CI normalization convergence over the independently closed M10 authority plus the M6 Java second-family row. Plan 219 is investigative; convergence remains blocked until a later Java-family closure.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 215 (`passed-m10-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification`); Plan 204 convergence remains blocked on M6 Java second-family closure while Plan 219 investigates the current boundary.
