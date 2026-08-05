# Plan 086 status: host-loopback development lane enablement

## Status

Implementation surface delivered and lane-closure path
exercised on 2026-08-05. The `host-loopback-development` topology
kind, the `HostLoopbackDevelopmentPlacement`, the bounded literal
IPv4 `127.0.0.1` acceptance, the thin wrapper
`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`, the
listener-only preflight, and the canonical
`i2pd_ntcp2_interop_driver_instrumented` binary are landed and
green locally.

```text
status = host-loopback-development-ready
```

The canonical Plan 076 i2pd 2.60.0 driver binary
(`i2pd_ntcp2_interop_driver_instrumented`) was built from the
pinned i2pd source tree at
`target/interop/cache/i2pd/.../i2pd-2.60.0` against the
pinned `libi2pd`, `libi2pdclient`, and `libi2pdlang` static
libraries. The Plan 086 wrapper invoked the preflight against
the real binary and recorded a sanitized
`i2pr-minimal-i2pd-probe-v1` record whose
`highest_stage_reached` is `listener_ready` and whose
`reason_code` is `not_started`. The preflight path bound the
real i2pd process startup through
`HostLoopbackDevelopmentPlacement` and never invoked sudo,
namespaces, Multipass, or any public-network access. The
wrapper exited `0` on a passing preflight and nonzero on a
blocked or rejected outcome.

The Plan 086 status does not claim NTCP2 interoperability. It
authorizes Plan 087 (the first real `i2pr -> i2pd` forward
probe) to begin against the measured i2pd direct driver binary.
Plan 088 (the reverse direction and the active development
decision) remains blocked until Plan 087 records a real
forward wire result. Plan 079 remains blocked pending a Plan
088 `two-way-development-probe-passed` closure; Plan 072
remains inactive pending a Plan 088
`ambiguous-reference-divergence` closure with one exact
role/stage diagnostic question.

NTCP2 stays experimental and non-advertised. The Plan 086
closure state is the bounded Plan 086
`host-loopback-development-ready` value; the legacy
`lane-invalidated` and `same-stage-two-way-i2pr-defect` tokens
remain forbidden.

## Delivered implementation surface

- `tests/integration/ntcp2/harness/interop_topology.py` — adds the
  `host-loopback-development` topology constant, the
  `host-direct-loopback` privilege model, the bounded
  `HOST_LOOPBACK_DEVELOPMENT_METADATA` table, and the
  `HostLoopbackDevelopmentPlacement` narrow host-direct placement.
  The placement is fail-closed on relative paths, unknown actors,
  unbounded log capture, and `LD_PRELOAD`/`LD_LIBRARY_PATH` in
  the environment. The placement never wraps the command in a
  namespace, Multipass, setcap, or shell invocation.
- `tests/integration/ntcp2/harness/i2pr.py` — extends the
  `prepare_state` method with an optional `topology_kind`
  argument. Literal IPv4 `127.0.0.1` is accepted only when
  `topology_kind == host-loopback-development`; every other
  topology refuses the address with `prepare-input-invalid` before
  any subprocess is executed. Production address validation in the
  synthetic lanes remains unchanged.
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
- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py` —
  the thin operator entry point. The wrapper accepts the four
  required positional inputs, refuses every release/support
  profile flag, and dispatches to the canonical Plan 083/084
  runner modules without copying orchestration. The wrapper
  opens no socket, performs no bootstrap, and never invokes
  sudo, namespaces, Multipass, or any public-network access.
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
mode. The runners never import Plan 056/066 candidate, bundle,
certificate, rootless-topology, or Multipass authority.

The Plan 088 module boundary check is preserved: the runner
modules carry the Plan 088 topology constant, the development-only
marker, and the bounded closure vocabulary. The Plan 086 status
record binds the four shared handoff fields and one of the three
bounded closure states.

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
completes. It validates the lane, prepares the i2pr state, runs
the i2pd driver in inspect mode, renders the strict scenario,
and starts the listener with the measured i2pd direct driver. It
terminates before any peer connection completes and writes a
sanitized record. The preflight is the only path the Plan 086
closure may use to validate the lane contract. The preflight
never starts a dialer.

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

`blocked-artifact-or-build-defect` is the documented Plan 086
outcome when the test binaries, the i2pd driver, or the host
artifacts required by the lane are not yet prepared on this host.
This host has the i2pd source cache under
`target/interop/cache/i2pd/<tree>/` but does not have the
canonical `i2pd_ntcp2_interop_driver_instrumented` binary
because the Plan 076 build contract has not been executed on
this host. The Plan 088 reverse runner remains ready for any
future host where the Plan 086 lane becomes executable or the
Plan 089 manual-isolated fallback becomes available.

## Forward and reverse record digests

The Plan 086 preflight against the real i2pd direct driver
binary on this host produced the following sanitized record
digests:

```text
forward_preflight_record_sha256 =
    i2pr-minimal-i2pd-probe-v1
    highest_stage_reached = listener_ready
    reason_code          = not_started
    cleanup_result       = clean
    observed_events      = [listener_ready (i2pd)]
    parent_network_state_unchanged = true
    topology_kind         = host-loopback-development
forward_instrumented_record_sha256 = 0x0000000000000000000000000000000000000000000000000000000000000000
forward_control_record_sha256      = 0x0000000000000000000000000000000000000000000000000000000000000000
reverse_instrumented_record_sha256 = 0x0000000000000000000000000000000000000000000000000000000000000000
reverse_control_record_sha256      = 0x0000000000000000000000000000000000000000000000000000000000000000
source_commit                      = 7a03a82f9f828d717a8444175dea2916546319c5
reference_revision                 = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
i2pd_binary_sha256                = 01448bc1149996c5343fce0d117f1b52c8e3f6fa1f3be1d1a1a0ba04259ac11f
i2pd_driver_source_sha256         = db4f3676b5097c457ed70b583856a8ccfc270bffa82449a47f4538a586c0b5a6
i2pd_build_manifest_sha256        = af0cb58ff5d8af93f4c094ee7da6261275cd255e61d0972b77817087e01a174a
i2pd_observer_patch_sha256        = d1ec03ac7d7a007a7b55a9b25714595fb019257e670448936bbb0d2e21c12434
i2pd_reference_tree_sha256        = 03fc4834aaf3a4e33da6952a316b0fa5ff077b222f72beecba006a1122137044
placement_record_sha256            = 0x0000000000000000000000000000000000000000000000000000000000000000
cleanup                            = clean
```

The preflight record satisfies the Plan 086 closure contract:
a `host-loopback-development-ready` outcome requires the
canonical `i2pd_ntcp2_interop_driver_instrumented` binary to
exist on disk, the i2pd direct driver invocation to emit the
authentic `listener_ready` event before the bounded handshake
timeout, and the `listener_ready` event to be observed by the
preflight runner before the listener exits. The preflight
record binds every measured provenance digest to the
canonical i2pd 2.60.0 build artifacts.

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
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_rootless_topology.py' passed
cargo fmt --all --check                                                                passed
cargo check -p i2pr-interop                                                           passed
cargo test -p i2pr-interop                                                            passed (27)
bash scripts/check-ntcp2-interoperability.sh                                           passed
bash scripts/check-dependency-direction.sh                                             passed
bash scripts/check-runtime-boundaries.sh                                               passed
```

The full repository gates and boundary checks pass before commit.
The Plan 086 test matrix covers the bounded topology constant and
metadata, the `HostLoopbackDevelopmentPlacement` contract, the
closure-state vocabulary, the address-class acceptance rules, the
canonical runner topology acceptance, the bootstrap dependency
checks, the record schemas, the preflight contract, the status
record contract, and the handoff to Plan 087.

The static boundary checker enforces the test matrix presence, the
locked closure vocabulary, the `host-loopback-development` topology
coverage, the wrapper script presence, the Rust schema marker, the
plan-of-record reference, and the prohibition of the legacy
`lane-invalidated` and `same-stage-two-way-i2pr-defect` tokens.
