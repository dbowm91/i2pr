# Plan 083 status: minimal i2pr-to-i2pd NTCP2 wire probe

## Status

Implemented as the in-process record schema and focused test matrix on
2026-08-01. The probe runner itself is **not** implemented in this pass
because the host is the Plan 046 `apparmor_restrict_on` negative baseline
and the Plan 080 Multipass guest cannot complete on this constrained host.
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
- `tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py` — 43
  focused tests covering the schema contract, the field allowlist, the
  bounded stage and reason codes, the process counter skeleton, the
  observed-event validation, the canonical record digest, and the
  pass-record rules.
- `tests/integration/ntcp2/harness/test_plan083.py` — 25 Plan 083 test
  matrix cases covering the locked direction, the locked reference,
  the stage model, the reason-code allowlist, the observed-event set,
  the process-counter layout, the topology allowlist, the provenance
  fields, and the release-authority boundary.
- `scripts/check-ntcp2-interoperability.sh` — extended to enforce the
  Plan 082 runner artifacts, the Plan 083 probe module, and the
  Plan 083 test matrix presence.

## Validation

```text
python3 -m unittest ... -p 'test_minimal_i2pd_probe.py'   passed (43)
python3 -m unittest ... -p 'test_plan083.py'                passed (25)
python3 -m unittest ... -p 'test_plan082.py'                passed (3)
python3 -m unittest ... -p 'test_i2pr_prepare.py'           passed (3)
python3 -m unittest ... -p 'test_harness.py'                passed (111)
python3 -m unittest ... -p 'test_plan065.py'                passed (29)
bash scripts/check-ntcp2-interoperability.sh               passed
```

The touched-code test suite plus the static boundary checks pass before
commit. The probe record schema is round-trippable and the canonical
`record_sha256` digest is stable across key ordering. The schema
rejects every forbidden field, generic reason code, unknown topology,
and zero or out-of-range DeliveryStatus message ID.

## Handoff

The actual probe runner (the subprocess driver that consumes the probe
record schema, allocates the loopback port, launches the i2pr launcher
and the Plan 076 i2pd driver, and consumes the structured event
streams) remains a Plan 083 follow-up. That runner may only be
exercised in the Plan 046 rootless sealed-namespace lane or the
Plan 048/049 Multipass lane; this host can only validate the schema
and the focused tests. Plan 084 may begin once the future runner
delivers a real `i2pr -> i2pd` result and the reverse direction is
attempted with the same schema.
