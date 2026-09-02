# Plan 149 status — SAM 3.1 self-composing local product corrective authority

Status: **`active-m7-sam31-self-composing-local-product-corrective`**.

Registered: **2026-09-02**.

Plan of record:
[`plans/149-m7-sam31-self-composing-local-product-corrective.md`](149-m7-sam31-self-composing-local-product-corrective.md).

Source audit:

- Plan 145 remaining-gap corrective roadmap;
- Plan 146 passed private-destination reference requalification;
- Plan 147 raw-driver implementation/localhost byte-pump result;
- Plan 148 blocked audit.

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

plan_146 = passed-m7-sam31-private-destination-reference-requalification

plan_147_raw_driver_implementation = landed-and-retained
plan_147_local_binary_smoke = passed
plan_147_full_original_acceptance = superseded-by-plan149

plan_148 = blocked-audit-superseded-for-next-action-by-plan149-150

plan_149 = active-m7-sam31-self-composing-local-product-corrective
plan_150 = blocked-on-plan149

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately

milestone7_local_product = not-closed
sam31_private_destination = reference-compatible-via-plan146
sam31_raw_socket_owner = implemented-via-plan147
sam31_self_composing_product = not-yet-passed
sam_independent_clients = 0-passed
next_executable_plan = 149
next_product_layer = remain-on-milestone7
```

## Why Plan 148 cannot be resumed directly

The post-Plan-148 audit identified a product-composition blocker that exists before external-client provisioning.

The canonical Plan 147 raw STREAM test manually performs private setup after SAM `SESSION CREATE`:

- constructs/installs `SamDestinationBridge`s;
- installs deterministic inbound-tunnel factories;
- cross-installs each peer's validated LeaseSet2;
- manually spawns per-destination runtime drivers.

The production `execute_session_create()` path does not do those things. It currently installs the destination runtime, separate Streaming pool, stream registry session, and SAM session entry, then returns success.

`execute_stream_connect()` subsequently requires a bridge in `sam_destinations`; without the test-only setup it returns an I2P error for a missing bridge. `deliver_outbound()` also depends on an installed inbound-tunnel factory and otherwise drops the request rather than producing a useful product-path failure.

Therefore fetching/building external clients alone would not satisfy Plan 148.

## Plan 147 acceptance correction

Plan 147 delivered important real implementation:

- permanent line-parser -> owned raw `TcpStream` handoff;
- actual Streaming `Established` wait;
- OS CSPRNG in production CONNECT/delivery;
- TCP -> `StreamingManager::send_data()`;
- `drain_delivered()` -> TCP;
- supervised ACK/retransmit runtime driver;
- same-read buffered raw-byte preservation;
- a localhost binary byte-pump test.

Retain all of that.

However Plan 147's own original acceptance criteria also required SILENT exactness, slow-reader/slow-writer bounds, fault/retransmit acceptance, close/reset, sibling streams, and multi-megabyte bounded transfer. Its closure record deferred those items to Plan 148. Plan 149 now owns them and supersedes the broad interpretation of `passed-m7-sam31-dedicated-raw-stream-driver`.

## Additional concrete protocol defect

The current raw-transition handler writes `STREAM STATUS RESULT=OK` before handoff regardless of the request's `SILENT=true` flag. The raw driver retains but does not use the flag. Plan 149 must correct CONNECT/ACCEPT SILENT semantics and non-silent ACCEPT peer-Destination metadata before external closure.

## External-client provenance correction

Plan 148's recorded libsam3 pin is invalid for the official repository:

```text
recorded: e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7 / v1.0.0
```

Verified official `i2p/libsam3` references include:

```text
v0.31.2 -> ea52a3251d60906d67f9a1031a6ed7642753f94f
current official master snapshot used by Plan 150 guidance:
7d6e658798baec31394c5685f9583343cc00900b
```

Plan 150 replaces the live external-client guidance with correctly pinned `libsam3` + `i2psam`, keeping legacy i2plib as supplementary evidence because its 2019 high-level asyncio API is awkward on current Python runtimes.

## Execution sequence

1. **Plan 149** — make SAM `SESSION CREATE` self-compose the local destination/Streaming product, remove hidden test setup from canonical acceptance, and close deferred Plan 147 SILENT/backpressure/fault/lifecycle criteria.
2. **Plan 150** — provision correctly pinned external clients through a reproducible unprivileged lane and close final independent-client/FORWARD/NAMING evidence.

Plan 148 remains historical failed-audit evidence and must not be used as the next executable plan.

## Environment contract

Both plans remain compatible with the constrained development policy:

```text
root/sudo              = not required
namespaces             = not required
Docker                 = not required
VM/Multipass           = not required
systemd                = not required
public I2P network     = not required
live NTCP2/SSU2        = not required
localhost TCP          = required
GitHub-hosted manual interop workflow = allowed for Plan 150
```

Plan 149's local product fabric is an explicitly localhost/authenticated-router-link-bypassed seam. It must never be described as live I2P tunnel interoperability.

## Handoff instruction

Read this status, Plan 149, Plan 146 status, and Plan 147 status.

Execute **Plan 149 only**.

Do not fetch/build external clients and do not execute Plan 150 until the new black-box self-composed SAM test proves that two destinations created only through the real SAM listener can CONNECT/ACCEPT and exchange raw bytes without any private bridge, LeaseSet2, inbound-tunnel-factory, or driver setup by the test.
