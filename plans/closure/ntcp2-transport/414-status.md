# Plan 414 status: stopped — forward loopback attempt failed without stage evidence

Closure token: `stopped-forward-attempt-failed-with-unclassified-stage`.

Plan: `plans/implementation/ntcp2-transport/414-stock-i2pd-decoded-message-observation.md`.
Implementation commit: `5321e03` — add stock i2pd decoded message observer.

## Scope and disposition

Plan 414 tested whether pristine pinned i2pd's debug log could provide a decoded
inbound I2NP observation without changing reference source. The source audit
confirmed that the type-10 handler log occurs downstream of NTCP2 AEAD
verification, NTCP2 I2NP block bounds checking, `FromNTCP2()`, and
`I2NPMessagesHandler::PutNextMessage`. The direct helper starts the NTCP2
transport with SSU2 disabled and does not start i2pd's tunnel manager. The
logger writes to the helper-owned `data_dir/i2pd.log`.

A bounded adapter and helper wait were added. They require exactly one
post-baseline NTCP2 I2NP block and one type-10 handling line, and the adapter
emits only counts, direction, pin, and a raw-log digest. Parser self-tests pass.

The one allowed forward attempt was started by the Plan 414 runner. It returned
exit code 2 with the sanitized result `wire-process-rejected`. The runner's
failure path removed the attempt tree before preserving per-process exit
status, typed events, or the log digest. Therefore the attempt cannot be
classified as a protocol rejection, a successful delivery, or a precise
environmental failure. It is an unclassified harness failure. The attempt
budget is spent; no second forward attempt or reverse attempt was run.

Plan 414 is stopped. Plan 434 remains blocked because the required two-way
authenticated DeliveryStatus evidence was not produced. A corrective runner
plan must preserve sanitized failure evidence before another wire attempt.
Normal-daemon NTCP2 remains disabled and non-advertised under Plan 101. No
support or conformance claim changed.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Exact source-to-log path at pinned i2pd 2.61.0 | Verified by source inspection at `NTCP2.cpp` `HandleReceived` / `ProcessNextFrame`, and `I2NPProtocol.cpp` `PutNextMessage` / `HandleI2NPMessage`. |
| No patched or instrumented reference | Preserved. The helper was rebuilt against pristine source revision `635b013a612ff47278ef02acf8580a28e10e26c5`; `I2PD_INTEROP_OBSERVER` was not enabled. |
| Bounded source observer and sanitized post-run parser | Implemented in commit `5321e03`. Both no-process self-tests passed. |
| Forward authenticated DeliveryStatus receipt | Not established. The sole attempt returned `wire-process-rejected`; per-process stage evidence was lost. |
| Reverse authenticated DeliveryStatus receipt | Not run. Stopped after the forward runner failure. |
| Raw identity, logs, state cleanup | The runner removed its temporary attempt tree in its `finally` path. No raw logs or identity material were committed. This cleanup also erased diagnostic details needed to classify the failure. |
| Router daemon posture and support claims | Unchanged. |

## Pin and artifact evidence

- i2pd version and source revision: `2.61.0`,
  `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Pinned source tree SHA-256:
  `dbffcb2960766cf07cc87a5a56377472122ae577832f2ff5dfaa51611eb82f98`.
- Helper source and binary digests are captured in the ignored build manifest
  at `target/interop/plan410-i2pd/build-manifest.txt`; they are local build
  artifacts, not committed evidence. The rebuilt helper source SHA-256 is
  `3cdbc969bed546ecb3b3d4ab56a7f226b10052c382a7389fb9ec2e27e50b4c65`, the
  helper binary SHA-256 is
  `cdb1a5325e20f9d53acb0fd44ed0fad24039b04f698a18dfc67f9f6b988f5783`, and
  the build-manifest SHA-256 is
  `361a3cfeaf5d3c69bb25f8e554c2c52a84c3482383a91b3f754952361b912b0d`.
- No attempt-log digest is available because the failed runner path deleted the
  log before invoking the sanitizer.

## Commands and outcomes

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-interop --all-targets` — passed, 32 tests.
- `rtk cargo build --locked -p i2pr-interop` — passed.
- `rtk bash tools/i2pr-interop/reference/i2pd-current/build.sh --i2pd-source-dir target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 --output-dir target/interop/plan410-i2pd` — passed; rebuilt the helper against pristine pinned libraries.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test` — passed; no reference process started.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --self-test` — passed, including missing, malformed, duplicate, oversized, and missing-context cases.
- `rtk python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed.
- `rtk python3 scripts/check-tooling-inventory.py` — passed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `rtk bash scripts/check-ntcp2-vectors.sh` — passed.
- `rtk bash scripts/check-ntcp2-interoperability.sh` — passed as a historical-boundary check only.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests, run before the final Plan 414 closure edit.
- Plan 414 forward runner invocation — exited 2 and emitted only
  `{"schema":"i2pr-plan414-evidence-v1","result":"rejected","reason_code":"wire-process-rejected"}`.
- Exact invocation:
  `rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --launcher target/debug/i2pr-interop --driver target/interop/plan410-i2pd/i2pd-current-ntcp2-driver --build-manifest target/interop/plan410-i2pd/build-manifest.txt --observer tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --work-parent target/interop --evidence plans/closure/ntcp2-transport/414-evidence.json`
- Full workspace routine floor — not run; no production crate changed.

## Findings

- **Medium — failure evidence lost.** The runner removes its owned attempt state
  on failure but does not first write sanitized per-process outcomes and
  digests. This prevents the next owner from distinguishing an i2pd helper
  failure from an i2pr launcher failure or reading the bounded reason code.
  The corrective work must preserve a sanitized failure record outside the
  temporary state before cleanup and negative-test every failure path.

No protocol defect or protocol success is inferred from this attempt.

## Unblock audit and roadmap disposition

Plan 434 remains blocked. Plan 435 remains blocked on Plans 433 and 434.
Plans 433 and 436 remain blocked on Plan 431, which stopped at the unavailable
authorized independent non-loopback topology. Plans 437–439 remain blocked by
their recorded predecessor requirements. Plan 414 is stopped and requires a
new bounded corrective before further wire attempts. No core-router recovery
successor became ready as a result of this closure.

