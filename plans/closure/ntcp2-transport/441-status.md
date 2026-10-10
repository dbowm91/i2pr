# Plan 441 status: blocked — stock-control result lacks terminal attribution

Closure token: `blocked-stock-control-runner-did-not-retain-terminal-category`

Plan: `plans/implementation/ntcp2-transport/441-current-pin-single-session-control-and-interop-recovery.md`

## Attempt and source baseline

The current-pin i2pd 2.61.0 source cache was `/tmp/i2pd-plan401` at the
required revision `635b013a612ff47278ef02acf8580a28e10e26c5`, with no tracked
source changes. The pristine library/helper build completed under
`/tmp/i2pd-plan441`. The new control runner used two independent loopback
process roots and identities, network ID 2, and no reseed endpoints.

One stock-to-stock control attempt launched both helper roles. Both processes
exited 66; the sanitized receipt recorded `environment-blocked`, both roles as
rejected, and four stages per role. The runner discarded each helper's bounded
`terminal_rejected.reason_code`, so the actual first failing phase is
unavailable. The result is **not** evidence of an NTCP2 protocol rejection.
No i2pr↔i2pd wire attempt was made after the failed positive control.

The attempt budget is spent. No second stock-control or i2pr wire attempt is
authorized under Plan 441.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Use exact pinned stock source and pristine helper | Build passed for i2pd `2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`; no source patch was used. |
| Establish a stock-to-stock positive control before i2pr attribution | Blocked. Both helpers exited 66, but the missing terminal category prevents localization. |
| Preserve a session-specific, sanitized receipt | Incomplete. Receipt retained only fixed role outcomes and stage counts; it did not retain the available bounded rejection category. |
| Run forward/reverse i2pr NTCP2 after control | Not run, correctly gated by the failed positive control. |
| Keep normal/public NTCP2 disabled | Preserved. No daemon, RouterInfo support, or configuration change was made. |

## Commands and outcomes

Executed locally on Linux:

- `bash tools/i2pr-interop/reference/i2pd-current/build.sh --i2pd-source-dir /tmp/i2pd-plan401 --output-dir /tmp/i2pd-plan441` — passed; built the pinned static libraries and pristine helper.
- `cargo build --locked -p i2pr-interop` — passed.
- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan441_stock_control.py --self-test` — passed for the event acceptance oracle before the live attempt.
- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan441_stock_control.py --driver /tmp/i2pd-plan441/i2pd-current-ntcp2-driver --manifest /tmp/i2pd-plan441/build-manifest.txt --launcher target/debug/i2pr-interop --work-dir /tmp --evidence /tmp/i2pd-plan441/stock-control.json` — exited 1; result `environment-blocked`; both helper exits 66; terminal categories absent.
- `python3 scripts/check-current-pin-ntcp2-runner.py --self-test` — passed for its existing Plan 410–424 source surface.
- `python3 -m py_compile tools/i2pr-interop/reference/i2pd-current/run_plan441_stock_control.py` — passed.

No hosted CI result is claimed. The helper's raw private logs and temporary
identity roots were removed. The persisted receipt at
`/tmp/i2pd-plan441/stock-control.json` contains no paths or identities, but is
outside the repository and is not the missing terminal evidence.

## Security, compatibility, and findings

- **High — control oracle incomplete.** Without the first fixed terminal
  category, Plan 441 cannot distinguish fixture startup, handshake, or data
  failure. Do not attribute this result to i2pr or NTCP2 protocol behavior.
- The code defect is in the new Plan 441 Python runner: it projects role status
  and stage counts but omits `terminal_rejected.reason_code`. Plan 445 is the
  registered corrective and has a fresh, explicit attempt budget.
- No production code, config, key format, reference source, or support claim
  changed. No dependencies were added.
- No other live attempt was made after the failed control.

## Unblock audit and disposition

Plan 441 remains blocked on the source-evidenced corrective Plan 445. Plan 445
is ready because its hard dependency, Plan 440, is closed. Plans 434 and 435
remain blocked on their original authenticated-link and product prerequisites;
this status does not alter them or any historical closure record.

Plan 441 is blocked, not passed. If Plan 445 satisfies the entire remaining
Plan 441 acceptance, it may supersede this status explicitly; otherwise the
exact residual evidence gate remains open. NTCP2 stays disabled in the normal
daemon and non-advertised.
