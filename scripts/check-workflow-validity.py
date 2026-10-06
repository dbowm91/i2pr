#!/usr/bin/env python3
"""Plan 365 — CI workflow validity guard.

The routine floor ran `cargo`, shell guards, and Python guards, and every one of
them passed while `.github/workflows/ci.yml` did not parse as YAML at all. An
unparseable workflow means the whole file is rejected, so the `quality`, `msrv`,
and `dependency-policy` jobs never ran and every `check-*` step inside it was
inert. Nothing in the floor could see that.

The regression was a merge resolution that dropped six spaces of indentation on a
single step line. The YAML parser catches it, but the *structural* assertions
below catch it too and name it in terms a merge reviewer recognises.

Run with `python3`. `bash scripts/*.py` garbles these scripts and exits 2.

Exit codes: 0 = all workflows valid; 1 = at least one violation (listed on
stderr); 2 = the guard could not run (see `_require_yaml`).

Scope: this guard READS workflows. It never edits one.
"""

from __future__ import annotations

import sys
from pathlib import Path

LEDGER = "plans/implementation/workspace-foundation/365-workflow-validity-guard.md"


def _require_yaml():
    """Return the `yaml` module, or raise.

    Failing loudly is the point of this plan. A guard that cannot import its
    parser must not silently report success -- that is precisely the defect
    class being fixed (a floor step that passes while checking nothing).
    """
    try:
        import yaml  # noqa: PLC0415
    except ImportError as exc:  # pragma: no cover - environment defect
        raise RuntimeError(
            f"PyYAML is unavailable, so workflow validity cannot be checked: {exc}. "
            "Refusing to report success on an unchecked tree. Install PyYAML or add "
            "it deliberately; do not make this guard skip silently."
        ) from exc
    return yaml


def _workflow_files(root: Path) -> list[Path]:
    directory = root / ".github" / "workflows"
    if not directory.is_dir():
        raise RuntimeError(f"missing workflow directory: {directory}")
    files = sorted(
        p
        for p in directory.iterdir()
        if p.is_file() and p.suffix in (".yml", ".yaml")
    )
    if not files:
        raise RuntimeError(f"no workflow files found in {directory}")
    return files


def check_workflows(root: Path) -> list[str]:
    """Return a list of violation strings; empty means every workflow is valid."""
    yaml = _require_yaml()
    violations: list[str] = []

    for path in _workflow_files(root):
        rel = path.relative_to(root)
        try:
            data = yaml.safe_load(path.read_text(encoding="utf-8"))
        except yaml.YAMLError as exc:
            # The parser already gives us the line, which is what makes a merge
            # regression diagnosable instead of merely "invalid".
            mark = getattr(exc, "problem_mark", None)
            where = f"{rel}:{mark.line + 1}:{mark.column + 1}" if mark else str(rel)
            violations.append(f"{where}: workflow does not parse as YAML: {exc}")
            continue

        if not isinstance(data, dict):
            violations.append(
                f"{rel}: workflow root must be a mapping, got {type(data).__name__}"
            )
            continue

        # `on:` is spelled `True` when a workflow uses the bare `on` key and YAML
        # 1.1 resolves it to a boolean. Accept either, reject its absence.
        if "on" not in data and True not in data:
            violations.append(f"{rel}: workflow declares no `on:` trigger")

        jobs = data.get("jobs")
        if not isinstance(jobs, dict) or not jobs:
            violations.append(f"{rel}: workflow declares no `jobs:`")
            continue

        for job_id, job in jobs.items():
            if not isinstance(job, dict):
                violations.append(
                    f"{rel}: job {job_id!r} must be a mapping, got {type(job).__name__}"
                )
                continue
            _check_steps(rel, job_id, job, violations)

    return violations


def _check_steps(rel: Path, job_id: str, job: dict, violations: list[str]) -> None:
    steps = job.get("steps")
    if steps is None:
        # A job may legitimately be a reusable-workflow call (`uses:`) with no steps.
        if "uses" not in job:
            violations.append(f"{rel}: job {job_id!r} has neither `steps:` nor `uses:`")
        return
    if not isinstance(steps, list):
        violations.append(
            f"{rel}: job {job_id!r} `steps:` must be a list, got {type(steps).__name__}"
        )
        return

    seen: set[str] = set()
    for index, step in enumerate(steps):
        if not isinstance(step, dict):
            violations.append(
                f"{rel}: job {job_id!r} step {index} must be a mapping, "
                f"got {type(step).__name__}"
            )
            continue

        name = step.get("name")
        if not isinstance(name, str) or not name.strip():
            violations.append(f"{rel}: job {job_id!r} step {index} has no `name:`")
        elif name in seen:
            violations.append(
                f"{rel}: job {job_id!r} has duplicate step name {name!r}; "
                "CI logs become ambiguous about which step failed"
            )
        else:
            seen.add(name)

        if "run" not in step and "uses" not in step:
            violations.append(
                f"{rel}: job {job_id!r} step {name or index!r} has neither "
                "`run:` nor `uses:`"
            )


def main(argv: list[str]) -> int:
    root = Path(__file__).resolve().parent.parent
    try:
        violations = check_workflows(root)
    except RuntimeError as exc:
        print(f"workflow validity guard could not run: {exc}", file=sys.stderr)
        return 2

    if violations:
        print("CI workflow validity check failed:", file=sys.stderr)
        for violation in violations:
            print(f"- {violation}", file=sys.stderr)
        print(f"see {LEDGER}", file=sys.stderr)
        return 1

    count = len(_workflow_files(root))
    print(f"CI workflow validity: {count} workflow files parse and are structurally valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))