# Plan 092 status: forward-handshake evidence integrity and ownership closure

## Status

Plan 092 is **partial / incomplete**. The plan delivered the privacy-safe
handshake stage observation contract, the i2pr runtime observed handshake
driver with terminal-counter preservation, the repaired Plan 083 event
ingestion with current-run dedup, the dedicated Plan 092 regression
matrix, and the status authority rewrite that records Plan 091 as
partial and Plan 092 as the single next executable plan. The first
clean committed-head ownership reproduction on this host
authenticated the i2pd listener, the i2pr dialer started, the i2pd
NTCP2 transport accepted the TCP connection and read EOF on the
first SessionRequest prefix read while the i2pr status reported
`tcp_connected` followed by a `receiver_delivery_status_missing`
terminal, with the i2pr status counters showing
`authenticated: 1`, `frames_sent: 1`, `frames_received: 1`,
`i2np_sent: 1`, `i2np_received: 0`. The runner serialized the
direction as `terminal_result = protocol_rejected,
highest_stage_reached = tcp_connected, reason_code =
reference-events-missing` because the i2pd events file emitted only
`process_started`, `router_info_exported`, and `listener_ready` and
never emitted `tcp_accepted`, `ntcp2_authenticated`, or
`frame_*` markers. The ownership probe is incomplete and the
forward direction is not yet a passing wire run. Plan 088 must
not run until Plan 092 closes with a passing instrumented forward
record and a passing control forward record.

## Reproduce command

```text
python3 scripts/interop/run-minimal-i2pd-host-loopback-probe.py \
    --direction i2pr-to-i2pd-ipv4 \
    --repo-root "$(pwd)" \
    --run-root /tmp/opencode/plan092-evidence-1 \
    --run-id plan092-evidence-1 \
    --source-commit <clean-head-sha> \
    --output /tmp/opencode/plan092-evidence-1/forward-record.json \
    --i2pd-driver-binary \
        target/interop/i2pd-driver/build/i2pd_ntcp2_interop_driver_instrumented \
    --handshake-timeout-ms 30000
```

## First clean committed-head reproduction (preserved)

The forward attempt
(`/tmp/opencode/plan092-test-1/forward-record.json`) records:

```text
terminal_result            = protocol_rejected
highest_stage_reached      = tcp_connected
reason_code                = reference-events-missing
record_sha256              = 696aa1339d3d950f9fec2a2e0b1f5bede2035761a71e167af6ab28b249cc998d
delivery_status_message_id = 69206017
i2pr_binary_sha256         = 0 (the i2pr binary digest is not bound
                              into the record for this reproduction)
i2pd_binary_sha256         = be6d28b7f1bb18f930b71489a746e0d3a39f9e7f7a9277ff0f684446c618470d
i2pr_router_info_sha256    = 99f399bd6d45096f5dd4818dab4660db8272d644c006bb67e8ed1e9294dd49c4
i2pd_router_info_sha256    = 38749ea9b72ac8b47c9db82e3cfe1dd668f78094e236d35a1f9a74490d0ca5b0
i2pr_router_hash_sha256    = 9f804227b62c5a1a9cb4fefc318116a5bd11ddfe20530ab8c29f5e92db16041d
i2pd_router_hash_sha256    = 2e38a807a9bd799cc48a7b3eb20a532e82472e461a6afec1960465c9556811b0
parent_network_state_unchanged = true
placement_record_sha256    = a3fdd6e9bd42e6dcf94e36641d20040bb93d2e4a593d152665636887ec5cf30c
process_counters           = i2pd_listener=1/1/0, i2pd_prepare=1/1/0,
                              i2pr_dialer=1/1/0, i2pr_prepare=1/1/0
```

The observed events are:

```text
listener_ready      (source_side=i2pd,  event_sha256=2723f09c1d5f44b719070874e6bccabe1cd96007203fc4d60fa47fcea4fbfb27)
tcp_connected       (source_side=i2pr,  event_sha256=0x00..00)
terminal_rejected   (source_side=i2pr,  event_sha256=0x00..00)
```

The i2pd log
(`/tmp/opencode/plan092-test-1/i2pd-data/i2pd.log`) contains:

```text
NTCP2: Start listening v4 TCP port <fresh-port>
NetDb: RouterInfo added: <i2pr-router-hash>
NTCP2: Receive length read error: End of file
```

The i2pr status log
(`/tmp/opencode/plan092-test-1/raw/i2pr.log`) contains:

```text
phase: tcp_connected
  result: ready
  reason_code: tcp_connect_succeeded
phase: terminal
  result: rejected
  reason_code: receiver_delivery_status_missing
  counters: {
    listener_ready: 0,
    tcp_connected: 1,
    authenticated: 1,
    frames_sent: 1,
    frames_received: 1,
    i2np_sent: 1,
    i2np_received: 0,
    delivery_status_message_id: 69206017,
    expected_peer_router_hash_sha256: 2e38a807a9bd799cc48a7b3eb20a532e82472e461a6afec1960465c9556811b0,
  }
```

## Ownership analysis (forward direction, current-run)

The wire trace, the i2pd log, and the i2pr status do not yet agree
on which side terminates the Noise handshake:

1. The i2pd NTCP2 transport logged
   `NTCP2: Receive length read error: End of file` on the first 2-byte
   SessionRequest prefix read. The TCP connection was closed before
   i2pd read any handshake bytes.
2. The i2pr runtime state machine transitioned to
   `Authenticated` and emitted a frame; the i2pr status counters
   carry `authenticated: 1`, `frames_sent: 1`, `frames_received: 1`,
   `i2np_sent: 1`, and `i2np_received: 0`. The i2pr reports a
   data-phase `receiver_delivery_status_missing` terminal.
3. The i2pd events file does not emit `tcp_accepted`,
   `ntcp2_authenticated`, `frame_emitted`,
   `frame_authenticated_and_decrypted`, or `i2np_message_decoded`
   markers; only `process_started`, `router_info_exported`, and
   `listener_ready` were emitted by the listener-mode run.

These three observations cannot all be true simultaneously:

- If i2pd received zero bytes on the SessionRequest prefix read,
  the TCP connection was closed before i2pd consumed any handshake
  bytes. i2pr could not have written SessionRequest, SessionCreated
  could not have been returned, and the i2pr state machine cannot
  legitimately reach `Authenticated`.
- If i2pr reached `Authenticated`, i2pr must have read a valid
  SessionCreated and written a valid SessionConfirmed. i2pd must
  therefore have processed the SessionRequest prefix successfully.

The contradiction points to one of two branches:

- **Branch A (i2pr runtime / state-machine defect)**: the i2pr
  `drive_initiator_handshake` driver transitions to `Authenticated`
  on its own state-machine path without verifying that i2pd's
  SessionCreated read actually returned a parseable response. The
  SessionCreated read path in
  `crates/i2pr-runtime/src/ntcp2_driver.rs` would need to enforce
  that the read returned exactly the bounded `length` bytes and that
  the bytes satisfy the protocol-state-machine `Bytes` transition.
  This is the most likely ownership because the i2pr status carries
  `authenticated: 1` even though i2pd never read the SessionRequest.
- **Branch D (evidence / observer defect)**: the i2pd observer patch
  may not be firing on the actual TCP accept in listener mode, so
  the runner sees no `tcp_accepted` or `ntcp2_authenticated` events
  even when the i2pd transport actually processed the handshake.
  This is unlikely because the i2pd transport log explicitly
  records EOF on the first read, not a successful read.

This plan selects **Branch A — i2pr runtime / state-machine defect
as the dominant ownership**, with Branch D reserved as the
secondary owner. The selection is recorded before any protocol
correction is implemented; per Plan 092 WP10, the narrow correction
must remain in the smallest correct `i2pr-runtime` surface, must
preserve every strict RouterInfo / Router Hash / endpoint /
network-ID / static-key / replay / length / padding / transcript
check, and must add a deterministic regression that fails before
the correction and passes afterward.

## Narrow correction surface (planned)

The dominant correction lives in
`crates/i2pr-runtime/src/ntcp2_driver.rs` at the
`drive_inner_observed` function:

```text
HandshakeAction::ReadExact { length } => {
    // Plan 092: the bounded read must succeed exactly and the
    // returned bytes must satisfy the state-machine Bytes
    // transition. If the read returns any other io_result the
    // driver must NOT advance the state machine to the next
    // transition. The current driver maps io_result back to a
    // typed Io error which terminates the handshake; that path
    // is preserved but the early-return must also preserve the
    // accumulated counter snapshot so the runner can correlate
    // the last observed state with the typed failure.
}
```

The companion runtime counter preservation is in
`HandshakeCounterSnapshot::as_status_counters` and
`HandshakeRunOutcome { result, counters }`. The driver returns the
outcome struct from `drive_initiator_handshake_observed` and
`drive_responder_handshake_observed` so the runner can correlate
the last observed state with the typed failure.

## Mandatory invariants

```text
reference revision                  = pinned i2pd 2.60.0 revision
topology                            = host-loopback-development
network ID                          = 99
endpoint                            = literal 127.0.0.1:<fresh-port>
development_only                    = true
release_qualified                   = false
isolation_qualified                 = false
peer RouterInfo                     = real signed i2pd output
peer endpoint                       = exact address/port match
source attribution                  = exact clean committed SHA
binary attribution                  = measured i2pr/instrumented/control digests
process ownership                   = HostLoopbackDevelopmentPlacement
observer behavior                   = passive, bounded, metadata-only
instrumented/control protocol path  = behaviorally equivalent
cleanup                             = bounded and clean
Plan 088                            = blocked until all Plan 092 closure gates pass
NTCP2                               = experimental and non-advertised
```

## Forward direction decision

The forward direction is recorded as
`insufficient-evidence-for-pass` for the purposes of the Plan 088
decision vocabulary. Plan 088 remains blocked until Plan 092
closes with a passing instrumented forward record and a passing
control forward record bound to the same corrective commit.

## What this plan delivered

- `crates/i2pr-runtime/src/ntcp2_handshake_observer.rs` —
  privacy-safe observer trait, no-op default, and the bounded
  `HandshakeIoResult` enum.
- `crates/i2pr-runtime/src/ntcp2_driver.rs` — observed handshake
  driver entry points, terminal-counter preservation via
  `HandshakeCounterSnapshot` and `HandshakeRunOutcome`, and the
  `i2pr-ntcp2-handshake-stage-v1` schema marker constant.
- `tests/integration/ntcp2/harness/handshake_stage.py` — the
  privacy-safe observation schema with closed allowlists, forbidden
  fields, and canonical event SHA-256 digests.
- `tests/integration/ntcp2/harness/test_plan092.py` — the 28-case
  regression matrix covering the status authority, schema,
  runtime, observer coverage, event ingestion, static
  enforcement, and probe record contract.
- `tests/integration/ntcp2/harness/plan083_runner.py` — current-run
  event dedup, `tcp_accepted` inclusion in the final drain, and the
  bounded terminal-counter preservation.
- `tools/i2pr-interop/src/main.rs` — preserved terminal counters
  across the error path so the runner can correlate the last
  authenticated/frame/I2NP state with the typed failure.
- `scripts/check-ntcp2-interoperability.sh` — extended to require
  the privacy-safe observation schema, the active-sequence token
  in `AGENTS.md`, the `partial / incomplete` declaration in the
  Plan 091 status, and the absence of raw or hex handshake capture
  outside the forbidden-follow-up section of the Plan 091 status.
- `plans/091-status.md`, `plans/087-status.md`, `plans/088-status.md` —
  status authority rewrite.

## What this plan did not do

- It did not produce a passing instrumented forward record.
- It did not produce a passing control forward record.
- It did not implement the Branch A narrow correction; the
  ownership analysis and the planned correction surface are
  recorded above for the next execution pass.
- It did not change the i2pd direct driver source.
- It did not modify pinned i2pd NTCP2 transport code.
- It did not advance `specs/support.toml` or advertise NTCP2.
- It did not run Plan 088.
- It did not run a replacement reproduction; the Branch A
  ownership selection was made on the first clean
  committed-head reproduction.