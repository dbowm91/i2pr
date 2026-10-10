# Plan 448 status: blocked — transport integration and advertisement gate remain open

Closure token: `blocked-plan448-transport-integration-and-routerinfo-eligibility`

Plan: `plans/implementation/floodfill/448-global-bandwidth-governor-and-eligible-capability-class.md`

## Implemented slice

- `i2pr-runtime::BandwidthGovernor` provides separate inbound and outbound
  token buckets, bounded per-process and per-peer waiter limits, FIFO admission
  per direction, cancellation cleanup, privacy-redacted peer keys, aggregate
  counters, and hard rate/burst/queue ceilings.
- Daemon `[bandwidth]` configuration is strict, disabled by default, rejects
  invalid rates, shares, bursts, and queue sizes, and requires the currently
  available SSU2 runtime owner when enabled. The daemon passes one shared
  governor into SSU2 composition.
- The SSU2 socket owner charges outbound datagram bytes immediately before
  `send_to` and charges received datagram bytes before protocol processing.
  This bounds accepted processing and outgoing socket submissions; it cannot
  throttle bytes already received by the host NIC.
- `i2pr-netdb::BandwidthClass` computes a data-only class from the configured
  bottleneck rate and integer share, with checked arithmetic and pinned Java
  KiB/s boundaries. It cannot construct or authorize a RouterInfo capability.
- No `caps` builder, advertisement permit, support inventory, conformance
  record, `f`, or `R` behavior was changed.

## Verification evidence

| Command | Result |
| --- | --- |
| `rtk cargo test --locked -p i2pr-runtime --lib bandwidth::tests -- --test-threads=1` | Passed: 5 governor tests, including fractional refill conservation, bounded per-peer queue, cancellation release, oversize rejection, and independent direction capacity. |
| `rtk cargo test --locked -p i2pr-daemon --lib config::tests::bandwidth -- --test-threads=1` | Passed: 2 config tests covering defaults, valid explicit settings, missing rates, share/queue/rate bounds, and missing owner rejection. |
| `rtk cargo test --locked -p i2pr-netdb --lib bandwidth_class_uses_integer_kib_boundaries_and_share -- --test-threads=1` | Passed: 1 exact-boundary/integer-normalization/share-validation test. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets` | Passed: 120 passed, 1 ignored across 5 suites. |
| `rtk cargo test --locked -p i2pr-transport --all-targets` | Passed: 57 tests across 2 suites. |
| `rtk cargo test --locked -p i2pr-netdb --all-targets` | Passed: 278 tests across 7 suites. |
| `rtk cargo check --locked -p i2pr-runtime -p i2pr-daemon -p i2pr-netdb` | Passed. |
| `rtk cargo fmt --all --check` | Passed. |
| `rtk cargo clippy --locked -p i2pr-runtime -p i2pr-daemon -p i2pr-netdb --all-targets --all-features -- -D warnings` | Passed. |
| `rtk bash scripts/check-runtime-boundaries.sh` | Passed, including runtime-boundary positive controls. |
| `rtk bash scripts/check-m12-floodfill-boundaries.sh` | Passed. |
| `rtk python3 scripts/check-core-router-recovery-contract.py` and `--self-test` | Both passed. |
| `rtk python3 scripts/check-global-plan-number-uniqueness.py` | Passed. |
| `rtk python3 scripts/check-tooling-inventory.py` | Passed. |
| `rtk python3 scripts/check-workflow-validity.py` | Passed: 11 workflow files. |
| `rtk git diff --check` | Passed. |

The first daemon filtered-test invocation omitted `--lib`, causing Cargo to
compile every daemon integration-test executable. That build was interrupted
and replaced with the scoped library command above; no test result was inferred
from the interrupted run.

## Blocking requirements

The pass criteria in the implementation plan are not met:

1. The configured governor currently has one daemon integration, the SSU2
   datagram socket owner. There is no shared NTCP2 writer integration, and
   normal-daemon NTCP2 remains disabled by its existing guard.
2. The plan requires bounded reservations for client, control, transit, and
   floodfill traffic, with a minimum control budget and priority behavior. The
   implemented FIFO queues have no traffic-class reservations or weighted
   peer scheduling. No priority inversion qualification exists.
3. No socket-backed test has yet measured actual emitted bytes/rates under
   sustained simultaneous peers, asymmetric load, cancellation during send,
   and service restart. Current governor evidence is deterministic unit-level
   evidence, not end-to-end transport qualification.
4. The normal SSU2 daemon remains loopback-only and non-advertised. The local
   RouterInfo builder still rejects bandwidth capability letters, and the
   class mapper has no health/eligibility permit. Therefore there is no safe
   signed RouterInfo class publication path or mutation-tested underclaim gate.
5. The shared-rate class uses configured capacity, but no signed RouterInfo,
   known-good Java candidate fixture, or Plan 444 eligible/ineligible Java
   selection control was produced. Plan 444 remains blocked.

Resource bounds for the implemented primitive are explicit: each directional
bucket holds at most the configured burst (maximum 4 MiB); the process waiter
queue is capped at 16,384 requests; and configured rate is capped at 10 GiB/s.
The default remains disabled. No public-network traffic or reference router
was started for Plan 448.

## Unblock and disposition

Resume after the daemon has an approved shared transport owner for every
enabled production transport and the bounded traffic-class allocation policy
is specified. Add socket-backed contention/restart evidence before connecting
any health-gated RouterInfo builder. Only a separate opaque capacity permit
derived from live enforced limits and health may authorize a class; `f` and
`R` continue to require their independent role and reachability gates.

Plan 448 is **blocked**, not passed. Plan 444 remains blocked pending complete
enforcement plus Java selection controls; Plans 433/436/437/438 retain their
independent prerequisites. `specs/support.toml`, `specs/CONFORMANCE.md`, and
public capability claims are unchanged.
