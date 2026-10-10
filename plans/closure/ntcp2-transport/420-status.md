# Plan 420 status: stopped — RouterInfo accepted, no DeliveryStatus

Closure token: stopped-router-info-accepted-without-delivery-status.

Plan: plans/implementation/ntcp2-transport/420-post-validation-session-progress-observation.md.
Implementation commit: df581a5 — observe NTCP2 post-validation session progress.

Plan 420 added count-only observation for the pinned stock SessionConfirmed
from marker and the stock NTCP2 session-termination marker. The source mapping
is pinned i2pd 2.61.0, libi2pd/NTCP2.cpp: the received marker is logged at the
start of ProcessSessionConfirmed; the accepted RouterInfo marker is emitted in
EstablishSessionAfterSessionConfirmed after the decrypted payload is parsed
and RouterInfo::IsUnreachable() is false. The termination marker is emitted by
NTCP2Session::Terminate. The parser stores only counts; test endpoint and
Router Hash strings were absent from sanitized output.

All required focused checks passed. The single forward attempt recorded one
SessionRequest, one SessionConfirmed receive marker, one post-validation
RouterInfo-accepted marker, and one session-termination marker. All selected
validation-rejection counters were zero. Decrypted-frame, I2NP-block, and
DeliveryStatus counts were zero. The helper exited 66 with
control-listening-peer-connect-timeout; the launcher exited 2 with
receiver-frame-read-failed; cleanup passed. Reverse was not run.

The termination count is process-wide and the permitted evidence intentionally
does not retain identity or endpoint values, so it cannot be correlated with
the accepted RouterInfo marker. No authenticated connected-link or I2NP
delivery claim follows from this attempt. No actionable implementation defect
was localized in this bounded forward diagnostic. Plan 421 then exposed a
reverse-only scenario identity mismatch before wire; Plan 422 is registered to
correct and test that mapping and spend the reverse direction once. Plans
433–439 remain blocked on their recorded dependencies; normal-daemon NTCP2
remains disabled and non-advertised.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Exact post-validation and termination markers | Passed; counters match source-verified stock marker prefixes. |
| Count-only evidence and redaction | Passed; endpoint and Router Hash fixture values do not appear in observer output. |
| Observer, runner, checker controls | Passed; marker, baseline, malformed-input, redaction, and fail-closed mutation checks passed. |
| Pinned helper and launcher build | Passed; helper rebuilt against pristine i2pd 2.61.0 and i2pr-interop built. |
| Forward attempt | Rejected; RouterInfo accepted count 1, termination count 1, no I2NP block or DeliveryStatus. |
| Reverse attempt | Not run after forward failure. |
| Cleanup | Passed; raw/private attempt state removed, sanitized evidence retained. |

## Commands and outcomes

- Observer self-test, runner self-test, checker, checker self-test — passed.
- cargo fmt --all --check — passed.
- cargo test --locked -p i2pr-interop --all-targets — passed, 32 tests.
- cargo build --locked -p i2pr-interop — passed.
- Pinned helper rebuild — passed against revision 635b013a612ff47278ef02acf8580a28e10e26c5.
- NTCP2 vectors and historical boundary, tooling inventory, plan uniqueness, 51 planning tests, and git diff --check — passed.
- Forward attempt using run_plan414.py with the pinned helper, launcher, observer, and target/interop work parent — exited 2; reverse not run.
- Full workspace routine floor — not run; qualification tooling and planning artifacts only.

## Findings and unblock audit

- **Medium — no I2NP data phase was observed.** RouterInfo validation progressed past the stock rejection point, but no I2NP block or DeliveryStatus appeared before timeout. The process-wide termination marker cannot be linked to the same session without retaining prohibited identity-bearing log data.
- Plan 434 remains blocked on authenticated two-way I2NP evidence. Plan 433 remains blocked because Plan 431 stopped without an authorized independent non-loopback topology. Plans 435–439 remain blocked by the dependencies recorded in the core-router roadmap.
- Proposal 170 Plans 412/413 remain blocked on Plan 437. No other registered work line is dependency-ready.
