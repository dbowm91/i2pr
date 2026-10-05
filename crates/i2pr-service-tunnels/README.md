# i2pr-service-tunnels

Runtime-neutral policy and bounded protocol filters for I2P service tunnels. The crate
provides validated service specifications, explicit Destination/linkability groups,
access and resource policy, and HTTP, SOCKS, IRC, and CONNECT parsing/filtering.

The crate owns no sockets, async runtime, timers, DNS, filesystem, SAM sessions, or
router state. Applications supply transport-authenticated peer identity when a policy
requires it and own all connections, clocks, persistence, and lifecycle. See the
[portable core contract](../../specs/references/portable-service-tunnel-core-v1.md)
and ADR 0032 (`docs/adr/0032-portable-service-tunnel-policy-core-and-adapters.md` in
the i2pr repository).

This crate is experimental and is not a standalone tunnel product. The repository has
not selected a license; package publication remains disabled. No SAM protocol
implementation is included.
