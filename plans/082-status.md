# Plan 082 status: i2pr state preparation and runner contract correction

## Status

Implemented as a pre-protocol launcher and harness correction on 2026-08-01.
This record does not claim NTCP2 interoperability and does not authorize Plans
083, 084, or 079.

The active sequence remains Plan 082 → Plan 083 → Plan 084. Plan 078/080 is
retained as historical evidence of a pre-protocol stop, not a protocol defect.

## Delivered correction

- `i2pr-interop ntcp2 prepare` creates or reloads the existing identity,
  NTCP2 static material, and signed endpoint-bound RouterInfo without opening a
  listener or dialing a peer.
- The command emits only the bounded `i2pr-interop-state-prepared-v1` result
  and fixed rejection categories.
- `I2prAdapter.prepare_state()` validates the record, exact RouterInfo bytes,
  endpoint, digest, and Router Hash before live rendering.
- The mixed runner freezes a canonical `i2pr-minimal-run-identity-v1` record,
  supplies real Plan 065 hashes and correlation fields, and no longer creates
  a `-gen` live scenario.
- Pre-protocol failures retain their bounded stage categories; live process
  counters no longer fabricate starts with a fallback value.

No i2pd wire run, TCP connection, NTCP2 handshake, authenticated frame, or
I2NP DeliveryStatus result was attempted by this correction.

## Validation

```text
cargo fmt --all --check                         passed
cargo check -p i2pr-interop                     passed
cargo test -p i2pr-interop                      passed (22)
python3 -m unittest ... -p 'test_i2pr_prepare.py' passed (3)
python3 -m unittest ... -p 'test_plan082.py'      passed (3)
python3 -m unittest ... -p 'test_harness.py'      passed (111)
python3 -m unittest ... -p 'test_plan065.py'      passed (29)
direct ntcp2 prepare + inspect smoke             passed
```

The full repository gates and boundary checks are run before commit. Any host
or reference-cache blocker remains typed and cannot become protocol evidence.

## Handoff

Plan 083 may begin only from this committed correction and must run the single
minimal `i2pr → i2pd` probe. It must not treat this status record or the local
preparation smoke as a protocol result.
