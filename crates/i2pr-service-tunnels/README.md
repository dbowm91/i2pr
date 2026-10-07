# i2pr-service-tunnels

Runtime-neutral policy and bounded protocol filters for I2P service tunnels. The crate
provides validated service specifications, explicit Destination/linkability groups,
access and resource policy, and HTTP, SOCKS, IRC, and CONNECT parsing/filtering.

The crate owns no sockets, async runtime, timers, DNS, filesystem, SAM sessions, or
router state. Applications supply transport-authenticated peer identity when a policy
requires it and own all connections, clocks, persistence, and lifecycle. See the
[portable core contract](../../specs/references/portable-service-tunnel-core-v1.md)
and ADR 0033 (`docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md` in
the i2pr repository).

This crate is experimental and is not a standalone tunnel product. The i2pr repository is
licensed under MIT. Package publication remains disabled for a technical reason rather
than a licensing one: this crate declares `i2pr-proto` as a path-only dependency, and
`cargo package` requires a version requirement on every dependency it stages, so the
crate cannot currently be packaged. Consuming it from Git works today; see
`docs/architecture/i2pr-service-tunnels.md` ("Distribution posture") in the i2pr
repository for the full audit and the chain that would unblock a crates.io release. No
SAM protocol implementation is included.
