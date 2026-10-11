# Plan 458 status: blocked — real-process SAM exchange remains unproven

Closure token: `blocked-real-process-nonzero-hop-sam-exchange-unproven`

Plan: [`458-remote-sam-product-composition-corrective.md`](../../implementation/core-router-recovery/458-remote-sam-product-composition-corrective.md)

## Outcome

The normal router-network SAM path now registers each destination with the
shared daemon-owned service manager, asks the SSU2/NetDB product owner to
provision its real destination tunnels, and commits the SAM session only after
the product reports inbound ownership and a nonempty LeaseSet2. The local-only
SAM fabric is no longer selected for router-network requests. Dynamic
destinations are bounded to eight, and session teardown is sent back to the
product owner.

SAM-only startup now reports the product as unavailable. A running SSU2
product that cannot provision a destination reports the separate typed
provisioning failure and leaves the router serving. Both failure paths roll
back the pending SAM session and dynamic destination. A regression found while
rerunning the full lifecycle suite showed the no-owner case could reach the
per-destination error path; `execute_router_network_session_create` now checks
that the product owns the activation channel before reserving a destination.

The required Plan 458 capability result is still missing: there is no executed
normal-process `i2pr run` plus external SAM client exchange through nonzero-hop
tunnels in both directions against pinned i2pd. The existing Plan 193 M6
Streaming lane passes at its service-group/in-process scope and is not evidence
for SAM-created destinations. Plan 458 is therefore blocked rather than
passed.

A bounded ordinary-process attempt used four locally seeded, exact-pinned
i2pd 2.61.0 floodfill peers with independently signed family identities, then
started the regular `i2pr run` and connected an external SAM client. Router
startup and the SAM listener succeeded, but SAM `SESSION CREATE` returned a
non-success router-product provisioning result. The stream exchange never
started, so this run supplies no request/reply or nonzero-hop evidence. Its
scratch roots and child processes were cleaned; no raw reference logs or
identities were retained.

Follow-up attempts carried a closed-set stage from deferred product activation
through the private completion channel and SAM `MESSAGE`. The activation loops
also now stop immediately when the coordinator reports a terminal
`DestinationBuildFailed` for this SAM destination instead of discarding that
outcome and waiting for the build deadline. On the next bounded run both
inbound builds completed and the failure moved to `outbound-build`; a later
run stopped at `inbound-build`. These timeout labels mean no matching build
installed before the configured deadline. They do not establish why the
selected peers did not complete the request. Neither run reached a stream.
No raw reference logs, destination identity, or dynamic error details cross
the SAM response or are retained. Plan 458 remains blocked pending a source
correction for the intermittent build completion and a successful nonzero-hop
bidirectional exchange.

## 2026-10-11 bounded corrective continuation

Source inspection found the activation path discarded the initial build
submission result, so a rejected delivery could wait until the install
deadline. It also ignored a matching strict-decode `InvalidReply` in both
inbound and outbound activation loops. The path now returns immediately for a
rejected submission and correlates invalid replies by the selected terminal
peer, short-build kind, and exact message ID. SAM still reports only fixed
failure labels and retains the same rollback behavior. Registration and
post-activation summary failures now have separate fixed labels from the
generic SAM catch-all.

A bounded ordinary-process rerun after those changes stopped at
`failure_stage=inbound-build`, before either SAM STREAM exchange began. An
intermediate run stopped at `failure_stage=outbound-build`; another surfaced
the previous generic `destination-registry` category before the registration
and summary branches were separated. The exact-stage rerun again alternated
to the inbound timeout. No matching terminal invalid-reply event was observed
in these runs, so the new correlation checks did not identify the remaining
cause. No more Plan 458 attempts were made after this exact source correction.

## 2026-10-11 exact-attempt reply correlation

Further inspection found that an outbound short-build reply is carried inside
an I2NP `TunnelGateway` (type 19), and the coordinator can reject its decoded
inner reply after authenticating the selected peer and matching the pending
tunnel. The activation path previously recognized direct short-build invalid
replies but could miss this wrapped invalid-reply outcome. Invalid-reply
outcomes now retain their `BuildAttemptId`; inbound and outbound activation
match that exact attempt, and the outbound path recognizes the type-19 gateway
response from the selected terminal peer. This avoids attributing an unrelated
concurrent destination's reply to the SAM session.

The single bounded pinned-peer rerun after this source correction still
stopped at `failure_stage=outbound-build` before SAM stream setup. No matching
attempt-scoped invalid-reply event was surfaced, so the type-19 path did not
localize the unresolved build installation failure. The scratch topology was
cleaned. No further run is justified without a distinct source-level cause;
Plan 458 remains blocked and no Plan 455 stock-to-i2pr delivery is established.

The fixed stage mapping test, the 9-case real-process lifecycle suite, daemon
Clippy, service-tunnel boundary checker, and planning unittest suite passed
locally. The full workspace floor then completed against the final source
change: 4,822 tests passed and 39 were ignored across 187 suites, with
workspace Clippy, documentation, doc tests, and all listed planning,
dependency, boundary, evidence, vector, workflow, and license checks passing.
Plan 458 remains blocked: no nonzero-hop request/reply stream or exact-byte
exchange was completed, and the reason a selected tunnel build does not
install remains unresolved.

The workspace floor initially exposed a regression in the separate local
co-owned test fabric: its delivery guard required an inbound-tunnel factory
even though `bridge_to_peer` receives the tunnel directly. The guard now checks
only the signed LeaseSet2 and outbound role required by that API; router-only
bridges still have neither and remain excluded. The exact Plan 144 full
handshake regression test passes after this correction.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Shared normal-daemon SAM/product composition | Implemented in commits `11b50be5`, `cb6a092b`, `7f965cc8`, and `b3917688`; focused daemon tests and service-tunnel boundary guard pass. |
| Transactional SAM CREATE, bounded dynamic sessions, rollback and teardown | Manager tests pass; `sam_stream_self_composed` passes 4 tests; the real-process lifecycle suite passes 9 tests. |
| Product unavailable versus route provisioning failure | `run_lifecycle_readiness` verifies both typed replies and that router startup/shutdown remains healthy. |
| Bootstrap unavailable, no tunnel peers, nonzero-hop remote route | The no-service-group failure path is tested. A seeded pinned-peer normal-process success path is not established. |
| Gate B/C external SAM exchange, exact-byte request and reply | Not run. No nonzero-hop bidirectional real-process application exchange is claimed. |
| M6 preflight, NetDB, and service-group Streaming controls | Previously passed locally; they exercise separate lower-level/service-group paths and do not close this requirement. |
| Public capability/support promotion | None. Defaults, loopback-only SAM, SSU2 profile and non-advertisement remain unchanged. |

## Commands and outcomes

All results below are local Linux evidence, not CI results.

| Command | Outcome |
| --- | --- |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed before the focused daemon qualification runs. |
| `rtk cargo fmt --all --check` | Passed after the final source change. |
| `rtk cargo test --locked -p i2pr-daemon router_network_sam --lib -- --test-threads=1` | Passed, 6 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 9 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` | Passed, 4 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test sam_stream_independent plan144_full_handshake_reaches_bidirectional_established -- --exact --test-threads=1` | Passed after correcting the local delivery material guard. |
| `rtk cargo test --locked -p i2pr-daemon --test i2pcontrol_shared_service_manager -- --test-threads=1` | Passed, 6 tests. |
| `rtk cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | Passed. |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed, 4,821 tests; 39 ignored across 187 suites (1,068.53s). |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed. |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --locked --workspace --no-deps` | Passed; 33 crates documented. |
| `rtk cargo test --locked --workspace --doc` | Passed, 29 suites. |
| `rtk bash scripts/check-service-tunnel-boundaries.sh` | Passed. |
| `rtk bash tests/integration/m6-interop/run-preflight.sh` | Passed previously; lower-layer control only. |
| `rtk bash tests/integration/m6-interop/run-netdb.sh` | Passed previously; lower-layer control only. |
| `rtk bash tests/integration/m6-interop/run-streaming.sh` | Passed previously at Plan 193 service-group scope; not SAM destination composition evidence. |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Blocked: four pinned local peers and ordinary `i2pr run` reached external SAM `SESSION CREATE`, which returned a router-product provisioning failure before any stream exchange. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Blocked before stream setup; corrected ordinary-process result was `failure_stage=inbound-build`. The runner's scratch peers and process were cleaned. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after terminal build-failure handling | Blocked before stream setup at `failure_stage=outbound-build`; both inbound builds had completed on this attempt. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after matching outbound handling | Blocked before stream setup at `failure_stage=inbound-build`; no matching inbound build installed before its deadline. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after submit-result and strict-reply source corrections | Blocked before stream setup; the SAM client observed the generic `destination-registry` category before registration and summary failures received separate labels. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after registration/summary stage separation | Blocked before stream setup at `failure_stage=outbound-build`. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after exact outbound invalid-reply correlation | Blocked before stream setup at `failure_stage=inbound-build`; no matching invalid reply was correlated. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after type-19 gateway and exact-attempt invalid-reply correlation | Blocked before stream setup at `failure_stage=outbound-build`; no matching attempt-scoped invalid reply was observed. |
| `rtk cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1` | Passed, 15 tests, after attempt IDs were added to invalid-reply outcomes. |
| `rtk cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | Passed after exact-attempt correlation. |
| `rtk cargo fmt --all --check` | Passed after exact-attempt correlation. |
| `rtk bash scripts/check-service-tunnel-boundaries.sh` | Passed after exact-attempt correlation. |
| `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed, 51 tests; expected missing-workflow warning emitted by test fixture. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_network_sam_failure_stage_tests -- --test-threads=1` | Passed after the bounded stage additions. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed: 9 tests after the SAM failure mapping change. |
| `rtk cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | Passed after the corrective changes. |
| `rtk bash scripts/check-service-tunnel-boundaries.sh` | Passed after the corrective changes. |
| `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed: 51 tests; expected temp-repo missing-workflow warning emitted by its workflow validity fixture. |
| `rtk cargo fmt --all --check`; `rtk cargo check --locked --workspace --all-targets`; sibling app build; workspace test/Clippy/doc/doc-test commands and all remaining `AGENTS.md` floor checkers | Passed on the final corrective source: 4,822 workspace tests passed, 39 ignored across 187 suites; all remaining commands exited successfully. |

The full workspace floor initially exposed a local co-owned SAM delivery test
regression: `plan144_full_handshake_reaches_bidirectional_established` failed
because the guard required an inbound-tunnel factory that this direct API does
not use. After narrowing the guard to the signed LeaseSet2 and outbound role,
the test passed on its own and the full workspace test suite passed. The
ordinary-process positive remote SAM exchange remains blocked at session
provisioning.

## Compatibility and security

- Router-network SAM continues to use the daemon's existing identity, SSU2
  owner, NetDB and bounded product channels; no second router or socket owner
  was added.
- Provisioning failure crosses the private product channel as a closed enum;
  SAM exposes only its fixed lowercase stage label, never a formatted lower
  level error, destination identifier, or key material.
- Dynamic SAM destinations have a fixed capacity of eight and are private to
  the session. Requests do not fall back to local synthetic tunnels.
- SAM remains loopback-only, disabled by default, experimental and
  non-advertised. No protocol support inventory or conformance claim changed.
- A published LeaseSet2 cannot be synchronously withdrawn by this path on
  session retirement; it expires under its lease lifetime. This remains a
  lifecycle limitation pending a separately qualified revocation mechanism.

## Findings and unblock audit

- Critical: none.
- High: none localized by the executed checks.
- Medium: normal-process nonzero-hop SAM request/reply and exact-byte
  bidirectional exchange remain unproven; the plan's capability gate is open.
- Low: retirement relies on LeaseSet2 expiry rather than immediate withdrawal.

Plan 458 can resume when a bounded local pinned-i2pd topology and an external
SAM driver exercise the ordinary `i2pr run` product path in both directions.
That run must prove nonzero-hop routing and exact bytes, and include the
bootstrap/no-peer/session rollback controls. Plan 451 remains blocked pending
that product evidence; Plans 433/431 and NTCP2 Plans 455/457 retain their own
independent blockers. Plan 456 remains passed only at its controlled
source/lifecycle scope. No historical status or support claim is promoted.

Disposition: **blocked pending the Plan 458 real-process capability gate**.
