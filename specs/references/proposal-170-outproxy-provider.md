# Plan 343 — I2P-routed outproxy provider: normative record

Status: **frozen 2026-10-05** (Plan 343).

This records the shape of the I2P-routed outproxy provider: what an outproxy is,
how one is selected, what a request to it must look like, and what is forbidden.
It is written so a later change cannot quietly introduce a direct-clearnet
fallback, a plugin loader, an unbounded retry, or a weakening of the direct
path's `.i2p`-only grammar.

## 1. The one invariant everything else serves

**No direct clearnet capability exists anywhere in this design.**

The router never resolves a clearnet name, never opens a clearnet socket, and
has no fallback branch. A clearnet target is only ever an opaque label handed to
an outproxy, and the only route the router ever opens is an I2P Streaming
connection to an outproxy destination.

This is stated first because it is the property that everything else is shaped
around, and because it is the one a future change is most likely to erode
without noticing. There is no fallback branch to remove later, because there is
never a fallback.

It is enforced in three independent places:

| Layer | Mechanism | Where |
|---|---|---|
| Structural | `OutproxyEndpoint::parse` refuses any `ProxyList` entry that is not an I2P destination | `i2pr-service-tunnels/src/outproxy.rs` |
| Behavioural | A clearnet target with no configured provider is a typed refusal, never a direct route | `OutproxyConfig::route`, `open_via_outproxy` |
| Static | Rules 9-11 scan both outproxy files for socket, resolver, and plugin spellings, with a positive control | `scripts/check-service-tunnel-boundaries.sh` |

## 2. No pinned reference is authority for this design

| Reference | What it actually does |
|---|---|
| i2pd `2c69414` | Has **no I2P-routed outproxy**. Its only outproxy options are `httpproxy.outproxy`, `socksproxy.outproxy.enabled`, `socksproxy.outproxy`, `socksproxy.outproxyport` (`libi2pd/Config.cpp:143,177-179`) — a **clearnet** upstream defaulting to `127.0.0.1:9050`, with no stored password. |
| Java I2P freeze | Does route outproxies over I2P, but its spellings for these fields and its at-rest credential scheme were **not verified** when this was written; the relevant subtree did not materialize from the sparse checkout. Not cited. |

Adopting i2pd's shape would violate the no-direct-clearnet rule outright. The
`OutproxyType` vocabulary and the `SSLProxies` subset rule below are therefore
**i2pr's own design**, justified by this repository's guardrails, and are not
presented as interoperability-derived.

## 3. The target grammar is separate, not a relaxation

`http::target::validate_host` rejects every non-`.i2p` host, and it is called
*inside* `parse_absolute_form` and `parse_authority_form`. That is correct for
the direct path, and `absolute_form_rejects_clearnet` asserts it. Admitting
clearnet conditionally would weaken the direct path too.

`OutproxyTarget` is therefore a **separate grammar** for the outproxy path, and
the two never share a parse result. It accepts two disjoint forms, and which one
matched is what `is_i2p()` reports:

| Form | Meaning | Route |
|---|---|---|
| `.i2p` destination or static alias | An I2P name | **Direct**, outproxy never consulted |
| Bare clearnet DNS label | An opaque label for the outproxy to resolve | Via the outproxy |

The `.i2p` arm exists in the grammar deliberately. If the grammar refused it, the
bypass would be unreachable through this type, the decision would have to be
duplicated in every caller's control flow, and "an `.i2p` request is never
diverted off-network" would be a property of call sites rather than of the
design.

### The clearnet grammar is narrower than a resolver accepts

- **IP literals are refused.** An outproxy *can* reach them, and letting a local
  client ask one to connect to an arbitrary internal address on the outproxy's
  network is a port-scan primitive. i2pr does not relay that.
- A trailing root dot is refused: the outproxy, not this router, decides how to
  spell the name.
- Labels are non-empty, at most 63 bytes, alphanumeric plus `-`, and may not
  start or end with `-`.
- The total host is at most 253 bytes.
- `user@host` is refused, so userinfo can never reach an upstream authority.

## 4. Typed vocabulary and bounded policy

`OutproxyType` is a **closed** set — `http`, `socks5`, `socks4a`. It is not a
provider name and is never treated as a command, path, or module, so there is no
spelling that reaches anything executable.

`OutproxyPolicy` clamps rather than rejects:

| Input | Result | Ceiling |
|---|---|---|
| `attempts = 1000` | 4 | `MAX_OUTPROXY_ATTEMPTS` |
| `attempts = 0` | 1 | floors, so "never try" is not expressible |
| `connect_timeout_ms = 10^6` | 120 000 | `MAX_OUTPROXY_CONNECT_TIMEOUT_MS` |
| `backoff_ceiling_ms = 10^6` | 5 000 | `MAX_OUTPROXY_BACKOFF_MS` |

Backoff is `BASE_STEP * (attempt - 1)` clamped to the ceiling: a **linear** ramp,
not exponential. The attempt count is already hard-capped, so an exponential
curve buys nothing and needs saturating shift arithmetic to be safe.
`backoff_ms(usize::MAX)` saturates at the ceiling rather than wrapping.

## 5. Cross-field rules

- **`SSLProxies` is a tunnelled allowlist that must be a subset of `ProxyList`.**
  A tunnelled outproxy outside the list would be a route the failover policy
  never rotates into and never accounts for, so it is refused rather than
  allowed.
- **An empty tunnelled list permits no tunnelled request.** Fail-closed default.
- **`present_credential` requires a username.** And an armed credential whose
  stored form cannot be recovered is an error, never an unauthenticated request.
- **A SOCKS outproxy refuses an HTTP Basic credential** rather than silently
  dropping it on the wire.

## 6. Secret handling

- `OutboundSecret` and `OutproxyAuthHeader` have no `Debug`, no `Display`, and
  no `Clone`. There is no way to print or copy a credential.
- Both are fixed-size `Zeroizing<[u8; N]>` buffers. `i2pr-service-tunnels` does
  **not** enable `zeroize/alloc`, and a credential should not sit in a heap
  allocation anyway. The ceiling is a compile-time constant, so no length check
  can be forgotten at a call site.
- The `username:password` pair is staged in a compile-time-bounded buffer, so no
  secret placement depends on input length.
- The stored form is Plan 341's sealed ciphertext and is opened **only** inside
  `RouterOutproxyProvider::auth_header`, so the plaintext's lifetime is header
  construction.
- A username containing `:` or a control byte is refused at build time, because
  it could otherwise alter the Basic pair framing or inject a header boundary.
- `OutproxyCounters` is **counts only** — no host, username, header, or error
  string — so it is safe to project into a status surface verbatim.

## 7. Errors carry no operator value

No `OutproxyError` variant carries the rejected value. Echoing a rejected
`ProxyList` entry or `OutproxyType` spelling would put unbounded wire input into
an error message that reaches logs and control replies, for no gain: the value
is already in the failed request the caller sent.

`classify` is **total** over the enum, so a new codec error cannot be silently
reported as a dead outproxy.

## 8. Response framing

- **HTTP.** Exactly `HTTP/1.1` — RFC 9110 §9.3.6 requires HTTP/1.1 for
  `CONNECT`, and accepting the `HTTP/1.` prefix would let a 1.0 response pass as
  a tunnel grant. Only the status line is interpreted. `407` is a distinct typed
  failure (`CredentialRejected`) so a wrong password is not reported as a dead
  outproxy.
- **SOCKS5.** Length-driven, not fixed-offset: the reply length comes from the
  address type and the domain length byte. Every address type is followed by a
  2-byte port; omitting it is exactly the defect Plan 343's own row caught.
- **SOCKS4a.** Eight-byte reply, with a null status byte required.
- **Tunnelled bytes are preserved.** A proxy may answer `CONNECT` and start
  pushing payload in the same burst, so everything after the response head
  becomes the session's `tunnel_prefix` and the pump's first read. Dropping it
  would corrupt the first request on every such outproxy.

## 9. What this record does not claim

The provider is **not reachable from any request path**. No Proposal 170 option
sets an outproxy and no HTTP or SOCKS handler consults the provider, so the code
is exercised only by its own tests. That is provider infrastructure, not a
capability, and `specs/support.toml` and every advertisement surface are
untouched. The option surface, the request-path integration, and the loopback
outproxy wire lane are Plan 342.

**Plan 327 remains blocked.**
