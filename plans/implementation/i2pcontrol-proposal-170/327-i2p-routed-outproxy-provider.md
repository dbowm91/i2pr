# Plan 327 — I2P-routed outproxy provider and canonical proxy option completion

Status: **blocked-prop170-outproxy-provider-needs-routed-provider-and-secret-owner**

Classification: capability + security boundary.

Hard dependency: Plan 323 closed.

## Objective

Implement the Proposal 170 outproxy-related TunnelManager fields without violating i2pr’s no-runtime-plugin and no-implicit-direct-clearnet architecture.

The wire name UseOutproxyPlugin describes Java’s provider mechanism; i2pr does not need to load arbitrary Rust plugins to provide equivalent semantics.

## Provider architecture

Add a small typed outproxy-provider interface owned by the service-tunnel layer.

The default provider is a static/configured I2P-routed provider:
- selects from configured I2P outproxy destinations;
- opens the route through the existing Destination/Streaming path;
- never opens a direct clearnet socket;
- returns typed unavailable/auth/resolve/connect outcomes;
- uses bounded selection/retry/backoff.

An optional authenticated out-of-process provider may be considered only if it materially improves compatibility and preserves the same network boundary. No arbitrary dynamic library loading from I2PControl input.

## Canonical fields

Give real semantics to:
- ProxyList;
- UseOutproxyPlugin boolean;
- OutproxyAuth;
- OutproxyUsername;
- OutproxyPassword;
- OutproxyType;
- SSLProxies;
- relevant JumpList behavior.

UseOutproxyPlugin=true selects the configured provider path. If none is configured, creation/edit fails before listener/destination allocation. false/absent follows the ordinary non-outproxy policy.

ProxyList entries must be validated I2P names/destinations. They are not clearnet hosts.

## HTTP/SOCKS behavior

Integrate provider selection into HTTP and CONNECT client paths and the Proposal-applicable SOCKS families according to the frozen canonical applicability matrix.

Preserve:
- .i2p direct routing;
- no DNS leak;
- no direct-clearnet fallback;
- proxy auth secret redaction;
- bounded CONNECT/HTTP parsing;
- TLS semantics from existing service-tunnel policy.

OutproxyType is a typed finite vocabulary derived from the pinned Proposal/Java behavior, not an arbitrary executable/provider name.

## Persistence and rotation

Provider references and public proxy lists may persist in control generations. Passwords must use the existing verifier/secret-store policy or another non-echoing secret owner; do not serialize plaintext secrets merely because Java rawConfig does.

Changing provider/proxy list is a transactional service reconfiguration with clear drain/rebuild semantics.

## Evidence

- .i2p direct path bypasses outproxy;
- clearnet target with provider succeeds through a controlled I2P outproxy fixture;
- clearnet target without provider fails and never opens a direct socket;
- provider failover/retry ceiling;
- auth success/failure and no secret echo;
- malformed ProxyList/OutproxyType;
- HTTP CONNECT and applicable SOCKS coverage;
- restart/persistence;
- no dynamic library loading or unrestricted command execution.

## Acceptance criteria

Plan 327 closes when every canonical outproxy field has a real I2P-routed owner and UseOutproxyPlugin is no longer an explicit incompatibility.

No direct-clearnet capability is introduced.

## Current disposition

Plan 327 is closed blocked in `plans/closure/i2pcontrol-proposal-170/327-status.md`. The existing `.i2p` Streaming path does not supply an outproxy protocol or configured provider owner. The current credential persistence path stores a one-way inbound-auth verifier, which cannot produce an outbound password after restart. Reopen after a reviewed static I2P outproxy/HTTP-SOCKS provider and restart-safe non-echoing outbound secret owner are available.
