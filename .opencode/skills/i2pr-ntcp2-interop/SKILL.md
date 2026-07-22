---
name: i2pr-ntcp2-interop
description: Operate, diagnose, or extend the repository's Plan 038/040/041/043/045 host-side Ubuntu 24.04 reference-router NTCP2 interoperability harness, including host preflight, pinned Java I2P and i2pd preparation, isolated scenario execution, Plan 044 mixed-runner composition, typed evidence validation, and cleanup. Use when an agent is asked to run a Plan 038 profile on the host, prepare the reference routers, add or modify a scenario, dispatch a bounded Plan 044 mixed direction, or validate evidence. The companion skills `i2pr-rootless-sandbox` and `i2pr-multipass-recovery` cover the Plan 046 sealed-namespace lane and the Plan 048/049/050/051 Multipass recovery lane.
---

# I2PR NTCP2 Interoperability (host harness, Plans 038/040/041/043/045)

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
`plans/045-closure-attempt.md`, `tests/integration/ntcp2/README.md`, and the
relevant `docs/adr/` records before changing the harness.

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

Consult [operations.md](references/operations.md) for command routing,
profiles, typed outcomes, and implementation-specific stop conditions.
