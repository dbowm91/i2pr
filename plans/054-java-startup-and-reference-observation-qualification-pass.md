# Plan 054: Java startup and reference-observation qualification pass

## Status and dependencies

- Plan type: controlled external qualification and source-inspection pass.
- Starting branch: `main` after Plan 053.
- Authoring baseline before Plan 053 implementation: `e6d776771308aed0bd4b942da5012bafb182f5b9`.
- Hard dependency: Plan 053 must be complete. The canonical lane must already produce a verified diagnostic Plan 052 bundle with immutable run identity and observation-v2 records.
- This plan owns:
  - the Java I2P intermittent-startup investigation and stable per-direction state model;
  - source-locking receiver-side authenticated-frame and I2NP-decode observations for Java I2P and i2pd;
  - adapter integration of those observations.
- This plan does not own direct reference-initiated dial helpers or the final two-run Milestone 3 certificate.
- Milestone 3 remains open.
- NTCP2 remains experimental and non-advertised.

## Objective

Close two blockers that prevent the Plan 052 observation predicate from being meaningful:

1. Java I2P 2.12.0 must start deterministically in the rootless execution environment from a per-direction state directory without reusing mutable state across scenarios.
2. Both pinned references must expose source-locked, non-handshake-only evidence for:
   - authenticated NTCP2 frame decryption; and
   - successful decoding/dispatch of the bounded DeliveryStatus I2NP message.

At the end of this plan, the two i2pr-initiated directions should be capable of reaching the full observation-v2 predicate:

```text
i2pr-to-java-ipv4
i2pr-to-i2pd-ipv4
```

The reference-initiated directions may remain typed blockers pending Plan 055.

## Fixed references

- Java I2P 2.12.0 at revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`.
- i2pd 2.60.0 at revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Artifact and installed-tree digests must continue to match `tests/integration/ntcp2/references.lock.toml` and the verified cache manifest.

Do not silently update reference versions during this pass.

## Non-negotiable constraints

1. Do not share one mutable Java data directory across directions.
2. Do not treat a readable `/dev/urandom` as proof that Java startup is correct.
3. Do not preselect entropy as the cause before the matrix isolates it.
4. Do not patch reference transport behavior merely to emit a success marker.
5. Handshake markers may never satisfy receiver-side data acceptance.
6. A receiver marker must be tied to an exact source path, exact symbol, exact pinned revision, and bounded semantic meaning.
7. Raw reference logs remain local diagnostics and must be sanitized before evidence export.
8. A test-only observer may observe existing behavior but must not cause the transport handshake or data decode to succeed.
9. Missing or ambiguous receiver markers produce a typed rejection, never a pass.
10. Java state templates may be prepared online, but authoritative scenario execution remains offline.

## Part I: controlled Java startup matrix

### J1. Freeze the experiment contract

Use `java_startup_probe.py` as the single experiment driver. Extend it only where required to emit the complete sanitized matrix record.

The matrix variables are:

| Variable | Values |
| --- | --- |
| namespace | `outer`, `rootless` |
| data state | `empty`, `seeded-clone` |
| launcher | `runplain`, `wrapper` |
| sequence | `single`, `generate-live` |

This is 16 cells. Run exactly three independent attempts per cell initially, for 48 starts. Each attempt uses a new data directory.

Do not reuse a failed attempt directory in a later cell.

### J2. Define `seeded-clone` precisely

The seeded template must be created once during the preparation phase from the pinned Java artifact and then treated as immutable.

Required template properties:

- Java router completed one clean startup to the defined readiness boundary;
- Java router completed one clean shutdown;
- no Java process remains;
- file ownership is the guest execution user;
- private files retain required restrictive modes;
- the template has a recorded deterministic tree digest;
- the template is stored outside all per-direction run roots;
- each attempt copies the template into a fresh directory;
- the template itself is never launched again after freezing.

Recommended layout:

```text
target/interop/java-template/
  template-data/
  template-manifest.json
  template-tree.sha256
```

Each attempt uses:

```text
target/interop/java-startup-matrix/<matrix-run-id>/<cell>/<attempt>/data/
```

### J3. Readiness and shutdown boundaries

A Java attempt is `started` only when all required events are observed:

- process exists;
- router startup marker appears;
- NTCP2 transport is configured;
- required NTCP2 listener binds to the expected synthetic endpoint or reaches the reference-control readiness state used by the harness;
- process remains alive for a fixed stabilization interval.

Use a bounded stabilization interval such as 5 seconds after readiness. Do not use arbitrary long sleeps as the readiness mechanism.

A clean attempt also requires:

- graceful shutdown command issued;
- process exits within the configured timeout;
- no child Java process remains;
- no stale lock file blocks a fresh clone;
- data tree remains readable and ownership-correct.

### J4. Record bounded environment probes

Before each attempt record only sanitized facts:

- namespace mode;
- launcher mode;
- data-state mode;
- sequence mode;
- `/dev/random` and `/dev/urandom` file type, ownership class, and read permission result;
- one bounded `getrandom()` success/failure and latency bucket;
- current kernel entropy availability bucket, not raw values when policy forbids it;
- seed-file presence, size bucket, and digest;
- key-backup presence and file-count bucket;
- process count before and after;
- exit code and typed failure stage.

Never retain random bytes, private keys, seed contents, or full filesystem paths in exported evidence.

Example sanitized entropy probe:

```json
{
  "getrandom_result": "success",
  "latency_bucket_ms": "0-10",
  "urandom_readable": true,
  "random_readable": true,
  "seed_file_state": "present-nonempty",
  "seed_file_sha256": "<digest>"
}
```

### J5. Split startup failure stages

The probe must classify at least:

```text
java-process-spawn-failed
java-wrapper-bootstrap-failed
java-router-start-marker-missing
java-random-source-shutdown
java-key-generation-failed
java-ntcp2-configuration-failed
java-listener-readiness-timeout
java-premature-process-exit
java-graceful-shutdown-timeout
java-residual-process
java-state-permission-invalid
java-state-lock-invalid
```

Use exact known Java exception classes/messages only as internal matching inputs. Export the bounded code and a sanitized excerpt hash, not the full stack trace.

### J6. Analyze the matrix without forcing a preferred answer

The result document must identify which variables correlate with failure.

Examples:

- Failure only for `empty + generate-live` indicates state initialization or generation/live sequencing.
- Failure only for `wrapper` indicates wrapper lifecycle or restart behavior.
- Failure only inside `rootless` with both data states and launchers indicates namespace/environment interaction.
- Failure disappears for `seeded-clone` in all rootless cells, while empty state remains unstable, supporting a per-run cloned-template mitigation.

Do not claim entropy causation solely because `FortunaRandomSource` appears in a stack trace.

### J7. Prove the selected Java state model

After selecting a supported model, run a qualification sequence:

- 10 consecutive rootless starts from 10 independent cloned directories;
- alternate i2pr-to-Java generation/live sequencing as the actual harness will use;
- clean shutdown after each;
- no failed start;
- no residual process;
- no mutation to the frozen template;
- identical template digest before and after.

If the selected model cannot pass 10 consecutive starts, Plan 054 remains open.

## Part II: source-lock receiver observations

### O1. Replace Markdown-only assertions with a machine-readable catalog

Create a locked catalog such as:

```text
tests/integration/ntcp2/reference-observation-catalog.toml
```

Required top-level fields:

```toml
schema = "i2pr-reference-observation-catalog-v1"
revision = 1
```

Each observation entry must include:

```toml
reference = "java_i2p"
reference_version = "2.12.0"
reference_revision = "<40-char SHA>"
semantic_level = "frame_authenticated_and_decrypted"
source_path = "<exact path>"
symbol = "<exact class/method/function>"
marker_kind = "structured-log" # or typed callback/control observer
marker = "<exact bounded marker>"
sanitization_rule = "<named sanitizer>"
minimum_count = 1
```

The Markdown catalog becomes explanatory documentation generated from or checked against the TOML catalog. It must not be the executable source of truth.

### O2. Source-inspection procedure

For each pinned reference:

1. check out the exact pinned revision;
2. verify revision and source-tree digest;
3. trace the receive path from authenticated NTCP2 frame input through AEAD verification, block parsing, I2NP message decoding, and dispatch;
4. identify existing observable events or supported callbacks;
5. record exact path, symbol, and semantic boundary;
6. add positive and negative control tests;
7. reject any marker that can occur before the claimed semantic boundary.

Required semantic distinction:

```text
SessionConfirmed / connection established
    != authenticated data frame decrypted
    != I2NP message decoded and accepted
```

### O3. Preferred observer hierarchy

Use the first viable method in this order:

1. Existing structured event or counter already emitted by the pinned reference.
2. Existing debug-level event enabled by configuration only.
3. Existing supported callback, message-history facility, or control surface observed by a test-only sidecar/helper.
4. Test-only observer linked against unmodified pinned libraries that subscribes to an existing internal callback.

Do not modify the reference cryptographic or transport state machine to emit a success marker.

If no non-invasive observer exists, stop with:

```text
reference-receiver-observation-not-available-with-unmodified-reference
```

Do not create an instrumented reference and silently call it the pinned reference.

### O4. Java observation requirements

Source-inspect likely receive-path areas, but do not assume names until verified:

- NTCP2 connection receive/decrypt path;
- NTCP2 reader block parser;
- I2NP message creation/dispatch path;
- message history or event-log integration.

The final Java entries must prove:

- at least one authenticated data frame was successfully decrypted after handshake;
- the bounded DeliveryStatus message was decoded and handed to the router's I2NP handling path;
- the marker is correlated to the current run, preferably through the fixed message ID or bounded nonce.

### O5. i2pd observation requirements

Source-inspect the exact pinned revision around:

- NTCP2 session receive handler;
- AEAD data-phase decrypt;
- block parser;
- I2NP message handler/dispatch;
- debug log statements and counters.

The final i2pd entries must prove the same two semantic boundaries as Java.

### O6. Correlation requirements

A generic message count is insufficient when other startup traffic can occur. Correlate the bounded test message using one of:

- fixed DeliveryStatus message ID unique to the run;
- bounded test nonce encoded in an allowed field;
- run-local sequence number tied to both sender and receiver records.

The correlation value must be non-secret and safe to export.

### O7. Observation control experiments

For each reference marker pair, run all controls:

1. **Positive:** valid handshake plus valid bounded data message; both receiver levels observed exactly as expected.
2. **Handshake-only:** valid handshake, no data message; authentication may be observed, data levels must remain not-observed.
3. **Malformed encrypted frame:** authentication succeeds, frame authentication/decrypt must not be reported successful.
4. **Valid frame with invalid/unsupported I2NP encoding:** frame decrypt may be observed, I2NP decode must remain not-observed and carry a typed decode rejection.
5. **Wrong correlation value:** unrelated message/event must not satisfy the current run.
6. **No reference process:** no marker may be synthesized from stale logs.
7. **Repeated run:** counters and log cursors reset so prior events do not satisfy the new run.

### O8. Adapter integration

Update Java and i2pd adapters to return typed observation-v2 records rather than a single string such as `authenticated`.

Recommended interface:

```python
def collect_observation(
    *,
    role: str,
    run_id: str,
    correlation: dict[str, str],
    log_cursor: LogCursor,
) -> dict[str, object]:
    ...
```

The adapter must:

- start observing from a run-specific cursor;
- exact-match source-locked markers;
- count only post-cursor events;
- sanitize details;
- finalize the observation digest;
- never infer data receipt solely from i2pr sender counters.

## Part III: integrate Java state and observations into the lane

### I1. Java template preparation is a separate phase

Add a preparation command that creates/verifies the frozen template. The authoritative execution phase may only clone and use it; it may not download, install, or reseed from the network.

### I2. Per-direction Java clone

For each Java direction:

1. verify frozen template digest;
2. clone to a fresh run directory;
3. verify ownership and restrictive modes;
4. launch from the clone;
5. collect observations using a fresh cursor;
6. shut down and clean the clone;
7. verify template digest unchanged.

### I3. Plan 052 result semantics

The two i2pr-initiated directions may be `passed` only if:

- run identity cross-checks;
- both peers authenticate;
- i2pr observes frame emission;
- reference observes frame authentication/decryption;
- reference observes the correlated I2NP message decode/dispatch;
- cleanup is clean;
- all artifact classes exist and bundle verifies.

The reference-initiated directions remain typed blockers until Plan 055; their absence must not prevent creation of a diagnostic-complete bundle, but it prevents certificate status.

## Tests required

### Java probe tests

- every matrix selector validates;
- each attempt gets a unique directory;
- template cannot be launched directly after freeze;
- template mutation is detected;
- private-file mode regression is detected;
- exception-to-stage classification is bounded;
- raw stack traces are absent from sanitized records;
- process residual detection works;
- matrix aggregation reports cell counts correctly.

### Catalog tests

- exact pinned revision required;
- unknown source path rejected;
- empty symbol rejected;
- handshake marker cannot claim data-level semantics;
- duplicate semantic entry rejected;
- unrecognized sanitizer rejected;
- Markdown and TOML catalog drift detected.

### Observation tests

- positive markers satisfy receiver predicate;
- handshake-only does not;
- malformed frame does not;
- invalid I2NP does not satisfy decode;
- stale log marker does not count;
- wrong correlation does not count;
- observation digest changes when any level changes;
- raw paths and payload words are rejected by sanitizer.

### Integration tests

- fake Java seeded-clone path survives generate/live sequence;
- Java template digest unchanged after run;
- i2pr-to-Java cannot pass without receiver markers;
- i2pr-to-i2pd cannot pass without receiver markers;
- full positive fixture passes the Plan 052 observation predicate;
- diagnostic bundle remains valid when reference-initiated directions are typed blockers.

## Suggested commit sequence for a smaller model

1. `interop: expand Java startup probe matrix records`
2. `interop: add immutable Java seeded-template lifecycle`
3. `tests: qualify Java startup matrix classification`
4. `interop: add machine-readable reference observation catalog`
5. `interop: source-lock Java receiver observation markers`
6. `interop: source-lock i2pd receiver observation markers`
7. `interop: collect observation-v2 records from reference adapters`
8. `interop: wire cloned Java state into mixed-router runs`
9. `tests: close Plan 054 control experiment matrix`
10. `docs: record Plan 054 qualification results`

Do not combine Java root-cause work and both reference observation implementations in one large commit.

## Smaller-model execution guidance

### Work empirically

Run the matrix before selecting a mitigation. A stack trace mentioning a random source is evidence of failure location, not proof of environmental entropy failure.

### Keep attempts isolated

Incorrect:

```python
for attempt in range(10):
    launch(reference_data_dir)
```

Correct:

```python
for attempt in range(10):
    data_dir = clone_frozen_template(attempt_root / str(attempt))
    launch(data_dir)
```

### Do not use sleeps as observation proof

Incorrect:

```python
time.sleep(10)
return "authenticated"
```

Correct:

```python
wait_for_exact_marker(cursor, catalog.authentication_marker, timeout)
wait_for_correlated_marker(cursor, catalog.i2np_decode_marker, nonce, timeout)
```

### Separate diagnostics from evidence

Raw logs may be retained only under `raw-local` mode outside the export root. Sanitized evidence must contain bounded codes, counts, digests, and short redacted details.

### Stop conditions

Stop and write a typed status instead of improvising when:

- the pinned source does not contain the documented symbol;
- no non-invasive observation boundary exists;
- a proposed marker can occur before frame authentication or I2NP decode;
- stable Java startup requires sharing a mutable data directory;
- the template changes during an attempt;
- reference source modification appears necessary to manufacture a marker;
- the host cannot run the 48-cell matrix reliably.

If the current host is resource-constrained, run the matrix in the already qualified owned Multipass guest or the documented dedicated Ubuntu fallback. Do not weaken isolation to make the test cheaper.

## Explicit acceptance criteria

Plan 054 is complete only when all of the following are true:

1. All 16 Java matrix cells execute with three isolated attempts each, or a typed environment blocker records why the complete matrix could not run.
2. The matrix result identifies the failure-correlated variables without unsupported causal claims.
3. One supported Java per-direction state model is selected and documented.
4. The selected model passes 10 consecutive rootless cloned-state starts with clean shutdown.
5. The frozen Java template digest is unchanged across qualification.
6. No mutable Java data directory is shared across directions.
7. A machine-readable observation catalog exists and is revision-locked.
8. Java authenticated-frame and I2NP-decode observations are tied to exact source paths and symbols.
9. i2pd authenticated-frame and I2NP-decode observations are tied to exact source paths and symbols.
10. Positive and all six negative/control experiments pass for both references.
11. Handshake-only markers cannot satisfy the data phase.
12. Adapters emit finalized observation-v2 records using run-specific cursors and correlation.
13. `i2pr-to-java-ipv4` reaches the full Plan 052 predicate in at least one qualification run, or a precise source/environment blocker remains.
14. `i2pr-to-i2pd-ipv4` reaches the full Plan 052 predicate in at least one qualification run, or a precise source/environment blocker remains.
15. A verified diagnostic bundle contains the resulting records.
16. All Python, Rust, static-boundary, and documentation checks pass.
17. `plans/054-status.md` records matrix results, selected model, exact markers, exact revisions, tests, and remaining blockers.
18. No reference-initiated direction or Milestone 3 closure is claimed by this plan.

## Required handoff artifacts

- Java matrix summary JSON and sanitized per-cell records.
- Immutable Java template manifest and digest record, not the private template contents.
- Machine-readable observation catalog.
- Updated explanatory observation catalog documentation.
- Adapter implementations and control tests.
- One Plan 052 diagnostic bundle showing current direction outcomes.
- `plans/054-status.md`.
