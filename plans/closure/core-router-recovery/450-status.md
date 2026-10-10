# Plan 450 status: passed — real-process controlled Gate A

Closure token: `passed-real-process-ssu2-sam-identity-restart-gate-a`

Plan: [`450-real-daemon-gate-a-router-lifecycle-and-readiness.md`](../../implementation/core-router-recovery/450-real-daemon-gate-a-router-lifecycle-and-readiness.md)

## Outcome

The compiled `i2pr run` process now has a black-box regression row that starts
with explicit loopback SSU2 and SAM configuration, confirms the SSU2 UDP port
is owned while the process is serving, performs SAM 3.1 HELLO and STREAM
session creation from an independent TCP client, shuts the process down, then
restarts it and confirms the SSU2 transport identity bytes are unchanged.
Existing rows continue to cover identity refusal, occupied SAM bind refusal,
disabled-by-default listeners, and orderly shutdown.

This closes controlled Gate A only. It proves neither an authenticated peer
nor populated NetDB, a tunnel, or application traffic through I2P. Plan 451 is
now dependency-ready to pursue those controlled Gate B/C outcomes. Plan 431's
independent LAN qualification and Plan 433's external full-router gate remain
open.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Normal-process SSU2 and SAM ownership | `controlled_ssu2_and_sam_survive_real_process_restart` starts the compiled daemon with both explicitly enabled and verifies SAM serving plus UDP port ownership. |
| Persisted transport identity survives restart | The test reads `ssu2.transport.identity` before and after a real process restart and compares the bytes without printing them. Plan 442's unit test independently binds the signed RouterInfo public `s`/`i` values to the persisted keys and router identity. |
| Actual SAM 3.1 session lifecycle | An independent TCP client performs HELLO and `SESSION CREATE STYLE=STREAM ... DESTINATION=TRANSIENT`; both replies are checked. |
| Cancellation and restart | The child is interrupted, reaped, then the same configured router is started again and serves SAM. |
| Negative/default controls | Existing black-box cases cover missing router identity, occupied listener port, and default profile with no listener. Plan 442 storage tests cover corrupt/truncated transport identity and router-hash mismatch. |
| Authenticated peer, NetDB, tunnel, application data | Not established; deferred to Plan 451 and the independent external gates. |

## Commands and outcomes

Executed locally on Linux:

```text
rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd
  passed (required sibling binaries already current)
rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1
  passed (5 tests)
rtk cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
  passed (6 passed, 1 ignored)
rtk cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
  passed (10 tests)
rtk python3 scripts/check-core-router-recovery-contract.py --self-test
  passed
rtk cargo fmt --all --check
  passed after formatting the new test
rtk git diff --check
  passed before the implementation commit
```

Implementation commit: `905c43a` (`test(recovery): exercise real SSU2 daemon restart lifecycle`).
No hosted CI result is claimed.

## Compatibility, security, and limitations

Only a black-box integration test changed. Listener defaults, key format,
RouterInfo publication, support inventory, network IDs, and runtime policy are
unchanged. The test uses a private temporary data directory, a loopback TCP
client, and a loopback UDP port; it records no key bytes in evidence. No stock
reference or public network was contacted. The test's process-level identity
continuity is combined with Plan 442's source-level signed `s`/`i` binding
evidence; it does not expose the private RouterInfo to a new diagnostic API.

Findings: critical — none; high — none; medium — Gate B/C and external router
qualification remain open by design; low — hosted CI for this new row has not
run.

## Unblock audit and disposition

Plan 451's only new hard gate, Plan 450 Gate A, is now closed. Its other
written interface prerequisites (Plans 432/440 and the existing Plan 161/443
controlled lanes) are already available, so Plan 451 moves to **ready**.
Plans 431 and 433 remain gated on independently addressed router evidence;
Plans 452–454 are unaffected and remain independently eligible. No support or
advertisement claim is promoted.
