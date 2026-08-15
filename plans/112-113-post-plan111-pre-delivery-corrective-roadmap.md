# Plans 112-113: Post-Plan 111 pre-delivery corrective roadmap

- Status: **closed; corrective roadmap complete**
- Date: 2026-08-15
- Parent authority: `plans/102-amendment-exploratory-tunnel-dependency.md`
- Predecessor: `plans/111-status.md`
- Closure records: `plans/112-status.md` and `plans/113-status.md`
- Next executable action: **qualified external-delivery checkpoint**
- External network work authorized by this roadmap: **none**

## 1. Purpose

Plan 111 materially corrected the short-tunnel-build cryptographic core, but a post-closure audit against the current final I2P specification and two current independent router implementations found a small set of remaining pre-delivery issues.

These issues do **not** justify another broad tunnel-build rewrite or another Milestone 3 interoperability harness. They divide cleanly into two scopes:

1. **Plan 112 — outbound pre-delivery closure**
   - fixes deterministic local defects that must be corrected before an outbound STBM is handed to an independent router;
   - is the only mandatory blocker before a narrow **outbound** external-delivery checkpoint.

2. **Plan 113 — inbound standards/reference reconciliation**
   - resolves the remaining discrepancy between the final specification prose and the current Java I2P/i2pd inbound-build implementations;
   - is required before i2pr claims or enables production inbound short-build support;
   - does **not** block Plan 112 or an outbound-only external-delivery checkpoint.

The objective is to preserve forward momentum while keeping conformance claims precise.

## 2. Research result

### 2.1 Current final specification

Normative source:

- I2P Tunnel Creation Specification (ECIES-X25519)
- `https://i2p.net/en/docs/specs/tunnel-creation-ecies/`
- observed metadata: **Updated 2025-06; Accurate for 0.9.66**

The final specification states for short records:

- request plaintext size: 154 bytes;
- request fields through byte 55;
- Mapping beginning at byte 56;
- remaining bytes through 153: **random padding**;
- reply plaintext size: 202 bytes;
- Mapping beginning at byte 0;
- remaining bytes through 200: **random padding**;
- reply byte at 201;
- short encrypted record size: 218 bytes;
- record number in ChaCha IV byte 4;
- build record order randomized;
- recommended minimum record count 4;
- inbound builds require one originator fake containing the creator's correct 16-byte hash prefix and a real X25519 ephemeral public key.

The specification also contains this prose requirement:

> The creator ephemeral public key is ... only included in the plaintext record in an Inbound Tunnel Build message.

However, the 154-byte layout table does not assign that key a fixed offset or option encoding.

### 2.2 Current Java I2P reference

Pinned source tree:

- repository: `i2p/i2p.i2p`
- branch: `master`
- commit: `498488b0d01d9f59efe906424e56ff5e25f58a4d`
- commit date: 2026-08-14

Relevant files:

- `router/java/src/net/i2p/data/i2np/BuildRequestRecord.java`
- `router/java/src/net/i2p/data/i2np/BuildResponseRecord.java`
- `router/java/src/net/i2p/router/tunnel/pool/BuildMessageGenerator.java`

Observed behavior:

- short request records encode the fixed 56-byte prefix, Mapping, then fill the remaining 154-byte plaintext with `ctx.random().nextBytes(...)`;
- short reply records encode Mapping, fill the reply padding using `ctx.random().nextBytes(...)`, place the status byte, then AEAD-encrypt;
- inbound build originator fake records use:
  - bytes `0..16`: creator/router truncated identity hash;
  - bytes `16..48`: a real fresh X25519 public key;
  - remainder: random bytes;
  - creator retains a SHA-256 hash of the fake for modification detection;
- inbound role assignment marks the first remote hop as IBGW;
- outbound role assignment marks the last remote hop as OBEP;
- the short `BuildRequestRecord` constructor does **not** expose or encode a separate creator-ephemeral field beyond the fixed layout + Mapping + random padding.

### 2.3 Current i2pd reference

Pinned source tree:

- repository: `PurpleI2P/i2pd`
- branch used by current source search: `openssl`
- commit: `dfcb8a8043c0c689e5681c5ae5da89df5643347e`
- commit date: 2026-08-14

Relevant files:

- `libi2pd/TunnelConfig.cpp`
- `libi2pd/Tunnel.cpp`
- `libi2pd/TransitTunnel.cpp`

Observed behavior:

- `TunnelConfig` creates inbound paths with the first remote hop as gateway and subsequent remote hops as participants;
- outbound configuration clears the gateway role on the first remote hop and marks the final remote hop as the reply endpoint;
- `ShortPhonyTunnelHopConfig::CreateBuildRequestRecord()` creates the inbound originator fake as:
  - local 16-byte hash prefix;
  - fresh X25519 public key;
  - random remainder;
- no separate creator-ephemeral field is inserted into the real short request plaintext;
- i2pd currently zero-fills unused request/reply plaintext padding rather than randomizing it.

The i2pd zero-padding behavior is a **reference implementation divergence from the final specification**, not authority to retain zero padding in i2pr. Java I2P and the final specification agree that the padding should be random.

### 2.4 Corrected interpretation

The post-Plan-111 audit therefore resolves as follows:

```text
Noise-N correction                   = Plan111 landed
single-HKDF request es               = Plan111 landed
slot nonce / raw ChaCha IV byte 4    = Plan111 landed
OBEP garlic key/tag                  = Plan111 landed
explicit per-hop tunnel IDs          = Plan111 landed
role-aware responder KDF             = Plan111 landed
request random padding               = missing; Plan112
reply random padding                 = missing; Plan112
direction/role topology validation   = missing; Plan112
HopCryptoContext ephemeral accessor  = wrong offset; Plan112
count-prefixed action/event contract = internally working but API/docs inconsistent; Plan112
fixed-vector generator provenance    = stale/non-reproducible from repo; Plan112
inbound public production path       = not coherently gated; Plan112 fail-closed
inbound creator-key spec discrepancy = Plan113
```

## 3. Important correction to the previous diagnosis

Do **not** implement a guessed extra 32-byte creator public key inside the 154-byte request plaintext.

Two current independent implementations agree on the originator-fake structure and do not expose such a field in their real short-request codec. The final specification prose still says the creator ephemeral key is present in inbound plaintext, but does not define a location.

Therefore:

- Plan 112 must make inbound production construction explicitly fail closed rather than claiming it is disabled while lower-level production entry points remain callable;
- Plan 113 owns the standards/reference discrepancy;
- Plan 113 must either resolve the discrepancy from upstream evidence or record an explicit interoperability policy;
- no implementation may silently consume random-padding bytes as a private creator-key field.

## 4. Plan sequencing

```text
Plan 111  core cryptographic correction landed
   |
   +--> Plan 112  outbound short-build pre-delivery closure  [NEXT]
   |       |
   |       +--> narrow outbound external-delivery checkpoint
   |
   +--> Plan 113  inbound spec/reference reconciliation
           |
           +--> required before inbound external delivery
           +--> required before full inbound/outbound exploratory-tunnel acceptance
```

Plan 113 may run after Plan 112 or after the first outbound-only delivery checkpoint. It is not a reason to hold outbound progress hostage.

## 5. Plan 112 closure target

Plan 112 must leave this state:

```text
plan_111_core_crypto              = retained
plan_112                          = passed-outbound-pre-delivery-closure
request_padding                   = random-from-injected-csprng
reply_padding                     = random-from-injected-csprng
outbound_role_topology            = validated
inbound_role_topology             = validated-for-structure
production_inbound_builder        = explicitly-fail-closed-pending-plan113
hop_context_ephemeral_accessor    = corrected-or-removed
stbm_action_payload_contract      = count-prefixed-and-validated
otbrm_event_payload_contract      = count-prefixed-and-validated
fixed_vector_provenance           = reproducible-in-repo
outbound_short_build              = locally-conformant-pre-delivery
outbound_external_delivery        = next-qualified-checkpoint
inbound_short_build               = locally-reference-compatible-spec-text-discrepancy
normal_daemon_ntcp2               = disabled-and-unenableable
```

## 6. Plan 113 closure target

Plan 113 must leave one of two explicit states.

Preferred resolution:

```text
plan_113                          = passed-inbound-reference-reconciliation
inbound_short_build               = locally-defined-and-reference-aligned
originator_fake                   = hash16-x25519pub32-random170-integrity-checked
spec_text_discrepancy             = resolved-or-documented-with-upstream-evidence
inbound_external_delivery         = eligible-for-later-checkpoint
```

Fail-closed resolution:

```text
plan_113                          = closed-inbound-spec-reference-discrepancy-unresolved
inbound_short_build               = disabled
outbound_short_build              = unaffected
outbound_external_delivery        = allowed-after-plan112
```

A fail-closed Plan 113 is acceptable. Inventing a private wire format is not.

## 7. Anti-expansion rules

Neither Plan 112 nor Plan 113 authorizes:

- activation or repair of normal-daemon NTCP2;
- SSU2 implementation;
- a new generic I2NP dispatcher;
- Java/i2pd subprocess orchestration;
- Docker, Multipass, namespaces, root, or privileged networking;
- Python interoperability harness growth;
- public-network validation;
- transit tunnel data-plane implementation;
- client tunnels, streaming, or SAM/I2CP work;
- mixed ElGamal/ECIES tunnel records.

Any need for those surfaces must stop at an explicit seam and be planned separately.

## 8. Evidence hierarchy

When evidence conflicts, use this order:

1. current final I2P specification;
2. current Java I2P reference implementation;
3. current i2pd implementation as independent cross-check;
4. repository-local frozen vectors/reference tests;
5. historical proposals only for rationale.

For the specific inbound creator-key discrepancy, final spec text and two current implementations disagree. Plan 113 must preserve that disagreement explicitly rather than pretending it does not exist.

## 9. Handoff

Plans 112 and 113 are closed. Plan 112 passed the outbound pre-delivery
closure, and Plan 113 selected the deployed-reference-compatible inbound
policy while preserving the explicitly documented final-spec discrepancy.

The next executable action is:

`qualified external-delivery checkpoint`

This checkpoint is unblocked for both locally eligible directions. It is not
Milestone 4B acceptance and must not activate normal-daemon NTCP2 or expand
into a generic interoperability harness.
