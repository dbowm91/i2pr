from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
CHECKER = REPO_ROOT / "scripts/check-global-plan-number-uniqueness.py"


class GlobalPlanNumberUniquenessTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.plans = Path(self.temp.name) / "plans"
        (self.plans / "implementation").mkdir(parents=True)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def add(self, relative: str) -> None:
        path = self.plans / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("# fixture\n", encoding="utf-8")

    def run_checker(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(CHECKER), "--plans-root", str(self.plans)],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_new_unique_number_passes(self) -> None:
        self.add("implementation/example/900-unique.md")
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_duplicate_implementation_owner_fails_with_both_paths(self) -> None:
        first = "implementation/first/900-one.md"
        second = "implementation/second/900-two.md"
        self.add(first)
        self.add(second)
        result = self.run_checker()
        self.assertEqual(result.returncode, 1)
        self.assertIn(first, result.stderr)
        self.assertIn(second, result.stderr)

    def test_same_owner_closure_record_does_not_create_second_owner(self) -> None:
        self.add("implementation/example/900-plan.md")
        self.add("closure/example/900-status.md")
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_closure_in_another_subsystem_fails(self) -> None:
        self.add("implementation/example/900-plan.md")
        misplaced = "closure/other/900-status.md"
        self.add(misplaced)
        result = self.run_checker()
        self.assertEqual(result.returncode, 1)
        self.assertIn(misplaced, result.stderr)

    def test_exact_historical_collisions_are_allowed(self) -> None:
        self.add("implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md")
        self.add("implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md")
        self.add("implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md")
        self.add("implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md")
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_fifth_historical_number_owner_fails(self) -> None:
        self.add("implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md")
        self.add("implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md")
        extra = "implementation/example/296-extra-owner.md"
        self.add(extra)
        result = self.run_checker()
        self.assertEqual(result.returncode, 1)
        self.assertIn(extra, result.stderr)


if __name__ == "__main__":
    unittest.main()
