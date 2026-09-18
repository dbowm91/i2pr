# SSU2 Transport Roadmap

Status: closed

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/09-ssu2.md` (UDP transport dossier)
- `specs/IMPLEMENTATIONS.md` (M8 Java/i2pd entry points)

Related ADRs:

- See `docs/adr/` index for SSU2 decisions.

## 1. Purpose and ownership boundary

SSU2 v2 protocol foundation/addresses, handshake/token/RouterInfo, data-phase reliability/fragmentation, UDP runtime + local session product, path validation/publication/selection, peer-test/relay reachability, independent i2pd interop, CI lane correction.

Historic plans: 154–162 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- Classical X25519 only; Java `pq` option is parser-tolerance (`MAX_SSU2_PQ_SCHEMES = 8`), no ML-KEM. No public-network transport claim.

## 4. Current state

Plan 161 (independent IPv4 interop + final closure) + Plan 162 (external-test lane isolation).

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
154 -> 155 -> 156 -> 157 -> 158 -> 159 -> 160 -> 161 -> 162 (linear).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 154 | ready | registered-m8-ssu2-v2-roadmap | `plans/implementation/ssu2/154-m8-ssu2-transport-and-reachability-roadmap.md` | `plans/closure/ssu2/154-status.md` |
| 155 | closed | passed-m8-ssu2-v2-protocol-foundation-and-addresses | `plans/implementation/ssu2/155-m8-ssu2-v2-protocol-foundation-and-addresses.md` | `plans/closure/ssu2/155-status.md` |
| 156 | closed | passed-m8-ssu2-v2-handshake-token-and-routerinfo | `plans/implementation/ssu2/156-m8-ssu2-v2-handshake-token-and-routerinfo.md` | `plans/closure/ssu2/156-status.md` |
| 157 | closed | passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation | `plans/implementation/ssu2/157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md` | `plans/closure/ssu2/157-status.md` |
| 158 | closed | passed-m8-ssu2-udp-runtime-and-local-session-product | `plans/implementation/ssu2/158-m8-ssu2-udp-runtime-and-local-session-product.md` | `plans/closure/ssu2/158-status.md` |
| 159 | closed | passed-m8-ssu2-path-validation-publication-and-transport-selection | `plans/implementation/ssu2/159-m8-ssu2-path-validation-publication-and-transport-selection.md` | `plans/closure/ssu2/159-status.md` |
| 160 | closed | passed-m8-ssu2-v2-peer-test-and-relay-reachability | `plans/implementation/ssu2/160-m8-ssu2-peer-test-and-relay-reachability.md` | `plans/closure/ssu2/160-status.md` |
| 161 | closed | passed-m8-ssu2-independent-ipv4-interop-and-final-closure | — | `plans/closure/ssu2/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`; `plans/closure/ssu2/161-status.md` |
| 162 | closed | passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration | `plans/implementation/ssu2/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md` | `plans/closure/ssu2/162-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-ssu2-vectors.sh` and `scripts/check-ssu2-acceptance-evidence.sh`.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- None open; reference pins frozen.

## 11. Completion definition

Closed: both session directions against exact-pinned i2pd 2.61.0 (Plan 161); lane hygiene (Plan 162).

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 161 (independent IPv4 interop + final closure) + Plan 162 (external-test lane isolation).
