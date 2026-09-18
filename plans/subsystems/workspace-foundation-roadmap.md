# Workspace Foundation Roadmap

Status: closed

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/01-common-identity-crypto.md` (codecs, identity, crypto — M1 foundation)
- `specs/protocols/02-i2np.md` (message envelope shared by transports/NetDB/tunnels)

Related ADRs:

- None required (pre-ADR era).

## 1. Purpose and ownership boundary

Early workspace skeleton, bounded wire codecs, identity/crypto/storage foundations, supervision/cancellation, bounded channels/resource governor, deterministic network testkit, and observability validation.

Historic plans: 000–025 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No transport, NetDB, tunnel, or client behavior; those belong to later subsystems.

## 4. Current state

Milestone 1/2 closures (Plans 010, 020) plus correctives (002, 015, 025).

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
000 -> 001 -> 010..015 -> 020..025 (linear foundation build).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 0 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/000-mvp-roadmap.md` | — |
| 1 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/001-preplan-workspace-skeleton.md` | `plans/closure/workspace-foundation/001-closure.md` |
| 2 | see token | no record | — | `plans/closure/workspace-foundation/002-milestone-0-corrective-closure.md` |
| 10 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/010-milestone-1-overview.md` | `plans/closure/workspace-foundation/010-milestone-1-closure.md` |
| 11 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/011-m1-codec-foundation.md` | `plans/closure/workspace-foundation/011-closure.md` |
| 12 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/012-m1-common-structures.md` | `plans/closure/workspace-foundation/012-closure.md` |
| 13 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/013-m1-identity-crypto-storage.md` | `plans/closure/workspace-foundation/013-closure.md` |
| 14 | see token | no record | — | `plans/closure/workspace-foundation/014-closure.md`; `plans/closure/workspace-foundation/014-m1-i2np-evidence-fuzzing-closure.md` |
| 15 | see token | no record | — | `plans/closure/workspace-foundation/015-m1-corrective-closure.md` |
| 20 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/020-milestone-2-overview.md` | `plans/closure/workspace-foundation/020-milestone-2-closure.md` |
| 21 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/021-m2-supervision-cancellation.md` | `plans/closure/workspace-foundation/021-closure.md` |
| 22 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/022-m2-bounded-channels-resource-governor.md` | `plans/closure/workspace-foundation/022-closure.md` |
| 23 | archived | historical narrative (no status record) | `plans/implementation/workspace-foundation/023-m2-deterministic-network-testkit.md` | `plans/closure/workspace-foundation/023-closure.md` |
| 24 | see token | no record | — | `plans/closure/workspace-foundation/024-m2-observability-validation-closure.md` |
| 25 | see token | no record | — | `plans/closure/workspace-foundation/025-closure.md`; `plans/closure/workspace-foundation/025-m2-targeted-corrective-closure.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor only (`cargo fmt/check/test/clippy/doc`, boundary scripts).

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- Historical era only; no open decision points.

## 11. Completion definition

Closed: M1/M2 closures plus targeted correctives are landed and retained.

## 12. Milestone status summary

Full row history is §7. Current authority: Milestone 1/2 closures (Plans 010, 020) plus correctives (002, 015, 025).
