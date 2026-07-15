# Plan 038: Ubuntu reference-router interoperability harness

## Objective

Create a repeatable Ubuntu-only build and execution environment for controlled NTCP2 interoperability testing against the pinned Java I2P and i2pd reference implementations without contacting the public I2P network.

This plan is a prerequisite for final Milestone 3 closure. It does not begin Milestone 4 and does not enable the normal daemon, capability advertisement, reseeding, RouterInfo publication to the public network, NetDB participation, tunnels, SAM, I2CP, SSU2, or public-network testing.

The completed lane must provide:

- idempotent Ubuntu host setup;
- source-pinned builds of Java I2P and i2pd;
- exact reference artifact and configuration hashes;
- disposable isolated router state;
- a fail-closed Linux network-namespace topology with no default route or public egress;
- implementation-specific configuration generation and RouterInfo exchange;
- a top-level i2pr interoperability launcher that composes the runtime and NTCP2 protocol owners without activating the production daemon;
- deterministic scenario orchestration, bounded process supervision, cleanup, and sanitized evidence;
- executable smoke and full-matrix commands suitable for the Ubuntu build environment;
- a manual Ubuntu CI/workflow lane after the local harness is proven.

## Current state and problem statement

The repository currently contains:

- `tests/integration/ntcp2/manifest.toml`, with pinned versions and eight scenario groups;
- `tests/integration/ntcp2/README.md`, describing the intended manual lane;
- `tests/integration/ntcp2/evidence/README.md`, defining sanitized result records;
- `scripts/check-ntcp2-interoperability.sh`, which validates the manifest and evidence boundary;
- local unit, deterministic testkit, and fuzz evidence.

Those files are a contract and preflight only. They do not install or build the reference routers, create an isolated topology, launch processes, exchange RouterInfo, drive i2pr, execute scenarios, or write results.

The implementation must preserve that distinction until actual mixed-router evidence exists.

## Controlling sources

Use these repository documents as the local baseline:

- `plans/030-milestone-3-overview.md`
- `plans/030-milestone-3-closure.md`
- `plans/036-m3-interoperability-adversarial-validation-closure.md`
- `plans/036-closure.md`
- `plans/037-m3-corrective-integration-closure.md`
- `plans/037-closure.md`
- `tests/integration/ntcp2/manifest.toml`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `docs/architecture.md`
- `GUARDRAILS.md`

Use these pinned upstream sources when implementing setup and build logic:

- Java I2P `2.12.0`, source revision `2800040`.
- i2pd `2.60.0`, source revision `f618e41`.
- Java I2P upstream build guidance requires JDK 17 or newer, Apache Ant, GNU gettext, and a UTF-8 locale; the pinned upstream CI builds with Temurin JDK 17, IzPack 5.2.4, and `ant distclean pkg5`.
- i2pd upstream Ubuntu CI installs `build-essential`, `cmake`, Boost, OpenSSL, and zlib and builds through CMake; the harness must build with UPnP disabled.

Record the exact upstream URLs, revisions, dependency decisions, and build commands in a checked-in lock manifest rather than relying on prose alone.

## Target platform contract

Initial support is intentionally narrow:

- Linux only;
- Ubuntu only;
- x86_64/amd64 only for the first closure;
- `apt` package management;
- Bash 4 or newer;
- Python 3 from Ubuntu for orchestration and structured result writing;
- Linux network namespaces through `iproute2`;
- `sudo` available for package installation and namespace lifecycle;
- no Docker, Podman, Kubernetes, or systemd dependency required for the first implementation.

The scripts must read `/etc/os-release`, `uname -m`, Java, Ant, CMake, compiler, Python, and kernel versions and write them to the run metadata. Unsupported operating systems or architectures must fail before modifying the host.

Do not key behavior to the moving `ubuntu-latest` label. The first CI lane should explicitly select one Ubuntu image, preferably `ubuntu-24.04`, and record the exact image metadata. Additional Ubuntu releases may be added only after separate evidence.

## Architecture decision

Separate the workflow into two security domains:

```text
network-enabled preparation
  -> install Ubuntu packages
  -> fetch pinned source revisions
  -> build reference artifacts
  -> hash and cache artifacts

network-isolated execution
  -> create disposable run root
  -> create two network namespaces
  -> connect them only with a veth pair
  -> verify no default routes or public egress
  -> generate disposable identities/configuration
  -> launch i2pr and one reference router
  -> execute one bounded scenario
  -> sanitize results
  -> kill/drain processes
  -> delete namespaces and secret-bearing state
```

The execution phase must not download dependencies, contact package repositories, use DNS, reseed, bootstrap, or reach any public endpoint.

## Planned repository layout

The implementation should converge on the following layout. Names may be refined, but responsibilities must remain separate.

```text
scripts/interop/
  ubuntu/
    setup-host.sh
    check-host.sh
  build-java-i2p.sh
  build-i2pd.sh
  build-references.sh
  run-scenario.sh
  run-matrix.sh
  cleanup.sh
  verify-isolation.sh
  lib/
    common.sh
    namespaces.sh
    processes.sh
    hashing.sh

tests/integration/ntcp2/
  references.lock.toml
  harness/
    runner.py
    evidence.py
    process.py
    topology.py
    java_i2p.py
    i2pd.py
    i2pr.py
  config/
    java-i2p/
      README.md
      router.config.template
      clients.config.template
    i2pd/
      README.md
      i2pd.conf.template
      tunnels.conf.template
  scenarios/
    smoke-java-ipv4.toml
    smoke-i2pd-ipv4.toml
    reference-crosscheck-ipv4.toml
    ...full manifest scenario files...
  evidence/
    README.md

tools/i2pr-interop/
  Cargo.toml
  src/main.rs
```

Generated source trees, binaries, identities, keys, RouterInfo files, logs, and raw results must live under ignored paths such as:

```text
target/interop/cache/
target/interop/build/
target/interop/runs/
```

No generated secret-bearing file may be committed.

## Phase A: lock manifest and host contract

### Deliverables

Create `tests/integration/ntcp2/references.lock.toml` containing at minimum:

- reference name;
- release version;
- exact source repository;
- exact source revision;
- required build system;
- exact build command;
- required Ubuntu packages;
- expected output paths;
- runtime entrypoint;
- configuration template version;
- whether network access is allowed for build and forbidden for execution;
- artifact hashing policy;
- source and configuration provenance.

Do not commit a fabricated stable artifact hash if the upstream build embeds timestamps or other nondeterministic material. Instead:

- verify the exact checked-out source revision;
- hash the complete produced artifact or install tree after each build;
- record the hash in the run evidence;
- cache by source revision, host contract, and build-command version;
- invalidate the cache if any input changes.

### Host checker

Implement `scripts/interop/ubuntu/check-host.sh` with two modes:

```text
check-host.sh --pre-install
check-host.sh --post-install
```

It must verify:

- Ubuntu through `/etc/os-release`;
- amd64/x86_64;
- `sudo` availability when required;
- UTF-8 locale;
- free disk space and writable `target/interop`;
- required kernel namespace support;
- `ip netns` functionality through a create/delete probe;
- no stale `i2pr-*` namespaces or test processes;
- required commands after setup;
- exact tool versions emitted in machine-readable form.

The namespace probe must leave no persistent namespace or interface if it fails.

## Phase B: idempotent Ubuntu setup

Implement `scripts/interop/ubuntu/setup-host.sh`.

The setup script must:

- use `set -euo pipefail`;
- require Ubuntu before running `apt`;
- run noninteractively;
- install only declared packages;
- be safe to run repeatedly;
- avoid installing or enabling Java I2P/i2pd system services;
- never start either router;
- never modify global firewall policy;
- run the post-install checker;
- print a deterministic summary of installed versions.

Initial package groups should include:

### Common build and harness tools

- `ca-certificates`
- `curl`
- `git`
- `wget`
- `xz-utils`
- `unzip`
- `zip`
- `coreutils`
- `findutils`
- `procps`
- `util-linux`
- `iproute2`
- `nftables`
- `python3`
- `python3-venv`
- `python3-pip` only if a later locked dependency requires it; prefer the standard library

### Java I2P build tools

- `openjdk-17-jdk-headless`
- `ant`
- `gettext`

### i2pd build tools

- `build-essential`
- `cmake`
- `pkg-config`
- `libboost-all-dev`
- `libssl-dev`
- `zlib1g-dev`

Do not install `libminiupnpc-dev` unless the pinned build unexpectedly requires it. The intended i2pd build uses `-DWITH_UPNP=OFF`, and runtime configuration must also disable UPnP/NAT behavior.

The plan must pin and verify the IzPack 5.2.4 installer used by the Java I2P upstream workflow. Add its URL and expected SHA-256 to the lock manifest after independently verifying the artifact. Do not execute an unverified downloaded JAR.

## Phase C: source-pinned reference builds

### Common source-fetch rules

Both build scripts must:

- accept `--offline` and fail if the source/cache is absent;
- accept `--force-rebuild`;
- clone or fetch only the locked repository and revision;
- detach at the exact commit;
- reject dirty or mismatched source trees;
- avoid branch-name assumptions;
- write build logs under the disposable build directory;
- record source commit, tool versions, command line, start/end time, and output hashes;
- never install system services;
- produce a relocatable runtime directory under `target/interop/cache/<reference>/<cache-key>/`;
- make the final cache directory immutable to normal harness execution where practical.

### Java I2P build

Implement `scripts/interop/build-java-i2p.sh` using the pinned upstream path:

1. Verify JDK 17, Ant, gettext, UTF-8 locale, and verified IzPack 5.2.4.
2. Check out revision `2800040`.
3. Generate a local `override.properties` with non-production build metadata and `noExe=true`.
4. Run the source-clean build command based on the pinned upstream workflow, initially `ant distclean pkg5`.
5. Verify `install.jar` exists and hash it.
6. Install it noninteractively into a disposable staging prefix; do not use `/opt`, `/usr`, a user home, or a persistent router directory.
7. Identify the exact headless router launcher and required runtime JARs.
8. Run a version-only/readiness probe that cannot join a network.
9. Copy only required runtime files into the cache directory.
10. Record the installed-tree hash and executable invocation in build metadata.

If `pkg5` proves unsuitable for a minimal headless test artifact, stop and record the exact alternative Ant target selected from the pinned build file. Do not silently switch to an unreviewed target.

### i2pd build

Implement `scripts/interop/build-i2pd.sh`:

1. Verify compiler, CMake, Boost, OpenSSL, and zlib versions.
2. Check out revision `f618e41`.
3. Configure in a separate build directory.
4. Use CMake with at least:

```text
-DWITH_GIT_VERSION=ON
-DWITH_UPNP=OFF
-DCMAKE_BUILD_TYPE=RelWithDebInfo
```

5. Build with a bounded job count derived from available CPUs and memory.
6. Run the upstream unit-test target if one exists at the pinned revision.
7. Verify the binary reports the expected version/revision.
8. Copy the binary and required data files into the cache directory.
9. Hash the binary and complete runtime tree.

Do not use the Ubuntu distro `i2pd` package as interoperability evidence because it may not match the pinned revision.

### Aggregate builder

Implement:

```text
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
```

The aggregate builder must produce a machine-readable summary containing both cache keys and artifact hashes.

## Phase D: fail-closed namespace isolation

Use one i2pr namespace and one reference-router namespace per scenario.

Example topology:

```text
ns-i2pr-<run-id>       ns-ref-<run-id>
    veth-i2pr  <---->  veth-ref
    192.0.2.1/30       192.0.2.2/30
    2001:db8:36::1/64  2001:db8:36::2/64
```

Both veth ends must be moved out of the host namespace. The host must retain no endpoint in the scenario subnet.

Each namespace must contain only:

- loopback;
- the scenario veth interface;
- the directly connected IPv4 route;
- the directly connected IPv6 route when the scenario enables IPv6;
- no default route;
- no DNS route;
- no bridge to the host or Internet.

Implement `scripts/interop/verify-isolation.sh` and call it before launching any router. It must fail unless all of these are true:

- `ip route` has no default route;
- `ip -6 route` has no default route;
- `ip route get 1.1.1.1` fails;
- `ip -6 route get 2606:4700:4700::1111` fails;
- only expected interfaces exist;
- only expected connected routes exist;
- no namespace process is already running;
- the namespace cannot connect to a host/public canary;
- forwarding and route leakage are absent.

Add namespace-scoped nftables output rules as defense in depth. Permit only:

- loopback;
- established traffic;
- the exact peer test addresses and scenario ports;

Reject everything else. The test must not depend on nftables alone; route isolation remains primary.

Every runner must install a shell/Python `finally`/`trap` cleanup that:

- sends graceful termination;
- waits a bounded interval;
- sends SIGKILL if necessary;
- verifies process exit;
- deletes both namespaces;
- removes veth interfaces;
- deletes secret-bearing run state;
- records cleanup counters;
- reports cleanup failure as scenario failure.

## Phase E: disposable router configuration

Create implementation-specific generators in the Python harness.

### Common requirements

Each scenario receives a unique run root containing:

- reference data directory;
- i2pr data directory;
- generated configuration;
- disposable identity and NTCP2 static key material;
- PID files;
- raw bounded logs;
- typed result staging.

All state must be deleted after the sanitized record is written.

The generators must enforce:

- no reseed;
- no bootstrap peers;
- no public peer sources;
- no automatic address discovery;
- no UPnP or NAT mapping;
- no SSU/SSU2 for NTCP2-only scenarios;
- no transit participation where the implementation supports disabling it;
- no floodfill role;
- no client tunnels or proxy listeners beyond implementation-required local management endpoints;
- literal namespace addresses only;
- fixed explicit NTCP2 ports;
- bounded memory and bandwidth settings;
- all consoles/APIs bound only inside the namespace or disabled;
- no persistent update checks.

Do not guess configuration property names. For each property:

1. locate it in the pinned source or official sample configuration;
2. record the source path and meaning in the template README;
3. add a generator assertion that the rendered configuration contains the intended value;
4. add a startup-log/readiness assertion proving the reference accepted it;
5. stop if the implementation ignores or rewrites a safety-critical setting.

### RouterInfo exchange

The harness must implement explicit, disposable peer introduction without reseed or public NetDB participation.

Required flow:

1. Generate/start the first participant far enough to produce its signed RouterInfo.
2. Validate the RouterInfo locally and extract only the test address required by the scenario.
3. Transfer the RouterInfo through the run directory, not through a public service.
4. Import it into the peer through a documented implementation-specific test path.
5. Repeat in the opposite direction where required.
6. Hash the imported RouterInfo for ephemeral run correlation.
7. Do not copy RouterInfo bytes into committed evidence.

Implement separate adapters for Java I2P and i2pd. The exact NetDB import path or direct-peer mechanism must be derived from the pinned implementations and covered by a smoke test. If either implementation cannot be safely seeded with a single synthetic peer, stop and document the required upstream-supported mechanism rather than enabling reseed.

## Phase F: reference-router process adapters

Each reference adapter must expose a common internal interface:

```text
prepare(run_context)
start(run_context)
wait_ready(deadline)
export_router_info()
import_peer_router_info(path)
query_typed_state()
stop(deadline)
collect_sanitized_result()
```

### Java I2P adapter

The Java adapter must:

- invoke the staged headless runtime, never a system service;
- set an explicit router/config directory;
- set bounded JVM memory;
- disable update/reseed/bootstrap behavior through verified configuration;
- expose a readiness condition derived from a stable local state or bounded log token;
- locate the generated RouterInfo deterministically;
- retain only bounded raw logs until sanitation;
- classify process exit, timeout, configuration rejection, and transport readiness separately.

### i2pd adapter

The i2pd adapter must:

- invoke the staged binary directly;
- use explicit `--datadir` and configuration paths;
- run in the foreground;
- disable daemonization, UPnP, NAT, reseed/bootstrap, SSU/SSU2, and unrelated services;
- expose a stable readiness condition;
- locate/export the generated RouterInfo deterministically;
- classify process exit, timeout, configuration rejection, and transport readiness separately.

Both adapters must treat unexpected outbound connection attempts, ignored safety configuration, or public endpoints in generated RouterInfo as immediate test failures.

## Phase G: i2pr interoperability launcher

Do not enable the normal daemon. Add a dedicated non-production launcher under `tools/i2pr-interop` or an equivalently isolated root crate.

The launcher exists to solve the approved composition boundary:

```text
i2pr-interop
  -> i2pr-runtime owns Tokio, sockets, deadlines, tasks, namespaces-visible endpoints
  -> i2pr-transport-ntcp2 owns handshake/data state
  -> i2pr-transport owns manager/resource contracts
  -> i2pr-proto/i2pr-crypto/i2pr-storage provide protocol material
```

It must not become a production dependency of any router crate and must not be installed by default.

Required command surface:

```text
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

Required behavior:

- consume disposable identity/static-key material from the run directory;
- produce a signed local RouterInfo for the synthetic endpoint;
- drive the complete bounded inbound or outbound handshake under one total deadline;
- transition into authenticated data-phase ownership;
- send and receive a bounded synthetic I2NP test message;
- expose only typed JSON lines on stdout;
- never print keys, RouterInfo bytes, endpoint strings, payload bytes, or arbitrary remote errors;
- retain inbound admission through authentication;
- use replay, duplicate, manager, active-link, queue, and deadline owners end to end;
- exit after one scenario rather than acting as a general router;
- join every child and report final zero/expected counters.

This phase may consume and complete Plan 037 Track F. Any required crate-boundary change must be documented in an ADR and preserve `i2pr-runtime` as the sole socket/Tokio owner.

## Phase H: scenario runner

Implement `tests/integration/ntcp2/harness/runner.py` using the Python standard library unless a dependency is unavoidable and locked.

The runner must accept:

```text
--scenario <id>
--reference java_i2p|i2pd
--build-cache <path>
--run-root <path>
--keep-failed-sanitized
--offline
--verbose-typed
```

It must not offer an option to disable isolation.

### Scenario lifecycle

For every scenario:

1. Validate host and cache.
2. Allocate a unique run ID.
3. Create the run root with restrictive permissions.
4. Create namespaces and veth pair.
5. Apply exact addresses and firewall rules.
6. Run isolation preflight.
7. Render and hash configurations.
8. Generate disposable identities/keys.
9. Start the required participant order.
10. Exchange RouterInfo through the approved adapter.
11. Wait for bounded readiness.
12. Execute initiator/responder action.
13. Exchange one or more bounded synthetic I2NP messages where required.
14. Execute scenario-specific mutation, delay, resource, or duplicate behavior.
15. Capture typed outcomes and resource snapshots.
16. Stop and drain all participants.
17. Verify zero/expected task, queue, permit, link, and process counters.
18. Hash raw ephemeral artifacts.
19. Emit the sanitized evidence record.
20. Delete raw logs, RouterInfo, identities, keys, configurations, and namespaces.

### Initial execution profiles

Implement profiles in this order:

#### Profile 1: environment smoke

- Java I2P starts in isolation, produces a synthetic RouterInfo, performs no public connection, and shuts down cleanly.
- i2pd starts in isolation, produces a synthetic RouterInfo, performs no public connection, and shuts down cleanly.
- Repeat each five times.

#### Profile 2: reference crosscheck

- Java I2P and i2pd connect to each other over isolated NTCP2 using explicit RouterInfo exchange.
- Validate the harness independently of i2pr.
- Run both dial directions if the references permit deterministic role control.
- No claim about i2pr follows from this profile.

#### Profile 3: i2pr handshake smoke

- i2pr initiator to Java I2P.
- Java I2P initiator to i2pr.
- i2pr initiator to i2pd.
- i2pd initiator to i2pr.
- IPv4 first.
- One authenticated bounded I2NP exchange per direction.

#### Profile 4: full manifest

Execute the existing eight scenario groups, including applicable IPv6, padding boundaries, skew/replay/identity/network failures, partial/coalesced I/O, duplicate-link races, slow input, oversized/mutated input, queue/resource saturation, and cleanup.

Do not begin adversarial profiles until positive handshake/data smoke passes for both implementations and directions.

## Phase I: evidence and sanitation

Retain only the schema defined in `tests/integration/ntcp2/evidence/README.md`, extended as necessary with:

- Ubuntu version and image identifier;
- kernel and architecture;
- Java/Ant/IzPack versions;
- compiler/CMake/Boost/OpenSSL versions;
- Python and Rust toolchain versions;
- source revisions;
- reference artifact hashes;
- installed-tree hashes;
- i2pr commit;
- configuration hashes;
- namespace topology hash;
- scenario ID and deterministic parameters;
- typed outcome;
- final resource/process counters;
- cleanup result;
- sanitized evidence hash;
- exact reproduction command.

Raw artifacts may exist only inside the disposable run root during execution. The sanitizer must reject output containing:

- private-key markers;
- router identity files;
- NTCP2 static keys;
- RouterInfo bytes or base64 values;
- I2NP payload bytes;
- packet captures;
- raw IP/port endpoint diagnostics;
- arbitrary reference-router log lines;
- environment secrets;
- home-directory paths or usernames.

Add a mechanical evidence validator and extend `scripts/check-ntcp2-interoperability.sh` to validate real records without interpreting missing records as success.

## Phase J: commands and developer workflow

The completed developer workflow should be:

```text
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/build-references.sh
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full
```

Offline repeatability must be demonstrated with:

```text
bash scripts/interop/build-references.sh --offline
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke --offline
```

The test runner must never require the repository checkout or build cache to be writable by root. Use a narrowly scoped privileged helper for namespace operations or preserve caller ownership explicitly. Do not leave root-owned files in the checkout.

## Phase K: CI integration

Normal pull-request CI should continue to run only cheap, nonprivileged checks:

- shell syntax/lint where available;
- Python syntax and unit tests;
- lock-manifest validation;
- configuration-template validation;
- evidence sanitation checks;
- scenario-schema checks;
- no committed generated artifacts;
- no forbidden public-network options.

Add a separate Ubuntu manual workflow after local proof:

```text
.github/workflows/ntcp2-interop-ubuntu.yml
```

Initial workflow requirements:

- `workflow_dispatch` only;
- explicit `ubuntu-24.04` runner rather than `ubuntu-latest`;
- least-privilege repository permissions;
- package setup through the checked-in script;
- source/cache keys derived from the lock manifest;
- no repository or cloud secrets;
- environment-smoke and handshake-smoke profiles first;
- full matrix enabled only after stable smoke evidence;
- bounded job timeout;
- `always()` cleanup step;
- artifact upload limited to sanitized evidence and build metadata;
- no raw logs, identities, keys, RouterInfo, packet captures, or run directories uploaded.

A failed cleanup, isolation check, or sanitizer must fail the workflow even if the protocol scenario passed.

## Required tests

### Setup/build tests

- setup script is idempotent;
- unsupported OS/architecture fails before `apt`;
- exact source revisions are verified;
- source mismatch or dirty source fails;
- unverified IzPack artifact fails;
- offline build uses cache only;
- missing offline cache fails cleanly;
- corrupted cached artifact fails hash verification;
- both reference version probes match the lock manifest.

### Isolation tests

- namespaces contain no default route;
- public IPv4 and IPv6 route probes fail;
- host namespace has no scenario endpoint;
- DNS/public connect attempts fail;
- only expected veth traffic is allowed;
- stale namespace detection works;
- SIGINT/SIGTERM/error cleanup removes namespaces and processes;
- forced-kill cleanup is recorded;
- 100 create/start/teardown iterations leave no namespace, process, interface, or run directory.

### Configuration tests

- every safety-critical option is derived from pinned source documentation;
- generated configs contain no public peer/reseed/update source;
- generated RouterInfo contains only the synthetic endpoint;
- SSU/SSU2 is absent or disabled for NTCP2 scenarios;
- UPnP/NAT discovery is disabled;
- identities and static keys differ between scenarios;
- configurations and state are deleted after sanitation.

### Process-adapter tests

- readiness success;
- startup timeout;
- immediate process exit;
- malformed config;
- ignored safety option;
- graceful stop;
- forced stop;
- bounded log handling;
- no arbitrary log text reaches committed evidence.

### End-to-end tests

- each reference environment-smoke profile passes five repetitions;
- Java/i2pd reference crosscheck proves the topology and RouterInfo exchange path;
- i2pr handshake smoke passes both directions against both references;
- bounded I2NP exchange succeeds;
- queue/task/permit/link/process counters return to zero or an explicitly expected value;
- offline rerun succeeds from the prepared cache;
- no process opens a public route or endpoint.

## Documentation updates

Update:

- `tests/integration/ntcp2/README.md` with exact setup, build, run, cleanup, and troubleshooting commands;
- `tests/integration/ntcp2/evidence/README.md` with the final record schema;
- `docs/private-testnet.md` with the namespace topology and two-phase network policy;
- `docs/security-model.md` with test-harness privileges and artifact sanitation;
- `docs/architecture/tooling.md` with harness components;
- `AGENTS.md` and `CONTRIBUTING.md` with the Ubuntu-only scope and prohibited public testing;
- `specs/CONFORMANCE.md` with the distinction between environment smoke, reference crosscheck, i2pr mixed-router evidence, and public support claims.

Do not mark NTCP2 supported or advertised as part of harness setup alone.

## Closure record

Create `plans/038-closure.md` containing:

- exact commits and changed files;
- Ubuntu version/image and architecture;
- installed package/tool versions;
- reference source revisions and artifact hashes;
- exact build commands and cache keys;
- namespace topology and isolation proof;
- configuration-source inventory for every safety-critical option;
- process-adapter readiness and teardown evidence;
- environment-smoke results;
- reference crosscheck results;
- i2pr handshake-smoke results if Track F is complete;
- offline repeatability result;
- local test commands and results;
- manual CI run ID and job results if the workflow is enabled;
- sanitized evidence file names and hashes;
- deviations, unresolved blockers, and explicit Milestone 3 disposition.

If the complete i2pr adapter is not yet available, Plan 038 may close only as an environment/harness foundation. In that case the closure must remain explicit that mixed-router i2pr evidence is blocked and Milestone 3 remains open.

## Acceptance criteria

Plan 038 is complete only when:

1. Ubuntu setup is idempotent and verified.
2. Java I2P and i2pd build from exact pinned revisions.
3. Build artifacts and configurations are hashed and reproducible from the lock manifest.
4. Execution works offline after preparation.
5. Each router can start, produce a synthetic RouterInfo, and stop in a network namespace with no public route.
6. Isolation checks fail closed before process launch.
7. Cleanup removes all namespaces, interfaces, processes, identities, keys, raw logs, and run state.
8. A reference crosscheck validates RouterInfo exchange and NTCP2 connectivity between Java I2P and i2pd, or a precisely documented implementation limitation blocks it.
9. The i2pr interoperability launcher exists without enabling the normal daemon.
10. Positive inbound/outbound handshake and bounded I2NP smoke cases pass against both references.
11. Sanitized evidence contains exact hashes and typed outcomes only.
12. Normal CI validates the harness boundary, and a separate Ubuntu manual lane records a fresh successful smoke run.
13. `plans/038-closure.md` exists and accurately states remaining blockers.
14. Support metadata remains experimental and non-advertised until the full Plan 036 matrix passes.

## Stop conditions

Stop and document rather than weakening the design if:

- the Ubuntu environment cannot create isolated network namespaces;
- either namespace has a default/public route;
- a reference router cannot disable reseed/bootstrap/update/public peer discovery;
- a reference ignores the explicit synthetic endpoint;
- a safe one-peer RouterInfo import path cannot be established;
- the build cannot be pinned to the required revision;
- reference artifacts cannot be distinguished or hashed reliably;
- Java I2P requires an unverified executable dependency;
- i2pd requires UPnP/NAT support for the selected build path;
- cleanup cannot reliably terminate all processes and remove namespaces;
- the sanitizer cannot guarantee exclusion of keys, identities, RouterInfo, payloads, or raw endpoints;
- completion would require enabling the production daemon or public-network behavior;
- the adapter would move sockets/Tokio into a lower crate or cryptographic protocol logic into runtime.

## Milestone 3 gate

Building and starting the references is necessary but not sufficient for Milestone 3 closure.

Milestone 3 remains blocked until the harness records reproducible Java I2P and i2pd results for:

- i2pr as initiator and responder;
- authenticated NTCP2 handshakes;
- bounded bidirectional I2NP exchange;
- duplicate-link stability;
- replay, skew, identity, network, padding, malformed, slow-peer, and resource cases;
- zero/expected cleanup counters;
- exact source, artifact, configuration, evidence, and CI/manual run identifiers.

Only after those results exist may `plans/030-milestone-3-closure.md` be rewritten from blocked to complete and Milestone 4 planning begin.
