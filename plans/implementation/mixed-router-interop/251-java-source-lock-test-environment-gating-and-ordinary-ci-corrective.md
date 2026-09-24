# Plan 251 — Java source-lock test environment gating and ordinary-CI corrective

Status at registration:
**registered-ready-java-source-lock-test-environment-gating-and-ordinary-ci-corrective**

Subsystem: retained M6 Java compatibility-lane test infrastructure.
This does not reopen M6 experimental progression or resume Java live Streaming work.

Baseline: `958c06171a6d60dc3d1866ed8b7d93937d001d6f`

## Objective

Restore a trustworthy ordinary workspace/CI floor by making exact-pinned Java
**source-tree-dependent** tests explicitly environment-gated under the repository's
existing policy.

Keep the source locks. Ordinary CI must not require an undeclared Java source checkout.
Explicit source-lock execution must fail when its environment is missing or at the wrong
revision.

## Evidence

GitHub Actions run `36038288580` on Plan 249 fails both Ubuntu and macOS in
`i2pr-daemon --test java_tunnel_external`. The observed source-read failures are:

- `p246_connection_event_reenters_scheduler_chooser`;
- `p246_default_ack_delay_is_500_on_frozen_helper`;
- `p246_packet_handler_sets_deadline_before_event`;
- `p246_received_reschedule_calls_connection_timer`;
- `p246_scheduler_precedence_is_source_locked`;
- `p246_timer_wrapper_delegates_to_connection_event`;
- `p246_transition_add_event_uses_fresh_wrapper`;
- `p247_no_ack_delay_override`.

They fail with missing exact Java source files such as `Connection.java`,
`ConnectionOptions.java`, `ConnectionPacketHandler.java`, `SchedulerImpl.java`,
`SchedulerChooser.java`, and `SimpleTimer2.java`.

Run `36027709234` on the preceding `9f605ae...` commit fails the same way, proving this is
pre-existing Plan-246/247 environment coupling rather than a Plan 249 regression.

MSRV and dependency-policy jobs are green.

## Invariants

- no patching, vendoring, copying, or modifying Java I2P;
- exact Java I2P 2.13.0 pin remains
  `9134f808337b401e8e53c73734c81fab04280c9d`;
- no early-success when source files are absent;
- do not ignore the whole `java_tunnel_external` binary;
- gate only tests whose assertion requires the external source tree;
- pure Rust parser/classifier/harness tests remain ordinary;
- live external topology tests keep their own explicit ignored/environment semantics;
- no production/wire/runtime/M11 changes.

## Work package A — inventory source-tree dependencies

Audit `crates/i2pr-daemon/tests/java_tunnel_external.rs` for direct or helper-mediated Java
source reads.

Classify each relevant test as:

1. self-contained Rust;
2. exact-pinned Java source-lock;
3. live Java topology.

Only category 2 is owned here. Record the final category-2 list in closure.

## Work package B — one fail-closed source-root gate

Reuse the existing canonical Java source-root/pin helper if present. Otherwise add one
narrow test-only helper and one documented environment variable.

Before source assertions:

- require the configured source root;
- prove expected source layout;
- verify exact commit `9134f808337b401e8e53c73734c81fab04280c9d`;
- fail clearly on missing/mismatched environment.

Do not fall back to guessed sibling checkouts in ordinary CI.

## Work package C — ignore-gate only category-2 tests

Apply repository-consistent explicit ignore markers, e.g.:

```text
#[ignore = "requires exact-pinned Java I2P 2.13.0 source tree"]
```

Ordinary:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
```

must skip source-lock tests while still running all self-contained tests.

## Work package D — explicit source-lock runner

Add a small explicit runner, preferably:

`scripts/run-java-source-lock-tests.sh`

It must invoke each category-2 test narrowly with `--ignored --exact` (or equivalent) and
must not use a broad `--ignored` invocation that could launch `streaming_through_java`.

Requirements:

- missing source env -> nonzero;
- wrong pin -> nonzero;
- correct exact-pinned tree -> execute every listed source lock;
- no `|| true`, `continue-on-error`, filename filtering, or silent skip.

If the exact Java tree is unavailable on the implementation host, closure may record that
explicit source lane as unexecuted; the source assertions themselves must remain unchanged.

## Work package E — small static evidence guard

Extend the existing M6 evidence checker or add a narrow guard proving:

- source-dependent tests are ignore-gated;
- explicit runner and gated test list match;
- Java pin unchanged;
- ordinary CI does not inject a fake source path;
- `streaming_through_java` is absent from the source-lock runner;
- assertion bodies were not replaced by unconditional success.

Avoid another large harness.

## Verification

Required rows:

1. ordinary `java_tunnel_external` skips category-2 source locks without missing-file
   failures;
2. pure p246/p247 tests still run;
3. explicit runner with missing env fails;
4. wrong pin fails before source assertions;
5. runner excludes `streaming_through_java`;
6. exact pin unchanged;
7. Ubuntu ordinary workspace CI green;
8. macOS ordinary workspace CI green;
9. MSRV green;
10. dependency policy green.

Commands:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

Current-SHA GitHub Actions Ubuntu/macOS results are required closure evidence.

## Out of scope

Java live Streaming continuation; Plan 247 live polling; Java source/jar changes; reference
pin changes; M11 transit semantics; daemon transit composition; public I2P.

## Acceptance criteria

Plan 251 closes only when all external-source-dependent Java source-lock tests are
explicitly gated, ordinary workspace tests no longer require external Java source,
explicit source-lock execution remains fail-closed, exact pin is unchanged, pure/live test
semantics are not weakened, Ubuntu/macOS quality jobs are green on the implementation SHA,
MSRV/dependency policy are green, and no production code changes land.

## Stop conditions

Stop rather than broaden scope if ordinary CI would require deleting assertions, vendoring
Java, changing the pin, or excluding the whole integration-test binary. Record unrelated
new CI failures separately.

## Handoff for smaller models

Do not touch M11 code. First reproduce the no-source failure, identify the exact
source-reading tests, gate only those, add the narrow explicit runner, prove missing env
fails there, then run ordinary workspace/CI.
