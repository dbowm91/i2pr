# Controlled NTCP2 testnet boundary

Plan 035 permits socket tests only on loopback or an explicitly authorized
isolated testnet. This document is a harness contract, not a runnable public
bootstrap configuration.

The harness must pin Java I2P and i2pd versions, create synthetic identities
and independently stored NTCP2 static key/IV records, assign private literal
IPv4/IPv6 endpoints, select inbound/outbound roles, and capture only bounded
typed events. Each run records versions, configuration identifiers, deterministic
seed/scenario names, timeout policy, and teardown counters. It must not retain
private keys, RouterInfo payloads, I2NP bytes, raw endpoint diagnostics, or
arbitrary remote error text.

The harness must fail closed when a target is not loopback or inside the
explicit isolated namespace. It must not use reseeding, public bootstrap,
automatic address discovery, NAT mapping, RouterInfo publication, or NetDB
mutation. Every process and task is terminated and drained before the run is
reported complete.

The reproducible Plan 036/037 lane is documented in
[`tests/integration/ntcp2/README.md`](../tests/integration/ntcp2/README.md),
with exact reference pins in its manifest and a fail-closed repository
preflight in `scripts/check-ntcp2-interoperability.sh`. The lane is manual and
does not run from normal CI. Plan 037 adds local ownership and parser
corrections, but mixed-router handshake and data evidence is still not present
in this checkout because the complete wire-level adapter and an authorized
testnet run are not available; this remains a closure blocker.

## Plan 038 Ubuntu harness boundary

Plan 038 defines the first harness foundation for Ubuntu amd64. It has two
security domains. Preparation may use `apt` and network access only to install
declared tools and fetch the locked Java I2P 2.12.0 and i2pd 2.60.0 revisions;
it records source, tool, build-command, and artifact hashes. Execution must
run from those prepared inputs without downloads, DNS, reseed, bootstrap,
RouterInfo publication, NetDB mutation, or public endpoints.

The commands are:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
bash scripts/interop/run-scenario.sh --scenario <id> --reference java-i2p --build-cache <path> --run-root <path>
bash scripts/interop/run-scenario.sh --scenario <id> --reference i2pd --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh --profile environment-smoke
```

Before a router starts, the harness creates one namespace per participant and
connects them only with a veth pair. Both veth endpoints leave the host
namespace. Each participant gets loopback, its scenario interface, and only
the expected directly connected IPv4/IPv6 routes. Missing default-route and
public-egress checks are fatal; namespace-scoped nftables rules are only
defense in depth. Cleanup must terminate and drain processes, delete the
namespaces and veth interfaces, and remove secret-bearing run state. Cleanup
failure is a scenario failure.

Environment smoke means only reference startup/readiness and clean teardown.
Reference crosscheck means Java I2P and i2pd exercise one another through the
isolated topology; it is not i2pr evidence. i2pr mixed-router evidence starts
only with bounded authenticated runs between i2pr and each reference in both
directions. Records may retain typed outcomes, bounded metadata, and hashes of
sanitized artifacts/configuration. Raw addresses, identities, RouterInfo,
I2NP, keys, transcripts, logs, and arbitrary remote error text must be deleted.
