# Plan 116 status: local tunnel data plane

- Status: **implementation-landed-partial-scaffolding**
- Implementation date: **2026-08-18**
- Plan-of-record:
  [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Predecessor: [`plans/115-status.md`](115-status.md)
- Immediate successor:
  [`plans/117-milestone-5-qualified-external-delivery-lane.md`](117-milestone-5-qualified-external-delivery-lane.md)

## Closure result

Plan 116's data-plane scaffolding has been implemented and integrated
into `i2pr-tunnel`, but the full deterministic pair, fragmentation
boundary, and registrar wiring are not yet complete. The crate
compiles cleanly with `cargo check -p i2pr-tunnel` and the broader
workspace passes `cargo check --workspace --all-targets`. Of the
`i2pr-tunnel` lib test suite 182 tests pass and 17 provisional tests
fail because the underlying protocol semantics are still being
completed.

### Delivered in this commit

- `crates/i2pr-tunnel/Cargo.toml` — production `aes` and `cbc`
  dependencies for the AES-256 CBC layer transforms.
- `crates/i2pr-tunnel/src/lib.rs` — module declarations and public
  re-exports for the new data plane modules.
- `crates/i2pr-tunnel/src/identity.rs` — manual `Zeroize`
  implementations for `TunnelId` and `TunnelPeer` (the latter via a
  manual byte loop because `i2pr_proto::Hash` does not impl
  `Zeroize`).
- `crates/i2pr-tunnel/src/data.rs` (new) — `TunnelPayloadHeader`,
  `DeliveryInstruction`, `FragmentDelivery`, `TunnelFragment`,
  `TunnelMessageBuilder` (with bounded fragmentation and
  `fill_nonzero` bounded by a `(idx as u8).wrapping_add(1)` fallback),
  and `TunnelMessageParser` for first/follow-on fragment records.
- `crates/i2pr-tunnel/src/fragment.rs` (new) — bounded
  `BoundedReassembler` with hard ceilings on stored messages and
  bytes.
- `crates/i2pr-tunnel/src/layer.rs` (new) — AES-256 ECB block
  encryptor plus AES-256 CBC encrypt/decrypt wrappers, the
  `TunnelLayerTransform` outbound/inbound inverse transforms, and
  the `DuplicateWindow` bounded exact-match replay window.
- `crates/i2pr-tunnel/src/roles.rs` (new) — runtime-neutral
  outbound/inbound/local role types (`OutboundGatewayRole`,
  `OutboundParticipantRole`, `OutboundEndpointRole`,
  `InboundGatewayRole`, `InboundParticipantRole`,
  `LocalInboundEndpointRole`), `RouterDeliveryAction` envelope, and
  the `OBGWRouterDelivery` carrier.
- `crates/i2pr-tunnel/src/established.rs` (new) —
  `EstablishedHop`, `EstablishedTunnel`, secret-material ownership,
  zeroization-on-drop, and the `EstablishedTunnelError` taxonomy.
- `Cargo.lock` and `Cargo.toml` workspace — `crypto-common = "=0.1.7"`
  pin forces the aes/cbc cipher 0.4.x trait world to compile against
  a single crypto-common version, side-stepping the
  `sha2 0.11.0`/`digest 0.11.3` duplicate crypto-common 0.2.x.

### Pending for closure

- Full 1..=63 fragment generation and reassembly (current builder
  emits a single first fragment for short messages and rejects
  > 965-byte messages).
- Deterministic outbound → OBEP → TunnelGateway → inbound-hop → IBEP
  pair round trip with real IV extraction.
- `EstablishedTunnel` integration with `ShortBuildRegistrar` and the
  `ExploratoryPool` registration path.
- `ExploratoryPool::select_inbound_reply_path` returning the
  first inbound-hop receive tunnel id rather than the registration
  tunnel id.
- The 17 currently failing lib tests in `data::tests`, `layer::tests`,
  and `roles::tests`.

### Build/test posture

```text
cargo check -p i2pr-tunnel        = clean (0 errors, 12 warnings)
cargo check --workspace --all-targets = clean
cargo test -p i2pr-tunnel --lib   = 182 passed; 17 failed (provisional)
cargo test --workspace --lib      = all other crates pass
```

## Anti-loop compliance

- No Emissary, i2pd, or Java short-build work was attempted.
- No NTCP2 correction, NTCP2 activation, SSU2 work, Docker, QEMU,
  Multipass, namespacing, or Python interoperability harness work
  was attempted.
- No public I2P access, live mixed-router tunnels, Q1, Q2, generic
  I2NP router dispatch, garlic message construction, LeaseSet
  management, or streaming layer work was attempted.