# Plan 476 status amendment — private-LAN product dial path

This amendment records a source prerequisite for the Plan 476 local runner. It
does not close the guarded per-host process/session executor or either live
qualification tier.

## Source change

The runtime now has a separate SSU2 direct-dial target constructor for RFC1918
IPv4 endpoints. The existing constructor remains loopback-only. Daemon target
selection requires the explicit `private_lan_qualification` profile, and the
service-product bootstrap and destination replacement paths now carry that
profile when creating authenticated peer dials. This allows a Plan 476 owner
configuration to dial its validated signed private peer RouterInfo while
rejecting public, documentation-only, IPv6, loopback-under-the-private-LAN
constructor, unspecified, and zero-port endpoints.

This change does not provide a pair manifest binding inside the daemon, a
per-host process/session receipt collector, request/reply evidence, restart and
withdrawal evidence, or a live run. The operator executor must still bind its
local configuration and temporary NetDB to the exact validated pair before
starting the router. No live command was run; the operator confirmed that no
independent host pair is available.

## Verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all` | Passed |
| `cargo check --locked -p i2pr-daemon --all-targets` | Passed |
| `cargo test --locked -p i2pr-runtime private_lan_dial_targets_are_rfc1918_ipv4_only -- --test-threads=1` | Passed (1 test) |
| `cargo test --locked -p i2pr-daemon --lib daemon_dial_target_requires_explicit_private_lan_profile -- --test-threads=1` | Passed (1 test) |
| `python3 tests/integration/ssu2/plan472_live.py --self-test` | Passed (2 positive / 8 negative cases) |
| `python3 tests/integration/ssu2/test_plan476_executor.py` | Passed (4 tests) |
| Full AGENTS.md routine floor | Passed: 4,856 workspace tests, 39 ignored (187 suites); Clippy, rustdoc/doc-tests, planning and boundary/evidence scripts, and `cargo deny check advisories bans sources` passed |

## Remaining executor interface

Source inspection confirms the process boundary is still absent: `i2pr run`
accepts a config path and has no qualification observer option or status
command. The SSU2 runtime snapshot exposes aggregate counters but no peer or
session-generation identity, while authenticated peer/link metadata exists only
inside the daemon's inbound I2NP dispatch path. The runner therefore cannot yet
attribute an external process's authenticated session or I2NP effect from an
independent receipt. Adding a live runner around stdout, aggregate counters, or
operator-authored phase labels would not establish those facts.

## Disposition

**Status remains:** `blocked-local-process-executor-and-authorized-host-pair-unavailable`.

The next implementation step is still the local process/session owner that
starts only the declared local router and collects attributable, sanitized
session and I2NP receipts. Tier B additionally requires an explicitly
authorized independent host pair. Neither the private-LAN profile nor the
offline receipt simulator is evidence of live SSU2 or I2NP behavior.
