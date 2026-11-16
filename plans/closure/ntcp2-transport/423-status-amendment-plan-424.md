# Plan 423 status amendment: responder I/O operation was not identified

This amendment corrects the operation attribution in
[`423-status.md`](423-status.md), preserving its attempt, result, and closure
record. A source audit of the runtime driver showed that its `await_confirmed`
phase contains both a SessionCreated write and a SessionConfirmed read. The
Plan 423 terminal code `responder_session_confirmed_part1_io_failed` does not
identify which operation failed, and `ExactIoError.kind` was not serialized
into the sanitized status. Therefore the earlier “read closed” wording was
more specific than the retained evidence supports.

The authoritative Plan 423 outcome is: the reverse attempt reached TCP; the
responder failed with a bounded I/O error during `await_confirmed`; the helper
reported `control-dialing-session-not-established`; and the sanitized helper
stage counters were zero. There is no shared session token establishing that
these two terminal outcomes have a common cause. Cleanup passed and no I2NP
block or DeliveryStatus was observed.

The discrepancy is owned by [Plan 424](../../implementation/ntcp2-transport/424-reverse-responder-io-operation-attribution.md),
which preserves the exact responder I/O operation and `IoErrorKind` and
spends one reverse-only attempt. It does not authorize a forward rerun or
protocol change.

Plan 423 remains stopped. Plan 434 remains blocked on authenticated
bidirectional NTCP2 and correlated I2NP delivery. Normal-daemon NTCP2 remains
disabled and non-advertised.
