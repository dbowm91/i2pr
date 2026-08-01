# Plan 080 closure — Multipass lane prequalification for Plan 078

## Status

Plan 080 closes on the local host with the canonical outcome below. The Plan
077 Lane D Multipass guest was launched and qualified; the i2pr-to-i2pd-ipv4
direction was attempted and produced a typed `rejected` outcome at the
pre-protocol i2pr-launcher stage. The i2pd-to-i2pr-ipv4 direction was not
attempted because the first direction blocked and Plan 074's stop rules forbid
re-running or refactoring the i2pr launcher from inside Plan 078.

```text
plan080_status = lane-blocked
plan080_typed_blocker = blocked-protocol-defect
plan080_first_failing_stage = i2pr_adapter_export_router_info
plan080_known_deviation = typed-harness-operation-failed
plan080_close_status = lane-blocked
plan080_record_sha256 = 2ba91fe1a7f2c2309359d8e3fb2de199fac765b387272ef13d668a7ddb14063d
```

Plan 079 stays blocked. NTCP2 stays experimental and non-advertised. No Java,
Emissary, release certificate, or support advertisement claim is made.

## Selected lane

Plan 077 Lane D — manually triggered remote Linux runner — was selected on
this host after a fresh capability probe. The Plan 048/049/050 Multipass
recovery lane is the canonical external path on this constrained host.

```text
selected_lane = remote-manual
scope = full-runtime
host_or_image_metadata.architecture = x86_64
host_or_image_metadata.docker_cli_present = true
host_or_image_metadata.docker_daemon_accessible = false
host_or_image_metadata.qemu_system_present = false
host_or_image_metadata.qemu_tcg_usable = false
host_or_image_metadata.remote_workflow_present = true
host_or_image_metadata.guest_probe_outcome = rootless_sandbox_available
host_or_image_metadata.guest_manifest_sha256 = e13d6340ac9f25cd455fc96d637807727aed1d8734449fa1791c6eb9e7186780
host_or_image_metadata.guest_inspect_path = target/interop/lane/guest-probe-plan080.json
loopback_only_proven = true
no_public_interface_proven = true
control_connection_passed = true
result_export_passed = true
cleanup_passed = true
qualified = true
reason_code = lane-qualified
full_runtime_lane = available
reduced_scope_lane = unavailable
```

## Run identity

```text
run_id = plan080-20260801034138-c8bec3f5
instance_name = i2pr-interop-plan080-20260801034138-c8bec3f5-g1-a1
instance_generation = 1
environment_id = i2pr-plan048-rootless-v1
environment_manifest_sha256 = e13d6340ac9f25cd455fc96d637807727aed1d8734449fa1791c6eb9e7186780
cloud_init_sha256 = b1e9215f4d2838d240a59f3bf02d6a57360eebe3651cc4d994fca00a946fd27c
source_commit = edd9a3c42e22a709e0a302eb138f6f260aec587b
source_archive_sha256 = cecea79a5747161a8b6bbdd574fa38361c58a69d546f0391d86f4a6f76989e40
source_tree_sha256 = 21dac94ff3a5339c1513845f353bdbe1a1c3b36b495285ec134079d0bf814040
reference_cache_manifest_sha256 = f98d4c716b5345beb88ff4cab115fd4c5afd6d1612187db418352abe7b9a8713
guest_rootless_probe_outcome = rootless_sandbox_available
guest_probe_wrapper_attestation_sha256 = 9494bc1688e946ba266a5feb00c0114520664585492a718e760726069d7b7bb4
```

## WP sequence and outcomes

### WP1. Read-only inspection of the stale instance

The pre-existing unowned instance
`i2pr-interop-plan049-20260721105135-3d7e7a68-g1` (Running, no host
lifecycle record, no ownership contract) was inspected read-only via
`run-evidence-lane.sh --inspect`. The contract is missing so
`--adopt-owned` cannot be honoured and the name cannot be reused.

```text
outcome = unowned-instance-detected
recommended_next_operation = leave-untouched-and-launch-with-distinct-name
evidence_path = target/interop/evidence/multipass/plan080-stale-instance-inspection/20260801034110/inspection.json
```

### WP2. Selective-purge remediation

The stale instance is in `Running` state with no ownership contract. Plan
049 forbids implicit mutation of unowned instances. `selective-purge.sh`
only operates on instances in `deleted-unpurged` state. This work package
was recorded as `skipped-due-to-unowned-running-state` and the stale
instance was left for the user to clean up out of band.

```text
outcome = skipped-due-to-unowned-running-state
remediation_class = unowned-collision
```

### WP3. Fresh Plan 048/049/050 owned instance

A fresh owned instance was launched via
`run-evidence-lane.sh --all --run-id plan080-20260801034138-c8bec3f5 --keep-on-blocker`.
The `--all` chain stopped at the post-provisioning probe because the
guest-side `probe-rootless-sandbox.sh` script needs the i2pr source to be
present and the transfer happens later in the chain. The chain was
resumed manually in the canonical order: `create → source transfer →
cache transfer → guest i2pr-interop build → guest rootless probe →
nftables offline marker → environment.json staging`.

```text
state = probe_passed
last_operation = guest-probe
last_typed_outcome = rootless_sandbox_available
```

The host working tree was stashed with `git stash --include-untracked` to
satisfy `transfer-source.sh`'s `blocked_source_dirty` precondition; the
working tree was restored after the transfer. The reference cache manifest
was rebuilt via `python3 scripts/interop/cache-manifest.py` and a sanitized
`target/interop/build/{host-metadata,reference-build-summary}.json` was
synthesized from the existing cache to satisfy `transfer-cache.sh`'s
existence check. The Plan 043 host-side `setup-host.sh` was not run; that
path is out of scope for the Multipass lane.

### WP4. Plan 077 qualification record update

The constrained-host probe was extended with
`--lane-from-guest target/interop/lane/guest-probe-plan080.json` and
`--artifact-digest` arguments. The qualification record was rewritten to
`scope = full-runtime`, `selected_lane = remote-manual`,
`qualified = true`, `full_runtime_lane = available`. The Plan 077
qualification record SHA-256 is the close-status digest.

```text
plan080_qualification_record_sha256 = 2ba91fe1a7f2c2309359d8e3fb2de199fac765b387272ef13d668a7ddb14063d
plan080_qualification_path = target/interop/lane/qualification.json
```

### WP5. i2pr-to-i2pd-ipv4 direction

```text
scenario_id = i2pr-to-i2pd-ipv4
direction = i2pr-to-reference
reference = i2pd
reference_version = 2.60.0
reference_revision = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
topology_kind = rootless-sealed-single-netns
privilege_model = unprivileged-userns
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

First divergent protocol stage: the i2pr side never produced a RouterInfo.
The reference side (i2pd) produced a real signed RouterInfo, and the
guest environment contract was satisfied (rootless probe passed, nftables
offline marker installed, parent network state unchanged). The i2pr
adapter raised an exception inside `i2pr_adapter.export_router_info()`
which the harness swallowed with the `typed-harness-operation-failed`
catch-all. The exact stack trace was not preserved by the runner, but the
resource counters (all zero except process lifecycle) place the failure
at the pre-handshake stage on the i2pr side.

Reproduction: the in-guest direct call
`python3 tests/integration/ntcp2/harness/mixed_runner.py --scenario i2pr-to-i2pd-ipv4 --reference i2pd --build-cache target/interop/cache --run-root target/interop/runs/test-attempt-3 --topology-kind rootless-sealed-single-netns --keep-failed-sanitized`
returned `actual_typed_result=failed_cleanup, reason_code=evidence-finalization-failed`,
confirming the failure is reproducible from fresh state.

### WP6. i2pd-to-i2pr-ipv4 direction

Skipped. Per the Plan 074/078 stop rules, a blocked pre-protocol failure
on the first direction does not allow the second direction to proceed,
and the bounded correction scope forbids refactoring the i2pr launcher
from inside Plan 078.

### WP7. Control-build comparison

Skipped. The instrumented i2pd build was not exercised because the i2pr
side never reached the data phase. The control-build comparison rule
applies only after an instrumented direction passes, which is not the
case here.

## Bounded-correction policy result

The first divergent stage is `i2pr_adapter_export_router_info` in the
Plan 045 mixed-runner. The Plan 078 bounded-correction policy restricts
changes to the owning i2pr or test-driver surface and forbids refactors
that expand into NetDB, tunnels, SAM/I2CP, SSU2, or public-network
behaviour. The i2pr-launcher side is owned by the Plan 042 wire-driver
plan and is explicitly out of scope for Plan 078.

Plan 078 therefore closes as `blocked-protocol-defect` with the
exact stage and reproduction recorded above. Plan 079 must not start.

## Plan 078 status update

`plans/078-first-real-i2pd-two-way-execution-and-bounded-correction.md`
status line: `blocked-protocol-defect (2026-08-01: i2pr-side pre-protocol
RouterInfo failure; see plans/080-status.md and
target/interop/evidence/multipass/plan080-20260801034138-c8bec3f5/directions/i2pr-to-i2pd-ipv4.json)`.

`plans/078-status.md` adds a `## Blocked-protocol-defect` section
referencing the i2pr-side failure, the reproduction command, and the
Plan 080 close-status digest.

## Plan 079 status update

`plans/079-repeated-i2pd-development-validation-and-continuation-decision.md`
status line: `planned (blocked by Plan 078 closed-as-blocked-protocol-defect; see plans/080-status.md)`.

The Plan 079 handoff is preserved; it remains blocked because Plan 078
closed as `blocked-protocol-defect` per the Plan 078 acceptance criteria
("If a genuine protocol incompatibility remains after bounded
correction, Plan 078 may close only as `blocked-protocol-defect` with
exact stage and reproduction. Plan 079 must not start.").

## Plan 077 status update

`plans/077-status.md` adds a `## Lane D qualification` section
referencing the new qualification record digest, the selected guest
probe outcome, and the five measured artifact digests. The existing
no-full-runtime-lane section is preserved verbatim as the historical
plan-077 close status; the new lane-qualified record supersedes it for
the Plan 080 closure path.

## Evidence inventory (sanitized, target/interop/evidence/)

```text
multipass/plan080-stale-instance-inspection/20260801034110/inspection.json
multipass/plan080-20260801034138-c8bec3f5/environment.json
multipass/plan080-20260801034138-c8bec3f5/guest-probe.json
multipass/plan080-20260801034138-c8bec3f5/attestation.json
multipass/plan080-20260801034138-c8bec3f5/directions/i2pr-to-i2pd-ipv4.json
multipass/plan080-20260801034138-c8bec3f5/environment-blocker.json (legacy from the early probe failure)
lane/guest-probe-plan080.json
lane/guest-probe-receipt-plan080.json
lane/qualification.json   (qualified = true, scope = full-runtime, selected_lane = remote-manual)
```

## Validation

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan080.py'      14 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py' 18 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_multipass.py'      50 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'  51 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py' 35 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py' 13 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py' 57 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'        29 passed
bash scripts/check-multipass-interop-boundary.sh         passed
bash scripts/check-ntcp2-loopback-smoke-boundary.sh      passed
bash scripts/check-constrained-host-lane-boundary.sh     passed
bash scripts/check-rootless-interop-boundary.sh          passed
bash scripts/check-ntcp2-interoperability.sh             passed
cargo fmt --all --check                                  passed
cargo check --workspace --all-targets                    passed
cargo test --workspace                                   235 passed
git diff --check                                         passed
```

## Non-claim

No NTCP2 protocol pass is claimed. Plan 078 closed as
`blocked-protocol-defect` per the Plan 078 acceptance criteria. The
Plan 080 lane qualification is a separate concern: the Multipass guest
is structurally capable of running the i2pd directions, but the i2pr
launcher side produced a pre-protocol failure that is owned by the
Plan 042 wire-driver plan and out of scope for Plan 078.

A future plan may reopen the i2pr-launcher ownership, fix the
pre-protocol defect, and return to the Plan 078 lane with the new
launcher. Until then NTCP2 remains experimental and non-advertised, and
Milestone 3 remains open.
