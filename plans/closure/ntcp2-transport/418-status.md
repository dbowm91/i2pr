# Plan 418 status: stopped — SessionConfirmed was received but not established

Closure token: `stopped-forward-session-confirmed-received-without-connected-peer`.

Plan: `plans/implementation/ntcp2-transport/418-current-pin-helper-debug-stage-logging.md`.
Implementation commit: `35501ca` — enable private NTCP2 helper debug milestones.

## Scope and disposition

Plan 418 configured the i2pr-owned direct helper to call the pinned logger's
existing `SetLogLevel("debug")` before `Logger().Start()`. The fail-closed
checker now requires exactly one call between `SendTo` and `Start`, and its
self-test rejects both a missing call and an incorrectly ordered call. No
reference source, binary, normal-daemon setting, or production Rust code was
changed.

All pre-attempt gates passed. The one forward attempt recorded one
`SessionRequest received` marker and one `SessionConfirmed received` marker,
with zero decrypted-frame, I2NP-block, and DeliveryStatus counts. The helper
still ended at `control-listening-peer-connect-timeout` (66) and the launcher
at `receiver-frame-read-failed` (2); cleanup passed and reverse was not run.
The source shows that `SessionConfirmed received` is logged before the
SessionConfirmed AEAD, RouterInfo, address, and static-key checks complete.
Therefore the observed marker does not prove that the NTCP2 session was
established. Plan 418's attempt budget is spent and no protocol result is
inferred.

Plan 418 is stopped. Plan 419 is registered to count a bounded set of
source-verified SessionConfirmed rejection markers in sanitized evidence and
spend its own one-forward-attempt budget. Plan 434 remains blocked. Normal
daemon NTCP2 remains disabled and non-advertised.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Helper logger configured before startup | Passed. The call order is `SendTo` → `SetLogLevel("debug")` → `Start`; checker mutations reject missing or post-start configuration. |
| Pristine pinned helper rebuild | Passed against i2pd 2.61.0 revision `635b013a612ff47278ef02acf8580a28e10e26c5`; reference source and binary are untouched. |
| Sanitized post-baseline handshake counts | Passed. Forward counts: SessionRequest received 1; SessionConfirmed received 1; SessionCreated received 0; SessionConfirmed sent 0; SessionRequest/Created/Confirmed AEAD failures 0; decrypted frames 0; I2NP blocks 0; decoded DeliveryStatus 0. |
| Authenticated peer and DeliveryStatus | Not established. The SessionConfirmed marker is before payload validation; helper connected-peer check timed out and launcher read closed. |
| Reverse attempt | Not run after forward failure. |
| Temporary cleanup | Passed (`cleanup_result=passed`); no `plan417-*` directory remained. |
| Product/reference posture | Unchanged; no wire or support claim, daemon activation, bind, or advertisement. |

## Provenance

- i2pd revision: `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Source tree SHA-256: `dbffcb2960766cf07cc87a5a56377472122ae577832f2ff5dfaa51611eb82f98`.
- Helper source SHA-256: `9a0da2e44daac76c0ed68b498ebca664cacd63d9b3c77a34ec15fd6caf98a0ad`.
- Helper binary SHA-256: `655fe54cecaae39ffc466974cd3c405092f28bbb6bf4e6d65cb2775aeed37deb`.
- Sanitized attempt record: [`418-evidence.json`](418-evidence.json). Raw logs and private attempt state were deleted.

## Commands and outcomes

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-interop --all-targets` — passed, 32 tests.
- `rtk cargo build --locked -p i2pr-interop` — passed.
- `rtk bash tools/i2pr-interop/reference/i2pd-current/build.sh --i2pd-source-dir target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 --output-dir target/interop/plan410-i2pd` — passed; helper rebuilt against pristine libraries. Compiler emitted existing unused-parameter warnings from pinned headers.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test` — passed.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --self-test` — passed.
- `rtk python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed.
- `rtk bash scripts/check-ntcp2-vectors.sh` — passed.
- `rtk bash scripts/check-ntcp2-interoperability.sh` — passed as historical Plan 099 boundary check only.
- `rtk python3 scripts/check-tooling-inventory.py` — passed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests; synthetic fixture emitted the expected missing-workflows-directory notice.
- `rtk git diff --check` — passed.
- Forward invocation exited 2 and preserved sanitized evidence; reverse was not run:

  ```text
  rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --launcher target/debug/i2pr-interop --driver target/interop/plan410-i2pd/i2pd-current-ntcp2-driver --build-manifest target/interop/plan410-i2pd/build-manifest.txt --observer tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --work-parent target/interop --evidence plans/closure/ntcp2-transport/418-evidence.json
  ```

- Full workspace routine floor — not run; this plan changed only non-production qualification tooling and planning artifacts.

## Findings and unblock audit

- **Medium — SessionConfirmed processing remains unresolved.** Stock debug evidence reaches the SessionConfirmed receive marker but no I2NP data phase. That marker precedes AEAD and RouterInfo validation, so the rejection point is not yet known. Plan 419 owns fixed-category failure markers; no protocol defect is inferred.
- Plan 434 remains blocked on authoritative two-way authenticated I2NP evidence.
- Plan 433 remains blocked because Plan 431 stopped without an authorized independent non-loopback topology; Plan 432 passed.
- Plan 435 remains blocked on 433/434; Plan 436 on 433; Plan 437 on 433/436; Plan 438 on 437; Plan 439 on 433/435/436/438.
- Proposal 170 Plans 412/413 remain blocked on Plan 437. No other work line became dependency-ready.
- Plan 419 is registered to capture the bounded SessionConfirmed rejection stage. No blocked successor was silently unblocked.

