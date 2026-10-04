# Plan 326 — Encrypted/blinded LeaseSet2 and Proposal 170 client-authorization modes

Status: **blocked-prop170-encrypted-leaseset-awaiting-qualified-red25519-provider**

Classification: cryptographic protocol + destination/NetDB capability.

Hard dependencies: Plans 323 and 324 closed, and Plan 325 passed with an accepted I2P-compatible Red25519 provider.

## Objective

Implement the full Proposal 170 EncryptLeaseSet / OptionalLookup / LeaseSetClientAuths semantics by adding the missing Encrypted LeaseSet2 publication, lookup and client-authorization capabilities to the canonical i2pr destination/NetDB stack.

## Normative substrate

Use the current I2P Encrypted LeaseSet specification, Proposal 123/client-auth semantics, B32-for-encrypted-LeaseSets specification, common structures and I2CP option semantics.

No behavior is inferred solely from Java UI labels when the protocol specification is normative.

## Required protocol work

### Red25519/blinding

Integrate the Plan 325 provider behind protocol-specific typed wrappers:
- destination signing type 7 or 11 input rules;
- blinded type 11 outer signing;
- daily alpha derivation;
- private/public blinding equivalence;
- blinded destination hash/storage-key derivation;
- 56+ character encrypted B32 address encoding/decoding and checksum.

### Encrypted LS2 structure

Implement DatabaseStore type 5 as a first-class validated record:
- bounded outer/middle/inner layers;
- ChaCha20;
- HKDF-SHA256 domain strings;
- publication/expiry binding;
- outer Red25519 signature;
- inner LS2/MetaLS2 validation;
- canonical replacement/freshness semantics;
- persistence/restart revalidation;
- floodfill opaque-store/serve semantics from ADR 0027 without decrypting at the floodfill.

### Lookup secret / OptionalLookup

Map the canonical OptionalLookup value onto the actual blinded lookup secret/address derivation semantics. Secret ownership must be explicit and redacted/zeroized.

### Client authorization

Implement both schemes:
- DH/X25519 per-client auth with public client keys;
- PSK per-client auth with secret keys.

LeaseSetClientAuths is a bounded list of structured Name/Key entries. Names are UI metadata only; key bytes drive auth.

Randomize emitted authorization-entry order as recommended and cap client count/aggregate crypto work.

### Ten Proposal modes

Map every canonical EncryptLeaseSet string to exact behavior:
- disable;
- encrypted (aes), if this means legacy LS1 AES in the pinned Proposal/Java semantics;
- blinded;
- blinded with lookup password;
- encrypted (psk);
- encrypted with lookup password (psk);
- encrypted with per-user key (psk);
- encrypted with lookup password and per-user key (psk);
- encrypted with per-user key (dh);
- encrypted with lookup password and per-user key (dh).

Research and freeze legacy AES mode separately before implementation; do not mislabel Encrypted LS2 as AES.

If a mode is genuinely legacy but Proposal 170 still requires it, isolate its implementation and provider dependencies from modern LS2 rather than silently rejecting it.

## Router integration

One destination owner remains authoritative. TunnelManager configuration drives that owner; it does not maintain a second LeaseSet stack.

Wire encrypted lookup into:
- local Destination publication;
- client lookup/decryption;
- NetDB type-5 validation/storage;
- floodfill server storage/serving;
- restart/rotation;
- service-tunnel Get status without secret exposure.

## Resource/security constraints

Bound client count, record size, KDF/DH operations, retries, lookup state and daily rotation work. Secret keys/cookies/lookup secrets are non-Clone where feasible, redacted and zeroized.

## Evidence

- official and independent Red25519/blinding vectors;
- encrypted-LS2 encode/decode/sign/decrypt vectors;
- B32 vectors;
- DH and PSK authorized/unauthorized clients;
- wrong secret/wrong client/wrong date;
- all ten canonical mode integration rows;
- restart and daily rollover;
- floodfill opaque handling;
- malformed size/count/crypto corpus;
- external lookup/publication against at least one independent implementation before capability claim.

## Acceptance criteria

Plan 326 closes only when every Proposal LeaseSet mode has its exact implemented semantics and the encrypted-LS2 path works end-to-end through real publication and lookup.

No mode may pass from parser acceptance or inert storage.
