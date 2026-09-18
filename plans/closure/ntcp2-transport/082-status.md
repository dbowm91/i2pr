# Plan 082 status: i2pr state preparation and runner contract correction

## Status

Implemented as a pre-protocol launcher and harness correction on 2026-08-04.
This record does not claim NTCP2 interoperability and does not authorize Plans
083, 084, or 079.

The active sequence remains Plan 082 → Plan 083 → Plan 084. Plan 078/080 is
retained as historical evidence of a pre-protocol stop, not a protocol defect.

## Delivered correction

- `i2pr-interop ntcp2 prepare` creates or reloads the existing identity,
  NTCP2 static material, and signed endpoint-bound RouterInfo without opening a
  listener or dialing a peer. The bounded stdout record is the
  `i2pr-interop-state-prepared-v1` schema with fixed rejection reasons
  (`prepare_input_invalid`, `prepare_state_path_invalid`,
  `prepare_router_info_verify_failed`).
- `i2pr-interop ntcp2 validate-scenario --scenario-config <path>` parses the
  strict Plan 065 live scenario through the same `Scenario::load` path used by
  the listener/dial commands and emits the
  `i2pr-interop-scenario-validated-v1` result. The command opens no socket,
  starts no process, and writes no state directory.
- `I2prAdapter.prepare_state()` validates the record, exact RouterInfo bytes,
  endpoint, digest, and Router Hash before live rendering. The adapter parses
  exactly one preparation line from the bounded log, rejects zero digests and
  bounded-allowlist reasons, and never fabricates a process start.
- `I2prAdapter.validate_scenario()` invokes the Rust `validate-scenario`
  command via the selected `ProcessPlacement`, parses exactly one record, and
  fails closed with `live-scenario-render-failed`.
- The mixed runner freezes a canonical `i2pr-minimal-run-identity-v1` record,
  asserts the frozen digest against the on-disk record, supplies real Plan 065
  hashes and correlation fields, validates both RouterInfos and distinct
  Router Hashes before live rendering, and no longer creates a `-gen` live
  scenario.
- Pre-protocol failures retain their bounded stage categories
  (`i2pr-state-preparation-failed`, `i2pr-preparation-record-invalid`,
  `i2pr-router-info-missing`, `i2pr-router-info-validation-failed`,
  `i2pr-router-hash-invalid`, `reference-state-preparation-failed`,
  `reference-router-info-missing`,
  `reference-router-info-validation-failed`,
  `reference-router-hash-invalid`, `run-identity-freeze-failed`,
  `live-scenario-render-failed`, `listener-process-start-failed`,
  `dialer-process-start-failed`). Live process counters no longer fabricate
  starts with a fallback value.
- The reference adapter's `process-start-failed` exception now collapses to
  `listener-process-start-failed` (i2pr initiator path) or
  `dialer-process-start-failed` (i2pr responder path) instead of the broad
  `typed-harness-operation-failed`.

No i2pd wire run, TCP connection, NTCP2 handshake, authenticated frame, or
I2NP DeliveryStatus result was attempted by this correction.

## Validation

```text
cargo fmt --all --check                         passed
cargo check -p i2pr-interop                     passed
cargo test -p i2pr-interop                      passed (22)
python3 -m unittest ... -p 'test_i2pr_prepare.py' passed (5)
python3 -m unittest ... -p 'test_plan082.py'      passed (7)
python3 -m unittest ... -p 'test_harness.py'      passed
python3 -m unittest ... -p 'test_plan065.py'      passed
direct ntcp2 prepare + validate-scenario + inspect smoke passed
```

The full repository gates and boundary checks are run before commit. Any host
or reference-cache blocker remains typed and cannot become protocol evidence.

## Handoff

Plan 083 may begin only from this committed correction and must run the single
minimal `i2pr → i2pd` probe. It must not treat this status record or the local
preparation smoke as a protocol result.
