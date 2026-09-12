# Plan 193 — M6 i2pd mixed-router Streaming qualification

Status: **registered executable**. This plan supersedes the execution role of the historical duplicate-numbered `plans/188-m6-mixed-router-streaming-with-i2pd.md`. The old Plan 188 Streaming file remains historical context only; do not execute it as a numbered plan.

## 1. Goal

Close the first-family mixed-router Streaming gap against exact-pinned unmodified i2pd 2.61.0 by layering the existing `i2pr-client::streaming` implementation on the now-proven mixed-router destination path.

Plan 193 is not allowed to reimplement SSU2, tunnel construction, NetDB, LeaseSet2, ECIES/Garlic, or the Plan 192 I2CP-style Data envelope. It must reuse those passed seams and prove that ordinary Streaming establishes and transfers bytes in both directions over them.

Passing Plan 193 proves **i2pd-family mixed-router Streaming only**. Milestone 6 remains open until Plan 194 qualifies the same bounded path against Java I2P.

## 2. Starting authority

Required retained floor before implementation:

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 local destination product = passed
plan_188 short-build consumed-reference installs = retained-passed
plan_190 inbound NetDB reply-path correction = passed
plan_191 inbound-delivery boundary diagnosis = retained
plan_192 = passed-m6-i2cp-wire-format-corrective
m6_destination_remote_interop = passed-against-i2pd-through-destination-message-plane
milestone6_interoperable = not-yet-claimed
```

Plan 192 head used when this plan was registered:

```text
05d6d52870a81eda8891dc492d43c6d4b79bbae8
routine CI = 34668461179 (success)
workspace floor = 2283 passed, 6 ignored
```

If a newer implementation head exists when Plan 193 starts, treat the newer green head as the source floor and record it in `plans/193-status.md`.

## 3. Exact reference and environment

Use unmodified exact-pinned i2pd:

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
```

Controlled profile remains:

```text
root/sudo          = no
namespaces         = no
Docker/VM          = no
systemd            = no
public reseed      = no
public I2P network = no
router/service bind = loopback only
reference source   = clean exact pin
```

Create the reference Streaming destination through i2pd's public SAM surface. Reference-side zero-hop client tunnels are acceptable in the isolated topology; every counted i2pr route must continue using the real one-hop mixed-router tunnel material already proven by Plans 185–192.

## 4. Architecture lock

Direction A must be:

```text
ordinary local test client / i2pr service caller
 -> i2pr StreamingManager
 -> StreamingDestinationAdapter
 -> DestinationRouting
 -> Plan 192 short-transport + I2CP-style Data envelope
 -> ECIES/Garlic
 -> real i2pr outbound destination tunnel
 -> selected lease from validated remote Standard LeaseSet2
 -> i2pd destination
 -> ordinary i2pd Streaming/SAM service
```

Direction B must return through:

```text
i2pd Streaming/SAM client
 -> published i2pr Standard LeaseSet2
 -> real i2pr inbound destination tunnel
 -> TunnelData recovery
 -> ECIES/Garlic decrypt
 -> Plan 192 Data-envelope decode
 -> StreamingDestinationAdapter
 -> owning StreamingManager listener/connection
```

Forbidden counted substitutes:

- direct destination-over-SSU2 delivery;
- `LocalZeroHop` on the i2pr side;
- private `accept_from_peer` or similar injection after runtime startup;
- SAM/I2CP local fabric standing in for router-to-router Streaming;
- a new interop-only Streaming implementation;
- patched i2pd;
- public-network fallback.

## 5. First task: compose one canonical external Streaming driver

Add one bounded external driver rather than extending the historical M3 harness family. Preferred shape:

```text
crates/i2pr-daemon/tests/streaming_i2pd_external.rs
tests/integration/m6-interop/run-streaming-i2pd.sh
scripts/check-m6-streaming-i2pd-evidence.sh
```

The driver may reuse helper code or extract shared test-only helpers from `destination_tunnel_external.rs`, but must not duplicate production protocol implementations.

The runner must:

1. verify exact i2pr head and clean exact i2pd pin;
2. start fresh loopback-only unprivileged reference state;
3. establish the already-proven SSU2/tunnel/NetDB/destination stack through production seams;
4. create the i2pd reference Streaming destination through public SAM;
5. execute both Streaming directions;
6. collect sanitized command-derived evidence;
7. tear everything down and prove resource baselines;
8. fail nonzero on every missing mandatory row.

## 6. Direction A — i2pr initiates to i2pd

Required trajectory:

1. resolve or reuse a validated remote Standard LeaseSet2 from the real NetDB path;
2. call the ordinary i2pr Streaming connect surface;
3. emit SYN through the existing destination routing path;
4. consume i2pd's ordinary Streaming response through the inbound destination path;
5. reach `Established` without private state mutation;
6. small payload round-trip with digest equality;
7. a payload large enough to require multiple Streaming packets/window bookkeeping;
8. reverse-direction data after establishment;
9. orderly half-close and full close with bounded completion;
10. no retained pending connection/tunnel/destination bytes after cleanup.

## 7. Direction B — i2pd initiates to i2pr

Publish the normal i2pr destination Standard LeaseSet2 through the already-passed path. Using i2pd's public SAM STREAM client:

1. connect to the i2pr destination;
2. prove the SYN reaches the normal i2pr Streaming listener through the real inbound tunnel;
3. normal accept/SYN-response logic establishes the connection;
4. small payload digest matches both directions;
5. multi-packet payload digest matches both directions;
6. close/half-close releases exactly the targeted connection;
7. sibling connection remains unaffected.

No test may invoke internal listener admission after the external runtime has started.

## 8. Streaming interoperability matrix

The counted external matrix must include at least:

```text
i2pd-stream-a-established
i2pd-stream-a-small
i2pd-stream-a-large
i2pd-stream-a-half-close
i2pd-stream-b-established
i2pd-stream-b-small
i2pd-stream-b-large
i2pd-stream-b-half-close
i2pd-stream-siblings
```

Then exercise bounded protocol robustness using existing deterministic seams where possible:

```text
i2pd-stream-drop-data-retransmit
i2pd-stream-drop-ack-recovery
i2pd-stream-reorder-two-packets
i2pd-stream-duplicate-packet
i2pd-stream-stalled-reader-bounded
i2pd-stream-reference-disconnect-bounded
i2pd-stream-clean-resource-baseline
```

Do not build packet-manipulating namespaces or patch i2pd. If a robustness row cannot be driven without changing the controlled-environment contract, retain equivalent local deterministic coverage and explicitly justify the external omission in the status file. Establishment + small/large + close in both directions are never optional.

## 9. Wire-compatibility checkpoints

Because Plan 192 changed the destination Data-envelope composition, add focused regression assertions that the Streaming path does not accidentally double-wrap or regress it:

- outgoing Streaming packet becomes exactly one I2CP-style Data body;
- protocol byte is `PROTOCOL_TYPE_STREAMING`;
- negotiated source/destination ports survive encode/decode;
- Garlic clove contains the 9-byte short-transport I2NP envelope;
- inbound decode yields the original Streaming packet bytes before `StreamingManager::process_inbound_packet`;
- corrupted gzip/body CRC remains fail-closed before tunnel dispatch.

These should be ordinary local unit/integration tests; do not infer them only from external success.

## 10. Resource and privacy requirements

Retain existing ceilings for:

- Streaming connections;
- send/receive windows;
- retransmission queue entries/bytes;
- pending destination routing bytes;
- tunnel queues/reassembly;
- external runner attempts/timeouts.

Evidence may contain hashes/digests, counts, state transitions, sequence/ACK summaries where non-sensitive, exact pins, and public destination identifiers. Never record private destination keys, ECIES secrets, raw application payloads, or raw reference datadirs.

## 11. Evidence integrity

`scripts/check-m6-streaming-i2pd-evidence.sh` must reject at least:

- literal/unconditional `passed` records;
- missing exact i2pd pin/clean-checkout proof;
- missing real one-hop tunnel proof;
- `LocalZeroHop` or direct-transport counted paths;
- establishment inferred from a status file rather than command output;
- self-composed i2pr peer substituted for i2pd;
- missing direction A or B;
- missing digest verification;
- skipped external dependency treated as success;
- public-network participation;
- evidence containing private key/application plaintext material;
- missing cleanup/baseline gating.

Wire the static checker into routine CI. The expensive external execution may remain workflow-dispatch-only.

## 12. Hosted external lane

Extend the existing M6 manual workflow or add a narrowly named workflow step for the Plan 193 runner. It must run on the exact implementation head and upload only sanitized evidence.

Before Plan 193 closure, require at least one exact-head successful external i2pd Streaming run. Prefer two complete passes on the same revision if practical; if only one is retained, status must record why and routine CI still must be green on the exact head.

## 13. Stop conditions

Stop and create one narrow corrective instead of broadening Plan 193 if the first failure is attributable to a specific Streaming semantic, including:

- SYN/options/signature mismatch;
- port encoding/selection;
- sequence/ACK/NACK arithmetic;
- retransmission timing;
- CLOSE/RESET/half-close semantics;
- packet size/MTU interaction;
- remaining I2CP Data-envelope mismatch specific to protocol 6.

At the stop point, retain the minimized first failing packet/state transition and compare it to the current official Streaming specification plus exact-pinned i2pd behavior. Do not route around the defect through RAW/DATAGRAM or local fabric.

If failure occurs below Streaming (SSU2, build, NetDB, LeaseSet2, ECIES/Garlic), stop and identify the exact regression against Plans 184–192; do not relabel it as a Streaming failure.

## 14. Acceptance criteria

Plan 193 passes only when all are true:

1. exact-pinned clean i2pd 2.61.0 is used;
2. direction A reaches ordinary Streaming `Established`;
3. direction B reaches ordinary Streaming `Established` through normal listener/accept logic;
4. both directions traverse real one-hop i2pr destination tunnels and validated LeaseSet2/ECIES/Garlic paths;
5. small digests match both directions;
6. multi-packet/large digests match both directions;
7. close/half-close is bounded and correct;
8. sibling connections remain isolated;
9. the required bounded robustness rows or explicitly justified retained deterministic equivalents are green;
10. no direct/zero-hop/self-composed substitute is counted;
11. all Plans 184–192 focused regressions remain green;
12. workspace fmt/check/test/clippy/doc/static/deny floor is green;
13. static Plan 193 evidence checker passes in routine CI;
14. exact-head manual external i2pd Streaming lane passes;
15. `plans/193-status.md` records `milestone6_i2pd_streaming_interop = passed` and `milestone6_interoperable = not-yet-claimed`;
16. `next_executable_plan = 194`.

## 15. Handoff

On pass:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194
```

Plan 194 owns Java I2P second-family qualification and is the only remaining M6 closure gate.