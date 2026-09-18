# NetDB Roadmap

Status: closed

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/04-reseed-netdb.md` (bootstrap, validation, query/publication)

Related ADRs:

- See `docs/adr/` index for NetDB decisions.

## 1. Purpose and ownership boundary

RouterInfo validation, local NetDB store, persistent cache + SU3 reseed trust path, transport-neutral query/publication state machines, daemon bootstrap.

Historic plans: 102–106 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No public-network NetDB participation claims; no floodfill operation.

## 4. Current state

Plan 106 (daemon NetDB bootstrap integration + Milestone 5 handoff).

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
102 -> 103 -> 104 -> 105 -> 106 (linear).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 102 | archived | historical narrative (no status record) | `plans/implementation/netdb/102-amendment-exploratory-tunnel-dependency.md`; `plans/implementation/netdb/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md` | `plans/closure/netdb/102-amendment-status.md` |
| 103 | archived | historical narrative (no status record) | `plans/implementation/netdb/103-routerinfo-validation-and-local-netdb-foundation.md` | `plans/closure/netdb/103-status.md` |
| 104 | archived | historical narrative (no status record) | `plans/implementation/netdb/104-persistent-netdb-cache-and-su3-reseed-trust-path.md` | `plans/closure/netdb/104-status.md` |
| 105 | archived | historical narrative (no status record) | `plans/implementation/netdb/105-transport-neutral-netdb-query-and-publication-state-machines.md` | `plans/closure/netdb/105-status.md` |
| 106 | archived | historical narrative (no status record) | `plans/implementation/netdb/106-daemon-netdb-bootstrap-integration-and-milestone-5-handoff.md` | `plans/closure/netdb/106-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-netdb-tunnel-evidence.sh` (M6 tunnel layer).

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- None open; later M6 work (Plans 186/190) extended NetDB over tunnels in `mixed-router-interop`.

## 11. Completion definition

Closed: daemon bootstrap integration handed off to exploratory tunnels (Plan 106).

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 106 (daemon NetDB bootstrap integration + Milestone 5 handoff).
