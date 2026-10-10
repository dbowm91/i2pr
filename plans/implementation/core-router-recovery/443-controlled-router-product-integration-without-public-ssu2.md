# Plan 443 — Controlled real-router integration independent of public SSU2 qualification

Status: **blocked — controlled daemon-owned component path passed; normal-process and stock-to-stock topology gate remains open**. Plans 440 and 432 are closed. Targeted dependency corrective to Plan 433; original Plan 433 external/public product acceptance remains blocked by 431. Closure: `plans/closure/core-router-recovery/443-status.md`.

## Objective and why

Compose the **normal product process** under a controlled loopback/private-lab profile and test real NetDB/tunnel/application flows with already-proven SSU2 session behavior, without requiring public/non-loopback capability publication. A regression in multi-router routing should be investigated even while Plan 431 lacks a two-host qualification lane. Emissary's Router→TransportManager→NetDb/TunnelManager assembly is a model to inspect, not code to copy.

## Scope/classification

**Capability:** controlled multi-router application exchange through i2pr's actual daemon, not a test-only router stack; no normal public network claim. **Infrastructure:** one SSU2 socket owner, validated RI cache and live NetDB transport bridge, real exploratory/destination tunnel pools, SAM/I2CP adapter, typed readiness projection. **Invariant:** no fake NetDB lookup completion, no static manually injected links in acceptance, no hidden zero-hop fallback, no unauthorized public RI address/R/f, no destination/router identity correlation in output. **Polish:** controlled startup and precise degraded explanation. Out: public SSU2 activation, NTCP2, floodfill public caps, full 2-family transport conformance.

## Ordered work

1. Read Plan 432 and Plan 161 closures; freeze the controlled profile (separate signing identities, network ID and address policy, stock-peer acceptance of local addresses, each process real listener and isolated data dir). Require a known-working stock-to-stock control **before** attributing candidate/tunnel issues to i2pr. An isolated network ID must match peers; stock i2pd network-ID-2 test routers on loopback need explicit local acceptance. Reject mixed ID99/ID2 fixtures.
2. Review current daemon composition `bootstrap_daemon`, existing `register_ssu2_service`, NetDB dispatcher, tunnel pools, destination streaming, SAM/I2CP and their cancellation owners. Wire already implemented pieces through a single production daemon owner; avoid duplicating independent i2pr test transports.
3. Enable controlled-local registered socket + real signed network-ID-consistent RouterInfo **only in explicit controlled profile**; use Plan 430 private/test qualification gates. Never advertise local addresses into public NetDB. Derive test-only local address allowance in the reference fixture, not a general parsing bypass.
4. Establish authenticated peer connections and actual DatabaseLookup/DatabaseStore, then genuine >0-hop exploratory and destination tunnel installation through separate stock routers. Seeding can provide initial signed RouterInfos, but live acceptance **must** include real network replies, no simulated post-seed traffic and stable peer selection; validate peer failure/backoff.
5. Run SAM3.1 or I2CP client to remote service across >0-hop outbound and inbound paths, exact-byte request/response and clean shutdown/restart. Freeze an explicit ≤N-attempt budget and classify router admission, tunnel selection, client readiness and I2NP independently. No reliance on Java SAM readiness when Java cannot select its only floodfill (add known working peers).
6. Preserve captured accepted-message events as enum counts and hop roles rather than raw destination identifiers. Distinguish `controlled-router-product-passed` from `normal-public-router-qualified`. The latter is still Plan 433 after Plan 431.

## Failure, restart, contention and compatibility

Every process/listener supervised, bounded buffer and dial/tunnel build budgets, retry backoff, cleanup-to-baseline; no extra daemon listener in disabled mode. Reuse Plan 432 cache; prevent stale RouterInfo/tunnel from masquerading as recovered after restart. For unproven outside-lab addresses, do not publish and do not set an accessible address capability. No automatic config migration. Always report missing local-address acceptance as `fixture-ineligible`, not protocol failure.

## Verification and handoff

`cargo test --locked -p i2pr-daemon -- --test-threads=1`; `cargo test --locked -p i2pr-tunnel --all-targets`; `cargo test --locked -p i2pr-client --all-targets`; source-verified Plan 161 and Plan 193 external runners in explicit authorized controlled mode; actual new normal-daemon controlled-product runner; full `AGENTS.md` floor and exact-head CI. Preflight stock-to-stock control and known-working floodfill independent of target i2pr. In closure include exact RIs, pins, actual data path **without embedding identity bytes in logs**.

Pass only on real controlled multihop application receipt/reply on the normal i2pr process, with appropriate mixed-router authenticated links and no public address claims. If a topology cannot build stock-to-stock, halt/repair fixture rather than continuously adding stage observers. A controlled positive does **not** close Plan 433; it unblocks meaningful live integration and isolates what Plan 433 still requires. Closure `plans/closure/core-router-recovery/443-status.md`, update 433/439 dispositions conservatively.
