# Plan 298 — Streaming active-fingerprint differential harness

Status: registered-anonymity-streaming-fingerprint-harness-blocked-on-296

Classification: infrastructure + evidence only. No production Streaming tuning is authorized.

Hard dependencies: Plan 296 closed; existing M6 local Streaming authority and Plan 193 i2pd progression evidence retained.

## Objective

Build a hostile-remote-Destination black-box harness that measures observable Streaming behaviors capable of classifying i2pr, Java I2P, and i2pd under identical scripted stimuli, without changing i2pr production constants.

## Why ready after Plan 296

The router already composes real Destination tunnels and Streaming. Plan 296 establishes the application-boundary invariant; this plan isolates lower-layer observable fingerprinting as a separate evidence problem.

## Current implementation evidence

The audit found candidate differences including i2pr initial congestion window 16 and initial retransmission timeout 5 seconds, while documented/reference defaults differ. The 1730-byte advertised maximum payload appears aligned. Delayed ACK and resend behavior differ across documentation/implementations and must be measured, not inferred.

`StreamingConfig::balanced()` active values must be distinguished from hard ceilings. Do not encode a max-resend assertion without tracing constructor field mapping and observing behavior.

## Invariants

- No production Streaming constant changes.
- No raw application payloads in committed evidence.
- Exact pins and identical stimuli for comparisons.
- Timing uses bounded monotonic measurements/tolerance bands, not flaky one-shot verdicts.
- External-lane unavailability is unexecuted, not passed.
- Existing Streaming correctness/interop evidence is not reinterpreted as fingerprint equivalence.

## Scope

### In

Controlled hostile Destination fixture; SYN/options/flags and maximum payload; initial send burst/window; delayed ACK; RTO/retransmission schedule; observable resend/terminal behavior; choke/NACK/ACK response where supported; close/reset; loss/delay/reorder/duplicate/ACK-withholding stimuli; sanitized Java/i2pd/i2pr traces.

### Out

Production tuning, HTTP/TLS fingerprinting, tunnel path selection, global traffic-analysis simulation, or false exact-equality claims.

## Required work

### A. Observable fingerprint schema

Create machine-readable traces containing only packet direction, relative monotonic time bucket, flags/options, payload length, normalized sequence/ack relations, retransmission ordinal, advertised packet size/window/choke state, and terminal reason.

Normalize random sequence origins/identities. Exclude payload bytes.

### B. Hostile-Destination fixture

Controlled server can accept handshake, withhold/delay ACKs, ACK selected ranges, advertise constrained windows, reorder/duplicate responses, trigger orderly close/reset, and record normalized inbound behavior.

Prefer existing i2pr-testkit deterministic fault primitives; production crates must not depend on testkit.

### C. Exact reference runners

Source-lock:
- i2pr head under test;
- i2pd 2.61.0 commit 635b013a612ff47278ef02acf8580a28e10e26c5;
- Java I2P 2.13.0 commit 9134f808337b401e8e53c73734c81fab04280c9d.

Use identical scenario definitions; do not patch reference routers.

### D. Scenario matrix

At minimum: clean handshake/small payload; send-window saturation; delayed first ACK; ACK withheld through two retransmission opportunities; single loss; deterministic reorder; constrained advertised window; choke/un-choke where interoperable; orderly close; abrupt/reset close.

Repeat timing-sensitive scenarios and record distributions/tolerances.

### E. Differential report

Classify dimensions: common across all; i2pr matches Java; i2pr matches i2pd; references differ and i2pr matches neither; not reliably observable; harness limitation.

Identify high-confidence i2pr-only combinations for Plan 299 without prescribing fixes.

## Failure/cancellation/restart/contention

Every scenario has hard deadline/bounded trace size. Cancellation tears down child processes/tasks and emits no partial pass. Evidence ordering remains deterministic.

## Compatibility and migration

None; evidence-only.

## Required tests

- trace schema bounds/round-trip;
- normalization removes random identifiers but preserves behavior;
- seeded traces classify known differences;
- deterministic fault scripts;
- exact-pin runner checks;
- evidence scan for raw payload/identity leakage;
- missing reference binaries fail explicit runs.

## Exact verification commands

~~~text
cargo fmt --all --check
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-testkit --all-targets -- --test-threads=1
bash tests/integration/anonymity/run-streaming-fingerprint-preflight.sh
bash tests/integration/anonymity/run-streaming-fingerprint-i2pr.sh
bash tests/integration/anonymity/run-streaming-fingerprint-i2pd.sh
bash tests/integration/anonymity/run-streaming-fingerprint-java.sh
bash scripts/check-streaming-fingerprint-evidence.sh
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
~~~

## Documentation updates

Add evidence note describing hostile-Destination observability and explicitly state no production behavior/anonymity claim changed.

## Acceptance criteria

Plan 298 passes only if:
1. bounded sanitized schema/fixture committed;
2. exact three-family runners source-locked;
3. scenario matrix executes for all three or closes blocked/unexecuted;
4. timing variability represented honestly;
5. i2pr-only candidate dimensions mechanically listed;
6. no production Streaming tuning;
7. evidence contains no raw payload/private identity.

## Stop conditions

Stop if reference fixture requires patching the reference router, timing noise prevents stable classification after bounded repetitions, or collecting a dimension requires sensitive payload logging.

## Closure evidence required

Retain pins, scenarios, normalized traces/digests, classification matrix, repetition/tolerance rules, limitations, and commands.

## Handoff notes

Plan 299 must consume this observed matrix as primary authority, not source-code intuition.
