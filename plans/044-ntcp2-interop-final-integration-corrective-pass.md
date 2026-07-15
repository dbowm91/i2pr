# Plan 044: NTCP2 interoperability final integration corrective pass

## Objective

Close the remaining integration defects between the repaired Ubuntu reference-router apparatus, the Java I2P/i2pd control harness, the runtime-owned i2pr NTCP2 wire driver, and the Plan 043 build-system gate chain.

The current checkout contains the major component implementations required by Plans 040 through 043, but it does not yet provide qualifying mixed-router evidence. This plan converts those components into one executable, reproducible, fail-closed path and then performs the first authorized Ubuntu 24.04 amd64 runs needed to determine whether Milestone 3 can close.

The completed work must establish, without contacting the public I2P network, that:

- the pinned Java I2P and i2pd runtimes start from verified offline caches;
- the reference-only Java I2P/i2pd control scenarios establish authenticated NTCP2 links in both controlled directions;
- the `i2pr-interop` launcher is actually composed with each reference router rather than validated only through local tests;
- i2pr can initiate and accept authenticated NTCP2 sessions with both references;
- the selected data-phase proof is valid for the pinned reference implementations and does not depend on an unsupported echo assumption;
- each required positive and negative scenario emits one independently attributable typed result;
- evidence survives cleanup without retaining identities, static keys, RouterInfo, I2NP payloads, raw logs, packet captures, endpoints, or private paths;
- ordered gate execution cannot relabel or corrupt records produced by earlier gates;
- cleanup and host-state verification remain independent terminal requirements;
- Plan 043 produces a validated aggregate manifest from a clean checkout on the supported Ubuntu build environment.

Milestone 3 remains open throughout this plan. NTCP2 remains experimental and non-advertised until a separate closure review verifies all required evidence.

## Current repository state

This plan begins from the state represented by the following implementation series:

- Plan 040 apparatus hardening;
- Plan 041 Java I2P/i2pd reference crosscheck implementation;
- Plan 042 bounded runtime-owned NTCP2 wire driver;
- Plan 043 ordered Ubuntu build-system workflow.

The repository already has:

- full 40-character Java I2P and i2pd source pins;
- canonical reference identifiers `java_i2p` and `i2pd`;
- strict cache metadata and current-cache selection;
- disposable Linux network namespaces and exact nftables policy generation;
- separate reference-pair scenarios and topology ownership;
- a runtime-owned handshake action executor and authenticated link owner;
- a non-production `i2pr-interop` launcher with strict scenario parsing;
- typed launcher status records;
- evidence validators, cache manifests, aggregate validation, and clean-host verification;
- a manual Ubuntu 24.04 workflow with ordered profile selection.

The remaining blockers are integration and first-run correctness issues, not a reason to redesign the overall architecture.

## Controlling documents

The implementing agent must read and preserve the boundaries in:

- `plans/000-mvp-roadmap.md`
- `plans/030-milestone-3-overview.md`
- `plans/039-plan-038-corrective-interoperability-roadmap.md`
- `plans/040-interop-apparatus-corrective-pass.md`
- `plans/040-closure.md`
- `plans/041-reference-router-private-crosscheck.md`
- `plans/041-closure.md`
- `plans/042-runtime-owned-ntcp2-wire-driver.md`
- `plans/042-status.md`
- `plans/043-ubuntu-build-system-interop-gates.md`
- `docs/adr/0015-ubuntu-reference-router-harness.md`
- `docs/adr/0016-ubuntu-build-system-interop-gates.md`
- `docs/architecture/interop-apparatus.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `specs/CONFORMANCE.md`
- `tests/integration/ntcp2/README.md`
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`

Where implementation and documentation disagree, fail closed and update both in the same change. Do not weaken a safety or evidence requirement merely to make a scenario pass.

## Non-negotiable boundaries

1. Do not activate `i2pr-daemon`.
2. Do not reseed, bootstrap, publish RouterInfo to a public NetDB, enable SSU2, create tunnels, accept transit traffic, expose SAM/I2CP/proxy/console services, or contact the public I2P network.
3. Keep Tokio, sockets, tasks, deadlines, cancellation, replay, admission, queues, and authenticated link ownership in `i2pr-runtime`.
4. Keep `i2pr-transport-ntcp2` runtime-neutral and free of Tokio, sockets, filesystem access, and harness policy.
5. Use `tools/i2pr-interop` only as a non-production composition root.
6. Use exact locked reference revisions and verified cache metadata. No packages or floating branches may substitute for the pinned runtimes.
7. Preparation is the only network-enabled phase. Scenario execution must use verified offline caches and namespace-local synthetic links.
8. Do not retain raw RouterInfo, identities, transport keys, I2NP bodies, transcripts, raw logs, packet captures, endpoint text, or private paths in evidence.
9. Listener readiness, TCP connectivity, a generic `NTCP2` log line, local loopback tests, and padding-only exchange are not authenticated interoperability evidence.
10. Protocol success never overrides cleanup failure.
11. Reference-control evidence is not i2pr evidence.
12. Workflow existence is not a passing result.

## Primary defects to correct

### 1. Java RouterInfo export path conflicts with the documented runtime layout

`JavaI2pAdapter` configures Java I2P with a writable router/data directory under `reference-data`, and the checked-in provenance document states that `router.info` is written there. The current export implementation still checks `reference-runtime`.

Required correction:

- update `tests/integration/ntcp2/harness/java_i2p.py` to locate the RouterInfo only in the pinned, reviewed writable router directory;
- accept only reviewed filenames and regular files inside the adapter-owned run root;
- reject symlinks, empty files, oversized files, and candidates outside the run root;
- preserve strict RouterInfo validation before import or evidence classification;
- update adjacent provenance documentation if the actual pinned source behavior differs after authorized execution.

Add tests that create a synthetic adapter layout and prove:

- `reference-data/router.info` is accepted;
- a file only under `reference-runtime` is not accepted;
- symlink and path-escape candidates are rejected;
- the exported copy remains inside the disposable exchange directory.

### 2. Schema-1 evidence sanitation rejects safe fixed taxonomy values

The schema-1 scenario records contain a fixed expected-result class describing bounded I2NP exchange. The current generic forbidden-string scan rejects the token `I2NP`, even when it appears in a repository-controlled enum rather than a payload, path, transcript, or remote log.

Required correction:

- replace broad substring rejection with field-aware sanitation;
- represent `expected`, `actual_typed_result`, `known_deviation`, and reproduction metadata through explicit allowlisted enums or fixed templates;
- continue rejecting raw I2NP bytes, decoded message text, RouterInfo contents, identities, keys, endpoint text, private paths, and arbitrary remote error strings;
- reject unknown free-form strings in evidence-producing code paths;
- preserve exact schema order and digest behavior unless a versioned schema migration is required;
- if the schema changes, update validators, aggregate code, README documentation, and all tests atomically.

Add positive and negative tests proving:

- `authenticated-handshake-and-bounded-i2np-exchange` is accepted as a fixed taxonomy value;
- raw endpoint text remains rejected;
- `RouterInfo`, private-key markers, absolute home/root paths, packet-capture names, and long encoded payload material remain rejected;
- a passed record with zero hashes or placeholder commit data remains invalid;
- evidence finalization failure cannot be mislabeled as cleanup failure unless cleanup itself also failed.

### 3. Gate archival relabels evidence from previous gates

The Plan 043 workflow runs several profiles in one checkout. The current `run-gate.sh` archives every JSON record present in the common evidence directory after each profile. Later gates therefore rename records produced by earlier gates and break aggregate attribution.

Required correction:

Adopt one of these reviewed designs, in preference order:

1. Each gate writes to a gate-specific staging directory, validates its records, then atomically moves only those records into the common evidence directory with the gate prefix.
2. Record the evidence directory contents before the matrix run and archive only newly created files afterward, rejecting modified pre-existing files.
3. Give the matrix runner an explicit output directory and prevent direct writes to the aggregate evidence directory.

The chosen implementation must:

- make gate attribution immutable;
- reject filename collisions;
- reject a record whose scenario ID is not allowed for the current gate;
- reject modification or deletion of earlier gate records;
- leave no unprefixed evidence files after successful archival;
- keep `run-manifest.json` reserved for the aggregate step;
- preserve records from earlier gates unchanged byte-for-byte;
- clean the gate staging directory even on failure.

Add a sequential-gate regression test that simulates:

```text
environment-smoke
reference-crosscheck-ipv4
handshake-smoke
full
```

and confirms that every record retains its original gate prefix and digest.

### 4. Workflow toolchain and locked-build contract is incomplete

The workflow installs the minimal Rust profile but executes `cargo fmt` and `cargo clippy`. It also omits `--locked` from several contract commands.

Required correction:

- install the repository toolchain with explicit `rustfmt` and `clippy` components;
- use `--locked` for `cargo check`, `cargo test`, `cargo clippy`, `cargo doc`, and launcher builds where Cargo accepts it;
- retain the exact Rust 1.95.0 contract;
- record `rustc --version --verbose` and `cargo --version --verbose` in sanitized build metadata;
- update `validate-build-contract.py` to verify required components and locked invocations structurally;
- keep third-party action references at fixed reviewed major versions or stronger immutable pins;
- do not make privileged runs available to fork pull requests or arbitrary untrusted code.

Add a static workflow test that fails if:

- `rustfmt` or `clippy` components are omitted;
- a required Cargo command lacks `--locked`;
- `ubuntu-latest` appears;
- a moving `master`, `main`, or `latest` action reference appears;
- arbitrary profile, revision, URL, endpoint, network-ID, or shell input is introduced.

### 5. Mixed i2pr/reference scenarios are not wired

The current schema-1 runner blocks every non-environment-smoke profile before constructing an i2pr/reference pair. The `I2prAdapter` and Rust launcher exist, but the harness does not render the launcher scenario, start the two implementations in controlled order, exchange RouterInfo, or consume terminal status.

This is the central deliverable of Plan 044.

## Deliverable A: Define the mixed-router execution model

Do not treat a manifest entry with `direction = "both"` as one ambiguous run. Expand each primary IPv4 scenario into two independently attributable directional executions.

Required direction set:

```text
i2pr -> Java I2P
Java I2P -> i2pr
i2pr -> i2pd
i2pd -> i2pr
```

Each direction must have:

- a unique execution ID;
- one declared initiator and one declared responder;
- one exact local and peer endpoint assignment;
- one startup order;
- one RouterInfo generation/import order;
- one firewall direction policy;
- one launcher role;
- one terminal typed result;
- one evidence record;
- one cleanup result.

Do not allow one direction to mask another. The aggregate handshake-smoke gate passes only when all four direction records pass.

Implement either:

- a dedicated mixed-router scenario expansion layer, or
- explicit directional scenario files derived from the existing manifest.

Whichever design is chosen, keep the human-readable manifest, runner expansion, aggregate expected-scenario set, and validation tests synchronized.

## Deliverable B: Render strict launcher scenarios

Create a single reviewed harness function for writing `run-root/scenario.toml` for `i2pr-interop`.

The renderer must populate the exact launcher schema with:

- schema version;
- execution-specific scenario ID;
- `initiator` or `responder` role;
- IPv4 or IPv6 family;
- local synthetic address and port;
- peer address and port only for initiator runs;
- private network ID 99;
- confined state directory;
- peer RouterInfo path only for initiator runs;
- bounded handshake/read/write/queue/drain deadlines;
- one supported padding profile;
- the reviewed smoke-message profile;
- deterministic seed only where allowed by the scenario;
- expected-result class;
- confined status path.

The renderer must reject:

- absolute paths;
- parent traversal;
- endpoints outside the synthetic namespace ranges;
- mismatched address families;
- missing peer data for initiators;
- peer data for responders;
- unsupported network IDs;
- arbitrary padding or message profiles;
- unknown fields.

Before starting a router, parse the rendered file with the same Rust launcher parser through a non-networking validation mode or a focused parser test path. Do not discover schema errors only after privileged processes start.

## Deliverable C: Mixed-router RouterInfo lifecycle

For every direction:

1. Create the two namespaces and exact firewall policy.
2. Prepare both implementations without starting public or unrelated services.
3. Generate the responder RouterInfo first where the initiator requires it before dial.
4. Strictly validate the RouterInfo signature, transport address, static key, obfuscation IV, network ID assumptions, endpoint binding, size, and run-root confinement.
5. Copy only the required RouterInfo into the initiator's confined state/import path.
6. Never place RouterInfo in the retained evidence directory.
7. Delete all RouterInfo copies during cleanup.

For i2pr as responder:

- prepare i2pr state and generate its signed RouterInfo before starting the reference initiator;
- import the i2pr RouterInfo into the reference implementation using the implementation-specific NetDB filename convention;
- start the i2pr listener and require a separate `listener_ready` status before starting the reference initiator.

For i2pr as initiator:

- generate and validate the reference RouterInfo first;
- stage the reference RouterInfo under the i2pr run root;
- start the reference responder and confirm readiness;
- invoke `i2pr-interop ntcp2 dial` with the strict scenario.

## Deliverable D: Compose `I2prAdapter` with reference adapters

Extend the mixed runner so it owns:

- `NamespaceTopology`;
- one `I2prAdapter`;
- one `JavaI2pAdapter` or `I2pdAdapter`;
- startup sequencing;
- readiness observation;
- terminal launcher status;
- reference-side authenticated observation;
- bounded stop/join;
- evidence finalization;
- residual-state verification.

The adapter boundary must distinguish:

- process started;
- listener ready;
- authenticated;
- data-phase proof complete;
- terminal failure;
- process exited;
- forced termination;
- cleanup complete.

Do not accept launcher stdout that fails the strict status parser. Do not accept multiple terminal statuses, terminal-before-ready for listener mode, scenario-ID mismatch, unknown counters, unknown reason codes, or arbitrary additional JSON fields.

The mixed runner must translate exceptions only into fixed reason codes. Raw exception strings and remote log lines must not cross the evidence boundary.

## Deliverable E: Establish a valid data-phase interoperability oracle

The current local driver sends a DeliveryStatus message and requires an inbound DeliveryStatus message. The repository explicitly has not proven that Java I2P or i2pd will echo or otherwise respond with DeliveryStatus in this synthetic context.

Before using this as the positive gate, inspect the pinned Java I2P and i2pd behavior and choose a protocol-valid proof.

The accepted design must satisfy both of these goals:

- prove i2pr can send a bounded parseable I2NP message that the reference accepts after authentication;
- prove i2pr can receive and parse a bounded I2NP message from the reference over the same authenticated session or an independently controlled reverse-direction session.

Candidate designs, in preference order:

1. A deterministic private-test trigger supported by both pinned references that causes a documented response message.
2. Separate send and receive assertions per direction, using implementation-specific but protocol-valid test hooks confined to the private environment.
3. Authoritative reference counters or structured state APIs proving accepted inbound I2NP, combined with a separately induced reference-to-i2pr message.
4. A minimal harness-only reference-side NTCP2 data injection path, only if it still exercises the pinned router's authenticated transport and does not bypass the reference implementation.

Do not use:

- an assumed echo that the protocol does not specify;
- generic log substring matching as the sole message proof;
- TCP byte counts without authenticated-frame and message parsing;
- self-handshake or i2pr-to-i2pr exchange;
- padding-only or termination-only exchange.

Document the selected oracle with:

- pinned upstream source locations;
- expected initiating event;
- exact message type and bounds;
- sender and receiver observations;
- timeout behavior;
- false-positive analysis;
- why the mechanism remains private and non-production.

If no valid shared oracle exists, split the Java and i2pd smoke mechanisms while preserving one common evidence schema.

## Deliverable F: Strengthen the reference-only control trigger

The Plan 041 runner configures one-way initiation policy, imports RouterInfo, starts both routers, and waits for authenticated log observations. It does not currently issue an explicit implementation-specific dial or traffic trigger.

On the authorized Ubuntu host:

- verify whether the pinned routers initiate an NTCP2 session automatically after importing the sole peer;
- verify exact readiness and authenticated observations against real logs/state;
- verify that the declared initiator is actually the endpoint that emitted the initial SYN, without using ephemeral source-port heuristics;
- if automatic dialing is not deterministic, add a reviewed private trigger for each implementation;
- keep the reverse direction as a separate scenario;
- require dual authoritative observations rather than a single generic phrase;
- preserve the firewall as an enforcement mechanism, not merely an observation mechanism.

Where possible, prefer structured state or implementation-specific counters over fragile English log phrases. If log parsing remains necessary, pin exact bounded patterns to the locked versions and reject unknown or ambiguous matches.

## Deliverable G: Mixed evidence schema and counters

Schema-1 records currently describe one reference and one scenario but use zero runtime counters for non-smoke paths. Extend or version the evidence schema so a passing mixed-router record includes real values for:

- exact i2pr commit and clean/dirty disposition;
- reference identifier, version, and full revision;
- reference artifact and installed-tree hashes;
- launcher binary hash;
- rendered launcher configuration hash;
- rendered reference configuration hash;
- namespace topology and firewall-policy hash;
- direction and initiator/responder roles;
- authenticated-link count;
- handshake attempts and terminal outcomes;
- frames sent and received;
- I2NP messages sent, accepted, received, and parsed, using only aggregate counters;
- queue items/bytes high-water marks where available;
- admission/replay/backoff counters relevant to the scenario;
- process started/exited/forced counters for both sides;
- runtime child/task cleanup counters;
- cleanup result;
- fixed known-deviation/reason code;
- evidence digest.

The evidence must not contain:

- local or peer endpoint text;
- router hashes or identities;
- static public/private keys;
- obfuscation IVs;
- RouterInfo bytes;
- I2NP bodies or message IDs;
- raw status streams;
- raw logs;
- absolute paths;
- arbitrary exception text.

Passed records must reject zero-filled required hashes and required counters that remain at zero.

## Deliverable H: Negative and resource scenarios

Do not run the full profile until all four primary IPv4 directions pass.

Then wire the existing full manifest to real mixed-router execution for:

- IPv6 positive paths or explicit validated `skipped_ipv6` results;
- malformed handshake input;
- replay rejection;
- clock-skew rejection;
- padding minimum, representative, maximum, and maximum-plus-one behavior;
- oversized handshake/frame/block rejection;
- malformed block ordering;
- slow peer and read/write/handshake deadlines;
- pending-handshake admission exhaustion;
- active-link admission exhaustion;
- queue item exhaustion;
- queue byte exhaustion;
- duplicate/simultaneous-link race;
- cancellation during connect, handshake, authenticated exchange, and drain;
- peer disconnect at major phases;
- cleanup after injected failure.

Each negative scenario must define:

- injection point;
- responsible side;
- expected typed result;
- maximum runtime;
- expected counters;
- cleanup expectations;
- whether the reference process is expected to remain healthy.

A negative case passes only when the expected bounded rejection occurs. An unexpected successful session is a test failure.

## Deliverable I: Cleanup and host-state correctness

Review the cleanup path after mixed-run integration.

Required behavior:

- normal process handles receive bounded graceful stop and join;
- emergency cleanup uses atomic PID records and namespace PID enumeration;
- all owned namespaces and host veths are removed;
- no router or launcher process remains;
- all run roots, identities, keys, RouterInfo, scenario files, status streams, and raw logs are deleted;
- only validated sanitized evidence remains;
- namespace nftables policy disappears with the namespace;
- host routes, nftables rules, and forwarding values match the recorded baseline;
- cleanup failure changes a protocol pass to `failed_cleanup`;
- cleanup verification runs even after timeout, signal, matrix failure, evidence failure, or aggregate failure.

Add failure-injection tests for:

- reference refusing graceful stop;
- launcher refusing graceful stop;
- namespace deletion failure;
- evidence write failure;
- run-root deletion failure;
- residual process detection;
- host-state digest drift.

Do not suppress cleanup errors in the final workflow disposition.

## Deliverable J: Aggregate and workflow correctness

After gate-specific archival is fixed, require the aggregate manifest to include exactly the expected records for the selected profile.

For `handshake-smoke`, expected passing evidence must include:

```text
environment-smoke: Java I2P
environment-smoke: i2pd
reference-crosscheck-ipv4: Java I2P -> i2pd
reference-crosscheck-ipv4: i2pd -> Java I2P
handshake-smoke: i2pr -> Java I2P
handshake-smoke: Java I2P -> i2pr
handshake-smoke: i2pr -> i2pd
handshake-smoke: i2pd -> i2pr
```

For `full`, include the handshake-smoke set plus every required full-matrix record or validated IPv6 skip.

The aggregate validator must reject:

- missing records;
- extra passed records;
- duplicated gate/scenario pairs;
- mislabeled gate prefixes;
- modified earlier-gate records;
- digest mismatch;
- mixed commits;
- mixed reference cache selections;
- dirty checkout unless explicitly permitted and classified;
- incomplete direction coverage;
- non-passing prerequisite gates;
- cleanup not verified as clean;
- placeholder or zero values;
- records from a previous workflow run.

Include a workflow/run nonce or equivalent bounded run identifier in aggregate metadata so stale evidence cannot satisfy a new run. Do not expose secret runner data.

## Deliverable K: Documentation reconciliation

Update documentation to reflect the exact post-Plan-044 state.

In particular, correct any statement claiming that the checkout lacks a wire-level adapter. The accurate pre-execution status is:

```text
runtime-owned NTCP2 wire adapter implemented and locally validated;
mixed-router harness composition and authorized evidence pending;
NTCP2 remains experimental and non-advertised.
```

After successful external execution, update only the evidence statements directly supported by retained records. Do not advertise NTCP2 or close Milestone 3 in the implementation commit itself.

Update as needed:

- `README.md`
- `AGENTS.md`
- `CONTRIBUTING.md`
- `GUARDRAILS.md`
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`
- `.opencode/skills/i2pr-ntcp2-interop/references/operations.md`
- `docs/architecture/interop-apparatus.md`
- `docs/architecture/i2pr-runtime.md`
- `docs/private-testnet.md`
- `docs/protocol-support.md`
- `docs/security-model.md`
- relevant ADRs
- `specs/CONFORMANCE.md`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`

## Implementation sequence

Execute the work in this order.

### Phase 1: Deterministic corrective fixes

1. Correct Java RouterInfo export location and path validation.
2. Repair field-aware evidence sanitation.
3. Fix gate-specific evidence staging and archival.
4. Fix workflow Rust components and locked Cargo commands.
5. Add focused regression tests for all four corrections.

Do not begin a privileged Ubuntu run until this phase passes locally.

### Phase 2: Mixed-run schema and composition

1. Define the four primary IPv4 direction executions.
2. Implement strict launcher-scenario rendering.
3. Implement RouterInfo generation/import sequencing.
4. Compose `I2prAdapter` with Java and i2pd adapters.
5. Consume strict readiness and terminal statuses.
6. Populate real mixed-run counters and hashes.
7. Prove cleanup through local failure-injection tests.

At the end of this phase, `handshake-smoke` must no longer return `i2pr-mixed-router-profile-not-wired`. On unsupported hosts it should return only the appropriate host-contract blocker before privileged work.

### Phase 3: Data-phase oracle validation

1. Inspect pinned reference behavior.
2. Select and document valid Java and i2pd data-phase proofs.
3. Implement the required triggers/observations.
4. Add deterministic local parser/counter tests.
5. Reject the prior echo assumption if it is not supported.

Do not classify an authenticated handshake as complete handshake-smoke evidence until the data-phase proof is implemented.

### Phase 4: Authorized reference-control execution

On an authorized disposable Ubuntu 24.04 amd64 host:

```bash
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo -E bash scripts/interop/reset-lane-state.sh
sudo -E bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
sudo -E bash scripts/interop/verify-clean-host.sh --record-baseline
bash scripts/interop/build-references.sh --force-rebuild
python3 scripts/interop/cache-manifest.py --verify
sudo -E bash scripts/interop/offline-reuse.sh
sudo -E bash scripts/interop/run-gate.sh --profile environment-smoke --offline
sudo -E bash scripts/interop/run-gate.sh --profile reference-crosscheck-ipv4 --offline
python3 scripts/interop/validate-evidence.py
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify
```

Stop if environment smoke or either reference direction fails. Diagnose the reference harness before introducing i2pr as the variable under test.

### Phase 5: Authorized primary mixed-router execution

After the reference control passes:

```bash
sudo -E bash scripts/interop/run-gate.sh --profile handshake-smoke --offline
python3 scripts/interop/validate-evidence.py
python3 scripts/interop/aggregate-evidence.py --profile handshake-smoke
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify
```

Require all four direction records. Preserve typed failures exactly; do not retry until the failure is understood and any code/configuration change has a new commit identity.

Repeat the complete handshake-smoke chain:

- once from a fresh checkout and empty generated state;
- once from verified offline caches;
- once after reboot or equivalent runner reset.

All repetitions must produce the same semantic disposition and zero residual state. Hashes may differ only where documented nondeterministic build/runtime metadata legitimately differs.

### Phase 6: Full bounded matrix

Only after Phase 5 passes:

```bash
sudo -E bash scripts/interop/run-gate.sh --profile full --offline
python3 scripts/interop/validate-evidence.py
python3 scripts/interop/aggregate-evidence.py --profile full
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify
```

Investigate every unexpected result. Do not weaken an expected negative outcome to obtain a green matrix.

### Phase 7: GitHub Actions proof

Run the manual workflow in increasing order:

1. `environment-smoke`
2. `reference-crosscheck-ipv4`
3. `handshake-smoke`
4. `full`

For each workflow run:

- record the run ID and attempt in sanitized metadata;
- inspect job steps and uploaded artifacts;
- confirm only approved evidence and build summary files were uploaded;
- confirm cleanup ran under `if: always()`;
- confirm no artifact contains forbidden material;
- confirm the aggregate manifest validates after artifact retrieval.

Do not add a schedule until at least two complete manual `full` runs pass on independent fresh runners.

## Local validation requirements

Before pushing implementation changes, run from the repository root:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
python3 scripts/interop/validate-scenarios.py
python3 scripts/interop/validate-build-contract.py
python3 scripts/interop/validate-evidence.py
bash -n scripts/check-ntcp2-interoperability.sh scripts/interop/*.sh scripts/interop/lib/*.sh scripts/interop/ubuntu/*.sh
git diff --check
```

Add focused tests for:

- Java RouterInfo directory selection;
- safe taxonomy evidence validation;
- sequential gate archival;
- mixed scenario expansion;
- launcher scenario rendering;
- listener readiness versus terminal status;
- exactly one terminal status;
- all four direction coverage;
- stale evidence rejection;
- mixed-run evidence hashes and counters;
- unsupported DeliveryStatus echo assumption;
- cleanup precedence;
- aggregate manifest coverage.

## Required external evidence

A Plan 044 closure record must reference sanitized records and workflow runs proving:

- exact Ubuntu 24.04 amd64 host contract;
- exact Java I2P and i2pd revisions;
- successful network-enabled reference build;
- successful verified offline reuse;
- both environment-smoke records;
- both reference-control directional records;
- all four primary mixed-router IPv4 directional records;
- required full-matrix records or validated IPv6 skips;
- real nonzero commit/configuration/artifact/tree/topology/launcher hashes;
- real nonzero authenticated frame and I2NP counters for positive cases;
- expected typed rejections for negative cases;
- zero residual process, namespace, interface, run-root, and host-state drift;
- successful aggregate validation;
- successful cleanup verification;
- manual workflow run IDs and attempts.

Do not commit raw generated evidence if repository policy keeps execution evidence as workflow artifacts. The closure record may cite sanitized artifact digests and workflow identifiers instead.

## Stop conditions

Stop and leave a typed blocker if any of the following occurs:

- the pinned source revision cannot be fetched or verified;
- a reference build requires undeclared network access;
- offline reuse attempts DNS, clone, fetch, curl, package installation, or dependency download;
- Java I2P or i2pd ignores the private network-ID or transport configuration;
- a reference attempts public routing, reseed, DNS, update, or unrelated service startup;
- RouterInfo cannot be strictly validated and endpoint-bound;
- the reference-only crosscheck cannot prove the intended direction;
- no protocol-valid data-phase oracle can be established;
- listener readiness is confused with authentication;
- evidence sanitation would require retaining raw protocol material;
- cleanup cannot prove zero residual state;
- workflow artifacts contain forbidden material;
- a proposed fix requires moving runtime ownership into a protocol crate;
- a test passes only by weakening a guardrail or evidence requirement.

## Exit criteria

Plan 044 implementation is complete only when:

1. Java RouterInfo export follows the actual pinned runtime-directory contract.
2. Safe fixed evidence taxonomy values validate while secret/payload/path material remains rejected.
3. Sequential gates preserve immutable per-gate evidence attribution.
4. The workflow installs required Rust components and uses locked Cargo commands.
5. `handshake-smoke` no longer returns `i2pr-mixed-router-profile-not-wired`.
6. The four primary IPv4 directions execute independently.
7. Each positive direction proves authenticated handshake plus a protocol-valid bounded I2NP send/receive result.
8. Reference-control directionality is proven rather than assumed.
9. Mixed-run evidence contains real hashes and counters.
10. Full negative/resource scenarios produce expected bounded results.
11. Cleanup and clean-host verification pass after every run.
12. Aggregate validation rejects stale, mislabeled, incomplete, or placeholder evidence.
13. The manual Plan 043 workflow passes through the requested profile on an authorized Ubuntu runner.
14. Documentation accurately distinguishes implemented wire support from proven interoperability.

Plan 044 does not itself close Milestone 3. After these exit criteria are met, create a separate Milestone 3 evidence/closure review that compares the retained results directly against `plans/000-mvp-roadmap.md` and `specs/CONFORMANCE.md` before changing support or advertisement status.

## Handoff instructions

The implementing agent should begin with the deterministic corrective defects and tests, then wire the four primary mixed-router directions, then validate the data-phase oracle, and only then use the privileged Ubuntu lane.

Keep commits reviewable by separating:

1. deterministic harness/evidence/workflow corrections;
2. mixed-run composition and scenario rendering;
3. data-phase oracle implementation;
4. negative/resource scenario integration;
5. documentation and closure evidence.

Every commit must leave the repository tests passing and must preserve typed blockers for work not yet completed. Do not replace an explicit blocker with a synthetic pass.