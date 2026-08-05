# ADR 0025: Plan 090 i2pd direct-driver RouterInfo and pre-TCP classification correction

- Status: Accepted for Plan 090.
- Date: 2026-08-05.
- Scope: test-only i2pd direct driver (`tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`) and Plan 083 runner (`tests/integration/ntcp2/harness/plan083_runner.py`).
- Supersedes: none.

## Context

The Plan 087 first-instrumented attempt reached the i2pd
`listener_ready` event and then the i2pr dialer rejected the
i2pd `router.info` with `peer_router_info_invalid` before any
TCP connection. The i2pd direct driver's exported `router.info`
decoded with zero `RouterAddress` entries, so the i2pr parser
walked `info.addresses()` and found no entry whose endpoint
matched the rendered `127.0.0.1:<port>`. The Plan 087
investigation correctly identified the defect in the driver's
`initialise_i2pd_runtime` and the Plan 090 corrective pass
applies four behavior-neutral corrections.

The Plan 090 first clean committed-head attempt then
authenticated the i2pd listener and reached TCP, but the NTCP2
Noise handshake closed the socket before the i2pr initiator
reached `ntcp2_authenticated`. The retained Plan 090 record
was a `pre_protocol_rejected` because no `tcp_connected` event
was observed. The Plan 083 runner's pre-TCP classification
required explicit reasoning; the historical implementation
classified a no-TCP record as `protocol_rejected` with
`reason_code = reference-events-missing`, which overstates the
TCP-level protocol participation.

## Decision

### Plan 090 driver corrections

The i2pd direct driver applies four narrow corrections that
do not edit serialized RouterInfo bytes, do not construct a
RouterAddress in Python, do not sign a harness-created
RouterInfo, do not modify pinned i2pd transport behavior, and
do not add a driver-only fake endpoint:

1. **Publish the NTCP2 address.**
   `set_bool_option("ntcp2.published", true)` (was
   `set_int_option("ntcp2.published", 0)`). The option is
   registered as `value<bool>()->default_value(true)` in
   `libi2pd/Config.cpp` line 330; storing as `int` and
   extracting as `bool` causes the
   `boost::program_options::any_cast` mismatch that the Plan
   064 driver silently swallowed. With `published = true`,
   `NewRouterInfo()` takes the published branch at
   `RouterContext.cpp` lines 143–151 and the address is
   serialized with `host`, `port`, and `i`.

2. **Populate `m_Options` before mutating it.**
   `i2p::config::ParseCmdline(1, fake_argv, ignoreUnknown=true)`
   followed by `Finalize()` materializes the declared defaults
   into `m_Options`. Without this, every `SetOption` call
   silently no-ops because the option name is absent from the
   map.

3. **Use the typed `uint16_t` overload for `port` and
   `ntcp2.port`.** A new `set_uint16_option` helper stores
   the value as `uint16_t` (was `int`). Both options are
   registered as `value<uint16_t>()` in `Config.cpp` lines 63
   and 331.

4. **Disable reserved-range filtering for loopback peers.**
   `i2p::transport::transports.SetCheckReserved(false)` before
   `i2p::context.Init()`. `Transports::IsInReservedRange`
   defaults to enabled
   (`Transports.cpp` line 156,
   `m_CheckReserved(true)`); the deserializer at
   `RouterInfo.cpp` lines 256–262 strips `host` for any IP
   in the reserved range, including `127.0.0.0/8`.

The driver also fails closed with
`router-info-endpoint-mismatch` if the authoritative
in-memory RouterInfo does not carry the exact configured
NTCP2 endpoint. The verification runs after `context.Init()`
and before `emit_event("router_info_exported")`; the driver
never claims a successful `router_info_exported` for an
unverified RouterInfo.

### Plan 083 pre-TCP classification

The Plan 083 runner now classifies every no-`tcp_connected`
record as `pre_protocol_rejected` with a bounded pre-protocol
reason code from the Plan 083 allowlist. The bounded mapping
covers:

- `peer_router_info_invalid` →
  `REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED`
- `scenario_render_failed` →
  `REASON_PRE_PROTOCOL_RENDER_FAILED`
- `run_identity_invalid` →
  `REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED`
- `preparation_failed` →
  `REASON_PRE_PROTOCOL_PREPARATION_FAILED`
- `i2pd_router_info_invalid` →
  `REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED`
- any unknown pre-TCP rejection →
  `REASON_PRE_PROTOCOL_REFERENCE_FAILED` (the explicit
  Plan 090 D5 fallback rather than the generic
  `reference-events-missing`).

A generic `protocol_rejected` result is forbidden unless at
least one authentic `tcp_connected` event exists.

### Plan 083 placement-owned scenario validation

The host-loopback `i2pr-interop ntcp2 validate-scenario`
subprocess is routed through
`HostLoopbackDevelopmentPlacement.run` rather than a direct
`subprocess.run` call. The placement owns the entire process
lifecycle; the runner never composes a shell, namespace, or
Multipass wrapper.

## Consequences

- The i2pd direct driver produces a signed RouterInfo whose
  NTCP2 endpoint matches the configured listener. The Plan
  090 first clean committed-head attempt retained the
  driver's `router_info_exported` event with a valid endpoint
  hash; the Plan 087 forward direction reached TCP
  authentication before the NTCP2 Noise handshake closed the
  socket.
- The Plan 083 runner is structurally incapable of producing
  a `protocol_rejected` record without an authentic
  `tcp_connected` event. Pre-TCP rejections are explicitly
  bounded and fail closed.
- The i2pd direct driver remains bound to the pinned i2pd
  2.60.0 revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
  No pinned transport code, cryptography, handshake, or
  framing is patched. The Plan 090 corrections are confined
  to driver-side configuration, lifecycle, and ownership.

## Records and enforcement

The implementation is in:

- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — the four driver corrections and the
  `router-info-endpoint-mismatch` fail-closed check.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the "Plan 090 verified RouterInfo lifecycle" section
  documents the pinned-source lifecycle and config/export
  ownership.
- `tests/integration/ntcp2/harness/plan083_runner.py` — the
  pre-TCP classifier, placement-owned scenario validation,
  typed pre-protocol reason allowlist, and the bounded
  pre-protocol reject path.
- `tests/integration/ntcp2/harness/test_plan090.py` — the
  Plan 090 test matrix covering the source verification,
  driver binary, control parity, pre-TCP classification,
  placement validation, and record validation.
- `scripts/check-ntcp2-interoperability.sh` — the static
  boundary check enforces the Plan 090 driver corrections,
  the lifecycle documentation, the Plan 090 test matrix
  presence, and the Plan 083 pre-TCP classification surface.

The retained Plan 090 record is preserved at
`/tmp/opencode/plan090-real-20260805174541-fresh/forward-record.json`;
the i2pr status log is at
`/tmp/opencode/plan090-real-20260805174541-fresh/raw/i2pr-status.jsonl`.
NTCP2 remains experimental and non-advertised. The Plan 090
closure remains open until the forward direction passes; Plan
088 may not run until then.