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
