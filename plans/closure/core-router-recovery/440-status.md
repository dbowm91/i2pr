# Plan 440 status: passed — evidence topology and qualification contract

Closure token: `passed-evidence-topology-and-qualification-contract-with-fail-closed-successor-gates`

Plan: `plans/implementation/core-router-recovery/440-emissary-informed-evidence-and-topology-contract.md`

## Implementation

- `82eea4b9ad5510eb3a91764c6de53e9d25c225ee` — added the recovery evidence-contract guard, its mutation self-test, routine-floor and CI invocations, and the tooling inventory entry.
- This closure commit records the status and dependency audit.

The pinned Emissary comparison remains a read-only behavioral reference. The
accepted profile ladder distinguishes deterministic protocol tests, real
loopback transport, controlled multirouter product evidence, independently
addressed private-LAN qualification, and normal/public readiness. Loopback is
valid for scoped transport evidence but cannot prove external reachability,
public address eligibility, or stock-peer selection.

The stock-to-stock positive control must pass before attributing a candidate,
tunnel, or wire failure to i2pr. NTCP2 acceptance requires per-session
authenticated-link and decoded I2NP evidence; a RouterInfo marker, helper
future, process-wide count, timeout, or close is insufficient. Plan 443's
controlled product result cannot satisfy Plan 433's external gate.

## Requirement-to-evidence matrix

| Requirement | Result and evidence |
| --- | --- |
| Preserve a scoped evidence profile taxonomy | Pass. The five tiers and their evidence limits are recorded in `plans/diagnostics/2026-10-10-emissary-router-qualification-comparison.md`; Plan 440 classifies loopback correctly and keeps public claims behind the separate gates. |
| Require positive controls before protocol attribution | Pass as a contract. Plans 441 and 443 require a known-working stock-to-stock path and stop as fixture/environment-blocked when that control fails. The live stock-to-stock attempt belongs to Plan 441 and was not run as part of this documentation/guard closure. |
| Require session-specific NTCP2 acceptance and sanitized evidence | Pass as a contract. Plan 441 requires one exclusive pair, in-memory correlation, authenticated link, decoded I2NP/DeliveryStatus, and allowlisted persistent fields. |
| Keep public and controlled product gates separate | Pass. Plans 433 and 439 retain their prior dependencies; Plan 443 is explicitly unable to close Plan 433. |
| Add fail-closed regression guard | Pass. The new checker covers seven contract rules and self-tests the corresponding mutations. |
| Preserve historical evidence and support posture | Pass. No historical closure was edited, no reference source was modified, and `specs/support.toml` and protocol behavior are unchanged. |

## Commands and outcomes

Executed locally on the current branch:

- `python3 scripts/check-core-router-recovery-contract.py` — passed.
- `python3 scripts/check-core-router-recovery-contract.py --self-test` — passed; seven in-memory contract mutations rejected.
- `python3 scripts/check-tooling-inventory.py` — passed after updating the derived inventory.
- `python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `python3 scripts/check-adr-number-uniqueness.py` — passed.
- `python3 scripts/check-workflow-validity.py` — passed; 11 workflow files parse and are structurally valid.
- `git diff --check` — passed before closure-record edits; rerun on final closure tree before commit.

No transport, product, or reference-router test was needed for this contract-only
change. No hosted CI result or live interop result is claimed.

## Security, compatibility, and findings

- **High qualification gates remain open.** No non-loopback SSU2 qualification,
  authenticated NTCP2 interop, normal-daemon multihop product path, or floodfill
  selection is claimed here. Their registered successors own those proofs.
- No configuration, key format, RouterInfo, listener, runtime, or wire behavior
  changed. No migration applies.
- The checker is a source/documentation guard; it does not manufacture runtime
  evidence. Its seven mutation controls demonstrate that the named contract
  violations are detected.
- No dependency changes. No new secrets, identities, endpoints, or raw peer
  data are stored.

## Unblock audit and roadmap disposition

Plan 440 is closed at the evidence-contract scope. Plans 441, 442, 443, and 444
list Plan 440 as their only remaining readiness dependency (with Plan 432
already passed for 443), so all four move to `ready`.

Plans 433–439 retain their original qualification dependencies. Plan 431 remains
stopped pending an independently addressed authorized topology; Plans 434/435
remain gated on authenticated NTCP2 evidence; Plans 436–439 retain their normal
role and full-product gates. No unrelated blocked row had all dependencies
closed. No support or capability claim was promoted.
