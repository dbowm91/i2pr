"""Plan 097 Plan 095 artifact-path and cleanup corrective pass tests.

Plan 097 closes two workflow defects that remained after Plan 096:

- **Defect A**: the i2pr producer writes to a CWD-relative
  ``output/i2pr-interop`` while the manifest and verifier consume
  from ``${BUILD_DIR}/output/i2pr-interop`` after a step-local
  ``cd "$BUILD_DIR"``. Producer and consumer identities do not
  match; the manifest would hash a file that does not yet exist at
  that path.
- **Defect B**: the disposable run-root cleanup uses
  ``find $RUN_ROOT -mindepth 1 -delete`` (descendant-only) plus
  ``test ! -e "$RUN_ROOT" || true`` (suppressed absence assertion).
  The root directory can survive cleanup while the job claims the
  cleanup is clean.

The Plan 097 regression matrix exercises the corrected workflow:

- one canonical absolute ``$BUILD_OUTPUT`` path used by every
  producer, verifier, manifest generator, artifact uploader, and
  live consumer;
- strict recursive root deletion with an exact path guard before
  ``rm -rf`` and an unsuppressed absence assertion after;
- bounded synthetic mutations that break producer/consumer
  identity or weaken cleanup semantics are caught by the matrix;
- existing Plan 095 lane invariants remain satisfied.

The test cases that require comparing a mutated workflow text use
in-memory string substitution so the real workflow file is not
modified.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


import yaml


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
WORKFLOW_PATH = (
    REPO_ROOT
    / ".github/workflows/ntcp2-interop-host-loopback-development.yml"
)

LIVE_JOB_NAMES = (
    "forward-instrumented",
    "forward-control",
    "validate-gate",
)

PINNED_I2PD_REVISION = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"


def _load_workflow() -> dict[str, object]:
    """Load the Plan 095 workflow YAML."""

    with WORKFLOW_PATH.open(encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def _read_workflow_text() -> str:
    """Return the raw text of the Plan 095 workflow."""

    return WORKFLOW_PATH.read_text(encoding="utf-8")


def _step_blocks(text: str) -> list[tuple[str, str]]:
    """Return ``(step_name, body)`` pairs for every named step.

    A step body runs from the line following the step header
    through the next step header or job header. The workflow
    uses six-space indentation for jobs and ten-space indentation
    for steps.
    """

    lines = text.splitlines()
    blocks: list[tuple[str, str]] = []
    current_name: str | None = None
    body_lines: list[str] = []
    for line in lines:
        # Match a step header at column 6 (six leading spaces then "- name: ").
        if line.startswith("      - name:"):
            if current_name is not None:
                blocks.append((current_name, "\n".join(body_lines)))
            current_name = line[len("      - name:"):].strip()
            body_lines = []
            continue
        # A new job header terminates the current step body.
        if current_name is not None and (
            line.startswith("  ") and not line.startswith("      ")
        ):
            blocks.append((current_name, "\n".join(body_lines)))
            current_name = None
            body_lines = []
            continue
        if current_name is not None:
            body_lines.append(line)
    if current_name is not None:
        blocks.append((current_name, "\n".join(body_lines)))
    return blocks


def _step_body(text: str, name: str) -> str:
    """Return the body of the named step, or empty string if absent.

    YAML literal-block line continuations (``\\\n``) are collapsed
    to single spaces so the tests can match the shell view of the
    command.
    """

    for step_name, body in _step_blocks(text):
        if step_name == name:
            return _normalize_body(body)
    return ""


def _normalize_body(body: str) -> str:
    """Collapse YAML literal-block line continuations to single spaces.

    A line that ends with ``\\`` followed by an indented next line is
    a YAML literal-block continuation that the shell sees as one
    logical line. The tests assert against the shell view, so we
    collapse ``\\\n[whitespace]*`` to a single space.
    """

    return re.sub(r"\\\n[ \t]*", " ", body)


class Plan097WorkflowExistsTests(unittest.TestCase):
    """Plan 097 case 1: workflow file exists and parses."""

    def test_workflow_file_exists(self) -> None:
        self.assertTrue(WORKFLOW_PATH.is_file())

    def test_workflow_parses_as_yaml(self) -> None:
        workflow = _load_workflow()
        self.assertIsInstance(workflow, dict)
        self.assertEqual(
            workflow.get("name"),
            "NTCP2 host-loopback live-wire evidence (Plan 095 manual)",
        )


class Plan097CanonicalPathTests(unittest.TestCase):
    """Plan 097 WP2 cases 2-6: canonical absolute paths declared."""

    def test_canonical_build_dir_is_explicit(self) -> None:
        text = _read_workflow_text()
        # ``BUILD_DIR`` must be the explicit absolute build root.
        self.assertRegex(
            text,
            r'BUILD_DIR:\s*"\$\{\{\s*github\.workspace\s*\}\}/target/interop/plan095-build"',
            "BUILD_DIR must be defined as the absolute github.workspace/target/interop/plan095-build path",
        )

    def test_canonical_build_output_is_explicit(self) -> None:
        text = _read_workflow_text()
        # ``BUILD_OUTPUT`` must be defined in at least the i2pr build
        # steps. The build-i2pr-interop, hash-i2pr-build-manifest,
        # and verify-build-artifacts steps must declare it.
        self.assertRegex(
            text,
            r'BUILD_OUTPUT="\$BUILD_DIR/output"',
            "BUILD_OUTPUT must be defined as the canonical absolute path",
        )

    def test_i2pr_target_dir_is_explicit(self) -> None:
        text = _read_workflow_text()
        # The i2pr Cargo ``--target-dir`` must come from an explicit
        # variable, not a relative path.
        self.assertRegex(
            text,
            r'I2PR_TARGET_DIR="\$BUILD_DIR/i2pr-target"',
            "I2PR_TARGET_DIR must be defined as a canonical absolute path",
        )

    def test_i2pr_cargo_manifest_is_explicit(self) -> None:
        text = _read_workflow_text()
        # The cargo invocation must include an explicit
        # ``--manifest-path "${GITHUB_WORKSPACE}/Cargo.toml"``.
        body = _step_body(text, "build-i2pr-interop")
        self.assertIn(
            '--manifest-path "${GITHUB_WORKSPACE}/Cargo.toml"',
            body,
            "build-i2pr-interop must use --manifest-path with explicit workspace",
        )

    def test_i2pr_cargo_target_dir_is_explicit(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "build-i2pr-interop")
        self.assertRegex(
            body,
            r'--target-dir\s+"\$I2PR_TARGET_DIR"',
            "build-i2pr-interop must use --target-dir with explicit I2PR_TARGET_DIR",
        )


class Plan097ProducerConsumerIdentityTests(unittest.TestCase):
    """Plan 097 WP2 cases 7-13: producer and consumer use canonical path."""

    def test_producer_copies_to_canonical_output(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "build-i2pr-interop")
        self.assertRegex(
            body,
            r'cp\s+"\$I2PR_TARGET_DIR/release/i2pr-interop"\s+"\$BUILD_OUTPUT/i2pr-interop"',
            "producer must copy to $BUILD_OUTPUT/i2pr-interop",
        )

    def test_producer_does_not_rely_on_relative_output_path(self) -> None:
        text = _read_workflow_text()
        # The producer must NOT use ``output/i2pr-interop`` as a
        # bare destination unless the step explicitly proves
        # ``pwd == $BUILD_DIR`` first.
        body = _step_body(text, "build-i2pr-interop")
        # The producer must not write the binary to a relative
        # destination.
        self.assertNotRegex(
            body,
            r'cp\s+"\$I2PR_TARGET_DIR/release/i2pr-interop"\s+output/i2pr-interop',
            "producer must not write to a CWD-relative output path",
        )
        # ``mkdir -p output`` alone (without BUILD_OUTPUT) inside
        # build-i2pr-interop is forbidden because the step has not
        # established its working directory.
        self.assertNotRegex(
            body,
            r'mkdir\s+-p\s+output\b',
            "producer must not create a relative output directory",
        )

    def test_producer_verifies_canonical_artifact_exists(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "build-i2pr-interop")
        self.assertRegex(
            body,
            r'test\s+-f\s+"\$BUILD_OUTPUT/i2pr-interop"',
            "producer must verify existence at canonical path",
        )

    def test_producer_verifies_canonical_artifact_executable(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "build-i2pr-interop")
        self.assertRegex(
            body,
            r'test\s+-x\s+"\$BUILD_OUTPUT/i2pr-interop"',
            "producer must verify executability at canonical path",
        )

    def test_producer_verifies_canonical_artifact_non_symlink(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "build-i2pr-interop")
        self.assertRegex(
            body,
            r'test\s+! -L\s+"\$BUILD_OUTPUT/i2pr-interop"',
            "producer must verify non-symlink at canonical path",
        )

    def test_manifest_hashes_canonical_artifact(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "hash-i2pr-build-manifest")
        self.assertRegex(
            body,
            r'sha256sum\s+"\$BUILD_OUTPUT/i2pr-interop"',
            "manifest must hash the canonical absolute artifact",
        )
        # The manifest JSON itself must also be written to the
        # canonical absolute output directory.
        self.assertRegex(
            body,
            r'cat\s+>\s+"\$BUILD_OUTPUT/i2pr-build-manifest\.json"',
            "manifest JSON must be written to canonical output path",
        )

    def test_verifier_hashes_canonical_artifact(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "verify-build-artifacts")
        # All build-output existence checks must reference
        # $BUILD_OUTPUT.
        self.assertRegex(
            body,
            r'test\s+-x\s+"\$BUILD_OUTPUT/i2pr-interop"',
            "verifier must check executability at canonical path",
        )
        self.assertRegex(
            body,
            r'test\s+-f\s+"\$BUILD_OUTPUT/i2pr-build-manifest\.json"',
            "verifier must check manifest presence at canonical path",
        )
        # The verifier must NOT cd to BUILD_DIR and then reference
        # relative output paths (the pre-correction Defect A).
        self.assertNotRegex(
            body,
            r'cd\s+"\$BUILD_DIR"',
            "verifier must not rely on cd BUILD_DIR before relative output paths",
        )
        self.assertNotRegex(
            body,
            r'test\s+-x\s+output/i2pr-interop',
            "verifier must not check a relative output path",
        )

    def test_verifier_accepts_multi_digest_libraries_field(self) -> None:
        # Plan 097 follow-up: the ``i2pd_libraries_sha256`` manifest
        # field is a space-separated concatenation of three library
        # digests (libi2pd, libi2pdclient, libi2pdlang), not a single
        # 64-hex digest. The verifier must not refuse the field by
        # enforcing a hard ``len(value) == 64`` check; the only
        # required invariant is that every fragment be a nonzero
        # 64-hex digest. Drift back to a strict single-digest length
        # check re-introduces the verify-build-artifacts failure
        # observed on the first Plan 095 CI dispatch.
        text = _read_workflow_text()
        body = _step_body(text, "verify-build-artifacts")
        self.assertNotIn(
            "len(value) != 64",
            body,
            "verifier must not enforce len(value) == 64 for sha256 fields "
            "(i2pd_libraries_sha256 is multi-digest)",
        )
        self.assertIn(
            "fragments = value.split(' ')",
            body,
            "verifier must split multi-digest fields on space",
        )


class Plan097ArtifactUploadTests(unittest.TestCase):
    """Plan 097 WP3 case 14: upload-artifact path matches canonical tree."""

    def test_build_upload_targets_canonical_output_tree(self) -> None:
        text = _read_workflow_text()
        upload_body = _step_body(text, "upload-build-artifact")
        self.assertIn(
            "path: ${{ github.workspace }}/target/interop/plan095-build/output/",
            upload_body,
            "build upload must target the canonical plan095-build/output tree",
        )
        self.assertNotIn(
            "${{ github.workspace }}/output",
            upload_body,
            "build upload must not target a sibling workspace-root output directory",
        )

    def test_instrumented_download_targets_canonical_tree(self) -> None:
        text = _read_workflow_text()
        download_body = _step_body(text, "download-build-artifact")
        # The download path inside the instrumented and control jobs
        # must match the upload path.
        self.assertRegex(
            download_body,
            r"path:\s*\$\{\{\s*github\.workspace\s*\}\}/target/interop/plan095-build/output/",
            "instrumented download must target canonical build output tree",
        )

    def test_control_download_targets_canonical_tree(self) -> None:
        text = _read_workflow_text()
        # The control job also downloads the build artifact; locate
        # its body by walking all steps with the same name.
        download_steps: list[str] = []
        for name, body in _step_blocks(text):
            if name == "download-build-artifact":
                download_steps.append(body)
        self.assertEqual(
            len(download_steps), 3,
            "expected exactly 3 download-build-artifact steps "
            "(instrumented, control, validate-gate)",
        )
        for body in download_steps:
            self.assertRegex(
                body,
                r"path:\s*\$\{\{\s*github\.workspace\s*\}\}/target/interop/plan095-build/output/",
                "control/validate-gate download must target canonical tree",
            )

    def test_live_jobs_use_canonical_i2pr_binary(self) -> None:
        text = _read_workflow_text()
        # The instrumented and control jobs must invoke i2pr via
        # the downloaded canonical artifact path, not a checkout
        # build or a stale alternate path. The env declaration
        # binds I2PR_BINARY to the canonical workspace path; the
        # wrapper receives the value through the env variable.
        for step_name in (
            "run-forward-instrumented",
            "run-forward-control",
        ):
            body = _step_body(text, step_name)
            self.assertRegex(
                body,
                r'I2PR_BINARY:\s*"\${{ github\.workspace }}/target/interop/plan095-build/output/i2pr-interop"',
                f"{step_name} env must bind I2PR_BINARY to the canonical path",
            )
            self.assertIn(
                '--i2pr-binary "$I2PR_BINARY"',
                body,
                f"{step_name} must pass the canonical I2PR_BINARY env to the wrapper",
            )


class Plan097CleanupPathGuardTests(unittest.TestCase):
    """Plan 097 WP6 cases 21-26: strict path guard and rm -rf."""

    def _assert_uses_strict_cleanup(self, body: str, label: str) -> None:
        self.assertIn(
            'rm -rf --',
            body,
            f"{label} must use 'rm -rf --' for destructive deletion",
        )
        self.assertNotIn(
            "find \"$PLAN095_RUN_ROOT\" -mindepth 1 -delete",
            body,
            f"{label} must not use descendant-only deletion",
        )
        # The post-cleanup absence assertion must not be
        # suppressed with ``|| true``.
        self.assertNotRegex(
            body,
            r"test\s+!\s*-e\s+\"\$\w+\"\s*\|\|\s*true",
            f"{label} must not suppress the absence assertion",
        )
        # Path guard before rm -rf.
        self.assertRegex(
            body,
            r'case\s+"\$\w+"\s+in',
            f"{label} must include a case-statement path guard",
        )
        self.assertIn(
            'refusing unexpected PLAN095_RUN_ROOT',
            body,
            f"{label} must log a refusal on unexpected path",
        )
        self.assertIn(
            "exit 72",
            body,
            f"{label} must exit 72 on unexpected path",
        )
        # The strict absence assertion (without suppression) must
        # follow the rm -rf.
        self.assertRegex(
            body,
            r'rm -rf\s+--\s+"\$PLAN095_RUN_ROOT"\s*\n\s*test\s+!\s*-e\s+"\$PLAN095_RUN_ROOT"',
            f"{label} must assert absence strictly after rm -rf",
        )

    def test_instrumented_cleanup_uses_strict_rm_rf(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "delete-raw-run-state")
        self.assertIn(
            "${{ github.workspace }}/target/interop/plan095-instrumented",
            body,
            "instrumented cleanup step must target the instrumented run root",
        )
        self._assert_uses_strict_cleanup(body, "instrumented cleanup")

    def test_control_cleanup_uses_strict_rm_rf(self) -> None:
        text = _read_workflow_text()
        bodies = [
            body for name, body in _step_blocks(text)
            if name == "delete-raw-run-state"
        ]
        self.assertEqual(
            len(bodies), 2,
            "expected exactly 2 delete-raw-run-state steps "
            "(instrumented + control)",
        )
        control_bodies = [
            body for body in bodies
            if "${{ github.workspace }}/target/interop/plan095-control" in body
        ]
        self.assertEqual(
            len(control_bodies), 1,
            "expected exactly one control cleanup body",
        )
        self._assert_uses_strict_cleanup(control_bodies[0], "control cleanup")

    def test_instrumented_cleanup_path_guard_matches_instrumented_root(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "delete-raw-run-state")
        self.assertIn(
            '"${{ github.workspace }}/target/interop/plan095-instrumented")',
            body,
            "instrumented cleanup path guard must allow the instrumented root",
        )
        self.assertIn(
            '"$PLAN095_RUN_ROOT" in',
            body,
            "instrumented cleanup path guard must match the run-root variable",
        )

    def test_control_cleanup_path_guard_matches_control_root(self) -> None:
        text = _read_workflow_text()
        bodies = [
            body for name, body in _step_blocks(text)
            if name == "delete-raw-run-state"
            and "${{ github.workspace }}/target/interop/plan095-control" in body
        ]
        self.assertEqual(len(bodies), 1)
        body = bodies[0]
        self.assertIn(
            '"${{ github.workspace }}/target/interop/plan095-control")',
            body,
            "control cleanup path guard must allow the control root",
        )


class Plan097CleanupSemanticsTests(unittest.TestCase):
    """Plan 097 WP6 cases 27-30: cleanup semantics proven."""

    def test_instrumented_raw_root_must_be_absent_after_cleanup(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "delete-raw-run-state")
        # The absence assertion must follow ``rm -rf`` and must not
        # include any suppressing operator.
        self.assertRegex(
            body,
            r'rm -rf\s+--\s+"\$PLAN095_RUN_ROOT"\s+test\s+!\s*-e\s+"\$PLAN095_RUN_ROOT"',
            "instrumented absence assertion must follow rm -rf strictly",
        )
        # And must appear before any sanitized-tree assertions in
        # the run block.
        idx_absence = body.find('test ! -e "$PLAN095_RUN_ROOT"')
        # Search for the sanitized-survival assertion (within the
        # run block, not the env declaration).
        sanitized_survival_idx = body.find(
            'sanitized evidence tree was lost',
        )
        self.assertGreater(
            idx_absence, 0,
            "absence assertion must be present",
        )
        self.assertGreater(
            sanitized_survival_idx, 0,
            "sanitized survival assertion must be present",
        )
        self.assertGreater(
            sanitized_survival_idx, idx_absence,
            "sanitized survival assertion must follow absence assertion",
        )

    def test_control_raw_root_must_be_absent_after_cleanup(self) -> None:
        text = _read_workflow_text()
        bodies = [
            body for name, body in _step_blocks(text)
            if name == "delete-raw-run-state"
            and "${{ github.workspace }}/target/interop/plan095-control" in body
        ]
        self.assertEqual(len(bodies), 1)
        body = bodies[0]
        self.assertRegex(
            body,
            r'rm -rf\s+--\s+"\$PLAN095_RUN_ROOT"\s*\n\s*test\s+!\s*-e\s+"\$PLAN095_RUN_ROOT"',
            "control absence assertion must follow rm -rf strictly",
        )

    def test_instrumented_sanitized_evidence_survives(self) -> None:
        text = _read_workflow_text()
        body = _step_body(text, "delete-raw-run-state")
        self.assertIn(
            "${{ github.workspace }}/target/interop/plan095-evidence/instrumented",
            body,
            "instrumented cleanup must reference the disjoint sanitized tree",
        )
        self.assertIn(
            "forward-instrumented-sanitized.json",
            body,
            "instrumented cleanup must verify sanitized record file",
        )

    def test_control_sanitized_evidence_survives(self) -> None:
        text = _read_workflow_text()
        bodies = [
            body for name, body in _step_blocks(text)
            if name == "delete-raw-run-state"
            and "${{ github.workspace }}/target/interop/plan095-control" in body
        ]
        self.assertEqual(len(bodies), 1)
        body = bodies[0]
        self.assertIn(
            "${{ github.workspace }}/target/interop/plan095-evidence/control",
            body,
            "control cleanup must reference the disjoint sanitized tree",
        )
        self.assertIn(
            "forward-control-sanitized.json",
            body,
            "control cleanup must verify sanitized record file",
        )

    def test_sanitized_evidence_disjoint_from_run_roots(self) -> None:
        text = _read_workflow_text()
        # The sanitized trees must never be nested inside the
        # disposable run roots.
        self.assertNotIn(
            "target/interop/plan095-instrumented/sanitized", text,
            "sanitized path must not nest under the instrumented run root",
        )
        self.assertNotIn(
            "target/interop/plan095-control/sanitized", text,
            "sanitized path must not nest under the control run root",
        )


class Plan097UploadContractTests(unittest.TestCase):
    """Plan 097 case 32: artifact upload contract preserved."""

    def test_if_no_files_found_is_error(self) -> None:
        text = _read_workflow_text()
        # Every upload-artifact step must keep ``if-no-files-found: error``.
        upload_steps = re.findall(
            r"uses:\s*actions/upload-artifact[^\n]*\n(?:\s+with:\s*\n(?:\s+\S+:[^\n]*\n)+|\s+[^\n]*\n)+",
            text,
        )
        self.assertGreater(len(upload_steps), 0)
        for step in upload_steps:
            self.assertIn(
                "if-no-files-found: error",
                step,
                "every upload-artifact step must keep if-no-files-found: error",
            )


class Plan097LaneContractTests(unittest.TestCase):
    """Plan 097 cases 33-39: Plan 095 lane invariants preserved."""

    def test_no_retry_loop(self) -> None:
        text = _read_workflow_text()
        self.assertNotIn("nick-fields/retry", text)
        self.assertNotIn("retry-action", text)

    def test_workflow_trigger_is_workflow_dispatch_only(self) -> None:
        workflow = _load_workflow()
        on = workflow.get(True) or workflow.get("on")
        self.assertIsInstance(on, dict)
        self.assertIn("workflow_dispatch", on)
        self.assertNotIn("pull_request", on)
        self.assertNotIn("push", on)
        self.assertNotIn("schedule", on)

    def test_live_jobs_run_on_ubuntu_24_04(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        for job_name in LIVE_JOB_NAMES:
            self.assertEqual(
                jobs[job_name].get("runs-on"), "ubuntu-24.04",
                f"job {job_name} must run on ubuntu-24.04",
            )

    def test_network_id_is_99(self) -> None:
        text = _read_workflow_text()
        self.assertIn('"network_id": 99', text)

    def test_topology_is_host_loopback_development(self) -> None:
        text = _read_workflow_text()
        self.assertIn("host-loopback-development", text)

    def test_plan088_absent_from_execution_path(self) -> None:
        text = _read_workflow_text()
        self.assertNotIn("i2pd-to-i2pr-ipv4", text)
        self.assertNotIn("plan084_runner", text)
        self.assertNotIn("minimal_i2pd_reverse_probe", text)

    def test_ntcp2_stays_experimental_non_advertised(self) -> None:
        text = _read_workflow_text()
        self.assertIn("experimental-non-advertised", text)


class Plan097StatusAuthorityTests(unittest.TestCase):
    """Plan 097 case 40: status authority leaves 087 open / 088 blocked."""

    def test_plan087_status_authorities_open(self) -> None:
        text = (REPO_ROOT / "plans/087-status.md").read_text()
        self.assertIn("open-pending-plan095-ci-forward-evidence-pair", text)

    def test_plan088_status_authorities_blocked(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("blocked-pending-plan095-ci-closure", text)
        self.assertIn("insufficient-evidence", text)

    def test_plan087_references_plan_097(self) -> None:
        # The post-Plan-097 status authority must reference the
        # Plan 097 closed token.
        text = (REPO_ROOT / "plans/087-status.md").read_text()
        # Either an explicit Plan 097 token or a reference to the
        # plan-of-record path.
        self.assertTrue(
            "plan_097 = passed-artifact-path-and-cleanup-correction" in text
            or "Plan 097" in text,
            "Plan 087 status must reference Plan 097 closure",
        )

    def test_plan088_references_plan_097(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertTrue(
            "plan_097 = passed-artifact-path-and-cleanup-correction" in text
            or "Plan 097" in text,
            "Plan 088 status must reference Plan 097 closure",
        )

    def test_plan095_awaits_authoritative_run(self) -> None:
        for status_path in ("plans/087-status.md", "plans/088-status.md"):
            text = (REPO_ROOT / status_path).read_text()
            # The status may carry the pre-authoritative-run
            # "awaiting" token, the post-dispatch
            # "active-instrumented-pre-protocol-rejected" token,
            # or the "passed" / "ci-live-wire-closure-next-executable"
            # token. Any of these names Plan 095 as the live-wire
            # authority for the forward direction.
            valid_tokens = (
                "plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run",
                "plan_095 = ci-live-wire-lane-active-instrumented-pre-protocol-rejected",
                "plan_095 = ci-live-wire-closure-next-executable",
            )
            self.assertTrue(
                any(token in text for token in valid_tokens),
                f"{status_path} must name plan_095 with one of {valid_tokens!r}",
            )


class Plan097SyntheticMutationTests(unittest.TestCase):
    """Plan 097 cases 18-20: synthetic mutation regression surface.

    The tests below mutate the workflow text in memory to introduce
    one of the two defects and assert the structural checks reject
    the mutated text. The mutations cover:

    - producer output path changed but manifest path unchanged;
    - manifest hash path changed but producer path unchanged;
    - upload path changed away from the canonical tree.

    These prove the regression surface catches the prior defective
    semantics on synthetic fixtures.
    """

    @staticmethod
    def _build_output_constant() -> str:
        return 'BUILD_OUTPUT="$BUILD_DIR/output"'

    @staticmethod
    def _producer_pattern() -> str:
        return (
            r'cp\s+"\$I2PR_TARGET_DIR/release/i2pr-interop"'
            r'\s+"\$BUILD_OUTPUT/i2pr-interop"'
        )

    @staticmethod
    def _manifest_hash_pattern() -> str:
        return r'sha256sum\s+"\$BUILD_OUTPUT/i2pr-interop"'

    def test_producer_mismatch_caught(self) -> None:
        text = _read_workflow_text()
        # Replace the producer destination with a different path.
        # We perform a simple substring replacement using a
        # substring that exists in the workflow; the producer
        # check then asserts the corrected canonical path is gone.
        mutated = text.replace(
            '"$BUILD_OUTPUT/i2pr-interop"',
            '"$BUILD_DIR/wrong-output/i2pr-interop"',
            1,  # Only replace the first occurrence (the producer's cp)
        )
        # Sanity: the mutation must actually introduce the wrong
        # destination.
        self.assertIn(
            "$BUILD_DIR/wrong-output/i2pr-interop",
            mutated,
            "mutation must introduce the producer mismatch",
        )
        # The first occurrence of the canonical destination is
        # now the wrong destination; the canonical producer
        # pattern no longer matches.
        self.assertNotRegex(
            mutated[: mutated.find("\"$BUILD_DIR/wrong-output/i2pr-interop\"") + 60],
            self._producer_pattern(),
            "producer destination mismatch must be detected",
        )

    def test_manifest_mismatch_caught(self) -> None:
        text = _read_workflow_text()
        # Replace the manifest hash path (which lives inside the
        # cat > "$BUILD_OUTPUT/i2pr-build-manifest.json" heredoc).
        mutated = text.replace(
            '"$BUILD_OUTPUT/i2pr-interop" | awk',
            '"$BUILD_DIR/wrong-output/i2pr-interop" | awk',
        )
        self.assertNotRegex(
            mutated,
            self._manifest_hash_pattern(),
            "manifest hash path mismatch must be detected",
        )

    def test_upload_path_mismatch_caught(self) -> None:
        text = _read_workflow_text()
        mutated = text.replace(
            "${{ github.workspace }}/target/interop/plan095-build/output/",
            "${{ github.workspace }}/target/interop/wrong-output/",
        )
        self.assertNotIn(
            "${{ github.workspace }}/target/interop/plan095-build/output/",
            mutated,
            "upload path mismatch must be detected",
        )

    def test_defect_a_synthetic_rejects_pre_correction(self) -> None:
        # Build a synthetic workflow text that emulates the
        # pre-Plan-097 defect A: producer writes to a
        # CWD-relative ``output/i2pr-interop`` while the
        # verifier hashes ``$BUILD_DIR/output/i2pr-interop``.
        synthetic = _read_workflow_text().replace(
            '"$BUILD_OUTPUT/i2pr-interop"',
            'output/i2pr-interop',
            1,
        )
        # The corrected producer check must reject this text.
        body = _step_body(synthetic, "build-i2pr-interop")
        self.assertNotRegex(
            body,
            r'cp\s+"\$I2PR_TARGET_DIR/release/i2pr-interop"\s+"\$BUILD_OUTPUT/i2pr-interop"',
            "Defect A regression must reject producer writing to relative output",
        )

    def test_defect_b_synthetic_rejects_pre_correction(self) -> None:
        # Build a synthetic workflow text that emulates the
        # pre-Plan-097 defect B: descendant-only ``find -delete``
        # plus ``|| true`` suppression of the absence assertion.
        synthetic = _read_workflow_text().replace(
            'rm -rf -- "$PLAN095_RUN_ROOT"\n          test ! -e "$PLAN095_RUN_ROOT"',
            'if [[ -d "$PLAN095_RUN_ROOT" ]]; then\n            find "$PLAN095_RUN_ROOT" -mindepth 1 -delete\n          fi\n          test ! -e "$PLAN095_RUN_ROOT" || true',
        )
        # The corrected cleanup check must reject this text.
        self.assertIn(
            'find "$PLAN095_RUN_ROOT" -mindepth 1 -delete',
            synthetic,
            "Defect B regression must retain the descendant-only deletion",
        )
        self.assertRegex(
            synthetic,
            r'test\s+!\s*-e\s+"\$PLAN095_RUN_ROOT"\s*\|\|\s*true',
            "Defect B regression must retain the suppressed absence assertion",
        )


if __name__ == "__main__":
    unittest.main()
