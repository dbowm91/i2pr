# Plan 324 — Destination signing/encryption policy for canonical SigType and EncType

Status: **in-progress-prop170-destination-key-policy**

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

## Current implementation progress

- Canonical `SigType` now accepts only the supported EdDSA-SHA512/Ed25519 type 7 (`"7"` or `"EDDSA_SHA512_ED25519"`); other names and legacy types fail before allocation. `EncType` accepts only active Standard LeaseSet2 X25519 type 4; this does not reinterpret the separate legacy Destination public-key slot. Canonical values persist in the owned TunnelManager definition and round-trip through `rawConfig`.
- Added `DestinationCryptoPolicy` to the existing `DestinationPolicy` identity owner. It preserves dedicated/shared/persistent/key-reference grouping while making crypto selection typed; shared group construction rejects mixed policies. Before Destination material reaches the product fabric, the manager checks the signing key is type 7 and an active X25519 key is present. The accepted setting is therefore checked at the owner that generates or restores the identity.
- Since type 7/type 4 equal the current defaults, an explicit same-policy edit does not replace the public identity. No other supported algorithm can currently be selected; unsupported requests are rejected before listener, destination, or persistent-key allocation. Red25519 remains separately gated by Plan 325, whose qualification status is blocked.
- The Proposal tunnel matrix now assigns SigType and EncType to the crypto-policy owner for all twelve types. The canonical wire decoder preserves both values as strings; exact policy validation runs before destination or listener allocation. The running TunnelManager product path checks the generated/restored signing type and compares the selected LeaseSet2 encryption type and public key bytes with the active identity key before publication. Persisted canonical definitions retain the values in `rawConfig`; same-policy edits preserve the existing serialized Destination identity.
- Focused evidence: `cargo test --locked -p i2pr-i2pcontrol --test contract plan289_tunnel_request_envelope_rules -- --test-threads=1`, `cargo test --locked -p i2pr-i2pcontrol --lib proposal_tunnel_matrix -- --test-threads=1`, `cargo test --locked -p i2pr-daemon --lib plan293_deep_primitives_rejected_with_named_limitation -- --test-threads=1`, `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets -- --test-threads=1`, and `cargo test --locked -p i2pr-daemon --test i2pcontrol_differential differential_corpus_against_production_composition -- --test-threads=1`.
- Full local floor: formatting, workspace all-target check, workspace all-target tests (`3,775 passed, 35 ignored; 133 suites`), workspace all-target/all-feature Clippy with warnings denied, workspace rustdoc with warnings denied, and workspace doctests passed. Dependency-direction, runtime-boundary, service-tunnel-boundary, I2PControl acceptance-evidence, and exploratory-tunnel evidence guards passed.
- No key-format migration is introduced. Existing persisted identity material remains byte-for-byte stable for the supported type-7/type-4 policy; unsupported imported or requested types fail before allocation. The legacy Destination public-key slot remains distinct from the active LeaseSet2 X25519 key. Secret material is neither added to control results nor logs.
- Closure limitation: only EdDSA-SHA512/Ed25519 SigType 7 and active Standard LeaseSet2 X25519 EncType 4 are currently generated. No alternate supported choice exists, so a different-type identity replacement path cannot be exercised until a qualified provider is available. Red25519 remains gated by Plan 325. LeaseSet encryption/client authorization and outproxy behavior remain owned by Plans 326 and 327.
