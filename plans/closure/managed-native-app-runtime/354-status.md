# Plan 354 status — listener-independent SAM/I2CP private connection seams

Status: **passed-managed-app-private-client-transport-seams**.

Classification: infrastructure + invariant. The work adds reusable daemon
connection drivers; it does not launch applications, authenticate principals,
establish sandbox containment, or change SAM/I2CP support claims.

## Implementation commits

- `67708a1` — activated Plan 354 and froze the listener/private-stream ownership
  design in the daemon and API architecture docs.
- `1b4288e` — added shared async-stream SAM/I2CP drivers, managed SAM FORWARD
  denial, focused duplex tests, and the CI boundary checker.
- `633e9fc` — fixed SAM listener admission permit lifetime and separated the
  client-capacity and session-capacity acceptance cases.
- `a84e02a` — placed the I2CP private-connection test module after production
  marker items to satisfy the workspace Clippy lint.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Freeze the ownership boundary before refactoring | `67708a1`, `docs/architecture/i2pr-api.md`, `docs/architecture/i2pr-daemon.md` | Loopback listeners are adapters; trusted private streams use the same daemon drivers; no localhost bridge is part of private admission. |
| One SAM driver for listener and private streams | `sam.rs::serve`, `drive_private_connection`, `handle_connection` | Both origins pass an owned `SamIoStream` to the same driver. Listener metadata is an explicit `Loopback { peer_ip }` origin; private origin requires no peer address. |
| Managed SAM cannot request host-target forwarding | `private_connection_uses_sam_driver_without_listener_and_denies_forward`; `scripts/check-managed-app-private-client-seams.py` | Private `STREAM FORWARD` returns the existing `I2P_ERROR` reply before `execute_stream_forward`; the forwarding registry remains empty. Host TCP connects exist only in the ordinary FORWARD path. |
| Raw STREAM handoff accepts private byte streams | `RawStreamHandoff.stream: SamIoStream`; `raw_handoff_accepts_in_memory_transport_type`; existing SAM raw/loopback acceptance suites | The command-to-raw transition and raw pump carry the erased async stream; no internal TCP socket pair is required for private use. Existing ordinary FORWARD socket bridging remains scoped to the listener path. |
| One I2CP driver for listener and private streams | `i2cp.rs::serve`, `drive_private_connection`, `handle_connection` | Listener and private paths pass `I2cpIoStream` to the same driver, preserving protocol byte, frame, timeout, notify, and teardown logic. |
| Drive both private paths with no listener enabled | SAM and I2CP private duplex tests | Both test configurations set `enabled = false`; they use `tokio::io::duplex`, with no listener bind or host connection in private admission. SAM completes HELLO; I2CP completes protocol-byte + GetDate/SetDate. |
| Preserve connection/session bounds and cleanup | SAM client-capacity/session-capacity tests; I2CP active-connection cleanup assertion; full serialized workspace tests | SAM admission now holds the semaphore permit for the supervised connection lifetime. The previously ineffective permit lifetime is fixed; the session-capacity test reserves a separate client slot so each ceiling is tested independently. I2CP EOF teardown removes its active connection. |
| Preserve ordinary listener behavior and support boundary | SAM/I2CP regression suites; `specs/support.toml`; crate manifests | Existing listener acceptance suites pass. No support inventory or dependency changes. Listener enable defaults and advertised support remain unchanged. |
| Enforce the seam structurally | `scripts/check-managed-app-private-client-seams.py`; `.github/workflows/ci.yml`; `AGENTS.md` | CI and the routine floor check shared-driver calls, explicit private origin, bounded admission, no socket operations in private entry points, raw transport generality, and the pre-side-effect FORWARD deny. |

## Connection ownership

Before:

```text
loopback TCP listener -> TcpStream-specific protocol loop -> SAM raw TcpStream
loopback TCP listener -> TcpStream-specific I2CP protocol loop
```

After:

```text
loopback listener adapter ----+
                              +--> bounded shared protocol driver
trusted private byte stream --+       | SAM / I2CP state and dispatch
                                      +--> generic SAM raw byte-stream driver
```

The SAM listener obtains peer metadata at accept and supplies an explicit
loopback origin. A private caller supplies no peer metadata and receives a
private origin whose FORWARD branch rejects before host-target registration or
connect. Both paths retain the same service state, cancellation behavior, and
bounded admission semaphore.

## Verification

All results are local Linux results. No hosted CI, macOS, or external-router
result is claimed.

Focused Plan 354 checks:

```text
cargo fmt --all --check                                                        passed
cargo check --locked -p i2pr-daemon --all-targets                              passed
cargo test --locked -p i2pr-daemon private_connection -- --test-threads=1     passed (3 tests)
cargo test --locked -p i2pr-daemon sam -- --test-threads=1                    passed (30 tests)
cargo test --locked -p i2pr-daemon i2cp -- --test-threads=1                   passed (8 tests)
cargo test --locked -p i2pr-daemon --test sam_loopback -- --test-threads=1    passed (18 tests)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps          passed
bash scripts/check-dependency-direction.sh                                   passed
bash scripts/check-runtime-boundaries.sh                                     passed
python3 scripts/check-managed-app-private-client-seams.py                    passed
bash scripts/check-sam-acceptance-evidence.sh                                passed
bash scripts/check-i2cp-acceptance-evidence.sh                               passed
python3 scripts/check-global-plan-number-uniqueness.py                       passed
python3 -m unittest discover -s tests/planning -p 'test_*.py'                passed (7 tests)
```

Full routine floor on the completed Plan 354 implementation:

```text
cargo fmt --all --check                                                        passed
cargo check --locked --workspace --all-targets                                passed
cargo +1.88 check --locked --workspace --all-targets                           passed (MSRV)
cargo test --locked --workspace --all-targets -- --test-threads=1             passed (4075 passed, 35 ignored, 148 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps          passed
cargo test --locked --workspace --doc                                         passed (20 suites)
bash scripts/check-dependency-direction.sh                                    passed
python3 scripts/check-global-plan-number-uniqueness.py                        passed
python3 scripts/check-portable-service-tunnel-api.py                          passed (678 declarations)
bash scripts/check-portable-service-tunnel-consumer.sh                        passed (8 tests, clean temporary build)
python3 -m unittest discover -s tests/planning -p 'test_*.py'                 passed (7 tests)
bash scripts/check-runtime-boundaries.sh                                      passed
python3 scripts/check-managed-app-private-client-seams.py                     passed
bash scripts/check-service-tunnel-boundaries.sh                               passed
bash scripts/check-m11-per-epoch-composition.sh                               passed
bash scripts/check-service-anonymity-boundaries.sh                           passed
bash scripts/check-fixture-manifest.sh                                        passed
bash scripts/check-ntcp2-vectors.sh                                           passed
bash scripts/check-ssu2-vectors.sh                                            passed
bash scripts/check-i2cp-vectors.sh                                           passed (15 tests)
bash scripts/check-ntcp2-interoperability.sh                                 passed
bash scripts/check-constrained-host-lane-boundary.sh                         passed
bash scripts/check-m11-transit-boundaries.sh                                 passed
bash scripts/check-m11-transit-qualification-evidence.sh                     passed
bash scripts/check-sam-acceptance-evidence.sh                                passed
bash scripts/check-ssu2-acceptance-evidence.sh                               passed
bash scripts/check-i2cp-acceptance-evidence.sh                               passed
bash scripts/check-i2pcontrol-acceptance-evidence.sh                         passed
bash scripts/check-service-tunnel-acceptance-evidence.sh                     passed
bash scripts/check-exploratory-tunnel-evidence.sh                            passed
bash scripts/check-netdb-tunnel-evidence.sh                                  passed
bash scripts/check-destination-tunnel-evidence.sh                            passed
bash scripts/check-streaming-tunnel-evidence.sh                              passed with existing guarded-label WARN diagnostics
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                    passed
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test        passed, expected negative-control diagnostics emitted
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
  passed (18 tests)
cargo deny check advisories bans sources                                      passed; existing duplicate transitive-version warnings emitted
```

The first full test attempt exposed that SAM's listener permit was dropped
before the connection future ran. The Plan 354 fix keeps the owned permit in
`handle_connection`. The existing session-capacity test then had to reserve one
extra client slot to isolate its session ceiling, and a separate test now proves
that the client ceiling closes over-capacity connections. The complete workspace
test floor passed after those changes.

## Compatibility, security, and limitations

- No wire protocol, configuration schema, package contract, or advertised
  capability changed.
- No workspace dependency or support-inventory change was made.
- Private SAM/I2CP entries are crate-visible only, require an injected async
  stream, and do not bind/connect a host socket or consult listener enable flags.
- The ordinary loopback SAM FORWARD implementation remains available under its
  existing profile; the managed-private profile has a closed deny branch.
- This seam does not authenticate an app principal, create per-app namespaces,
  carry managed-app framing, launch a process, or provide OS containment. Those
  are Plan 355 or later owners.
- No external SAM/I2CP router interoperability or new support breadth is
  claimed.

Findings by severity: critical none; high none; medium none; low none
unresolved. The pre-existing SAM permit-lifetime defect was fixed and covered in
this plan. Existing M6 Streaming evidence warnings and duplicate dependency
version warnings are diagnostic-only and did not fail their checkers.

## Unblock audit and roadmap disposition

Plan 355 was the only registered implementation plan with a hard dependency on
Plan 354. Its other hard/interface dependencies are already satisfied: Plan 352
closed the managed-app contract correction, and ADR 0032 freezes the trust
boundary. Plan 354 now provides the no-listener SAM/I2CP interfaces, so Plan 355
moves from blocked to ready in the same planning update. No other registry row
or subsystem roadmap names Plan 354 as a hard/interface dependency. Package and
lifecycle/AppManager work is still unregistered, and Proposal-170 adaptation,
sandboxing, brokered clearnet, process launch, and UI hosting remain separately
gated. No runtime capability or anonymity claim is promoted.

Roadmap disposition: **closed** as
`passed-managed-app-private-client-transport-seams`.
