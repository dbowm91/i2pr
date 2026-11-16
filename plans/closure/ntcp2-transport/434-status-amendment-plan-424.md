# Plan 434 status amendment: exact reverse I/O attribution registered

Current disposition token: `blocked-reverse-authenticated-link-and-i2np-unproven`.

This status amendment supersedes the current interpretation in
[`434-status-amendment-plan-423.md`](434-status-amendment-plan-423.md) where it
describes the reverse outcome as a read-closed result. Source review showed
that Plan 423's `await_confirmed` stage includes both writing SessionCreated
and reading SessionConfirmed, while its terminal code retained neither the
specific operation nor the `IoErrorKind`. The reverse I/O operation therefore
remains unknown; the helper's session-not-established result is not correlated
to it by a shared session token.

Plan 434 remains blocked. It still lacks authenticated forward and reverse
NTCP2 and correlated I2NP DeliveryStatus evidence. Plan 424 is now registered
to add bounded operation/kind attribution and spend one reverse-only attempt.
That diagnostic cannot by itself satisfy Plan 434's two-direction acceptance.

No production protocol behavior, reference source, listener policy, or support
claim changes. Plans 433 and 435–439 remain blocked on their registered
dependencies, and Plan 431 remains stopped at the non-loopback topology gate.
