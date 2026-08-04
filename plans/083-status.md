# Plan 083 status: minimal i2pr-to-i2pd NTCP2 wire probe

## Status

Implemented as the in-process record schema, the focused test matrix,
the real-subprocess runner orchestration, and the i2pd C++ driver
observer extensions on 2026-08-04. The runner is structurally incapable
of producing a mixed-router pass unless it launches one real i2pr
process and one configured real reference process and consumes
authentic structured events from both.

This host is the Plan 046 `apparmor_restrict_on` negative baseline. The
Plan 080 Multipass guest cannot complete on this constrained host. No
real TCP connection, NTCP2 handshake, authenticated frame, or I2NP
DeliveryStatus decode has been attempted or retained.

This record does not claim NTCP2 interoperability. It does not authorize
Plan 079 (repeated development validation) or Plan 073 (release
qualification). Plan 084 remains blocked until the reverse direction
closes with a development decision.

## Delivered implementation surface

- `tests/integration/ntcp2/harness/minimal_i2pd_probe.py` — the canonical
  probe module. Defines the locked record schema
  `i2pr-minimal-i2pd-probe-v1`, the strictly-increasing stage model
  (`not_started`, `state_prepared`, `peer_router_info_imported`,
  `listener_ready`, `tcp_connected`, `noise_authenticated`,
  `session_confirmed_accepted`, `authenticated_frame_written`,
  `authenticated_frame_decrypted`, `i2np_delivery_status_decoded`), the
  bounded terminal-result set, the bounded reason-code set, the per-process
  counter skeleton, the observed-event validator, the canonical record
  digest, and the `build_record` / `validate_record` helpers.
- `tests/integration/ntcp2/harness/plan083_runner.py` — the runner
  orchestration module. Owns the strict stage progression, the typed
  process counters, the structured event collection, the lane
  validation, the run-identity freeze, the bounded shutdown and cleanup,
  the host-blocker detection, the `write_host_blocked_record` helper,
  and the real-subprocess `execute_real_probe(...)` entry point. The
  runner supports dependency injection through `FakeEventSource` and
  `FakeProcess` for unit tests. It never imports Plan 056/066 candidate,
  bundle, certificate, rootless-topology, or Multipass authority.
- `tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py` — 43
  focused tests covering the schema contract, the field allowlist, the
  bounded stage and reason codes, the process counter skeleton, the
  observed-event validation, the canonical record digest, and the
  pass-record rules.
- `tests/integration/ntcp2/harness/test_plan083.py` — 48 Plan 083 test
  matrix cases covering the locked direction, the locked reference,
  the stage model, the reason-code allowlist, the observed-event set,
  the process-counter layout, the topology allowlist, the provenance
  fields, the release-authority boundary, the runner contract, and
  the cleanup verification.
- `tests/integration/ntcp2/harness/test_plan083_runner.py` — 14
  runner-specific tests covering pre-protocol and protocol rejection
  paths, the fake event-source and fake-process injection contract,
  the lane-validation rules, the host-blocker detection, and the
  `write_host_blocked_record` helper.
- `tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h`
  and `interop_observer.cpp` — added `ObserveAuthenticated` plus the
  bounded wait primitives `WaitForAuthenticated`,
  `WaitForReceivedI2NP`, and `WaitForSentI2NP`. The new wait
  primitives never block the transport thread and never fabricate
  metadata; they return `false` on timeout and copy the last
  observer-recorded metadata to the caller.
- `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`
  — added a Plan 083 authenticated observer seam inside
  `NTCP2Session::Established()` that emits `ObserveAuthenticated`
  after the Noise handshake completes.
- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — `run_listen` now waits boundedly for `WaitForAuthenticated`
  followed by `WaitForReceivedI2NP`, emits the
  `frame_authenticated_and_decrypted` and `i2np_message_decoded`
  events with the exact observed message ID, and exits cleanly with
  a typed blocker on handshake or data-phase timeout. `run_dial`
  waits boundedly for `WaitForSentI2NP` after the asynchronous
  socket write and emits the `frame_emitted` event with the exact
  DeliveryStatus message ID. The inspect mode no longer fabricates
  observer events.
- `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py` —
  added `test_i2pd_driver_source_uses_real_i2pd_api` extensions that
  assert the new wait primitives, `test_i2pd_observer_patch_marker`
  that asserts the Plan 083 authenticated observer seam, and
  `test_i2pd_observer_header_exposes_wait_primitives` that asserts
  the new observer header API.

## Host blocker

This host reports `blocked_unprivileged_user_namespace` from the
Plan 046 rootless probe. The runner detects this through the
`I2PR_PLAN046_HOST_BLOCKER` environment variable and returns a typed
`lane_invalid` record without attempting a live wire run. The
`write_host_blocked_record` helper produces a valid probe record with
zero binary/router-info/router-hash digests and an empty
observed-events list, suitable for sanitized evidence.

## Real-subprocess probe entry point

`execute_real_probe(...)` implements the 11-step execution architecture:

1. lane/placement validation — checks topology kind, network ID, and
   the bounded `delivery_status_message_id` range
2. i2pr state preparation via `target/debug/i2pr-interop ntcp2 prepare`
   with deterministic seed and the synthetic 192.0.2.0/24 endpoint
3. i2pd state preparation via the Plan 076 direct driver in inspect
   mode; the local i2pd Router Hash is captured from the
   `router_info_exported` event
4. RouterInfo exchange — the i2pr RouterInfo is copied into the
   exchange directory for the i2pd driver to import
5. run-identity freeze — the per-run `delivery_status_message_id`
   and the 64-hex Router Hash pair are bound into the Plan 065
   strict scenario (`i2pr-launcher-scenario-v2`)
6. i2pd listener start via the i2pd direct driver in listen mode as
   a separate subprocess
7. i2pr dialer start via `target/debug/i2pr-interop ntcp2 dial
   --scenario-config` as a separate subprocess
8. structured event collection — concurrent polling of the i2pd
   `events.ndjson` stream and the i2pr JSONL status stream; only
   authentic observed events are recorded
9. one exact DeliveryStatus transfer — the `delivery_status_message_id`
   is verified against the i2pd observer-recorded metadata
10. bounded shutdown and cleanup — ordered process termination
11. one compact diagnostic record — validated through `build_record`
    and written to `probe-record.json` with the canonical
    `record_sha256` digest

The runner refuses to fall back to SAM, HTTP, support-topology, or
synthetic-fallback helpers for any primary direction; the C++ i2pd
direct driver is the only allowlisted reference driver mode.

## Validation

```text
python3 -m unittest ... -p 'test_minimal_i2pd_probe.py'   passed (43)
python3 -m unittest ... -p 'test_plan083.py'                passed (48)
python3 -m unittest ... -p 'test_plan083_runner.py'        passed (14)
python3 -m unittest ... -p 'test_i2pd_direct_driver.py'    passed (57)
python3 -m unittest ... -p 'test_i2pd_direct_control.py'   passed
cargo fmt --all --check                                    passed
cargo check --workspace --all-targets                      passed
cargo test --workspace                                     passed (235)
cargo clippy --workspace --all-targets --all-features      passed
bash scripts/check-ntcp2-interoperability.sh               passed
bash scripts/check-dependency-direction.sh                 passed
bash scripts/check-runtime-boundaries.sh                   passed
bash scripts/check-rootless-interop-boundary.sh            passed
bash scripts/check-multipass-interop-boundary.sh           passed
```

The full repository gates and boundary checks pass before commit. The
probe record schema is round-trippable and the canonical
`record_sha256` digest is stable across key ordering. The schema
rejects every forbidden field, generic reason code, unknown topology,
and zero or out-of-range DeliveryStatus message ID.

The i2pd direct driver builds cleanly from the pinned i2pd 2.60.0
source tree with the updated observer patch and the new wait
primitives. Inspect mode produces a real signed RouterInfo and exits
cleanly; listen mode waits boundedly for the peer handshake and emits
a typed blocker on timeout; dial mode waits boundedly for the
asynchronous socket write and exits cleanly with the exact
DeliveryStatus message ID.

## Handoff

The actual probe runner may only be exercised in the Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass lane; this host can
only validate the schema, the runner module, the focused tests, and
the i2pd driver extensions. Plan 084 may begin once the future
runner delivers a real `i2pr -> i2pd` result and the reverse direction
is attempted with the same schema.