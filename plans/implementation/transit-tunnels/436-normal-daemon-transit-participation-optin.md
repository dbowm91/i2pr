# Plan 436 — Opt-in ordinary-router transit participation and admission

Status: registered / **blocked on Plan 433**. Roadmap: plans/subsystems/core-router-recovery-roadmap.md and plans/subsystems/transit-tunnels-roadmap.md. This succeeds the **one-family experimental** Plan 268 without changing its closure token.

## Objective and readiness

Connect the existing i2pr-tunnel M11 transit participant/data-plane and daemon TransitLiveOwner to normal daemon authenticated-router ingress under explicit operator opt-in. A genuine multihop originating/receiving client product must first pass Plan 433 so transit success does not mask ordinary router failure. Distinguish healthy opt-in transit service from declaring unrestricted public transit readiness.

## Current implementation and classification

Plan 268 proved three receipt-chain completions among eight fixed attempts with no i2pr semantic failures on its qualified head, but `TransitParticipation::Disabled` stays the normal product default and caps/share measurements are zero. i2pr-tunnel already has bounded participant, IBGW and OBEP roles with short-build acceptance and expiry; i2pr-daemon has `transit_owner.rs`, `transit_compose.rs`, `transit_volume.rs`, and router I2NP dispatcher. **Capability:** accept, forward and expire authorized transit tunnels with resource limits. **Infrastructure:** one live transport receive path, admission/lease ownership, bandwidth accounting and generation-aware health. **Invariant:** hop knows only hop-local keys/forward tuple, no origin/circuit path disclosure, no public transit participation from config alone. **Polish:** redacted operator counters; no full hashes per label. Out: floodfill, mandatory public serving, redesign of M11 tunnel crypto, bandwidth-class fabrication.

## Ordered work packages

1. Review M11 Plans 249–268 and ADR 0026 acceptance topology and privacy constraints. Source-census normal `dispatch_router_i2np` ingress, short-build/reply routing, TunnelData, and Plan 340 transit-volume owner. Verify exact Plan 268 implementation is still present on execution SHA.
2. Add an explicit `transit` opt-in to the normalized daemon config and one immutable resource policy (max slots, per-peer/total bandwidth, build requests, buffered bytes, max age, shutdown drain, fairness vs own destinations). Must be default-off and reject invalid resource allocations before socket creation.
3. Integrate the proven `TransitLiveOwner` into **the same** normal transport / I2NP pump, not an independent socket. Enforce validation and early cheap reject before expensive ECIES short-build work; record one admitted-hop tuple per accepted build. Keep the participant's knowledge and any observer minimal.
4. Implement admission decisions from actual health/available bandwidth and immediate rollback on conflicting tunnel IDs, crypto failure, rejected forwarding or expired build. Measure counters from completed byte forwarding, not from synthetic callbacks, and avoid using counts to construct unsupported RouterInfo claims.
5. In isolated exact-pinned i2pd tests, demonstrate real stock-router creator → i2pr participant (gateway, middle, endpoint as applicable) → receiver data delivery in ≥2 independently qualifying runs on one SHA, plus reject, replay, duplicates, expiry, cancellation and restart. Record why attempts cannot qualify instead of retrying indefinitely.
6. Add negative default/no-cap test, strict opt-in and overload backpressure tests. A degraded transit owner must stop new admissions, revoke any capability associated with transit readiness, and drain installed hops deterministically.

## Failure, cancellation, restart, migration

One participant cannot exhaust resources needed for i2pr's local clients; global and per-peer quota enforcement precedes crypto, bytes charged through forwarding completion and released on every terminal outcome. On restart no stale forwarding state survives as active. Config change may require controlled drain; preserve disabled defaults and existing test profile.

## Verification and evidence

Commands: cargo test --locked -p i2pr-tunnel --all-targets; cargo test --locked -p i2pr-daemon --test m11_transit_data_plane -- --test-threads=1; cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1; the prequalified exact pinned Plan 268 i2pd lane (one bounded new receipt qualification contract), cargo fmt/check plus full AGENTS.md floor. Do not invoke public I2P stress tests.

Acceptance requires real transit build and data forwarding with independent endpoints, cleanup to baseline, fair resource accounting under congestion, opt-in policy, no per-hop identity disclosure, and truthful non-advertisement until requirements for public RouterInfo claims are independently met. Stop on a reproducible semantic fault and register a narrow corrective; missing evidence is blocked not success. Closure plans/closure/transit-tunnels/436-status.md lists exact references/hop roles, two runs and Plan 437 readiness.
