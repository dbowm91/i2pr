# Plan 162 — Milestone 8 SSU2 external-test lane isolation and CI restoration

Status: **next executable corrective pass**.

Registered: **2026-09-04**.

Triggered by Plan 161 direction-A implementation on current head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c`.

Depends on:

- Plans 153–160 explicitly passed;
- Plan 161 direction A (`i2pr initiator -> exact-pinned i2pd 2.61.0 responder`) genuinely proven over real loopback UDP;
- the Plan 161 transcript corrections and regenerated SSU2 vectors retained exactly as landed.

Temporarily blocks:

- further Plan 161 closure work until routine CI is restored.

After this plan passes, execution returns directly to **Plan 161**. Plan 162 is not a new Milestone 8 product stage.

## 1. Goal

Correct one test-lane integration defect without reopening SSU2 protocol design:

> the environment-dependent Plan 161 independent-i2pd integration test must remain compiled by ordinary workspace checks, remain fail-closed when explicitly invoked as external interop, but must not execute automatically in routine workspace CI where no external i2pd process/environment exists.

The expected closing classification is:

```text
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability
plan_161 = in-progress-direction-a-proven
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration

milestone8_ssu2_direction_a = passed-via-plan161
milestone8_final_acceptance = not-yet-closed
next_executable_plan = 161
next_product_layer = milestone8-ssu2-v2
```

Do **not** mark Plan 161 or Milestone 8 passed in this corrective.

## 2. Concrete defect and evidence

Plan 161 added:

```text
crates/i2pr-runtime/tests/ssu2_independent.rs
```

The test is intentionally fail-closed and requires external-lane variables including:

```text
I2PD_ROUTER_INFO
I2PD_SSU2_ENDPOINT
I2PR_SSU2_BIND
I2PR_SSU2_FLOODFILL
EVIDENCE_DIR
```

That behavior is correct for a dedicated interop lane.

However, the integration test currently participates in ordinary workspace execution. Routine CI on exact head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c`, run `33915994884`, failed in both quality jobs:

```text
Quality (ubuntu-latest) = failure
Quality (macos-latest)  = failure
Dependency policy       = success
MSRV (Ubuntu)           = success
```

Both quality failures are the same expected-environment failure:

```text
ssu2_independent_ipv4_interop ... FAILED
missing required env I2PD_ROUTER_INFO
```

The ordinary unit/integration suites preceding it were green. This is therefore a **lane-selection defect**, not current evidence of an SSU2 protocol regression.

On Linux, routine CI invokes:

```text
cargo test --locked --workspace -- --test-threads=1
```

On macOS, routine CI builds every test executable and then invokes each executable serially. Any fix must work for **both** execution forms without adding fragile OS-specific name filtering.

## 3. Required design: explicit ignored external test

Use Rust/libtest's normal explicit external-test gate.

The preferred implementation is to mark only the Plan 161 external test as ignored with a descriptive reason, for example:

```rust
#[tokio::test]
#[ignore = "Plan 161: requires exact-pinned external i2pd environment"]
async fn ssu2_independent_ipv4_interop() {
    ...
}
```

The exact attribute ordering may follow `rustfmt`/local convention.

### 3.1 Why `#[ignore]` is the required default

This gives the desired properties simultaneously:

- `cargo check --workspace --all-targets` still compiles the external test;
- ordinary `cargo test --workspace` discovers but does not execute it;
- the macOS direct-executable loop also sees the test as ignored and exits successfully;
- a dedicated external lane must opt in explicitly with `--ignored`;
- the existing fail-closed environment checks stay intact once the test actually starts.

Do **not** solve this by teaching ordinary CI to grep out one executable name. That would create two CI selection implementations and is easier to drift.

Do **not** use `|| true`, blanket `continue-on-error`, or a workspace-wide test exclusion.

A Cargo feature/`required-features` gate is acceptable only if a concrete toolchain/libtest constraint makes `#[ignore]` impossible. If that unexpected condition occurs, stop and document why before using the alternative; the external test must still compile in an explicit normal validation command.

## 4. Preserve explicit fail-closed external execution

Do not weaken these behaviors in `ssu2_independent.rs`:

```text
missing required external environment -> hard failure when explicitly executed
non-loopback bind/endpoint             -> hard failure
malformed/mismatched RouterInfo        -> hard failure
external authentication failure        -> hard failure
missing DeliveryStatus evidence        -> hard failure
resource baseline failure              -> hard failure
```

In particular, **do not** replace `env_value()`/`env_path()` with an early `return Ok(())` or equivalent when variables are absent. That would convert an accidentally misconfigured external acceptance lane into a synthetic pass.

After gating, the canonical explicit direction-A invocation becomes conceptually:

```text
I2PD_ROUTER_INFO=... \
I2PD_SSU2_ENDPOINT=127.0.0.1:<port> \
I2PR_SSU2_BIND=127.0.0.1:<port> \
I2PR_SSU2_FLOODFILL=<0-or-1> \
EVIDENCE_DIR=... \
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

The eventual Plan 161 external wrapper/workflow must use an explicit ignored-test invocation (or an equally explicit dedicated external command if Plan 161 later splits the driver into multiple tests).

## 5. Add a narrow lane-isolation regression contract

The correction must prove all three states below.

### 5.1 Routine invocation: discovered, not executed

With all Plan 161 external environment variables absent:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1
```

must exit `0` and report the external test as ignored rather than passed or failed.

The full ordinary workspace command must also pass:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
```

If `--all-targets` differs from the routine CI command on the current workflow, execute both the exact CI command and the repository floor command so the distinction is explicit.

### 5.2 Explicit external invocation without environment: fails closed

Clear all external variables and invoke the ignored test explicitly:

```text
I2PD_ROUTER_INFO
I2PD_SSU2_ENDPOINT
I2PR_SSU2_BIND
I2PR_SSU2_FLOODFILL
EVIDENCE_DIR
```

Then run the test with `--ignored --exact`.

The command must fail nonzero because required external state is absent. Record the bounded reason category (`missing required env ...`), not an environment dump.

This expected-negative command is evidence that the gate did not turn absence of an external peer into success.

### 5.3 Explicit external invocation with pinned i2pd: still passes

Re-run the already-proven direction-A trajectory against the exact mandatory reference:

```text
i2pd 2.61.0
commit 635b013a612ff47278ef02acf8580a28e10e26c5
```

using the same unprivileged loopback process seam and the new explicit ignored-test invocation.

Require the existing direction-A result to remain true:

- tokenless TokenRequest -> Retry -> SessionRequest -> SessionCreated -> SessionConfirmed;
- mutual authentication;
- one small DatabaseStore i2pr -> i2pd;
- one fragmented DatabaseStore i2pr -> i2pd;
- DeliveryStatus reply for each store back to i2pr;
- graceful session termination;
- active-session/resource baseline restored;
- only sanitized lengths/digests/counters written to evidence.

If this trajectory fails after adding only the test gate, stop. Do not reclassify the earlier Plan 161 protocol correction or edit protocol code inside Plan 162.

## 6. Routine CI requirements

The intended implementation should require **no special-case edit** to `.github/workflows/ci.yml` beyond changes strictly necessary to express a general invariant.

Current routine Linux and macOS test strategies should both pass naturally because libtest ignores the external test.

Forbidden CI fixes:

```text
exclude the whole i2pr-runtime crate
exclude all integration tests
match/skip ssu2_independent by executable filename in shell
append `|| true`
set continue-on-error for workspace tests
inject fake I2PD_* values
launch an unpinned/mock peer in routine CI
```

The dedicated Plan 161 external workflow remains the owner of real i2pd provisioning and external execution.

On the exact Plan 162 closing commit, routine CI must pass all jobs, including:

```text
Dependency policy
MSRV (Ubuntu)
Quality (ubuntu-latest)
Quality (macos-latest)
```

Record the run ID and exact head SHA in `plans/162-status.md` before marking Plan 162 passed.

## 7. Plan 161 evidence/status correction

Update `plans/161-status.md` as part of this pass, preserving all valid direction-A evidence.

It must clearly state:

```text
plan_161 = in-progress-direction-a-proven
plan_161_current_blocker = routine-ci-external-test-lane-selection
plan_162 = active corrective
```

During Plan 162 execution, do not rewrite direction A as unproven. The protocol trajectory passed; only its integration into the ordinary test lane is defective.

Update the recorded canonical direction-A test command to include the explicit `--ignored` opt-in after Plan 162 lands.

After Plan 162 closes, `plans/161-status.md` should state that the CI-lane blocker is cleared and the remaining Plan 161 work is still:

- direction B (`i2pd initiator -> i2pr responder`);
- token/Retry matrix beyond the already-proven tokenless path where externally accessible;
- malformed/spoof/resource rows;
- Java secondary-lane pass or exact nonblocking blocker;
- final fail-closed SSU2 evidence ledger/checker;
- manual SSU2 external workflow on exact closing head;
- final support/conformance closure only after all mandatory Plan 161 criteria pass.

## 8. Planning/authority documents to update

Register Plan 162 as the **temporary next executable plan** and make the return path explicit in the minimum necessary authority surfaces:

```text
plans/162-status.md
plans/161-status.md
plans/154-status.md
plans/README.md
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
```

Use this authority shape while Plan 162 is active:

```text
plan_161 = in-progress-direction-a-proven-blocked-by-plan162
plan_162 = active-m8-ssu2-external-test-lane-isolation-and-ci-restoration
next_executable_plan = 162
resume_after_plan162 = 161
milestone8_final_acceptance = not-yet-closed
```

After Plan 162 passes, roll those same surfaces to:

```text
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_161 = in-progress-direction-a-proven
next_executable_plan = 161
milestone8_final_acceptance = not-yet-closed
```

Do not renumber or supersede Plan 161. Plan 162 is an inserted corrective, not a replacement final-interop plan.

## 9. Hard scope boundary

Expected behavioral code change:

```text
crates/i2pr-runtime/tests/ssu2_independent.rs
  -> test execution metadata/gating only
```

Expected production-code diff:

```text
crates/*/src/ = no changes
```

Do not modify in this pass:

- SSU2 handshake/KDF/header protection/data semantics;
- regenerated Plan 161 vectors except if a pure manifest path/reference error is discovered;
- UDP runtime behavior;
- TransportManager behavior;
- RouterInfo parsing/signing rules;
- token/replay/path/relay policy;
- daemon SSU2 activation/publication policy;
- i2pd source;
- external i2pd pin;
- `specs/support.toml` advertised/support claims;
- `Cargo.lock` or dependencies.

If a protocol/runtime change becomes necessary, stop Plan 162 and record a separate narrow defect. Do not hide protocol work inside a CI corrective.

## 10. Required execution sequence

1. Record starting SHA (`4a38e295...` unless main advanced before execution).
2. Reproduce/read routine CI run `33915994884` and verify the failure remains solely external-test environment selection.
3. Add the explicit ignored-test gate to `ssu2_independent_ipv4_interop`.
4. Run the focused routine invocation with no external environment and confirm **ignored + exit 0**.
5. Run the explicit `--ignored --exact` invocation with the environment cleared and confirm **nonzero fail-closed**.
6. Run local format/check/focused SSU2 floors.
7. Re-run direction A against exact-pinned i2pd using the explicit ignored-test invocation; require the same authenticated I2NP evidence.
8. Update Plan 161/162 authority records and executor-facing documentation.
9. Verify `git diff <start> -- 'crates/*/src' Cargo.lock` is empty.
10. Run the complete workspace floor.
11. Commit the Plan 162 implementation/closure candidate.
12. Require routine GitHub CI success on the exact closing SHA on both Ubuntu and macOS.
13. Record exact commands, results, external pin, closing SHA, and CI run ID in `plans/162-status.md`.
14. Only after 1–13 pass, mark Plan 162 passed and restore `next_executable_plan = 161`.

## 11. Validation floor

At minimum run:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets

# Ordinary no-peer behavior: must be ignored, exit 0.
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1

# Explicit external behavior with environment absent: must fail nonzero.
# Run with all I2PD_*/I2PR_SSU2_*/EVIDENCE_DIR variables explicitly unset.
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1

# Explicit external behavior with exact-pinned i2pd provisioned: must pass.
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1

cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-runtime --lib
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-fixture-manifest.sh

cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

For the expected-negative explicit invocation, the executor must verify the command fails **for missing required external configuration**, not because of compilation, panic unrelated to configuration, or a test-discovery mistake.

## 12. Acceptance criteria

Plan 162 closes only when all are true:

1. The Plan 161 external integration test remains compiled by `cargo check --workspace --all-targets`.
2. Ordinary invocation with no external environment exits 0 and reports the test ignored, not passed.
3. Routine Linux workspace tests no longer execute the external interop trajectory.
4. The macOS direct-test-executable loop no longer executes the external interop trajectory.
5. The external test requires explicit `--ignored` opt-in.
6. Explicit opt-in with required variables absent fails nonzero with the expected bounded missing-environment reason.
7. No environment-absent early-success path is added.
8. No routine-CI filename filtering, `|| true`, `continue-on-error`, fake peer configuration, or broad test exclusion is introduced.
9. Exact-pinned i2pd remains `635b013a612ff47278ef02acf8580a28e10e26c5` and unmodified.
10. Plan 161 direction A passes again through the explicit external invocation with mutual authentication, small + fragmented DatabaseStore delivery, DeliveryStatus return traffic, graceful termination, and resource baseline.
11. No production source under `crates/*/src/` changes.
12. No SSU2 wire/vector semantic change is made by this pass.
13. `Cargo.lock` and dependency versions do not change.
14. Full local workspace/SSU2 quality floor passes.
15. Routine GitHub CI passes on the exact Plan 162 closing SHA for Ubuntu and macOS quality jobs plus MSRV/dependency policy.
16. `plans/161-status.md` retains valid direction-A evidence and records the corrected explicit invocation.
17. `plans/162-status.md` records exact closing SHA, commands/results, i2pd pin, and hosted CI run ID.
18. Authority docs identify Plan 162 only as the temporary corrective and restore Plan 161 as next executable after closure.
19. Milestone 8 remains open; direction B and the remaining Plan 161 acceptance matrix are not inferred from direction A.
20. No public I2P, broad router interoperability, IPv6 interop, PQ v3/v4, or Milestone 6 interoperability claim is added.

## 13. Stop conditions

Stop and create a narrower follow-up rather than weakening this plan if:

- ordinary CI still executes the ignored test because of a toolchain/libtest behavior not accounted for here;
- the explicit ignored test cannot be selected reliably on Rust 1.95.0/MSRV-compatible repository tooling;
- the direction-A external trajectory stops passing after the gate-only change;
- any SSU2 protocol/runtime source edit appears necessary;
- the failing hosted job contains an additional substantive regression beyond the absent external environment;
- the exact-pinned i2pd acquisition/build seam can no longer be reproduced for a reason that requires changing Plan 161 architecture.

Do not convert these conditions into broader CI exceptions.

## 14. Handoff

Execute Plan **162** now.

When it passes, return immediately to Plan **161** and continue the remaining independent interoperability/final-closure work. Do not begin Milestone 9 planning or claim Milestone 8 closure until Plan 161 itself satisfies all of its mandatory acceptance criteria.