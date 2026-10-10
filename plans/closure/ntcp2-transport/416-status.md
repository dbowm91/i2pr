# Plan 416 status: stopped — source trace did not localize an i2pr-owned correction

Closure token: `stopped-source-trace-did-not-localize-an-i2pr-owned-correction`.

Plan: `plans/implementation/ntcp2-transport/416-current-pin-ntcp2-authentication-stage-diagnosis.md`.
Status-transition commit: `57c8978` — start Plan 416 NTCP2 stage diagnosis.

## Scope and disposition

Plan 416 traced the Plan 415 forward result without another wire attempt or
source modification. The launcher emits `tcp_connected` after the bounded
socket connect succeeds, before Noise handshake bytes are written. Its later
`receiver-frame-read-failed` outcome is the runtime oracle's `Closed` result,
which is mapped by the launcher to `ReceiverFrameReadFailed`. This status does
not identify which NTCP2 handshake or data phase closed the stream.

The pinned i2pd helper derives the expected peer IdentHash from the supplied
RouterInfo and polls the stock transport manager's `IsConnected(peer_hash)`.
The pinned implementation checks whether that IdentHash is present in the
connected-peer map. The Plan 415 helper event therefore says that this map
lookup remained false until timeout. The attempt did not retain stock-log
milestones such as SessionRequest/SessionCreated/SessionConfirmed, and no
source evidence shows a concrete i2pr-owned configuration or state transition
that explains the mismatch. A small local C++ probe confirmed that
`istreambuf_iterator` does not make `ifstream::good()` false at EOF, ruling out
one apparent but incorrect helper-read hypothesis.

The diagnosis remains ambiguous. No production or harness code was changed,
and no wire attempt was made. Plan 416 is stopped under its explicit rule to
stop when source tracing cannot localize an i2pr-owned correction. Plan 434
remains blocked. Plan 417 now owns sanitized stock-log handshake-stage evidence
and a fresh one-forward-attempt budget. Reference source patching remains
prohibited; normal-daemon NTCP2
remains disabled and non-advertised.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Trace launcher TCP and receiver failure | Passed as source trace. `execute_initiator` emits `tcp_connected` after `service.dial`; receive oracle `Closed` maps to `ReceiverFrameReadFailed`. TCP connection alone is not authentication evidence. |
| Trace helper peer identity and connected-state check | Passed as source trace. The helper derives `peer_ident_hash` from supplied RouterInfo and calls i2pd's `TransportManager::IsConnected`; pinned i2pd checks connected-peer map membership. |
| Identify a concrete i2pr-owned discrepancy | Not achieved. Existing sanitized evidence lacks the stock handshake milestones needed to distinguish the first failed phase. |
| Change code or run a new wire attempt | Not done; Plan 416 forbids either without a localized source-owned correction. |
| Reference source and daemon posture | Unchanged. Pinned source was read only; daemon NTCP2 remains disabled and non-advertised. |

## Commands and outcomes

- `rtk rg -n -C 5 'receiver-frame-read-failed|tcp_connected|TransportManager::IsConnected|IsConnected\(' tools/i2pr-interop crates/i2pr-transport-ntcp2 crates/i2pr-runtime` — passed; located the status and helper call paths.
- `rtk sed -n '920,1085p' tools/i2pr-interop/src/main.rs` and `rtk sed -n '1216,1292p' tools/i2pr-interop/src/main.rs` — source trace confirmed TCP event timing and `Closed` mapping.
- `rtk sed -n '1200,1348p' tools/i2pr-interop/reference/i2pd-current/src/i2pd_current_ntcp2_driver.cpp` — source trace confirmed the helper's RouterInfo-derived peer hash and connected-state poll.
- `rtk sed -n '1072,1100p' target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5/libi2pd/Transports.cpp` — pinned i2pd `IsConnected` implementation checks the connected-peer map.
- A local C++20 `istreambuf_iterator` probe printed `good=1` after reading to EOF; the temporary probe and input were removed. This ruled out the suspected stream-state issue; it was not a router test.
- No wire attempt, build, or code test was run for Plan 416 because no correction was localized.
- Full workspace routine floor — not run; no code changed.

## Findings and unblock audit

- **Medium — NTCP2 loopback authentication remains unresolved.** Plan 415's helper timed out waiting for the expected connected peer, while the launcher reported a closed receive stream. Existing sanitized stages do not reveal the stock NTCP2 handshake milestone at which the peer diverged. No protocol defect or success is inferred.
- Plan 434 remains blocked on authoritative two-way authenticated I2NP evidence.
- Plan 433 remains blocked because Plan 431 stopped without an authorized independent non-loopback topology; Plan 432 passed.
- Plan 435 remains blocked on Plans 433/434; Plan 436 on 433; Plan 437 on 433/436; Plan 438 on 437; and Plan 439 on 433/435/436/438.
- Proposal 170 Plans 412/413 remain blocked on Plan 437. No plan in another work line became dependency-ready.
- Plan 417 is registered to add sanitized handshake-stage observation and a fresh bounded attempt. No other blocked successor was unblocked.

