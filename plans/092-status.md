# Plan 092 status: forward-handshake evidence integrity and ownership closure

## Status

Plan 092 is **partial / superseded by Plan 093**. The plan delivered
the privacy-safe handshake stage observation contract, the i2pr
runtime observed handshake driver with terminal-counter preservation,
the repaired Plan 083 event ingestion with current-run dedup, the
dedicated Plan 092 regression matrix, and the initial status
authority rewrite.

Plan 093 supersedes Plan 092 as the active plan of record. Plan 093
reopens the ownership analysis after locating the retained i2pd log
message in its true source: `NTCP2: Receive length read error` is
emitted by `NTCP2Session::HandleReceivedLength` in the
**authenticated data phase**, not by the `HandleSessionRequestReceived`
handshake path. Plan 092's "Branch A i2pr runtime state-machine
defect" classification is **superseded** — the apparent
contradiction between the i2pr `authenticated: 1` counter and the
i2pd EOF on the first read resolves once the i2pd log is correctly
read: the EOF came after the handshake completed, in the data phase.

The remaining forward blocker is therefore owned by the **mixed
corrective surface** below and not by the i2pr Noise state machine:

```text
i2pr       = one-frame target-DeliveryStatus receive oracle
i2pd driver = observer reset + target-send predicate lifecycle
```

Plan 088 must not run until Plan 093 closes with a passing
instrumented forward record and a passing control forward record
bound to the same corrective source commit and pinned i2pd
revision.

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

## Diagnostic reclassification by Plan 093 (supersedes prior ownership)

The current Plan 092 ownership analysis is **superseded by Plan 093**
once the i2pd log line is located in its true source. The
retained log says:

```text
NTCP2: Receive length read error: End of file
```

In pinned i2pd 2.60.0, this message is emitted by
`NTCP2Session::ReceiveLength()` and `NTCP2Session::HandleReceivedLength()`
in `libi2pd/NTCP2.cpp`. These functions read the two-byte obfuscated
frame length in the **authenticated NTCP2 data phase**, not the
handshake. The inbound handshake reader is
`HandleSessionRequestReceived` and emits a distinct `SessionRequest
read error` diagnostic.

The same retained run also reports i2pr counters
`authenticated: 1`, `frames_sent: 1`, `frames_received: 1`,
`i2np_sent: 1`, `i2np_received: 0`. Those counters are consistent
with a completed handshake followed by a data-phase sequencing
failure. They are inconsistent with the Plan 092 Branch A theory
that i2pr authenticated without i2pd reading SessionRequest.

Pinned i2pd also sends local RouterInfo automatically on an inbound
session:

```text
NTCP2Session::Established()
  -> transports.PeerConnected(session)

Transports::PeerConnected(incoming session)
  -> session->SendLocalRouterInfo()
```

Therefore, on an inbound i2pr connection, i2pd may send its local
RouterInfo as the first data-phase message before the direct driver
submits the correlated DeliveryStatus reply. The NTCP2 specification
explicitly permits RouterInfo in the data phase and identifies it as
a valid message Bob may use to begin the data phase.

The current i2pr interop oracle reads exactly one authenticated frame
and immediately returns `receiver_delivery_status_missing` when that
first frame does not contain the target DeliveryStatus. This is too
strict for interoperating with the pinned reference. It closes the
socket after receiving i2pd's legitimate initial RouterInfo /
DatabaseStore traffic, causing i2pd's next data-phase length read to
observe EOF.

A second reference-driver defect compounds this:

1. `run_listen()` emits `listener_ready` and only then calls
   `ResetObserverSink()`. A fast TCP-accept/authentication observation
   may therefore be recorded by an earlier listener invocation and
   then erased by the post-ready reset.
2. The observer's sent slot is process-global and cumulative. i2pd's
   automatic local-RouterInfo send may increment the sent counter
   before the driver submits its DeliveryStatus reply.
3. `WaitForSentI2NP()` currently accepts any nonzero sent count, so
   it may return the stale automatic-RouterInfo observation and permit
   shutdown before the DeliveryStatus reply is actually written.

The remaining forward blocker is therefore owned by the **mixed
corrective surface** below, not by the i2pr Noise state machine
unless a new correctly classified run proves otherwise:

```text
owner         = mixed corrective surface
i2pr          = one-frame target-DeliveryStatus receive oracle
i2pd driver   = observer reset and target-send predicate lifecycle
```

## What this plan delivered (preserved)

The Plan 092 implementation surface is preserved verbatim and
continues to be enforced by the static boundary checker:

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
  initial status authority rewrite (further updated by Plan 093).

## What this plan did not do (preserved)

- It did not produce a passing instrumented forward record.
- It did not produce a passing control forward record.
- It did **not** implement the Branch A narrow correction; the
  prior classification is superseded by Plan 093.
- It did not change the i2pd direct driver source.
- It did not modify pinned i2pd NTCP2 transport code.
- It did not advance `specs/support.toml` or advertise NTCP2.
- It did not run Plan 088.
- It did not run a replacement reproduction under Plan 093.

## Handoff to Plan 093

Plan 093 supersedes Plan 092 as the active plan of record. Plan 093
owns:

- WP1: the status authority rewrite that supersedes Plan 092's
  Branch A classification;
- WP2: the pinned source-classification tests that bind
  `ReceiveLength` / `HandleReceivedLength` as data-phase and
  `HandleSessionRequestReceived` as handshake reads;
- WP3: the i2pd observer reset/generation contract;
- WP4: the bounded sequence ring and exact target predicate waits;
- WP5: the i2pr bounded multi-frame target receive oracle;
- WP6: the canonical runner event authority with late event drain;
- WP7: the live binary and source provenance contract that rejects
  zero digests in attempted live records;
- WP8: the Plan 093 focused test matrix
  (`tests/integration/ntcp2/harness/test_plan093.py`);
- WP10 + WP11: the fresh instrumented and control forward attempts;
- WP12: the closure reconciliation that rewrites `plans/087-status.md`,
  `plans/088-status.md`, and creates `plans/093-status.md`.

The retained Plan 092 forward record at
`/tmp/opencode/plan092-test-1/forward-record.json` (record SHA
`696aa1339d3d950f9fec2a2e0b1f5bede2035761a71e167af6ab28b249cc998d`)
is preserved as diagnostic history. The zero `i2pr_binary_sha256`
digest in that record is preserved verbatim as a non-authoritative
diagnostic and is rejected by Plan 093 WP7 in any attempted live
record.

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