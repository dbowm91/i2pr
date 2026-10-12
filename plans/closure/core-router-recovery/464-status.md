# Plan 464 status: blocked — diagnostic receipt was read from the wrong stream

Closure token: `blocked-plan464-sam-attempt-receipt-not-retained`

Plan: [`464-normal-sam-attempt-cause-and-corrective-run.md`](../../implementation/core-router-recovery/464-normal-sam-attempt-cause-and-corrective-run.md)

## Outcome

The coordinator now emits identity-free per-attempt receipts for selection,
bounded delivery admission/rejection, authenticated reply correlation, and
install commit. The SSU2 session/runtime path carries only the bounded I2NP
message IDs alongside encrypted datagrams and emits a receipt after successful
UDP `send_to`; queue admission is not represented as emission. Staging or
bandwidth rejection and UDP send failure have separate receipts. The Plan 458
runner was extended to correlate numeric build attempt/message IDs into
aggregate counts without retaining identities, peer hashes, destination keys,
payloads, or message IDs.

The single authorized exact-pinned four-router run reached the real SAM
`SESSION CREATE` operation and returned `I2P_ERROR` with
`failure_category=inbound-build`. It did not install the required inbound
tunnel. This proves only the terminal provisioning stage and not whether the
first missing event was UDP staging, emission, stock acceptance, authenticated
reply, exact match, or install.

The new runner parser initially read only `i2pr.stderr`, while the daemon's
formatted tracing layer writes to `i2pr.stdout`. It also omitted a diagnostic
file when it parsed zero attempts. The finite fixture cleanup removed both
temporary streams. Therefore no attempt-scoped receipt survived the run, and
the Plan 464 acceptance evidence is incomplete. The runner now reads both
streams and always writes a sanitized diagnostic summary. A corrected run is
delegated to Plan 465; this record does not authorize another attempt.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Selection/admission attempt receipts | Implemented in coordinator. |
| Correlated actual UDP emission receipt | Implemented through SSU2 session packetization, bounded runtime staging, reordering, and socket send. |
| Authenticated reply match and install receipts | Implemented in coordinator and normal product owner. |
| Deterministic receipt metadata propagation | Focused transport test passed for the message-ID handoff; runtime suite passed after the change. |
| One exact-pinned diagnostic run | Executed once; SAM CREATE failed at `inbound-build`; temporary formatted logs were not captured by the first parser revision. |
| Correct first missing transition and source correction | Not established; no product correction is justified from this run. |
| Plan 462 SAM capability and external byte-exact stream | Not established. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed before focused daemon qualification. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 9 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` | Passed, 4 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1` | Passed, 15 tests. |
| `rtk cargo test --locked -p i2pr-transport-ssu2 --test data_phase transmit_receipt_preserves_bounded_i2np_message_ids -- --exact --test-threads=1` | Passed, 1 test. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 132 passed, 1 ignored. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | One run; SAM CREATE returned `I2P_ERROR`, `failure_category=inbound-build`; no diagnostic JSON retained because the initial parser read the wrong stream. |
| `rtk cargo check --locked -p i2pr-runtime -p i2pr-transport-ssu2 -p i2pr-daemon --all-targets` | Passed after adding per-message receipt metadata. |

## Security and compatibility

- Receipts expose only opaque attempt IDs, bounded I2NP message IDs, direction,
  stage labels, and aggregate counts; no payload or peer identity is logged.
- Metadata travels beside encrypted datagrams and does not alter bytes, send
  admission, retransmission, queue limits, or cancellation behavior.
- No reference source, binary, or fixture was changed.

Disposition: **blocked; the one permitted diagnostic run did not retain its
attempt receipt.** Plan 465 owns one corrected capture run and must stop after
that evidence if no first missing transition is found.
