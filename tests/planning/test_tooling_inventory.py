"""Plan 372 — tests for `scripts/check-tooling-inventory.py`.

The guard exists because a published count that no longer matches the tree is
invisible until a reader happens to notice it. Two of the counts this plan
corrected had been wrong in the direction that misleads: `tooling.md` named
the MSRV CI job at a toolchain the project does not support, and asserted
zero Rust integration files under `tests/` while one is tracked.

Every case below builds a throwaway tree. The repository's own documents are
only ever read, never mutated.
"""

from __future__ import annotations

import importlib.util
import re
import shutil
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SCRIPT = REPO_ROOT / "scripts" / "check-tooling-inventory.py"
TOOLING = REPO_ROOT / "docs" / "architecture" / "tooling.md"
OVERVIEW = REPO_ROOT / "docs" / "architecture" / "overview.md"


def _load_guard():
    spec = importlib.util.spec_from_file_location("tooling_inventory_guard", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


GUARD = _load_guard()


class DerivationTest(unittest.TestCase):
    """The figures the guard publishes must come from the tree, not a table."""

    def setUp(self) -> None:
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.root = Path(self.tmp) / "tree"
        # A minimal tree: only what the derivation reads.
        (self.root / "scripts").mkdir(parents=True)
        (self.root / "crates").mkdir(parents=True)
        (self.root / "docs" / "architecture").mkdir(parents=True)
        (self.root / ".github" / "workflows").mkdir(parents=True)
        (self.root / "fuzz").mkdir(parents=True)
        (self.root / "AGENTS.md").write_text(
            "## Routine floor\n\n```text\n"
            "cargo fmt --all --check\n"
            "bash scripts/check-alpha.sh\n"
            "python3 scripts/check-beta.py\n"
            "```\n",
            encoding="utf-8",
        )
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/one"]\nrust-version = "1.89"\n',
            encoding="utf-8",
        )
        (self.root / "crates" / "one" / "src").mkdir(parents=True)
        (self.root / "crates" / "one" / "Cargo.toml").write_text(
            '[package]\nname = "one"\n', encoding="utf-8"
        )
        (self.root / "fuzz" / "Cargo.toml").write_text(
            "[[bin]]\n[[bin]]\n[[bin]]\n", encoding="utf-8"
        )
        (self.root / ".github" / "workflows" / "ci.yml").write_text(
            "name: CI\non:\n  push:\njobs:\n"
            "  q:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - name: a\n        run: bash scripts/check-alpha.sh\n"
            "      - name: b\n        run: python3 scripts/check-beta.py --self-test\n",
            encoding="utf-8",
        )
        (self.root / "docs" / "architecture" / "tooling.md").write_text("", encoding="utf-8")
        (self.root / "docs" / "architecture" / "overview.md").write_text("", encoding="utf-8")

    def test_floor_block_ignores_prose_after_the_fence(self) -> None:
        """A command line after the closing fence is not a floor step.

        Regression: the first draft of the self-test appended its mutation to
        the end of `AGENTS.md`, where it landed outside the block and was
        correctly ignored. The control then proved nothing.
        """
        (self.root / "AGENTS.md").write_text(
            (self.root / "AGENTS.md").read_text()
            + "\nNot a floor step: bash scripts/check-not-a-step.sh\n",
            encoding="utf-8",
        )
        self.assertEqual(GUARD.floor_total_steps(
            (self.root / "AGENTS.md").read_text()), 3)

    def test_build_artefacts_are_not_counted_as_checkers(self) -> None:
        """`__pycache__` matches a `check-*` glob and must be excluded.

        Four `.pyc` files under `scripts/__pycache__/` do this in the real
        tree, where an unfiltered `find` reports 58 where the answer is 54.
        """
        (self.root / "scripts" / "check-alpha.sh").write_text("", encoding="utf-8")
        (self.root / "scripts" / "check-beta.py").write_text("", encoding="utf-8")
        cache = self.root / "scripts" / "__pycache__"
        cache.mkdir()
        (cache / "check-beta.cpython-312.pyc").write_bytes(b"\x00")
        found = GUARD.tracked_check_scripts(self.root)
        self.assertNotIn("scripts/__pycache__/check-beta.cpython-312.pyc", found)
        self.assertEqual(len(found), 2)

    def test_ci_step_counting_counts_steps_not_commands(self) -> None:
        """One step running two commands is one invocation.

        It is one job step that fails as a unit; counting commands would make
        the figure depend on a formatting choice rather than on the workflow.
        """
        ci = (self.root / ".github" / "workflows" / "ci.yml").read_text()
        self.assertEqual(GUARD.ci_checker_invocations(ci), 2)
        self.assertEqual(GUARD.ci_distinct_checkers(ci), 2)

    def test_fuzz_targets_come_from_the_manifest(self) -> None:
        """`fuzz/fuzz_targets/` also holds a shared `support.rs`.

        Counting the directory would report one more than the manifest
        declares, which is exactly the off-by-one `tooling.md` had.
        """
        targets = self.root / "fuzz" / "fuzz_targets"
        targets.mkdir()
        for name in ("a.rs", "b.rs", "c.rs", "support.rs"):
            (targets / name).write_text("", encoding="utf-8")
        self.assertEqual(GUARD.fuzz_targets(self.root), 3)

    def test_msrv_toolchain_reads_the_msrv_job_not_the_first_one(self) -> None:
        """The `quality` job pins 1.95.0; the `msrv` job pins the floor."""
        ci = (self.root / ".github" / "workflows" / "ci.yml").read_text()
        ci += (
            "  msrv:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - uses: dtolnay/rust-toolchain@master\n"
            "        with:\n          toolchain: 1.89.0\n"
        )
        self.assertEqual(GUARD.ci_msrv_toolchain(ci), "1.89.0")


class PublishedFigureTest(unittest.TestCase):
    """The repository's real documents must satisfy the guard."""

    def test_real_tree_is_clean(self) -> None:
        self.assertEqual(GUARD.scan(REPO_ROOT), [])

    def test_every_check_script_has_an_inventory_row(self) -> None:
        """Rule 3, the load-bearing one.

        Plan 367 found two guards in the floor with no inventory row at all.
        Without this, adding a guard without documenting it is silent.
        """
        documented = GUARD.inventory_table_script_names(
            TOOLING.read_text(encoding="utf-8")
        )
        undocumented = [
            p
            for p in GUARD.tracked_check_scripts(REPO_ROOT)
            if p not in documented
        ]
        self.assertEqual(undocumented, [])

    def test_zero_root_test_claim_is_not_present(self) -> None:
        """`tooling.md` must not claim zero Rust files under `tests/`.

        The claim was false: `tests/portable-service-tunnel-consumer/tests/
        conformance.rs` is tracked, which is the direction that hides a real
        dependency edge.
        """
        tooling = TOOLING.read_text(encoding="utf-8")
        self.assertFalse(GUARD.tooling_claims_zero_root_tests(tooling))
        root_tests = GUARD.root_test_rs_files(REPO_ROOT)
        self.assertIn(
            "tests/portable-service-tunnel-consumer/tests/conformance.rs",
            root_tests,
        )

    def test_published_msrv_matches_the_manifest(self) -> None:
        tooling = TOOLING.read_text(encoding="utf-8")
        manifest = GUARD.workspace_rust_version(REPO_ROOT)
        # The published `rust-version` is compared for equality; the CI job's
        # toolchain is a full version and is compared by prefix, which is what
        # rule 8 does (`1.89.0` satisfies a floor of `1.89`).
        self.assertEqual(GUARD.tooling_rust_version(tooling), manifest)
        self.assertTrue(
            GUARD.ci_msrv_toolchain(
                (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text()
            ).startswith(manifest)
        )

    def test_method_b_tallies_are_internally_consistent(self) -> None:
        """The published Method B figures must match the document's own tables."""
        tooling = TOOLING.read_text(encoding="utf-8")
        self.assertEqual(
            GUARD.published_method_b(tooling), GUARD.method_b_counts(tooling)
        )

    def test_overview_crate_count_excludes_the_tool_package(self) -> None:
        """`overview.md` says "25 crates plus one tool".

        Comparing that prose against every member -- which includes
        `tools/i2pr-interop` -- makes a correct sentence look wrong. The first
        draft of the guard did exactly that and reported a false positive.
        """
        overview = OVERVIEW.read_text(encoding="utf-8")
        self.assertEqual(
            GUARD.overview_member_count(overview),
            len(GUARD.workspace_crate_names(REPO_ROOT)),
        )


class MutationTest(unittest.TestCase):
    """Each rule must reject the specific drift it claims to detect."""

    def setUp(self) -> None:
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.root = Path(self.tmp) / "tree"
        shutil.copytree(
            REPO_ROOT,
            self.root,
            symlinks=True,
            ignore=shutil.ignore_patterns("target", ".git", "__pycache__"),
        )

    def _violations(self) -> list[str]:
        return GUARD.scan(self.root)

    def test_published_count_that_no_longer_matches_is_rejected(self) -> None:
        """Rule 4 must reject a wrong `ci.yml` invocation count.

        The published figure is *derived*, not typed in. An earlier draft of this
        test replaced the literal string `| Checker invocations in `ci.yml` | 36`,
        so the mutation silently became a no-op the first time a plan legitimately
        changed the count -- Plan 379 added a checker and the published figure
        moved to 37. The test then passed with an empty violation list, which is
        indistinguishable from a guard that stopped working. Deriving the number
        from `ci.yml` keeps the mutation meaningful no matter how the count moves.
        """
        ci = (self.root / ".github" / "workflows" / "ci.yml").read_text(
            encoding="utf-8"
        )
        published = GUARD.ci_checker_invocations(ci)
        path = self.root / "docs" / "architecture" / "tooling.md"
        text = path.read_text()
        self.assertIn(
            f"| Checker invocations in `ci.yml` | {published} |",
            text,
            "the published figure the mutation targets is not where it was "
            "expected; update this test rather than weakening it",
        )
        path.write_text(
            text.replace(
                f"| Checker invocations in `ci.yml` | {published} |",
                "| Checker invocations in `ci.yml` | 2 |",
                1,
            ),
            encoding="utf-8",
        )
        self.assertTrue(
            any("rule 4" in v for v in self._violations()),
            self._violations(),
        )

    def test_new_checker_without_an_inventory_row_is_rejected(self) -> None:
        (self.root / "scripts" / "check-planted.sh").write_text(
            "#!/usr/bin/env bash\nexit 0\n", encoding="utf-8"
        )
        found = self._violations()
        self.assertTrue(
            any("rule 3" in v and "check-planted.sh" in v for v in found),
            found,
        )

    def test_stale_member_count_is_rejected(self) -> None:
        path = self.root / "docs" / "architecture" / "tooling.md"
        text = path.read_text()
        changed, replacements = re.subn(
            r"### Members \(\d+ crates \+ 1 non-production binary = \d+\)",
            "### Members (19 crates + 1 non-production binary = 20)",
            text,
            count=1,
        )
        self.assertEqual(replacements, 1)
        path.write_text(changed, encoding="utf-8")
        self.assertTrue(
            any("rule 6" in v for v in self._violations()), self._violations()
        )

    def test_stale_rust_version_is_rejected(self) -> None:
        path = self.root / "docs" / "architecture" / "tooling.md"
        path.write_text(
            path.read_text().replace(
                '`rust-version = "1.89"`', '`rust-version = "1.88"`', 1
            ),
            encoding="utf-8",
        )
        self.assertTrue(
            any("rule 7" in v for v in self._violations()), self._violations()
        )

    def test_zero_test_claim_reintroduced_is_rejected(self) -> None:
        path = self.root / "docs" / "architecture" / "tooling.md"
        path.write_text(
            path.read_text()
            + "\n1. **Zero Rust integration test files under `tests/`.**\n",
            encoding="utf-8",
        )
        self.assertTrue(
            any("rule 10" in v for v in self._violations()), self._violations()
        )

    def test_method_b_tally_typed_from_memory_is_rejected(self) -> None:
        """The figure a human is most tempted to type rather than derive."""
        path = self.root / "docs" / "architecture" / "tooling.md"
        text = path.read_text().replace(
            "rows marked `Floor: yes`", "rows marked `Floor: yes`", 1
        )
        import re

        text = re.sub(
            r"\|\s*[0-9]+ rows marked `Floor: yes`\s*\|",
            "| 99 rows marked `Floor: yes` |",
            text,
            count=1,
        )
        path.write_text(text, encoding="utf-8")
        self.assertTrue(
            any("rule 5b" in v for v in self._violations()), self._violations()
        )


if __name__ == "__main__":
    unittest.main()
