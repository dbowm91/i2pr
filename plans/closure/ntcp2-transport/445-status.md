# Plan 445 status: blocked — stock-to-stock NTCP2 control not established

Closure token: `blocked-stock-to-stock-control-helpers-not-established`

Plan: `plans/implementation/ntcp2-transport/445-current-pin-control-runner-attribution-corrective.md`

## Implementation and execution

- `d160806a26c9a39bf9f6452f28b63120ef50df0e` — added a sanitized terminal
  reason projection and self-tests to the Plan 441 stock-control runner,
  registered Plan 445, and recorded Plan 441's blocked outcome.
- The existing pinned helper was built from i2pd `2.61.0` at
  `635b013a612ff47278ef02acf8580a28e10e26c5`; no reference source changes
  were made.

Plan 445 spent its single stock-to-stock control attempt using two helper
processes with distinct persisted identities/data roots, loopback endpoints,
network ID 2, and external reseed disabled. The runner now captures only the
bounded terminal category from each process. Both helper roles reported
`control-*-session-not-established`; no authenticated connection, decoded I2NP,
or response exchange passed. The disposable identity roots were removed.

The stock positive control failed, so the runner did not proceed to any
i2pr↔i2pd attempt. Plan 445's live budget is spent.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Retain a fixed sanitized terminal category | Pass. Receipt reports `control-dialing-session-not-established` and `control-listener-session-not-established`; self-test rejects path-like detail. |
| Prove stock-to-stock topology before i2pr attribution | Blocked. Both pristine helper roles failed to establish a session. This does not identify the failure as an i2pr defect. |
| Require authenticated link, decoded I2NP and clean process completion | Not met. Helpers returned exit 66 before the positive control reached these acceptance points. |
| Run Plan 441 i2pr forward/reverse directions only after control | Not run, correctly gated by the failed control. |
| Keep normal/public NTCP2 disabled | Preserved; no daemon, RouterInfo, support, or config change. |

## Commands and outcomes

Executed locally on Linux:

- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan441_stock_control.py --self-test` — passed, including terminal-reason allowlisting and positive/negative receipt controls.
- `python3 -m py_compile tools/i2pr-interop/reference/i2pd-current/run_plan441_stock_control.py` — passed.
- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan441_stock_control.py --driver /tmp/i2pd-plan441/i2pd-current-ntcp2-driver --manifest /tmp/i2pd-plan441/build-manifest.txt --launcher target/debug/i2pr-interop --work-dir /tmp --evidence /tmp/i2pd-plan441/stock-control-445.json` — exited 1; `environment-blocked`; both roles exited 66 with the categories above; cleanup `removed`.
- The exact-pin helper build and `cargo build --locked -p i2pr-interop` passed under Plan 441 and the resulting binaries were reused without source changes.
- `git diff --check` — run before committing this closure.

No hosted CI result is claimed. Raw logs and temporary identities were removed;
only the sanitized receipt remains at `/tmp/i2pd-plan441/stock-control-445.json`,
outside the repository.

## Security, compatibility, and findings

- **High — control topology still cannot establish a stock NTCP2 session.** The
  source-pinned helper setup is available, but the observed session futures were
  not established. Without a passing stock positive control, i2pr protocol
  behavior cannot be attributed. Do not retry or tune the wire under this
  spent budget.
- The Plan 441 runner evidence defect is corrected, but that correction did not
  make the stock-to-stock control pass. Plan 441 therefore remains blocked.
- No production files, key formats, config, external reference source, or
  support claims changed. No dependency changes.
- No i2pr wire attempt or public-network test was run.

## Unblock audit and disposition

Plan 445 is blocked at the stock positive control. Plan 441 remains blocked;
Plan 445 did not satisfy its full acceptance or supersede the Plan 441 status.
Plans 434 and 435 remain gated on the authenticated-link/product prerequisites.
Plans 442–444 are unaffected and remain dependency-ready. No other blocked plan
had all registered prerequisites closed. A new attempt requires a source-based
fixture diagnosis and a new plan-of-record; do not extend Plan 410–424's stopped
micro-diagnostic sequence.

No capability, conformance, reachability, or anonymity claim is promoted.
