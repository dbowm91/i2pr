# Plan 419 status: stopped — no SessionConfirmed rejection marker or DeliveryStatus

Closure token: `stopped-session-confirmed-marker-without-rejection-or-delivery`.

Plan: `plans/implementation/ntcp2-transport/419-session-confirmed-rejection-stage-observation.md`.
Implementation commit: `e93ec2a` — add bounded NTCP2 rejection-stage counters.

Plan 419 extended the sanitized stock-log observer with fixed counters for
SessionConfirmed AEAD/KDF, block framing, RouterInfo verification/freshness,
NetDB import, address, host, version, and static-key rejection markers. The
observer retains counts only. Its self-test exercised all selected markers,
baseline exclusion, and redaction of endpoint, port, and timestamp values.

The required source guard, observer and runner self-tests, focused interop
tests, helper rebuild against pristine i2pd 2.61.0, NTCP2 vector and historical
boundary checks, tooling inventory, plan uniqueness, planning tests, and
`git diff --check` passed. The one forward attempt retained one SessionRequest
and one SessionConfirmed receive marker. Every selected rejection counter,
decrypted-frame count, I2NP-block count, and DeliveryStatus count was zero.
The helper exited 66 with `control-listening-peer-connect-timeout`; the
launcher exited 2 with `receiver-frame-read-failed`; cleanup passed. Reverse
was not run. The evidence does not establish authentication or a connected
peer.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Fixed source-verified marker allowlist | Passed; counters cover the selected SessionConfirmed rejection branches in pinned `NTCP2.cpp`. |
| Post-baseline bounded parsing | Passed; only allowlisted counters enter evidence, dynamic marker text is discarded. |
| Observer and runner mutation controls | Passed; marker fixtures, baseline exclusion, malformed inputs, redaction, and source-check mutations passed. |
| Pristine helper and launcher build | Passed; i2pd 2.61.0 pinned helper rebuilt, interop crate build passed. |
| Forward attempt | Rejected; helper 66 / launcher 2, all rejection markers and DeliveryStatus count zero. |
| Reverse attempt | Not run after forward failure. |
| Cleanup | Passed; sanitized evidence retained, private attempt files removed. |

## Commands and outcomes

- `rtk python3 tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --self-test` — passed.
- `rtk python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test` — passed.
- `rtk python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed.
- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-interop --all-targets` — passed, 32 tests.
- `rtk cargo build --locked -p i2pr-interop` — passed.
- Pinned helper rebuild with `tools/i2pr-interop/reference/i2pd-current/build.sh` — passed against revision `635b013a612ff47278ef02acf8580a28e10e26c5`.
- NTCP2 vectors, historical interoperability boundary, tooling inventory, global plan-number uniqueness, 51 planning tests, and `git diff --check` — passed.
- Forward attempt: `run_plan414.py` with launcher `target/debug/i2pr-interop`, pinned driver/build manifest, observer, `target/interop`, and evidence output `plans/closure/ntcp2-transport/419-evidence.json` — exited 2; reverse not run.
- Full workspace routine floor — not run; this plan changed qualification tooling and planning artifacts only.

## Findings and unblock audit

- **Medium — handshake progress remains unresolved.** Neither an allowlisted validation rejection nor an authenticated DeliveryStatus was observed. Plan 420 is registered to count the first post-validation stock marker and the session termination marker; it gets one new forward attempt only.
- Plan 434 remains blocked on authenticated two-way I2NP evidence. Plans 433 and 435–439 remain blocked on their registered prerequisites. Proposal 170 Plans 412/413 remain blocked on Plan 437.
- Normal-daemon NTCP2 remains disabled and non-advertised. No support claim changed.
