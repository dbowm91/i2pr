# Plan 145 status — Milestone 7 remaining-gap corrective roadmap

Status: **`active-m7-sam31-remaining-gap-corrective-roadmap`**.

Registered: **2026-09-01**. Updated after the Plan 148 audit: **2026-09-02**.

Plan of record:
[`plans/145-m7-sam31-remaining-gap-corrective-roadmap.md`](145-m7-sam31-remaining-gap-corrective-roadmap.md).

Newest executable authority:
[`plans/149-status.md`](149-status.md).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

plan_142_base64 = passed
plan_142_private_destination_external_compatibility = superseded-by-plan146

plan_143_local_delivery_seam = landed-and-retained
plan_143_full-raw-stream-acceptance = superseded-by-later-correctives

plan_144_in-process-streaming-handshake = passed-local-evidence
plan_144_independent-client-final-closure = not-passed

plan_146 = passed-m7-sam31-private-destination-reference-requalification

plan_147_raw_driver_implementation = landed-and-retained
plan_147_local-binary-smoke = passed
plan_147_full-original-acceptance = superseded-by-plan149

plan_148 = blocked-audit-superseded-by-plan149-150-corrective-sequence
plan_149 = active-m7-sam31-self-composing-local-product-corrective
plan_150 = blocked-on-plan149

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately

milestone7_local_product = not-closed
sam31_base64 = corrected
sam31_private_destination = reference-compatible-via-plan146
sam31_raw_socket_owner = implemented-via-plan147
sam31_self_composing_product = not-yet-passed
sam_independent_clients = 0-passed
router_construction = may-continue-within-m7
next_executable_plan = 149
next_product_layer = remain-on-milestone7
```

## Closed sub-claims

### Plan 146 — private destination

Plan 146 is closed. Java I2P 2.12.0 and i2pd 2.60.0 reference behavior confirmed the compact 455-byte Ed25519/ECIES-X25519 PrivateKeyFile representation used by i2pr. The import path preserves the embedded destination encryption public field and validates the signing seed/public relationship.

Do not reopen this sub-claim without a concrete new reference incompatibility.

### Plan 147 — raw socket owner and byte pump

Retain these Plan 147 results:

- dedicated ownership transfer of the SAM `TcpStream` after CONNECT/ACCEPT;
- permanent command-parser detachment;
- actual Streaming `Established` wait before CONNECT success;
- OS CSPRNG in the runtime CONNECT/delivery path;
- bounded TCP -> `StreamingManager::send_data()` segmentation;
- Streaming -> TCP `drain_delivered()` path;
- supervised ACK/retransmit polling;
- same-read post-command byte preservation;
- localhost binary byte transfer when bridge/routing/tunnel prerequisites are installed.

Plan 147 remains useful implementation evidence. Its broad closure label is no longer sufficient Milestone 7 acceptance authority because several original acceptance items were deferred.

## Why Plan 148 was superseded

Plan 148 correctly refused to count two copies of one Rust helper as independent clients. It also correctly recorded `sam_independent_clients = 0-passed`.

However the original status treated missing external-client source/build artifacts as the primary blocker. The subsequent source audit found an earlier product-composition defect:

- the canonical Plan 147 test manually installs `SamDestinationBridge`s after `SESSION CREATE`;
- it manually installs peer LeaseSet2 routing;
- it manually installs deterministic inbound-tunnel factories;
- it manually spawns per-destination runtime drivers;
- the production `execute_session_create()` path does not perform those steps.

A real external client cannot call those private Rust setup APIs. Therefore simply acquiring i2plib/libsam3 would not close Milestone 7.

The audit also confirmed a concrete raw-protocol issue: the current raw-transition handler writes `STREAM STATUS RESULT=OK` even for `SILENT=true`, while the raw driver ignores the retained silent flag.

## Corrective sequence

The active sequence is now:

1. [`plans/149-m7-sam31-self-composing-local-product-corrective.md`](149-m7-sam31-self-composing-local-product-corrective.md) — **next executable**. Make `SESSION CREATE` self-compose the local product, eliminate private post-create setup from canonical acceptance, and close deferred Plan 147 SILENT/backpressure/fault/lifecycle criteria.
2. [`plans/150-m7-sam31-external-client-reproducible-final-closure.md`](150-m7-sam31-external-client-reproducible-final-closure.md) — blocked on Plan 149. Run correctly pinned external clients through the real listener and close FORWARD/NAMING/final M7 evidence.

Plan 148 remains historical blocked-audit evidence and is superseded for execution.

## External-client guidance correction

The old Plan 148 `libsam3` pin (`e0da4f...`, `v1.0.0`) does not resolve in the official repository.

Plan 150 records live guidance around verified official revisions:

```text
libsam3:
  repo = https://github.com/i2p/libsam3
  preferred exact snapshot = 7d6e658798baec31394c5685f9583343cc00900b
  known release tag v0.31.2 = ea52a3251d60906d67f9a1031a6ed7642753f94f

i2psam:
  repo = https://github.com/i2p/i2psam
  exact snapshot = b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac

i2plib:
  repo = https://github.com/l-n-s/i2plib
  exact final commit = 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  role = supplementary unless a compatible Python runtime is deliberately qualified
```

## Environment policy

Plans 149/150 remain compatible with the constrained development environment:

```text
root/sudo              = not required
namespaces             = not required
Docker                 = not required
VM/Multipass           = not required
systemd                = not required
public I2P network     = not required
live NTCP2/SSU2        = not required
localhost TCP          = required
manual GitHub-hosted external-client workflow = allowed for Plan 150
```

The Plan 129 authenticated-router-link-bypassed localhost seam remains the allowed lower-network shortcut. It must be named/documented as local product evidence and never promoted to router-interoperability evidence.

## Handoff instruction

Read this file, `plans/149-status.md`, Plan 149, Plan 146 status, and Plan 147 status.

Execute **Plan 149 only**.

Do not execute Plan 150 and do not move to Milestone 8 until the black-box self-composed SAM product passes without private bridge, LeaseSet2, inbound-tunnel-factory, or driver setup by the test.
