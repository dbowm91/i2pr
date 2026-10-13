# Plan 476 status amendment — pair-bound offline receipts

This amendment records a fail-closed improvement to the Plan 476 offline
receipt contract. It does not complete the local router-process/session
executor or live qualification.

## Source change

The offline authorization now validates the concrete RFC1918 IPv4 pair,
distinct host addresses and UDP ports, same subnet, interface digests, and
network ID. Every receipt binds its claimed peer RouterInfo digest to the
other host's authorized RouterInfo. Reconciliation now requires one complete,
strictly ordered phase sequence for each role and nonzero observations for
authenticated-session, request, and reply phases. Unknown roles, missing
phases, replayed phases, wrong peer artifacts, and zero product observations
fail closed.

The fake adapter remains in-process and transport-free. Successful simulation
continues to report `live_qualification=not-run`.

## Verification

| Command | Outcome |
| --- | --- |
| `python3 tests/integration/ssu2/test_plan476_executor.py` | Passed (4 tests; positive sequence plus authorization and receipt negative matrices) |
| `python3 tests/integration/ssu2/plan472_live.py --self-test` | Passed (2 positive / 8 negative cases) |
| `python3 scripts/plan449-ssu2-preflight.py --self-test` | Passed (8 positive/negative cases) |
| `git diff --check` | Passed |

## Disposition

Plan 476 remains open. The checks strengthen simulated pair binding but do not
start a process, observe an authenticated session, or establish an I2NP
effect. The per-host local process owner and attributable source receipts are
still required for Tier A. Tier B remains unavailable until an independently
addressed, explicitly authorized host pair is supplied.
