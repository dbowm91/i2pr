# Exploratory Tunnels Roadmap

Status: closed

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/05-tunnels.md` (tunnel construction + messages)
- `specs/references/short-build-inbound-creator-key.md` (Plan 113 inbound policy)

Related ADRs:

- See `docs/adr/` index for tunnel-build decisions.

## 1. Purpose and ownership boundary

Short tunnel-build construction (ECIES-X25519), multirecord preprocessing, inbound/outbound pre-delivery closure, external delivery to live NetDB, local data plane.

Historic plans: 107–117 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No destination/garlic/Streaming behavior (Milestone 6); no multi-hop claims beyond the qualified scope.

## 4. Current state

Plan 117 corrective closure (live exploratory + NetDB integration).

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
107 -> 108 -> 109 -> 110 -> 111 -> 112 -> 113 -> 114 -> 115 -> 116 -> 117 (linear with corrective branches).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 107 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/107-milestone-5-exploratory-tunnel-substrate.md` | `plans/closure/exploratory-tunnels/107-status.md` |
| 108 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/108-conformance-amendment.md`; `plans/implementation/exploratory-tunnels/108-live-ecies-x25519-short-tunnel-build-construction.md` | `plans/closure/exploratory-tunnels/108-status.md` |
| 109 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`; `plans/implementation/exploratory-tunnels/109-short-build-record-and-noise-conformance-correction.md` | `plans/closure/exploratory-tunnels/109-status.md` |
| 110 | see token | no record | — | `plans/closure/exploratory-tunnels/110-short-build-multirecord-preprocessing-and-conformance-closure.md`; `plans/closure/exploratory-tunnels/110-status.md` |
| 111 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/111-handoff.md`; `plans/implementation/exploratory-tunnels/111-short-build-final-local-conformance-correction.md` | `plans/closure/exploratory-tunnels/111-post-closure-audit-amendment.md`; `plans/closure/exploratory-tunnels/111-status.md` |
| 112 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/112-113-post-plan111-pre-delivery-corrective-roadmap.md`; `plans/implementation/exploratory-tunnels/112-handoff.md` | `plans/closure/exploratory-tunnels/112-outbound-short-build-pre-delivery-closure.md`; `plans/closure/exploratory-tunnels/112-status.md` |
| 113 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/113-inbound-short-build-spec-reference-reconciliation.md` | `plans/closure/exploratory-tunnels/113-status.md` |
| 114 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/114-handoff.md`; `plans/implementation/exploratory-tunnels/114-short-build-terminal-routing-chain-correction.md` | `plans/closure/exploratory-tunnels/114-status.md` |
| 115 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/115-117-external-delivery-to-live-netdb-roadmap.md`; `plans/implementation/exploratory-tunnels/115-117-roadmap-amendment-emissary-q0.md`; `plans/implementation/exploratory-tunnels/115-handoff.md`; `plans/implementation/exploratory-tunnels/115-qualified-independent-short-build-consumption-and-external-delivery.md` | `plans/closure/exploratory-tunnels/115-completion-emissary-native-q0.md`; `plans/closure/exploratory-tunnels/115-status-amendment-emissary-q0.md`; `plans/closure/exploratory-tunnels/115-status.md` |
| 116 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/116-handoff.md`; `plans/implementation/exploratory-tunnels/116-local-tunnel-data-plane.md` | `plans/closure/exploratory-tunnels/116-completion-correction.md`; `plans/closure/exploratory-tunnels/116-final-closure.md`; `plans/closure/exploratory-tunnels/116-status.md`; `plans/closure/exploratory-tunnels/116-terminal-cleanup.md` |
| 117 | archived | historical narrative (no status record) | `plans/implementation/exploratory-tunnels/117-handoff.md`; `plans/implementation/exploratory-tunnels/117-live-exploratory-netdb-integration.md` | `plans/closure/exploratory-tunnels/117-corrective-closure.md`; `plans/closure/exploratory-tunnels/117-status.md`; `plans/closure/exploratory-tunnels/117-terminal-native-reference-correction.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-exploratory-tunnel-evidence.sh`.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- None open; spec cross-refs live in `specs/protocols/05-tunnels.md`.

## 11. Completion definition

Closed: Plan 117 live integration + corrective closure; handed off to destination-streaming (Plan 118).

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 117 corrective closure (live exploratory + NetDB integration).
