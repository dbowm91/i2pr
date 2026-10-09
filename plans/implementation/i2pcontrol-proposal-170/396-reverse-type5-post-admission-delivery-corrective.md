# Plan 396 — reverse type-5 post-admission delivery corrective

Status: **blocked-inbound-receive-owner-transition-plan-397**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Diagnose and correct the first failed transition after the reverse server's
encrypted type-5 DatabaseStore reaches the local delivery-admission boundary.
Plan 395 fixed begin validation; the stock consumer still connected but received
no fixture payload. Add bounded diagnostics to distinguish publication routing,
remote store/lookup visibility, inbound service dispatch, and response delivery.
Correct only the evidenced owner and rerun exact-pinned NONE.

## Evidence and constraints

Plan 395's pinned run at
`target/interop/els2-evidence-plan395-none-type5-20261009` passed both reference
mesh controls and the ordinary authority payload. The reverse product row
reported 2 attempts, 1 accepted, 1 failed, 0 pending; no begin rejection. Stock
SAM connect returned, but the payload read timed out. The run metadata was stale
(Plan 394) and is diagnostic only. Produce correctly labeled Plan-396 evidence.

## Invariants

1. Stock i2pd 2.61.0 at exact SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.
2. No direct transport, synthetic store, manually injected lease, or private
   API shortcut in the black-box payload path.
3. Diagnostics are bounded counts/enums. Never write addresses, keys, request
   ids, raw payloads, or reference logs to evidence.
4. Preserve the type-5 key binding, coordinator cap, ACK correlation, expiry,
   cancellation, and Standard LeaseSet2 behavior.
5. Do not alter type-11 transcript, credentials, support, capability, or
   advertisement.

## Work packages

1. Add post-timeout bounded snapshots for publication stages, pending/ACK state,
   server inbound delivery/Streaming acceptance, and outbound response admission.
   Keep evidence redacted and extend mutation tests.
2. Trace the exact first missing transition from that evidence and source.
3. Add deterministic regression for the identified owner and lifecycle/error
   path; preserve prior coordinator and service-role capacity tests.
4. Run focused tests, applicable guards, and one correctly labeled exact-pinned
   NONE lane. If a reference-specific or downstream protocol cause remains,
   register a successor with the captured boundary.

## Acceptance

The control-created reverse encrypted service delivers the stock NONE fixture
payload through the real tunneled type-5 publication and Streaming path, with
bounded lifecycle regressions and live evidence passing. PSK/DH and Proposal 170
convergence remain gated by their own evidence; no support promotion occurs.
