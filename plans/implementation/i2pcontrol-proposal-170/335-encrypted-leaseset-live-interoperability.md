# Plan 335 — Live Encrypted LeaseSet interoperability and Plan 326 successor reclosure

Status: **registered-encrypted-ls2-external-reclosure-blocked-on-plan334**

Classification: external interoperability + branch closure.

Hard dependency: Plan 334 passed.

## Objective

Qualify the complete Red25519/Encrypted LeaseSet2 branch against independent implementations and
establish the successor closure for historical blocked Plan 326.

This plan does not claim full Proposal 170; blocked RouterInfo and outproxy branches remain.

## Re-freeze

Record exact:
- i2pr implementation head;
- Red25519/ELS2 spec revisions;
- Proposal 170 revision;
- Java I2P pin;
- i2pd pin;
- Emissary behavioral-test pin;
- Cargo.lock/dependency hashes.

Any production crypto/ELS2 change after freeze invalidates the external run.

## Java I2P lane

Provision a controlled Java router and prove:
- i2pr-published no-auth ELS2 is discoverable/decryptable by Java;
- Java-published ELS2 is discoverable/decryptable by i2pr;
- lookup-secret service in both directions;
- PSK client authorization where Java supports the current mode;
- DH client authorization where Java supports the current mode;
- B33 address interoperability;
- daily rollover/storage-key transition.

## i2pd lane

Using the pinned i2pd reference:
- blinded/B33 address equality where deterministic;
- no-auth encrypted LeaseSet publication/lookup;
- client authorization modes supported by the pinned i2pd;
- Red25519 cross-sign/verify and blinding agreement.

Unsupported i2pd feature rows are classified, not treated as i2pr failures.

## Emissary black-box lane

Without source inspection:
- deterministic alpha/blinded key/storage key/B33 comparisons;
- cross-signature verification;
- no-auth ELS2 exchange;
- lookup-secret exchange;
- PSK/DH exchange where its exposed runtime supports them.

Retain only sanitized fixtures/hashes; never record private/lookup/client-auth secrets.

## Negative/interoperability matrix

Include:
- wrong secret/key/client;
- expired/stale record;
- wrong day;
- malformed B33;
- tampered outer signature;
- tampered encrypted layer;
- wrong storage key;
- incompatible sigtype/enc type;
- restart and rollover.

## Support-floor consequences

On pass:
- update Plan 281/M12 support authority through a new closure/reference note so DatabaseStore type 5
  is no longer deferred solely for lack of Red25519;
- update support.toml and conformance docs with exact qualified ELS2 capabilities;
- record historical Plans 325 and 326 as superseded by the 329–335 successor chain for forward
  architecture, without altering their closure files;
- mark the encrypted-LeaseSet branch complete.

## Acceptance criteria

Plan 335 passes only with bidirectional live interoperability against at least Java I2P and one
additional independent implementation on the overlapping feature set, plus the post-freeze
Emissary black-box differential.

No unexplained cryptographic/protocol mismatch and no unresolved high/medium security finding.

After closure, full Proposal 170 remains blocked only by the current successor work required for
Plan 322's production transit/IPv6 owners and Plan 327's I2P-routed outproxy/secret owner, followed
by a new final conformance-gate plan.
