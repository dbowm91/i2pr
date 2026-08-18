# Plan 116 status: local tunnel data plane

- Status: **correction-required-local-data-plane-not-closed**
- Date: **2026-08-18**
- Original plan-of-record:
  [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- Active completion/correction plan:
  [`plans/116-completion-correction.md`](116-completion-correction.md)
- Predecessor: [`plans/115-status.md`](115-status.md)
- Plan 117: **blocked until Plan 116 passes the local completion criteria**.

## Current result

The first Plan 116 implementation pass landed useful runtime-neutral scaffolding but did **not** close the local tunnel data plane.

Current implementation floor before the correction plan:

```text
91d3a8569ee20d71ab7a4ae27b6c54a1e5009429
```

Useful surfaces now exist in:

```text
crates/i2pr-tunnel/src/established.rs
crates/i2pr-tunnel/src/data.rs
crates/i2pr-tunnel/src/fragment.rs
crates/i2pr-tunnel/src/layer.rs
crates/i2pr-tunnel/src/roles.rs
```

The scaffold also promoted RustCrypto AES/CBC support into the production tunnel crate.

These modules should be corrected and completed rather than replaced wholesale.

## Why Plan 116 is reopened

The implementation pass reported 182 passing tunnel tests and 17 failing provisional tests, then committed those 17 tests as ignored. Those ignored tests cover core Plan 116 wire, AES, and role behavior and therefore cannot serve as a closure state.

In addition, repository review localized concrete defects/incompleteness:

```text
checksum builder includes zero delimiter                 = incorrect
checksum parser prefix handling                          = incorrect
unfragmented vs fragmented-first header                  = conflated
complete-message automatic fragmentation                 = incomplete
creator AES inverse ECB direction                        = incorrect
OBEP final-hop transform direction                       = incorrect
production role IV/RNG path                              = deterministic placeholder
padding randomness                                       = repeated/deterministic fallback
ShortBuildRegistrar real pool insertion                  = still placeholder slot 0
successful build -> EstablishedTunnel one-time transfer  = absent
inbound remote-hop vs local-endpoint ownership            = conflated
sentinel u32::MAX/zero-hash next-hop state               = provisional
reassembly expiry                                        = no-op
aggregate reassembly byte bound                          = absent
pre-insertion capacity enforcement                       = incomplete
endpoint processing of all cell records                  = incomplete
full outbound -> inbound deterministic pair              = not passing/present
Plan 116 provisional ignored tests                       = 17
```

## Normative correction authority

The active correction plan pins the required behavior to:

- <https://geti2p.net/spec/tunnel-message>
- <https://geti2p.net/docs/tunnels/implementation>
- pinned Emissary source revision `9b43484a21d5a1291c4881cdae62a36c527f8c0f` for deployed-source comparison only.

Key wire/crypto rules:

```text
TunnelData body                       = 4-byte ID + 16-byte IV + 1008-byte data
checksum                              = SHA256(bytes after zero delimiter || IV)[0..4]
checksum covers padding               = no
checksum covers zero delimiter        = no
unfragmented initial bit3             = 0, Message ID absent
fragmented first bit3                 = 1, Message ID present
follow-on sequence                    = 1..63
participant/IBGW/OBEP transform       = ECB-ENC / CBC-ENC / ECB-ENC
creator inverse                       = ECB-DEC / CBC-DEC / ECB-DEC
```

## Anti-loop state

No additional external short-build or transport validation is required to correct Plan 116.

```text
plan_115_Q0                    = passed-emissary-native-consumer
Q1_authenticated_transport     = deferred
Q2_external_return_established = deferred
NTCP2                          = experimental-non-advertised
Plan_116                       = active-local-correction
Plan_117                       = blocked-on-Plan116-local-pass
```

Do not reopen Emissary/i2pd/Java execution, NTCP2 harnessing, namespaces, VMs, or public-network work during this correction.

## Required closure

Plan 116 may close only as:

```text
plan_116 = passed-local-tunnel-data-plane
```

and only after all criteria in
[`plans/116-completion-correction.md`](116-completion-correction.md) pass, including:

- zero Plan 116 provisional ignored tests;
- canonical Tunnel Message checksum and fragment headers;
- correct forward/inverse AES round trips;
- injected production CSPRNG with fresh IV/padding;
- real one-time established-material transfer and pool registration;
- correct inbound reply-path gateway tunnel ID;
- functional bounded expiry/aggregate reassembly accounting;
- active outbound ROUTER deterministic trajectory;
- active outbound TUNNEL -> inbound tunnel -> local endpoint deterministic trajectory;
- active fragmented/out-of-order end-to-end trajectory;
- full local workspace validation green.

Until then, documentation must describe Plan 116 as incomplete and Plan 117 as blocked.
