# Plan 415 status: stopped — forward NTCP2 peer authentication did not complete

Closure token: `stopped-forward-reference-helper-failed-on-peer-control-connect-timeout`.

Plan: `plans/implementation/ntcp2-transport/415-failure-safe-current-pin-ntcp2-observation.md`.
Implementation commit: `ebee07a0d7362e5114195f55de973b35b3376d5c` — preserve sanitized NTCP2 attempt outcomes.

## Scope and disposition

Plan 415 corrected the Plan 414 runner so process stages, fixed failure categories,
bounded counts, and return codes are sanitized and atomically written outside
the temporary attempt tree before cleanup. It added no-process failure controls
for evidence redaction, malformed stage files, start/readiness failures,
nonzero child exits, observer rejection, evidence-write failure, and cleanup
failure. The i2pd reference source remained pristine at the pinned i2pd 2.61.0
revision. NTCP2 remains disabled and non-advertised in the normal daemon.

All pre-attempt gates passed. The single permitted forward attempt then failed:
the helper reported `control-listening-peer-connect-timeout` (return code 66),
the launcher reported `receiver-frame-read-failed` (return code 2), and the log
observer rejected the log because it saw no decoded DeliveryStatus. Sanitized
event stages and the process codes were retained in
[`415-evidence.json`](415-evidence.json). Attempt-tree cleanup passed. The
first-failure rule stopped the plan; reverse was not run. No authenticated
DeliveryStatus receipt or protocol success/failure is inferred.

Plan 415 is stopped under its stated acceptance rule. Plan 416 is registered to
trace the exact receiver/authentication stage and make a source-owned correction
only if the discrepancy is localized. Plans 433–439 remain gated on their
recorded prerequisites. Plan 431 still lacks an authorized independent
non-loopback reference topology; Plan 434 lacks authoritative two-way
authenticated I2NP evidence.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Atomic sanitized evidence before cleanup | Passed. The forward attempt record was written outside the disposable work tree and contains only the pin, direction, fixed stage/reason codes, bounded count fields, and process/cleanup outcomes. |
| Raw output, identities, message IDs, and event payloads excluded | Passed. The retained evidence has no raw stdout/stderr, router hashes, message IDs, paths, raw logs, or log/event digests. |
| Failure controls without router processes | Passed. Runner self-test covers malformed stage input, redaction, readiness/start failure categories, helper/launcher nonzero exits, observer rejection, evidence-write failure with cleanup, and cleanup failure. |
| Pinned i2pd provenance | Passed. Revision `635b013a612ff47278ef02acf8580a28e10e26c5`; source tree SHA-256 `dbffcb2960766cf07cc87a5a56377472122ae577832f2ff5dfaa51611eb82f98`. No reference source was modified. |
| Forward authenticated DeliveryStatus | Failed to establish. Helper stage ended at `control-listening-peer-connect-timeout`; launcher ended at `receiver-frame-read-failed`; observer count was not one. |
| Reverse attempt | Not run, as required after the failed forward attempt. |
| Temporary state and log cleanup | Passed. Runner reported `cleanup_result=passed`; no `plan415-*` directory remained under the work parent. |
| Product posture | Unchanged. No production router code, wire format, dependency, configuration default, bind, or advertisement changed. |

## Commands and outcomes

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-interop --all-targets` — passed, 32 tests.
- `rtk cargo build --locked -p i2pr-interop` — passed.
- `rtk bash tools/i2pr-interop/reference/i2pd-current/build.sh --i2pd-source-dir target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 --output-dir target/interop/plan410-i2pd` — passed; rebuilt helper against pristine pinned libraries.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test` — passed, no reference process started.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --self-test` — passed.
- `rtk python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed.
- `rtk bash scripts/check-ntcp2-vectors.sh` — passed.
- `rtk bash scripts/check-ntcp2-interoperability.sh` — passed as the historical Plan 099 boundary check only.
- `rtk python3 scripts/check-tooling-inventory.py` — passed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests. The test emitted its expected temporary-fixture notice that the synthetic repository has no `.github/workflows` directory.
- `rtk git diff --check` — passed.
- Forward runner command exited 2 with the sanitized stage evidence in `415-evidence.json`; no reverse invocation was made:

  ```text
  rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --launcher target/debug/i2pr-interop --driver target/interop/plan410-i2pd/i2pd-current-ntcp2-driver --build-manifest target/interop/plan410-i2pd/build-manifest.txt --observer tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --work-parent target/interop --evidence plans/closure/ntcp2-transport/415-evidence.json
  ```

- Full workspace routine floor — not run; this plan changed only non-production qualification tooling and planning artifacts.

## Security, compatibility, and findings

- No dependencies changed. Raw process output is captured only in memory and discarded. Attempt log digests are not retained. Evidence output is allowlisted and atomically replaced with mode `0600`.
- **Medium — NTCP2 loopback authentication remains unresolved.** TCP connected, but the reference helper did not observe its expected authenticated peer and the launcher could not read the receiver frame. The evidence localizes the failure stage but does not establish whether the cause is runner configuration, an i2pr transport defect, or an interop discrepancy. Plan 416 owns source tracing; reference patching and another attempt without a localized correction remain prohibited.
- No protocol support, capability, or anonymity claim changed.

## Unblock audit and roadmap disposition

- Plan 434 remains blocked pending authoritative two-way authenticated I2NP evidence; Plan 415 did not satisfy that gate.
- Plan 435 remains blocked on Plans 433 and 434.
- Plan 433 remains blocked because Plan 431 stopped without the authorized independent non-loopback topology; Plan 432 passed.
- Plan 436 remains blocked on Plan 433; Plan 437 on Plans 433 and 436; Plan 438 on Plan 437; Plan 439 on Plans 433, 435, 436, and 438.
- Proposal 170 Plans 412/413 remain blocked on Plan 437. No plan in another work line became dependency-ready from this closure.
- Plan 416 is registered as the bounded corrective successor for the newly classified helper/receiver stage. No blocked successor was silently unblocked.

