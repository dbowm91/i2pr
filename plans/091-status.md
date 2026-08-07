# Plan 091 status: forward NTCP2 Noise-handshake corrective pass

## Status

Plan 091 is **partial / incomplete**. The plan delivered its bounded
diagnostic and behavior corrections plus the i2pd driver preconditions,
but the forward direction did not pass: the i2pr dialer enters
`drive_initiator_handshake`, the i2pd listener reports a TCP
session-accept, then the i2pd NTCP2 transport reads `End of file` on
the first handshake body read while the i2pr reports a data-phase
`receiver_delivery_status_missing` terminal. The plan is **not
closed**. Plan 093 supersedes Plan 092 as the active plan of record
and rewrites the ownership analysis: the i2pd log message
`SessionRequest read error: End of file` from Plan 091 likewise
belongs to a transport read failure, not necessarily a handshake
SessionRequest read — the canonical Plan 093 source-classification
tests bind the exact strings to
`NTCP2Session::HandleReceivedLength` (data phase) and
`HandleSessionRequestReceived` (handshake). This record is preserved
verbatim as the diagnostic history and the exact source commit
remains load-bearing for Plan 093.

## Reproduce command (historical)

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

## Bounded corrections landed (preserved)

1. `i2p::context.SetNetID(cfg.network_id)` between
   `i2p::crypto::InitCrypto(false)` and `i2p::context.Init()`.
   Without it `RouterContext::GetNetID()` returns the default
   `I2PD_NET_ID` (=2) and the NTCP2 listener rejects the
   SessionRequest with `networkID 99 mismatch. Expected 2`.
2. i2pd logger started via `i2p::log::Logger().SendTo(<data_dir>/i2pd.log)`
   plus `Logger().Start()` after `InitCrypto` and stopped in `main()`
   before each `run_*` return.
3. `run_listen` waits boundedly for the i2pd transport to record a
   real TCP accept through the Plan 064 observer
   (`WaitForTcpAccepted`), emits a `tcp_accepted` event, and reaps
   the process with `listening-tcp-accept-timeout` if the wait
   fails.
4. `run_listen` composes a `DeliveryStatus` with the exact correlation
   `message_id` and submits it through the real i2pd transport
   (`transports.SendMessage(peer_ident_hash, reply)`).

The i2pr launcher's bounded `tcp_connected` event emission (Plan 091
WP3) and the runner's matched recognition of `tcp_accepted`,
`noise_message1_sent`, `noise_message2_received`, `noise_authenticated`,
`frame_emitted`, and `frame_decoded` are in place. The runner also
sets `listener_config["scenario_id"] = "minimal-i2pd-probe-listen"` so
the listener-mode `listener_ready` event does not collide with the
inspect-mode `listener_ready` event in the shared `events.ndjson` file.

## First clean committed-head reproduction (preserved)

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

## Forbidden follow-up: raw or hex handshake capture

Plan 091 does **not** authorise a follow-up that hex dumps the first
1 KiB of bytes the i2pr writes to the wire, that hex dumps the first
1 KiB the i2pd reads, or that retains any SessionRequest,
SessionCreated, SessionConfirmed, Noise transcript, frame, payload,
key, nonce, IV, plaintext, ciphertext, or RouterInfo bytes. The
follow-up plan must derive ownership from typed stage outcomes,
bounded octet counts, and source-level control flow only. The
privacy-safe handshake observation contract is owned by Plan 092 WP2.

## Plan 091 success criteria, evaluated

1. "Exact stage authority for TCP connection and the first NTCP2
   handshake flight": landed (`tcp_connected`, `tcp_accepted`,
   `noise_message1_sent`, `noise_message2_received`,
   `noise_authenticated`).
2. "Evidence-only reproduction with unchanged protocol behaviour":
   landed (clean committed-head run retained above).
3. "Determine whether the first failing operation is owned by i2pr,
   the bounded i2pd direct driver, or the pinned i2pd
   implementation": **not closed**. The diagnostic data is in place
   but the ambiguous side is not yet isolated to one owner.
4. "Apply one narrow correction only after ownership is
   demonstrated": **not applicable** because ownership is not yet
   demonstrated. The four i2pd driver changes above are the bounded
   pre-conditions for a future correction; they are not a
   Noise-handshake protocol fix.
5. "Passing instrumented forward result": not achieved.
6. "Semantically equivalent passing control result": not achieved.
7. "Retain exact record digests and reconcile the Plan 087/088 gate
   records": not achieved. The retained record is
   `forward-record.json` with SHA
   `f166611ffb12795d81e0458a9506c7047837ddddf9e41ac1dfcc503d24855d6e`
   (from a recent run; the SHA rotates per run).

The closure is therefore the diagnostic surface, not a pass.

## Plan 090 four-correction set retained

Plan 090's four corrections (`set_bool_option("ntcp2.published",
true)`, `ParseCmdline` + `Finalize` defaults,
`set_uint16_option` for ports, `SetCheckReserved(false)`) are in the
same file and are the load-bearing preconditions for Plan 091. The
Plan 090 source-verification section
`tests/integration/ntcp2/reference-drivers/source-verification.md`
"Plan 090 verified RouterInfo lifecycle" remains authoritative and is
unchanged by this plan.

## Build manifest and source verification (preserved)

The instrumented binary digest, source digest, and patch digest are
recorded in
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
- It did not add a new public record or promote a Plan 087 pass.
- It did not re-open Plan 088, Plan 079, Plan 072, or any release
  apparatus.
- It did not advance `specs/support.toml` or advertise NTCP2.

## Forward direction decision

The forward direction is recorded as `insufficient-evidence` for the
purposes of the Plan 088 decision vocabulary. The ownership probe is
owned by Plan 092. Plan 088 remains blocked until Plan 092 closes
with a passing instrumented forward record, a passing control forward
record, and the corresponding Plan 087 closure.