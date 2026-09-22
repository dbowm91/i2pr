# Plan 235 — M6 Java Streaming post-accept response boundary corrective

Status: **in-progress-m6-java-streaming-post-accept-response-boundary-corrective**

## 1. Objective

Continue Plan 234's exact remaining boundary without reopening the retained
Plan-232 route-derived lease fixture or claiming Java-family/M6 closure:

```text
Java accept worker returned and stored I2PSocket
-> no i2pr inbound TunnelData during the frozen SYN epoch
```

Plan 235 must determine whether the Java-generated SYN response is emitted by
the stock Java Streaming path, admitted by the i2pr outbound/tunnel path, and
delivered to the exact installed inbound route. It may correct only a proven
test/helper or harness defect. An i2pr production change requires a separate
production corrective plan after an exact i2pr-owned defect is proven.

## 2. Registration basis

Plan 234 closed as:

```text
passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
```

on implementation SHA `343bc156e02ac5f819fba8b3644cefca4624f513`.
Three same-SHA counted attempts emitted:

```text
P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED
```

with `accept_requested=1 accept_entered=1 accept_returned=1
socket_stored=1 accept_errors=0`, but
`i2pr_inbound_tunneldata_count=0`. Plan-232 route parity and the retained raw
Destination bidirectional pass remain authoritative.

## 3. Invariants and non-goals

1. Java I2P remains pinned to `9134f808337b401e8e53c73734c81fab04280c9d`.
2. i2pd remains pinned to `635b013a612ff47278ef02acf8580a28e10e26c5`.
3. The Plan-232 route-derived gateway/tunnel helper and all parity guards stay
   unchanged.
4. Existing Plan-230/231/232 topology, ports, and frozen timeout windows stay
   unchanged; no timeout inflation or topology expansion is allowed.
5. Raw Java logs remain scratch-only; durable evidence is bounded counts,
   booleans, IDs, hashes, ports, digests, and stage tokens.
6. No public-I2P participation, reseed, VMComm, `netDb.alwaysQuery`, Java
   source patching, reflection, private-field mutation, direct NetDB/tunnel
   injection, or external-router/client patching.
7. M10 authority remains closed.

Plan 235 does not claim a Streaming capability or Java-family/M6 closure from
helper accept alone.

## 4. Work packages

### A — retain and gate the Plan-234 boundary

Run the focused Plan-232 guards and verify the retained raw-Destination pass
before a new Streaming attempt. Emit
`P235-A-PLAN234-BASELINE-REGRESSION` and stop if route parity,
publication separation, or the retained exact reverse digest no longer holds.

### B — Java response-stage observation

Add only bounded read-only observation that distinguishes:

```text
accept returned
Java Streaming response generation entered
Java response write/admission returned
helper-side exception
```

The observation must use stock public Java Streaming APIs or existing
sanitized log-count extraction. It must not infer a response solely from
`accept_returned=true`; that fact remains a separate boundary.

### C — i2pr transport/tunnel response attribution

Extend the external driver epoch with the exact SYN message identity and
bounded counters for:

```text
transport request accepted
outbound tunnel dispatch accepted
expected inbound TunnelData
unrelated inbound TunnelData
tunnel recovery / Garlic / adapter stages
response state transition
```

The first failing stage wins. If a response reaches i2pr and an i2pr-owned
parser/state transition fails, stop with a complete evidence bundle and
register a separate production corrective; do not fix production code here.

### D — narrow test-only corrective

If the evidence proves a helper/control race, request serialization defect,
missing driver pump, or test-only binding error, correct only that surface and
repeat the exact epoch with a new implementation SHA. If no test-only defect
is proven, close with the exact post-accept boundary and register no broader
work.

### E — final authority only after Streaming passes

If Direction A and B actually pass, run the full Java harness and reconcile
every Plan-200 C/D row under the Plan-234 R1/R2 rules. `REQUIRED_FAILED` remains
fail-closed; no row may be deleted or globally forgiven.

## 5. Acceptance and stop conditions

Successful terminal:

```text
P235-JAVA-STREAMING-PASSED
```

only with Direction A+B, live refresh parity, full Java harness exit 0, the M6
checker, final closure checker, and exact-head routine floor.

Otherwise emit exactly one earliest bounded `P235-*` terminal, retain the
Plan-232 raw pass, and keep Plans 201/204 blocked. A helper accept observation
alone cannot produce a family pass.

## 6. Attempt discipline

Commit before counted execution. Maximum three counted attempts per
implementation SHA. No between-attempt tuning. Missing external environment or
infrastructure failure before the SYN epoch is not a protocol pass.

## 7. Verification floor

The closing implementation SHA must run the repository routine floor from
`AGENTS.md`, the M6 static/final closure checkers, focused Plan-232/234/235
tests, Java helper compilation against the exact pinned jars, and the full
`tests/integration/m6-interop/run-java.sh` lane when claiming final closure.
