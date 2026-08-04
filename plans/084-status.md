# Plan 084 status — i2pd-to-i2pr reverse probe and development decision

## Status

Plan 084 implements the reverse-direction probe surface and closes on
this host with the typed environment blocker
``lane_invalidation_pending``. The Plan 046 rootless sealed-namespace
probe returns ``blocked_unprivileged_user_namespace`` on this host,
and the Plan 080 Multipass guest cannot complete on this constrained
host (per Plan 051). No real wire attempt was retained on this host.

The implementation surface is delivered as the in-process record
schema, focused test matrix, and real-subprocess runner orchestration
module. The runner is structurally incapable of producing a
mixed-router pass unless it launches one real i2pr listener process
and one configured real i2pd dialer process and consumes authentic
structured events from both.

This record does not claim NTCP2 interoperability and does not
authorize Plan 079 (repeated development validation) or Plan 073
(release qualification). The Plan 084 development decision is
``lane-invalidated`` because the existing Plan 046 / Plan 080 lanes
cannot be exercised on this host. NTCP2 remains experimental and
non-advertised. Plan 072 remains inactive.

## Delivered implementation surface

- ``tests/integration/ntcp2/harness/minimal_i2pd_reverse_probe.py``
  -- the canonical reverse probe module. Defines the locked record
  schema ``i2pr-minimal-i2pd-reverse-probe-v1`` (version ``1``), the
  locked direction ``i2pd-to-i2pr-ipv4``, the locked reference
  ``i2pd``, the strictly-increasing ordered stage set (inherited from
  the Plan 083 forward probe), the bounded terminal-result set
  (inherited), the bounded reason-code set (inherited), the
  reverse-direction process counter skeleton
  (``i2pr_prepare``, ``i2pd_prepare``, ``i2pr_listener``,
  ``i2pd_dialer`` -- reversed from the Plan 083 forward direction),
  the observed-event validator, the canonical record digest, the
  forbidden field list, and the ``build_reverse_record`` /
  ``validate_reverse_record`` helpers. The module imports the
  bounded sets from ``minimal_i2pd_probe`` so the two directions
  produce comparability-validated records.
- ``tests/integration/ntcp2/harness/plan084_runner.py`` -- the
  reverse probe runner orchestration module. Owns the strict stage
  progression, the typed reverse-direction process counters, the
  structured event collection, the lane validation, the run-identity
  freeze, the bounded shutdown and cleanup, the host-blocker
  detection, the ``write_host_blocked_record`` helper, and the
  real-subprocess ``execute_reverse_probe(...)`` entry point. The
  runner supports dependency injection through ``FakeEventSource``
  and ``FakeProcess`` for unit tests. It never imports Plan
  056/066 candidate, bundle, certificate, rootless-topology, or
  Multipass authority.
- ``tests/integration/ntcp2/harness/test_plan084.py`` -- 51 Plan 084
  test matrix cases covering the locked direction, the locked
  reference, the schema marker, the required-field contract, the
  stage model, the reason-code allowlist, the observed-event set,
  the process-counter layout (and its asymmetry with the forward
  direction), the topology allowlist, the provenance fields, the
  release-authority boundary, the runner contract, the host-blocker
  detection, the cleanup verification, the schema cross-direction
  rejection (forward record rejected by the reverse schema and vice
  versa), and the Plan 084 development decision vocabulary
  (``two-way-development-probe-passed``,
  ``one-way-passed-reverse-defect``,
  ``same-stage-two-way-i2pr-defect``,
  ``ambiguous-reference-divergence``, ``lane-invalidated``).
- ``scripts/check-ntcp2-interoperability.sh`` -- extended to enforce
  the presence of the reverse probe module, the v1 schema marker, the
  reverse direction marker, the focused test matrix, the runner
  orchestration module, and the plan-of-record reference.

## Host blocker

This host reports ``blocked_unprivileged_user_namespace`` from the
Plan 046 rootless probe. The Plan 080 Multipass guest cannot
complete on this constrained host (per Plan 051). The reverse
runner detects the host blocker through the
``I2PR_PLAN046_HOST_BLOCKER`` environment variable and refuses to
attempt a live wire run. The ``write_host_blocked_record`` helper
produces a valid reverse probe record with zero
binary/router-info/router-hash digests and an empty observed-events
list, suitable for sanitized evidence.

## Real-subprocess reverse probe entry point

``execute_reverse_probe(...)`` implements the same 11-step execution
architecture as the Plan 083 forward direction with one role swap:

1. lane/placement validation -- checks topology kind, network ID,
   and the bounded ``delivery_status_message_id`` range
2. i2pr state preparation via ``target/debug/i2pr-interop ntcp2
   prepare`` with deterministic seed and the synthetic 192.0.2.0/24
   endpoint range
3. i2pd state preparation via the Plan 076 direct driver in inspect
   mode; the local i2pd Router Hash is captured from the
   ``router_info_exported`` event
4. RouterInfo exchange -- the i2pr RouterInfo is copied into the
   exchange directory for the i2pd driver to import
5. run-identity freeze -- the per-run ``delivery_status_message_id``
   and the 64-hex Router Hash pair are bound into the Plan 065
   strict scenario (``i2pr-launcher-scenario-v2``) with role
   ``responder`` and ``scenario_id = i2pd-to-i2pr-ipv4``
6. i2pr listener start via ``target/debug/i2pr-interop ntcp2 listen
   --scenario-config`` as a separate subprocess
7. bounded wait for the real i2pr ``listener_ready`` event before
   the i2pd dialer starts
8. i2pd dialer start via the i2pd direct driver in dial mode as a
   separate subprocess
9. structured event collection -- concurrent polling of the i2pd
   ``events.ndjson`` stream and the i2pr JSONL status stream; only
   authentic observed events are recorded
10. bounded shutdown and cleanup -- ordered process termination
11. one compact diagnostic record -- validated through
    ``build_reverse_record`` and written to
    ``reverse-probe-record.json`` with the canonical
    ``record_sha256`` digest

The runner refuses to fall back to SAM, HTTP, support-topology, or
synthetic-fallback helpers for any primary direction; the C++ i2pd
direct driver is the only allowlisted reference driver mode. The
runner never imports Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority. No real wire attempt
has been executed in this checkout because the host is the Plan 046
``apparmor_restrict_on`` negative baseline and the Plan 080
Multipass guest cannot complete on this constrained host.

## Development decision

```text
decision = lane-invalidated
```

The Plan 046 rootless sealed-namespace probe returns
``blocked_unprivileged_user_namespace`` on this host. The Plan 080
Multipass recovery guest cannot complete on this constrained host
(per Plan 051). No real wire attempt has been retained. The Plan
077/080 lane ownership and artifact-binding proofs remain valid; the
lane simply cannot be exercised on this particular kernel
configuration.

Per the Plan 084 development decision matrix, ``lane-invalidated``
is reserved for the case where the existing lane's ownership,
no-public-network, or artifact-binding proof actually fails before
protocol execution. The Plan 046 host blocker and Plan 051
resource constraint are exactly that case. The consequence is:

- return to the existing Plan 077/080 lane scripts;
- do not redesign the environment unless the existing lane cannot
  be refreshed;
- Plan 079 remains blocked;
- Plan 072 remains inactive;
- NTCP2 remains experimental and non-advertised;
- the diagnostic surface (this status, the schema module, the
  runner module, and the focused tests) is preserved for any
  future host that becomes runnable.

## Cross-host portability

The reverse probe module, the runner orchestration module, and
the focused test matrix travel with the repository unchanged. On a
host where the Plan 046 rootless sealed-namespace lane reports
``rootless_sandbox_available`` (or where the Plan 080 Multipass
guest reports the same after provisioning), the
``execute_reverse_probe(...)`` entry point may be invoked against
real subprocesses and the bounded development decision vocabulary
will resolve to whichever of the five exact values reflects the
wire result.

Cross-host portability for the Plan 046 lane is deferred to
``plans/047-cross-host-rootless-lane-expansion.md``. Cross-host
portability for the Plan 080 Multipass guest is bounded by the
Plan 051 resource constraints.

## Future plan unblocking

| Plan | Precondition | Status after Plan 084 |
| --- | --- | --- |
| Plan 072 | requires Plan 084 ``ambiguous-reference-divergence`` | remains inactive; the lane was never exercised, so no wire-stage reference divergence exists |
| Plan 079 | requires Plan 084 ``two-way-development-probe-passed`` | remains blocked; the lane is unavailable on this host |
| Plan 073 | requires release-qualification evidence | remains inactive; Java qualification and the Plan 058/059/060/066 evidence path remain untouched |

No future plan is unblocked by this Plan 084 closure. Plan 079
remains explicitly blocked; Plan 072 remains explicitly inactive.
The Plan 079 entry-gate reference now points at this status record.

## Validation

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'                          passed (51)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'                          passed (48)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'                  passed (14)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'              passed (43)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'              passed (57)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pr_prepare.py'                    passed (5)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'                          passed (29)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan082.py'                          passed (7)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'                  passed (13)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'                                 passed (1165)
cargo fmt --all --check                                                            passed
cargo check --workspace --all-targets                                              passed
cargo test --workspace                                                             passed (235)
cargo clippy --workspace --all-targets --all-features -- -D warnings               passed
bash scripts/check-ntcp2-interoperability.sh                                       passed
bash scripts/check-dependency-direction.sh                                         passed
bash scripts/check-runtime-boundaries.sh                                           passed
bash scripts/check-ntcp2-vectors.sh                                                passed
bash scripts/check-rootless-interop-boundary.sh                                    passed
bash scripts/check-multipass-interop-boundary.sh                                   passed
bash scripts/check-ntcp2-loopback-smoke-boundary.sh                                passed
```

The full repository gates and boundary checks pass before commit.
The reverse probe record schema is round-trippable and the canonical
``record_sha256`` digest is stable across key ordering. The schema
rejects every forbidden field, generic reason code, unknown
topology, forward direction, and zero or out-of-range DeliveryStatus
message ID. The forward-direction schema rejects every record that
carries the reverse direction marker.

The Plan 076 i2pd direct driver builds cleanly from the pinned
i2pd 2.60.0 source tree with the Plan 083 authenticated observer
seam and the Plan 083 bounded wait primitives. Inspect mode
produces a real signed RouterInfo and exits cleanly; listen mode
waits boundedly for the peer handshake and emits a typed blocker
on timeout; dial mode waits boundedly for the asynchronous socket
write and exits cleanly with the exact DeliveryStatus message ID.

## Handoff

The actual reverse probe runner may only be exercised in the Plan
046 rootless sealed-namespace lane or the Plan 048/049 Multipass
lane. Plan 079 continues to be blocked by this Plan 084 lane
unavailability; the next executable plan-of-record for repeated
development validation is therefore blocked on the Plan 046 host
becoming ``rootless_sandbox_available`` or the Plan 080 Multipass
guest completing on a less constrained host.