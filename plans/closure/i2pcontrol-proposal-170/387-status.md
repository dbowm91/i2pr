# Plan 387 status — blocked; Plan 388 owns pool-submission localization

Status: **blocked-post-start-server-pool-never-registers-plan-388**.

Plan: `plans/implementation/i2pcontrol-proposal-170/387-post-start-els2-server-destination-readiness.md`.

Implementation commits: `6e9f8ff` (`Plan 387: expose post-start ELS2 pool build state`),
`b34c843` (`Plan 387: keep ELS2 evidence hashes aligned`).

Plan 387 re-ran the exact pinned i2pd 2.61.0 NONE lane. The post-start
ordinary authority payload returned; a control-created reverse server
committed; the local publication handoff then failed after its 180-second
wait. The redacted pool snapshot showed zero inbound and outbound
registrations, zero usable inbound leases, minimum 1, and no LS2. This confirms
the failure remains before publication, but does not reveal whether the pool
driver skipped build submission, builds were pending, build attempts failed or
timed out, or registration/lease installation failed.

Plan 387 added the missing bounded diagnostic fields to the provisioning
snapshot and corrected the runner/checker/test identifiers and evidence
metadata to Plan 387. The clean failing run predates that metadata correction
and remains packaged as Plan 386; it is diagnostic only and is not rewritten.
The next run encountered reference i2pd process instability before the driver
could capture the new counters. Plan 388 owns the next localization and fix.

## Requirement-to-evidence matrix

| Requirement | Evidence | Outcome |
|---|---|---|
| Preserve exact i2pd pin and one-attempt remote lookup | `target/interop/els2-evidence-plan387-none-absolute/evidence.json`; runner source | Pin `635b013a612ff47278ef02acf8580a28e10e26c5`, 2.61.0, `MAX_ATTEMPTS=1` preserved |
| Preserve post-start ordinary authority payload | `driver-evidence.tsv`: `authority-b32-payload-returned=true` | Passed in the clean NONE diagnostic run |
| Commit a server Destination after startup | `driver-evidence.tsv`: `reverse-server-create=committed` | Passed |
| Identify pool/LeaseSet readiness state | Same driver evidence: `reverse-destination-provisioning=Some(DestinationProvisioningSnapshot { inbound_registrations: 0, usable_inbound_leases: 0, minimum_usable_inbound: 1, outbound_registrations: 0, lease_set_present: false })` | Failure remains before a real LS2; precise build transition unresolved |
| Capture pending/build failure state in a clean live run | Added fields in `service_product.rs`; next exact run stopped before driver after i2pd crash | Not captured; Plan 388 owns it |
| Reverse NONE payload | `evidence.json`, `results.tsv`, and `driver-evidence.tsv`, SHA-256 `adf5eaa3e7499b7f5b1ed4a7b48c9f848e91b2a1bcdb862c88546258b9afb15d` / `16b67c12afacc30d3338079c23269a2b8c183c641627e8193be39aad48527cd3` / `f53a70fc16084cc26211829c505ff47cddeeb0d93515e39fcd951dd2a0e75a8c` | Failed locally at `MissingLeaseSet`, before publication admission |
| Reverse PSK and DH payloads | No executions after NONE prerequisite failed | Not passed |
| Preserve transcript, credential seam, inventory, and advertisement | Source diff; no support/inventory/spec files changed | Preserved; type 5 remains non-advertised |

The failed live artifact carries `plan=386` / `predecessor_plan=385` in its
packaging because it was generated before Plan 387 corrected the runner. Its
`evidence.json` also contains a `results_sha256` that does not match the final
`results.tsv`: the runner appended an `evidence-packaged` result after hashing
and then recopied the changed TSV. The hashes above are independently computed
from the retained files. Commit `b34c843` removes that post-hash row, verifies
the package files before the final TSV copy, and adds an evidence-checker
mutation for the invariant; the old artifact is not rewritten. It is retained
as diagnostic evidence, not relabeled or counted as a Plan 387 pass. A first
invocation also used a relative evidence path and failed before the driver
opened its evidence file; it is a harness invocation error, not product
evidence.

## Commands and outcomes

- `rtk cargo fmt --all` — passed after formatting the snapshot edit.
- `rtk cargo fmt --all --check` — passed.
- `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed; no rebuild required.
- `rtk cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1` — passed, 5 tests.
- `rtk bash tests/integration/els2/run-i2pd-els2.sh --self-test` — passed; did not invoke i2pd.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --self-test` — passed.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --mutation-table` — passed, 15 detected, 0 missed, 2 controls accepted.
- `rtk bash scripts/check-els2-live-lane-evidence.sh` — passed.
- `rtk bash scripts/check-encrypted-service-consumer-caller.sh` — passed.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests; the workflow-validity fixture printed its expected missing temporary workflow-directory notice.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed before registering Plan 388; rerun after registration.
- `rtk git diff --check` — passed.
- `rtk bash tests/integration/els2/run-i2pd-els2.sh` with an absolute isolated evidence directory — failed the required reverse NONE gate after 186 seconds; ordinary authority payload passed; reverse snapshot showed zero registrations and no LS2. Evidence SHA-256 values are recorded above.
- A later exact lane attempt encountered an i2pd segmentation fault and lost the consumer SAM listener before the driver; it was interrupted during its fixed wait and is not counted as a product result.
- The full routine floor was not run because the required live matrix did not pass. No CI result is claimed.

## Lifecycle, security, compatibility, and findings

- The new snapshot exposes only pool counts, pending-build counts, bounded
  failure/pause state, coordinator counters, and LeaseSet presence. It includes
  no Destination IDs, keys, type-5 contents, payloads, or control credentials.
- Build/publication queue bounds, retry deadlines, pool failure threshold,
  cancellation paths, persistent formats, and wire behavior were not changed.
- The reverse server is never reported published from a committed control
  generation or from queue insertion. The live handoff still failed at
  `MissingLeaseSet`.
- No protocol transcript, credential flow, support inventory, or advertisement
  changed. No ELS2 interoperability claim is made.
- **Medium — post-start server provisioning remains absent.** A real control
  generation is visible and the server commits, but after the bounded wait the
  runtime has zero pool registrations and no LS2. The clean run did not have
  the new counters needed to localize the build-owner transition. Plan 388 is
  required before further product changes.
- **Low — external lane instability.** One later run had an i2pd segmentation
  fault before driver execution. This does not discharge any product row.
- **Low — diagnostic evidence digest drift.** The predecessor runner modified
  `results.tsv` after recording its digest. Plan 387 corrected packaging for
  future evidence and added a mutation that detects removal of package-file
  verification; the retained failed artifact remains byte-for-byte unchanged.

## Unblock audit and disposition

| Future plan | Remaining dependencies | Can unblock now? | Disposition |
|---|---|---|---|
| 374 | Complete i2pd bidirectional rows, including reverse NONE/PSK/DH | No | remains blocked on the server pool/LS2 failure |
| 375 | Java pinned-source proof and Java live matrix | No | independently blocked; unaffected by i2pd evidence |
| 377 | Plans 374 and 375 passed | No | remains blocked on both inputs |
| 378 | Plan 377 passed | No | remains blocked; no full-conformance claim |
| 386 | Reverse publication availability | No | remains blocked pending a real router-backed LS2; Plan 388 owns the dependency |
| 387 | Pool/LeaseSet corrective | No | blocked by unresolved build-submission/completion transition |
| 388 | Plans 380/381 contracts and 386/387 diagnostic evidence | Yes | registered ready to localize and correct the pool transition |

No ELS2 qualification/convergence plan is unblocked. Plan 388 is the only newly
ready follow-up. Type-5 advertisement stays false and `full-proposal-conformant`
remains unset.
