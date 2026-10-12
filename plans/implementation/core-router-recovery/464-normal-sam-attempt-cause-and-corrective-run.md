# Plan 464 — Normal SAM build attempt cause and corrective run

**State:** blocked P2 (2026-10-11). Closure: [`464-status.md`](../../closure/core-router-recovery/464-status.md). Owner: core-router-recovery / destination-product. The one authorized causal run returned `inbound-build`, but the runner read only `i2pr.stderr` while the formatted tracing layer writes to `i2pr.stdout`, then deleted the temporary logs. No attempt-scoped receipt was retained, so the first missing transport transition remains unknown. Plan 465 authorizes one corrected evidence-capture run; no unchanged retry is authorized here.

## Objective

For one ephemeral ordinary-process SAM build attempt, identify the earliest missing source-owned transition, correct that cause once with a deterministic regression, then spend one exact-pinned four-peer rerun to prove the correction. Required transition attribution is: peer/route selected; request admitted to the bounded build queue; encoded request handed to SSU2; datagram actually emitted; stock peer accepted and produced a response; authenticated reply received; exact `BuildAttemptId` matched; install committed. If the evidence localizes a blocker outside i2pr (for example stock never selected or accepted the request), retain the product capability blocked with the exact missing condition and do not manufacture a source fix.

## Work packages

1. Map an opaque attempt ID through the existing selector, coordinator, `ExploratoryBuildQueue`, SSU2 send owner, authenticated inbound owner, exact-attempt matcher and install transaction. Reuse existing ownership and bounded counters; no peer hashes, destination keys, raw messages, RouterInfos or identifying logs. Add only the smallest attempt-scoped receipt needed to distinguish admission, actual transport handoff/emission, response, match and commit. Keep counters finite and aggregate; no new observer service or independent protocol path.
2. Add deterministic positive/negative tests for each transition boundary, dropped/cancelled queue admission, sender closure, wrong peer/link, stale/duplicate/malformed reply, exact-attempt mismatch, and failed install rollback. Prove byte/queue leases release on every terminal path. Preserve Plan 458's wrapped type-19 invalid-reply correlation regression.
3. Run one diagnostic SAM request against the existing exact-pinned fixture with unique CLI `--datadir` and `--conf` per reference. Establish the actual selected peer and actual signed role/RouterInfo eligibility. Attribute the earliest missing transition. Do not retry within the run or treat a timeout stage label as causal evidence.
4. If the first divergence is in i2pr source or fixture construction, make one narrow correction and add its regression. If the cause is reference selection/topology or no authorized trigger, stop with the classified blocker and no source workaround.
5. After a confirmed correction, run exactly one fresh pinned four-peer qualification: real nonzero-hop inbound and outbound install, signed LS2 with real leases, external SAM CREATE/STREAM CONNECT/ACCEPT both directions and exact-byte payload, teardown/restart and bounded failure controls. Preserve all raw logs outside Git; commit only sanitized counters and hashes.

## Invariants and non-goals

- No zero-hop shortcut, synthetic tunnel response, fake stock acceptance, patched reference source, public network, broadened listener, or support promotion.
- No unbounded channel or queue; every send, wait and retry has a fixed deadline and cancellation/cleanup path.
- Do not alter Plan 458 or 462 historical status records; this plan supplies a new evidence/correction authority.
- Do not rerun a stock fixture unchanged after the diagnostic attempt. The single post-fix run is spent only after the exact causal correction is recorded.

## Verification and closure

Build required managed-app siblings before focused daemon tests. Run the daemon SAM black-box and exploratory-build unit suites serially, the relevant lifecycle and boundary checks, the existing M6 preflight/tunnel/NetDB controls, then the single pinned post-correction lane and the full `AGENTS.md` floor for production changes. Close only when the causal regression and all Plan 462 capability requirements pass with exact attempt correlation; if the trigger or stock acceptance remains external, close as blocked with the source-owned earliest-missing transition and do not claim SAM tunnel capability.
