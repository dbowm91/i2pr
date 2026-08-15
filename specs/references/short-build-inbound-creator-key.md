# Inbound short-build creator-key reconciliation

Plan 113 selects the deployed-reference-compatible inbound policy. This note
records the bounded source review; it is evidence, not a replacement
specification.

## Final specification

The authority reviewed was the [ECIES-X25519 Tunnel Creation
Specification](https://i2p.net/en/docs/specs/tunnel-creation-ecies/), observed
with `Updated: 2025-06` and `Accurate for: 0.9.66`. Its 154-byte short request
layout assigns bytes 0–55 to fixed fields, bytes 56 onward to Mapping/options,
and the remainder to random padding. The prose immediately after that table
says that an inbound plaintext includes the creator's ECIES ephemeral public
key because the IBGW layer has no build-record DH.

The prose does not define an offset, Mapping key, option flag, or rule saying
that the key consumes padding. The same page separately requires an inbound
originator fake record containing the creator's 16-byte hash prefix and a real
X25519 public key, and requires creator-side modification detection.

## Pinned implementations

Java I2P master was inspected at commit
`498488b0d01d9f59efe906424e56ff5e25f58a4d` (2026-08-14):

- `router/java/src/net/i2p/data/i2np/BuildRequestRecord.java`, the short
  constructor, serializes fixed fields through byte 55, Mapping, and random
  padding. It has no separate creator-key field.
- `router/java/src/net/i2p/router/tunnel/pool/BuildMessageGenerator.java`,
  `createRecord()` and `createUnencryptedRecord()`, constructs the inbound
  path with the first remote hop as IBGW and the remaining remote hops as
  participants. Its blank/originator record starts with the local 16-byte hash,
  a fresh X25519 public key, and random remainder; its full-record hash is
  retained for modification detection.

i2pd `openssl` was inspected at commit
`dfcb8a8043c0c689e5681c5ae5da89df5643347e` (2026-08-14):

- `libi2pd/TunnelConfig.cpp`,
  `ShortECIESTunnelHopConfig::CreateBuildRequestRecord()`, serializes the
  fixed short fields, Mapping, and padding, with no separate creator-key
  field.
- `ShortPhonyTunnelHopConfig::CreateBuildRequestRecord()` emits the same
  originator-fake shape: `hash16 || fresh X25519 pub32 || random remainder`.
  `TunnelConfig::CreatePhonyHop()` attaches it after the remote inbound path;
  the first remote hop is the gateway and later remote hops are not.

## Bounded history result

The Java history search covered the Proposal 157 rollout commits
`bb19fcdac353aa4d2fa01b6f57dcf9f03183a545` and
`a7d9ca920f35f51840c0ecd599b00ba957720dc8`. The i2pd history search covered
the Proposal 157 short-build commits and
`606881898bdce1b37a8ab865b0508109bf595618` (2025-05-25), which explicitly
added the X25519 public key to the inbound phony record. No inspected history,
comment, issue, or newer specification revision supplied a concrete plaintext
creator-key encoding.

Accordingly, Plan 113 does not reinterpret padding or invent a private field.
The selected policy is `reference-compatible-spec-text-discrepancy`: the real
request remains fixed fields + Mapping + padding; inbound construction emits
exactly one originator fake with the shared deployed shape and verifies its
creator-side integrity hash. This is not a claim of strict final-spec text
conformance for the unresolved creator-key sentence.

The later delivery rule is intentionally out of scope: inbound STBM returns
through an existing outbound tunnel toward the new IBGW, rather than using the
direct outbound STBM path. Plan 113 records that boundary but does not add a
dispatcher or delivery adapter.
