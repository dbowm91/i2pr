# Plan 327 — I2P-routed outproxy provider disposition

Status: **blocked-prop170-outproxy-provider-needs-routed-provider-and-secret-owner**

Implementation commits: none. Plan 327 was audited after Plan 323 closed; no product implementation was added because the required provider and credential lifecycle owners are absent.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Resolve configured outproxy destinations through I2P and open a Streaming route, without direct clearnet fallback. | `crates/i2pr-daemon/src/service_tunnels_http.rs::resolve_target_for_service` and `open_streaming`; `crates/i2pr-daemon/src/service_tunnels_socks5.rs` | Blocked: existing client paths resolve request targets as `.i2p` destinations. No configured outproxy selector or HTTP/SOCKS-to-outproxy protocol owner exists. |
| Give ProxyList, UseOutproxyPlugin, OutproxyAuth, OutproxyUsername, OutproxyPassword, OutproxyType, and SSLProxies typed canonical owners. | `crates/i2pr-i2pcontrol/src/tunnel_options.rs`; `crates/i2pr-i2pcontrol/src/tunnel_matrix.rs`; `plans/implementation/i2pcontrol-proposal-170/327-i2p-routed-outproxy-provider.md` | Blocked: only the legacy `use_outproxy_plugin` slot exists in the current option inventory, with a fail-closed incompatibility. The other listed fields have no canonical mapping or product owner. |
| Send upstream credentials without plaintext persistence, response echo, or secret logging, and preserve restart behavior. | `crates/i2pr-daemon/src/i2pcontrol_tunnels.rs::persisted_control_options`; `crates/i2pr-service-tunnels/src/auth.rs::ProxyCredentials::stored_form` | Blocked: current stored form is a one-way verifier for inbound listener authentication. It cannot supply an outbound Basic credential after restart. TunnelManager option serialization is not a safe outbound secret store. |
| Bound provider selection/retries and fail closed when unavailable. | Existing `ServiceTunnelManager` bounded lifecycle and failure paths; no outproxy-provider owner | Not implemented; cannot be evidenced without the provider owner. |
| Exercise HTTP, CONNECT, applicable SOCKS, auth, failover, restart, and no-direct-fallback behavior. | Existing product tests cover direct `.i2p` paths only | Not run: there is no outproxy path to test, and no direct clearnet path was introduced. |

## Verification, compatibility, and security

This was a read-only source/architecture audit. No tests were added or run specifically for Plan 327; no product behavior, dependency, migration, protocol advertisement, listener, or network route changed. The branch's current local workspace floor is green (3,775 passed, 35 ignored; 133 suites), but that does not evidence outproxy behavior. No secret bytes or destination material were changed or exposed.

Findings by severity: critical 0; high 0; medium 0; low 0. The missing durable outbound credential owner is a prerequisite gap, not a validated vulnerability. Implementing only an in-memory or plaintext-persisted password would fail the plan's restart and secret-handling requirements.

## Roadmap disposition

Plan 327 is closed as blocked, not passed. To reopen, establish a reviewed non-echoing, restart-safe outbound-secret owner and implement the static I2P destination provider plus HTTP/CONNECT/SOCKS request semantics. Provider selection must route only through the existing Destination/Streaming path and must never fall back to an OS socket. Plan 328 remains blocked on Plans 322, 326, and 327. No full-Proposal claim is supported.

## Correction, 2026-10-05 — one of the two blockers is gone (dated; the text above is preserved)

Re-auditing this record while registering Plan 341 confirmed the diagnosis and
narrowed the blocker list from two items to one.

**Still blocked, and unchanged:** there is no outproxy provider. Nothing selects
an outproxy, opens a route to one, retries across them, or translates
`ProxyList` / `OutproxyType` / `UseOutproxyPlugin` / `SSLProxies` into
behaviour, and no request path performs HTTP, CONNECT, or SOCKS outproxy
semantics. That work is now registered as **Plan 342**.

**No longer a blocker:** the durable outbound credential owner. The record said
an in-memory or plaintext-persisted password would fail the restart and
secret-handling requirements — that remains true, and Plan 341 solved it the
only acceptable way. A ChaCha20-Poly1305 sealed form under a key derived by HKDF
from the router's own persisted signing seed is restart-safe, inert in a copied
configuration file, and never echoes plaintext. See
[`341-status.md`](341-status.md) and
[`specs/references/proposal-170-outbound-secret-owner.md`](../../../specs/references/proposal-170-outbound-secret-owner.md).

**One finding the original audit could not have made.** The plan assumed a
reference implementation would supply the design. There is none: pinned i2pd
`2c69414` has no I2P-routed outproxy at all — its outproxy is a clearnet upstream
defaulting to `127.0.0.1:9050`, with no stored password — and the pinned Java
at-rest scheme was not verified. i2pr therefore designs this from its own
guardrails, and says so rather than implying interoperability.

**Status token deliberately unchanged.** Plan 327 is still blocked; only its
blocker list shrank. Plan 328 remains blocked on 326 and 327.
