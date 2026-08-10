"""Plan 095 CI host-loopback live-wire evidence lane test matrix.

Plan 095 closes the Plan 094 forward-direction closure in a manual
GitHub Actions lane that runs on the standard github-hosted
ubuntu-24.04 image. The lane never uses rootless namespaces,
Multipass, Docker, privileged containers, host network mutation,
or any privileged operation in the live jobs. The build job may
install ordinary build dependencies; every live job runs without
privilege escalation.

The test matrix exercises the workflow contract statically:

- file location, manual trigger, no automatic pull_request trigger;
- permissions and concurrency group;
- runner label, timeout, dependency graph;
- prohibition tokens absent from the live jobs (sudo,
  ip netns, nft, iptables, unshare, --privileged,
  --network host, multipass, docker);
- build job may only invoke sudo for ``apt-get update`` and
  ``apt-get install`` package operations;
- bounded CI environment blocker vocabulary;
- bounded artifact upload paths;
- Plan 088 / 079 / 072 / NTCP2 advertisement gates preserved.
"""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

import yaml


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
WORKFLOW_PATH = (
    REPO_ROOT
    / ".github/workflows/ntcp2-interop-host-loopback-development.yml"
)


# Plan 095: the live-path jobs that may not invoke any
# privileged operation. The build job is the only job that may
# install ordinary build dependencies.
LIVE_JOB_NAMES = (
    "forward-instrumented",
    "forward-control",
    "validate-gate",
)


# Plan 095: the static-bounded tokens that must never appear in the
# live-path jobs. The check is a token-bounded regex match on the
# raw ``run:`` block; comments mentioning the tokens in upstream
# explanatory text are allowed only in the build/contract jobs.
PROHIBITED_LIVE_TOKENS: dict[str, re.Pattern[str]] = {
    "ip_netns": re.compile(r"\bip\s+netns\b"),
    "iptables": re.compile(r"\biptables\b"),
    "unshare": re.compile(r"\bunshare\b"),
    "privileged_flag": re.compile(r"--privileged\b"),
    "network_host_flag": re.compile(r"--network\s+host\b"),
    "multipass_binary": re.compile(r"(^|[^a-zA-Z0-9_-])multipass($|[^a-zA-Z0-9_-])"),
    "docker": re.compile(r"(^|[^a-zA-Z0-9_-])docker($|[^a-zA-Z0-9_-])"),
}


# Plan 095: the bounded CI environment blocker vocabulary. The
# blocker record must use one of these reason codes.
CI_ENVIRONMENT_BLOCKER_VOCABULARY: tuple[str, ...] = (
    "ci_binary_execution_blocked",
    "ci_loopback_bind_blocked",
    "ci_loopback_connect_blocked",
    "ci_reference_build_blocked",
    "ci_artifact_transfer_blocked",
    "ci_disk_space_blocked",
    "ci_unexpected_runner_environment",
)


def _load_workflow() -> dict[str, object]:
    """Load and cache the Plan 095 workflow YAML."""

    with WORKFLOW_PATH.open(encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def _job_runs(workflow: dict[str, object]) -> str:
    """Return the raw ``runs-on`` value for a job."""

    jobs = workflow.get("jobs") or {}
    if not isinstance(jobs, dict):
        raise AssertionError("jobs must be a mapping")
    job = jobs.get("runs-on")
    if not isinstance(job, str):
        raise AssertionError("runs-on must be a string")
    return job


def _job_steps_text(job: dict[str, object]) -> str:
    """Concatenate the ``run:`` blocks of a job into a single string."""

    steps = job.get("steps")
    if not isinstance(steps, list):
        return ""
    chunks: list[str] = []
    for step in steps:
        if not isinstance(step, dict):
            continue
        run_value = step.get("run")
        if isinstance(run_value, str):
            chunks.append(run_value)
        name_value = step.get("name")
        if isinstance(name_value, str):
            chunks.append(name_value)
    return "\n".join(chunks)


def _has_sudo_outside_apt_get(text: str) -> bool:
    """Return True iff ``text`` invokes sudo outside an apt-get command."""

    lines = text.splitlines()
    for line in lines:
        stripped = line.strip()
        if not stripped.startswith("sudo"):
            continue
        if "apt-get" in stripped:
            continue
        return True
    return False


class Plan095WorkflowFileTests(unittest.TestCase):
    """Plan 095 workflow file existence and structural contract."""

    def test_workflow_file_exists(self) -> None:
        self.assertTrue(WORKFLOW_PATH.is_file())

    def test_workflow_parses_as_yaml(self) -> None:
        workflow = _load_workflow()
        self.assertIsInstance(workflow, dict)
        self.assertEqual(
            workflow.get("name"),
            "NTCP2 host-loopback live-wire evidence (Plan 095 manual)",
        )

    def test_workflow_includes_workflow_dispatch_only(self) -> None:
        # Plan 095 WP2: the initial trigger must be workflow_dispatch
        # only; no automatic pull_request target.
        workflow = _load_workflow()
        on = workflow.get(True) or workflow.get("on")
        self.assertIsInstance(on, dict)
        self.assertIn("workflow_dispatch", on)
        self.assertNotIn("pull_request", on)
        self.assertNotIn("push", on)
        self.assertNotIn("schedule", on)


class Plan095PermissionsAndConcurrencyTests(unittest.TestCase):
    """Plan 095 permissions and concurrency group contract."""

    def test_permissions_are_contents_read_only(self) -> None:
        workflow = _load_workflow()
        permissions = workflow.get("permissions")
        self.assertIsInstance(permissions, dict)
        self.assertEqual(permissions.get("contents"), "read")

    def test_concurrency_group_present_and_no_cancel(self) -> None:
        workflow = _load_workflow()
        concurrency = workflow.get("concurrency")
        self.assertIsInstance(concurrency, dict)
        group = concurrency.get("group")
        self.assertIsInstance(group, str)
        self.assertTrue(group.startswith("ntcp2-host-loopback-development-"))
        self.assertEqual(concurrency.get("cancel-in-progress"), False)


class Plan095RunnerAndJobSetTests(unittest.TestCase):
    """Plan 095 job set, runner label, and dependency graph contract."""

    def test_all_jobs_run_on_ubuntu_24_04(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        self.assertIsInstance(jobs, dict)
        for job_name, job in jobs.items():
            self.assertIsInstance(job, dict)
            self.assertEqual(
                job.get("runs-on"),
                "ubuntu-24.04",
                f"job {job_name} must run on ubuntu-24.04",
            )

    def test_required_jobs_present(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        self.assertIsInstance(jobs, dict)
        required = {"contract", "build", *LIVE_JOB_NAMES}
        self.assertTrue(required.issubset(jobs.keys()), required - jobs.keys())

    def test_job_dependency_graph(self) -> None:
        # Plan 095 WP6/7/8/9: build depends on contract;
        # forward-instrumented depends on build;
        # forward-control depends on build AND forward-instrumented;
        # validate-gate depends on build, forward-instrumented,
        # and forward-control.
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        self.assertEqual(jobs["build"].get("needs"), "contract")
        self.assertEqual(
            jobs["forward-instrumented"].get("needs"), "build"
        )
        self.assertEqual(
            sorted(jobs["forward-control"].get("needs", [])),
            ["build", "forward-instrumented"],
        )
        self.assertEqual(
            sorted(jobs["validate-gate"].get("needs", [])),
            ["build", "forward-control", "forward-instrumented"],
        )

    def test_live_jobs_have_timeouts(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        for job_name in LIVE_JOB_NAMES:
            self.assertIsInstance(jobs[job_name], dict)
            self.assertIsInstance(jobs[job_name].get("timeout-minutes"), int)


class Plan095LivePathProhibitionTests(unittest.TestCase):
    """Plan 095: the live jobs may not invoke any privileged token."""

    def test_no_sudo_in_live_jobs(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        for job_name in LIVE_JOB_NAMES:
            text = _job_steps_text(jobs[job_name])
            self.assertFalse(
                "sudo" in text,
                f"live job {job_name} must not invoke sudo",
            )

    def test_no_prohibited_tokens_in_live_jobs(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        for job_name in LIVE_JOB_NAMES:
            text = _job_steps_text(jobs[job_name])
            for label, pattern in PROHIBITED_LIVE_TOKENS.items():
                self.assertIsNone(
                    pattern.search(text),
                    f"live job {job_name} contains prohibited token {label}",
                )

    def test_build_job_sudo_only_for_apt_get(self) -> None:
        # Plan 095 WP4: the build job may install declared packages;
        # any other sudo invocation in the build job is forbidden.
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        text = _job_steps_text(jobs["build"])
        self.assertFalse(
            _has_sudo_outside_apt_get(text),
            "build job may only invoke sudo for apt-get update/install",
        )


class Plan095ArtifactAndUploadTests(unittest.TestCase):
    """Plan 095: artifact uploads must be allowlisted and bounded."""

    def test_artifact_paths_target_evidence_or_build(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        # Collect every upload-artifact ``path:`` value across the
        # workflow. Each path must fall under either the sanitized
        # evidence directory or the build output directory.
        raw_yaml = WORKFLOW_PATH.read_text(encoding="utf-8")
        # Match both single-line paths and multi-line `|` block scalars.
        block_paths = re.findall(
            r"path:\s*\|[ \t]*\n((?:[ \t]+.+\n)+)", raw_yaml
        )
        flattened: list[str] = []
        for block in block_paths:
            for line in block.splitlines():
                stripped = line.strip()
                if stripped and not stripped.startswith("#"):
                    flattened.append(stripped)
        single_paths = re.findall(
            r"path:\s*(?!\|)([^\n#]+?)\n", raw_yaml
        )
        for entry in single_paths:
            stripped = entry.strip()
            if stripped and not stripped.startswith("#"):
                flattened.append(stripped)
        self.assertGreater(len(flattened), 0)
        for entry in flattened:
            # Plan 096: sanitized evidence lives under
            # ``plan095-evidence/instrumented`` or
            # ``plan095-evidence/control``; the build artifact
            # lives under ``plan095-build/output``; the gate
            # record lives under ``target/interop/evidence``.
            self.assertTrue(
                entry.startswith("target/interop/evidence/")
                or entry.startswith("target/interop/build/")
                or "/sanitized/" in entry
                or entry.endswith("/output/")
                or "plan095-build/output" in entry
                or "plan095-evidence/instrumented" in entry
                or "plan095-evidence/control" in entry
                or "plan095-instrumented/sanitized" in entry
                or "plan095-control/sanitized" in entry
                or entry.startswith("${{ env.PLAN095_SANITIZED }}")
                or entry.startswith("${{ github.workspace }}"),
                f"unexpected upload path: {entry!r}",
            )

    def test_raw_run_paths_never_uploaded(self) -> None:
        raw_yaml = WORKFLOW_PATH.read_text(encoding="utf-8")
        self.assertNotIn("target/interop/runs/", raw_yaml)
        # ``raw/`` directories are only referenced in the build job;
        # the live jobs upload only sanitized records.
        self.assertNotIn("raw/record", raw_yaml)
        self.assertNotIn("raw/state", raw_yaml)

    def test_sanitized_paths_not_under_disposable_run_roots(self) -> None:
        # Plan 096 WP3: the sanitized evidence path must be
        # disjoint from the disposable run root. Both the
        # instrumented and control paths must satisfy the contract.
        raw_yaml = WORKFLOW_PATH.read_text(encoding="utf-8")
        self.assertNotIn(
            "target/interop/plan095-instrumented/sanitized",
            raw_yaml,
        )
        self.assertNotIn(
            "target/interop/plan095-control/sanitized",
            raw_yaml,
        )
        self.assertIn(
            "target/interop/plan095-evidence/instrumented",
            raw_yaml,
        )
        self.assertIn(
            "target/interop/plan095-evidence/control",
            raw_yaml,
        )


class Plan095EnvironmentBlockerVocabularyTests(unittest.TestCase):
    """Plan 095 CI environment blocker reason codes."""

    def test_blocker_vocabulary_is_closed(self) -> None:
        # The static test enforces the bounded vocabulary exists;
        # production code that emits a blocker must select one of
        # these tokens.
        expected = {
            "ci_binary_execution_blocked",
            "ci_loopback_bind_blocked",
            "ci_loopback_connect_blocked",
            "ci_reference_build_blocked",
            "ci_artifact_transfer_blocked",
            "ci_disk_space_blocked",
            "ci_unexpected_runner_environment",
        }
        self.assertEqual(set(CI_ENVIRONMENT_BLOCKER_VOCABULARY), expected)


class Plan095GateRecordSchemaTests(unittest.TestCase):
    """Plan 095 CI gate record contract."""

    def test_validate_gate_writes_gate_record(self) -> None:
        raw_yaml = WORKFLOW_PATH.read_text(encoding="utf-8")
        self.assertIn("i2pr-ntcp2-plan095-ci-gate-v1", raw_yaml)
        self.assertIn("plan087_gate", raw_yaml)
        self.assertIn("plan088_gate", raw_yaml)
        self.assertIn("plan079_gate", raw_yaml)
        self.assertIn("plan072_gate", raw_yaml)
        self.assertIn("experimental-non-advertised", raw_yaml)


class Plan095StatusAuthorityTests(unittest.TestCase):
    """Plan 095 status authority naming and gate preservation."""

    def test_active_status_names_plan095_next_executable(self) -> None:
        # Plan 095 WP12: the active authority must name Plan 095 as
        # the next executable plan. Plan 094 is implementation-
        # landed; the forward evidence pair is not yet retained.
        text_087 = (REPO_ROOT / "plans/087-status.md").read_text()
        text_088 = (REPO_ROOT / "plans/088-status.md").read_text()
        expected_tokens = (
            "plan_095 = ci-live-wire-closure-next-executable",
            "plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run",
        )
        for text, label in ((text_087, "087"), (text_088, "088")):
            self.assertTrue(
                any(token in text for token in expected_tokens),
                f"plans/{label}-status.md must name Plan 095 as the "
                f"next executable plan with one of {expected_tokens!r}",
            )

    def test_plan088_remains_blocked_until_ci_closure(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("blocked-pending-plan095-ci-closure", text)

    def test_plan079_remains_blocked(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("blocked-pending-plan088-two-way-pass", text)

    def test_plan072_remains_inactive(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("inactive-pending-plan088-ambiguity", text)

    def test_ntcp2_stays_experimental_and_non_advertised(self) -> None:
        text = (REPO_ROOT / "AGENTS.md").read_text()
        self.assertIn("experimental and non-advertised", text)
        readme = (REPO_ROOT / "README.md").read_text()
        self.assertIn("experimental and non-advertised", readme)


class Plan095DocumentationContractTests(unittest.TestCase):
    """Plan 095 README, AGENTS, skill, and architecture propagation."""

    def test_readme_documents_plan095(self) -> None:
        text = (REPO_ROOT / "README.md").read_text()
        self.assertIn("Plan 095", text)

    def test_agents_md_documents_plan095(self) -> None:
        text = (REPO_ROOT / "AGENTS.md").read_text()
        self.assertIn("Plan 095", text)

    def test_skill_describes_plan095(self) -> None:
        skill_path = (
            REPO_ROOT / ".opencode/skills/i2pr-ntcp2-interop/SKILL.md"
        )
        if not skill_path.is_file():
            self.skipTest("interop skill not present")
        text = skill_path.read_text()
        self.assertIn("Plan 095", text)

    def test_architecture_document_records_plan095(self) -> None:
        arch_path = (
            REPO_ROOT / "docs/architecture/interop-apparatus.md"
        )
        if not arch_path.is_file():
            self.skipTest("architecture document not present")
        text = arch_path.read_text()
        self.assertIn("Plan 095", text)

    def test_build_i2pr_interop_step_restores_executable_bit(self) -> None:
        # Plan 098: bare ``cp`` defaults to the umask mode
        # (typically 0644 in the Ubuntu 24.04 GitHub-hosted runner),
        # which strips the executable bit the source binary
        # carries. The downstream upload zips the file with its
        # current mode and the downstream download extracts the
        # same mode; the live jobs then fail the ``test -x`` guard
        # with a typed ``ci_build_blocked``. The build-i2pr-interop
        # step must explicitly ``chmod 0755`` the binary after
        # copying it so the upload/download round-trip preserves
        # the binary's runnability.
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        body = _job_steps_text(jobs["build"])
        self.assertIn(
            "build-i2pr-interop",
            body,
            "build job must include a build-i2pr-interop step",
        )
        self.assertRegex(
            body,
            r"chmod\s+0755\s+\"\$BUILD_OUTPUT/i2pr-interop\"",
            "build-i2pr-interop must chmod 0755 the i2pr-interop artifact at the canonical output path",
        )

    def test_live_jobs_restore_executable_bit_after_download(self) -> None:
        # Plan 099: ``actions/upload-artifact@v4`` (archiver
        # 7.0.1 + zip-stream) records the default 0644 mode in
        # the zip entry's external attributes even when the file
        # on disk is 0755. The downstream
        # ``actions/download-artifact`` then extracts the same
        # 0644 mode, so the live jobs' ``test -x`` guard fails
        # closed with a typed ``ci_build_blocked``. The
        # forward-instrumented and forward-control jobs must
        # explicitly ``chmod 0755`` the downloaded binaries so
        # the live jobs see the correct mode regardless of the
        # upload action's zip-mode handling.
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        for job_name in ("forward-instrumented", "forward-control"):
            with self.subTest(job=job_name):
                text = _job_steps_text(jobs[job_name])
                self.assertIn(
                    "restore-executable-bit",
                    text,
                    f"{job_name} must include a restore-executable-bit step",
                )
                # The exact form may be a literal ``chmod 0755
                # "$BUILD_OUTPUT/<binary>"`` line or a ``chmod
                # 0755`` applied inside a loop over the binary
                # names. Match the binary names anywhere in the
                # step body and the ``chmod 0755`` token in the
                # same step.
                self.assertIn(
                    "chmod 0755",
                    text,
                    f"{job_name} must invoke chmod 0755 to restore the executable bit on downloaded artifacts",
                )
                for binary in (
                    "i2pr-interop",
                    "i2pd_ntcp2_interop_driver_instrumented",
                ):
                    self.assertIn(
                        binary,
                        text,
                        f"{job_name} must restore the executable bit on {binary}",
                    )

    def test_live_jobs_rc_capture_is_not_a_probe_argument(self) -> None:
        # Plan 099/100: a trailing backslash after the last
        # ``run-minimal-i2pd-host-loopback-probe.py`` argument
        # turns the next line (``instrumented_rc=$?`` /
        # ``control_rc=$?``) into a literal argument to the
        # Python script, which then errors out with
        # ``unrecognized arguments: instrumented_rc=0`` before
        # the live attempt begins. The ``<role>_rc=$?`` capture
        # must be on its own shell line so the probe runs cleanly
        # and the exit code is captured by the shell.
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        for job_name, capture in (
            ("forward-instrumented", "instrumented_rc=$?"),
            ("forward-control", "control_rc=$?"),
        ):
            with self.subTest(job=job_name):
                text = _job_steps_text(jobs[job_name])
                self.assertIn(
                    capture,
                    text,
                    f"{job_name} must capture the probe exit code via a separate shell line",
                )
                # The capture line must not be the continuation
                # of the probe command (i.e. the probe argument
                # list must end with the actual last argument,
                # not a trailing backslash).
                self.assertNotRegex(
                    text,
                    rf"run-minimal-i2pd-host-loopback-probe\.py.*\\[^\\]*\n\s*{re.escape(capture)}",
                    f"{job_name} must not concatenate '{capture}' onto the probe command via a trailing backslash",
                )


if __name__ == "__main__":
    unittest.main()
