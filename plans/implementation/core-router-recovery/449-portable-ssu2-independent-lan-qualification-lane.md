# Plan 449 — Portable, authorized independently addressed SSU2 qualification lane

Status: **registered / ready for preflight/harness implementation; external proof environment-dependent**. Date: 2026-10-10. Core-router recovery owner; qualification successor to stopped Plan 431 and local-component-passed Plan 442 (`plans/closure/core-router-recovery/{431,442}-status.md`). Roadmaps core-router recovery and SSU2 transport.

## Objective and rationale

Plan 442 implemented durable, router-hash-bound SSU2 static/intro keys and signed controlled RouterInfo with local preflight, but did not have an independently addressed authorized pair for the non-loopback evidence required by Plan 431. That **must not** gate separate local implementation plans 447/448. Deliver a portable zero-root **operator-run** evidence recipe and minimal controlled runner so the external gate becomes an actionable availability question rather than perpetual planning churn. When a pair is available, run a real independent i2pr↔stock-i2pd qualification; no public reachability claimed from private-LAN proof alone.

## Classification, boundaries, invariants

**Infrastructure:** host readiness probe, signed RouterInfo and port/interface preflight, independent peer identity/endpoint runner and redacted evidence manifest. **Capability:** independently addressed proof of stock peer bidirectional SSU2 direct sessions plus configured address/identity persistence and health/withdrawal behavior, not default public I2P network participation. **Invariant:** no root/sudo, namespaces, containers, VMs, systemd or public-network runs in routine acceptance; no remote administrative access without explicit owner authorization; no false `R`/`f`/introducer; no key/payload logging; exact pinned stock reference, no patch. **Polish:** two-host operator checklist and fail-reason inventory. Out: automatic discovery of an unrelated host, NAT-PMP/UPnP implementation, IPv6 or multi-reference-family promotion, unaudited Internet contact.

## Bounded implementation

1. **Environment inventory:** Define precise qualified-host inputs without embedding personal/public addresses: two owner-approved physical/virtual *existing* hosts with distinct verified local-interface IPv4 addresses on an isolated private subnet, mutual UDP reachability and ephemeral test-only ports; pin i2pd 2.61.0 revision `635b013a612ff47278ef02acf8580a28e10e26c5`; specify OS support and commands for an **unprivileged user**. Do not auto-provision root namespaces, containers or VMs. Validate clocks, firewall, routability, port availability, kernel socket options and test data permissions.
2. **Config/source qualification:** Review Plan 442's normal owner, `config.rs::parse_ssu2_bind`/`normalize_ssu2`, `lib.rs::register_ssu2_service` and `i2pr-runtime::Ssu2RuntimeService`. **Do not broaden non-loopback bind or advertisement before Plan 430's fail-closed consent/eligibility design is implemented**. If Plan 442's normal-daemon loopback-only config cannot bind the LAN target, first implement a strictly scoped explicit `authorized-lan-qualification` profile or operator-approved normal transport bind in the Plan 431 source owner, with private-test RouterInfo confined to lab data directories. It must not set public-advertised `R` merely for a private bind.
3. **Signed identities and canonical RouterInfos:** Persist i2pr router and SSU2 identity, create signed RI with actual key/address/port, match network ID, distinguish source and destination identities, reject stale keys and duplicate source hash. Use a private signed-known-peer bootstrap with source provenance, not a fake session, and retain only digests and enum evidence. Stock reference's reserved/private address acceptance must be configured explicitly for the **lab** and verified with a stock-to-stock sanity test only where candidate filtering is part of the acceptance question.
4. **Executable preflight (no packet attempt by default):** Script validates both host inventories and consent, subnet/route, known-pinned reference, transport-port/no-conflict, datadir separation, eligible RouterInfo, finite execution budget and cleanup trap. Missing host, missing authorization, route, incompatible clock or unavailable port produces `environment-unavailable`, not misleading `pass`. Ordinary CI runs deterministic fake host fixture/tests and no remote UDP.
5. **Bounded live LAN qualification:** With explicit operator invocation, run two-way direct UDP SSU2 against pristine i2pd, tokenless/Retry/cached-token, valid session establishment, actual small and fragmented I2NP/DeliveryStatus both directions, teardown and clean restart. Show source/dest were independently addressed (not 127.0.0.1 masquerading as LAN) and no non-I2P/non-lab egress. Verify signed RouterInfo update/withdrawal upon port change, restart or lost reachability; classify firewalled/introducer/IPv6 separately unless their own qualified evidence exists.
6. **Negative/abuse controls:** Invalid interface, changed public key/intro key, wrong network ID, expired RouterInfo, bad token, replay, UDP source spoof (synthetically in deterministic unit tests), resource exhaustion and unauthorized non-loopback startup. No active abuse/fuzz/load attack on shared production LAN. Default-disabled profiles still bind zero transport sockets.
7. **No false-qualification result:** A LAN pass is at most **one independent implementation family** and controlled non-loopback evidence. `specs/CONFORMANCE.md` two-family/public claim gates remain; Plan 431 can be superseded/unblocked only after its own exact evidence/eligibility contract is met. Product Plan 433 and final Plan 439 do not become automatically ready from a host inventory or component-test-only pass.

## Failure, cancellation, restart, migration and privacy

Owned child tasks use finite deadlines, SIGTERM and bounded SIGKILL on *owned* processes only; preserve persistent identity, revalidate cache and drop all volatile session state on restart. External host inventory is user-supplied transient configuration and **never committed to the repo**. Logs redact address identities and packet material; retain byte/phase/counter-level evidence with signed source hashes and manifest checksums only. This plan creates no new default network exposure or on-disk migration except the already-qualified Plan 442 identity format.

## Verification and acceptance

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
python3 scripts/check-core-router-recovery-contract.py --self-test
python3 scripts/check-global-plan-number-uniqueness.py
# exact two-host preflight and explicit opt-in qualification runner introduced
# by the implementation pass, then full AGENTS.md floor
```

**Infrastructure closure is conditional:** a portable manifest, safe fail-closed preflight, bounded tests and operator guide can be marked complete **only at that infrastructure scope**; absent hosts must be reported as `external-environment-blocked`. **Full Plan 449 qualification** requires real independent non-loopback stock-i2pd sessions and delivery evidence, identity/restart/withdrawal checks, and negative controls. Do not mark both as one pass. Closure `plans/closure/core-router-recovery/449-status.md` must provide two independent pass/fail columns (infrastructure and real LAN), exact refs/commands, security and authorization, CI outcome and Plan 431/433 unblock audit. No public support promotion merely from successful local or one-family LAN testing.
