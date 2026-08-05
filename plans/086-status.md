# Plan 086 status: host-loopback development lane enablement

## Status

Implementation surface delivered and lane-closure path exercised on
2026-08-05 against the corrective commit
`3d77afbb012b53d78705488b756951e799bfc85f`. The
`host-loopback-development` topology kind, the
`HostLoopbackDevelopmentPlacement`, the bounded literal IPv4 `127.0.0.1`
acceptance, the thin wrapper
`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`, the
listener-only preflight, the actual i2pd RouterInfo export, the strict
TOML scenario render with `validate-scenario`, and the canonical
`i2pd_ntcp2_interop_driver_instrumented` binary are landed and green
locally against the corrected source commit.

The corrective pass landed as commit
`3d77afbb012b53d78705488b756951e799bfc85f`; this closure record was
re-rendered as commit `9cefde95d112ec29da8798e0012db046b5d869cf`
without changing the corrective code surface. Future history may
amend this closure record; the source-commit attribution in the
sanitized records below always reflects the precise commit that
produced the preflight invocation.

```text
status = host-loopback-development-ready
```

The corrective commit supersedes the prior closure record that
attributed the lane-closure pass to commit
`b0efc4e244ea51f7bef89f17d49717ca2c9e85dd`, which was based on a
runner that wrote invalid JSON as `.toml`, skipped
`i2pr-interop ntcp2 validate-scenario`, and rendered the placeholder
placement digest. The Plan 086 closure is now re-attributed to commit
`3d77afbb012b53d78705488b756951e799bfc85f`.

The Plan 086 preflight against the real i2pd direct driver binary on
this host produced the following sanitized record digests:

```text
source_commit                       = 3d77afbb012b53d78705488b756951e799bfc85f
reference_revision                  = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
forward_preflight_record_sha256     = ab856b7db29addc9b7fa9165b2e29c7100b87cd864562469b48a0f6ddde90cb6
placement_record_sha256             = 109dba920420995b5446504f311d79abbcd3c5c5ed242dfaf6d71a7114fa42cd
i2pr_binary_sha256                  = 6eb9133dc128f2f1d9528e0d41ef7c165228dcde96d47aaea92faffcf1714d85
i2pd_binary_sha256                  = d92890746e022101fe6c8e6e0f54929a6c583937ff3148168cab72ebcaf4d7db
i2pr_router_info_sha256             = 6ba9effff7f6f82d094e14a36bdfff509692cdea355be7ef5ae797d0ed102d18
i2pd_router_info_sha256             = 93bc8605a08303022292ce2a3529a49194e22875561dfb44a07669356f2b6446
i2pr_router_hash_sha256             = bdaa0a546ad838e8a32667c46d8e164b104cd0a05be6ae02ea953a8c86eda6f7
i2pd_router_hash_sha256             = f3ee2e29303afd1ed04d56d25c23a87ca53c898b70a8a021550b1ce252194b5f
i2pd_driver_source_sha256           = fcfdb04a5df13b6c72c411158da8e949f5b4a856b72a1ff982ceee7783608313
lane_qualification_sha256           = 477a79088c699e52d99e3d96e14f0f9801ee9cf4e50f50c656406201b6321de3
highest_stage_reached               = listener_ready
reason_code                         = not_started
terminal_result                     = pre_protocol_rejected
cleanup_result                      = clean
parent_network_state_unchanged      = true
topology_kind                       = host-loopback-development
```

The Plan 086 preflight exercised the canonical 11-step execution
architecture with the measured i2pd direct driver binary. The
`validate-scenario` Rust command returned
`{"schema":"i2pr-interop-scenario-validated-v1","result":"validated"}`
on the strict `i2pr-launcher-scenario-v2` TOML whose `peer_router_info`
references the actual i2pd RouterInfo file the i2pd driver exported
in inspect mode. The preflight process startup is owned by
`HostLoopbackDevelopmentPlacement` and never invokes sudo,
namespaces, Multipass, or any public-network access. The wrapper exits
`0` on a passing preflight, `5` for a blocked preflight, `6` for a
failed forward or reverse probe, and `2` for invalid inputs.

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
| actual i2pd RouterInfo never reached the i2pr scenario      | driver writes `<output_dir>/router.info` from `i2p::context.GetRouterInfo()`; scenario `peer_router_info` references the file. |
| placement digest was an all-zero placeholder               | `HostLoopbackDevelopmentPlacement.digest()` returns a real SHA-256 over the measured placement inputs.             |
| wrapper did not fail on blocked forward/reverse probes      | wrapper exits 6 when `terminal_result != "passed"`; exits 5 when preflight is not ready; exits 2 on invalid inputs.|
| closure was attributed to an uncommitted working tree       | closure is now attributed to commit `3d77afb` and re-run from a clean checkout at HEAD.                            |

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
  placement's measured inputs.
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

The wrapper's `--preflight` mode stops before any peer connection
completes. It validates the lane, prepares the i2pr state through
the placement, runs the i2pd driver in inspect mode (which exports
the real signed RouterInfo to `output_dir/router.info`), captures
the i2pd RouterInfo SHA, measures the committed `i2pr-interop`
binary SHA, renders the strict Plan 065 `i2pr-launcher-scenario-v2`
TOML with `peer_router_info` referencing the i2pd RouterInfo file,
runs `i2pr-interop ntcp2 validate-scenario` and refuses to advance
on any non-`validated` outcome, and starts the i2pd listener with
the measured i2pd direct driver. It terminates before any peer
connection completes and writes a sanitized record. The preflight
is the only path the Plan 086 closure may use to validate the
lane contract. The preflight never starts a dialer.

The preflight and the forward/reverse probe paths share the same
canonical runner architecture. The preflight does not exercise
any TCP connection, NTCP2 handshake, authenticated frame, or
I2NP DeliveryStatus decode; it is a lane readiness probe, not a
protocol execution.

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
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'       passed (33)
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

A live preflight against the committed `i2pr-interop` binary and
the built `i2pd_ntcp2_interop_driver_instrumented` was executed
from a clean working tree at HEAD with `--source-commit =
3d77afbb012b53d78705488b756951e799bfc85f`. The wrapper exited `0`;
the sanitized record carries `record_sha256 = ab856b7d...` and
the canonical placement digest `109dba92...`. The Rust
`validate-scenario` parser returned `result = validated` on the
strict TOML scenario whose `peer_router_info` references the actual
i2pd RouterInfo file the i2pd direct driver exported in inspect
mode.

The full repository gates and boundary checks pass before commit.
The Plan 086 test matrix covers the bounded topology constant and
metadata, the `HostLoopbackDevelopmentPlacement` contract, the
closure-state vocabulary, the address-class acceptance rules, the
canonical runner topology acceptance, the bootstrap dependency
checks, the record schemas, the preflight contract, the status
record contract, the placement digest contract, and the handoff
to Plan 087.

The static boundary checker enforces the test matrix presence, the
locked closure vocabulary, the `host-loopback-development` topology
coverage, the wrapper script presence, the Rust schema marker, the
plan-of-record reference, the prohibition of the legacy
`lane-invalidated` and `same-stage-two-way-i2pr-defect` tokens, the
`placement_record_sha256` non-zero invariant, and the
`validate-scenario` integration.
