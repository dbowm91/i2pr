# Plan 253 status — M11 live daemon transit and data-plane corrective

Status: **registered-ready-m11-live-daemon-transit-data-plane-corrective**

Plan of record:
`plans/implementation/transit-tunnels/253-m11-live-daemon-transit-data-plane-corrective.md`

Baseline:
`c94038cfde953051da1a68a1ab29342727daa58b`

## Registration basis

Post-closure source review of Plan 252 found that its runtime-neutral full-message STBM
processor landed correctly, but several daemon completion claims were false positives:

- no live SSU2/router-I2NP owner calls `TransitIngressGate`;
- TunnelData uses an explicit no-op placeholder transform;
- replay/duplicate claims are not backed by canonical duplicate-window state;
- OBEP/IBGW established data-plane behavior is not role-correct;
- code-30 outcomes lose route metadata and are dropped by delivery;
- build delivery does not prove complete encoded I2NP message construction;
- rollback handles only `NoActiveSession`;
- cancellation does not drain registrations/secrets;
- peer mapping is not actually bounded;
- secret-owning `TransitHopMaterial` is cloneable.

The Plan 252 full-message build core remains retained evidence. This record does not claim
implementation.

## Unblock rule

Plan 254 exact-pinned i2pd qualification must remain unregistered until Plan 253 closes
with direct live-owner/data-plane evidence and ordinary CI is green.

M11 capability remains unclaimed and unadvertised.
