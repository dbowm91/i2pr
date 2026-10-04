# Proposal 170 `EncryptLeaseSet` mode mapping — Plan 334 normative freeze

Status: **frozen 2026-10-04** (Plan 334, before any implementation change).

This document freezes what each of Proposal 170's ten `EncryptLeaseSet` values
means in i2pr, and therefore what a control surface may do with it. It is written
before the control-plane changes so that the mapping can be reviewed on its own,
and so that a later implementation cannot quietly widen or narrow it.

## 1. The pinned source

| Item | Value |
|---|---|
| Document | I2P Proposal 170, "I2PControl Expansion", author Nick2k4 |
| Status | Open |
| Created / last updated | 2026-05-20 / 2026-05-20 |
| Retrieved | 2026-10-04, read-only |
| Source form | `https://i2p.net/proposals/170-i2pcontrol-expansion.txt` (19 010 bytes) |
| SHA-256 | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |

The hash was computed locally from the retrieved bytes. It matches the value
independently recorded in the reference project's M161 gate closure, so the text
has not changed between that read and this one.

The normative LeaseSet option block, verbatim:

```text
- `EncryptLeaseSet` - one of:
 - `disable`
 - `encrypted (aes)`
 - `blinded`
 - `blinded with lookup password`
 - `encrypted (psk)`
 - `encrypted with lookup password (psk)`
 - `encrypted with per-user key (psk)`
 - `encrypted with lookup password and per-user key (psk)`
 - `encrypted with per-user key (dh)`
 - `encrypted with lookup password and per-user key (dh)`
- `OptionalLookup`
- `LeaseSetClientAuths`
```

## 2. The central normative finding: the Proposal defines no modes

**Proposal 170 names the ten values and nothing else.** It supplies no per-mode
table, no property assignment, no wire type, no default, and no precedence rule
between `EncryptLeaseSet`, `OptionalLookup`, and `LeaseSetClientAuths`. Read
directly from the pinned text, the option block above is the entire specification
of these three parameters.

Two consequences follow, and they govern everything below.

1. **Any mapping i2pr implements is a derivation, not a quotation.** A conforming
   implementation is one whose mapping is defensible against the I2P
   specifications the Proposal defers to, not one that matches some other
   implementation's choice. Where the Proposal is silent and the referenced
   specifications are also silent, this document says so rather than inventing a
   rule.
2. **No mode may be aliased to another.** Because the Proposal supplies no
   semantics to inherit, an implementation that quietly treats one value as
   another is not being permissive, it is inventing contract. Every value below
   is either honoured exactly or refused explicitly.

The Proposal's own Compatibility section requires that existing applications
"continue to work without modification". The Implementation section records that
i2pd implements only the parameters marked "(adopted from i2pd)" and that
"Unmarked parts of this proposal are not implemented in i2pd" — so the
LeaseSet security block is unimplemented in the two production implementations
and there is no de-facto behavior to match.

## 3. Where the meaning comes from

Each mode's semantics are derived from the Encrypted LeaseSet specification
(`https://geti2p.net/en/docs/specs/encryptedleaseset`, pinned and implemented by
Plans 329–333) and the common-structures specification. Those define exactly two
modern record shapes and exactly two authorization schemes:

- **DatabaseStore type 5** — the modern encrypted LeaseSet2 (ELS2) record, whose
  layer-1 flags byte carries `0x00` (no authorization), `0x01`
  (Diffie-Hellman), or `0x03` (pre-shared key).
- **A lookup secret** — the optional password that perturbs the daily blinded
  key and therefore the DHT storage key. Present or absent is a real, distinct
  behavior, not a label.
- **Legacy LS1 with AES** — the historical encrypted LeaseSet, `i2cp.leaseSetType`
  OLD, whose encryption uses a 256-byte key and AES. The Encrypted LeaseSet
  specification records that ChaCha20 "was selected over AES" and that the
  destination public key is "currently unused except for the IV for LeaseSet
  encryption, which is deprecated". i2pd's own LeaseSet table marks type 1
  (OLD) as deprecated.

The three axes that the ten values combine are therefore exactly:

| Axis | Values |
|---|---|
| Record shape | ordinary LeaseSet2 (no encryption), modern ELS2 type 5, legacy LS1/AES |
| Lookup secret | absent, present |
| Client authorization | none, pre-shared key, Diffie-Hellman per-user key |

## 4. The ten frozen mappings

Each row names the exact protocol behavior. "Refused" means the value is
accepted as a syntactically valid Proposal value and then rejected with a typed,
specific error — never a generic failure, never a fallback, and never a silent
substitution of a different mode.

| # | Proposal value | i2pr behavior | Owner |
|---|---|---|---|
| 1 | `disable` | **Apply.** Publish an ordinary, unencrypted LeaseSet2 under the service's unblinded destination. No type-5 record, no lookup secret, no authorization. | existing ordinary publication path |
| 2 | `encrypted (aes)` | **Refused.** Legacy LeaseSet1 over AES is not implemented and is not being resurrected. The error names the deprecated path explicitly so an operator can act on it. See §5. | — |
| 3 | `blinded` | **Apply.** Publish a type-5 ELS2 record with **no** authorization block and **no** lookup secret. | `EncryptedLeaseSet2Publisher::build_database_store` (Plan 332) |
| 4 | `blinded with lookup password` | **Apply.** Type-5 ELS2, no authorization, **with** a lookup secret. The address carries `B32_FLAG_REQUIRES_BLINDING_SECRET`. | Plan 332 publisher + the lookup-secret owner (§6) |
| 5 | `encrypted (psk)` | **Apply.** Type-5 ELS2 with a pre-shared-key authorization block and **no** lookup secret. The address carries `B32_FLAG_REQUIRES_CLIENT_KEY`. | `build_authorized_database_store` with `Els2AuthorizationServerConfig::psk` (Plan 333) |
| 6 | `encrypted with lookup password (psk)` | **Apply.** As mode 5 **plus** a lookup secret; the address carries both flags. | mode 5 + §6 |
| 7 | `encrypted with per-user key (psk)` | **Apply.** As mode 5. The Proposal does not distinguish this string from mode 5, and neither do the I2P specifications — see §5.2. | mode 5 |
| 8 | `encrypted with lookup password and per-user key (psk)` | **Apply.** As modes 6 and 7. | mode 6 |
| 9 | `encrypted with per-user key (dh)` | **Apply.** Type-5 ELS2 with a Diffie-Hellman authorization block and no lookup secret. The address carries `B32_FLAG_REQUIRES_CLIENT_KEY`. | `build_authorized_database_store` with `Els2AuthorizationServerConfig::dh` (Plan 333) |
| 10 | `encrypted with lookup password and per-user key (dh)` | **Apply.** As mode 9 **plus** a lookup secret. | mode 9 + §6 |

Every "Apply" row names a Plan 332 or Plan 333 owner that already exists, is
frozen, and has been proven byte-identical to an independent implementation. Plan
334 adds a mapping and a control surface. It adds no cryptography.

## 5. The two dispositions that need justification

### 5.1 `encrypted (aes)` is refused, and refusing it is conforming

Plan 334 requires either an isolated reviewed legacy owner or proof that an
explicit unsupported disposition is allowed. The proof has three independent legs,
none of which depends on another implementation's choice.

1. **The Proposal does not define the mode.** Section 2 above: the value is a
   bare string. There is no contract to implement, and no property to satisfy.
2. **The modern format has no legacy path.** The Encrypted LeaseSet
   specification is a three-layer ChaCha20 construction; it states that ChaCha20
   "was selected over AES" and contains no LS1/AES path. The reference
   implementations agree on the mapping: Java I2P sets its legacy
   `encryptLeaseSet=true` property only for the legacy AES constant, and selects
   `leaseSetType=5` for every modern mode. There is therefore no modern
   interpretation of this string to alias it to.
3. **The legacy path is deprecated upstream.** The common-structures
   specification marks the LeaseSet encryption IV as deprecated; i2pd's table
   marks LeaseSet type 1 (OLD) deprecated; Proposal 121 (Encrypted LeaseSet) was
   **Rejected** and superseded.

Implementing a deprecated AES LeaseSet1 subsystem — wire format, DatabaseStore
type, floodfill store and serve, verification, keyring distribution — is
disproportionate for a protocol the ecosystem has already moved past, and
implementing it badly is worse than not having it. The refused disposition is
explicit, typed, and specific; it is not a generic "unsupported".

**What this does not claim.** i2pr does not claim to interoperate with a legacy
AES LeaseSet1 publisher, and it must never emit one. A floodfill receiving a type
`1` record continues to reject it, unchanged.

### 5.2 `encrypted (psk)` and `encrypted with per-user key (psk)` are the same mode

The Proposal lists both. The I2P specifications define exactly one
pre-shared-key authorization scheme and exactly one Diffie-Hellman scheme, and
neither distinguishes "a shared key" from "a key per user" as separate wire
modes — the per-user distinction is expressed by *how many* entries the
authorization block carries, not by which of the two strings was used.

The honest mapping is therefore that modes 5/7 and 9/10 are the same protocol
behavior, reached by two spellings. What a control surface does with the
distinction is a user-interface matter, not a protocol one: both spellings build
the same block, and the number of configured clients is what varies.

**Why this is not an alias.** An alias would be substituting a *different*
behavior for a value. This is the same behavior under two names, and it is
recorded rather than hidden so that an operator who types the longer spelling
gets an answer they can reason about. i2pr reports which spelling it received and
what it did with it.

## 6. `OptionalLookup` and `LeaseSetClientAuths`

The Proposal names both parameters and gives neither a type nor a schema. What
can be said:

- **`OptionalLookup`** is the lookup secret. In i2pr it is a `String` supplied
  when a mode requires one (modes 4, 6, 8, 10) and refused when one does not
  (modes 1, 2, 3, 5, 7, 9). Supplying it for a mode that does not use it is an
  error, not a no-op: silently ignoring a supplied secret would leave an operator
  believing a service is protected by one when it is not.
- **`LeaseSetClientAuths`** is the per-client authorization set. In i2pr it maps
  onto `Els2AuthorizationServerConfig`, which distinguishes
  `Psk(Vec<PskClientKey>)` from `Dh(Vec<AuthClientPublicKey>)` at construction.
  The mode selects the variant; a caller cannot supply PSK entries to a
  Diffie-Hellman mode or vice versa.

Neither parameter may be supplied with `disable` (mode 1) or with a mode that
does not consume it. Both are refused with a message naming the mode that was
selected.

**Secrets never round-trip.** The `get` action's documented response contains a
`rawConfig` object. No PSK, no Diffie-Hellman private key, and no lookup secret
may appear in any `get` or `rawConfig` response. The control surface reports
*whether* a secret is configured and its length, and never its bytes. This is the
same rule the ELS2 secret types already enforce by having no `Display`, no
`Debug` that reveals, and no serializer.

## 7. What a control surface must do with a refused or invalid value

- A value outside the ten is rejected by name, listing the ten. It is never
  case-folded, trimmed into a match, or fuzzy-matched: a mode name is an
  identifier, and the Proposal's strings contain parentheses and spaces that make
  normalization a guess.
- Mode 2 is rejected with a message that names the deprecation, so the failure is
  actionable rather than mysterious.
- A mode that needs `OptionalLookup` or `LeaseSetClientAuths` and does not get
  them is rejected, and so is the reverse.
- `create` and `edit` are transactional in these fields: a configuration whose
  security block does not fully validate leaves the stored configuration exactly
  as it was. A tunnel is never left half-converted between an ordinary and an
  encrypted LeaseSet, because that would publish an address nobody can read.

## 8. Interaction with the existing `SigType` and `EncType` options

Plan 293/324 already bound `SigType` and `EncType` to a typed policy. An
encrypted-LeaseSet mode composes with them rather than replacing them:

- Modes 3–10 are type-5 ELS2 and therefore require the blinded service identity
  (Red25519, unblinded type 7, blinded type 11) established in Plan 332. A mode
  that requires this cannot be combined with a conflicting `SigType`.
- `EncType` continues to select the LeaseSet *encryption key* carried inside the
  LeaseSet2 for client ECIES sessions. It is orthogonal to the encrypted-LeaseSet
  mode and is not replaced by it.
- The deep option matrix in `proposal_tunnel_matrix.rs` must be recomputed against
  this mapping, not hand-edited. A cell may move from `blocked` to `apply` only
  where the corresponding mode has a real owner in this document.

## 9. Claims this document does not make

1. **It is not an interoperability claim.** Nothing here has been exercised
   against a live network. The type-11 signature transcript divergence recorded in
   Plan 336 means an i2pr-signed type-5 record is currently unverifiable by i2pd
   and Java I2P regardless of the mode.
2. **It does not make any mode reachable from a shipped configuration.** A
   control surface exists; whether it is enabled is a separate, non-advertised
   question governed by `specs/support.toml`.
3. **It does not resolve the `encrypted (psk)` naming redundancy against other
   implementations.** It records i2pr's reading, and the redundancy is a Proposal
   gap worth raising upstream rather than a defect to hide.
