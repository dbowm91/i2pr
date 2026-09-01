# Plan 145 status — Milestone 7 remaining-gap corrective authority

Status: **`active-m7-sam31-remaining-gap-corrective-roadmap`**.

Registered: **2026-09-01**.

Plan of record:
[`plans/145-m7-sam31-remaining-gap-corrective-roadmap.md`](145-m7-sam31-remaining-gap-corrective-roadmap.md).

Supersedes for next-action authority: Plan 141's former `Plan 145 candidate` handoff and the over-broad closure interpretations in Plans 142/143.

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

plan_142_base64 = passed
plan_142_private_destination_external_compatibility = superseded-by-plan146-requalification

plan_143_local_delivery_seam = landed-and-retained
plan_143_full-raw-stream-acceptance = superseded-by-plan147-corrective

plan_144_in-process-streaming-handshake = passed-local-evidence
plan_144_independent-client-final-closure = not-passed

plan_145 = active-m7-sam31-remaining-gap-corrective-roadmap
plan_146 = next-executable
plan_147 = blocked-on-plan146
plan_148 = blocked-on-plan147

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately

milestone7_local_product = not-closed
sam31_base64 = corrected
sam31_private_destination = external-reference-proof-required
sam31_raw_stream = not-yet-product-closed
sam_independent_clients = 0-passed
router_construction = may-continue-within-m7
next_executable_plan = 146
next_product_layer = remain-on-milestone7
```

## Why Plan 145 exists

The post-Plan-144 audit found that the repository is CI-green but still has three distinct evidence/product gaps.

### 1. Private-destination interoperability

Plan 142 correctly fixed I2P Base64 (`-` / `~`, `=` padding), but it did not satisfy its own requirement for:

- a reference-generated `PRIV` imported by i2pr with exact public Destination equality;
- i2pr-generated `PRIV` consumed by an independent/reference PrivateKeyFile implementation;
- a real-listener external/reference smoke.

Current official SAM documentation still describes `PRIV` as 663+ binary / 884+ Base64 and the 256-byte encryption-private-key field as unused, while the common-structures spec permits type-specific private-key sizes in other contexts. Plan 146 must resolve this with executable reference behavior before the canonical i2pr `PRIV` format is considered closed.

### 2. Raw STREAM product path

Plan 143 landed valuable components (`i2pr_client::deliver`, Plan 129 destination-stack delivery, captured-outbound removal). Plan 144 fixed the canonical-vs-receiver StreamingManager handshake routing.

However the production SAM listener still lacks the dedicated raw TCP driver required by the original Plan 143 acceptance contract. In particular:

- CONNECT currently marks the SAM attachment Established after SYN creation rather than after actual Streaming establishment;
- production CONNECT constructs deterministic `ChaCha8Rng::seed_from_u64(0)`;
- the accepted TCP socket is not transferred to a raw application-byte owner;
- post-command buffered bytes are not handed to a raw driver;
- ACCEPT does not complete the real inbound handshake before raw mode;
- no TCP->`send_data()` / `drain_delivered()`->TCP loops exist;
- no bounded retransmit/delayed-ACK runtime driver exists;
- SILENT/backpressure/fault/close acceptance remains unproven.

Plan 147 owns these corrections.

### 3. Independent-client closure

Plan 144's in-process A<->B SYN/SYN-response test is useful local evidence but is not independent-client interoperability.

`i2plib` and `libsam3` are selected/pinned candidates, but neither has yet moved application bytes through the real i2pr SAM listener. `sam_independent_clients` therefore remains `0-passed`.

Plan 148 owns the two-client cross-implementation byte lane, FORWARD/naming revalidation, resource/privacy closure, M6 regression gate, and final Milestone 7 status.

## Execution sequence

1. [`plans/146-m7-sam31-private-destination-reference-requalification.md`](146-m7-sam31-private-destination-reference-requalification.md)
2. [`plans/147-m7-sam31-dedicated-raw-stream-driver-corrective.md`](147-m7-sam31-dedicated-raw-stream-driver-corrective.md)
3. [`plans/148-m7-sam31-independent-client-final-closure.md`](148-m7-sam31-independent-client-final-closure.md)

Execute sequentially.

Do not start Plan 147 with unresolved reference private-destination behavior. Do not start Plan 148 until the real TCP raw-stream product lane passes without internal application-byte shortcuts.

## Environment policy

All three passes are designed for the existing constrained host:

```text
root/sudo              = not required
namespaces             = not required
Docker                 = not required
VM/Multipass           = not required
systemd                = not required
public I2P network     = not required
live NTCP2/SSU2        = not required
localhost TCP          = required
reference libraries    = allowed
```

The Plan 129 authenticated-router-link-bypassed local seam remains the allowed lower-network shortcut. The destination/ECIES/Garlic/tunnel/Streaming stack above it may not be bypassed in acceptance evidence.

## Handoff instruction

The next implementation model should read Plan 145 and execute **Plan 146 only**.

A Plan 146 closure record must either:

- pass the bidirectional private-destination reference evidence and make Plan 147 executable; or
- record a concrete format/identity blocker and stop.

Do not move to Milestone 8 until Plan 148 closes Milestone 7.