# Plan 422 — correct reverse current-pin scenario identity mapping

Status: **active** — in-progress-reverse-scenario-identity-correction.
Corrective successor to [Plan 421](../../closure/ntcp2-transport/421-status.md).

## Objective

Correct and negative-test the current-pin runner's reverse scenario identity
mapping, then spend one reverse-only loopback attempt with the pinned i2pd
helper. The reverse i2pr responder's expected I2NP sender is the local i2pr
Router Hash; the expected receiver is the remote i2pd Router Hash, regardless
of which peer initiated the NTCP2 connection.

## Why ready

- Plan 421 stopped before helper startup because the reverse scenario supplied
  the i2pd identity as the expected sender, which the i2pr responder rejected.
- Source traces show the launcher validates its local Router Hash against
  expected_sender_router_hash_sha256 before receiving or sending frames.
- Reverse helper mode, launcher listener mode, loopback profile, and sanitized
  stock-log observer already exist. The attempt budget remains unspent.

## Invariants

1. Use pristine i2pd 2.61.0 at
   635b013a612ff47278ef02acf8580a28e10e26c5; no reference source/binary
   modification.
2. Use only 127.0.0.1, network ID 2, and
   current-network-loopback.
3. Run exactly one reverse-only attempt; do not rerun the failed forward
   direction and do not retry reverse.
4. Retain only count-only stage evidence and bounded outcome codes.
5. Normal-daemon NTCP2 remains disabled and non-advertised.

## Work and acceptance

Change the scenario builder so expected sender and receiver identities are
derived from the actual I2NP roles, independently of NTCP2 initiator/responder
role. Add no-process assertions for both scenario directions and negative
controls that swap or duplicate the identities. Extend the source checker to
guard this mapping. Run focused runner/observer/checker tests, interop tests,
the pinned helper build, NTCP2 vector and historical checks, plan/tooling
checks, planning tests, and git diff --check.

Then run one reverse-only attempt through the launcher and pristine stock
helper. A pass requires correlated authenticated NTCP2 and DeliveryStatus
evidence plus cleanup. Any failure closes Plan 422 as stopped. Plan 434 still
requires the separate successful forward direction and its complete
message/teardown matrix.
