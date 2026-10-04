# Plan 324 — Destination signing/encryption policy for canonical SigType and EncType

Status: **registered-prop170-destination-key-policy-blocked-on-plan323**

Classification: crypto integration + identity lifecycle capability.

Hard dependency: Plan 323 closed.

## Objective

Give canonical TunnelManager SigType and EncType real semantics through i2pr’s existing Destination identity owner, without adding obsolete algorithms merely to mimic Java configuration.

## Decision boundary

Proposal 170 exposes SigType and EncType but does not require a router to generate every historical I2P algorithm. Full control-plane conformance requires:
- exact parameter parsing and truthful capability rejection;
- meaningful selection among algorithms i2pr actually supports;
- no silent coercion;
- correct identity-replacement semantics.

Do not accept an option merely because the only current choice is the default unless the selected value is validated and persisted as explicit destination policy and changing it would be handled correctly.

## SigType

Audit current I2P signing-type rules and classify each known type:
- supported for new Destinations;
- verify/read compatibility only;
- offline-only;
- deprecated/not generated;
- pending Red25519 provider.

Ed25519 type 7 remains the baseline.

If Red25519 type 11 becomes available through Plan 325, make it selectable for new encrypted-destination use where the I2P specs permit/recommend it.

Do not add DSA, RSA, or obsolete ECDSA generation solely to make an enum wider unless a current Proposal-required use case demonstrably requires it and a reviewed provider already exists.

## EncType

Make EncType select the actual destination/LeaseSet encryption policy supported by i2pr. Preserve the existing separation between the legacy Destination public-key slot and the active LeaseSet2 X25519 encryption key.

Reject unsupported legacy encryption generation explicitly. Never reinterpret EncType as a cosmetic stored string.

## Identity lifecycle

SigType/EncType changes that alter public identity are ReplaceDestination operations:
- stage complete new key material before commit;
- never rotate a server Destination on a failed transaction;
- surface the identity-changing nature in Get/control state;
- persist new server identity atomically;
- drain old generation under the existing bounded policy;
- no private key bytes in logs/results.

PrivKeyFile/import paths from Plan 323 must validate the imported key types against the selected policy.

## Compatibility tests

Use current I2P key-certificate structures and independent fixtures:
- Ed25519 Destination generation/round trip;
- selected EncType LeaseSet2 publication;
- unsupported type rejection;
- mismatch between persisted/imported key and requested type;
- restart-stable server identity;
- explicit identity rotation on supported type change;
- no-op edit does not rotate.

## Acceptance criteria

Plan 324 passes when SigType and EncType are canonical, typed, meaningful policy selectors for all supported current i2pr algorithms, with exact unsupported dispositions for algorithms the I2P protocol allows but i2pr intentionally does not generate.

Plan 326 additionally requires Plan 325 to pass for Red25519.