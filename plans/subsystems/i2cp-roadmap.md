# I2CP Roadmap

Status: closed

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/10-i2cp-service-tunnels.md` (I2CP wire/profile foundation; service-tunnel material is M10 scope)

Related ADRs:

- See `docs/adr/` index for I2CP decisions.

## 1. Purpose and ownership boundary

I2CP protocol/wire foundation, connection/session/options state machines, client-owned destination + LeaseSet2 bridge, loopback server runtime, message data plane, self-composed local product, invalid-preamble close corrective, independent-client + LeaseSet2-lifecycle closure.

Historic plans: 163–172 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `HostLookup`/`HostReply`, no non-loopback I2CP, no broad historical feature compliance.

## 4. Current state

Plan 172 (`passed-m9-i2cp-independent-leaseset2-lifecycle-corrective`) — Milestone 9 final acceptance (experimental, loopback-only).

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
163 -> 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 171 -> 170 -> 172 (see legacy registry `m9_sequence`).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 163 | ready | registered-m9-i2cp-roadmap | `plans/implementation/i2cp/163-m9-i2cp-roadmap.md` | `plans/closure/i2cp/163-status.md` |
| 164 | closed | passed-m9-i2cp-protocol-and-wire-foundation | `plans/implementation/i2cp/164-m9-i2cp-protocol-and-wire-foundation.md` | `plans/closure/i2cp/164-status.md` |
| 165 | closed | passed-m9-i2cp-connection-session-and-options | `plans/implementation/i2cp/165-m9-i2cp-connection-session-and-options.md` | `plans/closure/i2cp/165-status.md` |
| 166 | closed | passed-m9-i2cp-client-owned-destination-and-leaseset2 | `plans/implementation/i2cp/166-m9-i2cp-client-owned-destination-and-leaseset2.md` | `plans/closure/i2cp/166-status.md` |
| 167 | closed | passed-m9-i2cp-loopback-server-runtime | `plans/implementation/i2cp/167-m9-i2cp-loopback-server-runtime.md` | `plans/closure/i2cp/167-status.md` |
| 168 | closed | passed-m9-i2cp-message-data-plane | `plans/implementation/i2cp/168-m9-i2cp-message-data-plane.md` | `plans/closure/i2cp/168-status.md` |
| 169 | closed | passed-m9-i2cp-self-composed-local-product-and-hardening | `plans/implementation/i2cp/169-m9-i2cp-self-composed-local-product-and-hardening.md` | `plans/closure/i2cp/169-status.md` |
| 170 | see token | [plan_170_external_wire_data_plane] retained-passed | — | `plans/closure/i2cp/170-m9-i2cp-independent-clients-and-final-closure.md`; `plans/closure/i2cp/170-status.md` |
| 171 | closed | passed-m9-i2cp-invalid-preamble-close-and-ci-corrective | `plans/implementation/i2cp/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md` | `plans/closure/i2cp/171-status.md` |
| 172 | closed | passed-m9-i2cp-independent-leaseset2-lifecycle-corrective | `plans/implementation/i2cp/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md` | `plans/closure/i2cp/172-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-i2cp-vectors.sh` and `scripts/check-i2cp-acceptance-evidence.sh`.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- None open; I2CP stays experimental/disabled/loopback-only.

## 11. Completion definition

Closed via Plan 172 (Plan 170 wire/data-plane retained-passed, acceptance superseded by 172).

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 172 (`passed-m9-i2cp-independent-leaseset2-lifecycle-corrective`) — Milestone 9 final acceptance (experimental, loopback-only).
