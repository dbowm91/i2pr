# Plan 411 — Java SAM/I2CP session-create diagnostic for Plan 375

Status: **in-progress-java-sam-session-create-diagnostic**.

Subsystem: Proposal 170 / Java ELS2 qualification.

## Objective and readiness

Attribute the Plan 375 Java SAM `SESSION CREATE STYLE=DATAGRAM DESTINATION=TRANSIENT`
failure (`I2P_ERROR`, `Address already in use`) to a bounded stage in a fresh,
controlled Java I2P 2.13.0 environment. Plan 375's closure explicitly requires
a separately registered diagnostic because its observed response was not
retained and Plan 279's frozen attempt budget is spent.

Hard prerequisites are satisfied: Plan 373 passed; the exact Java pin
`9134f808337b401e8e53c73734c81fab04280c9d` builds with JDK 21; Plan 375's
source freeze is recorded; `ControlledRouter.java` provides a test-only,
loopback-bound launcher over stock public router startup. Plan 411 does not
reuse or execute `tests/integration/floodfill/run-java-floodfill.sh`.

## Classification and invariants

**Infrastructure/diagnostic.** This plan attributes a setup failure and does
not qualify ELS2 or change a product capability.

1. Java I2P remains byte-for-byte unmodified at the frozen pin. Run the
   reference under JDK 21; compile the existing test-only launcher with
   `javac --release 17`, since this host has no JDK 21 compiler.
2. Every listener and peer endpoint is IPv4 loopback; reseed and public
   networking remain disabled.
3. One clean invocation contains one `SESSION CREATE` request. No retries,
   repeated SAM commands, or hidden attempt budget.
4. Reference keys, destination replies, raw SAM replies, and raw router logs
   stay in owner-only ephemeral scratch and are deleted on exit.
5. Committed evidence contains only the Java pin, topology facts, bounded stage
   tokens, response class, and a digest of the exact response. Never include
   `DESTINATION` or arbitrary peer/router data.
6. Do not modify Plan 279 or count this execution against its frozen budget.
7. No ELS2 support, type-5 behavior, Java interoperability, or RouterInfo
   advertisement claim follows from this diagnostic.

## Scope

In scope: a new independent runner under `tests/integration/els2/`, an evidence
checker with negative self-tests, one fresh Java router with loopback SAM and
I2CP listeners, one exact SAM request, an ephemeral preflight observation of
the default UDP bind `127.0.0.1:7655`, exact-pinned source tracing of the
returned error, and a closure record with a precise diagnosis or an explicit
unresolved stop. The pinned SAM source calls `getV3DatagramServer` before
constructing the client session, so client tunnels and an i2pr peer are not
required to attribute an address-bind failure.

Out of scope: the Plan 375 Java ELS2 driver/matrix; the spent Plan 279 runner;
any Java source or binary patch; production Rust code; network-ID or bandwidth
tier changes; public network use; support, conformance, or advertisement
changes.

## Ordered work packages

1. Add a new runner that validates the cached Java revision/build metadata,
   compiles only the existing test-only `ControlledRouter`, and creates a fresh
   datadir plus loopback ports. Preflight whether UDP `127.0.0.1:7655` can be
   bound; record only a boolean. Start one stock Java router with reseed and
   public update disabled. Do not invoke the Plan 279 runner or its i2pr
   qualification driver.
2. Send one `HELLO VERSION`, then one uniquely identified DATAGRAM
   `SESSION CREATE` with a transient destination and signature type 7. Capture
   the complete response only in ephemeral scratch. Map it through a closed
   classifier (`established`, `address-in-use`, `sam-error`, `timeout`,
   `transport-error`, `invalid-response`) and emit its SHA-256, never its raw
   text. Bound router startup, SAM bind/readiness to 300 seconds, SAM connect/read,
   teardown, and total runner
   time; all child processes are owned and terminated on every exit path.
3. At the exact Java pin, trace the observed response string through the
   responsible SAM/I2CP code path. Record source filenames, methods, and the
   specific condition that emits the response; do not patch, instrument, or
   rebuild Java source.
4. Add a fail-closed checker and self-test for the evidence schema, exact pin,
   loopback topology, one-request limit, secret/raw-log exclusion, and valid
   stage/classification combinations.
5. Execute exactly once. If mesh health or pin validation fails, stop without
   retrying. If the response is `address-in-use`, require source-level
   attribution before calling the diagnostic successful. If the failure does
   not reproduce, classify the prior event as unreproduced and keep Plan 375
   blocked pending its own ELS2 driver. If a source attribution remains
   ambiguous, close this plan stopped/blocked with the evidence gap.

## Failure, cancellation, and compatibility

The runner has a fixed wall-clock deadline and one attempt. It fails closed on
port collision, pin mismatch, missing router readiness, or missing evidence.
TERM is sent to every owned Java process; bounded grace is followed by KILL;
scratch removal is unconditional. The caller may override evidence output
directory only; it may not override the Java pin or bind addresses. Existing
Plan 279 artifacts and budgets remain unchanged.

## Verification

```sh
bash tests/integration/els2/run-java-sam-diagnostic.sh
python3 scripts/check-java-sam-diagnostic.py
python3 scripts/check-java-sam-diagnostic.py --self-test
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Also run formatting/whitespace checks and the affected Java-reference pin,
source-lock, and Plan 375/ELS2 boundary checkers. No production workspace test
floor is required unless production files change.

## Acceptance and closure evidence

Pass only if one fresh run reaches the SAM session-create stage, emits a
sanitized artifact, and the exact-pinned Java source trace attributes the
response or proves the prior failure unreproduced. Otherwise stop with an
explicit token and retain the blockers. The closure record must include the
command/result, pin, topology and listener proof, artifact digest, source
trace, teardown result, tests/checkers, findings, Plan 375 readiness, and the
unblock audit for Plans 377 and 378.
