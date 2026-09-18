# Destinations, Garlic and Streaming (local) Roadmap

Status: closed

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/06-garlic-ecies-leasesets.md` (end-to-end layer)
- `specs/protocols/07-streaming.md` (reliable byte streams)
- `specs/references/ecies-destination-ratchet.md` (Plan 126 evidence note)
- `specs/references/elligator2-production-representation.md` (Plan 131 evidence)
- `specs/references/streaming-packet-wire.md` (Plan 128 byte-level rules)
- `specs/references/streaming-client-payload-gzip.md` (client payload wire format)

Related ADRs:

- See `docs/adr/` index for ECIES/Streaming decisions.

## 1. Purpose and ownership boundary

LeaseSet2 protocol foundation, destination lifecycle + tunnel pools, ECIES garlic session layer, destination routing + NetDB composition, Streaming core, and the final corrective gates through Plan 134 (plus Plan 152 robustness corrective).

Historic plans: 118–134 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No independent-router destination/Streaming interoperability claim — that is `mixed-router-interop` external debt, explicitly not claimed.

## 4. Current state

Plan 134 (`passed-milestone6-recv-window-ack-ceiling-closure`) — Milestone 6 local closure authority.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
118-123 construction -> 124/125 correctives -> 126-130 final gates -> 131..134 evidence authority (see legacy registry `m6` rows).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 118 | archived | historical narrative (no status record) | `plans/implementation/destination-streaming/118-123-milestone6-router-construction-roadmap.md`; `plans/implementation/destination-streaming/118-planning-authority-cleanup-and-plan117-disposition.md` | `plans/closure/destination-streaming/118-status.md` |
| 119 | archived | historical narrative (no status record) | `plans/implementation/destination-streaming/119-m6-leaseset2-protocol-foundation.md` | `plans/closure/destination-streaming/119-status.md` |
| 120 | archived | historical narrative (no status record) | `plans/implementation/destination-streaming/120-m6-destination-lifecycle-and-tunnel-pools.md` | `plans/closure/destination-streaming/120-status.md` |
| 121 | archived | historical narrative (no status record) | `plans/implementation/destination-streaming/121-m6-ecies-garlic-session-layer.md` | `plans/closure/destination-streaming/121-status.md` |
| 122 | archived | historical narrative (no status record) | `plans/implementation/destination-streaming/122-m6-destination-routing-and-netdb-composition.md` | `plans/closure/destination-streaming/122-status.md` |
| 123 | archived | historical narrative (no status record) | `plans/implementation/destination-streaming/123-m6-minimal-streaming-core.md` | `plans/closure/destination-streaming/123-status.md` |
| 124 | see token | no record | — | `plans/closure/destination-streaming/124-m6-plan122-destination-routing-corrective-closure.md`; `plans/closure/destination-streaming/124-status.md` |
| 125 | see token | no record | — | `plans/closure/destination-streaming/125-m6-streaming-corrective-and-local-closure.md`; `plans/closure/destination-streaming/125-status.md` |
| 126 | closed | passed-ecies-destination-ratchet-corrective-foundation | `plans/implementation/destination-streaming/126-129-milestone6-final-corrective-roadmap.md`; `plans/implementation/destination-streaming/126-130-milestone6-final-corrective-roadmap.md`; `plans/implementation/destination-streaming/126-m6-ecies-destination-ratchet-corrective-foundation.md` | `plans/closure/destination-streaming/126-status.md` |
| 127 | closed | passed-destination-session-routing-final-closure | — | `plans/closure/destination-streaming/127-m6-destination-session-routing-final-closure.md`; `plans/closure/destination-streaming/127-status.md` |
| 128 | see token | no record | — | `plans/closure/destination-streaming/128-m6-streaming-wire-protocol-corrective-closure.md`; `plans/closure/destination-streaming/128-status.md` |
| 129 | superseded | superseded-by-plan130-final-gate. The historical | `plans/implementation/destination-streaming/129-m6-integrated-destination-streaming-final-gate.md` | `plans/closure/destination-streaming/129-status.md` |
| 130 | superseded | superseded-by-plan131-final-local-correctness-gate (local | — | `plans/closure/destination-streaming/130-m6-final-wire-runtime-corrective-closure.md`; `plans/closure/destination-streaming/130-status.md` |
| 131 | superseded | superseded-by-plan132-and-plan133-final-gates (historical | — | `plans/closure/destination-streaming/131-m6-final-local-correctness-closure.md`; `plans/closure/destination-streaming/131-status.md` |
| 132 | superseded | implementation-landed-evidence-superseded-by-plan133 | — | `plans/closure/destination-streaming/132-m6-final-evidence-and-transactional-closure.md`; `plans/closure/destination-streaming/132-status.md` |
| 133 | closed | passed-milestone6-final-evidence-authority-closure. | — | `plans/closure/destination-streaming/133-m6-final-evidence-authority-closure.md`; `plans/closure/destination-streaming/133-status.md` |
| 134 | closed | passed | — | `plans/closure/destination-streaming/134-m6-recv-window-ack-ceiling-closure.md`; `plans/closure/destination-streaming/134-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-destination-tunnel-evidence.sh` and `scripts/check-streaming-tunnel-evidence.sh`.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- Plan 152 (numbered in `sam`) is an M6 corrective owned here by content; see its status record.

## 11. Completion definition

Closed locally via Plan 134 (+152). Mixed-router interop stays in `mixed-router-interop`.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 134 (`passed-milestone6-recv-window-ack-ceiling-closure`) — Milestone 6 local closure authority.
