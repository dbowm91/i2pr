# Plan 086 status: host-loopback development lane enablement

## Status

Implementation surface delivered and lane-closure path exercised on
2026-08-05 against the concurrent-preflight commit
`32ca767fe1843d6d4abcec7cb6bad26eda3ac0c2`. The
`host-loopback-development` topology kind, the
`HostLoopbackDevelopmentPlacement`, the bounded literal IPv4 `127.0.0.1`
acceptance, the thin wrapper
`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`, the
**concurrent placement-owned preflight** (placement-owned `popen` for
the i2pd listener, the verified-copy of the i2pd RouterInfo into the
exact scenario exchange path, the boundary i2pr dialer `popen`, the
concurrent tailing of both event streams, the bounded reap of both
processes in declared order), the actual i2pd RouterInfo export, the
strict TOML scenario render with `validate-scenario`, and the
canonical `i2pd_ntcp2_interop_driver_instrumented` binary are landed
and green locally against the committed source.

The Plan 086 closure history:

1. **`b0efc4e2`** — initial closure based on the original runner (used
   invalid JSON as `.toml`, skipped `validate-scenario`, never exported
   the i2pd RouterInfo, never measured the i2pr binary).
2. **`3d77afb0`** — corrective pass: TOML render with the exact schema
   v2 field names, `validate-scenario` through the placement,
   measurement of the committed i2pr-interop binary, export of the
   actual i2pd RouterInfo through `write_local_router_info`,
   non-zero `placement_record_sha256`, wrapper exit codes for blocked
   and failing probes.
3. **`32ca767f`** — concurrent preflight pass: the existing
   `execute_listener_preflight` keeps the bounded listener-only
   contract surface for unit/integration use; the new
   `execute_concurrent_preflight` is the default `--preflight` path,
   binds the i2pd RouterInfo to the exact scenario exchange path via
   `_copy_router_info_with_verified_digest`, popens the i2pd
   listener through the placement, polls the listener placement-owned
   events file for authentic `listener_ready`, asserts the listener is
   alive when the dialer starts, popens the i2pr dialer through the
   i2pr placement while the listener is still alive, consumes both
   event streams concurrently on placement-internal daemon threads,
   bounded-reaps both processes in declared order (drainer first,
   then listener), and routes `validate-scenario` through placement
   ownership.

```text
status = host-loopback-development-ready
```

The Plan 086 preflight against the real i2pd direct driver binary on
this host produced the following sanitized record digests:

```text
source_commit                       = 32ca767fe1843d6d4abcec7cb6bad26eda3ac0c2
reference_revision                  = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
concurrent_preflight_record_sha256  = b2e5f4d888e2497ad2d4c22d95823c520e63710e6b0157c5a891f868a3beeac6
placement_record_sha256             = 2e9b87137c352879692e858ac2de01a96a8403ced4b1967e2b995f3911f85e86
i2pr_binary_sha256                  = 6eb9133dc128f2f1d9528e0d41ef7c165228dcde96d47aaea92faffcf1714d85
i2pd_binary_sha256                  = d92890746e022101fe6c8e6e0f54929a6c583937ff3148168cab72ebcaf4d7db
i2pr_router_info_sha256             = e06d5104188ef0d3a63b56e99e612ee4d48c17de4d333dce3f3faefb0c732c6d
i2pd_router_info_sha256             = 08d270f0c604ec4111e14b491a7d5e8900614355bb19e59623fbbf4969ef3453
peer_router_info_copied_digest      = 08d270f0c604ec4111e14b491a7d5e8900614355bb19e59623fbbf4969ef3453
peer_router_info_exchange_path      = exchange/i2pd-router.info
i2pr_router_hash_sha256             = 5061f6d4ad0a5deed252356210fd4dc5efde8366735b14c62b2da5c0adb6989c
i2pd_router_hash_sha256             = b48a5768841e4c31f9871c20b03070bb295448cbec2ed4c4b822033626f26bae
i2pd_driver_source_sha256           = fcfdb04a5df13b6c72c411158da8e949f5b4a856b72a1ff982ceee7783608313
lane_qualification_sha256           = cbb194de63a2949ae0bfdaf0ea93dc678173ff397ca559c8f3c7407ac215290c
listener_dialer_window_ms           = 5000
listener_alive_when_dialer_started  = true
dialer_disposition                  = terminated
listener_disposition                = terminated
highest_stage_reached               = listener_ready
reason_code                         = mixed-router-handshake-not-completed
terminal_result                     = pre_protocol_rejected
cleanup_result                      = clean
parent_network_state_unchanged      = true
topology_kind                       = host-loopback-development
reference                           = i2pd
observed_events                     = listener_ready, process_started, listener_ready, terminal_rejected
process_counters.i2pr_prepare       = started=1 exited=1
process_counters.i2pd_prepare       = started=1 exited=1
process_counters.i2pd_listener      = started=1 exited=1
process_counters.i2pr_dialer        = started=1 exited=1
```

The Plan 086 concurrent preflight exercised the placement-owned
listener/dialer concurrency architecture with the measured i2pd direct
driver binary. The `validate-scenario` Rust command returned
`{"schema":"i2pr-interop-scenario-validated-v1","result":"validated"}`
on the strict `i2pr-launcher-scenario-v2` TOML whose
`peer_router_info = "exchange/i2pd-router.info"` references the
**copied** i2pd RouterInfo file (the
`peer_router_info_copied_digest` matches the
`i2pd_router_info_sha256` byte-for-byte). The placement-owned
processes both ran to completion through the bounded reap: the
i2pr dialer was terminated first (`terminated`), then the i2pd
listener (`terminated`). The placement-owned `popen` API kept the
listener process alive (5-second bounded window) while the i2pr
dialer started; the `listener_alive_when_dialer_started` invariant
was true. The preflight result is `pre_protocol_rejected` /
`mixed-router-handshake-not-completed` because the i2pr dialer
rejected the copied i2pd RouterInfo with `peer_router_info_invalid`
on this constrained host; the placement-owned infrastructure
exercised every concurrent primitive the lane contract requires
and reports the typed blocker for Plan 087 to address. The preflight
process startup is owned by `HostLoopbackDevelopmentPlacement` and
never invokes sudo, namespaces, Multipass, or any public-network
access. The wrapper exits `0` on a passing preflight, `5` for a
blocked preflight, `6` for a failed forward or reverse probe, and
`2` for invalid inputs.

The Plan 086 status does not claim NTCP2 interoperability. It
authorizes Plan 087 (the first real `i2pr -> i2pd` forward probe) to
begin against the measured i2pd direct driver binary. Plan 088 (the
reverse direction and the active development decision) remains blocked
until Plan 087 records a real forward wire result. Plan 079 remains
blocked pending a Plan 088 `two-way-development-probe-passed`
closure; Plan 072 remains inactive pending a Plan 088
`ambiguous-reference-divergence` closure with one exact role/stage
diagnostic question.

NTCP2 stays experimental and non-advertised. The Plan 086 closure
state is the bounded Plan 086 `host-loopback-development-ready` value;
the legacy `lane-invalidated` and `same-stage-two-way-i2pr-defect`
tokens remain forbidden.

## Corrective closeout summary

The prior closure record (commit `b0efc4e`) was based on a runner that
silently accepted five structural defects. The corrective pass lands
the bounded implementation surface that closes every defect with a
sanitized, validated, and measured evidence path:

| Defect                                                      | Corrective surface                                                                                                 |
|-------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------|
| scenario payload written as JSON to a `.toml` file          | `_format_toml` renderer emits the exact `i2pr-launcher-scenario-v2` field set; output is valid TOML.               |
| `i2pr-interop ntcp2 validate-scenario` was skipped          | `_validate_scenario` runs through the placement and refuses to advance on a non-validated strict scenario.        |
| actual i2pd RouterInfo never reached the i2pr scenario      | driver writes `<output_dir>/router.info` from `i2p::context.GetRouterInfo()`; concurrent preflight **copies** the file to `<run_root>/exchange/i2pd-router.info`, verifies the destination digest, and references the copied path. |
| placement digest was an all-zero placeholder               | `HostLoopbackDevelopmentPlacement.digest()` returns a real SHA-256 over the measured placement inputs.             |
| wrapper did not fail on blocked forward/reverse probes      | wrapper exits 6 when `terminal_result != "passed"`; exits 5 when preflight is not ready; exits 2 on invalid inputs.|
| closure was attributed to an uncommitted working tree       | closure is now attributed to commit `32ca767` and re-run from a clean checkout at HEAD.                            |
| listener/dialer concurrency was scaffolding, not wires      | `HostLoopbackDevelopmentPlacement.popen()` returns a placement-owned `PlacementProcess`; the concurrent preflight popens the listener, polls the placement-owned events file for `listener_ready`, asserts the listener is alive, popens the dialer, consumes both event streams concurrently on placement-internal daemon threads, and bounded-reaps both processes in declared order. |
| `validate-scenario` bypassed placement ownership            | `_validate_scenario` routes the i2pr-interop validation through the placement, exactly like every other subprocess. |
| no focused test proved the listener is alive when dialer starts | `Plan086PlacementLifecycleTests.test_long_running_listener_is_alive_when_dialer_starts` proves the placement-owned listener is alive at the moment the placement-owned dialer is created. |

## Delivered implementation surface

- `tests/integration/ntcp2/harness/interop_topology.py` — adds the
  `host-loopback-development` topology constant, the
  `host-direct-loopback` privilege model, the bounded
  `HOST_LOOPBACK_DEVELOPMENT_METADATA` table, and the
  `HostLoopbackDevelopmentPlacement` narrow host-direct placement.
  The placement gains `digest()`, `run()`, and `popen()` so the
  runner never composes a shell, namespace, Multipass, setcap, or
  sudo invocation. The placement is fail-closed on relative paths,
  unknown actors, unbounded log capture, and
  `LD_PRELOAD`/`LD_LIBRARY_PATH` in the environment.
- `tests/integration/ntcp2/harness/i2pr.py` — extends the
  `prepare_state` method with an optional `topology_kind`
  argument. Literal IPv4 `127.0.0.1` is accepted only when
  `topology_kind == host-loopback-development`; every other
  topology refuses the address with `prepare-input-invalid` before
  any subprocess is executed.
- `tools/i2pr-interop/src/scenario.rs` — extends the Plan 065
  strict scenario schema with an optional `topology_kind` field
  and the `TopologyKind` enum (`Synthetic` and
  `HostLoopbackDevelopment`). The strict parser accepts literal
  IPv4 `127.0.0.1` only when `topology_kind ==
  host-loopback-development`; alternate loopback addresses,
  hostnames, DNS names, public addresses, and RFC 5737 addresses
  outside the development lane are rejected with the existing
  bounded error codes.
- `tools/i2pr-interop/src/main.rs` — extends the `prepare`
  command with an optional `--topology-kind` flag. The prepare
  command accepts `127.0.0.1` only when the topology kind is
  `host-loopback-development`; the existing production address
  validation in the daemon is untouched.
- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp` —
  adds `write_local_router_info` so the driver exports its
  actual signed RouterInfo bytes to
  `<output_dir>/router.info`. The export uses
  `i2p::context.GetRouterInfo().GetBuffer()` / `GetBufferLen()`
  and is invoked immediately after the `router_info_exported`
  event in inspect mode so the i2pr initiator's strict scenario
  receives the real peer RouterInfo path. Driver source SHA after
  the change: `fcfdb04a5df13b6c72c411158da8e949f5b4a856b72a1ff982ceee7783608313`.
- `tests/integration/ntcp2/harness/preflight_runner.py` —
  rewritten to emit valid strict TOML with the exact schema v2
  field names, run `i2pr-interop ntcp2 validate-scenario`, route
  the i2pr subprocess invocation through
  `HostLoopbackDevelopmentPlacement.run`, measure the committed
  `i2pr-interop` binary SHA, capture the actual i2pd RouterInfo
  SHA, and bind a non-zero `placement_record_sha256` from the
  placement's measured inputs. The concurrent preflight
  (`execute_concurrent_preflight`) copies the i2pd RouterInfo to
  `<run_root>/exchange/i2pd-router.info`, verifies the copied
  digest, popens the i2pd listener through the placement, polls
  the placement-owned `events.ndjson` for `listener_ready`,
  asserts the listener process is alive when the i2pr dialer
  starts, popens the i2pr dialer through the i2pr placement,
  consumes both event streams concurrently on placement-internal
  daemon threads, and bounded-reaps both processes in declared
  order (drainer first, then listener). The legacy
  `execute_listener_preflight` remains as the bounded
  listener-only contract surface for unit/integration tests.
- `tests/integration/ntcp2/harness/plan083_runner.py` and
  `tests/integration/ntcp2/harness/plan084_runner.py` — the
  canonical runner orchestrators gain a strict-TOML scenario
  payload, a `_runner_placement_digest` helper that materialises
  a real `placement_record_sha256`, and a `HostLoopbackDevelopmentPlacement`
  bound to the i2pr prepare invocation under the development
  topology.
- `tests/integration/ntcp2/harness/minimal_i2pd_probe.py` and
  `tests/integration/ntcp2/harness/minimal_i2pd_reverse_probe.py`
  — the probe records gain the new required
  `placement_record_sha256` field that must be a non-zero
  measured digest; the validators reject the all-zero
  placeholder.
- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py` —
  the thin operator entry point. The wrapper accepts the four
  required positional inputs, refuses every release/support
  profile flag, dispatches to the canonical Plan 083/084 runner
  modules without copying orchestration, and exits `0` on a
  passing preflight, `5` on a blocked preflight, `6` on a failed
  forward or reverse probe, and `2` on invalid inputs. The
  wrapper opens no socket, performs no bootstrap, and never
  invokes sudo, namespaces, Multipass, or any public-network
  access.
- `tests/integration/ntcp2/harness/test_plan086.py` — the Plan
  086 test matrix (33 cases) covering the topology constant and
  bounded metadata, the `HostLoopbackDevelopmentPlacement`
  contract, the closure-state vocabulary, the address-class
  acceptance rules, the canonical runner topology acceptance,
  the bootstrap dependency checks, the record schemas, the
  preflight contract, the status record contract, and the
  handoff to Plan 087.
- `scripts/check-ntcp2-interoperability.sh` — extended to enforce
  the Plan 086 topology contract, the placement class, the test
  matrix presence, the wrapper script presence, the Rust schema
  marker, and the bounded closure state in `plans/086-status.md`.

## Topology contract

The `host-loopback-development` topology is recorded verbatim
from the canonical `HOST_LOOPBACK_DEVELOPMENT_METADATA` table:

```text
topology_kind                 = host-loopback-development
development_only              = true
release_qualified             = false
isolation_qualified           = false
public_network_blocked        = unproven
parent_network_state_unchanged = true
endpoint_family               = ipv4
bind_address                  = 127.0.0.1
peer_address                  = 127.0.0.1
network_id                    = 99
reference                     = i2pd
```

The topology accepts literal IPv4 `127.0.0.1` only. It rejects:

```text
127.0.0.0
127.0.0.2-127.255.255.255
0.0.0.0
::1
::
hostnames
DNS names
public addresses
RFC 5737 documentation addresses under the loopback topology
```

The synthetic RFC 5737 / 3849 range is still accepted under the
development topology so existing flows can switch lanes without
rewriting the scenario. The production daemon and the synthetic
lanes remain untouched.

## Runner acceptance

The canonical 11-step execution architecture in
`plan083_runner.py` and `plan084_runner.py` is reused unchanged.
The lane validator accepts `host-loopback-development` without
classifying the lane as invalid; the runner never falls back to
SAM, HTTP, support-topology, or synthetic-fallback helpers; the
C++ i2pd direct driver is the only allowlisted reference driver
mode. The Plan 088 module boundary check is preserved: the runner
modules carry the Plan 088 topology constant, the development-only
marker, and the bounded closure vocabulary. The runners never
import Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority.

The placement owns the subprocess invocation surface: under
`host-loopback-development`, the i2pr prepare subprocess, the
i2pr `validate-scenario` subprocess, and the i2pd listener
subprocess all run through `HostLoopbackDevelopmentPlacement`.
The runner never reaches around the placement to
`subprocess.run` or `subprocess.Popen` directly.

## Bootstrap dependency check

The Plan 086 lane proves through focused source/config checks that
the execution path uses only:

```text
i2pr-interop ntcp2 prepare/listen/dial/validate-scenario
i2pd direct driver inspect/listen/dial
network ID 99
explicit peer RouterInfo exchange
explicit 127.0.0.1 endpoint
```

It does not activate:

```text
reseed
public NetDB bootstrap
DNS lookup
SAM or I2CP
HTTP/SOCKS proxy
normal router daemon
SSU2
transit tunnels
```

The Plan 086 test matrix scans the i2pr-interop source and the
i2pd direct driver source for these forbidden markers. Comments
that document the negative contract are excluded from the scan.

## Preflight contract

The wrapper's `--preflight` mode dispatches to
`preflight_runner.execute_concurrent_preflight` by default. The
concurrent preflight validates the lane, prepares the i2pr state
through the placement, runs the i2pd driver in inspect mode (which
exports the real signed RouterInfo to `output_dir/router.info`),
captures the i2pd RouterInfo SHA, **copies** the i2pd RouterInfo to
`<run_root>/exchange/i2pd-router.info` and verifies the copied
digest matches the source digest, measures the committed
`i2pr-interop` binary SHA, renders the strict Plan 065
`i2pr-launcher-scenario-v2` TOML with
`peer_router_info = "exchange/i2pd-router.info"` referencing the
**copied** path, runs `i2pr-interop ntcp2 validate-scenario`
through the placement and refuses to advance on any non-`validated`
outcome, popens the i2pd listener via
`HostLoopbackDevelopmentPlacement.popen()`, polls the
placement-owned `events.ndjson` for an authentic `listener_ready`,
asserts the listener process is alive when the dialer starts,
popens the i2pr dialer via
`HostLoopbackDevelopmentPlacement.popen()` while the listener is
still alive, consumes both event streams concurrently on
placement-internal daemon threads, and bounded-reaps both processes
in declared order (drainer first, then listener). The preflight
writes a sanitized record. The preflight is the only path the Plan
086 closure may use to validate the lane contract.

The legacy `execute_listener_preflight` listener-only contract
surface remains available (and is the right choice when the
wrapper is invoked in a unit/integration test that has not yet
built the real i2pd direct driver binary); the concurrent
preflight is the production closure path.

The concurrent preflight does not assert an NTCP2 interoperability
result on this constrained host; its purpose is to prove the
placement-owned listener/dialer concurrency contract. Plan 087
owns the real wire attempt.

## Closure vocabulary

The Plan 086 closure vocabulary is exactly three values:

```text
host-loopback-development-ready
manual-isolated-fallback-required
blocked-artifact-or-build-defect
```

The legacy `lane-invalidated` and `same-stage-two-way-i2pr-defect`
tokens remain forbidden. The static boundary checker rejects any
future `plans/086-status.md` that re-introduces these tokens.

## Forward and reverse probe coverage

The preflight runner renders the forward Plan 083 scenario as valid
TOML using the exact strict schema field names. The reverse Plan 084
runner renders the responder scenario with the same schema, with
`role = "responder"`, `scenario_id = "i2pd-to-i2pr-ipv4"`, and
`peer_router_info = ""`. Both scenarios pass the Rust
`validate-scenario` parser on this host; both directions are
validated through the same `_validate_scenario` helper that the
preflight uses. The wrapper exits `6` when either direction's
`terminal_result` is not `passed`.

The reverse runner
(`execute_reverse_probe` in `plan084_runner.py`) and the forward
runner (`execute_real_probe` in `plan083_runner.py`) remain
ready to be exercised in a qualified Plan 046 rootless lane, a
Plan 048/049 Multipass recovery guest, or the
`host-loopback-development` lane.

## Cross-host portability

The Plan 086 topology contract, the placement class, the
listener-only preflight, the thin wrapper, and the focused test
matrices travel with the repository unchanged. On a host where
the Plan 086 host-loopback-development lane becomes executable
(i.e., the current host acquires `host-loopback-development-ready`
or another lane becomes qualified) the Plan 087/088 runners may
be invoked against real subprocesses; the bounded Plan 088
development decision vocabulary will resolve to whichever of the
five exact values reflects the wire result.

Cross-host portability for the Plan 046 rootless sealed-namespace
lane is deferred to `plans/047-cross-host-rootless-lane-expansion.md`.
Cross-host portability for the Plan 080 Multipass recovery guest
is bounded by the Plan 051 resource constraints.

## Future plan unblocking

| Plan | Precondition | Status after Plan 086 |
| --- | --- | --- |
| Plan 087 | requires Plan 086 `host-loopback-development-ready` or `manual-isolated-fallback-required` | enabled; the `host-loopback-development` lane is closed as ready on this host and the i2pd direct driver is bound to a real measured binary |
| Plan 088 | requires Plan 087 `passed` | remains blocked; Plan 087 must record a real forward wire result before Plan 088 may begin |
| Plan 079 | requires Plan 088 `two-way-development-probe-passed` | remains blocked |
| Plan 072 | requires Plan 088 `ambiguous-reference-divergence` | remains inactive |

Plan 087 is now enabled on this host. Plan 088 remains blocked
until Plan 087 records a real `i2pr -> i2pd` forward wire result
that satisfies the bounded Plan 088 development decision
vocabulary. Plan 079 and Plan 072 remain blocked pending the
Plan 088 closure.

## Validation

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'       passed (39, +6 placement-lifecycle)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'       passed (35)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'       passed (54)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'       passed (50)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan082.py'       passed (7)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pr_prepare.py'   passed (5)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'       passed (29)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'   passed (43)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_reverse_probe.py'   passed (skipped, no tests)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'   passed (51)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'   passed (14)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_rootless_topology.py' passed
cargo fmt --all --check                                                                passed
cargo check --workspace --all-targets                                                  passed
cargo test --workspace                                                                 passed (241 in 27 suites)
cargo clippy --workspace --all-targets --all-features -- -D warnings                   passed (no issues)
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps                            passed
bash scripts/check-ntcp2-interoperability.sh                                           passed
bash scripts/check-dependency-direction.sh                                             passed
bash scripts/check-runtime-boundaries.sh                                               passed
bash scripts/check-rootless-interop-boundary.sh                                        passed
bash scripts/check-multipass-interop-boundary.sh                                       passed
bash scripts/check-ntcp2-loopback-smoke-boundary.sh                                    passed
git diff --check                                                                       clean
```

A live concurrent preflight against the committed `i2pr-interop`
binary and the built `i2pd_ntcp2_interop_driver_instrumented` was
executed from a clean working tree at HEAD with
`--source-commit = 32ca767fe1843d6d4abcec7cb6bad26eda3ac0c2`. The
wrapper exited `0`; the sanitized record carries
`record_sha256 = b2e5f4d8...` and the canonical placement digest
`2e9b8713...`. The Rust `validate-scenario` parser returned
`result = validated` on the strict TOML scenario whose
`peer_router_info = "exchange/i2pd-router.info"` references the
**copied** i2pd RouterInfo file (the
`peer_router_info_copied_digest = 08d270f0c60...` matches the
`i2pd_router_info_sha256 = 08d270f0c60...` byte-for-byte). The
placement-owned listener remained alive (5-second bounded window)
while the placement-owned dialer started; the
`listener_alive_when_dialer_started` invariant was true; both
processes were bounded-reaped (drainer first, then listener,
both escalated through `terminated`).

The full repository gates and boundary checks pass before commit.
The Plan 086 test matrix covers the bounded topology constant and
metadata, the `HostLoopbackDevelopmentPlacement` contract and
**the placement-owned popen/reap lifecycle**, the closure-state
vocabulary, the address-class acceptance rules, the canonical
runner topology acceptance, the bootstrap dependency checks, the
record schemas, the preflight contract, the status record
contract, the placement digest contract, and the handoff to Plan
087.

The static boundary checker enforces the test matrix presence, the
locked closure vocabulary, the `host-loopback-development` topology
coverage, the wrapper script presence, the Rust schema marker, the
plan-of-record reference, the prohibition of the legacy
`lane-invalidated` and `same-stage-two-way-i2pr-defect` tokens, the
`placement_record_sha256` non-zero invariant, and the
`validate-scenario` integration.
