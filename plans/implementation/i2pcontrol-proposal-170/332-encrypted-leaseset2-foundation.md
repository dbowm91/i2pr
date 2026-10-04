# Plan 332 — Type-5 Encrypted LeaseSet2 foundation, blinding lookup, and B33

Status: **registered-encrypted-ls2-foundation-blocked-on-plan331**

Classification: cryptographic protocol + NetDB/destination capability.

Hard dependencies: Plan 331 passed; Plan 324 destination key policy remains passed.

## Objective

Add the no-client-auth encrypted LeaseSet2 floor required for real type-5 DatabaseStore support:
blinded publication, validated storage/serving, lookup/decryption, optional lookup secret, and
canonical encrypted-service B33 addressing.

This plan does not implement PSK or DH client authorization; Plan 333 owns those.

## Required protocol model

Add first-class protocol structures for DatabaseStore type 5 with exact bounds:
- blinded public key sigtype (type 11);
- blinded public key;
- published/expires/flags;
- optional offline-signature block if supported by the frozen spec floor;
- encrypted payload length;
- encrypted outer data;
- final Red25519/transient signature.

Type 5 must not reuse the ordinary LeaseSet2 header shape.

## Blinding lifecycle

One destination owner derives:
- current UTC-day alpha;
- blinded public/private key;
- DHT storage hash;
- next-day material where precomputation is useful.

Rollover:
- atomic at UTC day boundary;
- no mixed old/new alpha, signing key, storage key or address;
- restart-safe;
- bounded precomputation;
- old records age out under ordinary NetDB policy.

Optional lookup secret contributes exactly as specified and never falls back silently to the
no-secret derivation.

## No-auth encrypted layers

Implement the no-client-auth encrypted LS2 form from the normative spec:
- credential/subcredential;
- exact HKDF domains;
- ChaCha20 layer encryption/decryption;
- secure random salts;
- exact size ceilings;
- inner LeaseSet2 validation after decrypt;
- top-level/inner timestamp and expiration consistency checks.

Use existing i2pr X25519/LS2 owners where applicable; do not create a second destination stack.

## B33 / blinded address

Implement canonical encrypted-service B32/B33 encoding and decoding:
- 35-byte decoded form;
- flags;
- one/two-byte sigtype handling required by the current spec;
- CRC/checksum rules;
- secret/auth-required flag semantics;
- exact suffix/length/canonical-bit validation.

Ordinary 52-character B32 remains unchanged.

## NetDB integration

Update the M12 record floor only after validation tests pass:
- parse/validate type 5;
- store under blinded storage key;
- floodfill stores/serves opaque encrypted records without decryption;
- client lookup derives the correct daily key;
- replacement/freshness rules are explicit;
- persistence, if enabled, revalidates on load.

Do not claim floodfill capability beyond currently qualified M12 authority.

## Destination publication/lookup

Local destinations can:
- construct and sign no-auth ELS2;
- publish it through the existing NetDB path;
- resolve a blinded address;
- fetch/decrypt the record;
- feed the decrypted inner LS2 into the existing streaming/destination path.

## Security and resource bounds

Bound:
- encrypted record length;
- inner LS2 length;
- KDF operations;
- lookup retries;
- daily-key cache cardinality;
- B33 input;
- XML/strings not applicable here;
- secrets and salts.

No secret in logs, metrics or Debug.

## Evidence

- official ELS2 vectors if available;
- independently generated Java/i2pd fixtures;
- Red25519/B33 deterministic fixtures;
- wrong day, wrong secret, tampered ciphertext/signature;
- malformed lengths/flags/sigtypes;
- publication -> NetDB -> lookup -> decrypt -> streaming-resolution local integration;
- restart and UTC rollover;
- floodfill opaque-store behavior.

Emissary remains black-box only and may be used after the Plan 332 implementation commit is frozen
for no-auth publication/lookup comparison.

## Acceptance criteria

Plan 332 passes when DatabaseStore type 5 is a real validated record and no-auth/lookup-secret
encrypted services work end-to-end.

Closure unblocks Plan 333.
