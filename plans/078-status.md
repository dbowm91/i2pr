# Plan 078 status — closed-as-blocked-protocol-defect

## Status

Plan 078 closes as `blocked-protocol-defect` per its acceptance criteria,
with the first divergent protocol stage recorded. The Plan 080 Multipass
lane prequalification promoted the host from `inherited-descriptors-seccomp`
to `remote-manual` (`qualified = true`, `scope = full-runtime`,
`full_runtime_lane = available`); inside that qualified lane, the
`i2pr-to-i2pd-ipv4` direction produced a typed `rejected` outcome at the
i2pr-launcher pre-protocol stage. The `i2pd-to-i2pr-ipv4` direction was
not attempted because the first direction blocked. Plan 079 must not
start from this result.

The selected capability after Plan 080:

```text
selected_lane = remote-manual
scope = full-runtime
full_runtime_lane = available
qualified = true
reason_code = lane-qualified
plan080_qualification_record_sha256 = 2ba91fe1a7f2c2309359d8e3fb2de199fac765b387272ef13d668a7ddb14063d
```

The selected capability before Plan 080 (preserved as the historical
preflight record):

```text
selected_lane = inherited-descriptors-seccomp
scope = reduced-scope-diagnostic
full_runtime_lane = unavailable
qualified = false
reason_code = full_runtime_lane_unavailable
```

## Blocked-protocol-defect evidence

The sanitized direction record is at
`target/interop/evidence/multipass/plan080-20260801034138-c8bec3f5/directions/i2pr-to-i2pd-ipv4.json`.

```text
scenario_id = i2pr-to-i2pd-ipv4
direction = i2pr-to-reference
reference = i2pd
reference_version = 2.60.0
reference_revision = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
data_phase_mode = initiator-data-only
expected_observation = i2pr-sent-only
actual_typed_result = rejected
known_deviation = typed-harness-operation-failed
process_counters.i2pr = {started: 1, exited: 1, forced: 0}
process_counters.i2pd = {started: 1, exited: 1, forced: 0}
resource_counters = {handshakes: 0, i2np_sent: 0, i2np_received: 0, frames_sent: 0, frames_received: 0}
i2pr_router_info_sha256 = 0000000000000000000000000000000000000000000000000000000000000000
reference_router_info_sha256 = 6c76a28033048ad189460d4cc444c2500900562814ef60a7bfd98734d3c5fb1c
cleanup_result = clean
```

The first divergent protocol stage is `i2pr_adapter_export_router_info`
inside the Plan 045 mixed-runner. The i2pd side produced a real signed
RouterInfo; the i2pr side never produced one. The harness swallowed the
underlying exception as `typed-harness-operation-failed`. Reproduction
from fresh state was confirmed by a direct `mixed_runner.py` invocation
that returned `actual_typed_result=failed_cleanup,
reason_code=evidence-finalization-failed`.

The Plan 074/078 bounded-correction policy restricts changes to the
owning i2pr or test-driver surface and forbids refactors that expand
into NetDB, tunnels, SAM/I2CP, SSU2, or public-network behaviour. The
i2pr-launcher side is owned by the Plan 042 wire-driver plan and is
explicitly out of scope for Plan 078. Plan 078 therefore closes as
`blocked-protocol-defect` without a passing record.

## Preflight evidence

The exact source commit was:

```text
b706a396a72b7b36ee50db0fc7c11c24139fa2ce
```

The pinned i2pd source revision remains:

```text
f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
```

The Plan 077 probe selected the reduced-scope capability because Docker's
daemon was inaccessible and QEMU TCG was absent. The remote workflow is
manual-only and therefore was not treated as a qualified lane. The resulting
sanitized records are ignored working data:

```text
probe.json sha256         = 84412c024c1f6c9c23129e3743d515b245df5dc82a72f69bbf7c019408c71e7a
qualification.json sha256 = 654f3f3c078342592f633e9046548c04a183dcee10fdf700521fde83e95370d8
qualification record_sha256 = b6ae500f18ddaa3aff739b029380c6816dfdac27910a241899d86b1aa317cb9c
```

The qualification record has empty artifact digests because no lane executed.
Consequently, no i2pr or i2pd binary digest exists for a Plan 078 protocol
run. The tracked i2pd driver inputs were measured for provenance review:

```text
driver source sha256       = cb1c0a0d8681179928252265b8cf83ce3abd26dfa469afb888d470e7538133e1
observer header sha256     = 77ca510439ac430362f15a0dd0006318963d22810d91f077ee20e7a13066d12b
observer source sha256     = 1be6e5db85aed063ba5d88aa252801b1e26275a0120c5e21628def0c2cbf6b28
observer patch sha256      = e1feef9cca60d3c184db3b79ee56f073e5916938d38be48ba7f462b0875b9595
run-driver.sh sha256       = e274ab9052ae9b1afaad1225e9c79651ac6f000692b43221a2ee599139ffe5df
source-lock.json sha256    = fd321bc7750209554ecf2e836d2f23112be291db99fcda12e42c611ec3053a68
manifest schema sha256    = e4be635d00c6aa074d58d61ad9f5e3fe0762741f3202fc5bfc5acc7bea7037bc
```

The pre-existing Multipass instance was inspected read-only and rejected: its
ownership contract is not verified and its guest source manifest points at a
different historical commit. It was not adopted, resumed, modified, or
destroyed.

## Exact commands and results

```text
bash scripts/interop/probe-constrained-host-lanes.sh
  passed; selected inherited-descriptors-seccomp

bash scripts/check-constrained-host-lane-boundary.sh
  passed

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
  passed; 18 tests

bash scripts/interop/multipass/run-evidence-lane.sh --inspect --run-id plan049-20260721105135-3d7e7a68
  inspected read-only; ownership/contract verification failed; no mutation

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
  passed; 51 tests
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
  passed; 35 tests
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
  passed; 13 tests
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
  passed
```

The loopback tests are contract tests only. They did not launch a real
reference process and are not Plan 078 protocol evidence.

## Corrections and closure boundary

No protocol correction was made because the required full-runtime lane was
unavailable and the stop rule prohibits starting a run without it. No schema,
orchestration framework, isolation path, or synthetic result was added. The
existing Plan 077 fail-closed preflight is the owning correction for this
host state. While running the CI-equivalent static checks, the build-contract
validator exposed that `offline-reuse.sh` built the launcher without the
pinned `+1.95.0` toolchain marker. The script now uses
`cargo +1.95.0 build --locked --package i2pr-interop`; this is a preparation
contract correction, not protocol evidence.

Level 2 repeated validation, negative controls, Java, Emissary, release
certification, support advertisement, and normal NTCP2 enablement remain open
and out of scope.
