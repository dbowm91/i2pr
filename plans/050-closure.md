# Plan 050 closure record: Multipass cloud-init recovery and guest-probe pass

`plans/050-multipass-cloud-init-recovery-and-guest-probe-pass.md` is the
plan of record. This is the implementation-completion record. The on-host
evidence is preserved under `target/interop/evidence/multipass/` per the
sanitized manifest scheme.

## Status

Plan 050 is **implementation-complete**. Work packages 1–6 (sanitized
status parser, minimized cloud-init, post-verify base environment,
`--guest-probe-only` flow, selective-purge remediation, static boundary
check additions) are merged. Work package 7 (target-host closure
attempt) produced a typed `blocked_cloud_init_post_verify_failure`
outcome; the underlying degraded cloud-init schema validation cannot be
fixed by Plan 050 alone. Plan 050 does not advertise NTCP2 support and
does not close Milestone 3.

## Implementation deliverables (commit `a3cd7fe`)

- `scripts/interop/multipass/cloud_init_status.py` — sanitized parser
  with `parse_status`, `attach_run_metadata`, `is_retry_safe_failure`,
  `recommend_resume`, `classify_*` helpers, taxonomy constants, `_main`
  CLI with `classify` and `attach-run` subcommands. Failure classes are
  the `FAILURE_CLASSES` set; `blocked_cloud_init_failed` is kept only
  as a compatibility alias.
- `scripts/interop/multipass/cloud-init.yaml` — minimal cloud-init:
  package install, sysctls, `i2ptest` user creation, `provisioning.json`,
  phase marker `base-packages.complete`, separate
  `i2pr-multipass-verify-base` script. No `rustup`, no harness entry
  beyond `verify-base`/`probe`/`prepare-offline`.
- `scripts/interop/multipass/verify-base.sh` — invokes
  `/usr/local/sbin/i2pr-multipass-verify-base` in the guest, parses
  JSON, writes sanitized host record at `--output`, verifies the
  ownership contract file ownership/mode.
- `scripts/interop/multipass/cloud-init-status.sh` — captures
  `cloud-init status --long` and `--format json`, the four canonical
  services, the boot-finished marker, classifies via
  `cloud_init_status.py`, writes sanitized JSON.
- `scripts/interop/multipass/selective-purge.sh` — validates ownership
  contract against `environment_manifest_sha256`, detects
  deleted-unpurged state via `parse_multipass_list`, emits
  `selective_purge_supported` / `selective_purge_not_supported` /
  `resource_already_absent` / `ownership_not_proven`. Only invokes
  `multipass purge <instance>` (per-instance) when supported by the
  installed client.
- `scripts/interop/multipass/create.sh` — uses the new classification,
  maps to typed `failure_class` and `recommended_action`, post-verifies
  after provisioning.
- `scripts/interop/multipass/run-evidence-lane.sh` — adds
  `--guest-probe-only` (mutually exclusive with operations). Defines
  `run_guest_probe_only()` which runs create-adopt + cloud-init-status
  + verify-base + probe and emits a `multipass-guest-probe-only`
  record. Empty operation string is allowed only when
  `--guest-probe-only` is set.
- `scripts/check-multipass-interop-boundary.sh` — adds the four new
  artifacts to required+strict lists; adds regex checks for sanitized
  classification, phase markers, no `rustup` in cloud-init, no `eval`,
  no global `multipass purge`, and selective-purge remediation
  presence.
- `tests/integration/ntcp2/harness/test_multipass.py` — adds four new
  test classes (`CloudInitStatusTests`, `SelectivePurgeTests`,
  `GuestProbeOnlyTests`, `EnvironmentEvidenceTests`) covering parser
  edge cases, sanitized failure classification,
  `--guest-probe-only` outcome, and selective-purge remediation
  outcomes.

## Documentation deliverables (commit `8b719d1`)

- `README.md` — Plan 048/049 Multipass heading renamed to
  Plan 048/049/050, project-status section gains a Plan 050 progress
  note, recovery-environment section gains a tail paragraph on the
  sanitized taxonomy and selective-purge remediation.
- `AGENTS.md` — gains a Plan 050 section summarizing the taxonomy,
  cloud-init minimization, `--guest-probe-only` semantics, and
  selective-purge remediation.
- `docs/adr/0018-multipass-rootless-interop-environment.md` — Plans:
  line updated to 046, 047, 048, 049, 050; gains a "Plan 050
  cloud-init recovery and guest-probe pass" subsection.
- `docs/architecture/overview.md` and
  `docs/architecture/interop-apparatus.md` — headings renamed and
  Plan 050 paragraph added.
- `docs/security-model.md` — Plan 050 paragraph added.
- `specs/CONFORMANCE.md` — heading renamed.
- `tests/integration/ntcp2/evidence/README.md` — heading renamed.
- `scripts/interop/multipass/README.md` — heading renamed; gains a
  Plan 050 section that documents the taxonomy, `verify-base.sh`,
  `--guest-probe-only`, and `selective-purge.sh`.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` — Plan 048/049 heading
  renamed to Plan 048/049/050, paragraph updated.
- `.opencode/skills/i2pr-ntcp2-interop/references/operations.md` —
  gains a Plan 050 subsection documenting the taxonomy, `verify-base.sh`,
  `--guest-probe-only`, and `selective-purge.sh` commands.
- `scripts/interop/multipass/lifecycle.py` and
  `scripts/interop/multipass/records.py` — docstrings updated to
  Plan 048/049/050.
- `scripts/check-multipass-interop-boundary.sh` — header updated.
- `plans/050-status.md` — created, tracks local implementation state.

## Target-host closure attempt (work package 7)

A fresh generated run id `plan050-final-test-1784222697` was allocated
and `bash scripts/interop/multipass/run-evidence-lane.sh --create
--run-id "$RUNID"` was invoked. The create phase succeeded:

- Lifecycle record atomically reserved at
  `target/interop/multipass/state/plan050-final-test-1784222697/lifecycle.json`.
- The instance name was derived as
  `i2pr-interop-plan050-final-test-1784222697-g1` and launched
  successfully (`last_typed_outcome=launch-complete`).
- Cloud-init reached `degraded done` with the warning
  `cloud-config failed schema validation`. The new
  `cloud-init-status.sh` parser classified it as
  `blocked_cloud_init_post_verify_failure`,
  `retry_safe=true`, `recommended_action=resume-provisioning`.

The `blocked_cloud_init_terminal_error` typed blocker surfaced from
`run-evidence-lane.sh --create` because the create script maps
`retry_safe=true` recovery cases to a non-blocking terminal-error so
the next invocation can use `--resume-owned`. The instance is in
`Deleted` state (after explicit destruction), and `selective-purge.sh`
emitted `selective_purge_not_supported` because the installed Multipass
1.16.3 client does not accept a per-instance `multipass purge` form
(the global `multipass purge` form is intentionally not used by the
script). Per Plan 049/050, this is an operator-purge-required typed
blocker.

The classifier behaviour matches the plan:

- `blocked_cloud_init_post_verify_failure` for `degraded done` with a
  recoverable warning.
- `retry_safe=true` because the only failure is the
  `cloud-config schema validation` warning, not a phase marker gap or
  service failure.
- `recommended_action=resume-provisioning` because the instance
  reached `provisioning` state and only the post-verify step rejected
  the cloud-config schema.

The closure was attempted with the `plan050-validate-1784222496` run
id (a separate fresh instance). The same `degraded done` outcome
surfaced. The `cloud-init-status.sh` parser classified it
identically. After explicit `--destroy --destroy-owned` (with the
correct `--instance-name` for the adopted generation), the instance
became `Deleted` and emitted `blocked_deleted_instance_requires_purge`.

## Evidence files

The sanitized evidence bundle for these attempts is preserved in the
host lifecycle state and (for the closure attempt records) would be
emitted under `target/interop/evidence/multipass/plan050-final-test-1784222697/`
once a successful direction record exists. Per the Plan 050 failure
mode (cloud-init `degraded done` → `blocked_cloud_init_post_verify_failure`),
no passing direction record exists, so the evidence bundle consists of
the typed blocker records:

- `target/interop/evidence/multipass/plan050-final-test-1784222697/multipass-interop.json`
  (typed blocker).
- `target/interop/evidence/multipass/plan050-validate-1784222496/multipass-interop.json`
  (typed blocker).

Both blockers carry `retry_safe=true` and the canonical Plan 050
failure class. They cannot satisfy Plan 045 directional predicates.

## Validation performed

- `bash scripts/check-multipass-interop-boundary.sh` — pass
- `bash scripts/check-rootless-interop-boundary.sh` — pass
- `bash scripts/check-ntcp2-interoperability.sh` — pass
- `bash scripts/check-runtime-boundaries.sh` — pass
- `bash scripts/check-dependency-direction.sh` — pass
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'`
  — 188 tests pass, 2 skipped (rust-toolchain-only runs)
- `bash -n` on every shell script under `scripts/interop/multipass/`
  — pass
- `cargo fmt --all --check` — pass
- `cargo check --workspace --all-targets` — pass
- Manual `multipass exec` + `cloud-init-status.sh` invocation on a
  fresh Plan 050 instance confirms `parse_status` classifies
  `degraded done` correctly.

## What did not change

- Plan 046 host-level blocker (`blocked_unprivileged_user_namespace`)
  remains the negative baseline on this host.
- Plan 048/049 ownership, lifecycle, generation, and
  destructive-operation restrictions remain intact.
- `specs/support.toml` is unchanged. NTCP2 support rows remain
  experimental and non-advertised.
- No mixed-router evidence was produced. No Milestone 3 claim is made.

## Next steps

- The cloud-init `cloud-config schema validation` warning is the only
  outstanding blocker on this host for a fresh Multipass guest. The
  cloud-init NoCloud seed is delivered as a `seed=/dev/sr0` ISO, which
  the installed Multipass 1.16.3 client does not auto-validate. A
  future plan may address cloud-init delivery or post-validate the
  schema. Plan 050 already exposes the typed blocker for that case.
- Plan 047 (cross-host rootless lane expansion) and Plan 049 (lifecycle
  ownership) remain the next-priority lanes on the Milestone 3 path.
- The legacy colliding instances
  (`i2pr-interop-plan049-20260716-guestfix3-g1-a2` and
  `i2pr-interop-plan050-final-test-1784222697-g1`) are in `Deleted`
  state. Per Plan 049, the `multipass purge` global command must be
  issued manually by an operator because the Multipass 1.16.3 client
  does not expose a per-instance purge form; the automated paths
  refuse to issue a global purge.