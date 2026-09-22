# Plan 236 — M6 Java Streaming response-emission and I2CP send attribution

Status: **registered-ready-m6-java-streaming-response-emission-and-i2cp-send-attribution**

## 1. Objective

Continue from Plan 235's exact bounded terminal without reopening the already
closed route, topology, raw-Destination, or Java accept work:

```text
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
```

Plan 236 must determine the earliest missing stage between:

```text
stock Java Streaming accepted the inbound SYN and returned a usable I2PSocket
    -> Java constructs/schedules the required Streaming response
    -> Connection.sendPacket(...)
    -> PacketQueue.enqueue(...)
    -> I2PSession.sendMessage(...)
    -> Java Router A receives/adopts the client message
    -> OCMOSJ / outbound tunnel dispatch
    -> exact target inbound-gateway path
    -> i2pr receives exact TunnelData
```

The plan is attribution-first. It may correct only a proven test/helper,
logging/evidence, or harness defect. It MUST NOT modify production Rust
Streaming, tunnel, Garlic, SSU2, NetDB, or Destination behavior unless this plan
first proves an exact i2pr-owned defect and then registers a separate production
corrective.

Plan 236 is not another broad Java-interoperability campaign.

## 2. Registration basis

Plan 235 closed on implementation SHA:

```text
61d75ae60b41392633512ff5f59b1318965695f9
```

with three same-SHA counted attempts producing:

```text
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
```

and the following stable facts:

```text
accept_requested=1
accept_entered=1
accept_returned=1
socket_stored=1
socket_surface_entered=1
socket_surface_ready=1
accept_errors=0
socket_surface_errors=0

i2pr_transport_request_accepted=1
i2pr_outbound_dispatch_attempts=2
i2pr_outbound_dispatch_accepted=2
i2pr_outbound_dispatch_rejections=0

i2pr_inbound_tunneldata_count=0
i2pr_expected_stream_tunneldata_count=0
i2pr_tunnel_recovery_count=0
i2pr_garlic_payload_count=0
i2pr_streaming_adapter_calls=0
```

Plan 232's route-derived lease parity and bidirectional raw-Destination pass
remain authoritative and are not to be relitigated.

## 3. Exact-pinned Java source model

Java I2P remains pinned at:

```text
2.13.0
9134f808337b401e8e53c73734c81fab04280c9d
```

The Plan-236 stage model is derived from that exact source:

1. `I2PServerSocketFull.accept()` delegates to the socket manager receive path.
2. `ConnectionHandler` removes the queued SYN and calls
   `ConnectionManager.receiveConnection(syn)`.
3. `ConnectionManager.receiveConnection()` validates the signed SYN, creates
   the inbound `Connection`, assigns a receive stream ID, and feeds the SYN to
   `ConnectionPacketHandler.receivePacket(...)`.
4. `ConnectionPacketHandler` records the inbound message, sets the next ACK
   time, and calls `con.eventOccurred()`.
5. The selected scheduler is responsible for causing the response/ACK send.
6. Normal first-send packets pass through `Connection.sendPacket()`.
7. `Connection.sendPacket()` delegates to `PacketQueue.enqueue()`.
8. `PacketQueue.enqueue()` serializes the Streaming packet and calls stock
   `I2PSession.sendMessage(... PROTO_STREAMING ...)`.
9. The Java router then owns the ordinary I2CP client-message / OCMOSJ /
   outbound-tunnel path.

Plan 236 MUST preserve this ordering. A returned application `I2PSocket` is
not evidence that stages 4–9 occurred.

## 4. Invariants and non-goals

1. Java and i2pd pins remain unchanged.
2. Plan-230/231/232 controlled topology remains unchanged.
3. Plan-232 route-derived lease gateway/tunnel helper remains the source of
   truth at all three local lease sites.
4. Router B remains publication target where already defined; publication
   target and lease gateway remain distinct concepts.
5. Plan-231 stock-Java/router attribution machinery may be reused but not
   broadened into a new topology.
6. Frozen Plan-230/231/232/235 timing windows remain unchanged. No timeout
   inflation.
7. Maximum three counted attempts per implementation SHA, with no
   between-attempt tuning.
8. No public I2P, reseed, VMComm, `netDb.alwaysQuery`, private-field mutation,
   reflection, Java router source patching, direct NetDB/tunnel/profile
   injection, or external-router/client patching.
9. The Java helper may use stock public APIs and bounded test-side counters.
10. Raw Java/router logs and packet captures are scratch-only. Durable evidence
    must be sanitized typed facts.
11. Plan-200/201 client-LS2 final-closure rows remain required under the
    Plan-234 reconciliation authority. Plan 236 does not silently supersede
    them.
12. M10 authority through Plans 213–215 remains closed.

## 5. Work package A — freeze the Plan-235 baseline

Before adding new response-emission evidence, preserve focused tests proving:

```text
plan232_route_parity = true
plan232_raw_reverse_pass = retained
plan234_java_accept_returned = true
plan235_socket_surface_ready = true
plan235_i2pr_outbound_admission = true
plan235_i2pr_inbound_tunneldata = 0
```

If a structural change causes a Plan-232/234/235 invariant to fail before the
new response epoch begins, stop:

```text
P236-A-BASELINE-REGRESSION
```

Do not compensate with topology, timeout, or lease changes.

## 6. Work package B — identify the exact stock-Java response scheduler path

Before execution, perform a source-locked trace on the pinned Java tree from:

```text
ConnectionPacketHandler.receivePacket(SYN)
    -> Connection.eventOccurred()
    -> SchedulerChooser-selected scheduler
    -> response/ACK construction
    -> Connection.sendPacket()
```

Record the exact class/method responsible for the first response packet for an
accepted inbound connection.

The implementation must not guess that `accept()` itself sends the response.
The plan's evidence helper/checker should carry the exact pinned method/class
names discovered here so later Java upgrades fail loudly rather than silently
changing the assumed ordering.

Required source-lock evidence:

```text
java_response_scheduler_class
java_response_scheduler_method
java_response_packet_kind
java_response_send_method
java_packetqueue_method
java_i2psession_send_method
java_source_pin
```

## 7. Work package C — response-generation and PacketQueue attribution

Add the minimum bounded observation necessary to distinguish:

```text
C0 accepted socket, no response scheduler event
C1 response scheduler event occurred
C2 response packet constructed
C3 Connection.sendPacket entered
C4 PacketQueue.enqueue entered
C5 PacketQueue enqueue returned false / send failed
C6 I2PSession.sendMessage called
C7 I2PSession.sendMessage accepted/returned success
C8 I2PSession send threw / returned failure
```

Preferred observation order:

1. existing stock Java Streaming DEBUG/INFO signatures and stock rate stats;
2. stock client/router I2CP logs already supported by the harness;
3. bounded helper-visible public API state only where it proves the stage;
4. scratch-only pcap/log inspection as a last discriminator, sanitized into
   counts/booleans before durable storage.

Do **not** patch the pinned Java I2P Streaming implementation to add counters.

If stock logs are insufficient to identify C1–C8, emit:

```text
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
```

and stop rather than inferring success.

## 8. Work package D — Router-A I2CP/client-message attribution

If Java proves `I2PSession.sendMessage` success, continue into the stock
router-side path and correlate only the Plan-236 response epoch.

Capture bounded evidence for:

```text
java_i2cp_send_observed
java_i2cp_protocol_streaming
java_i2cp_target_hash_match
java_client_message_admitted
java_client_message_statuses
java_target_leaseset_selected
java_outbound_tunnel_selected
java_dispatch_outbound_called
java_outbound_gateway_enqueued
java_outbound_gateway_overflow
java_correlated_dispatch_failure
```

Reuse Plan-231/232 attribution mechanisms where possible.

For an ACK-only response, do not require a Streaming
`SendMessageStatusListener` callback if exact pinned `PacketQueue` uses the
boolean `I2PSession.sendMessage(...)` overload. Instead prove the actual
overload selected and use the nearest stock router-side client-message
observation as the next stage.

Never manufacture a message-status expectation that the pinned source does not
provide.

## 9. Work package E — tunnel and target-IBGW continuation

Only if Router A proves client-message admission, trace the response through the
already-established controlled route:

```text
Router A outbound gateway
    -> exact outbound endpoint/transit processing
    -> selected target lease gateway
    -> exact target inbound gateway installed
    -> TunnelData toward i2pr
```

Record:

```text
selected_target_gateway_hash
selected_target_tunnel_id
target_ibgw_present
a_gateway_enqueue_delta
transit_obep_processed_delta
target_ibgw_dispatch_delta
i2pr_ssu2_datagram_delta
i2pr_i2np_message_delta
i2pr_inbound_tunneldata_delta
i2pr_expected_tunneldata_delta
```

The Plan-232 route-derived target remains authoritative. Do not derive the
expected gateway from the publication target.

## 10. Work package F — i2pr-owned continuation

If and only if exact response TunnelData reaches i2pr, continue through:

```text
TunnelData receive
    -> tunnel recovery
    -> Garlic decode
    -> Destination dispatch
    -> StreamingDestinationAdapter::receive
    -> Streaming connection state
```

Use the existing Plan-234/235 counters. The first failing i2pr-owned stage wins.

Possible terminals:

```text
P236-F-I2PR-TUNNEL-RECOVERY-FAILED
P236-F-I2PR-GARLIC-DECODE-FAILED
P236-F-I2PR-NO-STREAMING-PAYLOAD
P236-F-I2PR-STREAMING-ADAPTER-FAILED
P236-F-I2PR-DISPATCHED-NOT-ESTABLISHED
P236-F-DIRECTION-A-ESTABLISHED
```

An i2pr-owned failure is evidence for a new production corrective. Plan 236
itself MUST NOT modify production code.

## 11. Required classification order

Emit exactly one earliest terminal for every counted response epoch.

The classifier order is:

```text
P236-A-BASELINE-REGRESSION

P236-B-JAVA-RESPONSE-SCHEDULER-NOT-OBSERVED
P236-B-JAVA-RESPONSE-PACKET-NOT-CONSTRUCTED
P236-C-JAVA-SENDPACKET-NOT-OBSERVED
P236-C-JAVA-PACKETQUEUE-NOT-OBSERVED
P236-C-JAVA-PACKETQUEUE-SEND-FAILED
P236-C-JAVA-I2PSESSION-SEND-NOT-OBSERVED
P236-C-JAVA-I2PSESSION-SEND-FAILED
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP

P236-D-JAVA-ROUTER-I2CP-NOT-OBSERVED
P236-D-JAVA-CLIENT-MESSAGE-NOT-ADMITTED
P236-D-JAVA-NO-TARGET-LEASESET
P236-D-JAVA-NO-OUTBOUND-TUNNEL
P236-D-JAVA-DISPATCH-NOT-CALLED
P236-D-JAVA-OUTBOUND-GATEWAY-NOT-ENQUEUED

P236-E-TRANSIT-NOT-PROCESSED
P236-E-TARGET-IBGW-NOT-PRESENT
P236-E-TARGET-IBGW-NO-DISPATCH
P236-E-I2PR-NO-EXPECTED-TUNNELDATA

P236-F-I2PR-TUNNEL-RECOVERY-FAILED
P236-F-I2PR-GARLIC-DECODE-FAILED
P236-F-I2PR-NO-STREAMING-PAYLOAD
P236-F-I2PR-STREAMING-ADAPTER-FAILED
P236-F-I2PR-DISPATCHED-NOT-ESTABLISHED
P236-F-DIRECTION-A-ESTABLISHED

P236-OBSERVABILITY-GAP
```

Do not emit a later-stage terminal when an earlier stage is unproven.

## 12. Work package G — narrow corrective rules

Plan 236 may correct a test/helper/harness defect only if the attribution proves
it.

Examples in scope after proof:

- response-state extraction is looking at the wrong Java log/context;
- helper lifecycle closes or suppresses the connection before the ordinary
  stock response scheduler can run;
- test driver stops pumping before the existing frozen epoch expires;
- log/status parsing conflates ACK-only and SYN-bearing response packets;
- correlation uses the wrong client session or target hash.

After such a correction:

1. commit the corrective;
2. receive a new three-attempt budget for that new implementation SHA;
3. rerun the identical topology/windows;
4. document why the prior SHA was test-invalid.

Not in scope:

- changing Java I2P source;
- changing i2pr production Streaming;
- changing tunnel/SSU2/NetDB behavior;
- changing profile/topology;
- extending timeouts to make the response appear.

## 13. If Direction A unexpectedly establishes

If Plan 236 reaches:

```text
P236-F-DIRECTION-A-ESTABLISHED
```

do not declare Java-family M6 passed.

Continue only through the already-defined retained qualification authority:

1. Direction-A bidirectional small + multi-packet payload equality;
2. Direction-B Java -> i2pr connect/accept;
3. live route-derived refresh/republication parity;
4. close/EOF and sibling isolation;
5. full Java harness;
6. Plan-200 §C/D pass-or-explicit-stronger-external-evidence reconciliation;
7. `run-java.sh` exit 0;
8. mixed-router and final-closure evidence checkers.

If those all pass on the same exact head, Plan 236 may close Java second-family
M6 without a ceremonial successor. Otherwise stop at the first retained
qualification blocker.

## 14. SAM workspace-floor hang is separate evidence

The Plan-235 exact-head workspace serial test run was stopped after hanging in
`sam_stream_final_acceptance`. Plan 235 correctly records that run as
incomplete, not passed.

Plan 236 must not interpret that hang as evidence about the Java response path:
Plans 234/235 changed Java external test/helper/evidence surfaces, not SAM
production or the SAM final-acceptance suite.

Verification handling:

1. run the focused Plan-236/235/234/232 suites first;
2. run routine check/clippy/doc/deny/static scripts;
3. attempt the full serial workspace test with enough process/test-name
   visibility to identify the exact SAM test if it stalls again;
4. if the same SAM suite stalls, record:

```text
P236-V-WORKSPACE-FLOOR-INCOMPLETE-SAM-HANG test=<exact-test-or-last-observed-test>
```

5. do not represent the workspace floor as passed;
6. do not change SAM under Plan 236;
7. a diagnostic Plan-236 boundary may still close if no production code changed
   and all focused Java attribution evidence is valid;
8. **final Java-family/M6 closure is forbidden** until the full exact-head
   workspace floor completes successfully or a separately registered SAM
   test-hygiene corrective closes that blocker.

## 15. Durable evidence schema

At minimum, emit sanitized TSV facts equivalent to:

```text
attempt
implementation_sha
java_pin
lane=streaming-direction-a

plan232_route_parity
plan235_socket_surface_ready
plan235_transport_accepted
plan235_dispatch_accepted

java_response_scheduler_observed
java_response_packet_constructed
java_response_packet_kind
java_sendpacket_observed
java_packetqueue_observed
java_packetqueue_result
java_i2psession_send_observed
java_i2psession_send_result
java_i2psession_send_exception_class
java_i2cp_send_observed
java_i2cp_target_hash_match
java_client_message_admitted
java_send_statuses

target_gateway_hash
target_tunnel_id
target_ibgw_present
a_gateway_enqueue_delta
transit_processed_delta
target_ibgw_dispatch_delta

i2pr_ssu2_datagram_delta
i2pr_i2np_message_delta
i2pr_inbound_tunneldata_delta
i2pr_expected_tunneldata_delta
i2pr_recovery_delta
i2pr_garlic_delta
i2pr_streaming_adapter_delta
i2pr_connection_state

terminal
```

No payload plaintext, private keys, reply keys/tags, or raw packet contents.

## 16. Required focused tests

Add at least equivalent tests for:

```text
p236_plan235_baseline_precedes_response_attribution
p236_source_lock_names_exact_pinned_response_path
p236_scheduler_missing_precedes_packetqueue_missing
p236_packet_not_constructed_precedes_sendpacket_missing
p236_sendpacket_missing_precedes_packetqueue_missing
p236_packetqueue_failure_precedes_router_i2cp_missing
p236_i2psession_send_failure_precedes_router_i2cp_missing
p236_ack_only_path_does_not_require_status_listener
p236_router_i2cp_missing_is_distinct_from_no_target_leaseset
p236_no_outbound_tunnel_is_distinct_from_gateway_enqueue_failure
p236_target_ibgw_uses_route_derived_lease_not_publication_target
p236_no_expected_tunneldata_is_distinct_from_recovery_failure
p236_i2pr_owned_terminal_requires_expected_tunneldata
p236_direction_a_established_does_not_by_itself_close_m6
p236_workspace_sam_hang_cannot_be_recorded_as_pass
p236_no_production_change_before_owned_defect
```

## 17. Static checker requirements

Extend the M6 evidence checker to reject:

1. any `P236` token under production `src/`;
2. removal/regression of Plan-232 route-derived lease guards;
3. topology/pin/window changes;
4. Java I2P source patches;
5. a response-emitted claim without PacketQueue/I2PSession or equivalent
   exact-pinned evidence;
6. a router-I2CP claim without a Java-send claim;
7. an i2pr-owned failure without exact inbound TunnelData;
8. an M6 pass while full `run-java.sh` is nonzero;
9. an M6 pass while Plan-200 §C/D authority remains unresolved;
10. an M6 pass while the exact-head workspace test floor is incomplete;
11. raw log/pcap promotion into durable evidence;
12. global forgiveness of `REQUIRED_FAILED`.

## 18. Verification floor

On each implementation SHA:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

cargo test --locked -p i2pr-daemon --test java_tunnel_external p232_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p234_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p235_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash -n scripts/check-m6-final-closure-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
```

Also compile all staged Java helpers against the exact pinned jars.

Before any final Java-family/M6 pass claim:

```bash
cargo test --locked --workspace --all-targets -- --test-threads=1
bash tests/integration/m6-interop/run-java.sh
bash scripts/check-m6-final-closure-evidence.sh
```

If the workspace command stalls again in SAM, apply §14 rather than claiming a
pass or modifying SAM in this plan.

## 19. Attempt discipline

- implementation commit first;
- maximum three counted external attempts per implementation SHA;
- no between-attempt tuning;
- evidence directory unique per counted attempt;
- source/logging configuration fixed before attempt 1;
- infrastructure failure before the Plan-236 response epoch may be VOID only
  with explicit evidence that no Plan-236 terminal was reachable;
- the first attributable protocol/test stage wins.

## 20. Acceptance criteria

Plan 236 closes correctly when all of the following are true:

1. Plan-235 baseline is retained.
2. Exact pinned Java response scheduler/send path is source-locked.
3. Three counted attempts or an earlier decisive reproducible result identify
   one exact earliest response boundary.
4. Java response generation is not inferred from `accept()`.
5. PacketQueue/I2PSession semantics match the exact pinned overload actually
   used.
6. Router-side attribution occurs only after Java send is proven.
7. i2pr-owned attribution occurs only after expected TunnelData is proven.
8. No production code changes occur unless handed to a new corrective.
9. Raw-Destination Plan-232 pass remains authoritative.
10. Plan-200/201 final-closure authority remains fail-closed.
11. Verification results are represented accurately, including any recurring
    SAM workspace hang.
12. Registry, roadmap, Plan 201, and Plan 204 are updated together at closure.

## 21. Closure outcomes

### Outcome A — Java response never reaches PacketQueue/I2PSession

Close with the exact B/C terminal. This is a Java Streaming response-generation
or observability boundary, not an i2pr production defect.

### Outcome B — Java sends; Router A does not admit/dispatch

Close with the exact D terminal. Register only the router/client-message
successor justified by that stage.

### Outcome C — Router/tunnel path dispatches; i2pr sees no expected TunnelData

Close with the exact D/E terminal and preserve the route/tunnel evidence.
Register a narrow transport/tunnel successor only if warranted.

### Outcome D — expected TunnelData reaches i2pr and i2pr fails later

Close with the exact F terminal and register a separate production corrective.

### Outcome E — Direction A establishes

Emit:

```text
P236-F-DIRECTION-A-ESTABLISHED
```

then apply §13. Final M6 closure is allowed only if all retained qualification,
publication-authority, full-harness, final-checker, and exact-head workspace
gates pass.

## 22. Closure evidence

`plans/closure/mixed-router-interop/236-status.md` must record:

- implementation SHA(s);
- exact pins;
- exact source-locked Java response path;
- per-attempt typed Plan-236 evidence;
- exact terminal;
- retained Plan-232/235 baseline;
- whether any test-only corrective was admitted;
- proof that production code did or did not change;
- focused verification results;
- full workspace result or exact SAM-incomplete token;
- Plan-200/201 authority state;
- Plan-201/204 unblock audit;
- M10 unchanged confirmation.

## 23. Registration disposition

```text
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
plan_236 = registered-ready-m6-java-streaming-response-emission-and-i2cp-send-attribution

plan_201 = blocked-pending-plan236-java-streaming-response-emission-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan236
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 236-m6-java-streaming-response-emission-and-i2cp-send-attribution
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 24. Handoff notes for smaller-model execution

Do not touch the lease fixture, topology, profile bootstrap, or production
Streaming code.

Start by tracing the exact pinned Java scheduler path after
`ConnectionPacketHandler.receivePacket(SYN)`. Determine what concrete stock
event causes the first response packet and what log/stat signature proves it.

Then run the existing Streaming-only external lane and answer these questions
in order:

```text
Did Java schedule a response?
Did Java construct a response packet?
Did Connection.sendPacket run?
Did PacketQueue.enqueue run?
Did I2PSession.sendMessage accept it?
Did Router A receive the client message?
Did Router A select the target LS2 and outbound tunnel?
Did the controlled tunnel path reach the route-derived target IBGW?
Did i2pr receive exact TunnelData?
```

Stop at the first “no”.

Do not infer a SYN-ACK from a returned Java socket. Do not infer an i2pr defect
until exact response TunnelData reaches i2pr.
