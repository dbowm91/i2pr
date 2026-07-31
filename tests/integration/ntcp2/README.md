# Plan 038/040/041/043/044 Ubuntu reference-router interoperability harness

This is a manual, opt-in integration path. It is separate from normal
workspace tests and is restricted to Ubuntu 24.04 amd64. The harness is not a
public bootstrap configuration, does not enable `i2pr-daemon`, and does not
advertise NTCP2.

The pinned targets are Java I2P 2.12.0 at revision
`2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd 2.60.0 at revision
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. Their source URLs, build commands, package set, and
verified IzPack 5.2.4 hash are in [`references.lock.toml`](references.lock.toml).
Build hashes are recorded per build; no nondeterministic stable artifact hash
is fabricated.

## Preparation

Preparation is the only phase allowed to install packages or fetch sources.
Run it from the repository root on a disposable Ubuntu host:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
```

The setup script installs only the declared packages, never enables a router
service, and is safe to repeat. The builders clone/fetch only the locked
repositories, detach at the exact revisions, reject dirty or mismatched source
trees, and write cache/build metadata below `target/interop/`. Cache lookup
uses the canonical `java_i2p` and `i2pd` identifiers plus
`target/interop/cache/current-cache.json`; it never scans arbitrary metadata.

Offline repeatability uses only an already prepared cache:

```text
bash scripts/interop/build-references.sh --offline
```

## Isolated execution

The Plan 038/040 i2pr/reference scenarios create one `i2pr-*` namespace and
one `ref-*` namespace. Plan 041 uses a separate reference-pair owner and
creates `java-<short-run-id>` and `i2pd-<short-run-id>` namespaces. Both veth
endpoints leave the host namespace. The only allowed path is the directly
connected synthetic peer subnet; default routes, DNS, host bridges, public
egress, reseed, bootstrap, NAT/UPnP, SSU/SSU2, and unrelated client services
are forbidden. Route checks are primary and namespace-scoped nftables rules
are defense in depth.

Run a bounded scenario with the reference cache and optional explicit paths:

```text
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4 --reference java_i2p
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4 --reference i2pd
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full
```

`environment-smoke` validates reference startup, disposable RouterInfo
production, and cleanup only. `reference-crosscheck-ipv4` runs the two dedicated
Plan 041 scenarios, `reference-java-i2pd-ipv4` and
`reference-i2pd-java-ipv4`. It requires both offline caches, the explicit
private network ID 99, strict RouterInfo validation, one-way firewall policy,
and authoritative authenticated observations from both routers. It is a
reference-only control and is not i2pr evidence. The handshake/full profiles remain
`blocked` with reason `i2pr-mixed-router-profile-not-wired` while the reference
runner's i2pr/reference topology is incomplete; this is an explicit blocker,
not a skipped success. The launcher itself now has a bounded local
listener/dial, handshake, authenticated-link, and DeliveryStatus path.

The dedicated launcher seam is separate from the normal daemon:

```text
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

It emits versioned typed JSON only. `listen` emits listener readiness followed
by one terminal typed result, and `dial` emits one terminal typed result.
`inspect` delegates
RouterInfo structural, signature, and NTCP2-address validation to the
repository's strict Rust parser. The reference-pair runner uses this
inspection only inside a deleted run root and never treats it as mixed-router
i2pr evidence.

## Cleanup and evidence

Every runner path stops and drains children, deletes both namespaces and veth
state, removes identities, keys, RouterInfo, configs, raw logs, and run roots,
and treats cleanup failure as scenario failure. Emergency cleanup is:

```text
sudo -E bash scripts/interop/cleanup.sh
```

Plan 041 reference-pair runs hold a host-local lock so directional runs cannot
overlap. Emergency cleanup also owns their `java-*`/`i2pd-*` namespaces and
short `jv…`/`iv…` veth names.

Only sanitized JSON records containing typed outcomes and hashes may be
retained under `target/interop/evidence/`; secret-bearing run roots are always
deleted. Validate records with:

```text
bash scripts/interop/validate-evidence.py
bash scripts/check-ntcp2-interoperability.sh
```

An empty evidence directory is reported as “no evidence”, never as success.
Local testkit, loopback, vectors, and fuzz results remain useful local
evidence but cannot satisfy the two-reference, two-direction requirement.

Plan 041 pair records use schema 2 and retain only both reference revisions,
artifact/tree/configuration hashes, a topology hash, typed RouterInfo and
authenticated-link observations, bounded counters, direction policy, cleanup
result, digest, and reproduction command. They never retain raw RouterInfo,
identities, keys, endpoints, or logs.

## Troubleshooting

- A host or namespace error is fail-closed; run the pre/post checker and fix
  Ubuntu, amd64, UTF-8 locale, `sudo`, `iproute2`, or kernel namespace support.
- `blocked` with reason `i2pr-mixed-router-profile-not-wired` means the
  reference runner is not yet connected to the i2pr launcher. Do not replace it
  with a self-handshake or treat local launcher success as reference evidence.
- `blocked_host_contract` means execution did not start and no protocol claim
  may be inferred.
- Inspect only disposable local build metadata. Never retain raw logs, packet
  captures, RouterInfo, identities, keys, endpoint diagnostics, or payloads.

## Plan 043 build-system lane

The build-system lane is a manual, opt-in Ubuntu job. Its phases are ordered;
the next phase cannot promote a result when the preceding artifact or evidence
is absent or invalid. Cleanup is the exception: it runs after every privileged
phase and at the end even when an earlier phase fails.

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

The contract phase does not start routers. It runs the repository's locked Rust
checks, dependency/runtime boundary checks, `check-ntcp2-interoperability.sh`,
and the Python harness unit tests. The reference-build phase is the only
network-enabled phase and uses `setup-host.sh` plus
`build-references.sh --force-rebuild`. The offline-reuse phase restores a
verified cache and runs `build-references.sh --offline`; a cache miss, digest
mismatch, or attempted network operation is a hard failure.

The exact workflow command surface is:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo -E bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
sudo -E bash scripts/interop/verify-clean-host.sh --record-baseline
bash scripts/interop/build-references.sh --force-rebuild
python3 scripts/interop/cache-manifest.py --verify
bash scripts/interop/offline-reuse.sh
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke --offline
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4 --offline
cargo +1.95.0 build --locked --package i2pr-interop
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke --offline
sudo -E bash scripts/interop/run-matrix.sh --profile full --offline
bash scripts/interop/validate-evidence.py
python3 scripts/interop/aggregate-evidence.py --profile <profile>
bash scripts/check-ntcp2-interoperability.sh
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify --baseline target/interop/build/clean-host-baseline.json
```

`--offline` is required for execution and cache reuse, not preparation. The
workflow must additionally enforce network denial after cache restoration; a
flag alone is not evidence of offline execution. Do not supply arbitrary
shell fragments, source URLs, revisions, endpoints, network IDs, or paths as
profile inputs.

`reference-crosscheck-ipv4` is a control gate only. It runs the separate
`reference-java-i2pd-ipv4` and `reference-i2pd-java-ipv4` scenarios with
private network ID 99, strict RouterInfo validation/import, and dual
authenticated observations. The i2pr gate is not eligible to run unless that
control passes. The four i2pr/reference IPv4 directions must each independently
show authenticated handshake, strict binding, bounded DeliveryStatus exchange,
typed counters, sanitized finalization, and clean state. The full profile adds
bounded malformed, replay, deadline, resource, race, cancellation, and
failure-cleanup cases; it does not run unbounded fuzzing.

The retained aggregate manifest must include its schema, i2pr commit, workflow
run/attempt, host and lock digests, cache keys/tree hashes, expected scenario
IDs, actual record filenames and SHA-256 values, per-gate dispositions, and
cleanup-verification disposition/digest. Validation rejects missing or
unexpected passed records, placeholders, inconsistent hashes, incomplete
directions, forbidden material, and non-clean cleanup. Upload only the narrow
allowlist documented in `evidence/README.md`.

The workflow and helper apparatus now expose the ordered manual Plan 043 lane,
including clean-host verification and aggregate validation. The current
checkout has no completed successful aggregate run or mixed-router i2pr record.
These are explicit implementation blockers, not skipped successes. NTCP2
remains experimental and non-advertised.

## Plan 044 mixed-router scenarios

Plan 044 adds four directional mixed-scenario definitions under
`tests/integration/ntcp2/mixed-scenarios/`:

```text
i2pr-to-java-ipv4    (i2pr initiates, Java I2P responds)
java-to-i2pr-ipv4    (Java I2P initiates, i2pr responds)
i2pr-to-i2pd-ipv4    (i2pr initiates, i2pd responds)
i2pd-to-i2pr-ipv4    (i2pd initiates, i2pr responds)
```

Each direction has a unique execution ID, one declared initiator and responder,
one terminal typed result, and one evidence record. No direction may mask
another.

The mixed runner composes `I2prAdapter` with each reference adapter through a
strict launcher scenario renderer. The renderer populates the exact launcher
schema with execution-specific scenario ID, role, address family, synthetic
endpoints, private network ID 99, confined state directory, deadlines, padding
profile, smoke-message profile, and expected-result class. The renderer
rejects absolute paths, parent traversal, endpoints outside synthetic namespace
ranges, mismatched address families, missing peer data for initiators, peer
data for responders, and unknown fields.

The data-phase oracle does not rely on an echo assumption. It uses a
protocol-valid trigger supported by both pinned references. Evidence records
carry real counters for authenticated-link count, frames sent/received, I2NP
message aggregates, admission/replay counters, process lifecycle counters,
and cleanup disposition.

Gate archival uses gate-specific staging to prevent cross-gate record
relabeling. The aggregate manifest must include exactly the expected records
for the selected profile; missing, extra, mislabeled, or zero-valued records
fail the gate.

The current checkout contains the mixed-scenario definitions, the mixed-runner
composition, the strict launcher renderer, and the non-echo data-phase oracle.
No completed mixed-router i2pr record is present; these are explicit blockers,
not skipped successes. NTCP2 remains experimental and non-advertised.

## Plan 046 rootless sealed-namespace evidence lane

Plan 046 is closed with a typed host-level blocker. The lane is a
process-scoped user/network/mount/PID namespace that an ordinary user can
run without `sudo`, passwordless elevation, host capabilities, setuid helpers,
host-visible namespaces, host-visible veths, or host nftables mutation. The
primary evidence topology is `rootless-sealed-single-netns` with privilege
model `unprivileged-userns`; the legacy `privileged-dual-netns-veth`
backend is renamed, kept as an explicit opt-in qualification lane, and
never a silent fallback.

The sandbox capability probe (`scripts/interop/probe-rootless-sandbox.sh`)
emits a typed outcome, and on hosts that refuse unprivileged user
namespaces the wrapper writes the canonical typed blocker
`blocked_unprivileged_user_namespace` to the `--attestation-output` path.
Mixed-router evidence records carry `topology_kind`, `privilege_model`,
`sandbox_attestation_sha256`, and `parent_network_state_unchanged`. A
passed record that violates any of these is rejected.

The current host (`deadpool`, Ubuntu 24.04 amd64) activates
`kernel.apparmor_restrict_unprivileged_userns=1`, which confines every
unprivileged user namespace to a restrictive AppArmor policy even though
`kernel.unprivileged_userns_clone=1`. The ordinary invoking user has no
`CAP_MAC_ADMIN` and no other lever to lift that policy, and Plan 046
forbids `sudo`. The probe (host shell and `ssh i2ptest@localhost`) emits
the typed blocker; the on-host evidence directory
`target/interop/evidence/handshake-smoke-rootless--host-blocked/`
carries that blocker plus a kernel/sysctl/capability snapshot. Plan 047
(`plans/047-cross-host-rootless-lane-expansion.md`) records cross-host
recovery for hosts where the AppArmor restriction is `0` (or AppArmor is
unloaded).

The static boundary checker (`scripts/check-rootless-interop-boundary.sh`)
enforces the privilege surface independently of the host-level kernel
policy: it forbids `sudo`, `ip netns`, `nft`, `setcap`, `--privileged`,
`--network host`, and any fallback to the privileged backend from the
rootless-owned files. The Plan 046 manual no-escalation GitHub Actions
workflow (`.github/workflows/ntcp2-interop-rootless.yml`) is opt-in only.
Plan 046 does not advertise NTCP2 support and does not close Milestone 3.

## Plan 048/049 Multipass recovery environment

The host blocker is intentionally retained. On a compatible host, the
disposable Multipass lane uses a stable environment ID separate from each safe
run ID and concrete instance generation. The default path allocates a fresh
collision-resistant instance name; the legacy `i2pr-interop-rootless` name is
not authoritative. Use the lane as follows:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --all
bash scripts/interop/multipass/run-evidence-lane.sh --all \
  --run-id plan049-example --destroy-after-export
bash scripts/interop/multipass/run-evidence-lane.sh --inspect --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --resume-owned \
  --run-id <run-id>
```

The checked-in environment manifest fixes the Ubuntu 24.04 amd64 image,
reviewed environment contract, guest-only sysctls, and
`/home/i2ptest/i2pr/target/interop/cache`. Preparation transfers an exact
source archive and verified cache. The host baseline probe is recorded but does
not gate the guest. Ownership and guest policy must pass, then `probe.sh` must
return `rootless_sandbox_available` both before expensive execution and again
before any router process. Only then may `prepare-offline.sh` and the fixed
four-direction matrix run as `i2ptest`.

Lifecycle state is reserved atomically before launch and protected by a
per-run/per-instance lock. `--inspect` is read-only; `--adopt-owned`,
`--resume-owned`, `--recreate-owned`, and `--destroy-owned` require a complete
host/guest ownership proof. Name-only matches, unowned collisions, unknown
states, contract mismatches, and deleted-but-unpurged instances are typed
blockers. No normal path silently adopts, recreates, deletes, or globally
purges an instance. Recreated instances increment the generation, and snapshots
are bound to that generation and the environment contract.

The exporter atomically validates and places only sanitized evidence under
`target/interop/evidence/multipass/<run-id>/`; VM destruction preserves it.
Every directional record identifies the environment ID, run ID, instance
generation, ownership/contract digests, separate host and guest probe outcomes,
and the environment evidence hash. Mixed run IDs or generations are rejected.
Pre-router failures are written as sanitized environment blockers and cannot
satisfy protocol conformance. Multipass, guest policy, offline, cleanup, and
evidence failures are typed blockers, not protocol passes.

## Plan 052/053 evidence-pipeline integration

Plan 052 introduces a single-source run identity, observation schema v2,
atomic bundle layout, a standalone Java startup probe, and a reference
observation catalog. Plan 053 wires those primitives into the canonical
rootless and Multipass execution path through
`tests/integration/ntcp2/harness/plan052_pipeline.py`.

`plan052_pipeline.py` is the single owner for measured run identity
creation, freeze checks, bound per-direction artifact classes, diagnostic
finalization, and atomic export. The four primary directions
(`i2pr-to-java-ipv4`, `java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`,
`i2pd-to-i2pr-ipv4`) are emitted as five classes each
(`attestation/`, `direction/`, `trigger/`, `observation/`, `cleanup/`)
regardless of whether execution is blocked or rejected. Missing
source-locked reference receiver markers are typed
`reference-receiver-marker-not-source-locked` rejections; they are never
masked by `not-applicable` or zero-filled provenance.

Bundles are staged under
`target/interop/evidence/milestone-3/<run-id>/`. Export writes the
acknowledgement beside the bundle using a bundle-relative path. The
local accepted result is `diagnostic-complete-not-certificate`; it is
not a Milestone 3 certificate and does not advertise NTCP2.

External callers must pass `--run-id`, `--run-identity`,
`--bundle-staging`, and `--evidence-profile milestone-3-v2` explicitly
through every launcher boundary (`dispatch-gate.sh`,
`run-direction.sh`, `rootless-enter.sh`, `rootless_inner_runner.py`,
`mixed_runner.py`). The Plan 053 profile rejects legacy unbound
direction records. Static enforcement lives in
`scripts/check-ntcp2-interoperability.sh`, with focused tests under
`tests/integration/ntcp2/harness/test_plan053.py` and
`test_evidence_bundle.py`.

## Plan 054 Java startup and reference-observation qualification

Plan 054 closes the two Plan 052 evidence gates that depend on a live
Java reference and a per-side observation marker. The new artifacts are:

- `tests/integration/ntcp2/harness/java_matrix.py` — 16-cell matrix
  driver (`namespace × data-state × launcher × sequence`) with three
  isolated attempts per cell and a 10-consecutive-start
  qualification.
- `tests/integration/ntcp2/harness/java_startup_probe.py` — extended
  with the `seeded-clone` data state, the bounded entropy probe
  (`getrandom_result`, `latency_bucket_ms`, `seed_file_state`,
  `seed_file_sha256`), and the twelve typed failure stages
  (`java-process-spawn-failed` through `java-state-lock-invalid`).
- `tests/integration/ntcp2/reference-observation-catalog.toml` —
  machine-readable catalog
  (`i2pr-reference-observation-catalog-v1`) binding every marker to
  its exact source path, symbol, marker text, sanitization rule, and
  minimum count. The Markdown companion
  (`reference-observation-catalog.md`) is now drift-checked
  documentation.
- `tests/integration/ntcp2/harness/observation_catalog.py` — TOML
  load, validation, drift detection, and exact-match helpers.
- `tests/integration/ntcp2/harness/observation_helpers.py` —
  `LogCursor`, exact-match scanner, `build_observation()`, and the
  shared `sanitize_marker_line()`.
- `scripts/interop/java-prepare-template.py` — preparation-phase
  command that builds the frozen Java template and writes
  `template-manifest.json` plus `template-tree.sha256`. The execution
  phase is restricted to `seeded-clone` clones.

The Java and i2pd adapters expose
`collect_observation(role, run_id, correlation, log_cursor, catalog)`
and return finalized `i2pr-ntcp2-direction-observation-v2` records.
`mixed_runner._evaluate_plan052_predicate` now applies the
`receiver_passes_data_phase` predicate against those records and is
no longer hardcoded to reject every direction. The Plan 053 pipeline
accepts the live records through
`write_direction_artifacts(..., i2pr_observation=...,
reference_observation=...)`; the synthetic builder remains the typed
fallback for blocked and rejected directions.

External qualification (the complete 48-start matrix, the ten
consecutive rootless starts, and the seven control experiments)
requires the pinned Java 2.12.0 and i2pd 2.60.0 references on an
authorized Ubuntu 24.04 amd64 host or Multipass guest. The local
checkout on the `apparmor_restrict_on` Plan 046 negative baseline
cannot exercise the matrix yet; the Plan 048/049 Multipass recovery
lane is the canonical external path. Plan 054 does not close
Milestone 3. Plan 054 status is in `plans/054-status.md`.

Focused checks:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 063 Java I2P stripped-router direct NTCP2 driver

Plan 063 implements the source-locked Java I2P 2.12.0 stripped-router
direct NTCP2 driver. The driver is **test-only** and never becomes a
production dependency of `i2pr-daemon`. It uses the upstream
embedded `net.i2p.router.Router` and `RouterContext` with the pinned
dummy facades, the real NTCP/NTCP2 transport, the real outbound
message pool, and the real inbound message pool. Plan 063 does not
patch NTCP2 cryptography, the Noise handshake, framing, or RouterInfo
signature verification.

The Plan 063 deliverables are committed under
`tests/integration/ntcp2/reference-drivers/java/`:

- `src/JavaNtcp2InteropDriver.java` — the source-locked Java driver
  with strict config validation, bounded `inspect`/`listen`/`dial`
  modes, real `OutNetMessage` DeliveryStatus submission, and
  structured `i2pr-reference-event-v1` emission.
- `source-lock.json` — the source-lock record
  (`i2pr-java-helper-source-lock-v1`) binding the pinned Java
  revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`, the helper
  source path, the build contract, and the locked constraints.
- `classpath-manifest.json` — the runtime classpath binding every
  pinned jar in `target/interop/cache/java_i2p/<tree>/lib/` to its
  purpose.
- `build-manifest.schema.json` — the build-manifest schema
  (`i2pr-java-helper-build-manifest-v1`).
- `build-driver.sh` and `run-driver.sh` — the offline build and
  runtime seams.
- `README.md` — the driver README documenting the call graph, the
  strict config contract, and the Plan 063 constraints.

The harness adapter and test matrix live under
`tests/integration/ntcp2/harness/`:

- `java_direct_driver.py` — the Python harness adapter that binds
  every helper invocation into a Plan 062 v4 trigger record
  (`i2pr-reference-trigger-v4`) and validates the Plan 063 strict
  driver config contract.
- `test_java_direct_driver.py` and `test_java_direct_control.py` —
  the Plan 063 test matrix covering the source-verification
  contract, strict config contract, Python harness adapter,
  structured event contract, and the local inspect-mode round-trip
  where the pinned Java cache is available.

The qualification receipt lives at
`tests/integration/ntcp2/qualification/java-direct-driver.json`
(schema `i2pr-java-direct-driver-qualification-v1`). On this host
the receipt records the typed host-environment blocker
(`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
qualification remains to be produced in the Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass recovery lane.

The Plan 063 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 063 topology contract section.

Plan 063 does not wire the Java driver into the canonical primary
`mixed_runner.py`; that wiring belongs to Plan 065. Plan 063 status
is in `plans/063-status.md`.

## Plan 064 i2pd direct NTCP2 driver and observer correction

Plan 064 replaces the partial Plan 059 i2pd direct connect helper
with a correctly initialized, dual-mode, source-locked i2pd 2.60.0
NTCP2 interoperability driver. The driver is **test-only** and
never becomes a production dependency of `i2pr-daemon`. Plan 064
explicitly eliminates the eight documented defects of the Plan 059
helper (`D1`–`D8`): 64-hex SHA-256 Router Hash, NTCP2-address
static-key binding, source-verified pinned initialization, real
`CreateDeliveryStatusMsg` dispatch, bounded `SendMessage`
asynchronous semantics, sealed-topology reserved-range disable,
exact post-AEAD receive correlation, and measured provenance for
every helper input.

The Plan 064 deliverables are committed under
`tests/integration/ntcp2/reference-drivers/i2pd/`:

- `src/i2pd_ntcp2_interop_driver.cpp` — the source-locked C++
  driver with strict config validation, bounded `inspect` /
  `listen` / `dial` modes, real `CreateDeliveryStatusMsg`
  submission, and structured `i2pr-reference-event-v1` emission.
- `src/interop_observer.h` and `src/interop_observer.cpp` — the
  compile-time-gated passive observer API and sink.
- `patches/i2pd-2.60.0-interop-observer.patch` — the minimal
  observer patch that activates the post-AEAD receive seam and
  the successful frame-write send seam.
- `CMakeLists.txt`, `build-driver.sh`, `run-driver.sh`,
  `build-manifest.schema.json`, `source-lock.json`, and `README.md`
  — the build contract, the offline build seam, the runtime
  seam, the build-manifest schema, the source-lock record binding
  every artifact SHA-256, and the driver README.

The harness adapter and test matrices live under
`tests/integration/ntcp2/harness/`:

- `i2pd_direct_driver.py` — the Python harness adapter that binds
  every helper invocation into a Plan 062 v4 trigger record
  (`i2pr-reference-trigger-v4`) and validates the Plan 064 strict
  driver config contract.
- `test_i2pd_direct_driver.py` and `test_i2pd_direct_control.py` —
  the Plan 064 test matrices covering the source-verification
  contract, strict config contract, Python harness adapter,
  structured event contract, observer compile-time gating, the
  Plan 059 supersedure, and the typed host blocker.

The qualification receipt lives at
`tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
(schema `i2pr-i2pd-direct-driver-qualification-v1`). On this host
the receipt records the typed host-environment blocker
(`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
qualification remains to be produced in the Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass recovery
lane.

The Plan 064 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 064 i2pd topology contract section. The legacy
Plan 059 helper at
`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
is replaced by a fail-closed compatibility stub with the explicit
Plan 064 supersedure marker; the original source-lock record is
preserved verbatim as the bounded historical-reader path.

Plan 064 does not wire the i2pd driver into the canonical primary
`mixed_runner.py`; that wiring belongs to Plan 065.

Focused checks:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 065 NTCP2 canonical integration and live qualification

Plan 065 wires the corrected Java and i2pd direct drivers into the
canonical four-direction mixed-router lane, enforces the exact
DeliveryStatus correlation on the i2pr side, and produces one
complete four-direction live diagnostic bundle from a clean
implementation commit. Plan 065 establishes the implementation
floor from which Plan 066 may cut a candidate.

The Plan 065 implementation ships:

- `tools/i2pr-interop/src/scenario.rs` — the strict scenario
  schema bumped to `i2pr-launcher-scenario-v2` with the per-run
  DeliveryStatus `message_id`, the 64-lowercase-hex expected sender
  and receiver Router Hashes, the `reference_driver_mode` field,
  and the `run_identity_sha256` field. Legacy schema 1 records
  are rejected by the strict parser.
- `tools/i2pr-interop/src/main.rs` — the i2pr sender uses the
  scenario-owned message ID and verifies the round-trip envelope
  message ID and the DeliveryStatus payload message ID before frame
  emission. The i2pr receiver requires the exact envelope and
  payload message ID, rejects duplicates, and emits the bounded
  Plan 065 typed failure categories. The hard-coded `0x0420_0001`
  DeliveryStatus authority is removed.
- `tools/i2pr-interop/src/status.rs` — the status counter carries
  the per-run DeliveryStatus `message_id` and the expected peer
  Router Hash. The typed failure categories are added to the
  bounded `StatusReason` allowlist.
- `tests/integration/ntcp2/harness/launcher_protocol.py` and
  `tests/integration/ntcp2/harness/launcher_renderer.py` — the
  Python strict schema and renderer mirror the Rust schema with
  the same v2 marker and the same required primary fields.
- `tests/integration/ntcp2/harness/mixed_runner.py` — the canonical
  mixed-runner wires the new scenario primary fields through
  `render_and_validate` for both the i2pr initiator and responder
  paths.
- `tests/integration/ntcp2/harness/test_plan065.py` — the Plan 065
  test matrix.
- `scripts/check-ntcp2-interoperability.sh` — the static boundary
  checker enforces the Plan 065 schema marker, the required primary
  fields, the bounded typed failure categories, the absence of the
  hard-coded `0x0420_0001` DeliveryStatus authority, and the
  Plan 065 test matrix existence.

### Plan 065 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 074 real-driver and constrained-host corrective roadmap

Plan 074 is the active corrective roadmap for Milestone 3 NTCP2
interoperability. Plan 074 supersedes Plan 070 as the next executable
plan and reclassifies the implemented Plan 069 lane as orchestration
scaffolding and fake-process test coverage only; it is not valid
mixed-router evidence until Plan 075 closes. Plan 074 is the parent
authority for the active sequence **Plan 075 → Plan 076 → Plan 077 →
Plan 078 → Plan 079**. Plan 070 and Plan 071 are no longer active
execution authority.

The Plan 064 i2pd direct driver requires real library linkage and
genuine transport implementation. The Plan 064 helper's listen/dial
paths are terminal stubs when real pinned i2pd libraries are not
linked. The Plan 069 runner was scaffolding/fake-process coverage
only: it launched the i2pr launcher for both process handles and
could promote protocol milestones without consuming real structured
reference events. Zero real mixed-router attempts remain until Plan
078 executes.

The constrained-host lane decision is ordered: existing accessible
rootful Docker daemon (`--network none`), QEMU TCG guest (`-nic none`),
inherited connected TCP descriptors plus `no_new_privs`/seccomp for
reduced-scope protocol diagnostics, manually triggered dedicated
remote Linux runner, and a typed no-full-runtime-lane blocker.
Rootless namespaces, bubblewrap, rootless Podman/Docker, user-level
systemd `PrivateNetwork`, and repeated Multipass recovery are not
active work items on the known host.

## Plan 075 Plan 069 runner integrity and evidence correction

Plan 075 corrects the Plan 069 runner so it is structurally incapable
of producing a mixed-router pass unless it launches one real i2pr
process and one configured real reference process and consumes
authentic structured events from both. The corrected runner must
launch the reference role through the configured reference driver
via `tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`,
bind every accepted event to a measured reference process binary
digest, implementation name, run ID, direction, Router Hash pair,
and exact DeliveryStatus message ID, derive milestones only from
validated structured events, refuse synthetic provenance fallback
hashes, and fail closed with one of the typed blockers
`runner-reference-process-not-executed`,
`runner-reference-events-missing`,
`runner-synthetic-provenance-rejected`, or
`runner-protocol-event-unproven`. Plan 075 does not build i2pd, run
a real mixed-router direction, add Docker/QEMU/namespaces/CI, change
NTCP2 protocol code, or produce a Level 2 or Level 3 record. Plan 075
closes the runner-integrity surface only; the next active plan is
Plan 076 and a real mixed-router run remains blocked until the real
driver and a qualified Plan 077 execution lane exist. NTCP2 remains
experimental and non-advertised.
