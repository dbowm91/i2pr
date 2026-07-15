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

The reproducible Plan 036 lane is documented in
[`tests/integration/ntcp2/README.md`](../tests/integration/ntcp2/README.md),
with exact reference pins in its manifest and a fail-closed repository
preflight in `scripts/check-ntcp2-interoperability.sh`. The lane is manual and
does not run from normal CI. Mixed-router handshake and data evidence is still
not present in this checkout because the complete wire-level adapter and an
authorized testnet run are not available; this remains a closure blocker.
