# Plan 343 — I2P-routed outproxy provider policy and route owner

Status: **passed-outproxy-provider-policy-and-route-owner-with-no-reachable-request-path**

Closure record:
[`plans/closure/i2pcontrol-proposal-170/343-status.md`](../../closure/i2pcontrol-proposal-170/343-status.md)

## Current implementation progress

Completed and closed on 2026-10-05. Both halves of the provider landed: the
runtime-neutral decision layer and the daemon route owner. Three static-guard
inversions (clearnet socket, `dlopen`, `Command::new`) each fail the guard
closed, and one positive control proves the guard is not vacuous.

**Not reachable.** No Proposal 170 option sets an outproxy yet and no request
path consults the provider, so this is provider infrastructure and not a
capability claim. Plan 342 remains registered and owns the option surface, the
HTTP/SOCKS request-path integration, and the wire evidence lane.

Classification: capability + security boundary (foundation for Plan 342).

Hard dependencies: Plan 341 passed (the outbound secret owner this consumes);
Plan 342 registered.

Subsystem: `i2pcontrol-proposal-170`.

## Objective

Give the outproxy a real shape, so Plan 342 has something to wire a request path
to and Plan 327's "no provider" blocker has an answer.

The provider is two halves, split by what each layer is allowed to do:

- `i2pr-service-tunnels` owns the **policy**: what an outproxy is, which one to
  pick, what a request to it must look like, and what a failure means. No
  sockets, no async, no cryptography beyond base64 of a credential it is handed.
- `i2pr-daemon` owns the **I/O**: open a Streaming route to the selected I2P
  outproxy destination, speak the outproxy-facing handshake over it with bounded
  retry and a bounded read, and recover the credential through Plan 341's
  `OutboundSecretStore`.

## The invariant everything serves

**No direct clearnet capability exists anywhere in this design.** The router
never resolves a clearnet name, never opens a clearnet socket, and has no
fallback branch. A clearnet target is only ever an opaque label handed to an
outproxy, and the only route the router ever opens is an I2P Streaming
connection to an outproxy destination.

This is enforced twice, deliberately:

- **Statically.** `scripts/check-service-tunnel-boundaries.sh` rules 9-11 scan
  both outproxy files for socket, resolver, and plugin-loading spellings, with a
  positive control so the guard cannot silently become vacuous.
- **Structurally.** `OutproxyEndpoint::parse` refuses any `ProxyList` entry that
  is not an I2P destination, so an outproxy endpoint cannot even *name*
  something the router would connect to outside I2P.

There is no fallback branch to remove later, because there is never a fallback.

## The target grammar is separate, not a relaxation

`http::target::validate_host` rejects every non-`.i2p` host, and it is called
*inside* `parse_absolute_form` and `parse_authority_form`. That is correct for
the direct path and an existing row asserts it (`absolute_form_rejects_clearnet`).
Weakening it to admit clearnet when an outproxy is configured would weaken the
direct path too.

So `OutproxyTarget` is its own grammar for the outproxy path, and the two never
share a parse result. It accepts two disjoint forms, and which one matched is
what `is_i2p()` reports:

- an `.i2p` destination or static alias, which **bypasses** the outproxy;
- a bare clearnet DNS label, the only thing an outproxy is ever asked to resolve.

The `.i2p` arm exists in the grammar on purpose. If the grammar refused it, the
bypass would be unreachable through this type, the decision would have to be
duplicated in every caller's control flow, and "an `.i2p` request is never
diverted off-network" would be a property of call sites rather than of the
design. Accepting it makes `OutproxyConfig::route` the single place that refuses
to select an outproxy for an in-network target.

## Design decisions

- **`OutproxyType` is a closed vocabulary** (`http`, `socks5`, `socks4a`). It is
  not a provider name and is never treated as a command, path, or module, so
  there is no spelling that reaches anything executable.
- **IP literals are refused as outproxy targets.** An outproxy *can* reach them,
  and letting a local client ask one to connect to an arbitrary internal address
  on the outproxy's network is a port-scan primitive. i2pr does not relay that.
- **No resolver exists.** The clearnet host is an opaque label. i2pr performs no
  DNS resolution, so "no DNS leak" is a property of the type rather than a
  promise.
- **Linear backoff, saturating.** The attempt count is already hard-capped, so an
  exponential curve buys nothing and needs saturating shift arithmetic to be
  safe. `BASE_STEP * (attempt - 1)`, clamped to the ceiling.
- **Every operator input is clamped, not rejected.** A request for 1000 attempts
  is answered with the ceiling. The point is that no input can produce an
  unbounded retry or an unbounded socket wait.
- **`SSLProxies` is a tunnelled allowlist that must be a subset of `ProxyList`.**
  A tunnelled outproxy outside the list would be a route the failover policy
  never rotates into and never accounts for, so it is refused.
- **Errors carry no operator value.** Echoing a rejected `ProxyList` entry would
  put unbounded wire input into an error that reaches logs and control replies
  for no gain the caller does not already have.
- **Tunnelled bytes are preserved.** A proxy may answer `CONNECT` and start
  pushing payload in the same burst; the bytes after the response head become
  the pump's first read rather than being dropped.

## Locally defined, not interoperability-derived

No pinned reference is authority here. Pinned i2pd `2c69414` has no I2P-routed
outproxy at all — its outproxy options are a **clearnet** upstream defaulting to
`127.0.0.1:9050` with no stored password (`libi2pd/Config.cpp:143,177-179`) —
and adopting that shape would violate the no-direct-clearnet rule outright. The
pinned Java I2P at-rest scheme for outproxy credentials was not verified. The
`OutproxyType` vocabulary and the `SSLProxies` subset rule are therefore i2pr's
own design, justified by this repository's guardrails.

## Secrets

- `OutboundSecret` and `OutproxyAuthHeader` have no `Debug`, no `Display`, and no
  `Clone`.
- Both are fixed-size `Zeroizing<[u8; N]>` buffers. This crate does not enable
  `zeroize/alloc`, and a credential should not sit in a heap allocation anyway.
- The `username:password` pair is staged in a compile-time-bounded buffer, so no
  secret placement depends on input length.
- The stored form is Plan 341's sealed ciphertext and is opened only inside
  `auth_header`, so the plaintext's lifetime is header construction.
- A SOCKS outproxy refuses an HTTP Basic credential rather than silently dropping
  it on the wire.
- `OutproxyCounters` is counts only: no host, username, header, or error string.

## Invariants

1. No direct clearnet capability is introduced, in any path, including failures.
2. Every outbound credential moves through Plan 341's owner and nowhere else.
3. A clearnet target with no configured provider is a typed refusal.
4. Selection, retry, backoff, connect timeout, and handshake read are all bounded.
5. No plugin loading and no command execution.
6. An `.i2p` target is never routed through an outproxy.

## Out of scope

The Proposal 170 option surface and the HTTP/SOCKS request-path integration
(Plan 342); the wire evidence lane (Plan 342); encrypted LeaseSet2 and Red25519
(Plans 326/335); the type-11 ecosystem split (Plan 335); the Plan 328
conformance gate; any advertisement change.

## Stop conditions

Stop and re-audit if any design step would require a clearnet socket, a
plaintext password outside Plan 341's owner, an unbounded retry, or a
plugin/exec capability.
