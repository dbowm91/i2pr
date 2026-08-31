# Plan 140 status — SAM 3.1 interoperability and Milestone 7 closure

Status: **`blocked-audit-superseded-by-plan141-corrective-roadmap`**.

> **Current authority:** Plan 140 remains the audit record that discovered the blocked Milestone 7 closure. Its next-action guidance is superseded by [`plans/141-status.md`](141-status.md) and the Plan 142 → 143 → 144 corrective sequence. Historical findings below remain evidence.

Recorded: **2026-08-28**.

Plan 139 is closed. Plan 140 was executed as an audit and local closure
attempt, but Milestone 7 is not closed because the independent-client and
live STREAM acceptance criteria are not satisfied.

## Audit answers

1. **Bounded HELLO:** yes. Every accepted socket enters the same bounded
   `LineReader`/HELLO path and rejects overlong or control-bearing lines.
2. **Version claim:** yes. The server negotiates and reports SAM 3.1 only;
   newer or disjoint ranges fail explicitly.
3. **Permanent raw transition:** no. The listener still keeps the command
   reader after `STREAM CONNECT`/`STREAM ACCEPT`; no live per-stream raw socket
   handoff is installed.
4. **Session ownership:** partial. `SESSION CREATE` transactionally owns one
   destination and control socket, but stream sockets are not attached to a
   live destination delivery task.
5. **Private round trip:** yes for the repository's standard RFC 4648
   representation. `SamPrivateDestination` now consumes its zeroizing buffer
   without cloning it. **Plan 141/142 supersede this as an interoperability
   claim because the SAM wire alphabet itself was implemented incorrectly and
   the private-destination format lacks independent proof.**
6. **M6 product path:** no for a claimed live SAM trajectory. The daemon's
   current success path uses a test-only established-material bridge and
   captures adapter output; it does not deliver that output through the live
   tunnel/router path.
7. **Named bounds:** yes. Session, client, stream, pending-ACCEPT, line,
   forwarding, and raw-copy chunks have explicit ceilings.
8. **Direct shortcut:** no closure evidence may claim this. Existing stream
   tests use the documented capture seam and therefore remain regression
   tests, not independent-client acceptance evidence.
9. **Privacy:** default diagnostics redact private destinations and raw
   application bytes; no payload logging was added.
10. **Unsupported features:** yes. Unsupported styles, version ranges,
    options, DATAGRAM/RAW, SSL forwarding, and non-loopback targets fail with
    typed replies.

## Independent-client evidence

The official discovery source and pinned candidates are recorded in
[`tests/integration/sam/README.md`](../tests/integration/sam/README.md).
`i2plib` imports at its pinned revision, but its normal I2P Base64 spelling
uses `-~` and cannot complete the current server's imported-private
RFC-4648-only path. `txi2p` is not runnable in this environment because its
legacy `ometa` dependency is unavailable. No external client was counted as
passed, and no independent client was counted as having moved STREAM bytes.

Plan 141 reclassifies the `i2plib` Base64 finding as a concrete server
protocol defect: current SAM documentation requires the I2P Base64 alphabet,
so Plan 142 must correct the server rather than treating this merely as an
external-client compatibility discrepancy.

## Local results

Passed before this record was written:

```text
cargo test --locked -p i2pr-api --all-targets
# 113 passed
cargo test --locked -p i2pr-daemon --test sam_loopback --test sam_stream --test sam_forward_naming
# 30 passed
```

The secret-owner correction in `i2pr-api` removes the stale `Clone`
implementation from `SamPrivateDestination` and related request/outcome
containers.

The final local gate run also passed:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace                 # 1241 passed
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-constrained-host-lane-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'  # 153 passed
cargo deny check advisories bans sources                 # passed with duplicate warnings
git diff --check
```

The restored rootless harness files repair a pre-existing checker/entrypoint
drift from the historical harness-pruning commit; no rootless policy was
changed. The retired Plan 095 checker remains absent and is no longer listed
as a current local gate.

## Superseded disposition

The historical classification at the time of this audit was:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_135 = superseded-by-plan140-audit
plan_136 = passed-sam31-protocol-private-destination-foundation
plan_137 = passed-m7-sam31-loopback-server-session-lifecycle
plan_138 = passed-m7-sam31-stream-connect-accept-bridge
plan_139 = passed-m7-sam31-forward-naming-hardening
plan_140 = blocked-independent-client-stream-path-not-ready

milestone7_local_product = not-yet-closed
sam31_stream = implemented-locally; independent-client-interoperability-not-claimed
sam_independent_clients = 0-passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = blocked-until-stream-bridge-and-client-compatibility-correction
```

The current corrective classification is in `plans/141-status.md`.

The next executable work is **Plan 142**, followed by Plans 143 and 144.
Milestone 8 / SSU2 planning is not the next frontier until the corrective
sequence closes Milestone 7 or records a new concrete environment-independent
blocker.