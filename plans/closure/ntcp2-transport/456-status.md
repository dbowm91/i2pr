# Plan 456 status: passed — controlled normal-daemon NTCP2 source/lifecycle

Closure token: `passed-controlled-normal-daemon-ntcp2-owner-lifecycle`

Plan: [`456-guarded-normal-daemon-ntcp2-service-owner.md`](../../implementation/ntcp2-transport/456-guarded-normal-daemon-ntcp2-service-owner.md)

## Outcome

The normal `i2pr run` process now owns an explicitly configured, controlled-loopback NTCP2 listener and outbound initiator path. The opt-in profile requires a loopback bind and fixed nonzero port; ordinary/default configurations remain disabled and ordinary `enabled = true` remains rejected. NTCP2 reuses the persisted router identity, signed RouterInfo and common I2NP dispatcher, and the process bandwidth governor. Static NTCP2 key/IV state is stored identity-bound and survives process restart.

The black-box lifecycle test launched the compiled daemon, verified the signed NTCP2 RouterAddress against its persisted transport material and RouterIdentity, rejected a wrong-network peer, authenticated the matching-network peer, and restarted the daemon with stable key/IV state. The outbound path now selects only signed, validated bootstrap RouterInfo entries and only dials literal loopback addresses in this controlled profile. Handshake completion is required before link promotion; failed initiator handshakes participate in bounded retry backoff.

This is a source and controlled-process lifecycle pass only. It does not establish stock-i2pd I2NP receipt, normal Internet peer selection, public reachability, SSU2 failover, or Plan 435 qualification. Plan 101's default-off and non-advertised posture remains in force.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Named opt-in profile and strict loopback/fixed-port validation | Config normalization and negative tests passed; `check-router-readiness-contract.py --self-test` mutation controls passed. |
| Persisted NTCP2 identity material bound to the router identity | Real `i2pr run` lifecycle test reconstructed and verified the signed NTCP2 RouterInfo, checked RouterIdentity association, and confirmed key/IV bytes remained stable after restart. |
| Supervised owner, common governor, and common I2NP dispatch | Runtime registration uses the existing daemon owner, shared governor, validated bootstrap snapshot, and common inbound dispatcher; runtime and boundary tests passed. |
| Real controlled listener and authenticated session lifecycle | Black-box lifecycle test verified listener bind, wrong-network rejection, matching-network authentication, clean restart, and stable transport material. |
| Default-off and no ordinary/public activation | Existing config rejection tests, RouterInfo no-address check, config hygiene, router-readiness controls, and runtime/dependency boundary scripts passed. |
| Stock peer interoperability and decoded I2NP receipt | Not established; remains Plan 455's blocker. |
| Normal-process bidirectional I2NP and dual-family failover | Not established; remains Plan 457's acceptance scope. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed. |
| `rtk cargo test --locked -p i2pr-daemon --lib config::tests -- --test-threads=1` | Passed, 89 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 7 tests, including the controlled NTCP2 process lifecycle. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed before the final workspace run; final workspace run also passed. |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed on the closure tree: 4,805 passed, 39 ignored across 186 suites (1,245.32s). |
| `rtk cargo clippy --locked -p i2pr-daemon -p i2pr-runtime -p i2pr-storage --all-targets --all-features -- -D warnings` | Passed. |
| `rtk python3 scripts/check-router-readiness-contract.py --self-test` and `rtk python3 scripts/check-router-readiness-contract.py` | Passed. |
| `rtk bash scripts/check-runtime-boundaries.sh`, `rtk bash scripts/check-dependency-direction.sh`, `rtk bash scripts/check-ntcp2-vectors.sh` | Passed. |
| `rtk cargo fmt --all --check` and `rtk git diff --check` | Passed. |

The remaining routine-floor checks (workspace check/build, Clippy, rustdoc/doc tests, planning and boundary/evidence scripts, and `cargo deny`) passed in the final implementation batch before the test-only paused-time correction; the final-tree all-target workspace test above then passed. The correction changed only an existing runtime test's clock control.

## Security, migration, and compatibility

- NTCP2 remains explicitly opt-in, loopback-only, and non-advertised. Invalid ordinary/public activation is rejected.
- The daemon uses one RouterIdentity, one RouterInfo/NetDB owner, one I2NP dispatcher, and the shared bandwidth governor; no second router core or transport-owned runtime was added.
- Persisted transport keys remain distinct from destination identities and are associated with the existing RouterIdentity.
- No protocol-support inventory or conformance claim changed. No configuration migration or default change was introduced.

## Findings by severity

- Critical: none.
- High: none.
- Medium: stock-side receipt and real stock-owned send trigger remain unproven (Plan 455); ordinary two-family I2NP/failover remains unproven (Plan 457).
- Low: the controlled profile intentionally accepts only loopback peers, so it does not qualify independently addressed or public peers.

## Unblock audit and roadmap disposition

- Plan 455 is still blocked at independently observed stock receipt and a stock-owned outbound trigger. This pass does not alter its status.
- Plan 457's Plan 456 source/lifecycle prerequisite is now satisfied, but its Plan 455 prerequisite remains blocked; Plan 457 remains blocked.
- Plans 434, 435, 452, and 455 retain their existing closure authority. No public NTCP2 support or advertisement is authorized.
- Plan 458 is a separate ready P2 composition item and can proceed after documenting the remaining NTCP2 gates; it does not depend on NTCP2.

Disposition: **passed at the controlled normal-daemon source/lifecycle scope**; no external interoperability or public capability claim.
