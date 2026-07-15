# Plan 043: Ubuntu build-system interoperability gates

## Objective

Promote the repaired and proven Plan 038/039 interoperability apparatus into an explicit Ubuntu build-system lane with ordered preparation, reference-control, i2pr interoperability, adversarial, evidence, and cleanup gates.

The lane must be reproducible from a clean checkout, repeatable from verified offline caches, fail closed on missing or placeholder evidence, and leave no privileged or secret-bearing state on the runner.

This plan does not make privileged interoperability part of every ordinary pull-request build. The initial lane remains manual and optionally scheduled until repeated successful evidence establishes stability and runtime cost. Lightweight static, unit, deterministic, and manifest checks remain in ordinary CI.

## Prerequisites

- Plan 040 executable apparatus passes on Ubuntu 24.04 amd64.
- Plan 041 reference-only private crosscheck passes.
- Plan 042 passes all four primary IPv4 i2pr/reference direction gates.
- Evidence finalization writes sanitized records outside secret-bearing run roots.
- Cleanup verification is independently executable and returns nonzero on residual state.

## Build-system design principles

1. **Preparation and execution are separate trust domains.** Network-enabled source/dependency preparation finishes before any router execution.
2. **Execution is fail-closed and offline.** Router jobs use only verified caches and namespace-local synthetic links.
3. **Every gate consumes explicit artifacts.** Do not discover arbitrary cache directories recursively.
4. **Protocol success never overrides cleanup failure.** Cleanup verification is a required terminal gate.
5. **Evidence is typed and sanitized.** Raw logs, RouterInfo files, identities, keys, packet captures, and absolute private paths are never uploaded.
6. **Ordinary CI remains unprivileged.** Privileged namespace tests are isolated in the dedicated Ubuntu lane.
7. **Support claims follow evidence.** Workflow existence or green scaffolding jobs do not alter `docs/protocol-support.md`.

## Deliverable 1: Workflow structure

Refactor `.github/workflows/ntcp2-interop-ubuntu.yml` into explicit jobs or explicit ordered phases with separate status visibility.

Recommended jobs:

```text
contract
reference-build
reference-offline-reuse
environment-smoke
reference-crosscheck-ipv4
i2pr-handshake-smoke-ipv4
full-matrix
evidence-validation
cleanup-verification
```

Use `needs` to preserve the dependency chain. A later job must not run if its prerequisite evidence is absent or invalid, except cleanup, which must run with `if: always()`.

If GitHub-hosted runners cannot transfer required namespace-capable state across jobs, combine privileged execution into one job but preserve explicit named steps and per-gate evidence files. Reference build artifacts may be transferred between jobs only through verified workflow artifacts with digest validation.

## Deliverable 2: Trigger and concurrency policy

Initially support:

- `workflow_dispatch` with explicit profile selection;
- optional scheduled execution after the manual lane is stable;
- no automatic public-network or fork execution;
- no execution on arbitrary untrusted pull-request code with elevated privileges.

Add workflow concurrency:

```text
one NTCP2 interop run per branch/ref
cancel-in-progress: false
```

Do not cancel an active privileged run without allowing cleanup. If cancellation can occur, add a cancellation-safe cleanup trap and a subsequent recovery workflow or host cleanup check.

Workflow inputs may select:

- `environment-smoke`;
- `reference-crosscheck-ipv4`;
- `handshake-smoke`;
- `full`.

Inputs must not permit arbitrary shell fragments, source URLs, revisions, endpoints, network IDs, or paths.

## Deliverable 3: Exact runner and toolchain contract

Use an explicit Ubuntu 24.04 amd64 runner contract. Record the exact image metadata available from the runner environment into build evidence.

Install the repository-pinned Rust toolchain. Use `--locked` for Cargo builds. Record:

- Rust and Cargo versions;
- Ubuntu release;
- kernel;
- architecture;
- Java and Ant;
- compiler and CMake;
- Python;
- iproute2 and nftables;
- workflow run ID and attempt as non-secret metadata.

Pin third-party workflow actions to reviewed commit SHAs where repository policy permits. At minimum, use fixed major versions and document the remaining supply-chain exposure.

Do not depend on `ubuntu-latest`.

## Deliverable 4: Contract gate

The contract gate runs without starting routers and verifies:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

It must also verify:

- full 40-character reference pins;
- no placeholder evidence can validate as passed;
- workflow profiles map to distinct runner profiles;
- reference crosscheck does not alias i2pr scenarios;
- the launcher is built from the current checkout;
- no committed generated key, identity, RouterInfo, raw log, packet capture, or reference binary exists.

## Deliverable 5: Reference preparation gate

The network-enabled preparation gate runs:

```bash
sudo -E bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/build-references.sh --force-rebuild
```

It must:

- fetch only locked repositories and verified dependencies;
- record full source revisions and tool versions;
- execute reference unit tests where available;
- verify release/revision output;
- compute artifact and runtime-tree hashes;
- produce a canonical reference-build summary;
- verify no router process or namespace exists after preparation;
- archive only immutable runtime caches and sanitized build metadata.

Do not upload source trees, raw build logs containing private runner paths, or mutable router state unless separately reviewed and sanitized.

If caches are transferred to another job, create an artifact manifest containing every file path and SHA-256. Verify the artifact manifest before execution.

## Deliverable 6: Offline repeatability gate

The offline gate must prove that execution dependencies are complete.

Enforce offline behavior using more than a command-line flag where practical:

- restore verified reference caches;
- disable or block network access for the build/reuse process after restoration;
- run `build-references.sh --offline`;
- verify that no clone, fetch, curl, wget, package installation, or DNS attempt occurs;
- compare cache keys and tree hashes to the preparation output.

A cache miss or metadata mismatch is a hard failure, not permission to fetch.

## Deliverable 7: Environment-smoke gate

Run both references independently in the isolated topology:

```bash
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
```

Require evidence for:

- namespace creation and isolation verification;
- exact cache/tree validation;
- configuration rendering;
- process readiness;
- RouterInfo production;
- bounded stop;
- sanitized evidence persistence;
- deletion of raw state;
- zero residual namespace/process/interface state.

This gate must not claim authenticated inter-router connectivity.

## Deliverable 8: Reference-control gate

Run:

```bash
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
```

Require:

- exact Java and i2pd cache hashes;
- identical proven private network ID;
- strict RouterInfo validation and exchange;
- authenticated reference-to-reference NTCP2 observation;
- controlled direction evidence;
- clean shutdown and zero residual state.

The i2pr handshake gate must not run if the reference-control gate fails. This preserves a known-good control for diagnosing future failures.

## Deliverable 9: i2pr handshake-smoke gate

Build the current launcher:

```bash
cargo build --locked --package i2pr-interop
```

Then run:

```bash
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
```

The profile must require independent passing evidence for:

- i2pr dial to Java I2P;
- Java I2P dial to i2pr;
- i2pr dial to i2pd;
- i2pd dial to i2pr.

Each result must include:

- authenticated handshake completion;
- strict peer/network/static-key binding;
- selected I2NP smoke exchange in both relevant directions;
- bounded process and runtime counters;
- clean evidence finalization;
- zero residual state.

One passing direction must not mask another failed direction.

## Deliverable 10: Full-matrix gate

After handshake smoke passes, run:

```bash
sudo -E bash scripts/interop/run-matrix.sh --profile full
```

Include:

- IPv4 positive cases;
- IPv6 positive cases or explicit validated skips;
- malformed handshake cases;
- replay and clock-skew rejection;
- padding boundaries;
- oversized and malformed frames/blocks;
- slow peers and deadlines;
- pending and active admission exhaustion;
- queue item/byte exhaustion;
- duplicate/simultaneous connection race;
- cancellation and disconnect at major phases;
- cleanup after injected failure.

Use per-scenario timeouts and one overall workflow timeout. A timeout must still execute cleanup and preserve a sanitized timeout record where possible.

Do not run unbounded fuzzing in this lane. Keep fuzzing in its separate opt-in workflow.

## Deliverable 11: Evidence aggregation and validation

Create one run manifest that references all sanitized records by digest.

The aggregate manifest must contain:

- schema version;
- i2pr commit;
- workflow run and attempt;
- host contract digest;
- lock digest;
- reference cache keys and hashes;
- expected scenario IDs;
- actual scenario record filenames and SHA-256 values;
- per-gate disposition;
- cleanup verification disposition;
- aggregate manifest digest.

Validation must fail if:

- an expected record is missing;
- an unexpected passed record appears;
- any passed record contains a placeholder;
- hashes disagree with build metadata;
- scenario direction coverage is incomplete;
- cleanup is not clean or explicitly accepted forced cleanup under a negative test;
- forbidden file types or content appear in the upload tree;
- evidence contains endpoints, keys, identities, RouterInfo, payloads, raw log text, or private absolute paths.

Upload only:

```text
target/interop/evidence/*.json
target/interop/build/reference-build-summary.json
target/interop/evidence/run-manifest.json
```

Adjust exact filenames to the implemented schema, but keep the artifact allowlist narrow.

## Deliverable 12: Independent cleanup-verification gate

Cleanup must execute with `if: always()` after every privileged phase and once at the workflow end.

Run:

```bash
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh
```

Add `verify-clean-host.sh` if cleanup and verification are currently combined ambiguously.

Verification must fail if it finds:

- any namespace with an interop prefix;
- any residual interop veth;
- any i2pd, Java I2P, or i2pr-interop process from the run;
- any secret-bearing run directory;
- any identity, static key, RouterInfo, raw log, or packet capture under the retained artifact tree;
- modified global nftables state;
- modified host routes or forwarding state attributable to the harness.

The workflow conclusion must be failure if cleanup verification fails, even when all protocol scenarios passed.

## Deliverable 13: Cache policy

Reference runtime caches are expensive and may be reused only when all identity inputs match:

- canonical reference ID;
- full source revision;
- lock digest;
- build-command version;
- Ubuntu release and architecture;
- Java/Ant or compiler/CMake ABI-relevant versions;
- verified external dependency digests.

Use a restore-only cache for untrusted changes until cache poisoning risks are reviewed. A workflow run must re-hash the restored tree before use.

Do not cache:

- identities or keys;
- RouterInfo or NetDB state;
- rendered runtime configs;
- run roots;
- raw logs;
- namespace state;
- evidence records as inputs to later runs.

## Deliverable 14: Failure diagnostics

Preserve debuggability without leaking sensitive material.

Allowed diagnostics:

- fixed typed failure codes;
- phase name;
- bounded aggregate counters;
- reference identifier and full public source revision;
- artifact/config/topology digests;
- tool versions;
- elapsed duration buckets;
- cleanup counters.

Disallowed uploads:

- raw router or launcher logs;
- packet captures;
- RouterInfo bytes;
- identities or static keys;
- peer hashes derived from disposable identity;
- exact private endpoints if the evidence policy forbids them;
- absolute home/root paths;
- environment dumps.

For local authorized debugging, raw logs may exist only inside the run root until finalization and must be removed before the scenario returns.

## Deliverable 15: Promotion policy

Use staged promotion:

### Stage 1: Manual only

Run through `workflow_dispatch`. Require multiple successful runs from clean checkouts and cache-reuse runs.

### Stage 2: Scheduled control

Add a low-frequency scheduled run after Stage 1 stability. Keep failure notifications and retained sanitized evidence.

### Stage 3: Required milestone gate

Before closing Milestone 3, require a current successful manual or scheduled run at the closure commit.

### Stage 4: Pull-request integration decision

After Milestone 3, separately decide whether a reduced handshake-smoke lane should run on trusted pull requests. Do not automatically expose privileged execution to forked or untrusted code.

## Documentation updates

Update:

- `tests/integration/ntcp2/README.md` with exact local and workflow commands;
- `tests/integration/ntcp2/evidence/README.md` with aggregate manifest rules;
- `CONTRIBUTING.md` with ordinary versus privileged validation responsibilities;
- `GUARDRAILS.md` with workflow privilege and artifact restrictions;
- `docs/private-testnet.md` with the reference-control topology;
- `docs/security-model.md` with preparation/execution trust separation;
- `docs/architecture/tooling.md` with gate ownership;
- `docs/protocol-support.md` only after successful evidence supports a status change;
- `specs/CONFORMANCE.md` with exact evidence references.

## Required validation

Validate workflow syntax and local scripts, then execute the complete manual lane from the intended build system.

Required terminal sequence:

```text
contract:                     pass
reference-build:              pass
reference-offline-reuse:      pass
environment-smoke:            pass
reference-crosscheck-ipv4:    pass
i2pr-handshake-smoke-ipv4:    pass
full-matrix:                  pass or explicit validated IPv6 skips only
evidence-validation:          pass
cleanup-verification:         pass
```

Repeat at least once using restored verified caches and a fresh runner.

## Stop conditions

Stop promotion if:

- the runner cannot provide the required namespace/nftables privileges;
- third-party or restored artifacts cannot be verified;
- offline reuse performs a network operation;
- reference-control fails;
- any i2pr direction lacks independent evidence;
- evidence validation permits placeholders or forbidden material;
- cleanup cannot prove a clean host;
- workflow cancellation can strand privileged state without recovery;
- untrusted pull-request code would gain inappropriate privileged execution;
- total runtime or resource usage is unbounded.

## Exit criteria

Plan 043 is complete when:

- the manual Ubuntu workflow exposes all required semantic gates;
- preparation and isolated execution are separated;
- reference caches are exact and verified;
- offline repeatability passes;
- environment smoke and reference control pass;
- all four i2pr/reference IPv4 directions pass with authenticated I2NP exchange;
- the full bounded matrix completes with valid results;
- evidence aggregation rejects missing, placeholder, inconsistent, or forbidden records;
- cleanup verification independently proves zero residual state;
- retained artifacts contain only the narrow sanitized allowlist;
- the closure commit has a successful workflow run suitable for Milestone 3 review.

Completion of this plan permits the separate Milestone 3 closure review. It does not imply production readiness or authorize public-network operation.
