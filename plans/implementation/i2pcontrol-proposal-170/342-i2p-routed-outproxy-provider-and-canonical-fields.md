# Plan 342 — I2P-routed outproxy provider and canonical proxy field completion

Status: **registered-routed-outproxy-provider-awaits-no-implementation**

Classification: capability + security boundary (completes Plan 327).

Hard dependencies: Plan 341 passed (the outbound secret owner); Plan 323 closed;
Plan 327 closed blocked.

Subsystem: `i2pcontrol-proposal-170`.

## Objective

Give Plan 170's outproxy-related TunnelManager fields real semantics, on top of
the secret owner Plan 341 landed. This is Plan 327's remaining scope.

**Nothing here is implemented yet.** This document is the registered plan of
record for the work, not a claim about it.

## Provider architecture

A small typed outproxy-provider interface owned by the service-tunnel layer, with
one shipped implementation: a **static, configured, I2P-routed provider**.

- Selects from configured I2P outproxy destinations, validated as I2P names or
  destinations. A `ProxyList` entry is never a clearnet host.
- Opens every route through the existing Destination/Streaming path.
- **Never** opens a direct clearnet socket, under any failure. There is no
  fallback branch to remove later, because there is never a fallback.
- Returns typed unavailable / auth / resolve / connect outcomes.
- Uses bounded selection, retry, and backoff with a fixed ceiling.

`UseOutproxyPlugin` is the Java provider mechanism by wire name. i2pr does not
load plugins; `true` selects the configured provider path, and if none is
configured then creation and edit fail **before** any listener or destination is
allocated. No dynamic library loading and no command execution from I2PControl
input, ever.

## Canonical fields

Real semantics for `ProxyList`, `UseOutproxyPlugin`, `OutproxyAuth`,
`OutproxyUsername`, `OutproxyPassword`, `OutproxyType`, `SSLProxies`, and the
relevant `JumpList` behaviour.

- `OutproxyType` is a typed finite vocabulary. It is **not** an arbitrary
  executable or provider name, and it is not read from a config file as a command.
- `OutproxyPassword` is persisted only through Plan 341's sealed form, and is
  recovered only at header construction. It is never echoed into control output,
  even where Java's `rawConfig` would return it.
- Changing a provider or proxy list is a transactional reconfiguration with
  explicit drain and rebuild semantics, matching the existing
  `rollback_state` behaviour.

## Request paths

Integrate provider selection into the HTTP and CONNECT client paths and the
Proposal-applicable SOCKS families, preserving:

- direct `.i2p` routing, which bypasses the outproxy;
- no DNS leak;
- no direct-clearnet fallback;
- proxy auth redaction;
- bounded HTTP/CONNECT parsing;
- existing TLS semantics from service-tunnel policy.

## Evidence

The self-composed in-tree loopback outproxy fixture, decided with Plan 339 and
already on record: it is **loopback evidence, not interoperability**, matching the
existing `sam_stream_self_composed` precedent. Nothing vendored, patched, or
reused from an external router.

Required rows: `.i2p` direct path bypasses the outproxy; a clearnet target
succeeds through the controlled loopback outproxy; a clearnet target **without** a
provider fails and opens no direct socket; failover and retry ceiling; auth
success and failure with no secret echo; malformed `ProxyList` and `OutproxyType`;
HTTP CONNECT and applicable SOCKS coverage; restart and persistence; and proof
that no dynamic library loading or unrestricted execution exists.

## Invariants

1. No direct clearnet capability is introduced, in any path, including failures.
2. Every outbound credential moves through Plan 341's owner and nowhere else.
3. `UseOutproxyPlugin` with no configured provider fails before allocation.
4. Provider selection and retry are bounded.
5. No plugin loading and no command execution from control input.
6. A transaction that fails leaves no partial provider state.

## Out of scope

Encrypted LeaseSet2 and Red25519 (Plans 326/335), the type-11 ecosystem split, the
Plan 328 full-conformance gate, and any advertisement change.

## Stop conditions

Stop and re-audit if any design step would require a clearnet socket, a plaintext
password outside Plan 341's owner, an unbounded retry, or a plugin/exec
capability.
