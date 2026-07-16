# Plan 050 — Implementation Status

`plans/050-multipass-cloud-init-recovery-and-guest-probe-pass.md` is the
plan of record. This document tracks local implementation progress only;
it is not a closure record.

## Work package state

| WP | Scope | Status |
|----|-------|--------|
| 1  | Sanitized cloud-init status parser (`cloud_init_status.py`) | done (commit `a3cd7fe`) |
| 2  | Minimized cloud-init (no rustup, base-packages phase marker, `i2pr-multipass-verify-base`) | done (commit `a3cd7fe`) |
| 3  | Post-verify base environment command (`verify-base.sh`) | done (commit `a3cd7fe`) |
| 4  | `--guest-probe-only` flow in `run-evidence-lane.sh` | done (commit `a3cd7fe`) |
| 5  | Selective-purge remediation (`selective-purge.sh`) | done (commit `a3cd7fe`) |
| 6  | Static boundary check additions (`check-multipass-interop-boundary.sh`) | done (commit `a3cd7fe`) |
| 7  | Target-host closure attempt (fresh run id, `--guest-probe-only`) | pending |
| 8  | Documentation/ADR/operations note updates | pending |

## What shipped in `a3cd7fe`

- `scripts/interop/multipass/cloud_init_status.py` — sanitized parser
  with `parse_status`, `attach_run_metadata`, `is_retry_safe_failure`,
  `recommend_resume`, `classify_*` helpers, taxonomy constants,
  `_main` CLI with `classify` and `attach-run` subcommands. Failure
  classes are the `FAILURE_CLASSES` set; `blocked_cloud_init_failed` is
  kept only as a compatibility alias for transition consumers.
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
  `multipass purge <instance>` (per-instance) when supported.
- `scripts/interop/multipass/create.sh` — uses the new
  classification, maps to typed `failure_class` and
  `recommended_action`, post-verifies after provisioning.
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

## Verification performed locally

- `bash scripts/check-multipass-interop-boundary.sh` — pass
- `bash scripts/check-rootless-interop-boundary.sh` — pass
- `bash scripts/check-ntcp2-interoperability.sh` — pass
- `bash scripts/check-runtime-boundaries.sh` — pass
- `bash scripts/check-dependency-direction.sh` — pass
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'`
  — 188 tests pass, 2 skipped (rust-toolchain-only runs)
- `bash -n` on every shell script under `scripts/interop/multipass/`
  — pass

## Next moves

- WP 7: target-host closure attempt with a fresh generated run id and
  the `--guest-probe-only` flow. If guest probe passes, attempt
  `--resume-owned` (cloud-init recovery safe) before invoking
  `run-matrix.sh`. If cloud-init is not retry-safe, use
  `selective-purge.sh` to clean up before re-creating.
- WP 8: update `README.md`, `AGENTS.md`,
  `docs/adr/0018-multipass-rootless-interop-environment.md`,
  `docs/architecture/interop-apparatus.md`,
  `scripts/interop/multipass/README.md`, and
  `.agents/skills/i2pr-ntcp2-interop/references/operations.md` to
  reflect Plan 050 deliverables.
- Closure: write `plans/050-closure.md` only after WP 7 produces a
  `rootless_sandbox_available` guest probe or a typed blocker with
  sanitized evidence.