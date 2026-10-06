"""Tests for ``scripts/check-adr-number-uniqueness.py`` (Plan 361).

Every fixture lives in a temp dir. The real ``docs/adr/`` tree is only ever
read, never mutated -- this suite must not be able to renumber an ADR.

The negative cases are the point. The contracts below are shared: the
discoverable tests run each contract against the real checker, and the mutation
tests run the *same* contracts against deliberately broken copies, so a
mutation is only reported as caught when an assertion this suite already makes
rejects it.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Callable
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[2]
CHECKER = REPO_ROOT / "scripts/check-adr-number-uniqueness.py"
ADR_ROOT = REPO_ROOT / "docs/adr"
LEDGER = REPO_ROOT / "plans/global-number-collision-ledger.md"

# An independent copy of the recorded collisions, hard-coded here on purpose.
# If it were read from the script it would follow any mutation of the constant
# it is meant to check.
RECORDED_COLLISIONS = {
    "0030": frozenset(
        {
            "0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md",
            "0030-loopback-controlled-floodfill-reachability-advertisement.md",
        }
    ),
    "0032": frozenset(
        {
            "0032-managed-native-app-process-and-capability-boundary.md",
            "0032-els2-type11-signature-profile-boundary.md",
        }
    ),
    "0033": frozenset(
        {
            "0033-portable-service-tunnel-policy-core-and-adapters.md",
            "0033-els2-consumer-lookup-identity-and-install-key.md",
        }
    ),
}
RECORDED_FILES = sorted(name for group in RECORDED_COLLISIONS.values() for name in group)


@contextlib.contextmanager
def adr_fixture(*extra: str, omit: frozenset[str] = frozenset()):
    """Build a temp ADR tree holding every recorded collision plus ``extra``."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "docs" / "adr"
        root.mkdir(parents=True)
        for name in RECORDED_FILES:
            if name not in omit:
                (root / name).write_text("# fixture\n", encoding="utf-8")
        for name in extra:
            (root / name).write_text("# fixture\n", encoding="utf-8")
        yield root


def run_checker(adr_root: Path, checker: Path = CHECKER) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(checker), "--adr-root", str(adr_root)],
        capture_output=True,
        text=True,
        check=False,
        cwd=REPO_ROOT,
    )


def assert_clean(checker: Path, adr_root: Path) -> None:
    result = run_checker(adr_root, checker)
    assert result.returncode == 0, (
        f"expected exit 0, got {result.returncode}\n"
        f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
    )


def assert_fails(checker: Path, adr_root: Path, *must_mention: str) -> None:
    result = run_checker(adr_root, checker)
    assert result.returncode == 1, (
        f"expected exit 1, got {result.returncode}\n"
        f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
    )
    for needle in must_mention:
        assert needle in result.stderr, (
            f"stderr did not mention {needle!r}\nstderr:\n{result.stderr}"
        )


# --- contracts ---------------------------------------------------------------
# Each contract builds its own fixture and asserts one invariant. Raising
# AssertionError is the signal that a checker -- real or mutated -- broke the
# invariant, so the mutation tests below can reuse them unchanged.


def contract_unique_tree_passes(checker: Path) -> None:
    with adr_fixture("0000-a.md", "0001-b.md", "0031-c.md") as root:
        assert_clean(checker, root)


def contract_duplicate_number_fails_naming_both(checker: Path) -> None:
    first = "0900-first-claimant.md"
    second = "0900-second-claimant.md"
    with adr_fixture(first, second) as root:
        assert_fails(checker, root, first, second, "0900")


def contract_malformed_filename_fails(checker: Path) -> None:
    with adr_fixture("0000-good.md", "notes.md") as root:
        assert_fails(checker, root, "notes.md", "unparseable ADR filename")


def contract_nested_unparseable_filename_fails(checker: Path) -> None:
    with adr_fixture("0000-good.md") as root:
        (root / "archive").mkdir()
        (root / "archive" / "README.md").write_text("# fixture\n", encoding="utf-8")
        assert_fails(checker, root, "archive/README.md")


def contract_recorded_collision_is_tolerated(checker: Path) -> None:
    with adr_fixture() as root:
        assert_clean(checker, root)


def contract_third_claimant_of_recorded_number_fails(checker: Path) -> None:
    extra = "0032-third-claimant.md"
    with adr_fixture(extra) as root:
        assert_fails(checker, root, extra, "0032")


def contract_renamed_recorded_file_fails(checker: Path) -> None:
    # 0033 still has two claimants, but one was renamed: tolerated only on an
    # exact set match, so the mismatch itself must be reported.
    original = "0033-portable-service-tunnel-policy-core-and-adapters.md"
    renamed = "0033-portable-service-tunnel-policy-core-adapters.md"
    with adr_fixture(renamed, omit=frozenset({original})) as root:
        assert_fails(checker, root, renamed, "does not match the recorded collision")


def contract_stale_tolerated_entry_fails(checker: Path) -> None:
    retired = "0030-loopback-controlled-floodfill-reachability-advertisement.md"
    with adr_fixture(omit=frozenset({retired})) as root:
        assert_fails(checker, root, retired, "stale")


def contract_missing_root_fails(checker: Path) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        assert_fails(checker, Path(tmp) / "docs/adr", "not a directory")


ALL_CONTRACTS = (
    contract_unique_tree_passes,
    contract_duplicate_number_fails_naming_both,
    contract_malformed_filename_fails,
    contract_nested_unparseable_filename_fails,
    contract_recorded_collision_is_tolerated,
    contract_third_claimant_of_recorded_number_fails,
    contract_renamed_recorded_file_fails,
    contract_stale_tolerated_entry_fails,
    contract_missing_root_fails,
)


class AdrNumberUniquenessTests(unittest.TestCase):
    """Each contract, run against the real checker."""

    def test_contracts_hold(self) -> None:
        for contract in ALL_CONTRACTS:
            with self.subTest(contract=contract.__name__):
                contract(CHECKER)

    def test_real_tree_passes(self) -> None:
        self.assertEqual(run_checker(ADR_ROOT).returncode, 0)

    def test_script_tolerated_set_matches_this_suite(self) -> None:
        module = load_checker_module()
        self.assertEqual(dict(module.TOLERATED_DUPLICATES), RECORDED_COLLISIONS)

    def test_tolerated_set_has_no_glob(self) -> None:
        # Toleration is an exact set match; a glob would silently widen it.
        for number, names in RECORDED_COLLISIONS.items():
            for name in names:
                self.assertFalse(
                    set(name) & set("*?[]"), f"glob metacharacter in {number}: {name}"
                )

    def test_real_tree_contains_exactly_the_recorded_collisions(self) -> None:
        owners: dict[str, list[str]] = {}
        for path in sorted(ADR_ROOT.glob("*.md")):
            owners.setdefault(path.name.split("-", 1)[0], []).append(path.name)
        duplicated = {n: sorted(p) for n, p in owners.items() if len(p) > 1}
        self.assertEqual(duplicated, {n: sorted(p) for n, p in RECORDED_COLLISIONS.items()})

    def test_every_tolerated_entry_is_named_in_the_ledger(self) -> None:
        text = LEDGER.read_text(encoding="utf-8")
        self.assertIn("check-adr-number-uniqueness.py", text)
        for number, names in RECORDED_COLLISIONS.items():
            self.assertIn(number, text)
            for name in names:
                self.assertIn(name, text)

    def test_tolerated_set_is_load_bearing_against_the_real_tree(self) -> None:
        """Dropping one entry must make the REAL tree fail, naming both files.

        Proves the tolerated set is load-bearing rather than decorative.
        """
        for number in sorted(RECORDED_COLLISIONS):
            with self.subTest(dropped=number):
                reduced = {n: v for n, v in RECORDED_COLLISIONS.items() if n != number}
                with mock_tolerated(reduced) as module:
                    errors = module.find_errors(ADR_ROOT)
                    self.assertTrue(
                        errors, "real tree passed without a recorded exemption"
                    )
                    joined = "\n".join(errors)
                    for name in RECORDED_COLLISIONS[number]:
                        self.assertIn(name, joined)

                    # ... and the exit code flips, not just the error list.
                    stderr = io.StringIO()
                    with mock_argv(ADR_ROOT), contextlib.redirect_stderr(stderr):
                        self.assertEqual(module.main(), 1)
                    self.assertIn("ADR-number ownership check failed", stderr.getvalue())


class GuardMutationTests(unittest.TestCase):
    """Break the guard on purpose; the suite's own contracts must catch it."""

    def test_mutations_are_caught(self) -> None:
        source = CHECKER.read_text(encoding="utf-8")
        for name, (edits, catching) in MUTATIONS.items():
            with self.subTest(mutation=name):
                mutant_source, _applied = apply_edits(source, edits)
                # A mutation that does not compile would be "caught" for the
                # wrong reason; prove it is still a runnable script.
                compile(mutant_source, "<mutant>", "exec")
                with tempfile.TemporaryDirectory() as tmp:
                    mutant = Path(tmp) / "check-adr-number-uniqueness.py"
                    mutant.write_text(mutant_source, encoding="utf-8")
                    self.assertTrue(
                        catching, f"mutation {name} declares no catching contract"
                    )
                    for contract in catching:
                        with self.assertRaises(AssertionError, msg=(
                            f"mutation {name} was NOT caught by {contract.__name__}"
                        )):
                            contract(mutant)


# --- helpers shared with the mutation harness --------------------------------


def load_checker_module():
    """Load a fresh module object for the checker script."""
    spec = importlib.util.spec_from_file_location(
        "i2pr_check_adr_number_uniqueness", CHECKER
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@contextlib.contextmanager
def mock_tolerated(replacement: dict[str, frozenset[str]]):
    """Temporarily swap the tolerated set on one freshly loaded module.

    Yields that same module object, so the caller exercises the patched
    ``find_errors``/``main`` rather than a second, unpatched copy.
    """
    module = load_checker_module()
    with mock.patch.object(module, "TOLERATED_DUPLICATES", replacement):
        yield module


@contextlib.contextmanager
def mock_argv(adr_root: Path):
    with mock.patch.object(
        sys, "argv", ["check-adr-number-uniqueness", "--adr-root", str(adr_root)]
    ):
        yield


def apply_edits(source: str, edits: tuple[tuple[str, str], ...]) -> tuple[str, list[str]]:
    """Apply exact-string edits, failing loudly if the script text drifted."""
    applied: list[str] = []
    for old, new in edits:
        count = source.count(old)
        if count != 1:
            raise AssertionError(f"mutation anchor matched {count} times, expected 1: {old!r}")
        source = source.replace(old, new)
        applied.append(old.splitlines()[0].strip())
    return source, applied


# --- mutations ---------------------------------------------------------------

Contract = Callable[[Path], None]
Mutation = tuple[tuple[tuple[str, str], ...], tuple[Contract, ...]]
MUTATIONS: dict[str, Mutation] = {
    "skip-unparseable-filename": (
        (
            (
                '        if match is None:\n'
                '            errors.append(\n'
                '                f"unparseable ADR filename {relative}: expected {ADR_NAME_EXPECTATION}"\n'
                '            )\n'
                '            continue\n',
                "        if match is None:\n            continue\n",
            ),
        ),
        (contract_malformed_filename_fails, contract_nested_unparseable_filename_fails),
    ),
    "drop-tolerated-set-check": (
        (
            (
                "        if allowed is not None and set(paths) == set(allowed):\n"
                "            continue\n",
                "        if allowed is not None:\n            continue\n",
            ),
        ),
        (contract_third_claimant_of_recorded_number_fails, contract_renamed_recorded_file_fails),
    ),
    "exit-zero-on-error": (
        (
            ('            print(f"- {error}", file=sys.stderr)\n        return 1\n',
             '            print(f"- {error}", file=sys.stderr)\n        return 0\n'),
        ),
        (contract_duplicate_number_fails_naming_both, contract_malformed_filename_fails),
    ),
    "disable-duplicate-detection": (
        (("        if len(paths) <= 1:\n", "        if len(paths) <= 99:\n"),),
        (contract_duplicate_number_fails_naming_both, contract_third_claimant_of_recorded_number_fails),
    ),
    "report-errors-on-stdout": (
        (
            ('            print(f"- {error}", file=sys.stderr)\n',
             '            print(f"- {error}")\n'),
        ),
        (
            contract_duplicate_number_fails_naming_both,
            contract_malformed_filename_fails,
            contract_stale_tolerated_entry_fails,
        ),
    ),
    "drop-stale-exemption-check": (
        (
            (
                "    for number, allowed in sorted(TOLERATED_DUPLICATES.items()):\n"
                "        observed = set(owners.get(number, ()))\n"
                "        missing = sorted(set(allowed) - observed)\n"
                "        if missing:\n",
                "    for number, allowed in ():\n"
                "        observed = set(owners.get(number, ()))\n"
                "        missing = sorted(set(allowed) - observed)\n"
                "        if missing:\n",
            ),
        ),
        (contract_stale_tolerated_entry_fails,),
    ),
}


if __name__ == "__main__":
    unittest.main()