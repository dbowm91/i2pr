# Plan 421 — bounded current-pin reverse NTCP2 attempt

Status: **active** — `in-progress-current-pin-reverse-initiator-attempt`.
Corrective successor to [Plan 420](../../closure/ntcp2-transport/420-status.md).

## Objective

Execute the unspent current-pin reverse direction independently: stock i2pd
initiates NTCP2 to the loopback i2pr listener, then the existing sanitized
observer records only fixed stage counts and bounded result codes. Run at most
one attempt; do not retry or run the failed forward direction again.

## Why ready

- Plans 410 and 415–420 established the network-ID-2 loopback profile,
  pristine pinned helper, failure-safe evidence capture, and count-only stock
  log observer.
- The helper has a stock-library `dial` mode and the runner's reverse branch
  starts the i2pr listener before invoking it. Previous plan attempts stopped
  after forward failure, so the reverse attempt budget remains unspent.
- Plan 420's forward attempt reached the post-validation RouterInfo marker but
  produced no I2NP block or DeliveryStatus. A reverse-only attempt can test
  the independent direction without representing the failed forward path as
  repaired.

## Invariants

1. Use pristine i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`; never patch its source or
   binary.
2. Bind and dial only `127.0.0.1`, network ID 2, topology
   `current-network-loopback`; no reseed or public egress.
3. Freeze one reverse attempt; no retry, forward rerun, or daemon activation.
4. Retain only allowlisted stage counts, bounded result codes, pin, direction,
   and cleanup result. Keep raw logs, identities, endpoints, message IDs,
   payloads, and process output out of evidence.
5. Normal-daemon NTCP2 remains disabled and non-advertised.

## Work and acceptance

Add a fail-closed runner direction selector that permits exactly
`forward`, `reverse`, or the existing sequential `both` behavior, with
`both` still stopping after its first failed direction. Negative-test invalid
selectors and prove reverse-only mode cannot invoke forward. Run parser,
runner, and source-check self-tests, focused interop tests, helper rebuild,
NTCP2 vectors and historical boundary, tooling/plan checks, and planning tests.

Then run exactly one reverse-only attempt. Acceptance for the reverse attempt
requires correlated authenticated session and DeliveryStatus evidence plus
cleanup. Any failure stops Plan 421 with sanitized evidence. This plan does
not close Plan 434; its forward and full-message matrix remain outstanding.

No production wire change, support promotion, or capability advertisement is
in scope.
