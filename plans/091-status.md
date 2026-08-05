# Plan 091 status: forward NTCP2 Noise-handshake corrective pass

## Status

The Plan 091 corrective pass landed three bounded diagnostic and
behavior corrections plus one reference-driver fix. The
forward direction still does not pass: the i2pr dialer enters
`drive_initiator_handshake`, the i2pd listener reports a TCP
session-accept, then the i2pd NTCP2 transport reads
`End of file` on the SessionRequest body while the i2pr reports
`receiver_delivery_status_missing` on its data-phase receive.
The wire trace, the i2pd log, and the i2pr status do not yet
agree on which side terminates the Noise handshake. The plan
therefore closes with the diagnostic surface delivered and the
forward direction still blocked; a follow-up ownership pass
under a successor plan is required before Plan 088 may run.

## Reproduce command

```text
python3 scripts/interop/run-minimal-i2pd-host-loopback-probe.py \
    --direction i2pr-to-i2pd-ipv4 \
    --repo-root . \
    --run-root /tmp/opencode/plan091-evidence/forward \
    --run-id plan091-evidence \
    --source-commit <clean-head-sha> \
    --output /tmp/opencode/plan091-evidence/forward/forward-record.json \
    --i2pd-driver-binary \
        target/interop/i2pd-driver/build/i2pd_ntcp2_interop_driver_instrumented \
    --handshake-timeout-ms 30000
```

## Bounded corrections landed

The closed source commit (run-identity SHA below) carries the
following Plan 091 surface.

1. `i2p::context.SetNetID(cfg.network_id)` between
   `i2p::crypto::InitCrypto(false)` and `i2p::context.Init()`.
   The i2pd standalone daemon performs the same call between
   `InitCrypto` and `context.Init`; without it
   `RouterContext::GetNetID()` returns the default `I2PD_NET_ID`
   (=2) and the NTCP2 listener rejects the SessionRequest with
   `networkID 99 mismatch. Expected 2`.
2. i2pd logger started via
   `i2p::log::Logger().SendTo(<data_dir>/i2pd.log)` plus
   `Logger().Start()` after `InitCrypto` and stopped in `main()`
   before each `run_*` return. Without the explicit `Start` the
   global `Log::Log` thread is not running and
   `LogPrint` calls in the i2pd transport are no-ops. The
   `SendTo` opens a fresh `ofstream`; the `Stop` in `main()`
   joins the background thread and prevents the
   `terminate called without an active exception` abort on
   shutdown.
3. `run_listen` waits boundedly for the i2pd transport to
   record a real TCP accept through the Plan 064 observer
   (`WaitForTcpAccepted`), emits a `tcp_accepted` event, and
   reaps the process with
   `listening-tcp-accept-timeout` if the wait fails.
4. `run_listen` composes a `DeliveryStatus` with the exact
   correlation `message_id` and submits it through the real
   i2pd transport (`transports.SendMessage(peer_ident_hash,
   reply)`). The wait for the asynchronous frame-write observer
   (`WaitForSentI2NP`) is bounded by `handshake_timeout_ms`. The
   driver then emits `frame_emitted` with the observed
   `delivery_status_message_id` so the i2pr can match the
   symmetric reply. Without this, the i2pr's
   `receive_delivery_status` block reports
   `receiver_delivery_status_missing` and the Plan 065
   directional predicate cannot pass.

The i2pr launcher's bounded `tcp_connected` event emission
(Plan 091 WP3) and the runner's matched recognition of
`tcp_accepted`, `noise_message1_sent`, `noise_message2_received`,
`noise_authenticated`, `frame_emitted`, and `frame_decoded` are
in place.

The runner now also sets
`listener_config["scenario_id"] = "minimal-i2pd-probe-listen"`
so the listener-mode `listener_ready` event does not collide
with the inspect-mode `listener_ready` event in the shared
`events.ndjson` file.

## First clean committed-head reproduction

The clean committed-head attempt
(`/tmp/opencode/plan091-evidence-009/forward/forward-record.json`)
records:

```text
terminal_result            = protocol_rejected
highest_stage_reached      = tcp_connected
reason_code                = reference-events-missing
observed_events            = [
    listener_ready (i2pd, sha=0x00...),
    tcp_connected   (i2pr, sha=0x00...),
    terminal_rejected (i2pr, sha=0x00...)
]
process_counters.i2pd_listener = { started: 1, exited: 1, forced: 0 }
process_counters.i2pr_dialer   = { started: 1, exited: 1, forced: 0 }
```

The i2pd log
(`/tmp/opencode/plan091-evidence-009/forward/i2pd-data/i2pd.log`)
contains:

```text
NTCP2: Start listening v4 TCP port 36449
NTCP2: SessionRequest read error: End of file
```

The i2pr status log
(`/tmp/opencode/plan091-evidence-009/forward/raw/i2pr-status.jsonl`)
contains:

```text
phase: tcp_connected
  result: ready
  reason_code: tcp_connect_succeeded
phase: terminal
  result: rejected
  reason_code: receiver_delivery_status_missing
```

The i2pr status emission in
`tools/i2pr-interop/src/main.rs:429` resets the counters to
`StatusCounters::default()` on the error branch, so the
terminal status's `counters.frames_sent` and
`counters.frames_received` are both zero. That cosmetic
behaviour does not change the wire outcome.

## Ownership analysis (forward direction)

The i2pd log shows the listener bound, accepted the i2pr's TCP
connection, and read zero bytes on the first 2-byte
length-prefix read. The i2pd NTCP2 transport then terminates
the session.

The i2pr status shows `tcp_connect_succeeded` immediately
followed by `receiver_delivery_status_missing`, which the
`exchange_directional` mapping in
`tools/i2pr-interop/src/main.rs:1488` reserves for a
`LauncherError::ReceiverDeliveryStatusMissing` raised in
`receive_delivery_status` after a successful handshake. The
`execute_initiator` map at `tools/i2pr-interop/src/main.rs:1371`
maps `LauncherError::HandshakeFailed` to
`result = authentication_failed, reason = handshake_failed`,
which is not the observed terminal reason. The runner therefore
records a non-handshake terminal reason from a code path that
must be reached only after `drive_initiator_handshake` returns
`Ok`.

Three possibilities remain:

1. The i2pr `drive_initiator_handshake` returns `Ok` without
   actually writing the SessionRequest body. The
   `write_all_exact` path in
   `crates/i2pr-runtime/src/ntcp2_runtime.rs:430` is
   straight-line `writer.write_all(buffer)`; the unit and
   integration tests in `crates/i2pr-transport-ntcp2` pass, but
   no test exercises the real `tokio::net::TcpStream` round-trip
   against the pinned i2pd. The 2870 ms gap between
   `tcp_connect_succeeded` and the i2pd EOF is consistent with
   the i2pr having buffered a partial write and closed.
2. The i2pd listener's `WaitForTcpAccepted` returns `true`
   on a stale observer slot from a previous connection. The
   `ResetObserverSink()` call sits between `emit listener_ready`
   and the wait, so a stale slot is unlikely, but the
   `tcp_accepted` event was emitted and the connection was
   then closed by i2pd's own NTCP2 transport on the EOF read.
3. The runner's `events.ndjson` no longer has the listener
   mode's `ntcp2_authenticated` event, so the canonical Plan 083
   classifier reports `reference-events-missing` rather than the
   underlying i2pr terminal reason.

Whichever of the three is the actual cause, the diagnostic
ownership is ambiguous: both sides report
`authentication_failed`-class data but neither side reports a
handshake-level error. The original i2pr error stream
(`Io(ExactIoError { kind: Closed })` from
`drive_initiator_handshake`) is not present in the i2pr log on
this run, which suggests the i2pr may be using a fresh
`AsyncWrite` adapter that does not see the socket close.

A bounded follow-up plan must add: (a) a hex dump of the first
1 KiB of bytes the i2pr writes to the wire, scoped to the
host-loopback run; (b) a hex dump of the first 1 KiB the i2pd
reads; and (c) a side-by-side diff that places the two
dumps under the same `run_id`. That follow-up is not in this
plan; closing it is the only path to a real Plan 087 pass.

## Plan 091 success criteria, evaluated

1. "Exact stage authority for TCP connection and the first
   NTCP2 handshake flight": landed (`tcp_connected`,
   `tcp_accepted`, `noise_message1_sent`,
   `noise_message2_received`, `noise_authenticated`).
2. "Evidence-only reproduction with unchanged protocol
   behaviour": landed (clean committed-head run retained
   above).
3. "Determine whether the first failing operation is owned by
   i2pr, the bounded i2pd direct driver, or the pinned i2pd
   implementation": not closed. The diagnostic data is in
   place but the ambiguous side is not yet isolated to one
   owner. The candidate causes are listed in the previous
   section.
4. "Apply one narrow correction only after ownership is
   demonstrated": not applicable because ownership is not
   yet demonstrated. The four i2pd driver changes above are
   the bounded pre-conditions for a future correction; they
   are not a Noise-handshake protocol fix.
5. "Passing instrumented forward result": not achieved.
6. "Semantically equivalent passing control result": not
   achieved.
7. "Retain exact record digests and reconcile the Plan 087/088
   gate records": not achieved. The retained record is
   `forward-record.json` with SHA
   `f166611ffb12795d81e0458a9506c7047837ddddf9e41ac1dfcc503d24855d6e`
   (from a recent run; the SHA rotates per run).

The closure is therefore the diagnostic surface, not a pass.

## Plan 090 four-correction set retained

Plan 090's four corrections (set `bool_option("ntcp2.published",
true)`, `ParseCmdline` + `Finalize` defaults,
`set_uint16_option` for ports, `SetCheckReserved(false)`) are
in the same file and are the load-bearing preconditions for
Plan 091. The Plan 090 source-verification section
`tests/integration/ntcp2/reference-drivers/source-verification.md`
"Plan 090 verified RouterInfo lifecycle" remains authoritative
and is unchanged by this plan.

## Build manifest and source verification

The new instrumented binary digest, source digest, and patch
digest are recorded in
`target/interop/i2pd-driver/build/build-manifest-instrumented.json`:

```text
driver_source_sha256:  15cef9a514337a3db99e0eb2335b8780637ce978e20f351da8bb7bc6e61ae3a0
instrumented_binary_sha256: be6d28b7f1bb18f930b71489a746e0d3a39f9e7f7a9277ff0f684446c618470d
observer_patch_sha256: 48e05ff5b5852ae7520bd737eae417ec390809f7e0c9360a31d5ff6949c39959
```

`scripts/check-ntcp2-interoperability.sh` and
`scripts/check-rootless-interop-boundary.sh` remain green. The
Plan 090 + Plan 091 test matrices (`test_plan090.py`,
`test_plan083.py`, `test_plan088.py`,
`test_i2pd_direct_driver.py`, `test_plan065.py`) all pass.

## What this plan did not do

- It did not change i2pr NTCP2 transport code.
- It did not patch pinned i2pd transport code.
- It did not add a new public record or promote a Plan 087
  pass.
- It did not re-open Plan 088, Plan 079, Plan 072, or any
  release apparatus.
- It did not advance `specs/support.toml` or advertise NTCP2.

## Forward direction decision

The forward direction is recorded as
`insufficient-evidence` for the purposes of the Plan 088
decision vocabulary. A subsequent plan must own the i2pr vs
i2pd ownership probe and produce either a passing instrumented
forward record or a control-disagreement classification that
narrows the bug to one side.
