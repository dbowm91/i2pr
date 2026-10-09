# Plan 386 status — blocked; Plan 387 owns server Destination readiness

Status: **blocked-control-created-server-missing-router-leaseset-plan-387**.

Plan: `plans/implementation/i2pcontrol-proposal-170/386-els2-reverse-publication-availability-corrective.md`.

Implementation commits: `abac02c0` (`Plan 386: trace post-start ELS2 publication availability`),
`fbc6f8bc` (`Plan 386: limit publication queue to new servers`).

Plan 386 added a bounded, secret-free view of the product publication stage and
its failure class; scheduled server publication when a committed service
generation changes; waited for the actual local DatabaseStore delivery handoff
before beginning the reverse one-attempt SAM lookup; and extended the evidence
guard to cover the new boundary and scheduling behavior. The exact-pinned live
lane established that a control-created server has no router-backed LS2, so the
publication path fails before type-5 store construction or remote database
lookup. The required reverse payload matrix did not pass.

## Requirement-to-evidence matrix

| Requirement | Evidence | Outcome |
|---|---|---|
| Keep the stock i2pd 2.61.0 pin and one-attempt lookup | `tests/integration/els2/run-i2pd-els2.sh`; `reference` and `attempt_budget` rows | Preserved on every attempted live run |
| Reproduce post-start ordinary authority payload and mesh controls | `target/interop/els2-evidence-plan386-none-stage/driver-evidence.tsv`: authority payload returned; Plan 385's separately packaged authority row is also retained as the successful baseline | Driver reached and returned the authority payload before reverse publication. The aggregate `results.tsv` marks the driver-derived authority row failed when the later reverse assertion panics, so this Plan 386 run is not counted as a lane pass |
| Control-created reverse ELS2 server commits | `driver-evidence.tsv`: `reverse-server-create=committed` | Passed |
| Identify the local publication transition | `driver-evidence.tsv`: `reverse-publication-handoff=failed`, `reverse-publication-failure-stage=missing-leaseset`, `attempts=36,accepted=0,failed=36,pending=1` | Failed before type-5 store construction, floodfill selection, or remote lookup |
| Pass reverse NONE payload | Same evidence package; `reverse-payload-returned` failed | Not passed |
| Pass reverse PSK and DH payloads | Not run after the earlier local NONE publication gate failed | Not passed |
| Preserve transcript, credentials, inventory, and advertisement | Source diff and Plan 380/346 owner guards; no support or inventory files changed | Preserved; no capability promotion |

Plan 386’s evidence package inherits the Plan 385 runner’s then-current metadata
(`plan=385`, predecessor 384); the underlying result rows and exact pin are
retained as diagnostic evidence, not as a pass. Commit `abac02c0` corrected the
runner metadata to Plan 386 / predecessor 385 for subsequent runs. Raw i2pd
logs, control payloads, and credentials are not committed.

## Commands and outcomes

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed; no rebuild required.
- `rtk cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1` — passed, 5 tests.
- `rtk bash tests/integration/els2/run-i2pd-els2.sh --self-test` — passed; exact reference binary not invoked by self-test.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --self-test` — passed.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --mutation-table` — passed, 14 detected, 0 missed, 2 controls accepted. Includes mutations for removal of reverse publication stage evidence and post-start server publication scheduling.
- `rtk bash scripts/check-els2-live-lane-evidence.sh` — passed.
- `rtk bash scripts/check-encrypted-service-consumer-caller.sh` — passed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed before registering Plan 387; rerun after registration.
- `rtk git diff --check` — passed.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests. The workflow-validity fixture emitted its expected missing temporary workflow-directory notice; suite result was `OK`.

Exact live executions used separate ignored evidence directories. Multiple
runs reproduced reference startup instability (pinned i2pd process segmentation
faults and tunnel-pool timeout). Two clean NONE runs reached reverse control
create; the diagnostic run recorded the local `missing-leaseset` stage and
failed before remote lookup. The final run failed earlier in the ordinary
authority payload after a peer crash. No PSK or DH reverse run was attempted
under Plan 386 because the earlier local publication precondition was unmet.
The routine floor was not run because required live acceptance failed. No CI
result is claimed.

## Lifecycle, security, compatibility, and findings

- Publication pending state remains bounded by the existing per-destination
  set and five-second retry deadline. Product counters saturate and retain only
  scalar counts plus a fixed failure-stage enum; the provisioning snapshot
  exposes only pool counts, threshold, and LS2 presence.
- A local delivery `accepted` observation is explicitly not treated as remote
  floodfill storage or retrieval. The reverse SAM lookup starts only after the
  local publication handoff succeeds.
- No credential, destination identifier, signing material, type-5 ciphertext,
  or raw payload is written into the new diagnostic rows.
- No transcript, inventory, persistent format, credential seam, support
  boolean, or advertisement changed. Type 5 remains non-advertised and
  `full-proposal-conformant` remains unset.
- **Medium — post-start control-created server has no router-backed LeaseSet.**
  The observed transition is `MissingLeaseSet`: no type-5 DatabaseStore can be
  constructed. Root cause within service-Destination pool activation was not
  localized; Plan 387 owns that corrective.
- **Low — incomplete external matrix and routine floor.** NONE reverse failed
  locally; PSK/DH were not attempted; exact-pin instability also disrupted
  later attempts.

## Unblock audit and disposition

Plan 386 is **blocked**, not passed. The complete reverse matrix and routine
floor remain outstanding. Plan 387 is registered ready to diagnose and correct
the post-start server Destination provisioning/LeaseSet transition.

| Future plan | Remaining dependencies | Can unblock now? | Disposition |
|---|---|---|---|
| 374 | Complete reverse NONE/PSK/DH matrix and other i2pd direction rows | No | remains blocked; Plan 387 owns the newly localized prerequisite |
| 375 | Java pinned-source proof and Java live matrix | No | independently blocked; i2pd evidence cannot discharge it |
| 377 | Plans 374 and 375 passed | No | remains blocked on both inputs |
| 378 | Plan 377 passed | No | remains blocked on Plan 377; no full-conformance claim |
| 387 | Plans 380/381 interfaces, Plan 385 authority row, Plan 386 stage evidence | Yes | registered ready for post-start server readiness corrective |

No downstream plan is unblocked. The Plan 385 evidence remains a historical
blocked record; Plan 386 corrects forward and does not rewrite predecessor
closures.
