# Plan 423 status amendment: reverse responder operation identified

This amendment supersedes the unresolved operation wording in
[`423-status-amendment-plan-424.md`](423-status-amendment-plan-424.md).

Plan 424 added bounded operation attribution and spent one reverse-only
attempt. The responder emitted `responder_session_confirmed_read_closed`,
which identifies a closed read of SessionConfirmed after the SessionCreated
write completed. The pinned helper again reported
`control-dialing-session-not-established`; there is no shared session token,
so the two outcomes are not causally correlated. The sanitized evidence is
[`424-evidence.json`](424-evidence.json).

Plan 423 remains stopped. Its protocol error categorization passed, but the
attempt did not establish an authenticated reverse link or correlated I2NP
delivery. No further attempt or protocol correction is implied.
