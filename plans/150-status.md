# Plan 150 status — external-client core evidence retained; final acceptance superseded

Status: **`external-client-core-passed-final-acceptance-superseded-by-plan151`**.

Registered: **2026-09-02** (UTC). Original closure attempt: **2026-09-03** (UTC). Post-closure evidence audit: **2026-09-03**.

Plan of record:
[`plans/150-m7-sam31-external-client-reproducible-final-closure.md`](150-m7-sam31-external-client-reproducible-final-closure.md).

Newest final-acceptance authority:
[`plans/151-status.md`](151-status.md).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_146 = passed-m7-sam31-private-destination-reference-requalification
plan_147_raw_driver = landed-and-retained
plan_148 = blocked-audit-historical-superseded
plan_149 = passed-m7-sam31-self-composing-local-product-corrective

plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = active-m7-sam31-final-acceptance-evidence-correction

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost_final_acceptance = not-yet-closed
sam_independent_clients = at-least-two-passed-via-plan150
router_to_router_interoperability = not-claimed
next_executable_plan = 151
next_product_layer = remain-on-milestone7
```

## Retained successful Plan 150 evidence

The post-closure audit does **not** invalidate the useful external-client work.
Retain the following:

### Exact external provenance

```text
libsam3:
  repository = https://github.com/i2p/libsam3
  revision = 7d6e658798baec31394c5685f9583343cc00900b
  result = built-and-probed; not counted
  reason = public sam3CreateSession requires PRIV length >= 884, while i2pr's canonical Ed25519 PRIV is 608 characters

i2psam:
  repository = https://github.com/i2p/i2psam
  revision = b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
  result = passed; unmodified normal public API

i2plib substitute:
  repository = https://github.com/l-n-s/i2plib
  revision = 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  result = passed; unmodified i2plib.sam surface plus thin socket harness
```

The two counted clients remain independently implemented. No external source
was vendored or patched for i2pr.

### Valid executable results

The Plan 150 external lane genuinely proved:

- i2plib-surface ACCEPT ↔ i2psam CONNECT exact bidirectional 2 MiB payloads;
- i2psam ACCEPT ↔ i2plib-surface CONNECT exact bidirectional 2 MiB payloads;
- binary payload coverage including NUL, LF/CRLF, invalid UTF-8, all-byte and SAM-looking data;
- `SILENT=true` transcript behavior;
- private-destination import/generation through both counted client surfaces;
- NAMING `ME` / full Destination / malformed / unknown cases;
- unsupported version/style/option and malformed/duplicate-input rejection;
- a positive STREAM FORWARD trajectory through a real loopback target with exact bytes and authenticated peer metadata;
- the Plan 149 self-composed product suite on the same lane.

The manual GitHub-hosted SAM external-client workflow also completed
successfully on the audited Plan 150 head. These results are retained as
external-client core evidence.

## Why the original final closure is superseded

Plan 150's plan-of-record required more than the external core matrix.
Several carry-forward acceptance items were not executed by the final harness.

The clearest example is the current `run-independent.sh` row:

```text
record "multiple-stream-lifecycle" passed "retained Plan 149 black-box sibling/lifecycle suite"
```

That row is unconditional. The referenced Plan 149 black-box file contains
four tests (2 MiB transfer, SILENT, teardown, same-read raw bytes) and does not
contain a two-sibling-stream isolation test.

Plan 149 explicitly deferred these items to Plan 150:

- slow-reader / slow-writer boundedness;
- one-DATA-drop retransmission;
- ACK-drop recovery;
- duplicate DATA exact-once behavior;
- DATA reorder recovery;
- authenticated/ciphertext corruption rejection;
- retransmission-ceiling terminal behavior;
- sibling-stream and broader close/reset lifecycle acceptance.

Plan 150's final acceptance criteria kept those requirements, but the closing
external script did not execute them before generating its evidence summary.

The positive FORWARD test is also narrower than Plan 150's complete required
FORWARD matrix (silent forwarding, second stream, refusal/timeout, owner
teardown/unregister, ACCEPT/FORWARD exclusion, loopback-only rejection).

Finally, Plan 150 required explicit focused Plan 127–134 regression commands;
the closure recorded an aggregate workspace run rather than a complete
per-plan focused command ledger.

This is an evidence-authority problem, not evidence that the Plan 149 product
architecture is wrong.

## Reproduction of retained external evidence

```text
bash scripts/interop/fetch-sam-clients.sh --rebuild
bash tests/integration/sam/clients/build.sh
bash tests/integration/sam/run-independent.sh
```

The existing script remains useful, but Plan 151 must remove synthetic pass
bookkeeping and make every final acceptance row derive from an executed test.

The committed historical summary is
[`tests/integration/sam/evidence.md`](../tests/integration/sam/evidence.md).
Treat it as Plan 150 external-core evidence, not current final M7 authority.

## Handoff

Execute [`Plan 151`](151-m7-sam31-final-acceptance-evidence-correction.md).

Do not begin Milestone 8 implementation from this status. Plan 151 must close
the missing sibling-stream, slow-peer, deterministic-fault, CLOSE/RESET,
FORWARD lifecycle, and explicit M6 regression evidence before Milestone 7 is
finally closed.