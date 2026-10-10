# Plan 447 status: blocked — Gate A passes; reference link and application gates remain open

Closure token: `blocked-plan447-gates-b-and-c-not-qualified`

Plan: [`447-real-daemon-ssu2-netdb-sam-e2e-corrective.md`](../../implementation/core-router-recovery/447-real-daemon-ssu2-netdb-sam-e2e-corrective.md)

## Outcome

Plan 447's independent normal-process Gate A has partial local evidence. A
black-box test starts the compiled `i2pr run` binary as a child, performs SAM
3.1 negotiation against its configured loopback listener, verifies it remains
serving, and observes clean shutdown. The default profile also starts and shuts
down without opening a listener. This does not establish all of Gate A's SSU2
identity/cache assertions, and it does not establish a stock link, live NetDB
exchange, nonzero-hop tunnel, or application data path.

Gate B and Gate C remain blocked on their independent reference-router topology
and evidence. Plan 447 is therefore **blocked**, with the existing Gate A test
retained as partial process-composition evidence. Plans 431, 433, and 443 remain
unchanged; no capability or public-router claim is promoted.

## Requirement-to-evidence matrix

| Gate / requirement | Result |
| --- | --- |
| Gate A: actual `i2pr run` process owns configured loopback SAM listener | Partial pass: process-level test starts `CARGO_BIN_EXE_i2pr`, connects over TCP, and completes SAM 3.1 HELLO. |
| Gate A: listener remains live and process shuts down cleanly | Pass in the focused test; it re-probes after the bind deadline and checks shutdown. |
| Gate A: disabled-by-default listeners remain unbound | Pass in the focused test's default-profile row. |
| Gate A: full persisted Plan 442 SSU2 identity/cache inspection and restart evidence | Not established by this command; no full Gate A pass claimed. |
| Gate B: independently observed SSU2 authentication and I2NP/NetDB exchange with stock reference | Not run. |
| Gate C: nonzero-hop application request/reply through a real remote service | Not run. |
| Restart/fault recovery across stock peer and daemon loss | Not run. |
| Preserve loopback defaults and avoid public advertisement | Preserved; this work changed no production configuration or support inventory. |

## Commands and outcomes

Executed locally on Linux:

- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` — passed; required managed-app sibling binaries were already current (0 crates compiled).
- `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` — passed; 4 tests, including configured SAM 3.1 readiness, occupied-port refusal, identity refusal, and default-profile zero-listener lifecycle.
- No stock reference or external topology command was run for Gates B/C. No hosted CI result is claimed.

## Security, compatibility, and findings

- No production files, config defaults, key formats, reference source, support
  inventory, or RouterInfo advertisements changed.
- Gate A is process-level SAM readiness evidence only. It does not prove remote
  transport authentication, NetDB use, tunnel composition, or anonymity.
- Gate B/C remain unavailable without a qualified controlled reference topology;
  no fake peer, direct test transport, or synthetic tunnel result was used.

## Unblock audit and disposition

Plan 447 remains blocked on Gates B and C. Plan 433 remains blocked on its
independent-router end-to-end requirements; Plan 443 remains blocked on its
normal-process and reference evidence. Plan 446's blocked stock NTCP2 control
does not gate Gate A and provides no prerequisite evidence for Gates B/C.
Plans 448 and 449 remain independent. No other plan is silently unblocked.

The production daemon remains loopback-only and disabled by default for the
relevant listeners. Plan 431's independent non-loopback SSU2 requirement and
Plan 439's final acceptance remain unchanged.
