# Plan 148 status — blocked independent-client closure audit

Status: **`blocked-audit-superseded-by-plan149-150-corrective-sequence`**.

Registered: **2026-09-01**. Superseded for next-action authority: **2026-09-02**.

Plan of record: [`plans/148-m7-sam31-independent-client-final-closure.md`](148-m7-sam31-independent-client-final-closure.md).

Current corrective authority: [`plans/149-status.md`](149-status.md).

## Historical outcome

Plan 148 did not close Milestone 7.

The original attempt correctly rejected two instances of one in-repo Rust `SamClient` helper as "independent clients". That evidence was invalid because it was one implementation/codebase/test author and did not satisfy the two-independent-client requirement.

The attempt also did not complete the SILENT, multi-stream, backpressure, fault, privacy-log, FORWARD/NAMING, and external-client matrix.

Those findings remain valid historical evidence.

## Superseding diagnosis

The original status classified the blocker only as:

```text
blocked-external-client-build-failure
```

That diagnosis was incomplete.

A post-Plan-148 source audit found that the canonical Plan 147 raw STREAM test manually performs product setup that a real external SAM client cannot perform:

- constructs and installs `SamDestinationBridge`s;
- installs deterministic inbound-tunnel factories;
- cross-installs each peer's validated LeaseSet2;
- manually spawns the per-destination runtime driver.

The production `SamServiceState::execute_session_create()` path currently installs the destination runtime, Streaming pool, SAM/stream registries, and session entry, but does not install those product prerequisites.

`execute_stream_connect()` then requires a bridge to exist, and `deliver_outbound()` requires a peer inbound-tunnel factory. Therefore simply making external clients available would still not prove the black-box product.

Plan 149 owned that self-composition defect and closed the documented local
raw-path subset. The remaining external-client, slow-peer, fault-matrix, and
sibling-stream evidence is now Plan 150 work.

## Plan 147 evidence retained but narrowed

Retain Plan 147 as evidence for:

- dedicated raw `TcpStream` ownership;
- permanent command-parser detachment;
- actual Streaming `Established` wait;
- OS CSPRNG production path;
- TCP -> Streaming and Streaming -> TCP byte pump;
- supervised ACK/retransmit runtime ownership;
- same-read post-command raw-byte preservation;
- localhost binary byte-pump smoke when the required bridge/tunnel/routing prerequisites are installed.

Do **not** use the Plan 147 status as evidence that its complete original acceptance matrix passed. Plan 149 closed:

- self-composing session/product setup;
- exact SILENT semantics;
- authenticated non-silent ACCEPT metadata;
- multi-megabyte bounded transfer;
- terminal cleanup and post-shutdown resource baselines.

Plan 150 carries forward slow-reader/slow-writer bounds, the real socket
fault matrix, and sibling-stream lifecycle evidence.

## External-client provenance correction

The Plan 148 client table also contained a stale/invalid libsam3 pin:

```text
libsam3 e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7 / v1.0.0
```

That commit/tag does not resolve in the official `i2p/libsam3` repository.

Plan 150 replaces the live client guidance with verified exact revisions, preferring:

```text
libsam3 official master snapshot:
7d6e658798baec31394c5685f9583343cc00900b

i2psam official snapshot:
b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
```

Legacy i2plib `6edf51cd5d21cc745aa7e23cb98c582144884fa8` remains supplementary because its 2019 high-level asyncio API uses the removed `loop=` argument on modern Python.

## Current authority

```text
plan_146 = passed-m7-sam31-private-destination-reference-requalification
plan_147_raw_driver_implementation = landed-and-retained
plan_147_full_original_acceptance = superseded-by-plan149
plan_148 = blocked-audit-superseded-by-plan149-150-corrective-sequence
plan_149 = passed-m7-sam31-self-composing-local-product-corrective
plan_150 = next-executable-on-plan149-pass

sam_independent_clients = 0-passed
milestone7_local_product = closed-via-plan149
next_executable_plan = 150
```

## Handoff

Do not resume the Plan 148 narrative directly. Plan 149 has closed the
self-composed local product; final Milestone 7 promotion still belongs to
Plan 150.

Execute **Plan 150 only** for correctly pinned external-client/FORWARD/NAMING
final closure.
