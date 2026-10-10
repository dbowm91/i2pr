# Plan 431 — Public-capable SSU2 runtime, persistent keys and truthful RouterInfo

Status: registered / **ready**. Plan 430 passed at `fd6b41ead53fdfad86f230d105e0e2977f92830a`; re-census source before production changes and consume generation-bound owner evidence. Roadmap: `plans/subsystems/core-router-recovery-roadmap.md`.

## Objective and readiness

Supply an opt-in **non-loopback-capable** SSU2 runtime and healthy, cryptographically bound RouterInfo address lifecycle without confusing controlled-loopback evidence with public-network qualification. Requires Plan 430's typed readiness/advertisement contract. Existing Plan 161 qualified stock i2pd 2.61.0 IPv4 loopback SSU2 in both directions; Plan 159/160 supply path, peer-test, relay primitives. The normal daemon presently rejects non-loopback binds, advertisement and introducer service in `normalize_ssu2`; the controlled service generates ephemeral SSU2 material.

## Scope and invariant classification

**Capability:** externally routable SSU2 sessions in explicit operator-configured profile; safe advertisement only after independently qualified address/protocol health. **Infrastructure:** persistent per-router SSU2 static and introduction key lifecycle, controlled/public socket policy, runtime transport resources, RouterInfo generation/withdrawal, peer-test reachability feed. **Invariant:** independent router identity vs transport/destination keys; cryptographically bound published `s`, `i`, `v=2`; never advertise loopback/private/unverified externally; no key/log leaks, per-prefix preauth quotas. **Polish:** redacted reachability/status diagnostics. Out: SSU1, PQ, automatic port mapping, unconditional inbound reachability, broad `caps=f` or transit change, new transport codec.

## Ordered work packages

1. Freeze the SSU2 v2 spec, RouterAddress/NetworkID rules and exact pinned i2pd/Java references; recheck any changed normative address/capability requirements. Establish address-family policy for IPv4 and explicitly classify IPv6 as separate until independently proven.
2. Design versioned atomic SSU2 key storage and migration from current ephemeral controlled owner. On restart preserve intended router/SSU2 identities, never silently rotate; detect mismatch or corruption and withdraw stale RouterInfo before establishing a new identity generation.
3. Split the current single strict controlled profile into explicitly validated `controlled_loopback` and `operator_network` profiles. Permit non-loopback UDP *only* in the latter, after user opt-in and policy preflight. Private/local/router address publishing requires deliberate test-profile scope; no wildcard listener advertisement and no config-provided fake reachability result.
4. Bridge socket-bound address, bound persistent key, authenticated session acceptance, peer-test/direct verification and the Plan 430 readiness permit into a single generation-owned local RouterInfo. Publish only actually dialable, spec-valid addresses; withdraw on revocation, bind/key change, path loss, stale evidence or shutdown. Do not claim peer-test (`B`), introducer (`C`), direct reachable or bandwidth capabilities without a qualified runtime owner.
5. Connect the normal-daemon SSU2 manager to a shared peer-delivery/NetDB handle, removing test-only duplicated session owners. Add strict cancellation, replay, malformed datagram, fragmentation, prefix quota, suspended link and restart tests.
6. Execute two-way controlled **non-loopback** authenticated I2NP against pinned i2pd on independent addresses (isolated authorized namespace/hosts). Add a Java-family qualification or retain broad advertisement gated until that evidence exists. No public-network scanning or hostile traffic.

## Failure/cancellation/restart/contention

Fresh unconfirmed sessions consume bounded global + prefix permits before expensive cryptography; no queue-per-source allocation on random datagrams. Disconnects release leases and remove active reachability evidence. Publication generation must be withdrawn before stale socket/key reuse. On controlled/public profile failure, report typed unhealthy state and keep public RouterInfo unadvertised. Normal-router availability is not established by a UDP bind alone.

## Compatibility/migration and tests

Preserve strict default `ssu2.enabled=false` and existing controlled loopback behavior. New config fields use `deny_unknown_fields`; any stored key migration is atomic/versioned and rollback tested. Include exact SSU2 header/key-bound RouterInfo fixtures, malformed input, bind refusal, role/caps refusal, disabled zero-socket check, IPv4 positive and IPv6 negative/conditional tests, restart and spoof-source rejection. Commands:
```sh
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
bash tests/integration/ssu2/run-independent.sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
# run full AGENTS.md floor and new guarded non-loopback lane
```
Exact external runner command must be reconciled with its actual CLI and recorded, not assumed to pass merely by listing it.

## Acceptance, stop and closure

Require real sessions/I2NP in both directions on genuinely non-loopback private addresses, persistent identity across restart, strict RouterInfo binding, public-profile authorization, no false claim on unreachable address, resource baseline restoration, and evidence tier reported exactly (one-family vs two-family). **Normal public capability promotion is blocked until the `specs/CONFORMANCE.md` two-family gate**, regardless of one-family success; controlled operator testing remains permissible in isolation. Stop for missing qualified reference topology, unsafe identity migration or unsound address ownership; document a narrowly bounded successor instead of weakening gates. Closure `plans/closure/core-router-recovery/431-status.md` records SHA, tests, limitations, exact advertisement permissions and Plan 433 readiness.
