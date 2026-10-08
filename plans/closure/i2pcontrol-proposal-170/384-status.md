# Plan 384 status — blocked; corrective plan 385 owns remaining rows

Status: **blocked-post-start-authority-lookup-and-reverse-publication-corrective-plan-385**.

Plan: `plans/implementation/i2pcontrol-proposal-170/384-i2pr-els2-reverse-and-post-start-authority-successor.md`.

Implementation/evidence commit: `28572ae0496bfd89557c84584a4afe876652dfde`
(`Plan 384: add post-start authority lane and gossip gate`). This commit adds
the post-start ordinary authority driver, the standard-LS2 gossip convergence
gate and peer selection audit, and checker requirements. It does not establish
the end-to-end authority payload or reverse publication capability.

## Requirement-to-evidence matrix

| Requirement | Evidence | Outcome |
|---|---|---|
| Local `get` projects encrypted address | `crates/i2pr-daemon/tests/i2pcontrol_els2_black_box.rs::plan334_els2_create_get_rawconfig_round_trip_over_jsonrpc`; included in checker unit rows | Passed locally; existing test, no implementation change |
| Post-start ordinary client commits after startup | Plan 384 driver evidence: create succeeded; manager generation advanced; committed remote target projection present | Passed in the live run |
| Post-start ordinary listener provisioning | Plan 384 driver evidence: client listener bound | Passed in the live run |
| Reference standard LeaseSet gossiped before authority request | runner `gossip-convergence-gate` and `gossip-selection-audit`; all f/c/n candidates logged the standard destination before the request | Passed in the valid pinned run |
| Post-start authority payload reaches application | `authority-b32-payload-returned` | **Failed** in every valid gossip-gated run; listener bound, payload not returned |
| Reverse publication, NONE/PSK/DH | No reverse attempt | Not run; blocked by the ordinary lookup failure and unresolved publication path |
| Mesh control and consumer-direction rows | Existing Plan 381 controls in the same lane | Passed in the valid pinned run |
| No reference modification, no advertisement, secret scrubbing | Exact pinned stock i2pd; sanitized evidence excludes credentials; no support/inventory/advertisement edits | Preserved |

## Commands and executed results

Local, on this worktree:

- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed before subsequent focused daemon runs.
- `cargo fmt --all --check` — passed.
- `cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1` — passed, 5 tests.
- `bash scripts/check-els2-live-lane-evidence.sh --self-test` — passed.
- `python3 scripts/check-els2-live-lane-evidence.py --self-test` and `--mutation-table` — passed; 10 mutations detected, 2 controls accepted.
- `bash tests/integration/els2/run-i2pd-els2.sh --self-test` — passed.
- `bash scripts/check-encrypted-service-consumer-caller.sh` — passed.
- `python3 scripts/check-global-plan-number-uniqueness.py` — passed before registering Plan 385; rerun at handoff.
- Two valid pinned i2pd authority runs passed the gossip gate but failed to return the authority payload. A temporary local five-attempt lookup experiment also failed and was reverted; production remains at three attempts. One other run was invalid because a stock mesh process segfaulted and its standard control endpoint refused connections; it is not counted as evidence.
- Latest sanitized lane output: `target/interop/els2-evidence/evidence.md`, results SHA-256 `16c0dd6197538e25cae93aa91a8e8a0cb76a8da660c1f779b0893adbc6ad2348`, 15/17 rows passed, with `i2pr-rows` and `authority-b32-payload-returned` failed. This ignored build output is not committed; raw reference logs remain outside the repository.

The full routine floor was not run: the mandatory live authority payload failed,
and the reverse matrix was not reached. No CI result is claimed.

## Invariant, compatibility, and security review

- The pinned reference was stock i2pd 2.61.0 at
  `635b013a612ff47278ef02acf8580a28e10e26c5`; no reference modification was
  made.
- The lane retains `MAX_ATTEMPTS=1`. The temporary five-attempt production
  experiment did not fix the observed failure and was fully reverted; no
  production lookup budget change remains.
- The deployed type-11 profile, Proposal 170 inventory, Plan 380 credential
  seam, support inventory, and advertisement posture are unchanged.
- The evidence checker keeps reverse-row absence guarded until passing
  replacements exist. Raw logs and credential values are excluded.
- The new driver uses the loopback control surface and real transport; it does
  not inject a decoded LeaseSet. Its intermediate state evidence separates
  creation, generation, projection, bind, and payload outcomes.
- No persistent format or public control schema changed. The failure remains
  fail-closed; there is no silent success path.

## Findings

- **Medium — ordinary standard LeaseSet lookup does not return a payload.**
  The controlled floodfills had logged the exact target before the request,
  the post-start client committed and bound, and a temporary retry-budget
  increase did not resolve it. Plan 385 owns source-to-sink diagnosis and a
  regression-backed fix. Do not infer whether lookup selection, reply handling,
  or publication is responsible without new evidence.
- **Medium — reverse i2pr publication remains unqualified.** NONE, PSK, and DH
  were not run; Plan 385 owns the complete matrix. No mode may be represented
  as passed.
- **Low — full routine floor and CI were not run** because the required live
  acceptance row failed. Plan 385 must run the floor after its live gates pass.

## Unblock audit and disposition

Plan 384 is **blocked**, not passed: its stop condition fired. Plan 385 is
registered in the same subsystem and owns the failed standard lookup and the
unattempted reverse matrix. No future plan is unblocked by this result:

- Plan 374 remains blocked on the i2pd reverse and authority rows, now owned
  by Plan 385; the existing consumer rows remain delivered by Plan 381.
- Plan 375 remains independently blocked on its Java lane/source evidence.
- Plan 377 remains blocked on both Plans 374 and 375.
- Plan 378 remains blocked on Plan 377; `full-proposal-conformant` remains
  unset.

The Plan 384 implementation document remains for traceability. Plan 385 is the
new plan of record for all unresolved Plan 384 requirements.
