# Plan 111 post-closure audit amendment

- Status: **closed; superseded by Plans 112 and 113**
- Date: 2026-08-15
- Audited implementation commit: `21b5e8a68cd78826c8f7c502455fb6e1ad14c7c1`
- Successor roadmap: `plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`
- Next executable plan: `plans/112-outbound-short-build-pre-delivery-closure.md`

## 1. Purpose

Plan 111 successfully repaired the high-impact cryptographic defects that remained after Plans 109 and 110. Its implementation should be retained.

However, a post-closure audit against the current final I2P Tunnel Creation Specification plus current Java I2P and i2pd source found several remaining local pre-delivery defects and clarified the unresolved inbound creator-key question.

This amendment supersedes the **scope/status interpretation** of `plans/111-status.md` where that file says the complete outbound short-build surface is already ready for external delivery without another local correction.

It does **not** supersede the Plan 111 implementation itself.

## 2. Plan 111 value retained

The following Plan 111 corrections remain authoritative and must not be reopened absent concrete contrary evidence:

```text
Noise null-prologue MixHash          = corrected
request es single-HKDF split         = corrected
record-slot nonce/IV byte 4          = corrected
OBEP garlic reply tag length         = corrected-to-8
explicit per-hop tunnel IDs          = corrected
role decoded from authenticated data = corrected
frozen cryptographic constants       = retained
normal daemon NTCP2                  = disabled-and-unenableable
```

The current CI for implementation commit `21b5e8a...` passed. Plan 112 is a bounded outer-surface correction, not another cryptographic rewrite.

## 3. Findings confirmed after Plan 111

### 3.1 Request plaintext padding

Current i2pr `ShortRequestRecord::encode()` zero-fills the unused area after Mapping.

The final ECIES tunnel-creation specification requires random padding through byte 153.

Current Java I2P master at commit `498488b0d01d9f59efe906424e56ff5e25f58a4d` also fills this region with router RNG output.

Current i2pd at commit `dfcb8a8043c0c689e5681c5ae5da89df5643347e` zero-fills it. Because the final specification is explicit and Java I2P agrees, i2pd's behavior is treated as an implementation divergence, not the rule for i2pr.

Disposition: **Plan 112**.

### 3.2 Reply plaintext padding

Current i2pr `ShortReplyRecord::encode()` leaves unused plaintext reply bytes zero.

The final specification requires random padding through byte 200.

Current Java I2P `BuildResponseRecord.createShort()` fills the reply padding from its RNG.

Current i2pd currently zero-fills it.

Disposition: **Plan 112**.

### 3.3 Direction/role topology

Plan 111 made hop processing role-aware but did not validate that a configured path contains directionally valid remote-hop roles.

Current Java I2P and i2pd agree:

```text
outbound remote hops = Participant ... Participant, OutboundEndpoint
inbound remote hops  = InboundGateway, Participant ... Participant
```

The local creator is the outbound gateway for outbound tunnels and inbound endpoint for inbound tunnels; these local roles are not represented as ordinary remote-hop request records.

Disposition: **Plan 112**.

### 3.4 `HopCryptoContext::ephemeral_public()`

The current accessor copies bytes `0..32` from the 218-byte encrypted request even though the envelope is:

```text
0..16   hop hash prefix
16..48  ephemeral X25519 public key
```

Disposition: delete if unused or correct to `16..48` in **Plan 112**.

### 3.5 Count-prefixed payload contract

The multi-record builder correctly emits:

```text
count byte || count * 218-byte records
```

but high-level action/event docs still describe bare concatenated records, and `deliver_action()` infers count using integer division of total payload length.

This is an API-contract hazard immediately before transport delivery.

Disposition: **Plan 112**.

### 3.6 Frozen-vector provenance

The frozen constants are useful, but `fixed_vectors.rs` names a generator path that is not committed to the repository. The closure record separately says the generator was not committed.

Disposition: add one small Rust-only reproducibility artifact in **Plan 112**. Do not recreate a harness subsystem.

## 4. Inbound creator-key finding corrected

The Plan 111 planning model treated the final-spec creator-ephemeral sentence as if a concrete missing plaintext field merely needed an offset.

The post-closure source audit changes that interpretation.

### Final specification

The short request layout lists fixed fields through byte 55, then Mapping, other option/flag-implied data, and random padding through byte 153. Separately, prose says a creator ephemeral public key is included in plaintext for an inbound tunnel build because the IBGW layer lacks DH.

The final page does not define a fixed byte range or option encoding for that key.

### Current Java I2P

Pinned current master:

`498488b0d01d9f59efe906424e56ff5e25f58a4d`

- real short request constructor does not expose a separate creator-ephemeral field;
- inbound originator/IBEP fake record is:
  `creator hash16 || fresh X25519 pub32 || random remainder`;
- creator retains an integrity hash of the fake.

### Current i2pd

Pinned current source:

`dfcb8a8043c0c689e5681c5ae5da89df5643347e`

- real short request constructor does not expose a separate creator-ephemeral field;
- `ShortPhonyTunnelHopConfig` emits:
  `creator hash16 || fresh X25519 pub32 || random remainder`.

### Correct disposition

Do not consume arbitrary request-padding bytes for an invented creator-key field.

Current i2pr already has an originator-fake primitive closely matching both current reference routers.

Until the standards/reference discrepancy is resolved:

```text
production inbound construction = fail-closed
originator fake primitives       = retained/testable
outbound construction            = corrected independently
```

The dedicated authority is:

`plans/113-inbound-short-build-spec-reference-reconciliation.md`

## 5. Revised Plan 111 status

The accurate current interpretation is:

```text
plan_111                       = core-crypto-correction-landed
plan_111_crypto                = retained
request_padding                = pending-plan112
reply_padding                  = pending-plan112
direction_role_topology        = pending-plan112
hop_context_ephemeral_accessor = pending-plan112
payload_action_event_contract  = pending-plan112
fixed_vector_reproducibility   = pending-plan112
outbound_short_build           = locally-conformant-pre-delivery
inbound_short_build            = reference-compatible-spec-text-discrepancy
outbound_external_delivery     = eligible-for-qualified-checkpoint
inbound_external_delivery      = eligible-for-qualified-checkpoint
normal_daemon_ntcp2            = disabled-and-unenableable
```

This superseded any stronger `passed-final-local-short-build-conformance`
interpretation in the historical closure record. Plans 112 and 113 have now
closed the remaining outer-surface and inbound-semantics work; this amendment
is retained as the audit record and is no longer an active authority.

## 6. New authority sequence

```text
Plan 111 implementation retained
   -> Plan 112 outbound pre-delivery closure        [closed]
   -> Plan 113 inbound spec/reference reconciliation [closed]
      -> narrow qualified external-delivery checkpoint [unblocked; next]
```

Plan 113 does not block outbound delivery after Plan 112.

Full exploratory inbound/outbound acceptance still requires both directions when the roadmap reaches that checkpoint.

## 7. Anti-expansion

This amendment does not authorize:

- another broad Plan 108-111 crypto rewrite;
- NTCP2 activation or generic repair;
- SSU2;
- public-network validation;
- Python harness growth;
- Java/i2pd subprocess orchestration;
- generic I2NP dispatch;
- privileged isolation machinery.

Plans 112 and 113 are closed. The next action is the narrow qualified
external-delivery checkpoint recorded by the Plan 102 amendment closure.
