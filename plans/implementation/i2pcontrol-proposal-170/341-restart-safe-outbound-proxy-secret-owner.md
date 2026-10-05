# Plan 341 — Restart-safe non-echoing outbound proxy secret owner

Status: **passed-outbound-secret-owner-with-no-routing-and-no-outproxy-claim**

Closure record:
[`plans/closure/i2pcontrol-proposal-170/341-status.md`](../../closure/i2pcontrol-proposal-170/341-status.md)

## Current implementation progress

Completed and closed on 2026-10-05. All work packages A-C landed. The owner
seals and opens an outbound credential across a restart, under a key derived from
the router's own persisted signing seed, and never echoes plaintext. Three teeth
inversions (key derivation, authenticator enforcement, nonce freshness) each
fail the corresponding row; two further inversions are recorded as redundant
defense layers without isolated evidence. Nothing routes anywhere: the provider
and the canonical field semantics are Plan 342, registered and not implemented.

Classification: capability + security boundary (prerequisite reopen of Plan 327).

Hard dependencies: Plan 327 closed blocked; Plan 292 passed (the existing
inbound verifier this plan must not disturb).

Subsystem: `i2pcontrol-proposal-170`.

## Objective

Establish the one prerequisite Plan 327's closure record named as its blocker: a
**restart-safe, non-echoing owner for outbound proxy secrets**.

Today `i2pr_service_tunnels::auth::ProxyCredentials` holds
`SHA-256(username:realm:password)` and drops the password at construction. That
is correct for *inbound* listener authentication — the router only ever needs to
verify a presented pair — and it is deliberately one-way. An I2P-routed outproxy
is the opposite case: the router must **send** the password upstream in a
`Proxy-Authorization: Basic` header, so a verifier cannot produce it. After a
restart there is no plaintext anywhere to send.

Implementing only an in-memory or plaintext-persisted password would fail both
the restart and the secret-handling requirements, which is exactly why Plan 327
was closed blocked rather than partially built.

## What this plan does and does not do

| In scope | Out of scope |
|---|---|
| The secret owner: seal, open, persist, fail closed | The outproxy **provider** and its selection/failover |
| A capability trait the tunnel-policy layer can call | HTTP/CONNECT/SOCKS request integration |
| Router-bound key derivation from persisted identity | `ProxyList`, `OutproxyAuth`, `OutproxyType`, `SSLProxies`, `UseOutproxyPlugin` semantics |
| Redaction, bounds, tamper and wrong-router rejection | Any direct clearnet socket, in any form |
| Persistence across a real restart | Any advertisement or support claim |

A provider that cannot obtain an outbound credential after a restart is not a
provider, so this lands first. Plan 342 carries the rest.

## No pinned reference is authority for this design

Recorded so the design is not mistaken for reference-derived:

- The pinned **i2pd** `2c69414` has no I2P-routed outproxy at all. Its only
  outproxy options are `httpproxy.outproxy`, `socksproxy.outproxy.enabled`,
  `socksproxy.outproxy`, and `socksproxy.outproxyport`
  (`libi2pd/Config.cpp:143,177-179`) — a **clearnet** upstream defaulting to
  `127.0.0.1:9050`. It stores no outproxy password. Adopting its shape would
  violate this repository's no-direct-clearnet rule outright.
- The pinned **Java I2P** freeze does route outproxies over I2P, but its
  at-rest scheme for outproxy credentials was **not verified in this session**
  (the relevant subtree did not materialize from the sparse checkout). It is
  therefore not cited as authority.

The design below is justified by this repository's own guardrails and by
Plan 292's established credential policy, and says so in its own documentation.

## Design

**Capability, not a dependency.** `i2pr-service-tunnels` is permitted only
`i2pr-client` and `i2pr-proto` internally, so it cannot hold an AEAD. The
runtime-neutral layer therefore owns a **trait**; the daemon owns the
implementation. This mirrors the existing injected-capability pattern
(`RouterDeliveryService`, the local sink) and adds no new crate edge.

**Router-bound key.** The key is `HKDF-SHA256(salt = "", ikm = router signing
seed, info = "i2pr:outproxy:secret-box:v1", 32)`. The signing seed is the
persisted `SigningPrivateKey` already loaded at composition. Consequences, all
intended:

- **Restart-safe**: the same identity derives the same key on the next start.
- **Non-transferable**: a copied generation file is inert without the router's
  private identity, so a stolen config yields no credential.
- **No new key file**: no second secret to protect, rotate, or permission-check.

**Sealed form.** ChaCha20-Poly1305, fresh 12-byte nonce per seal, stored as
`$i2pr1o$` plus lowercase hex of `nonce || ciphertext || tag`. A fresh nonce per
seal is mandatory: reusing one under a fixed key would leak plaintext. The
workspace already pins `chacha20poly1305`, so this is a manifest line, not a new
dependency and not a version change.

**Non-echoing.** `Debug` and `Display` on every secret-bearing type print only
lengths. `open` returns the plaintext only at the point of header construction,
wrapped so it is zeroized on drop. No error variant carries plaintext. The
stored form is the only thing that reaches a generation file, and it is
indistinguishable from any other opaque blob to a reader of that file.

**Fail closed.** An unmarked, malformed, truncated, tampered, or
wrong-router value is a typed error with no partial plaintext. There is no
"decrypt best effort" path and no fallback to a stored verifier.

**Not `Clone`.** No secret-bearing type derives `Clone`, per the repository's
secret rules.

## Invariants

1. Plaintext exists only in a zeroizing buffer, only at header construction.
2. No `Debug`, `Display`, error, log, or control output ever contains plaintext
   or the derived key.
3. A stored form that is malformed, tampered, or from another router fails
   closed with no partial plaintext.
4. The key never leaves the daemon, is never persisted, and is derived fresh from
   the router identity on every start.
5. Plaintext and sealed forms are both bounded before any allocation.
6. No new task, timer, queue, listener, or socket.
7. The existing inbound verifier path is untouched and still one-way.

## Work packages

- **A** — `i2pr-service-tunnels`: the `OutboundSecretStore` trait, the stored-form
  marker and validation, the fail-closed default, unit rows.
- **B** — `i2pr-daemon`: the router-bound AEAD implementation, key derivation,
  composition wiring, unit rows.
- **C** — records.

## Tests

Unit: seal/open round trip; tamper detection at each byte position; wrong-router
rejection; unmarked/malformed/truncated/oversized rejection; non-echo across
`Debug`, `Display`, and every error; no-`Clone`; restart equivalence (rebuild
from the same seed opens the same value); a fresh nonce per seal.

Teeth: verified by inverting each fix — accepting a tampered form, echoing the
plaintext in `Debug`, and deriving the key from a constant — and confirming the
corresponding rows fail.

## Out-of-scope guard

Nothing here may be used to open a clearnet socket. The owner seals and opens a
credential; routing is Plan 342's problem and must go through the existing
Destination/Streaming path.

## Stop conditions

Stop and re-audit if the design would require plaintext in a generation file, a
second persisted key, a new crate edge, or any `Clone`/`Debug` on a secret.
