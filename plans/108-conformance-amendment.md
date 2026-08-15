# Plan 108 status amendment: short-build protocol conformance reopened

- Status: **active amendment; supersedes Plan 108 protocol-conformance claims**
- Date: 2026-08-15
- Historical implementation commit: `23961c9fed623ccf671a0c5ea958c9f464c84f88`
- Successor authority: `plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`

## Purpose

Plan 108 landed useful local architecture and passed its repository validation suite, but subsequent comparison against the current official I2P Tunnel Creation Specification established that the Plan 108 short-build wire/cryptographic algorithm is not protocol-conformant.

This amendment does **not** revert the Plan 108 implementation. It changes the status interpretation so downstream work does not treat internal round-trip success as I2P interoperability evidence.

## Retained Plan 108 value

These parts remain useful foundations subject to normal corrective refactoring:

- runtime-neutral `i2pr-tunnel` structure;
- bounded short-build state-machine ownership;
- typed failure states;
- success-only `ExploratoryPool` registration;
- corrected I2NP type IDs 23/24/25/26;
- generic HKDF helper in `i2pr-crypto` if its use is separately validated;
- deterministic testing discipline;
- transport/runtime separation;
- fail-closed normal-daemon NTCP2 state.

## Superseded protocol claims

Do not treat these Plan 108 claims as valid protocol evidence:

- Plan 108 short request plaintext field layout;
- low-order role flag encoding;
- layer encryption type `0x05`;
- custom millisecond time/expiration wire fields;
- custom request-key seed and nonce derivation;
- custom `ECIES-X25519-Build-Session-v1` KDF path;
- request envelope `ephemeral || nonce || AEAD body`;
- empty request AEAD associated data;
- fresh reply X25519 exchange;
- Plan 108 reply plaintext fields/response code `1`;
- concatenated-record message representation as a complete STBM payload;
- creator/responder self-round-trip tests as independent conformance proof.

The current official specification instead requires the exact layouts, Noise-N transcript, derived reply/layer keys, randomized records, raw-ChaCha20 preprocessing, and one-byte record count described in Plans 109 and 110.

## Current authoritative state

```text
plan_103                         = passed
plan_104                         = passed
plan_105                         = passed
plan_106                         = passed-local-bootstrap-integration
plan_107                         = passed-exploratory-substrate
plan_108                         = implementation-landed-protocol-conformance-reopened
plan_109                         = ready-for-implementation
plan_110                         = blocked-on-plan109
exploratory_tunnel_substrate     = implemented
short_build_state_machine        = architecture-retained-needs-protocol-correction
short_build_request_wire         = nonconformant-plan108
short_build_noise_state          = nonconformant-plan108
short_build_reply_wire_crypto    = nonconformant-plan108
short_build_multirecord          = incomplete-plan108
external_build_delivery          = unavailable
live_mixed_router_build          = blocked-do-not-attempt
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
```

## Required sequence

```text
Plan 109
  exact single-record request/reply wire format
  + literal Noise-N transcript/KDF
  + derived key state
  + independent fixtures

then

Plan 110
  randomized slots/fake records
  + raw ChaCha20 multi-record preprocessing
  + exact STBM/OTBRM payload framing
  + independent multi-hop local conformance closure

then

separate narrow external-delivery checkpoint
```

Do not skip directly from Plan 108 to live mixed-router validation.

## Authority precedence

For the current short-build line, use:

```text
Plans 109-110 corrective roadmap
 -> this Plan 108 conformance amendment
 -> Plan 109 / Plan 110 active child plan
 -> historical plans/108-status.md implementation record
 -> original Plan 108 plan
 -> Plan 107
```

The next executable plan is **Plan 109**.