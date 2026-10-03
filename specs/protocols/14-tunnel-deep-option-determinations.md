# Tunnel deep-option determinations (Plan 293)

Status: Plan 293 determination record. The 30 former
`BlockedPrimitive` cells are now `ExplicitIncompatibility` with the
three class limitations below. Supplying any of the six keys fails
before allocation with the named limitation; omitting the key
selects ordinary i2pr behavior. Plan 295 carries these limitations
into the final support claim.

Reference implementation: eggstack/emissary @ `6885a945`
(the pinned Proposal 170 fork). Citations below name fork paths at
that pin; the fork's own M121/M146/M152/M162 outcomes reached the
same zero-promotion determinations independently.

## A. `sig_type` (12 cells, all types)

Limitation: `dynamic destination SigType has no key-generation
owner (Ed25519-only)`.

- i2pr destination identity is Ed25519-only by architecture
  (ADR 0004, `router-identity-algorithms`). `DestinationIdentity`
  generation builds an Ed25519 seed plus X25519 static
  (`crates/i2pr-client/src/identity.rs`); `DestinationPublic`
  rejects any non-7 signing type (`UnsupportedSigningType`); all
  signature verification funnels through Ed25519-only checks
  (`crates/i2pr-crypto/src/lib.rs`, non-7 rejected with
  `UnsupportedAlgorithm`). No ECDSA/RSA/GOST/DSA provider exists
  in the workspace; `i2pr-crypto` depends only on
  `ed25519-dalek`/`x25519-dalek`/symmetric/HKDF primitives.
- Generating legacy/deprecated algorithms (DSA-SHA1, ECDSA,
  RSA-2048/3072/4096) would violate ADR 0004 and the
  no-locally-invented-crypto rule; no reviewed maintained
  provider is introduced by this plan.
- Accepting only the Ed25519 value would be inert, not support:
  it constrains nothing (no other value is generatable) and
  selects nothing (every destination is Ed25519 regardless). The
  reference implementation reached the same conclusion: M111
  promoted router-native type 7, M121 Outcome C demoted `SigType`
  to blocked for all families because "a singleton domain is
  inert rather than support", and every supplied value
  (`Some("7")` included) fails before allocation
  (`backends/options.rs` M121 anchor + rejection test).
- Any supplied `sig_type` value — Java spelling
  (`EDDSA_SHA512_ED25519`), numeric (`7`), canonical
  (`Ed25519`), legacy (`DSA-SHA1`) — fails with the limitation.
  Omission selects ordinary Ed25519 destinations.

## B. LeaseSet security (16 cells, publishing kinds)

Keys: `encrypt_lease_set`, `leaseset_password`,
`leaseset_blinding_secret`, `leaseset_client_auth`.

Limitation: `encrypted/blinded LeaseSet security and client
authorization have no publication owner`.

- No blinded-publication owner exists: `BLINDED` flag handling is
  deferred throughout (`is_blinded_on_publication`,
  `BlindedPublicationDeferred`, blinded disclosure mapped to
  deferred in `i2pr-netdb`); type-5 `EncryptedLeaseSet` decodes
  to `Deferred` (opaque, bounded) in I2NP and is rejected as a
  LeaseSet variant. No PSK/DH client-authorization verifier, no
  lookup-secret/blinding derivation, and no rotation owner exist
  (`i2pr-proto`, `i2pr-netdb`, `i2pr-client`).
- The reference implementation's ten-mode `EncryptLeaseSet`
  table (`backends/options.rs`, `LEASE_SET_SECURITY_MODES`:
  disable, encrypted (aes), blinded, blinded+lookup, psk,
  psk+lookup, psk per-user, psk lookup+per-user, dh per-user, dh
  lookup+per-user) keeps every row blocked: M157–M160 closed
  neutral primitives with zero promotions, M161 gated legacy
  LS1 AES as valid-but-blocked, and M162 rejects every supplied
  mode before allocation — including explicit `Disable`
  ("explicit disable is rejected before allocation; omit the
  field for ordinary publication").
- i2pr follows the same rule: any supplied value fails with the
  limitation, including `false`/`disable` (accepting explicit
  disable would store an inert affirmation of the default;
  omission already selects ordinary type-3 publication).
- Secret discipline: the three companions stay
  Secret-classified; they are rejected before the definition
  mirror or store, never logged, and never echoed (rejection
  messages name the key, never the value).

## C. `use_outproxy_plugin` (2 cells: httpclient, connectclient)

Limitation: `outproxy provider semantics have no I2P-routed
provider`.

- i2pr has no outproxy capability at all (HTTP/SOCKS paths
  document "no outproxy" explicitly); no provider registry,
  plugin ABI, or outproxy destination routing exists.
- A provider would need a general clearnet networking subsystem
  (fetch path, DNS, egress policy) that the guardrails forbid,
  and the MVP explicitly rejects runtime-loadable in-process
  plugins. A dummy provider or empty registry would be
  infrastructure with zero support value.
- The reference implementation's M146 closed as blocked on the
  same ground ("If there is no real provider and adding one
  would require a general clearnet networking subsystem, M146
  must remain blocked. Do not create a dummy provider to
  promote cells") with zero promotions.
- Any supplied value (`true`, a URL, empty) fails with the
  limitation. Omission selects direct I2P routing (the only
  behavior i2pr provides).

## Rejection semantics (all six keys)

- Whole-request validation precedes mutation: unknown-universe
  keys are rejected by the envelope; known-but-incompatible keys
  fail in `normalize_definition` before the definition mirror,
  the store, listener/session allocation, or task spawn.
- Out-of-mask pairs (e.g. LeaseSet keys on client kinds) still
  reject with the key name and no limitation (no disposition
  applies outside the mask).
- `ReplaceDestination` vs `MutableInPlace` diff classes are
  unaffected: incompatible keys never reach diffing.

## Items carried to Plan 295 (wire-shape divergences found)

These do not change any Plan 293 disposition (every value of
these keys is rejected regardless of shape), but Plan 295's
exact-wire-matrix work must adjudicate them against the pinned
Proposal before any final support claim:

1. `encrypt_lease_set`: i2pr inventory says Boolean; the
   reference validates a ten-mode string enum. If the Proposal
   defines the enum, the inventory type is wrong.
2. `leaseset_client_auth` (+ `leaseset_password`,
   `leaseset_blinding_secret`): i2pr models three split Secret
   strings; the reference models `LeaseSetClientAuths` as an
   array of objects plus an `OptionalLookup` string, which has
   no i2pr inventory entry at all.
3. `sig_type`: i2pr inventory says String-only; the reference
   accepts text-or-integer.
4. `use_outproxy_plugin`: i2pr inventory says String; the
   reference validates boolean. Applicability also differs: the
   reference targets four families
   (httpclient/socks/socksirc/connectclient, M146) while the
   i2pr mask covers two (httpclient/connectclient).
5. `AddressBook` `SetConfig`: the i2pr frozen thirteen
   (`private_book`, `local_book`, `router_book`,
   `published_book`, `subscriptions`, `refresh_interval`,
   `proxy_host`, `proxy_port`, `theme`, `log_file`, `log_level`,
   `lookup_timeout`, `max_entries`) differ as a set from the
   reference M096 thirteen (`subscriptions`,
   `published_addressbook`, `router_addressbook`,
   `local_addressbook`, `private_addressbook`, `etags`,
   `last_modified`, `log`, `update_delay`, `proxy_port`,
   `proxy_host`, `should_publish`, `theme`). Recorded for the
   Plan 294 implementation (which builds against the frozen
   inventory) and Plan 295 adjudication.
