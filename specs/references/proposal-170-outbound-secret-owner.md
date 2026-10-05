# Plan 341 — Outbound proxy secret owner: normative record

Status: **frozen 2026-10-05** (Plan 341).

This records the restart-safe, non-echoing owner for outbound proxy secrets — the
prerequisite Plan 327's own closure named as its blocker. It is written so a later
change cannot quietly reintroduce a plaintext password, a reusable nonce, or a
second persisted key.

## 1. Why a second secret owner exists at all

`i2pr_service_tunnels::auth::ProxyCredentials` is **deliberately one-way**. It
stores `SHA-256(username:realm:password)` and drops the password at construction.
That is exactly right for an inbound listener: the router only ever needs to
*verify* a presented pair, never to reproduce one.

An I2P-routed outproxy is the opposite case. The router is the **client** of the
outproxy and must *send* `Proxy-Authorization: Basic <base64(user:pass)>`
upstream. A one-way verifier cannot produce that credential, and after a restart
no plaintext exists anywhere to send. That asymmetry — not a missing config knob —
is why Plan 327 was closed blocked rather than partially built with an in-memory
or plaintext-persisted password.

Two consequences that must not be blurred:

- The two stored forms are **different kinds of value** and carry different
  markers: `$i2pr1$` for an inbound verifier, `$i2pr1o$` for a sealed outbound
  secret. Neither may ever be reinterpreted as the other, and
  `RouterBoundOutboundSecrets::open` rejects a real inbound verifier outright.
- The inbound path is **unchanged** by this work. It stays one-way, and adding an
  outbound owner must never make the inbound verifier reversible.

## 2. No pinned reference is authority for this design

Stated plainly so the construction is not mistaken for interoperability-derived:

| Reference | What it actually does |
|---|---|
| i2pd `2c69414` | Has **no I2P-routed outproxy**. Its only outproxy options are `httpproxy.outproxy`, `socksproxy.outproxy.enabled`, `socksproxy.outproxy`, `socksproxy.outproxyport` (`libi2pd/Config.cpp:143,177-179`) — a **clearnet** upstream defaulting to `127.0.0.1:9050`. It stores no outproxy password. |
| Java I2P freeze | Does route outproxies over I2P, but its at-rest scheme for outproxy credentials was **not verified** when this was written; the relevant subtree did not materialize from the sparse checkout. Not cited. |

Adopting i2pd's shape would violate this repository's no-direct-clearnet rule
outright. The design below is justified by this repository's own guardrails and
by Plan 292's established credential policy.

## 3. Construction

```text
key    = HKDF-SHA256(salt = "", ikm = router signing seed,
                     info = "i2pr:outproxy:secret-box:v1", len = 32)
stored = "$i2pr1o$" || hex(nonce || ciphertext || Poly1305 tag)
nonce  = 12 fresh bytes from the OS CSPRNG, per seal
aad    = "$i2pr1o$"
```

`chacha20poly1305` was already a pinned workspace dependency, so this adds a
manifest line, not a dependency and not a version change.

### Why the router signing seed

- **Restart-safe**: the seed is already persisted and reloaded on every start, so
  the router opens its own sealed forms with no new key file, no permission
  handling, and no rotation surface.
- **Non-transferable**: a sealed form lifted into another router's generation file
  derives a different key and fails to open. A stolen configuration file is not a
  stolen credential.
- **Separable**: HKDF with a purpose-specific label means the outproxy key is not
  the signing key, so compromising this store does not expose the router identity.

The label is a constant, not a parameter, so no caller can accidentally derive the
same key for a second purpose.

### Why a fresh nonce is mandatory

Under a fixed key, reusing a ChaCha20-Poly1305 nonce leaks the XOR of two
plaintexts and forfeits authentication entirely. The nonce is therefore drawn
from the CSPRNG per seal — never a counter, never a clock. A fixed-nonce
inversion is one of the teeth rows below.

## 4. Non-echo rules

| Surface | Rule |
|---|---|
| `OutboundSecret` | No `Debug`, no `Display`, no `Clone`. It cannot be printed, copied, or logged by accident. |
| `OutboundSecret` storage | Fixed `[u8; 512]` in a `Zeroizing`, so there is no heap allocation holding the credential and the whole buffer is erased on drop. |
| `OutboundSecretKey` | No `Clone`; `Debug`-free; zeroized on drop; `as_key` is private so no other module can hold the raw bytes. |
| Errors | Fixed reason strings. No error variant carries plaintext, a stored form, or a key. |
| Stored form | The only thing that reaches a generation file. It is opaque to a reader of that file and contains no plaintext. |
| `open` | Returns `OutboundSecret`, not `String`, so a recovered credential cannot be formatted into a log line by an ordinary `{:?}` at a call site. |

A credential is non-echoing in practice only if every surface is closed, so this
is checked at the type level rather than by convention.

## 5. Fail-closed rules

There is no best-effort path, because a partially recovered credential would be
sent to an upstream outproxy.

| Condition | Outcome |
|---|---|
| Unmarked stored form | error before any decode |
| Non-lowercase-hex or odd-length body | error before any decode |
| Frame shorter than nonce + tag | error — cannot be authentic |
| Frame longer than nonce + ceiling + tag | bounded error |
| Any tampered byte | authenticator rejection, no partial plaintext |
| Sealed by a different router | authenticator rejection |
| Decrypts to invalid UTF-8 | error; never a lossy value |
| Decrypts to empty | error; an empty secret is not a credential |
| No store installed | [`NoOutboundSecrets`] refuses both directions |

`NoOutboundSecrets` is the default, so "no owner installed" is a state the type
carries rather than an `Option` a caller can forget to check.

## 6. Honest limitations

- **Nothing routes anywhere yet.** This owner seals and opens a credential. The
  provider, the HTTP/CONNECT/SOCKS request paths, and the canonical
  `ProxyList` / `UseOutproxyPlugin` / `OutproxyAuth` / `OutproxyType` /
  `SSLProxies` semantics are **Plan 342** and are not implemented. No tunnel
  option reads this store yet.
- **No outproxy capability is claimed or advertised.** `specs/support.toml` is
  unchanged, and Plan 327 stays blocked.
- **The associated data is defense in depth without an isolated evidence row.**
  The AAD binds the ciphertext to the format marker, but because the marker is a
  constant that the framing check already enforces, dropping the AAD changes no
  observable behaviour and no row detects it. Recorded rather than claimed.
- **The framing pre-check is likewise redundant** with the strict hex decoder,
  and no row isolates it. It is kept because rejecting an absurd frame before any
  allocation is worth having, not because a row proves it.
- **`Zeroizing<[u8; 512]>` rather than a heap string.** This crate does not enable
  `zeroize/alloc`, and a fixed buffer is the better answer for a secret anyway;
  the ceiling is a compile-time constant, so no call site can forget a length
  check.

## 7. Evidence

| Requirement | Evidence |
|---|---|
| Round trip, and the stored form never contains the plaintext | `round_trip_recovers_the_credential` |
| A fresh nonce per seal, on the production path | `a_fresh_nonce_is_used_for_every_seal` |
| Tampering detected at every frame position | `tampering_is_detected_at_every_position` |
| Tampering rejected **by the authenticator**, not incidentally | `tampering_fails_on_authentication_not_on_an_incidental_check` |
| Truncation and padding fail closed | `a_truncated_or_padded_frame_fails_closed` |
| Another router cannot open the form | `another_router_cannot_open_the_form` |
| An inbound verifier is not an outbound sealed form | `plaintext_and_verifier_markers_are_not_interchangeable` |
| Empty and oversize plaintext refused; ceiling round-trips | `an_empty_or_oversize_plaintext_is_refused_before_sealing` |
| Restart recovers the credential from the same identity | `restart_recovers_the_credential_from_the_same_identity` |
| Availability reported; no error echoes material | `the_store_reports_availability_and_never_echoes` |
| Hex helpers strict (case, parity, non-hex) | `hex_helpers_are_strict` |
| Plaintext bounded and NUL-free | `plaintext_is_bounded_and_nul_free` (runtime-neutral layer) |
| Framing validated before decode | `stored_form_framing_is_checked_before_any_decode` |
| Oversize stored form rejected without work | `oversize_stored_form_is_rejected_without_work` |
| The default store fails closed | `the_default_store_fails_closed` |

Teeth: inverting the key derivation, ignoring the authenticator, and fixing the
nonce each make the corresponding row fail. Two further inversions — removing the
framing pre-check and dropping the AEAD associated data — make **no** row fail,
because both are redundant with independent checks; that is recorded in §6 rather
than papered over.
