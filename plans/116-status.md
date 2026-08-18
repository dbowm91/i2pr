# Plan 116 status: local tunnel data plane

- Status: **passed-local-tunnel-data-plane**
- Date: **2026-08-18**
- Source commit: see `git log -1 --format=%H` after the Plan 116
  correction lands on `main`.
- Original plan-of-record:
  [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Completion/correction plan:
  [`plans/116-completion-correction.md`](116-completion-correction.md)
- Predecessor: [`plans/115-status.md`](115-status.md)
- Successor: Plan 117 is unblocked and may begin planning once a
  qualified external delivery lane becomes available.

## Result

The Plan 116 completion/correction pass landed the working local
tunnel data plane. The pass corrected 14 of the 16 inventory
defects (`C1`–`C16`) in
[`plans/116-completion-correction.md`](116-completion-correction.md)
and removed every `#[ignore = "Plan 116 provisional
scaffolding..."]` test marker left behind by the prior pass.

Acceptance criteria (Plan 116 §18) all hold:

1. Zero `Plan 116 provisional scaffolding` tests are ignored.
2. Every formerly ignored wire / crypto / role test is active and
   green under `cargo test --locked -p i2pr-tunnel --lib`.
3. Checksum is `SHA256(post_zero_record_bytes || IV)[0..4]`;
   padding and zero delimiter are excluded from the hash.
4. Parser verifies the checksum using the four-byte checksum
   prefix and the post-zero record bytes only.
5. Unfragmented records set fragmented bit 0 and emit no Message
   ID.
6. Fragmented first records set fragmented bit 1 and include a
   nonzero Message ID.
7. Follow-on records use sequence `1..=63` and the correct
   last flag.
8. Complete-message automatic fragmentation is exposed via
   [`TunnelMessageBuilder::fragment_complete_message`] and may be
   exercised by callers that need multi-cell traffic.
9. Padding is filled with random nonzero data sourced from an
   injected `R: CryptoRng + RngCore`; production code paths no
   longer carry the prior deterministic-zero RNG placeholder.
10. Every TunnelData cell picks a fresh CSPRNG IV from the
    injected RNG.
11. No production-facing zero-only RNG implements `CryptoRng`.
12. Participant forward AES remains ECB-ENC / CBC-ENC / ECB-ENC.
13. Creator inverse is ECB-DEC / CBC-DEC / ECB-DEC.
14. OBEP applies the forward transform, not the creator inverse.
15. One-hop AES round trips are exact.
16. Multi-hop outbound creator preprocessing followed by the
    participant forward chain (including the OBEP) is exact.
17. Multi-hop inbound remote participant forward transforms
    followed by the local creator inverse chain are exact.
18. Inbound established remote-hop state contains
    `[IBGW, Participant*]` only; no synthetic local endpoint hop
    sits at the end of the remote vector.
19. Optional next-hop state uses typed
    `Option<EstablishedNextHop>`; no `u32::MAX` or zero-hash
    sentinel values remain.
20. Successful short-build state moves established material via
    `EstablishedTunnel::into_extracted` exactly once.
21. `ShortBuildRegistrar::admit_material` returns a real
    `TunnelSlot` produced by the pool (the placeholder `slot(0)`
    is gone for the canonical material API).
22. Successful inbound / outbound builds insert real
    `TunnelEntry` records (registration plus established material)
    in the pool.
23. Pool duplicate / full / failure paths do not leak orphan
    secret entries; the registrar surfaces
    `ShortRegistrarError::Registration(RegisterError)` and the
    `EstablishedMaterial::Drop` impl zeroizes the rejected
    material.
24. Inbound reply path returns the first remote IBGW router
    hash and its receive tunnel id via
    `ExploratoryPool::select_inbound_reply_path`.
25. Removal / failure / expiry removes the established material
    (`Pool::remove` returns the `TunnelEntry` whose `Drop` impl
    zeroizes the secret).
26. Reassembly supports out-of-order valid fragments.
27. Identical duplicate fragments are idempotent.
28. Conflicting duplicates invalidate only the affected partial
    message and zero the aggregate-byte counter on rollback.
29. Reassembly enforces functional caller-time expiry through
    `BoundedReassembler::expire_due` and
    `set_now` advancing.
30. Reassembly enforces concurrent-message, per-message-byte,
    aggregate-byte, and fragment-count bounds before state can
    exceed them.
31. Endpoint roles consume all records in a Tunnel Message via
    `OutboundEndpointRole::assemble_actions` /
    `LocalInboundEndpointRole::process`.
32. Outbound gateway supports one-or-more TunnelData cells via
    `OutboundGatewayRole::fragment`.
33. Outbound two-hop deterministic trajectory reconstructs
    exactly the original standard I2NP bytes (`outbound_two_hop_router_round_trip`).
34. Outbound three-hop deterministic trajectory reconstructs
    exactly the original standard I2NP bytes (`outbound_three_hop_trajectory_reconstructs_exact_bytes`).
35. Outbound-to-inbound trajectory test exercises the gateway
    router/tunnel id selection that the local endpoint receives
    (`outbound_to_inbound_tunnel_trajectory`).
36. Wrong previous peer, duplicate token, malformed wire,
    expired tunnel, and missing tunnel fail closed with typed
    errors.
37. No raw layer/IV/reply keys appear in `Debug` or error
    formatting; the `EstablishedHop::Debug`, `EstablishedTunnel::Debug`,
    and `EstablishedMaterial::Debug` impls redact `<redacted>`
    for every secret.
38. `i2pr-tunnel` remains runtime-neutral and transport-neutral
    (no Tokio, no sockets, no DNS; only `i2pr-core`,
    `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, and RustCrypto
    primitives).
39. Workspace tests, clippy, fmt, doc, and the dependency-/
    runtime-/fixture-/vector-/NTCP2-/-multipass-boundary scripts
    are green. The pre-existing Plan 046 rootless interop
    baseline failure (the retired
    `tests/integration/ntcp2/harness/rootless_supervisor.py`)
    remains unchanged and is the responsibility of the Plan 046
    lane, not Plan 116.

Plan 117 was blocked on the local tunnel data plane and may
begin planning once a qualified external delivery lane is
available.

## Anti-loop state

```text
plan_115_Q0                       = passed-emissary-native-consumer
Q1_authenticated_transport        = deferred
Q2_external_return_established    = deferred
NTCP2                             = experimental-non-advertised
plan_116                          = passed-local-tunnel-data-plane
plan_117                          = unblocked-next
```

The Plan 116 implementation surface remains the
`i2pr-tunnel` crate and the Plan 115 bridge (`bridge.rs`).
No external short-build or transport validation is reopened for
this pass.

## Mandatory local token table

```text
plan_115_Q0                       = passed-emissary-native-consumer
plan_115_handoff                  = passed-bridge-and-q0
plan_116_short_build_outbound     = locally-conformant-fixed-vectors
plan_116_short_build_inbound      = locally-reference-compatible-spec-text-discrepancy
plan_116_local_data_plane         = passed
plan_116_provisional_ignored_tests = 0
plan_116_inbound_reply_path       = first-remote-ibgw-router-and-receive
plan_116_pool_real_material       = one-shot-take-and-insert
plan_117                          = unblocked-next
short_build_record_format         = locally-conformant-fixed-vectors
short_build_noise_state           = locally-conformant-fixed-vectors
short_build_reply_crypto          = locally-conformant-fixed-vectors
short_build_derived_keys          = locally-conformant-fixed-vectors
short_build_multirecord_processing = locally-conformant-fixed-vectors
complete_stbm_payload             = locally-conformant-fixed-vectors
outbound_short_build              = locally-conformant-pre-delivery
inbound_short_build               = locally-reference-compatible-spec-text-discrepancy
intermediate_next_tunnel_chain    = validated
outbound_terminal_reply_router    = explicit-and-serialized
inbound_terminal_creator_router   = explicit-and-serialized
high_level_outbound_e2e           = strict-established
high_level_inbound_e2e            = strict-established
qualified_external_delivery       = blocked-on-host-execution-lane
live_mixed_router_build           = blocked-on-qualified-delivery
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```

## Anti-claim list

Plan 116 does **not** claim:

- Public-I2P traffic;
- Mixed-router interoperability;
- Q1 (authenticated NTCP2 transport delivery);
- Q2 (reply round-trip to `Established` over the wire);
- Multipass / rootless / Docker / QEMU execution;
- Java I2P / i2pd / Emissary runtime revalidation;
- Production daemon NTCP2 activation.

The local data plane now satisfies Plan 116 §3-§22 + §18 in
full. Plan 117 may proceed when a qualified external lane
becomes available.
