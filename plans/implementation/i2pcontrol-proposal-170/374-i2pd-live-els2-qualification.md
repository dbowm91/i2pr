# Plan 374 — Stock i2pd bidirectional Encrypted LeaseSet2 qualification

Status: **registered-i2pd-live-els2-qualification-blocked-on-plan373**

Classification: external interoperability + capability evidence.

Hard dependencies:
- Plan 373 passed.
- Proposal 170/346 passed.
- Proposal 170/350 passed.
- Proposal 170/351 passed.

Historical source: Proposal 170/347 stopped before either i2pd direction executed.

## Objective

Close the two i2pd rows of the original four-direction ELS2 matrix with a stock, unmodified i2pd
router and the real i2pr publication/consumer paths.

This is not a signature fixture. Success requires an application payload after a real type-5
DatabaseStore and blinded-key DatabaseLookup.

## Reference freeze

Start from the Plan-347 i2pd 2.61.0 pin:

`PurpleI2P/i2pd@635b013a612ff47278ef02acf8580a28e10e26c5`.

Before execution:
- verify the pin builds unmodified;
- record i2pd version and dependency versions;
- re-read the exact ELS2/type-5 consumer/publisher/config surfaces;
- record the current i2pr SHA and Cargo.lock hash;
- retain ADR 0032 (Proposal 170) transcript policy unchanged.

If a newer i2pd head is also tested, keep the 2.61.0 pin as the stable qualification baseline and
record the newer lane separately.

## Controlled topology

Use an isolated loopback/Ubuntu topology with no public-network dependency.

Required logical roles:
- **R** — i2pr service/client router under qualification, non-floodfill;
- **F** — controlled i2pr floodfill, a distinct process from R;
- **D** — stock i2pd service/client router;
- any required stock relay peers, seeded explicitly.

Do not satisfy a row by:
- inserting a decoded LeaseSet into a consumer;
- sharing an in-process NetDB store between R and F;
- directly invoking the ELS2 resolver with test bytes;
- modifying i2pd source.

Every success row must show actual router-to-router store/lookup traffic.

## Reference driver

Build an integration driver that uses only stock i2pd configuration/public surfaces.

It must be able to:
- create an i2pd encrypted service and obtain its extended B32/B33 address;
- create an i2pd client tunnel to a supplied encrypted-service address;
- configure any lookup secret / PSK / DH key using the stock syntax supported at the pin;
- expose a loopback echo/HTTP application behind the service;
- collect bounded sanitized evidence.

No copied i2pd implementation source belongs in i2pr; configuration snippets and public output are
allowed.

## Mandatory directions

### A. i2pr publisher/service -> i2pd consumer/client

Prove:
1. i2pr builds a type-5 record using the deployed ELS2 signature profile;
2. R sends DatabaseStore to F under the blinded storage key;
3. F admits/stores the record;
4. i2pd issues a blinded-key lookup and F serves type 5;
5. i2pd verifies/decrypts the record;
6. i2pd opens Streaming to the service;
7. an application payload round-trips.

### B. i2pd publisher/service -> i2pr consumer/client

Prove:
1. i2pd publishes a stock type-5 record into F;
2. F admits/stores it without decryption;
3. R resolves the B33 address through the production Plan-351 consumer path;
4. the production lookup uses the daily blinded storage key verbatim;
5. i2pr validates the deployed transcript, authorization layer, and inner LS2;
6. the inner destination is installed under its own destination hash;
7. a service-tunnel client completes an application payload round-trip.

## Capability matrix

No-auth is mandatory in both directions.

Before execution, source-prove the exact i2pd 2.61.0 support for:
- lookup secret;
- PSK client authorization;
- DH/X25519 client authorization.

Every overlapping mode is mandatory in both applicable directions. A mode may be
`reference-not-applicable` only with pinned source/config evidence.

For each auth mode include:
- authorized success;
- missing credential failure;
- wrong credential failure;
- no secret/key in logs/evidence.

## Persistence and rollover

For i2pr-owned services:
- restart R and prove the same unblinded identity survives;
- prove the expected current-day B33 and a fresh type-5 publication;
- reconnect from i2pd.

For daily rotation:
- use deterministic adjacent-day derivation evidence plus at least one live current-day lookup;
- do not wait for wall-clock midnight.

## Negative rows

At minimum:
- strict-only Proposal-146 outer signature presented to stock i2pd: rejected;
- tampered deployed-profile signature: rejected;
- wrong lookup secret;
- wrong PSK/DH credential;
- wrong blinded storage key;
- expired record;
- malformed B33;
- tampered encrypted layer;
- F does not return a type-3/type-7 record for a type-5 blinded lookup by mistake.

## Evidence artifact

Commit a bounded machine-readable artifact and checker containing:
- exact SHAs/versions;
- row IDs;
- publisher/consumer/floodfill roles;
- blinded storage-key hash;
- store/lookup results;
- accepted ELS2 transcript profile;
- application payload success;
- failure classification.

No secret material.

The checker fails if a mandatory row lacks:
`store -> lookup -> decrypt/validate -> application` provenance.

## Acceptance criteria

Plan 374 passes only when both i2pr<->i2pd directions complete the real type-5 path for no-auth and
all overlapping auth modes, with stock unmodified i2pd and no decoded-record injection.

Passing Plan 374 unblocks the i2pd half of Plan 377. It does not itself close Proposal 170/347.
