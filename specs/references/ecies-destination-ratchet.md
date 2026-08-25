# ECIES-X25519-AEAD-Ratchet destination ratchet — Plan 126 evidence note

Status: implemented-locally (Plan 126). This note records the normative
contract `i2pr-crypto::ecies` now implements, the corrections applied to
the superseded Plan 121 dialect, and the independent provenance of the
frozen conformance vectors.

## Scope

The destination ECIES session layer (`i2pr-crypto::ecies` primitives and
the `i2pr-client::session` manager) implements the I2P
ECIES-X25519-AEAD-Ratchet contract for the local New Session (NS) →
New Session Reply (NSR) → Existing Session (ES) lifecycle, bound to a
destination's long-term X25519 static key. The superseded Plan 121
i2pr-internal dialect (flag byte 0x00/0x02 framing, per-session random
"static" keys, single shared tag chain, no Noise split) is removed and
cannot be constructed through any public API.

## Wire formats implemented

| Message | Layout | Total length |
| --- | --- | --- |
| Bound NS | `elg2_aepk(32) \|\| static_section_ct(48) \|\| payload_ct(len+16)` | `96 + payload_len` |
| NSR | `tag(8) \|\| elg2_bepk(32) \|\| zero-len key-section MAC(16) \|\| payload_ct(len+16)` | `72 + payload_len` |
| ES | `tag(8) \|\| payload_ct(len+16)` | `24 + payload_len` |

There are no flag bytes anywhere in any message. Unbound New Sessions
(static-key section encrypting 32 zero bytes) are rejected typed with
`EciesError::UnboundNewSessionNotSupported`.

## Protocol constants

- Protocol name: `Noise_IKelg2+hs2_25519_ChaChaPoly_SHA256` (40 bytes).
  Note: the Plan 126 plan text paraphrased this name as
  `Noise_IKelg2_25519_ChaChaPoly_SHA256`; per plan §11.3 the current
  specification and both pinned references were checked and the `+hs2`
  variant is authoritative.
- `h0 = SHA256(protocol_name)` = `4CAF11EF2C8E36564C53E88885064DBAACBE0054AD178F8079A646827E6EE40C`
- `hh = SHA256(h0)` = `9CCF852CC93BB9504441E950E01D52322E0D47ADD1E9A555F755B569AE183B5C`

Both digests match the i2pd 2.60.0 `InitNoiseIKState` constants
(`crypto/cpp/Crypto.cpp`) and were re-derived locally with an
independent SHA-256 implementation.

## Handshake and ratchet derivation (as implemented)

NS (Alice → Bob):

```text
h  = hh; MixHash(bpk); MixHash(elg2_decode(aepk))
es = DH(aepk_priv, bpk);            (key, k) = HKDF(ck, es, "", 64)
MixHash(MAC included)               static_ct = AEAD(k, 0, apk, h)
ss = DH(apk_priv, bpk);             (ck, k) = HKDF(ck, ss, "", 64)
payload_ct = AEAD(k, 0, payload, h)
```

The static-key section encrypts Alice's **derived public key** (`apk`),
never her private scalar.

NSR (Bob → Alice), rooted at the final NS chaining key:

```text
tagsetKey = HKDF(ns_ck_final, ZEROLEN, "SessionReplyTags", 32)
DH_INITIALIZE(ns_ck_final, tagsetKey)   -> one-shot reply window
tag = first issued entry; h' = SHA256(h || tag)
ee: (ck_ba, k_ee) = HKDF(ck, DH(bepk_priv, aepk_pub), "", 64); MixHash(eepk); MixKey
se: (ck', k_se)  = HKDF(ck_ba, DH(bepk_priv, apk), "", 64)
MAC = AEAD(k_se, 0, ZEROLEN, h'); MixHash(MAC)
split: keydata = HKDF(ck', ZEROLEN, "", 64)
       k_ab = keydata[0:31]  (Alice -> Bob)
       k_ba = keydata[32:63] (Bob -> Alice)
payload_key = HKDF(k_ba, ZEROLEN, "AttachPayloadKDF", 32); payload = AEAD(payload_key, 0, p, h')
tagset_ab = DH_INITIALIZE(ck', k_ab)
tagset_ba = DH_INITIALIZE(ck', k_ba)
```

The specification prose describing an "ss" block inside the NSR payload
KDF is stale; proposal 144, pinned i2pd 2.60.0, and Java I2P all use the
Split + AttachPayloadKDF construction above.

TagSet ratchet:

```text
DH_INITIALIZE(rootKey, k):
    keydata = HKDF(rootKey, k, "KDFDHRatchetStep", 64)
    ck      = keydata[32:63]
    (sessTag_ck, symmKey_ck) = HKDF(ck, ZEROLEN, "TagAndKeyGenKeys", 64)
NextSessionTagRatchet:
    (chainKey, SESSTAG_CONSTANT) = HKDF(sessTag_ck, ZEROLEN, "STInitialization", 64)
per entry:
    keydata = HKDF(chainKey, SESSTAG_CONSTANT, "SessionTagKeyGen", 64)
    chainKey = keydata[0:31]; tag_N = keydata[32:40]
symmetric keys (sequential):
    keydata = HKDF(symmKey_chainKey, ZEROLEN, "SymmetricRatchet", 64)
```

Index alignment: tags are numbered from 1 on the wire; symmetric keys and
nonces from 0. The Nth issued entry pairs `tag_N` with `key_{N-1}` /
nonce `N-1`. `EciesTagSet::next_entry()` returns the pre-increment index,
which is exactly the symmetric-key index for that tag.

ES AEAD: nonce = `0x00000000 || LE64(index)`; associated data = the
8-byte session tag.

Elligator2: ephemeral public keys travel as 32-byte representatives;
`to_representative` masks bits 254–255 of the canonical seed before
encoding and `decode_representative` restores the canonical bytes. Both
sides always MixHash the decoded canonical bytes. Seeds that fail the
representable check are rejected typed (`EciesError::ElligatorEncode` /
`EciesError::ElligatorDecode`).

## Manager contract (i2pr-client)

- One paired session per remote X25519 static public key per local
  destination; the pairing is Provisional until Plan 127 binds it to a
  resolved Destination context.
- Inbound ES traffic is admitted through a bounded remove-on-hit tag
  window (look-ahead ceiling 32): replayed tags classify but decrypt
  paths return `EciesSessionError::UnknownSessionTag`.
- Outbound handshakes retain a bounded pending map whose reply-window
  tags are pre-derived at seal time; NSR acceptance is tag-driven and
  requires no caller-supplied remote identity.
- Duplicate bound New Sessions (same ephemeral representative) are
  rejected typed before any handshake work.
- All state is idle-bounded (default 600 s, ceiling 1800 s) and swept by
  `advance_time`.

## Independent vector provenance

The 31 frozen constants in the `fixed_vectors` test module were produced
once by an independent Python reference implementation of the
specification (Python `cryptography` 41.0.7 X25519 +
ChaCha20Poly1305 primitives with a hand-written HKDF-SHA256 and
transcript chain; no code shared with the Rust implementation beyond the
underlying primitives). The generator and its JSON output were written
to `/tmp/opencode/plan126_reference_vectors.py` and
`/tmp/opencode/plan126_vectors.json` during the implementation pass; the
Rust module was transcribed from that output programmatically and then
re-parsed and compared constant-by-constant against the JSON (all 31
matched). The frozen seeds are counter patterns; the ephemeral seeds
(`[0x18]*32`, `[0x1b]*32`) were chosen because their canonical forms are
Elligator2-representable. The constants are never recomputed at test
time; the production primitive must reproduce them or the conformance
tests fail.

Cross-checked references during implementation:

- Current I2P ECIES specification (geti2p.net/en/docs/specs/ecies/),
  proposal 144.
- i2pd 2.60.0 (`ECIES.cpp`, `Garlic.cpp`, `crypto/cpp/Crypto.cpp`) at the
  repository-pinned revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
