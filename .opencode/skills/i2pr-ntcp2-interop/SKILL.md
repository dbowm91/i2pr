---
name: i2pr-ntcp2-interop
description: Operate, diagnose, or extend the repository's Plan 038/040/041/043/044/045/052/053/054/055/056/058/059/074/075 host-side Ubuntu 24.04 reference-router NTCP2 interoperability harness, including host preflight, pinned Java I2P and i2pd preparation, isolated scenario execution, Plan 044 mixed-runner composition, typed Plan 052/053 evidence validation, Plan 055 reference-initiated trigger schema and source-inspected call graphs, Plan 056 certificate verifier, Plan 058 record and candidate integrity closure, Plan 059 i2pd direct helper, per-reference observation qualification receipts, canonical pipeline live-mode wiring, the Plan 074 real-driver and constrained-host corrective roadmap, and the Plan 075 runner integrity and evidence correction. Use when an agent is asked to run a Plan 038 profile on the host, prepare the reference routers, add or modify a scenario, dispatch a bounded mixed direction, create or validate a Plan 053 diagnostic bundle, validate the locked trigger record schema, audit Plan 056 candidate and supersession markers, run the Plan 059 i2pd helper controls, validate the Plan 059 qualification receipts, exercise the Plan 075 runner integrity contract, or validate evidence. The companion skills `i2pr-rootless-sandbox` and `i2pr-multipass-recovery` cover the Plan 046 sealed-namespace lane and the Plan 048/049/050/051 recovery lane.
---

# I2PR NTCP2 Interoperability (host harness, Plans 038/040/041/043/045/055/056/058/059/074/075)

Use this skill from the repository root for the **host-side** Ubuntu 24.04
amd64 Plan 038 reference-router NTCP2 interoperability harness. This skill
intentionally does **not** cover the Plan 046 rootless sealed-namespace lane
or the Plan 048/049/050/051 Multipass recovery lane — load those companion
skills for those lanes.

Read `AGENTS.md`, `plans/038-ubuntu-reference-router-interoperability-harness.md`,
`plans/040-interop-apparatus-corrective-pass.md`,
`plans/041-reference-router-private-crosscheck.md`,
`plans/043-ubuntu-build-system-interop-gates.md`,
`plans/044-ntcp2-interop-final-integration-corrective-pass.md`,
`plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`,
`plans/045-closure-attempt.md`, `plans/052-ntcp2-milestone-3-evidence-closure-follow-up.md`,
`plans/053-plan052-evidence-pipeline-integration-corrective-pass.md`,
`plans/054-java-startup-and-reference-observation-qualification-pass.md`,
`plans/055-reference-initiated-ntcp2-trigger-and-topology-qualification-pass.md`,
`plans/056-ntcp2-milestone-3-two-run-external-evidence-closure-pass.md`,
`plans/058-plan056-record-and-candidate-integrity-closure-pass.md`,
`plans/058-status.md`,
`plans/059-reference-side-implementation-and-live-qualification-closure-pass.md`,
`plans/059-status.md`,
`plans/074-milestone-3-real-driver-and-constrained-host-corrective-roadmap.md`,
`plans/075-plan-069-runner-integrity-and-evidence-correction.md`,
`plans/075-status.md`,
`tests/integration/ntcp2/README.md`, and the relevant `docs/adr/` records before changing the harness.

The canonical reference identifiers are `java_i2p` and `i2pd`. Locked source
objects: Java I2P `2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. Abbreviated revisions are not
valid cache or evidence inputs.

## Companion skills (load before doing this lane)

- `i2pr-rootless-sandbox` — Plan 046 host-side rootless sealed-namespace lane.
- `i2pr-multipass-recovery` — Plan 048/049/050 Multipass recovery lane
  (atomic lifecycle, cloud-init taxonomy, base verify, four Plan 045 directions,
  sanitized export, selective purge, and the Plan 051 dispatch-gate
  troubleshooting bridge).

If the host emits `blocked_unprivileged_user_namespace` from the Plan 046
probe, do not try to recover inside Plan 038. Hand off to
`i2pr-multipass-recovery`.

## Safety boundary

Treat the harness as experimental infrastructure, not an anonymity or security
tool. **Never** enable `i2pr-daemon`, use public egress, perform DNS / bootstrap
/ reseed, retain identities/keys/RouterInfo/raw logs/packet captures, or turn
a local self-handshake, loopback run, vector, or testkit result into Java I2P
or i2pd interoperability evidence. Keep support rows experimental and
non-advertised unless sanitized evidence satisfies `specs/CONFORMANCE.md`.

Run only on an authorized disposable Ubuntu 24.04 amd64 host. The namespace
and firewall checks are mandatory and fail closed. Do not bypass a host,
privilege, route, cleanup, or evidence validation error.

The exact host contract is Ubuntu 24.04 amd64/x86_64, Bash 4+, UTF-8 locale,
non-interactive `sudo` when not root, Linux namespace/nftables capability,
and ≥4 GiB free under `target/`. Declared package set and locked source,
IzPack, cache, and build-command inputs are authoritative in
`tests/integration/ntcp2/references.lock.toml`.

## Plan 042 runtime and launcher boundary

The NTCP2 wire driver is a runtime-owned composition. `i2pr-runtime` owns
Tokio sockets and tasks, action deadlines, cancellation, replay/admission,
authenticated frame state, bounded queues, and child joins. The
`i2pr-transport-ntcp2` state machines remain runtime-neutral and receive only
complete bounded actions. `tools/i2pr-interop` is a non-production launcher
seam: it validates bounded non-secret scenario input and composes the runtime
driver, but it must **never** activate `i2pr-daemon`.

The launcher status protocol has separate meanings. A completed `listen` emits
listener readiness and then a distinct authenticated terminal result; `dial`
emits one terminal typed result; and `inspect` emits redacted state metadata.
Listener readiness is not authentication.

Plan 042 selects the existing fixed-size DeliveryStatus message (I2NP type 10)
for the first data smoke: 12-byte body, 21-byte NTCP2/SSU2 short transport
encoding, and 24-byte NTCP2 block before frame overhead and padding. A
positive gate requires one authenticated outbound and one authenticated
inbound DeliveryStatus per direction plus orderly cleanup. Reference acceptance
or echo behavior is not yet verified; do not claim interoperability or
substitute padding/TCP readiness for the message exchange.

## Plan 052 evidence closure constraints

Plan 052 is the corrective execution plan for closing Milestone 3. It
supersedes Plan 045 for closure purposes and introduces the following
non-negotiable constraints:

- **Single-source provenance.** Every artifact binds to one exact 40-char
  source commit recorded in `run-identity.json`. Short SHAs, dirty
  trees, archive/manifest mismatches, and non-finalized run identities
  are typed blockers (`tests/integration/ntcp2/harness/run_identity.py`).
- **Tri-state diagnostics.** The prior `I2PR_INTEROP_DUMP_RUN_LOGS`
  switch is replaced by `I2PR_INTEROP_DIAGNOSTICS=off|sanitized|raw-local`.
  `raw-local` is forbidden under any export root
  (`tests/integration/ntcp2/harness/mixed_runner.py:_diagnostics_mode`).
- **Typed observation schema v2.** Per-side observations use
  `i2pr-ntcp2-direction-observation-v2` with bounded levels. A passed
  direction requires both-side `ntcp2_authenticated`, sender
  `frame_emitted`, receiver `frame_authenticated_and_decrypted` AND
  `i2np_message_decoded` (`tests/integration/ntcp2/harness/observation.py`).
- **Atomic evidence bundles.** Each Milestone 3 run produces
  `target/interop/evidence/milestone-3/<run-id>/` with `run-identity.json`,
  an `environment/` block, per-direction `attestations/`, `directions/`,
  `triggers/`, `observations/`, and `cleanup/` records, a
  `diagnostics/sanitized-summary.json`, and a sanitized manifest
  (`tests/integration/ntcp2/harness/evidence_bundle.py`).
- **Java startup probe.** Standalone at
  `tests/integration/ntcp2/harness/java_startup_probe.py`; it isolates
  Java startup from i2pr and NTCP2 and never asserts an interoperability
  result.
- **Reference-trigger contracts.** Source-inspection record at
  `tests/integration/ntcp2/reference-trigger-contracts.md`; until the
  helpers are committed, the two reference-initiated directions remain
  typed blockers.

A Plan 052 evidence bundle closes Milestone 3 only when (a) it contains
exactly the four primary direction records, (b) every record binds to
the same run identity, (c) every record satisfies the v2 observation
predicate, and (d) two complete reproducible runs exist. Anything less
remains a typed diagnostic result.

## Plan 053 integrated diagnostic lane

Plan 053 is the local integration corrective pass. The canonical path uses
`tests/integration/ntcp2/harness/plan052_pipeline.py` to measure one clean
source identity before directions, copy that identity into immutable staging,
write all five artifact classes for every primary direction, and finalize with
`verify_bundle()` before any export. The explicit context arguments are
`--run-id`, `--run-identity`, `--bundle-staging`, and
`--evidence-profile milestone-3-v2`; they must not be inferred from the current
working directory. Blocked or rejected directions still produce complete
records. Their result is `diagnostic-complete-not-certificate`, never a
Milestone 3 pass.

The bundle verifier rejects unknown paths/schemas, traversal, symlinks,
non-regular files, hidden temporary files, duplicate/case-colliding paths,
manifest checksum errors, and mutations after finalization. Export writes the
acknowledgement beside `milestone-3/<run-id>/`, not inside the immutable bundle.
The current lane has no Java/i2pd source-locked receiver markers, so missing
reference data-phase observations remain typed rejections.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
bash scripts/check-ntcp2-interoperability.sh
```

Do not present the diagnostic bundle, reference-only controls, or a blocked
rootless/Multipass run as NTCP2 interoperability evidence.

## Plan 054 Java startup and reference-observation qualification

Plan 054 is the active local qualification pass for the Plan 052
directional predicate. It introduces the 16-cell Java startup matrix
(`tests/integration/ntcp2/harness/java_matrix.py`), the frozen
template lifecycle (`scripts/interop/java-prepare-template.py` and
the `seeded-clone` data state), and the machine-readable reference
observation catalog
(`tests/integration/ntcp2/reference-observation-catalog.toml`,
schema `i2pr-reference-observation-catalog-v1`). The Java and i2pd
adapters expose `collect_observation(role, run_id, correlation,
log_cursor, catalog)` and return finalized
`i2pr-ntcp2-direction-observation-v2` records. `mixed_runner.py` no
longer hardcodes `reference-receiver-marker-not-source-locked`; the
Plan 052 predicate now consumes the live observation. The
`plan052_pipeline._build_observation` accepts the live `i2pr_observation`
and `reference_observation` records; the synthetic builder remains as
the typed fallback for blocked and rejected directions.

Do not run a complete external matrix or qualification on a host that
lacks the pinned Java 2.12.0 and i2pd 2.60.0 references. The
Plan 046 host is the negative baseline; the Plan 048/049 Multipass
recovery lane is the canonical external path. A local
`diagnostic-complete-not-certificate` bundle is not Milestone 3
closure. NTCP2 remains experimental and non-advertised.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 055 reference-initiated trigger and topology qualification

Plan 055 is the qualification pass for the two reference-initiated
directions (`java-to-i2pr-ipv4` and `i2pd-to-i2pr-ipv4`). It owns:

- The locked trigger record schema `i2pr-reference-trigger-v3` in
  `tests/integration/ntcp2/harness/trigger_record.py`, with the
  bounded `TriggerHelperKind` (`i2pd-direct-helper`,
  `java-direct-helper`, `java-minimal-support-topology`) and
  `TriggerOutcome` enumerations required by Plan 055 A2. The
  helper build provenance, attempt count, correlation nonce,
  bounded monotonic timestamps, and target RouterInfo / public
  NTCP2 static-key digests are all bound into the canonical
  trigger digest.
- The source-inspection record at
  `tests/integration/ntcp2/reference-trigger-contracts.md`. Plan
  055 B5 selected the i2pd direct helper against
  `i2pd::transports::Transports::ConnectToPeer`; Plan 055 C5
  recorded the Java decision
  `java-direct-helper-rejected-global-context-not-isolatable`
  because the pinned Java 2.12.0 outbound path requires a full
  `RouterContext` and NetDB population.
- ADR 0021 (`docs/adr/0021-minimal-java-support-topology.md`)
  authorizes the optional `java-minimal-support-topology` fallback
  when a direct helper is impossible; the ADR must be approved
  before any topology-assisted helper is implemented.
- The Plan 052/053 pipeline
  (`plan052_pipeline.write_direction_artifacts`) binds the
  trigger record digest, correlation nonce, and target RouterInfo
  hash into the direction record (Plan 055 E2). A successful
  trigger outcome may not mask a rejected direction; the
  bounded i2pr responder reason from the Rust launcher is
  preserved (Plan 055 E3).

The Plan 055 helpers and the support topology live in the Plan 046
rootless sealed-namespace lane or the Plan 048/049 Multipass
recovery lane; this host is the negative baseline. The two
reference-initiated directions remain typed blockers until Plan 056
produces two complete reproducible bundles. Plan 056 closed with a
typed host-environment blocker; the canonical two-bundle certificate
verifier, the candidate freeze, and the local diagnostic bundles
are locally generated under the ignored
`target/interop/evidence/plan056/` working directory. Plan 058
retired the Plan 056 candidate, superseded Plan 057, decided
ADR 0021 (Rejected), and split the previous Plan 057 follow-up
into Plan 059 (reference-side implementation) and Plan 060
(fresh candidate + two-run certificate). Plan 059 closes with the
typed blocker `blocked_java_support_topology_rejected` because the
Java support topology is forbidden; the
`java-to-i2pr-ipv4` direction remains a typed blocker for the
pinned Java I2P 2.12.0 revision. Plan 060 cannot start under the
current four-direction contract until the Java reference-initiated
direction is qualified through a future ADR or the four-direction
contract is revised.

The only tracked footprint of the local diagnostic effort is the
bounded local-diagnostic receipt at
`tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
with `artifact_storage = local-untracked`. The repository does
not track `target/interop/evidence/plan056/`.

## Plan 059 reference-side implementation and live qualification closure pass

Plan 059 implements the i2pd direct helper, the per-reference
observation qualification receipts, and the canonical pipeline
live-mode wiring that Plans 055-057 deferred. ADR 0021 was
Rejected by the Plan 058 record and candidate integrity closure
pass, so Plan 059 closes with the typed blocker
`blocked_java_support_topology_rejected` and the
`java-to-i2pr-ipv4` direction remains blocked for the pinned
Java I2P 2.12.0 revision. Plan 060 cannot start under the
current four-direction contract.

The plan delivered:

- `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
  — the i2pd direct helper source (`i2pd_direct_connect.cpp`),
  the bounded local Python driver (`i2pd_direct_connect.py`),
  the CMake build contract (`CMakeLists.txt`), the source-lock
  record (`source-lock.json`), and the helper README. The C++
  helper links against the pinned i2pd 2.60.0 libraries and
  exercises the documented `Transports::SendMessage` call graph.
- `tests/integration/ntcp2/reference-observation-qualification/`
  — the per-reference qualification receipts and summary. Both
  pinned references carry `qualified = false` for every semantic
  level until the Plan 046 rootless sealed-namespace lane or the
  Plan 048/049 Multipass recovery lane exercises the runtime
  controls. The Java receipt carries the
  `blocked_java_support_topology_rejected` blocker because ADR
  0021 is Rejected.
- `tests/integration/ntcp2/harness/plan059.py` — the Plan 059
  helper module that loads the source-lock record, the
  qualification receipts, and the qualification summary, and
  exposes `i2pd_helper_invocation` for the test matrix.
- `tests/integration/ntcp2/harness/plan052_pipeline.py` — the
  canonical Plan 052/053 pipeline now accepts a `live_mode` flag
  and binds helper, source, catalog, and qualification-receipt
  digests into the direction record. Live mode rejects the
  synthetic fallback for passed reference-initiated directions.
- `tests/integration/ntcp2/harness/test_plan059.py` — the Plan 059
  test matrix (36 cases: i2pd helper, Java support-topology gate,
  receiver observations, Java startup gate, pipeline live mode).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce the
  Plan 059 artifacts, the test matrix coverage, the canonical
  pipeline live-mode enforcement, and the ADR 0021 rejection.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan055.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 scripts/interop/plan056_drive_bundles.py --repo-root . \
    --run-a-id <plan056-a-id> --run-b-id <plan056-b-id> \
    --evidence-root target/interop/evidence/plan056
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 060 fresh-candidate and two-run Milestone 3 certificate closure pass

Plan 060 is the execution-only pass that cuts one fresh candidate
after Plan 058 and Plan 059 close, selects exactly one execution
lane (direct-host or guest), runs the four primary IPv4 mixed-router
directions twice on independent mutable state, and produces a
verified Milestone 3 certificate over the two sanitized bundles.

The plan cannot start under the current four-direction contract
until either a future pinned Java revision is adopted or the
closure contract is revised through a new ADR (ADR 0021 is
Rejected by Plan 058). The Plan 046 rootless sealed-namespace
lane returns `blocked_unprivileged_user_namespace` on this host;
the Plan 048/049 Multipass recovery lane is the canonical external
path but cannot complete on this constrained host (per Plan 051).
Plan 060 therefore closes on this host with the typed environment
blocker `blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable`.

The plan delivered:

- `tests/integration/ntcp2/harness/plan060.py` — Plan 060 helper
  module. Exports `plan060_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan060_close_status()
  -> "declared-not-executable"`, `execution_lane_lock(...)` for
  the Plan 058 two-lane contract, `candidate_record_digests()`
  for the bounded digest table, `freeze_readiness_report()` for
  the freeze-readiness checklist,
  `assert_plan060_freeze_invariants()` for the typed blocker
  enforcement, and `plan060_two_bundle_independence(...)` for the
  cross-run independence rules.
- `tests/integration/ntcp2/harness/test_plan060.py` — Plan 060
  test matrix (35 cases across the Plan 060 surface).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 060 artifacts, the Plan 060 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/060-candidate.md` — Plan 060 candidate record. Status
  `declared-not-executable`. Implements the executed source
  commit, the implementation floor, the bounded digest table,
  the lane lock, the typed blockers, and the schema marker.
- `plans/060-closure.md` — Plan 060 closure record with the
  typed blocker and the close-status.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan060.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

A future pinned Java revision that exposes a transport-only direct
seam may trigger an ADR re-issue that supersedes the ADR 0021
rejection and unblocks the `java-to-i2pr-ipv4` direction. Until
Plan 060 produces two passing bundles from a fresh
implementation-floor candidate, NTCP2 stays experimental and
non-advertised and Milestone 3 stays open.

## Plan 067 staged interoperability corrective roadmap

Plan 067 is the **active** Milestone 3 corrective roadmap. Plan 067
supersedes Plan 066 as the active execution authority. Plan 066
remains an immutable historical record of the unavailable
release-qualification lane on the constrained host. Plan 067
corrects the active planning premise that release-grade isolation
and two-run certification are prerequisites for the first external
protocol test, and it corrects the stale active use of the ADR 0021
Java support-topology rejection after ADR 0022 accepted the direct
Java stripped-router driver.

Plan 067 separates NTCP2 interoperability evidence into four bounded
tiers:

- **Level 0 — local conformance.** Deterministic local protocol and
  runtime ownership.
- **Level 1 — external loopback smoke.** Two real processes on the
  host loopback. i2pd is the primary initial validator. Emissary is
  conditional. No rootless namespace, no Multipass, no candidate
  freeze, no two-bundle certificate, no reviewer record, no Java I2P
  required.
- **Level 2 — repeated development interoperability.** Both
  directions against the primary independent validator (pinned i2pd
  2.60.0), three fresh-state repetitions per direction, exact
  message and identity correlation, bounded negative controls.
- **Level 3 — release qualification.** Java I2P 2.12.0 and i2pd
  2.60.0, isolated no-public-egress lane, reproducible
  source/reference provenance, exact authenticated data-phase
  message correlation, independent fresh state, sanitized durable
  evidence. The Plan 066 certificate verifier may be reused at Level
  3.

Java and i2pd remain required for release qualification. NTCP2 stays
experimental and non-advertised.

## Plan 068 staged evidence and Milestone 3 authority correction

Plan 068 implements the staged-evidence and authority correction
that Plan 067 proposes. Plan 068 is the only path that produces the
Plan 030/066 active-status correction, the Plan 066 supersession
notice, the evidence-tier constants and tests, the smoke and
development-validation record schemas and tests, the
static-check-scope simplification for Plans 069-073, and the
documentation propagation.

Plan 068 delivers:

- `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`
  (Accepted). ADR 0023 separates evidence into four bounded tiers
  and forbids lower-tier promotion into release bundles.
- `tests/integration/ntcp2/harness/evidence_tier.py` — the
  evidence-tier constants and tier-separation rules.
- `tests/integration/ntcp2/harness/loopback_smoke_record.py` — the
  Level 1 smoke record schema
  (`i2pr-ntcp2-loopback-smoke-v1`).
- `tests/integration/ntcp2/harness/development_validation.py` — the
  Level 2 development-validation summary schema
  (`i2pr-ntcp2-development-validation-v1`).
- The Plan 068 test matrices
  (`tests/integration/ntcp2/harness/test_evidence_tier.py`,
  `test_loopback_smoke_record.py`,
  `test_development_validation.py`,
  `test_plan068.py`).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce the
  new schema modules, the new test matrices, the ADR 0023
  acceptance marker, and the release-bundle smoke/development
  rejection.

Plan 068 also removes the stale
`blocked_java_support_topology_rejected` interpretation from the
active Java path: ADR 0021 remains Rejected and the Java support
topology remains forbidden, but the ADR 0022 direct Java driver is
the active Java architecture. Java may still be unavailable because
of host/runtime/build defects, but not because ADR 0021 forbids the
already accepted replacement architecture.

The focused closure baseline for Plans 069-073 is the touched-code
test suite plus `cargo fmt --all --check`, `cargo check --workspace
--all-targets`, `cargo test --workspace`,
`scripts/check-dependency-direction.sh`, and
`scripts/check-runtime-boundaries.sh`. Full historical harness
matrices, rootless checks, and Multipass checks remain available
for explicit integration checkpoints but are not required for
Level 1 or Level 2 closures.

Use the Plan 068 focused seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_tier.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan068.py'
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

The corrected repository state is:

```text
plan_068_staged_evidence = implemented
plan_069_runner_scaffolding = implemented_but_not_valid_mixed_router_lane
real_i2pd_driver = not_implemented
real_i2pd_library_linkage = absent
real_reference_process_in_plan069_runner = absent
real_mixed_router_attempts = 0
current_rootless_namespace_lane = unavailable
multipass_lane = unreliable_or_unavailable
support = experimental
advertised = false
normal_daemon_activation = disabled
```

The constrained-host lane decision is ordered: existing accessible
rootful Docker daemon (`--network none`), QEMU TCG guest (`-nic none`),
inherited connected TCP descriptors plus `no_new_privs`/seccomp for
reduced-scope protocol diagnostics, manually triggered dedicated
remote Linux runner, and a typed no-full-runtime-lane blocker.
Rootless namespaces, bubblewrap, rootless Podman/Docker, user-level
systemd `PrivateNetwork`, and repeated Multipass recovery are not
active work items on the known host.

## Plan 069 host-compatible NTCP2 loopback smoke lane

> **Reclassification (Plan 074, supersession note).** Plan 069
> implements the Plan 067 Level 1 host-loopback smoke runner and its
> static boundary check, but at the time Plan 074 was registered the
> runner was scaffolding/fake-process coverage only. The Plan 069
> runner selected the i2pr launcher for both process handles, did not
> invoke the supplied i2pd binary as the reference process, and could
> promote protocol milestones without consuming real structured
> reference events. The Plan 064 i2pd helper's listen/dial paths were
> terminal stubs when real pinned i2pd libraries were not linked. The
> Plan 069 closure record (`plans/069-status.md`) is preserved as a
> snapshot of that scaffolding state. Plan 075 is the runner integrity
> and evidence correction pass; the Plan 069 lane is not valid
> mixed-router evidence until Plan 075 closes.

Plan 069 implements the Plan 067 Level 1 host-loopback smoke lane.
The lane is a non-production composition that exercises a single
two-process NTCP2 direction (one i2pr launcher process, one Plan 064
i2pd direct driver process) on the host loopback, without sudo,
namespaces, Multipass, or any public-network access. The runner is
structurally incapable of producing a Level 3 release bundle or
certificate.

Use the Plan 069 focused seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pr-to-i2pd-ipv4 \
  --reference-driver <path> \
  --reference-build-manifest <path> \
  --reference-source-lock <path> \
  --output <smoke-record.json> \
  --source-commit <40-lowercase-hex>
```

The runner is the lane Plan 075 will exercise for the first real
external i2pr/i2pd direction execution on a host where unprivileged
namespaces are unavailable. Plan 075 owns the runner integrity
correction and the next status update; Plan 069 builds the lane and
proves it fails closed under the focused tests.

## Plan 075 Plan 069 runner integrity and evidence correction

Plan 075 corrects the Plan 069 runner so it is structurally
incapable of producing a mixed-router pass unless it launches one
real i2pr process and one configured real reference process and
consumes authentic structured events from both. The corrected
runner must:

- launch the reference role through the configured reference driver
  via `tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`,
  not a second `i2pr-interop` process;
- bind every accepted event to a measured reference process binary
  digest, implementation name, run ID, direction, Router Hash pair,
  and exact DeliveryStatus message ID;
- derive milestones only from validated structured events
  (`ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`),
  never from a TCP loopback probe alone;
- refuse synthetic provenance fallback hashes that fabricate a
  schema-valid digest from a run string;
- fail closed with one of the typed blockers
  `runner-reference-process-not-executed`,
  `runner-reference-events-missing`,
  `runner-synthetic-provenance-rejected`, or
  `runner-protocol-event-unproven` whenever any of the above
  contracts is violated.

Use the Plan 075 focused seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

Plan 075 does not build i2pd, run a real mixed-router direction,
add Docker/QEMU/namespaces/CI, change NTCP2 protocol code, or
produce a Level 2 or Level 3 record. Plan 075 closes the runner
integrity surface only. The next active plan is Plan 076; do not
attempt a real mixed-router run until the real i2pd driver and a
qualified Plan 077 execution lane exist.

## Plan 076 real pinned i2pd driver

Plan 076 is the next executable plan. It must build and link the exact pinned i2pd 2.60.0 source, remove terminal listen/dial stubs, prove real symbols and genuine inspect behavior, and produce measured instrumented/control manifests. Stop on unavailable or private APIs rather than substituting mocks or synthetic provenance.

## Plan 077 constrained-host lane

Plan 077 follows only after Plan 076 closes. Probe existing rootful Docker (`--network none`), then QEMU TCG (`-nic none`), then the explicitly reduced inherited-descriptor lane or a manually triggered remote runner. Do not install privileged services, retry rootless/Multipass lanes, or claim protocol evidence from a typed no-lane result.

## Plan 078 first real two-way run

Plan 078 is blocked until Plans 076 and 077 close. It owns one fresh instrumented `i2pr-to-i2pd-ipv4` run and one fresh instrumented `i2pd-to-i2pr-ipv4` run, exact DeliveryStatus/Router Hash correlation, control comparison, cleanup, and Level 1 records only. Plan 079 owns later repeated validation.

## Companion skills (load before doing this lane)

## Plan 044 mixed-runner composition (host-side executor)

The checkout contains the four directional mixed-scenario definitions under
`tests/integration/ntcp2/mixed-scenarios/`: `i2pr-to-java-ipv4`,
`java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`, and `i2pd-to-i2pr-ipv4`. Each
direction has a unique execution ID, one declared initiator and responder,
and one terminal typed result.

The mixed runner composes `I2prAdapter` with each reference adapter through
a strict launcher scenario renderer. The renderer populates the exact launcher
schema with execution-specific scenario ID, role, address family, synthetic
endpoints, private network ID 99, confined state directory, deadlines,
padding profile, smoke-message profile, and expected-result class.

The data-phase oracle does not rely on an echo assumption. It uses a
protocol-valid trigger supported by both pinned references. Evidence records
carry real counters for authenticated-link count, frames sent/received, I2NP
message aggregates, admission/replay counters, process lifecycle counters,
and cleanup disposition.

Gate archival uses gate-specific staging to prevent cross-gate record
relabeling. The aggregate manifest must include exactly the expected records
for the selected profile; missing, extra, mislabeled, or zero-valued records
fail the gate. **No completed mixed-router i2pr record is present; these are
explicit blockers, not skipped successes.** The single directional record
that landed during the Plan 045 closure attempt is described in
`plans/045-closure-attempt.md` and exists only as a sanitized evidence file
plus its corresponding typed blockers for the other three directions.

## Plan 043 workflow

The semantic gates are ordered and later gates are ineligible when required
inputs are missing or invalid:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

1. Inspect the lock, scenario definitions, and current workflow status. Do not
   change source revisions, package assumptions, scenario IDs, or the IzPack
   hash without updating the plan and conformance documentation.
2. Run the contract checks without starting routers. Preparation then runs
   `check-host.sh --pre-install`, the declared `setup-host.sh`, and
   `check-host.sh --post-install`.
3. Build exact reference caches with `build-references.sh --force-rebuild`.
   This is the only network-enabled phase and records source/tool/artifact/
   tree hashes. Resolve only through `target/interop/cache/current-cache.json`.
4. Restore the verified cache and run `build-references.sh --offline`.
   Re-hash the complete runtime tree. A cache miss or metadata mismatch is a
   hard failure; never fetch or choose an arbitrary cache.
5. Run `environment-smoke`, then `reference-crosscheck-ipv4`. The latter uses
   separate Java/i2pd namespaces, private network ID 99, staged strict
   RouterInfo validation/import, controlled directions, and dual authenticated
   observations. It is harness control evidence only.
6. Only after reference control passes, build the current launcher and run
   `handshake-smoke`; require four independent i2pr/reference directions,
   authenticated handshake, bounded DeliveryStatus exchange, typed counters,
   sanitized finalization, and clean state. Run `full` only afterward; it adds
   bounded adversarial/resource cases and never unbounded fuzzing.
7. Validate every record and the aggregate manifest with
   `validate-evidence.py` and `check-ntcp2-interoperability.sh`. Empty
   evidence, placeholders, forbidden content, missing scenarios, extra passed
   records, hash mismatches, or incomplete direction coverage fail the gate.
8. Record the clean-host baseline before privileged execution with
   `sudo -E bash scripts/interop/verify-clean-host.sh --record-baseline`.
   Always run `cleanup.sh`, then verify with
   `sudo -E bash scripts/interop/verify-clean-host.sh --verify --baseline
   target/interop/build/clean-host-baseline.json`. Reject residual namespaces,
   veths, child processes, secret-bearing run roots, forbidden retained
   files, or attributable host nftables/routes/forwarding changes. Cleanup
   verification failure overrides protocol success.

The workflow and helper apparatus expose the ordered manual Plan 043 lane,
including clean-host verification and aggregate validation, but **no completed
successful aggregate run is present.** Treat that as an explicit Plan 043
blocker, not a skipped pass. Retain only sanitized typed records and approved
hashes under `target/interop/evidence/`.

## Result interpretation

- `blocked_host_contract` — no router process or protocol claim was made.
- `i2pr-mixed-router-profile-not-wired` — the active scenario ID is not
  allowlisted for the current mixed-router gate.
- Rejected configuration/state, authentication, timeout, cleanup, or
  evidence-validation failures remain typed and visible. **Never** convert
  them to pass or omit them from the closure record.
- An empty evidence directory is not success. Plan 041 reference-pair records
  are harness controls, not i2pr mixed-router evidence.
- For Plan 046 typed host-level blockers (e.g.,
  `blocked_unprivileged_user_namespace`), hand off to
  `i2pr-multipass-recovery`.
- A blocked profile, a reference-only control record, or a typed blocker is
  **never** an i2pr interoperability result. Do not advertise NTCP2 and do
  not close Milestone 3.

## Authoritative command surface (host-side)

Run from the repository root:

```text
# Host + build gates
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline

# Profiles
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full

# One bounded run
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4 --reference java_i2p
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4 --reference i2pd

# Validation and cleanup
bash scripts/interop/validate-evidence.py
python3 scripts/interop/aggregate-evidence.py --profile <profile>
bash scripts/check-ntcp2-interoperability.sh
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify \
    --baseline target/interop/build/clean-host-baseline.json
```

For the Plan 046 rootless sealed-namespace lane (`probe-rootless-sandbox.sh`,
`rootless-enter.sh`, `check-rootless-interop-boundary.sh`), use the
`i2pr-rootless-sandbox` skill.

For the Plan 048/049/050/051 Multipass recovery lane (`run-evidence-lane.sh`,
`create.sh`, `prepare-offline.sh`, `probe.sh`, `snapshot.sh`, `restore.sh`,
`transfer-source.sh`, `transfer-cache.sh`, `verify-base.sh`,
`cloud-init-status.sh`, `verify-clean-host.sh`, `selective-purge.sh`,
`run-matrix.sh`, `run-direction.sh`, `export-evidence.sh`, `dispatch-gate.sh`,
`check-multipass-interop-boundary.sh`), use the `i2pr-multipass-recovery`
skill.

## Files to inspect

- `tests/integration/ntcp2/references.lock.toml` — Ubuntu contract, source
  pins, build commands, exact IzPack SHA-256.
- `tests/integration/ntcp2/scenarios/*.toml` — the eight bounded i2pr/
  reference scenario definitions. IDs synchronized with
  `tests/integration/ntcp2/manifest.toml`.
- `tests/integration/ntcp2/reference-scenarios/` — Plan 041 pair schema and the
  two directional Java I2P / i2pd control scenarios.
- `tests/integration/ntcp2/mixed-scenarios/` — the four Plan 044 directional
  i2pr/reference scenarios.
- `tests/integration/ntcp2/harness/` — Python topology, adapters, process
  bounds, runner, evidence, mixed-runner, launcher renderer, data-phase
  oracle, reference-trigger, rootless supervisor, and multipass code.
- `scripts/interop/` — host setup, builders, isolation, matrix, gate staging,
  aggregate, cleanup.
- `scripts/check-ntcp2-interoperability.sh`,
  `scripts/check-fixture-manifest.sh`, `scripts/check-ntcp2-vectors.sh` —
  static gate checkers.
- `tools/i2pr-interop/` — non-production launcher seam. The current checkout
  composes bounded state preparation, listener/dial, handshake, authenticated
  link, and DeliveryStatus smoke through the Plan 044 mixed-runner. Its
  success is local driver validation only.
- `target/interop/evidence/` — sanitized records only; gate-prefixed files
  live alongside `run-manifest.json`. `target/interop/runs/` is
  secret-bearing and is deleted after every run.

## Development rules

Keep production ownership boundaries intact: runtime owns Tokio tasks and
sockets; transport contracts remain runtime-neutral; the launcher crate under
`tools/i2pr-interop` is a non-production seam and must not activate the
daemon. Add negative-path tests for new configuration, topology, process,
parser, or evidence behavior. Prefer deterministic local checks and never add
raw network fixtures or secrets.

Before handoff, run from the repository root, in this order:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh        # when I2NP fixture bytes change
bash scripts/check-ntcp2-vectors.sh           # when NTCP2 vector bytes change
bash scripts/check-ntcp2-interoperability.sh  # when ntcp2 evidence/manifest change
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

Record commands, results, host constraints, and any blocked stop condition
in a closure record; do not report a blocked profile as a passing
interoperability result.

## Plan 062 NTCP2 evidence-contract and architecture correction

Plan 062 is the evidence-contract and architecture correction
pass. The plan does not implement the Java or i2pd drivers and
does not perform an authoritative external interoperability run;
those belong to Plans 063 and 064.

The plan lands:

- `docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`
  (Accepted) — two-process direct transport drivers for Java
  I2P and i2pd. ADR 0022 supersedes the conclusion of ADR 0021
  (Rejected by Plan 058) without rewriting ADR 0021. The primary
  topology forbids support routers, floodfill, reseed, SAM,
  I2CP, HTTP/I2PControl, and tunnels.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the source-locked API surface for the pinned Java I2P
  2.12.0 and i2pd 2.60.0 revisions.
- `tests/integration/ntcp2/harness/reference_trigger_v4.py` — the
  v4 trigger schema (`i2pr-reference-trigger-v4`). Replaces the
  40-hex SHA-1 Router Hash with the 64-hex SHA-256 Router Hash
  and binds the per-run DeliveryStatus `message_id`
  (`1..=0xffffffff`).
- `tests/integration/ntcp2/harness/reference_event.py` — the
  reference-event v1 schema (`i2pr-reference-event-v1`)
  recording per-driver structured events with exact DeliveryStatus
  message ID correlation for data-phase events.
- `tests/integration/ntcp2/harness/observation_v3.py` — the v3
  observation schema (`i2pr-ntcp2-direction-observation-v3`)
  with the mandatory correlation fields
  `delivery_status_message_id`, `peer_router_hash_sha256`,
  `local_router_hash_sha256`, and `source_event_sha256`.
- Retirement of the Plan 060 candidate from all future
  candidate validators and the static boundary checker. The
  Plan 060 candidate record is preserved verbatim; the Plan 060
  closure record carries the explicit "Superseded by Plan 062"
  marker.

The v3 trigger schema and the v2 observation schema remain
readable for historical inspection but cannot contribute to a
new passing bundle. The future candidate implementation floor is
Plan 065 closure or later.

The Plan 062 static boundary checker extension in
`scripts/check-ntcp2-interoperability.sh` fails when:

- the v4 trigger schema, reference-event v1 schema, or v3
  observation schema files are absent;
- ADR 0022 is absent or not Accepted after source verification;
- the Plan 060 candidate is not retired;
- the active code uses `_HEX40` for Router Hash;
- the Plan 062 documentation is absent.

Focused tests:

- `tests/integration/ntcp2/harness/test_reference_trigger_v4.py`
  — v4 trigger schema validator cases (40-hex rejection,
  63/65-hex rejection, uppercase rejection, all-zero provenance
  rejection, message ID bounds, v3 schema rejection, unknown
  fields).
- `tests/integration/ntcp2/harness/test_reference_event.py` —
  reference-event v1 schema cases (data-phase event fields,
  duplicate event sequence rejection, peer Router Hash mismatch
  rejection, terminal-event data-phase field rejection,
  forbidden payload text rejection).
- `tests/integration/ntcp2/harness/test_observation_v3.py` — v3
  observation schema cases (v2 rejection, mandatory correlation
  fields, receiver pass predicate, generic-phrase rejection,
  sender-only and wrong-message fixtures).
- `tests/integration/ntcp2/harness/test_plan062.py` — WP1-WP5
  surface tests covering the source-verification record, ADR
  0022 Accepted status, schema migration, Plan 060 retirement,
  and the Plan 061-066 roadmap chain.

Plan 062 does not advance any support row. NTCP2 remains
experimental and non-advertised; Milestone 3 stays open until
Plan 065 closes with one complete four-direction live diagnostic
bundle and Plan 066 produces a verified Milestone 3 certificate.

## Plan 063 Java I2P stripped-router direct NTCP2 driver

Plan 063 implements the source-locked Java I2P 2.12.0 stripped-router
direct NTCP2 driver. The driver is the test-only counterpart to the
Plan 064 i2pd driver; together they form the source-locked pair
required by the Plan 061 roadmap.

The plan delivered:

- `tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java`
  — the source-locked Java driver. It embeds the upstream
  `net.i2p.router.Router` and `RouterContext`, activates the
  pinned dummy facades, uses the real
  `net.i2p.router.transport.ntcp.NTCPTransport` (no patch, no
  SSU2, no `VMCommSystem`), and submits a real correlated
  `DeliveryStatusMessage` through `OutNetMessagePool`. The driver
  exposes `inspect`, `listen`, and `dial` modes through a strict
  config contract. The receive handler is a `HandlerJobBuilder`
  registered for `DeliveryStatusMessage.MESSAGE_TYPE` (constant
  value 10); the data-phase events (`frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`)
  are emitted from the receive handler invocation, never from a
  generic log phrase.
- `tests/integration/ntcp2/reference-drivers/java/source-lock.json`
  — the source-lock record
  (`i2pr-java-helper-source-lock-v1`) binding the pinned Java
  revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`, the helper
  source path, the build contract, and the locked constraints.
- `tests/integration/ntcp2/reference-drivers/java/classpath-manifest.json`
  — the runtime classpath binding every pinned jar in
  `target/interop/cache/java_i2p/<tree>/lib/` to its purpose. No
  Maven Central dependency may be introduced.
- `tests/integration/ntcp2/reference-drivers/java/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-java-helper-build-manifest-v1`) that requires measured
  digests for every pinned artifact, the driver source and
  binary, the classpath manifest, and the JDK versions.
- `tests/integration/ntcp2/reference-drivers/java/build-driver.sh`
  and `run-driver.sh` — the offline build and runtime seams. The
  build script requires the exact pinned source/build cache, uses
  a deterministic sorted source list, uses an explicit classpath,
  and emits no download.
- `tests/integration/ntcp2/harness/java_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into a
  Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 063 strict driver config contract. The
  adapter never reaches inside the Java helper state and never
  synthesises a passing record.
- `tests/integration/ntcp2/harness/test_java_direct_driver.py` and
  `test_java_direct_control.py` — the Plan 063 test matrix
  covering the source-verification contract, strict config
  contract, Python harness adapter, structured event contract, and
  the local inspect-mode round-trip where the pinned Java cache is
  available.
- `tests/integration/ntcp2/qualification/java-direct-driver.json` —
  the Plan 063 qualification receipt
  (`i2pr-java-direct-driver-qualification-v1`). On this host the
  receipt records the typed host-environment blocker
  (`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
  qualification remains to be produced in the Plan 046 rootless
  sealed-namespace lane or the Plan 048/049 Multipass recovery
  lane.

The Plan 063 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 063 topology contract section. The active v4 trigger
schema, v3 observation schema, and reference-event v1 schema
continue to enforce the 64-hex SHA-256 Router Hash contract.

Plan 063 does not wire the Java driver into the canonical primary
`mixed_runner.py`; that wiring belongs to Plan 065. The Plan 063
closure contract is the typed host-environment blocker
`blocked_unprivileged_user_namespace` on the Plan 046 negative
baseline; the canonical external lane is the Plan 048/049
Multipass recovery lane.

### Plan 063 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 064 i2pd direct NTCP2 driver and observer correction

Plan 064 replaces the partial Plan 059 i2pd direct connect helper
with a correctly initialized, dual-mode, source-locked i2pd 2.60.0
NTCP2 interoperability driver. The driver is **test-only** and
never becomes a production dependency of `i2pr-daemon`. Plan 064
explicitly eliminates the eight documented defects of the Plan
059 helper: 64-hex SHA-256 Router Hash, NTCP2-address static-key
binding, source-verified pinned initialization, real
`CreateDeliveryStatusMsg` dispatch, bounded `SendMessage`
asynchronous semantics, sealed-topology reserved-range disable,
exact post-AEAD receive correlation, and measured provenance for
every helper input.

The plan delivered:

- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — the source-locked C++ driver with strict config validation,
  bounded `inspect` / `listen` / `dial` modes, real
  `CreateDeliveryStatusMsg` submission, and structured
  `i2pr-reference-event-v1` emission.
- `tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h`
  and `interop_observer.cpp` — the compile-time-gated passive
  observer API and sink.
- `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`
  — the minimal observer patch that activates the post-AEAD
  receive seam and the successful frame-write send seam.
- `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
  — the source-lock record
  (`i2pr-i2pd-direct-driver-source-lock-v1`) binding the pinned
  i2pd revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`, the
  helper source path, the build contract, the observer patch
  digest, and the locked constraints.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-i2pd-direct-driver-build-manifest-v1`) that requires
  measured digests for the i2pd source tree, the observer patch,
  the helper source and binaries, the linked library manifest,
  the CMake version, and the compiler version.
- `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt`,
  `build-driver.sh`, `run-driver.sh`, and `README.md` — the
  offline build contract, the runtime seam, and the driver
  README.
- `tests/integration/ntcp2/harness/i2pd_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into
  a Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 064 strict driver config contract. The
  adapter never reaches inside the C++ helper state and never
  synthesises a passing record.
- `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py`
  and `test_i2pd_direct_control.py` — the Plan 064 test matrices
  covering the source-verification contract, strict config
  contract, Python harness adapter, structured event contract,
  observer compile-time gating, the Plan 059 supersedure, and the
  typed host blocker.
- `tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
  — the Plan 064 qualification receipt
  (`i2pr-i2pd-direct-driver-qualification-v1`). On this host the
  receipt records the typed host-environment blocker
  (`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
  qualification remains to be produced in the Plan 046 rootless
  sealed-namespace lane or the Plan 048/049 Multipass recovery
  lane.

The Plan 064 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 064 i2pd topology contract section. The Plan 059
helper at `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
is replaced by a fail-closed compatibility stub with the explicit
Plan 064 supersedure marker; the original source-lock record is
preserved verbatim as the bounded historical-reader path.

Plan 064 does not wire the i2pd driver into the canonical primary
`mixed_runner.py`; that wiring belongs to Plan 065.

### Plan 064 focused checks

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

Consult [operations.md](references/operations.md) for command routing,
profiles, typed outcomes, and implementation-specific stop conditions.

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
  message ID and the DeliveryStatus payload message ID before
  frame emission. The i2pr receiver requires the exact envelope
  and payload message ID, rejects duplicates, and emits the
  bounded Plan 065 typed failure categories
  (`SenderDeliveryStatusMessageIdZero`,
  `SenderRouterIdentityMismatch`,
  `SenderDeliveryStatusConstructionFailed`,
  `SenderFrameQueueAmbiguous`, `SenderFrameWriteFailed`,
  `SenderMultiplePrimaryDeliveryStatusEmitted`,
  `SenderCancellationObserved`, `ReceiverFrameReadFailed`,
  `ReceiverFrameAuthenticationFailed`, `ReceiverI2npDecodeFailed`,
  `ReceiverDeliveryStatusMissing`,
  `ReceiverDeliveryStatusIdMismatch`,
  `ReceiverDeliveryStatusDuplicate`,
  `ReceiverPeerIdentityMismatch`,
  `ReceiverDeliveryStatusTimestampInvalid`). The hard-coded
  `0x0420_0001` DeliveryStatus authority is removed.
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
  paths. The `_plan065_primary_fields` helper derives the
  DeliveryStatus `message_id` from the run identity and the
  correlation nonce; the `_reference_driver_mode_for` helper
  returns the source-locked driver mode for a reference kind. The
  runner rejects SAM, HTTP, I2PControl, support-topology, and
  synthetic-fallback helpers for any primary direction.
- `tests/integration/ntcp2/harness/test_plan065.py` — the Plan 065
  test matrix covering scenario v2 acceptance and rejection (zero
  message ID, 40-hex Router Hash, unknown reference driver mode,
  direction-helper mismatch), DeliveryStatus message ID
  derivation uniqueness, status counter contract, reference
  trigger v4 correlation, observation v3 correlation, pass
  predicate exact message ID and Router Hash correlation,
  support-router rejection, Plan 060 candidate retirement, and the
  Plan 066 implementation floor marker.

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

## Plan 066 fresh-candidate and authoritative NTCP2 two-run closure pass

Plan 066 is the execution-only pass that cuts one fresh candidate
descended from the Plan 065 implementation floor, selects exactly
one execution lane (direct-host or guest), runs the four primary
IPv4 mixed-router directions twice on independent mutable state,
and produces a verified Milestone 3 certificate over the two
sanitized bundles. The plan-of-record is
`plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md`.

The plan cannot start under the current four-direction contract
until either a future pinned Java revision is adopted or the
closure contract is revised through a new ADR (because ADR 0021 is
Rejected by Plan 058). The Plan 046 rootless sealed-namespace
lane returns `blocked_unprivileged_user_namespace` on this host;
the Plan 048/049 Multipass recovery lane is the canonical external
path but cannot complete on this constrained host (per Plan 051).
Plan 066 therefore closes on this host with the typed environment
blocker `blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable`.

The Plan 066 implementation ships:

- `tests/integration/ntcp2/harness/plan066.py` — the Plan 066
  helper module. Exports `plan066_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan066_close_status()
  -> "declared-not-executable"`, `plan066_execution_lane_lock(...)`
  for the Plan 058/060 two-lane contract,
  `plan066_candidate_record_digests()` for the bounded 23-row
  digest table, `plan066_freeze_readiness_report()` for the
  freeze-readiness checklist,
  `assert_plan066_freeze_invariants()` for the typed blocker
  enforcement, `plan066_directional_record(...)` for the per-
  direction record skeleton, `plan066_two_bundle_independence(...)`
  for the cross-run independence rules, and
  `plan066_finalized_bundle_marker()` for the bundle mutation
  guard.
- `tests/integration/ntcp2/harness/test_plan066.py` — the Plan 066
  test matrix (41 cases covering the 30 enumerated Plan 066
  Phase 12 cases plus the typed-blocker, freeze-readiness,
  helper-contract, and Plan 065 plan-of-record helpers).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 066 artifacts, the Plan 066 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/066-candidate.md` — the Plan 066 candidate record. Status
  `declared-not-executable`. Implements the executed source
  commit (the Plan 065 implementation floor), the bounded 23-row
  digest table, the lane lock, the typed blockers, and the schema
  marker.
- `plans/066-closure.md` — the Plan 066 closure record with the
  typed blocker and the close-status.

### Plan 066 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan066.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Until the Plan 046 rootless sealed-namespace lane or the Plan
048/049 Multipass recovery lane becomes runnable on a host with
the resources Plan 051 required, or until ADR 0021 is superseded
by an ADR that authorizes a different execution path for the
`java-to-i2pr-ipv4` direction, Milestone 3 stays open and NTCP2
stays experimental and non-advertised. The Plan 066 implementation
surface is mandatory regardless of close outcome.
