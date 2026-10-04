# Plan 333 — Encrypted LeaseSet2 PSK and DH client authorization

Status: **passed-encrypted-leaseset-client-authorization-psk-and-dh**

Closure: [`plans/closure/i2pcontrol-proposal-170/333-status.md`](../../closure/i2pcontrol-proposal-170/333-status.md)

Classification: cryptographic protocol + secret lifecycle.

Hard dependency: Plan 332 passed.

## Objective

Implement the two standard encrypted-LS2 per-client authorization modes without changing the
no-auth/lookup-secret behavior proved by Plan 332.

## PSK authorization

Implement the exact PSK layer-1 construction:
- bounded non-empty configured client-key set;
- correct client identifier derivation;
- per-generation fresh auth cookie/salt;
- exact HKDF domain labels;
- encrypted cookie record framing;
- randomized emitted entry order where recommended;
- constant-time matching where applicable;
- decryption using an authorized PSK only.

Secrets are typed zeroizing values and are never emitted by I2PControl/Get/rawConfig/logging.

## DH/X25519 authorization

Implement:
- per-client X25519 public keys;
- generation-local ephemeral X25519 keypair;
- shared-secret derivation with all-zero rejection;
- client identifier and encrypted cookie construction;
- exact layer framing and HKDF domains;
- bounded client count and aggregate DH work;
- authorized-client decrypt path using the client private key.

Use the existing reviewed x25519-dalek wrapper; do not duplicate X25519 primitives.

## Secret ownership

Create one narrow destination/client-auth secret owner that distinguishes:
- server-side PSK secret;
- server-side authorized DH public key;
- client-side PSK secret;
- client-side DH private key.

Persistence:
- explicit encrypted/owner-restricted secret storage policy;
- restart-safe;
- no use of one-way password verifiers for secrets that must be replayed;
- atomic with destination generation.

## Control-neutral API

This plan implements protocol owners, not Proposal 170 parsing. Provide typed runtime configuration
consumed by Plan 334.

Names associated with client keys are metadata only and never enter cryptographic derivation unless
the normative spec says so.

## Evidence

For PSK and DH separately:
- one authorized client;
- multiple authorized clients;
- wrong key;
- missing key;
- duplicate identifier collision handling;
- shuffled record order;
- max and max+1 clients;
- tampered cookie/salt/ephemeral key;
- restart recovery;
- daily rollover;
- wrong lookup secret combined with auth;
- cross-implementation publication/decryption against Java and i2pd where supported.

Post-freeze Emissary black-box tests may verify compatible auth behavior but no source may be
consulted.

## Acceptance criteria

Plan 333 passes when both PSK and DH authorized clients can retrieve/decrypt the same service while
unauthorized clients cannot, under bounded resource use and restart-safe secret ownership.

Closure unblocks Plan 334.
