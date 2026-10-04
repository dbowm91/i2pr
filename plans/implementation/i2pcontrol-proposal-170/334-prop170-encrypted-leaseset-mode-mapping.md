# Plan 334 — Canonical Proposal 170 encrypted-LeaseSet mode mapping

Status: **registered-prop170-encrypted-leaseset-mode-mapping-blocked-on-plan333**

Classification: control-plane capability integration.

Hard dependencies: Plans 323, 324, 331, 332, and 333 passed.

## Objective

Map the canonical Proposal 170 TunnelManager fields onto the real Red25519/ELS2 owners and replace
the historical blocked Plan 326 control gap.

No cryptographic primitive is implemented in this plan.

## Canonical fields

Implement the exact canonical behavior for:
- `SigType`;
- `EncryptLeaseSet`;
- `OptionalLookup`;
- `LeaseSetClientAuths`;
- any related `EncType`/destination key policy already frozen by Plan 324.

Use the exact current Proposal types/shapes from Plan 320.

## EncryptLeaseSet mode freeze

Before implementation, map every Proposal string to one protocol behavior and cite the normative
I2P/I2PTunnel meaning.

Expected categories include:
- disabled/ordinary LeaseSet;
- deprecated legacy encrypted/AES mode if Proposal still requires it;
- blinded/no-auth;
- blinded + lookup secret;
- PSK modes;
- PSK + lookup secret;
- per-user PSK modes;
- DH per-user modes;
- DH + lookup secret.

Do not conflate legacy LS1 AES with modern type-5 ELS2/ChaCha20.

If Proposal 170 still exposes a deprecated legacy mode, either:
- implement it in an isolated reviewed legacy owner using existing AES primitives; or
- prove from the current Proposal/reference behavior that explicit unsupported disposition is
  allowed.

No silent alias to modern ELS2.

## OptionalLookup

Map the plaintext control value into the destination's lookup-secret owner:
- validation/bounds before mutation;
- redaction in every response/debug/error;
- atomic generation replacement;
- B33 secret-required flag/address update;
- no empty-secret fallback after a failed secret configuration.

## LeaseSetClientAuths

Map each bounded `Name/Key` entry into the PSK or DH typed runtime owner according to the selected
mode.

Validate the complete list before persistence or runtime mutation.

Never echo key material in Get/rawConfig.

## Transactionality

TunnelManager create/edit:
1. parse the complete candidate;
2. validate cryptographic mode/keys;
3. stage destination generation;
4. stage persistent secret state;
5. reconcile runtime;
6. publish durable generation only at the defined commit point;
7. roll back to the prior generation on failure.

Identity-changing SigType/EncType edits follow Plan 324 semantics.

## Matrix promotion

Recompute the exact canonical type × option matrix.

Every previously blocked Red25519/EncryptLeaseSet/OptionalLookup/LeaseSetClientAuths cell must be
promoted only with:
- a named runtime owner;
- persistence evidence;
- security/redaction evidence;
- live protocol effect.

No inert promotion.

## Evidence

- one control-plane integration case for every exact mode;
- restart for every secret-bearing mode class;
- Get/rawConfig redaction;
- create/edit failure rollback;
- destination/B33 changes on lookup-secret changes;
- PSK and DH multi-user configuration;
- invalid mode/key/list combinations;
- matrix cardinality and zero owner gaps for this branch.

## Acceptance criteria

Plan 334 passes when Proposal 170 can configure every supported encrypted-LeaseSet mode through
the real ELS2 owners and the deep Red25519/LeaseSet cells are no longer parser-only or blocked.

Closure unblocks Plan 335.
