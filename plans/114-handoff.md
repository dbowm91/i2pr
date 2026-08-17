# Plan 114 handoff

- Status: **ready for implementation**
- Date: 2026-08-17
- Plan-of-record: `plans/114-short-build-terminal-routing-chain-correction.md`
- Scope: high-level short-build terminal routing + tunnel-ID chain correction
- External network gate: **none**
- Blocks: qualified independent-router short-build delivery checkpoint

## Current authority

Plans 111-113 retain their cryptographic, padding, multi-record, role-topology, and inbound-originator-fake results. A post-Plan-113 audit found one remaining composition defect in `crates/i2pr-tunnel/src/short.rs` and one directly related forwarding-chain invariant that is not enforced.

Current state:

```text
plan_111                        = retained-core-crypto-corrected
plan_112                        = passed-outbound-pre-delivery-closure
plan_113                        = passed-inbound-reference-reconciliation
plan_114                        = ready-for-implementation

terminal_next_router_high_level = incorrect-self-fallback
outbound_reply_router_model     = missing
intermediate_next_tunnel_chain  = not-validated
high_level_success_e2e          = insufficiently-strict

qualified_external_delivery     = blocked-on-plan114
normal_daemon_ntcp2             = disabled-and-unenableable
ntcp2                           = experimental-non-advertised
```

## Exact defect boundary

`ShortBuildPath -> build_hop_specs() -> MultiRecordHopSpec` is the corrective surface.

Do not reopen the low-level cryptographic implementation.

The high-level builder must stop doing this for the terminal hop:

```text
terminal next_router = terminal hop's own router hash
```

Required behavior:

```text
non-terminal hop:
  next_router = following hop router hash
  next_tunnel = following hop receive tunnel ID

terminal outbound OBEP:
  next_router = explicitly configured reply router hash
  next_tunnel = explicitly configured reply tunnel ID

terminal inbound remote hop:
  next_router = explicit creator/originator router hash
  next_tunnel = configured creator-side receive tunnel ID
```

## Reference evidence

Current Java I2P `BuildMessageGenerator.createUnencryptedRecord()` uses the following hop's receive tunnel/router for intermediate hops and explicit `replyTunnel`/`replyRouter` for the final outbound hop.

Current i2pd `TunnelHopConfig::SetNext()` sets `nextTunnelID = next->tunnelID`; its outbound constructor applies `SetReplyHop(replyTunnelID, replyIdent)` to the final hop, while its inbound constructor routes the final real hop toward the local router identity.

The Plan 113 lower-level inbound trajectory already supplies the local originator hash as the terminal inbound next router, so the low-level record format does not need redesign.

## Required implementation sequence

1. add an explicit outbound reply-router hash to `ShortBuildPath`;
2. validate direction-specific terminal metadata;
3. validate every intermediate `next_tunnel == following.receive_tunnel` at both high- and low-level public construction boundaries;
4. remove the terminal self-router fallback;
5. derive terminal next router by direction;
6. assert decrypted request routing fields for every hop;
7. replace permissive high-level E2E acceptance with exact outbound and inbound trajectories that must reach `Established`;
8. run focused and workspace checks;
9. add `plans/114-status.md` and restore `qualified_external_delivery = unblocked-next-checkpoint` only after all acceptance criteria pass.

## Scope guard

Do not add or change:

- NTCP2 activation;
- SSU2;
- live router execution as a closure criterion;
- Python harness code;
- Docker/Multipass/namespaces/root requirements;
- Noise/KDF/AEAD algorithms;
- Plan 113 inbound creator-key policy;
- generic I2NP dispatch;
- NetDB execution.

## Successful handoff state

```text
plan_114                        = passed-terminal-routing-chain-correction
intermediate_router_chain       = exact
intermediate_tunnel_id_chain    = exact
outbound_terminal_route         = explicit-reply-router-and-tunnel
inbound_terminal_route          = explicit-creator-router-and-tunnel
high_level_outbound_e2e         = strict-established
high_level_inbound_e2e          = strict-established
qualified_external_delivery     = unblocked-next-checkpoint
milestone4b                     = blocked-on-independent-router-evidence
normal_daemon_ntcp2             = disabled-and-unenableable
```

Do not begin external delivery before Plan 114 closes.
