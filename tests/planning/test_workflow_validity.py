"""Plan 365 — negative tests for `scripts/check-workflow-validity.py`.

Every case uses a temp-dir fixture tree. The repository's real
`.github/workflows/` is only ever read, never mutated.

The regression this guard exists for is reproduced exactly in
`test_deindented_step_fails`: a merge resolution that dropped six spaces of
indentation from one step line, which made the whole workflow unparseable and
silently stopped every job in it from running.
"""

from __future__ import annotations

import importlib.util
import io
import sys
import shutil
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SCRIPT = REPO_ROOT / "scripts" / "check-workflow-validity.py"


def _load_guard():
    spec = importlib.util.spec_from_file_location("workflow_validity_guard", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


GUARD = _load_guard()


def _workflow_body(jobs: str = "", triggers: str = "  push:\n    branches: [main]\n") -> str:
    return f"name: test\non:\n{triggers}jobs:\n  quality:\n    runs-on: ubuntu-latest\n{jobs}"


class WorkflowValidityTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.mkdtemp(prefix="i2pr-workflow-validity-")
        self.root = Path(self._tmp)
        self.workflows = self.root / ".github" / "workflows"
        self.workflows.mkdir(parents=True)

    def tearDown(self) -> None:
        shutil.rmtree(self._tmp, ignore_errors=True)

    def write(self, name: str, text: str) -> Path:
        path = self.workflows / name
        path.write_text(text, encoding="utf-8")
        return path

    # --- controls -------------------------------------------------------

    def test_control_real_workflows_are_valid(self) -> None:
        """The guard passes on the repository's real workflows."""
        self.assertEqual(GUARD.check_workflows(REPO_ROOT), [])

    def test_control_minimal_workflow_is_valid(self) -> None:
        self.write("ci.yml", _workflow_body("    steps:\n      - name: A\n        run: true\n"))
        self.assertEqual(GUARD.check_workflows(self.root), [])

    def test_control_reusable_workflow_job_needs_no_steps(self) -> None:
        body = "name: t\non:\n  push:\njobs:\n  call:\n    uses: ./.github/workflows/ci.yml\n"
        self.write("ci.yml", body)
        self.assertEqual(GUARD.check_workflows(self.root), [])

    # --- the actual regression -----------------------------------------

    def test_deindented_step_fails(self) -> None:
        """A merge that drops a step's indentation must fail.

        This is verbatim the defect found at `0d50319`: one `- name:` line at
        column 0 among six-space-indented siblings. The whole file stops
        parsing, so the entire workflow silently stops running.
        """
        good = _workflow_body(
            "    steps:\n      - name: Check A\n        run: true\n"
            "      - name: Check B\n        run: true\n"
        )
        broken = good.replace("      - name: Check B", "- name: Check B")
        self.assertNotEqual(good, broken, "fixture must actually be de-indented")
        self.write("ci.yml", broken)

        violations = GUARD.check_workflows(self.root)
        self.assertTrue(violations, "de-indented step must be rejected")
        self.assertTrue(
            any("does not parse as YAML" in v for v in violations),
            f"expected a YAML parse violation, got {violations}",
        )
        self.assertTrue(
            any("ci.yml:" in v for v in violations),
            f"violation must name the file and line, got {violations}",
        )

    # --- other negative cases -------------------------------------------

    def test_malformed_yaml_fails_with_position(self) -> None:
        self.write("ci.yml", "name: t\non:\n  push:\njobs:\n  q:\n   bad: [1, 2\n")
        violations = GUARD.check_workflows(self.root)
        self.assertTrue(any("does not parse as YAML" in v for v in violations), violations)

    def test_missing_on_trigger_fails(self) -> None:
        body = "name: t\njobs:\n  q:\n    runs-on: ubuntu-latest\n    steps:\n      - name: A\n        run: true\n"
        self.write("ci.yml", body)
        violations = GUARD.check_workflows(self.root)
        self.assertTrue(any("no `on:` trigger" in v for v in violations), violations)

    def test_no_jobs_fails(self) -> None:
        self.write("ci.yml", "name: t\non:\n  push:\n    branches: [main]\n")
        violations = GUARD.check_workflows(self.root)
        self.assertTrue(any("no `jobs:`" in v for v in violations), violations)

    def test_step_with_neither_run_nor_uses_fails(self) -> None:
        self.write("ci.yml", _workflow_body("    steps:\n      - name: A\n"))
        violations = GUARD.check_workflows(self.root)
        self.assertTrue(any("neither `run:` nor `uses:`" in v for v in violations), violations)

    def test_unnamed_step_fails(self) -> None:
        self.write("ci.yml", _workflow_body("    steps:\n      - run: true\n"))
        violations = GUARD.check_workflows(self.root)
        self.assertTrue(any("has no `name:`" in v for v in violations), violations)

    def test_duplicate_step_name_fails(self) -> None:
        self.write(
            "ci.yml",
            _workflow_body("    steps:\n      - name: Same\n        run: true\n      - name: Same\n        run: true\n"),
        )
        violations = GUARD.check_workflows(self.root)
        self.assertTrue(any("duplicate step name" in v for v in violations), violations)

    def test_non_mapping_root_fails(self) -> None:
        self.write("ci.yml", "- just\n- a\n- list\n")
        violations = GUARD.check_workflows(self.root)
        self.assertTrue(any("root must be a mapping" in v for v in violations), violations)

    # --- fail-closed behaviour ------------------------------------------

    def test_missing_workflow_directory_fails_closed(self) -> None:
        empty = self.root / "empty"
        empty.mkdir()
        with self.assertRaises(RuntimeError):
            GUARD.check_workflows(empty)

    def test_empty_workflow_directory_fails_closed(self) -> None:
        bare = self.root / "bare"
        (bare / ".github" / "workflows").mkdir(parents=True)
        with self.assertRaises(RuntimeError):
            GUARD.check_workflows(bare)

    def test_main_returns_two_when_guard_cannot_run(self) -> None:
        """A guard that cannot run must exit 2, never report success.

        `main()` resolves its repo root from the module's own `__file__`, so the
        temp repo is injected by repointing that, not by changing directory.
        """
        with tempfile.TemporaryDirectory() as tmp:
            broken_repo = Path(tmp) / "repo"
            (broken_repo / "scripts").mkdir(parents=True)
            (broken_repo / "scripts" / "check-workflow-validity.py").write_text(
                SCRIPT.read_text(encoding="utf-8"), encoding="utf-8"
            )

            original_file = GUARD.__file__
            try:
                GUARD.__file__ = str(broken_repo / "scripts" / "check-workflow-validity.py")
                self.assertEqual(GUARD.main([]), 2)
            finally:
                GUARD.__file__ = original_file


    def test_main_returns_one_when_a_workflow_is_invalid(self) -> None:
            """`main()` must map a violation to exit 1.

            Mutation M5 (`return 1` -> `return 0` on the violation path) escaped
            until this row existed, because the other rows exercise the pure
            `check_workflows` function rather than the process exit code.
            """
            with tempfile.TemporaryDirectory() as tmp:
                repo = Path(tmp) / "repo"
                scripts = repo / "scripts"
                workflows = repo / ".github" / "workflows"
                scripts.mkdir(parents=True)
                workflows.mkdir(parents=True)
                (scripts / "check-workflow-validity.py").write_text(
                    SCRIPT.read_text(encoding="utf-8"), encoding="utf-8"
                )
                # A step de-indented to column 0: the Plan-364-era merge regression.
                (workflows / "ci.yml").write_text(
                    _workflow_body("    steps:\n      - name: A\n        run: true\n      - name: B\n        run: true\n")
                    .replace("      - name: B", "- name: B"),
                    encoding="utf-8",
                )

                original_file = GUARD.__file__
                stdout, stderr = sys.stdout, sys.stderr
                try:
                    GUARD.__file__ = str(scripts / "check-workflow-validity.py")
                    captured = io.StringIO()
                    sys.stdout = io.StringIO()
                    sys.stderr = captured
                    code = GUARD.main([])
                finally:
                    GUARD.__file__ = original_file
                    sys.stdout, sys.stderr = stdout, stderr

                self.assertEqual(code, 1)
                self.assertIn("workflow validity check failed", captured.getvalue())


if __name__ == "__main__":
    unittest.main()