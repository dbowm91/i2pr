# Plan 355 status — router app-principal gateway over private client seams

Status: **passed-managed-app-principal-gateway-private-client-seams**.

Classification: infrastructure + invariant + bounded capability plumbing.
This closes the router-side SAM/I2CP gateway boundary. It does not implement an
application runtime, process authentication, package management, sandboxing,
brokered clearnet, or a usable third-party application capability.

## Implementation commits

- `94bfb65` — activated Plan 355.
- `d2f17f3` — froze trusted authorization, per-instance service-stream
  ownership, exact byte mapping, and the `control_scoped` gate in the
  language-neutral contract and architecture docs before production changes.
- `2716aa1` — added the daemon gateway, direct `i2pr-app-proto` dependency,
  isolated SAM/I2CP service contexts, supervised connection lifetime and
  completion reporting, boundary checker, CI/floor wiring, and dependency docs.

Plan 354's prerequisite closure is `6cd35bf` (`passed-managed-app-private-client-transport-seams`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Authorization comes only from trusted composition | `AppGatewayAuthorization::from_trusted_composition`; private fields; no serde derives or raw Hello decoder; `check-managed-app-gateway-boundary.py` | The daemon-local authorization binds one `AppPrincipal` to one immutable `EffectiveCapabilities`. The app protocol's `hello` and `RequestedCapability` values cannot construct it. Process/channel authentication remains the future runtime's responsibility. |
| Exact capability checks precede backend side effects | `denied_and_reserved_opens_have_no_backend_side_effects`; `AppGatewaySession::authorize`, `open_sam`, `open_i2cp` | SAM and I2CP denial leave service state and connection slots untouched. A requested SAM capability without a grant does not authorize. `control_scoped` stays typed unsupported even when granted. The existing grant constructor rejects `BrokeredTcp`. |
| One gateway session owns one principal's isolated SAM namespace | `private_sam_sessions_are_instance_scoped_and_forward_stays_denied` | Two principal sessions create the same SAM session id independently. `STREAM ACCEPT` from the other principal gets an invalid-id/error response and closes only that private connection. The second principal can then create the same id in its own context. |
| I2CP identifiers are per principal | `private_i2cp_get_date_runs_in_isolated_instance_contexts` | Each principal owns a distinct `I2cpServiceState`; both independently allocate local connection id 1 and complete protocol-byte + GetDate/SetDate over private duplex streams. |
| Private connections do not depend on listeners or loopback fallback | Gateway construction forces both listener configs disabled; `check-managed-app-gateway-boundary.py`; private SAM/I2CP gateway tests | Backend contexts are private `SamServiceState`/`I2cpServiceState` instances using Plan 354 private connection drivers. The gateway contains no host TCP connect or listener bind path. Ordinary listener state is not injected or shared. |
| SAM naming uses the canonical router-wide address book | `sam_state`; `check-managed-app-gateway-boundary.py` | Each private SAM context receives the supplied canonical `SharedAddressBook` handle. Other router-wide stacks are not constructed by the gateway. |
| Service stream semantics are frozen before code | `d2f17f3`; `specs/references/managed-native-app-runtime-v1.md` §3 | One successful SAM/I2CP open represents one protocol connection; data payloads are exact ordered protocol octets; the trusted runtime owns logical stream-id multiplexing. `control_scoped` remains reserved pending a separate Proposal-170 adapter. |
| Managed SAM cannot use host-target forwarding | `private_sam_sessions_are_instance_scoped_and_forward_stays_denied`; inherited Plan 354 test; `scripts/check-managed-app-private-client-seams.py` | Gateway SAM returns the existing typed `I2P_ERROR` for `STREAM FORWARD`; the listener-only host target path is unavailable to private origins. |
| EOF, cancellation, and resource exhaustion are bounded | `AppGatewayLimits`, session semaphore, `ChildScope`, `AppGatewayConnection::wait_closed`; `connection_admission_is_bounded_and_cleanup_releases_capacity`; `one_backend_eof_does_not_close_a_sibling_connection` | Gateway ceiling is at most the v1 128-stream limit and is further capped by configured SAM/I2CP client limits. Max+1 fails before task admission. Connection end is observable by the trusted runtime; EOF releases capacity, and one connection ending leaves a sibling usable. Session shutdown cancels and joins owned children. |
| Dependency and support boundaries stay narrow | `scripts/check-dependency-direction.sh`; `scripts/check-managed-app-gateway-boundary.py`; `Cargo.lock`; `specs/support.toml` | `i2pr-daemon` is the only new production consumer of `i2pr-app-proto`. No protocol support, RouterInfo advertisement, or `specs/support.toml` entry changed. |

## Ownership and data flow

```text
future trusted runtime
  authenticates process/channel and supplies AppPrincipal + effective grants
        |
        | one owned async byte stream per authorized open
        v
i2pr-daemon AppGatewaySession (one AppInstanceId)
  capability check -> bounded permit -> private service context -> ChildScope task
        +--> private SAM driver (canonical SharedAddressBook; FORWARD denied)
        +--> private I2CP driver
        +--> connection-end signal to close/reset the matching logical stream
```

No application process receives a host socket. No loopback listener is enabled
or used as a fallback. The module does not decode the outer managed-app frames;
the future trusted runtime retains framing and stream-id ownership.

## Security, contention, and compatibility

- Authorization is not serializable and has no constructor from application
  wire values or requested capabilities. Only trusted daemon composition can
  create it from authenticated identity and effective grants.
- Capability checks run before the gateway semaphore, service state, or child
  task is allocated. SAM and I2CP client state is created lazily and separately
  for each gateway session.
- The connection semaphore uses the minimum of the requested gateway ceiling,
  configured SAM clients, and configured I2CP clients; Plan 354's internal
  service admission remains a second bounded check. No automatic retries are
  introduced.
- Child tasks receive per-connection cancellation plus the session's
  `ChildScope`; explicit `shutdown` cancels and joins them. Dropping a session
  cancels its parent token. A connection handle reports terminal backend
  closure so its runtime owner can retire the corresponding logical stream.
- The v1 managed-app protocol is unreleased. A repository consumer search found
  no runtime/client consuming the open-stream contract, so the clarification
  does not change v1 `1.0`. SAM/I2CP wire behavior and support declarations are
  unchanged.
- No package/process/sandbox/admin capability is claimed. A future trusted
  runtime must authenticate the process/channel before it can construct
  gateway authority. OS containment still needs separate implementation and
  qualification.

## Verification

All results below are local Linux runs. No hosted CI, macOS, external-router,
or operating-system sandbox qualification result is claimed.

Focused Plan 355 checks:

```text
cargo test --locked -p i2pr-daemon --lib app_gateway -- --test-threads=1    passed (5 tests)
cargo test --locked -p i2pr-daemon sam -- --test-threads=1                   passed (31 tests)
cargo test --locked -p i2pr-daemon i2cp -- --test-threads=1                  passed (9 tests)
cargo check --locked -p i2pr-app-proto -p i2pr-daemon --all-targets         passed
cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-daemon --no-deps      passed
python3 scripts/check-managed-app-gateway-boundary.py                        passed
python3 scripts/check-managed-app-private-client-seams.py                    passed
```

Full routine floor on the completed Plan 355 implementation:

```text
cargo fmt --all --check                                                        passed
cargo check --locked --workspace --all-targets                                passed
cargo +1.88 check --locked --workspace --all-targets                           passed (MSRV)
cargo test --locked --workspace --all-targets -- --test-threads=1             passed (4080 passed, 35 ignored, 148 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps         passed
cargo test --locked --workspace --doc                                         passed (20 suites)
bash scripts/check-dependency-direction.sh                                    passed
python3 scripts/check-global-plan-number-uniqueness.py                        passed
python3 scripts/check-portable-service-tunnel-api.py                          passed (678 declarations)
bash scripts/check-portable-service-tunnel-consumer.sh                        passed (8 clean-build tests)
python3 -m unittest discover -s tests/planning -p 'test_*.py'                 passed (7 tests)
bash scripts/check-runtime-boundaries.sh                                      passed
python3 scripts/check-managed-app-gateway-boundary.py                         passed
bash scripts/check-service-tunnel-boundaries.sh                               passed
bash scripts/check-m11-per-epoch-composition.sh                               passed
bash scripts/check-service-anonymity-boundaries.sh                           passed
bash scripts/check-fixture-manifest.sh                                        passed
bash scripts/check-ntcp2-vectors.sh                                           passed
bash scripts/check-ssu2-vectors.sh                                            passed
bash scripts/check-i2cp-vectors.sh                                            passed (15 vectors)
bash scripts/check-ntcp2-interoperability.sh                                  passed
bash scripts/check-constrained-host-lane-boundary.sh                          passed
bash scripts/check-m11-transit-boundaries.sh                                  passed
bash scripts/check-m11-transit-qualification-evidence.sh                      passed (175 Plan 257 rows; 70 epoch keys; 13 Plan 258 keys)
bash scripts/check-sam-acceptance-evidence.sh                                 passed (22 rows)
bash scripts/check-ssu2-acceptance-evidence.sh                                passed (15 rows)
bash scripts/check-i2cp-acceptance-evidence.sh                                passed (24 rows)
bash scripts/check-i2pcontrol-acceptance-evidence.sh                          passed
bash scripts/check-service-tunnel-acceptance-evidence.sh                     passed (29 rows; 2 evidence rows remain blocked)
bash scripts/check-exploratory-tunnel-evidence.sh                             passed (12 guarded labels)
bash scripts/check-netdb-tunnel-evidence.sh                                   passed (12 guarded labels)
bash scripts/check-destination-tunnel-evidence.sh                             passed (21 guarded labels)
bash scripts/check-streaming-tunnel-evidence.sh                               passed (guarded-label check)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                     passed (two-family pins and guarded checks)
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test        passed (negative controls behaved as expected)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py' passed (18 tests)
cargo deny check advisories bans sources                                     passed
```

The streaming and mixed-router evidence checkers print existing guarded-label
warnings while returning success. The floodfill `--self-test` prints its
deliberate negative-control diagnostics and returns success. `cargo deny`
reports existing duplicate transitive lockfile versions (`block-buffer` and
`windows-sys`) as warnings; advisories, bans, and sources pass. One extra
non-floor probe used a nonexistent `.sh` spelling for the Plan 354 seam
checker and returned 127; the correct required `.py` checker above passed.

## Findings and limits

- Critical: none.
- High: none.
- Medium: none.
- Low: none.

The gateway is infrastructure, not an integrated application runtime. The
trusted process-authentication/outer-frame consumer, package/grant lifecycle,
OS sandbox, Proposal-170 adapter, and cross-platform qualification are not
implemented here.

## Roadmap disposition and unblock audit

Plan 355 is closed. It unblocks authoring a separate AppManager/package/process
plan against the concrete private SAM/I2CP gateway; no such plan is currently
registered, so no successor status token needed updating. The scoped
Proposal-170 adapter remains blocked on the canonical Proposal-170 continuation
(Plan 348, dependent on Plans 342 and 347). Sandbox, broker, SDK, and UI work
remain downstream and need their own plans and evidence. No router protocol,
M12, anonymity, or interoperability milestone was unblocked or had its status
changed.
