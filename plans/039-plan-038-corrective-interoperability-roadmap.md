# Plan 039: Plan 038 corrective interoperability roadmap

## Objective

Turn the Plan 038 Ubuntu reference-router harness foundation into a functioning, reproducible, fail-closed NTCP2 interoperability lane that can be used on the Ubuntu build system to close the remaining Milestone 3 evidence gap.

This roadmap does not reopen the architectural work already completed in Plans 031 through 038. It corrects the harness implementation, proves the pinned Java I2P and i2pd environments independently of i2pr, completes the runtime-owned wire driver, and promotes the resulting scenarios into an explicit build-system gate.

The final outcome must demonstrate all of the following without contacting the public I2P network:

- exact pinned Java I2P and i2pd source revisions build on Ubuntu 24.04 amd64;
- the produced reference runtime trees can be reused offline;
- all router processes execute in disposable, fail-closed Linux network namespaces;
- Java I2P and i2pd can exchange RouterInfo and establish an authenticated private NTCP2 link with one another;
- i2pr can initiate and accept authenticated NTCP2 sessions with both independent references;
- at least one bounded, parseable I2NP message is exchanged in each positive direction;
- malformed, replay, padding, duplicate-link, timeout, and resource-pressure cases produce typed bounded outcomes;
- every process, namespace, interface, key, identity, raw log, and resource lease is removed after each scenario;
- only sanitized records containing real commit, configuration, topology, artifact, and runtime-counter evidence leave the execution boundary.

Milestone 3 remains open until all required positive and negative gates in this roadmap pass. NTCP2 support remains experimental and non-advertised until then.

## Relationship to the MVP roadmap

`plans/000-mvp-roadmap.md` defines the Milestone 3 exit criteria:

- successful NTCP2 handshakes with at least two independent router implementations in an authorized testnet;
- required I2NP exchange over an authenticated link;
- bounded rejection of malformed and adversarial traffic;
- complete connection and resource cleanup.

Plan 038 added the intended Ubuntu harness structure but explicitly closed only the harness foundation. This roadmap is the corrective execution sequence required to satisfy the actual Milestone 3 exit criteria. It does not begin Milestone 4 reseeding, NetDB participation, public RouterInfo publication, exploratory tunnels, SAM, I2CP, SSU2, or public-network testing.

## Controlling documents

The implementing agent must treat the following as controlling inputs:

- `plans/000-mvp-roadmap.md`
- `plans/030-milestone-3-overview.md`
- `plans/030-milestone-3-closure.md`
- `plans/036-m3-interoperability-adversarial-validation-closure.md`
- `plans/037-m3-corrective-integration-closure.md`
- `plans/038-ubuntu-reference-router-interoperability-harness.md`
- `plans/038-closure.md`
- `tests/integration/ntcp2/manifest.toml`
- `tests/integration/ntcp2/references.lock.toml`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `docs/architecture.md`
- `docs/adr/0015-ubuntu-reference-router-harness.md`
- `GUARDRAILS.md`

When a plan task conflicts with a guardrail or a pinned protocol source, the guardrail or pinned source wins. Record the conflict and stop rather than weakening isolation or silently changing protocol behavior.

## Current state

The repository already contains:

- pinned reference metadata and Ubuntu package declarations;
- Java I2P and i2pd source-build scripts;
- namespace, nftables, cleanup, and isolation helpers;
- Java I2P and i2pd process adapters and configuration templates;
- scenario manifests and typed evidence schemas;
- a manual Ubuntu workflow;
- a non-production `i2pr-interop` launcher seam;
- runtime-owned socket, admission, replay, backoff, deadline, and link-lifecycle primitives;
- runtime-neutral NTCP2 handshake and data-phase state machines.

The existing closure correctly states that no mixed-router evidence exists. The launcher returns `blocked_missing_driver`, the runner executes only an environment-smoke path, and no authorized Ubuntu namespace run has produced reference artifacts or authenticated interoperability evidence.

## Confirmed corrective findings

The implementation must correct the following before the first expensive Ubuntu reference build is treated as meaningful:

1. **Revision verification is internally inconsistent.** The lock uses abbreviated source revisions, while the build helper compares them directly against the full value of `git rev-parse HEAD`. Pin full 40-character commit object IDs and use them everywhere that affects checkout, verification, cache identity, metadata, and evidence.

2. **Java reference identifiers are inconsistent.** The builder uses `java-i2p`; the runner and scenarios use `java_i2p`. Select one canonical machine identifier and use it in paths, metadata, command-line values, evidence, scenario files, and tests.

3. **Generated veth names exceed the Linux interface-name limit.** Namespace names may remain descriptive, but interface names must be generated from a short collision-resistant token and remain at most 15 bytes.

4. **The nftables input rule matches source ports.** Initial inbound TCP SYN packets use an ephemeral source port. Input rules must constrain the exact peer address and destination listening port. IPv6 rules must be equally narrow and must not permit an entire documentation prefix without port and protocol constraints.

5. **The Java adapter publishes the i2pr-side address while running in the reference namespace.** Render the exact local namespace address for each implementation and address family.

6. **Java base/config/data and RouterInfo paths are not proven.** Verify the pinned installation layout and runtime behavior on Ubuntu. The adapter must use the actual RouterInfo and NetDB locations and implementation-required filenames rather than assuming a generic data directory.

7. **The current reference-crosscheck profile is not a crosscheck.** It aliases the same i2pr scenarios and starts only one reference process. Add a distinct Java-I2P-to-i2pd topology, process lifecycle, RouterInfo exchange, connection observation, and evidence path.

8. **IPv6 scenarios are declared but adapters render IPv4-only settings.** Add family-aware rendering and capability probes or emit an explicit, evidence-backed `skipped_ipv6` result. Never run an IPv6-labelled scenario with IPv4-only configuration.

9. **Evidence uses placeholders and is deleted or retained with secrets.** Passed records must contain real hashes, commit IDs, counters, and cleanup results. Sanitized evidence must be copied outside the secret-bearing run root before the complete run root is removed. A keep flag must never preserve identities, keys, raw logs, RouterInfo files, or packet material.

10. **Emergency cleanup and process supervision disagree.** Either write atomic PID ownership records or have cleanup enumerate namespace PIDs. Cleanup must terminate, join, delete, and verify rather than merely issue best-effort commands.

11. **The i2pr wire driver is intentionally absent.** Complete the runtime action executor at the runtime/test-launcher composition boundary. Do not move socket ownership into protocol crates and do not activate the normal daemon.

12. **The workflow runs only environment smoke.** Split the build-system lane into ordered gates and make cleanup verification an independent terminal requirement.

## Execution sequence

This roadmap is decomposed into four dependent plans. Execute them in order.

### Plan 040: Correct the executable apparatus

Repair source pinning, host checks, cache identity, namespace naming, firewall semantics, implementation-specific address/path rendering, evidence persistence, cleanup ownership, and regression coverage.

Exit gate: both reference builders can complete on the target host, an offline rebuild/reuse check succeeds, environment smoke runs in isolated namespaces, and no secret-bearing state survives. This gate does not require an authenticated cross-router connection.

### Plan 041: Prove the pinned reference environment

Create a genuine reference-only Java I2P/i2pd crosscheck. Establish the exact private network-ID configuration, RouterInfo export/import conventions, readiness signals, and authenticated-link observations for the pinned revisions.

Exit gate: Java I2P and i2pd build from exact revisions, restart from offline cache, exchange RouterInfo, establish at least one authenticated NTCP2 connection in each required direction or under a documented deterministic connection policy, and shut down without residual state.

### Plan 042: Complete the i2pr runtime wire driver

Replace the `blocked_missing_driver` launcher seam with a bounded runtime-owned executor for outgoing and incoming handshake actions, replay checks, timestamps, padding policy, RouterInfo provision and validation, data-frame ownership, I2NP smoke exchange, typed events, and cleanup counters.

Exit gate: all four primary IPv4 paths pass:

- i2pr dials Java I2P;
- Java I2P dials i2pr;
- i2pr dials i2pd;
- i2pd dials i2pr.

Each path must authenticate, exchange the selected bounded I2NP message, and return all runtime counters to baseline.

### Plan 043: Integrate and promote the build-system lane

Refactor the manual Ubuntu workflow into explicit preparation, offline-repeatability, environment-smoke, reference-crosscheck, handshake-smoke, full-matrix, evidence-validation, and cleanup-verification gates. Preserve a lightweight normal CI lane and keep privileged mixed-router execution opt-in until repeated evidence is stable.

Exit gate: the Ubuntu build system produces retained sanitized artifacts for all required gates, with hard failure on missing evidence, leaked state, invalid cleanup, or unsupported support claims.

## Dependency graph

```text
Plan 040 executable apparatus
  -> Plan 041 reference-only proof
       -> Plan 042 i2pr wire driver and mixed-router smoke
            -> Plan 043 build-system promotion and full matrix
                 -> Milestone 3 closure review
```

Do not parallelize Plan 041 or Plan 042 ahead of Plan 040. Otherwise failures in the build, namespace, firewall, cache, or RouterInfo path will be indistinguishable from protocol defects.

Plan 043 workflow scaffolding may be drafted early, but no job may be presented as a passing interoperability gate until the corresponding local/authorized-host command has produced valid evidence.

## Required repository outcomes

The final implementation is expected to modify or extend at least:

```text
scripts/interop/
  ubuntu/check-host.sh
  ubuntu/setup-host.sh
  build-java-i2p.sh
  build-i2pd.sh
  build-references.sh
  run-scenario.sh
  run-matrix.sh
  cleanup.sh
  verify-isolation.sh
  lib/common.sh
  lib/namespaces.sh

tests/integration/ntcp2/
  references.lock.toml
  manifest.toml
  harness/
  config/java-i2p/
  config/i2pd/
  scenarios/
  evidence/README.md

tools/i2pr-interop/
crates/i2pr-runtime/
.github/workflows/ntcp2-interop-ubuntu.yml
```

Add new modules rather than overloading one runner file when responsibilities become distinct. In particular, reference-pair topology, metadata parsing, evidence finalization, and runtime handshake execution should have clear owners and focused tests.

## Evidence contract

No scenario may emit `passed` using placeholder values. A passed record must include:

- exact 40-character i2pr commit;
- exact 40-character reference source commit;
- artifact SHA-256;
- complete installed-tree SHA-256;
- rendered configuration SHA-256;
- canonical namespace/topology SHA-256;
- scenario schema/version and deterministic parameters;
- actual direction and address family;
- authenticated-handshake count;
- bounded I2NP messages sent and received;
- process started/exited/forced counters;
- task, queue, admission, active-link, replay, and backoff counters;
- cleanup result;
- evidence record digest;
- reproduction command that contains no endpoint, identity, key, or private path.

The finalization order must be:

1. stop and join router processes;
2. collect privacy-safe counters;
3. delete namespaces and veth interfaces;
4. derive and validate the sanitized record;
5. write the record under `target/interop/evidence/`;
6. delete the entire secret-bearing run root;
7. verify the run root, namespaces, interfaces, and processes are absent;
8. mark the record passed only if cleanup verification succeeds.

A cleanup failure overrides a protocol success.

## Required validation layers

### Static and unit validation

At minimum:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
bash -n scripts/interop/**/*.sh scripts/interop/*.sh
git diff --check
```

Use repository-compatible shell enumeration if `**` is unavailable in the executing shell.

### Authorized Ubuntu host validation

The final command sequence must be documented and executable from a clean Ubuntu 24.04 amd64 checkout:

```bash
sudo -E bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/build-references.sh --force-rebuild
bash scripts/interop/build-references.sh --offline
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
cargo build --locked --package i2pr-interop
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full
python3 scripts/interop/validate-evidence.py
sudo -E bash scripts/interop/cleanup.sh
```

The implementing agent may refine command names, but the semantic gates must remain distinct.

## Stop conditions

Stop and report a typed blocker rather than weakening the test boundary if any of the following occurs:

- a pinned source revision cannot be resolved to a unique full object ID;
- a reference build requires an unpinned dependency or download;
- the pinned reference cannot be configured for an isolated private network without reseed/bootstrap/public traffic;
- RouterInfo exchange requires copying unvalidated or secret-bearing files into evidence;
- network namespaces, nftables, or required privileges are unavailable on the target lane;
- the reference implementations cannot be configured to share the same test network ID;
- an authenticated result cannot be distinguished from TCP-connect-only readiness;
- cleanup cannot prove removal of all processes, namespaces, interfaces, and run secrets;
- a required protocol behavior would force socket, filesystem, or Tokio ownership into a runtime-neutral protocol crate;
- public-network access occurs during execution.

## Definition of done

This roadmap is complete only when:

- Plans 040 through 043 each have a closure record with exact commits and command evidence;
- the full Ubuntu lane passes from a clean checkout and again from offline reference caches;
- Java I2P and i2pd reference crosscheck passes;
- all four i2pr/reference IPv4 direction gates pass;
- the full negative/resource/duplicate-link matrix produces expected bounded outcomes;
- IPv6 either passes or is explicitly skipped using a validated host/implementation capability result;
- evidence contains no placeholders or forbidden material;
- cleanup verification finds zero residual state;
- `docs/protocol-support.md`, `specs/CONFORMANCE.md`, architecture documentation, and known limitations are updated conservatively;
- a separate Milestone 3 closure review confirms that the MVP roadmap exit criteria are satisfied.

Completion of this roadmap permits a Milestone 3 closure decision. It does not by itself authorize production-readiness claims or public-network testing.
