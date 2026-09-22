# Plan 235 status — M6 Java Streaming post-accept response boundary corrective

Status: **passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound**.

Plan of record:
[`235-m6-java-streaming-post-accept-response-boundary-corrective.md`](../../implementation/mixed-router-interop/235-m6-java-streaming-post-accept-response-boundary-corrective.md).

## Registration basis

Plan 234 closed with the exact terminal
`P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`: Java's bounded accept state was
`requested=1 entered=1 returned=1 stored=1 errors=0`, while i2pr observed no
inbound `TunnelData` in the frozen SYN epoch. The retained Plan-232 route
parity and raw-Destination reverse pass remain valid. Plan 235 is a narrow
follow-up at that post-accept / pre-i2pr-inbound boundary.

```text
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
plan_201 = blocked-after-plan235-java-socket-surface-ready-no-i2pr-inbound
plan_204 = blocked-on-m6-java-second-family-closure-after-plan235
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
active_plan = none
next_executable_plan = none
```

Plan 235 must not change production Rust behavior without a separately proven
i2pr-owned defect and a new production corrective plan.

## Closure disposition

Plan 235 closes at Outcome B with the exact earliest bounded terminal:

```text
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
```

The corrective distinguishes the Plan-234 accept fact from the public Java
socket surface and from i2pr outbound admission. The stock Java helper
reported `accept_requested=1 accept_entered=1 accept_returned=1
socket_stored=1 accept_errors=0 socket_surface_entered=1
socket_surface_ready=1 socket_surface_errors=0`. The Rust driver accepted the
Streaming transport request and both outbound cell dispatches (`1`, `2/2`),
then observed zero inbound TunnelData, zero expected TunnelData, zero
recovery/Garlic/adapter stages, and no established transition.

This proves a narrower post-accept / pre-i2pr-inbound boundary. It does not
prove that Java emitted a wire SYN-ACK, and it does not prove an i2pr-owned
production defect. No production `src/` file changed, no test-only defect was
proven, and Plan 235 registers no broader corrective successor.

## Exact implementation and pins

```text
implementation_sha = 61d75ae60b41392633512ff5f59b1318965695f9
java_i2p = 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd = 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_232_raw_destination_pass = retained-from-plan234-attempt2
```

Plan 234's retained raw-Destination reverse pass remains the authority for
the independent Java-family raw path. Plan 235's counted runs were
Streaming-only diagnostics and did not replace that retained pass.

## Counted attempts

All three attempts used the exact implementation SHA and frozen Java/i2pd
pins, loopback topology, and timeout windows. No between-attempt tuning
occurred.

| Attempt | Evidence directory | Result |
| --- | --- | --- |
| 1 | `target/interop/m6-java-evidence-plan235-attempt1` | `P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND`; Java socket surface ready; transport accepted `1`; outbound dispatch `2/2`; inbound TunnelData `0`. |
| 2 | `target/interop/m6-java-evidence-plan235-attempt2` | Same exact terminal and same bounded counts. |
| 3 | `target/interop/m6-java-evidence-plan235-attempt3` | Same exact terminal and same bounded counts. |

The authoritative durable observations are sanitized TSV counts/booleans and
stage tokens only; raw Java logs remain scratch-only.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Retain Plan-234/232 baseline | `p235-plan234-baseline`: route parity and publication separation true; retained Plan-232 raw-Destination pass | passed |
| Observe Java accept and public socket surface | `p235-java-response-state`: returned/stored/ready `1`, errors `0` | passed |
| Observe i2pr outbound request/admission | `p235-syn-epoch`: transport accepted `1`, dispatch accepted `2/2`, rejections `0` | passed |
| Observe expected inbound response | `p235-syn-epoch`: inbound `0`, expected `0` | blocked at exact boundary |
| Distinguish downstream recovery/decode/adapter stages | all bounded downstream counters `0`; no later stage falsely classified | passed |
| Java Direction A+B and refresh | Direction A did not establish; not reached | not satisfied |
| Full Java harness exit 0 / final M6 closure | wrapper exited `1` on required Streaming and Plan-200/201 rows; final gate remains fail-closed | not satisfied |
| Production corrective | no i2pr-owned defect proven | not applicable |

## Verification

Passed:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
cargo test --locked -p i2pr-daemon --test java_tunnel_external p234_ -- --test-threads=1  # 16 passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external p235_ -- --test-threads=1  # 5 passed
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'  # 18 passed
```

Each counted external attempt also compiled the Java helper against the exact
cached Java I2P jars and ran the workspace-gates slice. The final closure gate
correctly remains non-passing because no all-pass cross-family ledger exists.
The prior workspace-wide serial test run was stopped after hanging in
`sam_stream_final_acceptance`; it is not represented as a pass.

## Unblock audit

No future plan can be unblocked by Plan 235:

- Plan 201 remains blocked after the exact Java socket-surface-ready /
  no-i2pr-inbound boundary; its Java-family publication and final-closure
  authority gates remain unresolved.
- Plan 204 remains blocked on independent M6 Java second-family closure;
  closed M10 authority through Plans 213–215 is unchanged.
- Plan 205 remains retained/deferred; no direct-I2CP evidence authorizes the
  SAM/helper pivot.
- No successor corrective is registered because Plan 235 proved neither a
  helper/control defect nor an i2pr-owned production defect.

```text
plan_201 = blocked-after-plan235-java-socket-surface-ready-no-i2pr-inbound
plan_204 = blocked-on-m6-java-second-family-closure-after-plan235
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
active_plan = none
next_executable_plan = none
```
