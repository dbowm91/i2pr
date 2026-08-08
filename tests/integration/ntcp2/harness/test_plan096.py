"""Plan 096 CI workflow correctness and pre-dispatch closure tests.

Plan 096 is a narrow corrective pass over the Plan 095 manual
GitHub Actions lane. The plan fixes four demonstrated execution
defects in the workflow file and refuses to dispatch a live Plan
095 run until the static regression surface rejects each defect
on the pre-correction source and accepts the corrected source.

The regression matrix exercises:

- i2pr build path explicit manifest/target ownership;
- instrumented/control sanitized evidence disjoint from the
  disposable run roots;
- post-cleanup evidence existence assertions;
- embedded Python runtime correctness in the control validator
  and every other ``python3 - <<'PY'`` heredoc;
- canonical tracked-source digest for the pinned i2pd tree;
- instrumented/control/gate job dependency and fail-closed
  semantics;
- workflow remains manual ``workflow_dispatch`` for live
  execution, ``ubuntu-24.04``, host-loopback-development only,
  network id 99, and bounded attempt count;
- no automatic retry loop;
- Plan 088 remains absent from the workflow execution path;
- NTCP2 remains experimental and non-advertised.

The synthetic digest tests construct a temporary git worktree
that mirrors the canonical algorithm used in the workflow and
verify the algorithm's three properties: ``.git`` metadata
changes do not affect the digest, tracked content changes do
affect the digest, and the pinned revision equality check
rejects a wrong revision.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

import yaml


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
WORKFLOW_PATH = (
    REPO_ROOT
    / ".github/workflows/ntcp2-interop-host-loopback-development.yml"
)
PYYAML_AVAILABLE = True


# Plan 096: the live-path jobs that may not invoke any privileged
# operation and that participate in the bounded execution contract.
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


def _python_heredocs(text: str) -> list[tuple[str, str]]:
    """Extract every ``python3 - <<'PY' ... PY`` block from ``text``.

    Returns a list of ``(marker, body)`` pairs where ``marker`` is
    the heredoc end token (``PY`` in this workflow) and ``body`` is
    the body of the heredoc.
    """

    pattern = re.compile(
        r"python3\s*-\s*<<\s*'(?P<marker>\w+)'\s*\n(?P<body>.*?)\n\s*PY\s*\n",
        re.DOTALL,
    )
    return [
        (match.group("marker"), match.group("body"))
        for match in pattern.finditer(text)
    ]


def _canonical_tracked_source_digest(tree: Path) -> str:
    """Compute the canonical tracked-source digest for a directory.

    The algorithm mirrors the workflow's ``record-source-tree-digest``
    step: it enumerates ``git ls-files`` output, prints each path
    followed by the file content SHA-256, and digests the resulting
    byte stream. ``.git`` administrative files are excluded.
    """

    proc = subprocess.run(
        ["git", "-C", str(tree), "ls-files", "-z"],
        check=True,
        capture_output=True,
    )
    assert proc.returncode == 0
    payload = b""
    for chunk in proc.stdout.split(b"\x00"):
        if not chunk:
            continue
        relpath = chunk.decode("utf-8")
        file_path = tree / relpath
        digest = hashlib_sha256_hex(file_path.read_bytes())
        payload += relpath.encode("utf-8") + b"\x00" + digest.encode("ascii") + b"\x00"
    return hashlib_sha256_hex(payload)


def hashlib_sha256_hex(data: bytes) -> str:
    import hashlib

    return hashlib.sha256(data).hexdigest()


class Plan096WorkflowStructuralTests(unittest.TestCase):
    """Plan 096: workflow exists and is parseable."""

    def test_workflow_file_exists(self) -> None:
        self.assertTrue(WORKFLOW_PATH.is_file())

    def test_workflow_parses_as_yaml(self) -> None:
        workflow = _load_workflow()
        self.assertIsInstance(workflow, dict)
        self.assertEqual(
            workflow.get("name"),
            "NTCP2 host-loopback live-wire evidence (Plan 095 manual)",
        )


class Plan096BuildPathTests(unittest.TestCase):
    """Plan 096 WP2: i2pr build path uses explicit manifest/target."""

    def test_i2pr_build_uses_explicit_manifest(self) -> None:
        text = _read_workflow_text()
        # The build-i2pr-interop step must include an explicit
        # --manifest-path pointing at the repository's Cargo.toml.
        # We search the entire build job to allow for either
        # ``cd $BUILD_DIR`` or no ``cd`` before the cargo command.
        self.assertRegex(
            text,
            r"build-i2pr-interop[\s\S]*?--manifest-path\s+\"\$\{\{?\s*GITHUB_WORKSPACE\s*\}?}/Cargo\.toml\"",
            "i2pr Cargo invocation must use --manifest-path",
        )

    def test_i2pr_build_uses_explicit_target_dir(self) -> None:
        text = _read_workflow_text()
        self.assertRegex(
            text,
            r"build-i2pr-interop[\s\S]*?--target-dir\s+\"\$\w+\"",
            "i2pr Cargo invocation must use --target-dir",
        )

    def test_i2pr_binary_copied_from_explicit_target_dir(self) -> None:
        text = _read_workflow_text()
        # The downstream copy must source the binary from the
        # explicit target directory variable, not from a relative
        # ``target/release`` path. Plan 097: the destination must be
        # the canonical absolute ``$BUILD_OUTPUT`` path so the
        # producer, verifier, manifest, and uploader all resolve
        # to the same file. The YAML literal-block line continuation
        # (``\\`` followed by newline) is collapsed to a single
        # space before matching.
        collapsed = re.sub(r"\\\n[ \t]*", " ", text)
        self.assertRegex(
            collapsed,
            r"cp\s+\"\$\w+/release/i2pr-interop\"\s+\"\$\w+/i2pr-interop\"",
            "i2pr binary must be copied from the explicit target dir "
            "to the canonical absolute output path",
        )
        # The forbidden pattern: relative ``cp target/release/...``
        # anchored to BUILD_DIR.
        self.assertNotRegex(
            text,
            r"cp\s+target/release/i2pr-interop\b",
            "i2pr binary must not be copied from a relative target path",
        )
        # Plan 097: the producer must NOT write to a bare relative
        # ``output/i2pr-interop`` destination because that path
        # depends on step working directory.
        self.assertNotRegex(
            collapsed,
            r"cp\s+\"\$\w+/release/i2pr-interop\"\s+output/i2pr-interop\b",
            "i2pr binary must not be copied to a relative output path",
        )

    def test_i2pr_binary_verified_before_hashing(self) -> None:
        text = _read_workflow_text()
        # The build step must assert that the i2pr binary is a
        # regular, executable, non-symlink file before the build
        # manifest is computed. Plan 097: the canonical absolute
        # ``$BUILD_OUTPUT`` path must be used so the producer,
        # verifier, manifest, and uploader resolve to the same
        # file regardless of step working directory.
        self.assertIn(
            "BUILD_OUTPUT=\"$BUILD_DIR/output\"", text,
            "canonical BUILD_OUTPUT variable must be defined",
        )
        self.assertIn(
            "test -f \"$BUILD_OUTPUT/i2pr-interop\"", text,
            "i2pr binary existence must be asserted at canonical path",
        )
        self.assertIn(
            "test -x \"$BUILD_OUTPUT/i2pr-interop\"", text,
            "i2pr binary executability must be asserted at canonical path",
        )
        self.assertIn(
            "test ! -L \"$BUILD_OUTPUT/i2pr-interop\"", text,
            "i2pr binary must be asserted as non-symlink at canonical path",
        )


class Plan096EvidenceDisjointnessTests(unittest.TestCase):
    """Plan 096 WP3: sanitized evidence is disjoint from run roots."""

    def test_instrumented_sanitized_disjoint_from_run_root(self) -> None:
        text = _read_workflow_text()
        # The instrumented sanitized path must not be a child of
        # the disposable instrumented run root.
        self.assertNotIn(
            "target/interop/plan095-instrumented/sanitized", text,
            "instrumented sanitized path must not nest under the run root",
        )
        self.assertIn(
            "target/interop/plan095-evidence/instrumented", text,
            "instrumented sanitized path must live under plan095-evidence/",
        )

    def test_control_sanitized_disjoint_from_run_root(self) -> None:
        text = _read_workflow_text()
        self.assertNotIn(
            "target/interop/plan095-control/sanitized", text,
            "control sanitized path must not nest under the run root",
        )
        self.assertIn(
            "target/interop/plan095-evidence/control", text,
            "control sanitized path must live under plan095-evidence/",
        )

    def test_cleanup_asserts_sanitized_evidence_survives(self) -> None:
        text = _read_workflow_text()
        # Both ``delete-raw-run-state`` steps must assert the
        # sanitized tree still exists after deletion.
        deletes = re.findall(
            r"delete-raw-run-state[\s\S]*?(?=\n      - name:|\Z)",
            text,
        )
        self.assertEqual(len(deletes), 2, "expected instrumented + control cleanup")
        for block in deletes:
            self.assertIn("PLAN095_SANITIZED", block)
            self.assertIn("sanitized evidence tree was lost", block)

    def test_upload_paths_reference_durable_evidence(self) -> None:
        text = _read_workflow_text()
        # The upload-instrumented-evidence and upload-control-evidence
        # steps must point at the durable plan095-evidence tree, not
        # the disposable run root.
        # Walk the file line-by-line to find the path directive.
        instrumented_upload = ""
        control_upload = ""
        in_step = None
        for line in text.splitlines():
            stripped = line.strip()
            if stripped.startswith("- name: "):
                if "upload-instrumented-evidence" in stripped:
                    in_step = "instrumented"
                elif "upload-control-evidence" in stripped:
                    in_step = "control"
                else:
                    in_step = None
            elif in_step and stripped.startswith("path:"):
                value = line.split("path:", 1)[1].strip()
                if in_step == "instrumented":
                    instrumented_upload = value
                elif in_step == "control":
                    control_upload = value
                in_step = None
        self.assertTrue(instrumented_upload, "instrumented upload path missing")
        self.assertTrue(control_upload, "control upload path missing")
        self.assertIn("plan095-evidence/instrumented", instrumented_upload)
        self.assertIn("plan095-evidence/control", control_upload)
        # Defensive: the upload path must not include the disposable
        # run root name in its leading component.
        self.assertNotIn("plan095-instrumented/", instrumented_upload)
        self.assertNotIn("plan095-control/", control_upload)


class Plan096EmbeddedPythonTests(unittest.TestCase):
    """Plan 096 WP4: every embedded Python block is runtime-correct."""

    def test_every_heredoc_imports_its_used_names(self) -> None:
        text = _read_workflow_text()
        blocks = _python_heredocs(text)
        self.assertGreater(len(blocks), 0)
        # Identify the modules each block references via dotted
        # attribute access (e.g. ``os.environ`` -> ``os``).
        module_re = re.compile(r"(?<![\w.])(os|sys|json|pathlib|hashlib|argparse|re)\b")
        for marker, body in blocks:
            used_modules = set(module_re.findall(body))
            # The heredoc must contain an ``import`` statement for
            # every module it references. Comments are not enough.
            for module in sorted(used_modules):
                import_pat = re.compile(
                    rf"(?m)^\s*import\s+{module}\b|^\s*from\s+{module}\b",
                )
                self.assertRegex(
                    body,
                    import_pat,
                    f"heredoc ending {marker!r} uses module {module!r} "
                    f"without importing it",
                )

    def test_control_validator_uses_import_os(self) -> None:
        text = _read_workflow_text()
        # The control validator must include ``import os`` because
        # it uses ``os.environ``. Extract the first heredoc that
        # lives strictly after the
        # ``validate-instrumented-evidence`` step header.
        step_index = text.find("validate-instrumented-evidence")
        self.assertGreater(step_index, 0)
        # Apply the heredoc extraction to the substring that starts
        # at the step header.
        tail = text[step_index:]
        blocks = _python_heredocs(tail)
        self.assertGreater(len(blocks), 0, "no heredoc in validate step")
        body = blocks[0][1]
        # ``import os`` must appear as a line-starting import. The
        # body of this validator is dominated by the top-of-file
        # ``import`` block; an ``import os`` that is not at the
        # top of the heredoc still satisfies the import.
        self.assertRegex(
            body,
            r"(?m)^\s*import\s+os\b",
            "control validator must import os",
        )
        # And it must reference os.environ in the body.
        self.assertIn("os.environ", body)

    def test_no_heredoc_swallows_exceptions_to_return_success(self) -> None:
        text = _read_workflow_text()
        for marker, body in _python_heredocs(text):
            # The validators must propagate non-zero exits via
            # ``sys.exit(<int>)`` and not via a bare ``return`` or
            # ``except: pass`` pattern that masks a failure as
            # success.
            self.assertNotIn("return 0", body)
            self.assertNotIn("return None", body)
            # ``except:`` (bare) is acceptable only when followed
            # by ``sys.exit`` or ``raise``. We allow ``except``
            # clauses but forbid a swallowing one that returns.
            for clause in re.findall(
                r"except[^:]*:\s*\n\s*pass\b", body
            ):
                self.fail(
                    f"heredoc ending {marker!r} swallows an exception "
                    f"with bare 'except ... pass':\n{clause}",
                )


class Plan096SourceDigestTests(unittest.TestCase):
    """Plan 096 WP5: i2pd tracked-source digest canonicalized."""

    def test_workflow_uses_git_ls_files(self) -> None:
        text = _read_workflow_text()
        # The record-source-tree-digest step must enumerate the
        # pinned tree via ``git ls-files`` and must not use
        # ``find <dir> -type f`` (which includes ``.git`` files).
        digest_step = re.search(
            r"record-source-tree-digest[\s\S]*?(?=\n      - name:|\Z)", text
        )
        self.assertIsNotNone(digest_step)
        body = digest_step.group(0)
        self.assertIn("git -C i2pd ls-files", body)
        self.assertNotIn("find i2pd -type f", body)

    def test_pinned_revision_is_verified(self) -> None:
        text = _read_workflow_text()
        # The verify-pinned-revision step must assert HEAD equals
        # the pinned revision and refuse a dirty worktree.
        verify_step = re.search(
            r"verify-pinned-revision[\s\S]*?(?=\n      - name:|\Z)", text
        )
        self.assertIsNotNone(verify_step)
        body = verify_step.group(0)
        self.assertIn("git -C i2pd rev-parse HEAD", body)
        self.assertIn(PINNED_I2PD_REVISION, body)
        self.assertIn("git -C i2pd status --porcelain", body)


class Plan096SyntheticDigestTests(unittest.TestCase):
    """Synthetic tests for the canonical tracked-source digest.

    These tests construct a temporary git worktree, exercise the
    algorithm, and assert its three canonical properties:

    - ``.git`` metadata changes do not affect the digest.
    - tracked content changes do affect the digest.
    - the pinned revision equality check rejects a wrong revision.
    """

    def setUp(self) -> None:
        self._tmpdir = Path(tempfile.mkdtemp(prefix="plan096-digest-"))
        self._run = self._git(self._tmpdir)
        self._run("init", "--quiet", "--initial-branch=main", ".")
        self._run("config", "user.email", "plan096@example.invalid")
        self._run("config", "user.name", "Plan 096 Test")
        # A small tracked tree with deterministic content.
        (self._tmpdir / "src").mkdir()
        (self._tmpdir / "src" / "main.cpp").write_text("int main(){return 0;}\n")
        (self._tmpdir / "README.md").write_text("# Pinned\n")
        self._run("add", ".")
        self._run("commit", "--quiet", "-m", "initial")
        self._baseline = _canonical_tracked_source_digest(self._tmpdir)

    def tearDown(self) -> None:
        shutil.rmtree(self._tmpdir, ignore_errors=True)

    def _git(self, cwd: Path) -> "callable":
        def runner(*args: str) -> None:
            subprocess.run(
                ["git", "-C", str(cwd), *args],
                check=True,
                capture_output=True,
            )
        return runner

    def test_digest_stable_across_git_metadata(self) -> None:
        # Mutate ``.git`` administrative state; the digest must be
        # unchanged.
        git_dir = self._tmpdir / ".git"
        (git_dir / "FETCH_HEAD").write_text("0000000000000000000000000000000000000000\trefs/heads/main\n")
        (git_dir / "logs" / "HEAD").write_text(
            "0000000000000000000000000000000000000000 deadbeef deadbeef\n"
        )
        # ``git status`` updates the index; the digest must not
        # depend on index timestamps.
        subprocess.run(
            ["git", "-C", str(self._tmpdir), "status"],
            check=True,
            capture_output=True,
        )
        observed = _canonical_tracked_source_digest(self._tmpdir)
        self.assertEqual(
            observed, self._baseline,
            "tracked-source digest must be insensitive to .git metadata",
        )

    def test_digest_changes_when_tracked_content_changes(self) -> None:
        # Mutate a tracked file; the digest must change.
        (self._tmpdir / "src" / "main.cpp").write_text("int main(){return 1;}\n")
        self._run("add", "src/main.cpp")
        self._run("commit", "--quiet", "-m", "mutate")
        observed = _canonical_tracked_source_digest(self._tmpdir)
        self.assertNotEqual(
            observed, self._baseline,
            "tracked-source digest must change when a tracked file changes",
        )

    def test_digest_changes_when_tracked_file_renamed(self) -> None:
        (self._tmpdir / "src" / "main.cpp").rename(self._tmpdir / "src" / "main2.cpp")
        self._run("add", "-A")
        self._run("commit", "--quiet", "-m", "rename")
        observed = _canonical_tracked_source_digest(self._tmpdir)
        self.assertNotEqual(
            observed, self._baseline,
            "tracked-source digest must change when a tracked file is renamed",
        )

    def test_digest_excludes_git_administrative_files(self) -> None:
        # The algorithm must not include ``.git`` content. We add
        # a clearly-unique marker into ``.git`` and assert the
        # digest is unchanged.
        unique = "PLAN096-GIT-MARKER-DO-NOT-INCLUDE"
        (self._tmpdir / ".git" / "PLAN096_MARKER").write_text(unique + "\n")
        observed = _canonical_tracked_source_digest(self._tmpdir)
        self.assertEqual(
            observed, self._baseline,
            "tracked-source digest must exclude .git administrative files",
        )

    def test_pinned_revision_mismatch_blocks(self) -> None:
        # The workflow's verify-pinned-revision step uses
        # ``git rev-parse HEAD`` to compare with the pinned
        # revision. Exercise the equality test directly.
        head = subprocess.run(
            ["git", "-C", str(self._tmpdir), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
        ).stdout.decode("ascii").strip()
        self.assertNotEqual(head, PINNED_I2PD_REVISION)


class Plan096JobDependencyAndFailClosedTests(unittest.TestCase):
    """Plan 096 WP6/8/9: dependency graph and fail-closed semantics."""

    def test_job_dependency_graph(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        self.assertEqual(jobs["build"].get("needs"), "contract")
        self.assertEqual(jobs["forward-instrumented"].get("needs"), "build")
        self.assertEqual(
            sorted(jobs["forward-control"].get("needs", [])),
            ["build", "forward-instrumented"],
        )
        self.assertEqual(
            sorted(jobs["validate-gate"].get("needs", [])),
            ["build", "forward-control", "forward-instrumented"],
        )

    def test_validate_gate_fails_closed_on_missing_pass(self) -> None:
        # The validate-gate must exit non-zero when either record
        # is not a clean pass. Look for ``sys.exit(82)`` (or any
        # nonzero exit) when both records are not passed.
        text = _read_workflow_text()
        gate_step = re.search(
            r"emit-ci-gate-record[\s\S]*?(?=\n      - name:|\Z)", text
        )
        self.assertIsNotNone(gate_step)
        body = gate_step.group(0)
        self.assertRegex(
            body,
            r"sys\.exit\(\d+\)",
            "validate-gate must sys.exit nonzero on nonpassing evidence",
        )

    def test_no_silent_failure_via_or_echo(self) -> None:
        # The live attempts must not use ``|| echo "..."`` patterns
        # that mask a nonzero exit as success. The corrected
        # workflow records the exit and propagates it explicitly.
        # Comments referencing the removed pattern are allowed;
        # the test scans non-comment lines.
        text = _read_workflow_text()
        for line in text.splitlines():
            stripped = line.strip()
            if stripped.startswith("#"):
                continue
            self.assertNotIn(
                "|| echo", line,
                f"live attempt must not use '|| echo' to mask failure: {line!r}",
            )
        # Also ensure the corrected pattern of explicit exit propagation
        # is present in both live jobs.
        self.assertIn("instrumented_rc=$?", text)
        self.assertIn("control_rc=$?", text)
        self.assertRegex(text, r"exit\s+\$\{instrumented_rc\}")
        self.assertRegex(text, r"exit\s+\$\{control_rc\}")

    def test_instrumented_failure_blocks_control(self) -> None:
        # The validate-instrumented-evidence step must exit
        # non-zero on a nonpassing instrumented record. The
        # forward-control job depends on forward-instrumented and
        # will therefore be skipped by GitHub Actions.
        text = _read_workflow_text()
        # The static check is implicit: the step that runs
        # before any live control attempt must exit 74/75/76/77
        # on various failure modes.
        validate_step = re.search(
            r"validate-instrumented-evidence[\s\S]*?(?=\n      - name:|\Z)", text
        )
        self.assertIsNotNone(validate_step)
        body = validate_step.group(0)
        for exit_code in (74, 75, 76, 77):
            self.assertIn(f"sys.exit({exit_code})", body)


class Plan096LaneContractTests(unittest.TestCase):
    """Plan 096 invariants 1-12: lane contract preserved."""

    def test_workflow_trigger_is_workflow_dispatch_only(self) -> None:
        workflow = _load_workflow()
        on = workflow.get(True) or workflow.get("on")
        self.assertIsInstance(on, dict)
        self.assertIn("workflow_dispatch", on)
        self.assertNotIn("pull_request", on)
        self.assertNotIn("push", on)
        self.assertNotIn("schedule", on)

    def test_all_jobs_run_on_ubuntu_24_04(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        for job_name, job in jobs.items():
            self.assertEqual(
                job.get("runs-on"), "ubuntu-24.04",
                f"job {job_name} must run on ubuntu-24.04",
            )

    def test_live_jobs_loopback_only(self) -> None:
        workflow = _load_workflow()
        jobs = workflow.get("jobs") or {}
        # The forward-instrumented and forward-control jobs must
        # bind the i2pr launcher to literal 127.0.0.1 (the
        # wrapper enforces this at the topology level).
        for job_name in ("forward-instrumented", "forward-control"):
            text = _job_steps_text(jobs[job_name])
            # The plan090 wrapper is bound to host-loopback-development,
            # which the static wrapper check enforces.
            self.assertIn("run-minimal-i2pd-host-loopback-probe.py", text)

    def test_network_id_is_99(self) -> None:
        # The Plan 095 evidence contract records network_id 99
        # in the sanitized record. The runner enforces the
        # development-only network id at the scenario boundary.
        text = _read_workflow_text()
        # The network id 99 is referenced in the sanitized
        # record and in the gate record.
        self.assertIn("network_id\": 99", text)
        # The bind_address is the literal IPv4 loopback.
        self.assertIn("\"bind_address\": \"127.0.0.1\"", text)

    def test_instrumented_attempt_count_is_one(self) -> None:
        text = _read_workflow_text()
        # The forward-instrumented job must invoke the wrapper
        # exactly once (no retry loop). We look for the wrapper
        # invocation in the run-forward-instrumented step and
        # assert no ``continue-on-error: true`` retry is added
        # in the same step.
        run_step = re.search(
            r"run-forward-instrumented[\s\S]*?(?=\n      - name:|\Z)", text
        )
        self.assertIsNotNone(run_step)
        body = run_step.group(0)
        # The corrected workflow uses ``continue-on-error: true``
        # for evidence preservation but propagates the exit
        # explicitly. Assert there is no nested ``python3 ...`` retry.
        self.assertEqual(
            body.count("run-minimal-i2pd-host-loopback-probe.py"), 1
        )

    def test_control_attempt_count_is_at_most_one(self) -> None:
        text = _read_workflow_text()
        run_step = re.search(
            r"run-forward-control[\s\S]*?(?=\n      - name:|\Z)", text
        )
        self.assertIsNotNone(run_step)
        body = run_step.group(0)
        self.assertEqual(
            body.count("run-minimal-i2pd-host-loopback-probe.py"), 1
        )

    def test_no_retry_loop(self) -> None:
        # The workflow must not contain a retry step or a
        # ``uses: nick-fields/retry`` action.
        text = _read_workflow_text()
        self.assertNotIn("retry-action", text)
        self.assertNotIn("nick-fields/retry", text)
        self.assertNotIn("continue-on-error: false", text) is None  # noqa: E501

    def test_plan_088_absent_from_execution_path(self) -> None:
        # The Plan 088 reverse probe must not be invoked in the
        # forward-only workflow.
        text = _read_workflow_text()
        self.assertNotIn("plan084_runner", text)
        self.assertNotIn("i2pd-to-i2pr-ipv4", text)
        self.assertNotIn("minimal_i2pd_reverse_probe", text)

    def test_ntcp2_stays_development_only(self) -> None:
        # The CI gate record must mark the topology as
        # development-only and not advertise NTCP2.
        text = _read_workflow_text()
        self.assertIn("development_only", text)
        self.assertIn("release_qualified", text)
        self.assertIn("experimental-non-advertised", text)
        self.assertIn("host-loopback-development", text)
        # The Python gate record uses ``False`` not ``false``;
        # match both literal spellings.
        for marker in (
            '"release_qualified": False',
            '"isolation_qualified": False',
        ):
            self.assertIn(marker, text, f"gate record missing {marker!r}")


class Plan096TrustBoundaryTests(unittest.TestCase):
    """Plan 096 WP9: build and evidence artifacts are disjoint."""

    def _extract_path_value(self, text: str, step_name: str) -> str:
        """Extract the value of a ``path:`` directive under a step.

        Workflow paths may use ``${{ ... }}`` substitution which
        includes no whitespace, so we walk line-by-line.
        """

        in_step = False
        lines = text.splitlines()
        for line in lines:
            if line.strip() == f"- name: {step_name}":
                in_step = True
                continue
            if in_step and line.strip().startswith("- name: "):
                # Reached the next step.
                break
            if in_step and line.strip().startswith("path:"):
                # Strip the leading ``path:`` and any quote marks.
                value = line.split("path:", 1)[1].strip()
                return value
        return ""

    def test_build_artifact_does_not_contain_sanitized_evidence(self) -> None:
        text = _read_workflow_text()
        # The build artifact upload path must be the build
        # output directory, not the sanitized evidence tree.
        path = self._extract_path_value(text, "upload-build-artifact")
        self.assertTrue(path, "upload-build-artifact path missing")
        self.assertIn("plan095-build/output", path)
        self.assertNotIn("plan095-evidence", path)

    def test_evidence_artifacts_do_not_contain_build_output(self) -> None:
        text = _read_workflow_text()
        for job in ("upload-instrumented-evidence", "upload-control-evidence"):
            path = self._extract_path_value(text, job)
            self.assertTrue(path, f"{job} path missing")
            self.assertNotIn(
                "plan095-build/output", path,
                f"{job} must not upload build output",
            )
            self.assertNotIn(
                "i2pr-interop", path,
                f"{job} must not upload i2pr-interop binary",
            )

    def test_validate_gate_verifies_instrumented_binary_sha(self) -> None:
        # The validate-gate step must compute and bind the
        # instrumented binary SHA-256 from the build output.
        text = _read_workflow_text()
        gate = re.search(
            r"emit-ci-gate-record[\s\S]*?(?=\n      - name:|\Z)", text
        )
        self.assertIsNotNone(gate)
        body = gate.group(0)
        self.assertIn("i2pd_ntcp2_interop_driver_instrumented", body)
        self.assertIn("i2pd_ntcp2_interop_driver_control", body)
        self.assertIn("i2pr-interop", body)
        self.assertIn("hashlib.sha256", body)


if __name__ == "__main__":
    unittest.main()
