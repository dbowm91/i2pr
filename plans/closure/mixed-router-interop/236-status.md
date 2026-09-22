# Plan 236 status — M6 Java Streaming response-emission observability boundary

Status: `passed-m6-java-streaming-response-emission-observability-gap`

Plan 236 is closed at the first unproven response-emission stage. It did not
claim Java response generation, Router-A I2CP admission, tunnel dispatch, or
an i2pr-owned defect.

## Authority and exact pins

The implementation checkpoint is commit `41b6ccfea16ca051f31605dbf89bf749d0cc77e5`.
The Java reference remained Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; i2pd and the Plan-232 controlled
topology were unchanged.

The source lock proved this exact path in the pinned checkout:

```text
ConnectionPacketHandler.receivePacket(SYN)
  -> Connection.eventOccurred()
  -> SchedulerReceived.eventOccurred()
  -> Connection.sendPacket(PacketLocal)
  -> PacketQueue.enqueue(PacketLocal)
  -> boolean I2PSession.sendMessage(... SendMessageOptions)
```

For the inbound Java connection, the status-listener overload is conditional;
Plan 236 did not manufacture a status callback requirement for the ACK-only
path.

## Counted evidence

The same committed implementation SHA produced the same terminal on two
identical counted attempts:

| Attempt | Evidence | Terminal | Bounded facts |
| --- | --- | --- | --- |
| 2 | `target/interop/m6-java-evidence-plan236-attempt2` | `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` | source lock `true`; response observation complete `false`; all response/router/tunnel stage observations unknown |
| 3 | `target/interop/m6-java-evidence-plan236-attempt3` | `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` | same facts as attempt 2 |

The pre-commit attempt 1 exposed and corrected a test-parser defect and is not
counted as Plan 236 evidence. No Java source, production Rust, topology,
profile, lease fixture, timeout, or external client was patched.

Plan 235 remains retained: Java accepted and returned a usable public
`I2PSocket`, i2pr outbound admission was `2/2`, and inbound TunnelData was `0`.
That socket result is not promoted to response emission.

## Verification

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p23 -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n scripts/interop/check-m6-java-response-source-lock.sh tests/integration/m6-interop/run-java.sh scripts/check-m6-mixed-router-acceptance-evidence.sh scripts/check-m6-final-closure-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
javac <ReferenceStreamingService.java against exact-pinned jars>
all repository boundary/vector/evidence checkers in AGENTS.md
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
```

The serial full-workspace command was started but did not complete: it reached
`sam_stream_final_acceptance` and was terminated after the recurring SAM hang.
Record this as the required bounded diagnostic, not as a pass:

```text
P236-V-WORKSPACE-FLOOR-INCOMPLETE-SAM-HANG test=sam_stream_final_acceptance
```

No SAM or production behavior was changed to work around the hang. Remote CI
verification remains required before the repository handoff is complete.

## Disposition and unblock audit

```text
plan_201 = blocked-after-plan236-java-response-emission-observability-gap
plan_204 = blocked-on-m6-java-second-family-closure-after-plan236
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-registered; new response-observability corrective required
```

Plan 201 and Plan 204 remain blocked because the Java-family response path and
the retained Plan-200/201 authority are unresolved. M10 authority through
Plans 213–215 is unchanged. No downstream plan became dependency-ready.

## Follow-up boundary

A future corrective may improve stock-Java response observability only under a
new registered plan. It must retain the exact source lock, avoid patching the
pinned Java implementation, and continue to stop before Router-A/i2pr
attribution until `I2PSession.sendMessage` is proven.
