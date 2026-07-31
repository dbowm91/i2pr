# Plan 075 closure — Plan 069 runner integrity and evidence correction

## Implementation commit

The Plan 075 implementation lands in the same commit as this
closure record. The implementation surface is described in
`plans/075-plan-069-runner-integrity-and-evidence-correction.md`
and the focused checks below record the run results. No real
mixed-router direction is executed; the runner is corrected and
the runner-integrity contract is enforced through a fail-closed
set of typed blockers.

## Defects corrected

The Plan 069 runner carried six defects that Plan 075 closes
through structural changes rather than compatibility shims:

1. **D1 — both process roles selected the i2pr launcher.** The
   runner now builds a direction-unique command via
   `ROLE_ASSIGNMENTS` and `ProcessRole`/`TransportRole` pair; the
   listener role for `i2pr-to-i2pd-ipv4` resolves to the reference
   driver, and the listener role for `i2pd-to-i2pr-ipv4` resolves
   to the i2pr launcher.
2. **D2 — the supplied reference driver was validated but never
   invoked.** The reference role is now built through
   `_build_reference_command`, which calls the committed
   `tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`
   with the measured driver binary path and the strict-config
   payload.
3. **D3 — protocol milestones were auto-marked after a TCP loopback
   probe.** `_monitor_protocol` now strips the auto-mark calls and
   delegates milestone evaluation to `_consume_reference_events`,
   which validates the Plan 062 reference-event v1 stream and
   binds each milestone to a structured event.
4. **D4 — synthetic provenance fallback hashes.** The
   `_render_reference_strict_config` helper now measures the
   helper source, observer patch, driver binary, and build
   manifest digests from the on-disk artifacts. The reference tree
   digest is read from the source-lock record and fails closed
   when the placeholder is still a zero-prefixed 64-hex string.
5. **D5 — the runner described a two-process lane while composing
   two i2pr launchers.** The role pair is unique per direction;
   the runner's `TYPED_BLOCKER_REFERENCE_PROCESS_NOT_EXECUTED`
   guard fires whenever the listener and dialer roles resolve to
   the same process role.
6. **D6 — event records did not bind to a binary digest.** The
   event validator now compares the event's
   `driver_binary_sha256`, `implementation`,
   `implementation_revision`, `local_router_hash_sha256`, and
   `peer_router_hash_sha256` against the run's measured values
   and the scenario-owned `delivery_status_message_id`. The
   `TYPED_BLOCKER_PROTOCOL_EVENT_UNPROVEN` blocker fires whenever
   any field mismatches.

## Typed blockers

The runner exposes the four Plan 075 typed blockers through
`TYPED_BLOCKER_CODES`:

- `runner-reference-process-not-executed`
- `runner-reference-events-missing`
- `runner-synthetic-provenance-rejected`
- `runner-protocol-event-unproven`

The static boundary check
`scripts/check-ntcp2-loopback-smoke-boundary.sh` enforces the
presence of all four markers in the runner plus the
`Plan075RunnerIntegrityTests` marker in the test suite.

## Focused tests and results

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

- `test_loopback_smoke.py` — 51 tests, all green. Covers the
  existing 42 Plan 069 tests plus the Plan 075
  `Plan075RunnerIntegrityTests` (9 cases):
  - `test_role_pair_is_unique_per_direction`
  - `test_reference_command_uses_run_script`
  - `test_i2pr_command_uses_launcher_binary`
  - `test_both_commands_resolve_to_i2pr_launcher_blocked`
  - `test_missing_reference_events_raises_typed_blocker`
  - `test_protocol_milestone_without_event_raises_typed_blocker`
  - `test_synthetic_provenance_rejected`
  - `test_no_synthetic_provenance_placeholders_in_runner`
  - `test_typed_blocker_constants_defined`
- `test_loopback_smoke_record.py` — 35 tests, all green.
- `test_reference_event.py` — 13 tests, all green.
- `bash scripts/check-ntcp2-loopback-smoke-boundary.sh` —
  passes. Verifies the runner, shell, test, and plan artifacts
  are present; verifies the smoke record schema is referenced;
  verifies the runner is free of release/rootless/Multipass
  authority; verifies the Plan 075 typed blockers and the
  `ProcessRole`/`TransportRole` markers are present in the
  runner; verifies the four `Plan075RunnerIntegrityTests`
  classifications are present in the test module.
- `cargo fmt --all --check` — passes.
- `cargo check --workspace --all-targets` — passes.
- `cargo test --workspace` — 235 tests across 27 suites pass.
- `git diff --check` — no whitespace errors.

## Plan 075 closure posture on this host

Plan 075 closes on this host with the typed environment blocker
that the constraints impose. The pinned i2pd source tree has not
been built on this host since the initial Plan 046 fiasco, so the
canonical sealed-topology qualification is unavailable. The
strict-config renderer therefore raises
`source-lock-reference-tree-placeholder` when the supplied
source-lock record carries the original zero-prefixed reference
tree placeholder. The runner never falls back to a synthetic
digest; the build preflight raises the typed blocker before any
process is launched.

The runner code is now structurally incapable of producing a
passing mixed-router record without a real reference binary, a
real source-lock tree digest, and a real structured event stream
satisfying the four protocol milestones. The remaining closure
work is the responsibility of Plan 076 (real pinned i2pd library
and direct driver construction), Plan 077 (constrained-host
execution lane), Plan 078 (first real two-way execution), and
Plan 079 (repeated development validation).

## Updated documentation

The amendment propagated by this plan also updated the active
status pages and skills:

- `README.md` — added Plan 074 and Plan 075 sections with the
  supersession notice and the corrected repository state.
- `AGENTS.md` — added Plan 074 and Plan 075 sections and the
  updated Plan 069 reclassification.
- `docs/architecture/interop-apparatus.md` — added the Plan 074
  corrective roadmap and the Plan 075 runner integrity section.
- `docs/protocol-support.md` — added the Plan 074 and Plan 075
  sections.
- `tests/integration/ntcp2/README.md` — added the Plan 074 and
  Plan 075 sections.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` — added the
  Plan 074 and Plan 075 sections.
- `plans/030-milestone-3-closure.md` — added the Plan 074 active
  correction banner.
- `plans/069-status.md` — added the Plan 074 supersession note
  that documents the scaffolding reclassification.

## Next plan

The next plan is **Plan 076** — real pinned i2pd library and
direct driver construction. Plan 076 closes the D1–D8 i2pd
driver defects so the corrected Plan 075 runner can be exercised
against a real, source-locked i2pd binary. The active sequence
remains **Plan 075 → Plan 076 → Plan 077 → Plan 078 → Plan 079**.
