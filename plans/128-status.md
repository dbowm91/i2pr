# Plan 128 — Status (closed)

```text
plan_123 = passed-corrected-streaming-wire-local
plan_128 = passed-streaming-wire-protocol-corrective-closure
next = plans/130-m6-final-wire-runtime-corrective-closure.md (closed)
```

Plan 130 note: the Plan 128 packet codec is unchanged and retained.
Plan 130 corrected the runtime semantics around it — application
sequence numbering starts at 1 post-SYN, `ackThrough == 0` is a valid
cumulative acknowledgement driven by packet flags rather than numeric
zero, plain-ACK/simple-ACK forms follow the reference contract, NACK
feedback follows `MessageInputStream.updateAcks`, and delayed ACKs use
the 750 ms reference default through a synchronous `poll_acks` poll.
See [`plans/130-status.md`](130-status.md) and the Plan 130 addendum in
`specs/references/streaming-packet-wire.md`.

## Scope delivered

Plan 128 made the `i2pr-proto::streaming` packet wire format and the
`i2pr-client::streaming` control packets match the current I2P
Streaming specification. Normative provenance lives in
`specs/references/streaming-packet-wire.md`.

### `crates/i2pr-proto/src/streaming/packet.rs` (rewritten)

- **Normative flag map** (§1): every flag constant now matches the
  current I2P bit assignment — `SYNCHRONIZE 0x0001`, `CLOSE 0x0002`,
  `RESET 0x0004`, `SIGNATURE_INCLUDED 0x0008`,
  `SIGNATURE_REQUESTED 0x0010`, `FROM_INCLUDED 0x0020`,
  `DELAY_REQUESTED 0x0040`, `MAX_PACKET_SIZE_INCLUDED 0x0080`,
  `PROFILE_INTERACTIVE 0x0100`, `ECHO 0x0200`, `NO_ACK 0x0400`,
  `OFFLINE_SIGNATURE 0x0800`, reserved mask `0xF000`. Policy sets:
  `INITIAL_SYN_FLAGS = 0x04A9`, `SYN_RESPONSE_FLAGS = 0x00A9`,
  `CLOSE_FLAGS = 0x000A`, `RESET_FLAGS = 0x000C`.
- **No option TLVs** (§2): the invented
  `STREAMING_OPTION_MAX_PACKET_SIZE`/`STREAMING_OPTION_SIGNATURE`
  type/length records and their constants are removed; the decoder
  rejects unparsed trailing option bytes fail-closed.
- **Flag-driven option codec** (§3): `StreamingOptions` (semantic
  values only), `StreamingOptionDecodeContext` (retained peer signing
  key), `SignatureLocation`. The encoder writes DELAY → FROM → MAX →
  SIGNATURE in normative order; the decoder parses per flags with
  typed `UnsupportedOfflineSignature` rejection before any later
  field could be misparsed.
- **Variable-length signatures** (§4): signature length is inferred
  from the FROM Destination signing key, or from the retained peer
  key through the decode context on established connections. No
  hard-coded 64-byte assumption remains in structural decoding.
- **Correct preimage** (§5): outbound construction encodes the
  complete packet once with a zeroed placeholder of the exact
  key-derived length (`StreamingOptions::encode_with_placeholder`),
  signs those already-zeroed bytes directly, then patches only the
  signature bytes (`install_packet_signature`). The prior
  sign-shorter/add-TLV/sign-again pattern is gone.
- **MAX_PACKET_SIZE semantics** (§6): the option is a 2-byte
  big-endian integer bounding the Streaming payload only;
  `DEFAULT_ADVERTISED_MAX_PAYLOAD = 1730`;
  `MAX_STREAMING_PAYLOAD_BYTES = 1730` is no longer defined as the
  packet ceiling minus the header; `MAX_STREAMING_PACKET_BYTES` is a
  checked sum of independent header/NACK/option/payload bounds.
- **SYN policy** (§7/§8): initial SYN emits flags `0x04A9` with eight
  replay NACK words carrying the remote Destination hash and the
  signature covering them; SYN response emits `0x00A9` with zero
  NACKs, no NO_ACK, and valid `ackThrough`.
  `validate_syn_policy` is split into `validate_initial_syn`
  (replay binding required) and `validate_syn_response` (binding not
  required). The old validator's wrong-side signature-length check
  (receiver key instead of sender key) is removed.
- Two-phase decode: `peek_streaming_header` routes before full strict
  decoding so established-connection control packets can supply the
  peer-key decode context.
- Legacy aliases `MAX_STREAMING_HEADER_BYTES` /
  `MAX_STEAMING_NACK_COUNT` are removed.

### `crates/i2pr-client/src/streaming/`

- `manager.rs`: all four signed builders (initial SYN, SYN response,
  CLOSE, RESET) go through one `build_signed_packet` helper using the
  placeholder + direct-sign + patch flow. CLOSE/RESET emit `0x000A` /
  `0x000C` with the raw final signature and no FROM. Inbound CLOSE /
  RESET require `SIGNATURE_INCLUDED` (`CloseMissingSignature` /
  `ResetMissingSignature`) and verify against the **peer** signing key
  retained on the connection — fixing the previous defect where RESET
  was verified against the local destination key. Unknown standalone
  signed control fails closed at routing or decode.
  `connect()` takes the advertised maximum payload; negotiation is
  `min(local advertised, remote advertised)` recorded from both SYN
  paths.
- `connection.rs`: connections retain `peer_signing_key` and both
  advertised payload maxima; `transition_established` negotiates
  against the locally advertised value.

### Tests

- `crates/i2pr-proto/tests/plan128_wire.rs` (§11): numeric pinning of
  every flag constant and policy set; exact initial-SYN wire layout
  including `06 c2` MAX bytes, eight NACK words equal to the receiver
  hash, raw final-signature placement, no TLV markers anywhere in the
  option region; zero-NACK/no-NO_ACK response shape; DELAY-before-FROM
  ordering; trailing-option-garbage rejection; header peek; preimage
  differing from wire bytes only by the zeroed signature; placeholder
  install failing closed on a nonzero tail.
- `crates/i2pr-client/tests/plan128_trajectory.rs` (§12/§9/§6):
  manager handshake A→B→A with exact stream-id ownership on ordinary
  data packets in both directions; originator stays
  `OutboundSynSent` until the valid response; CLOSE `0x000A` / RESET
  `0x000C` shapes verified by the receiver against the retained peer
  identity; corrupted signature byte fails closed; unsigned RESET
  fails closed; unknown-stream signed control fails closed; negotiated
  max = min of intentionally different advertisements (1200/2000 and
  3000/1500).
- Existing `plan123_trajectory.rs` / `plan125_trajectory.rs` updated
  to the corrected API; the Plan 125 RFC 1952 gzip tests remain green
  unchanged (`payload.rs` was not modified).

## Explicitly out of scope

No destination-tunnel, SAM, external transport, or live-network work.
Streaming interoperability is not claimed; Plan 129 proves the
corrected packets over the complete destination stack.

## Validation

```text
cargo +1.95.0 fmt --all --check                          pass
cargo +1.95.0 check --locked --workspace --all-targets   pass
cargo +1.95.0 test  --locked --workspace                 pass (49 suites, 0 failed)
cargo +1.95.0 clippy --locked --workspace --all-targets \
  --all-features -- -D warnings                          pass
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --workspace --no-deps  pass
bash scripts/check-dependency-direction.sh               pass
bash scripts/check-runtime-boundaries.sh                 pass
```

## Final gate handoff (Plan 129)

The Plan 129 integrated Milestone 6 local-product gate passed on
2026-08-25 over this wire-corrected surface; `plan_123` stays
`passed-corrected-streaming-wire-local` and Plan 125 becomes
`superseded-by-final-corrective-closure`. See
[`plans/129-status.md`](129-status.md).

```text
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = passed-milestone6-integrated-local-product-gate
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```
