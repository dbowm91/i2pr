# Plan 083 status: minimal i2pr-to-i2pd NTCP2 wire probe

## Status

Implemented as the in-process record schema, focused test matrix, and
the narrow test-only runner orchestration module on 2026-08-01. The
runner is a development diagnostic that fails closed, never synthesizes
pass or provenance, and does not attempt a live wire run on this host.

This host is the Plan 046 `apparmor_restrict_on` negative baseline.
The Plan 080 Multipass guest cannot complete on this constrained host.
No real TCP connection, NTCP2 handshake, authenticated frame, or
I2NP DeliveryStatus decode has been attempted or retained.

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
- `tests/integration/ntcp2/harness/plan083_runner.py` — the test-only
  runner orchestration module. Owns the strict stage progression, the
  typed process counters, the structured event collection, the lane
  validation, the run-identity freeze, the bounded shutdown and cleanup,
  the host-blocker detection, and the `write_host_blocked_record` helper.
  The runner supports dependency injection through `FakeEventSource` and
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
  fields, the release-authority boundary, the runner contract (lane
  validation, run-id validation, message-id validation, topology
  rejection, router-info rejection, process-counter preservation for
  failed lanes), the stage progression, the fake event source
  injection, the host-blocker detection and typed record, and the
  cleanup verification.
- `scripts/check-ntcp2-interoperability.sh` — extended to enforce the
  Plan 082 runner artifacts, the Plan 083 probe module, and the
  Plan 083 test matrix presence.

## Host blocker

This host reports `blocked_unprivileged_user_namespace` from the
Plan 046 rootless probe. The runner detects this through the
`I2PR_PLAN046_HOST_BLOCKER_ENV` environment variable and returns a
typed `lane_invalid` record without attempting a live wire run. The
`write_host_blocked_record` helper produces a valid probe record with
zero binary/router-info/router-hash digests and an empty observed-events
list, suitable for sanitized evidence.

## Validation

```text
python3 -m unittest ... -p 'test_minimal_i2pd_probe.py'   passed (43)
python3 -m unittest ... -p 'test_plan083.py'                passed (48)
python3 -m unittest ... -p 'test_plan082.py'                passed (3)
python3 -m unittest ... -p 'test_i2pr_prepare.py'           passed (3)
python3 -m unittest ... -p 'test_harness.py'                passed (111)
python3 -m unittest ... -p 'test_plan065.py'                passed (29)
python3 -m unittest discover -s tests/integration/ntcp2/harness  passed (1094)
bash scripts/check-ntcp2-interoperability.sh               passed
```

The full repository gates and boundary checks pass before commit. The
probe record schema is round-trippable and the canonical `record_sha256`
digest is stable across key ordering. The schema rejects every forbidden
field, generic reason code, unknown topology, and zero or out-of-range
DeliveryStatus message ID.

## Runner architecture

The runner implements the 11-step execution architecture:

1. lane/placement validation — checks topology kind, network state,
   and network ID
2. i2pr state preparation — delegates to Plan 082 preparation
3. i2pd state preparation — locates the Plan 076 driver
4. RouterInfo exchange and strict validation — verifies SHA-256 digests
5. run-identity freeze — validates run-id, source-commit, and message ID
6. i2pd listener start — launches the Plan 076 driver process
7. i2pr dial start — launches the i2pr-interop dial process
8. structured event collection — consumes i2pr and i2pd events
9. one exact DeliveryStatus transfer — cross-process correlation
10. bounded shutdown and cleanup — ordered process termination
11. one compact diagnostic record — validated through `build_record`

The runner supports dependency injection through `FakeEventSource` and
`FakeProcess` for unit tests. The `FakeEventSource` accepts events via
`add_event` and returns them through `wait_for_event`. Terminal
rejection is injected via `inject_terminal_rejected`.

## Handoff

The actual probe runner may only be exercised in the Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass lane; this host can
only validate the schema, the runner module, and the focused tests.
Plan 084 may begin once the future runner delivers a real
`i2pr -> i2pd` result and the reverse direction is attempted with the
same schema.
