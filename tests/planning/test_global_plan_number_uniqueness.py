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

    def write_ledger(self, rows: str = "") -> None:
        """Install the collision ledger the guard derives its allowlist from.

        Plan 372 removed the guard's hardcoded collision dict and made it read
        this table instead, so a fixture tree without one is a tree the guard
        must refuse rather than silently pass over. `test_missing_ledger_fails_closed`
        covers the refusal; every other test needs a real ledger.
        """
        (self.plans / "global-number-collision-ledger.md").write_text(
            "# fixture ledger\n\n"
            "## Plan-number collisions\n\n"
            "| Qualified plan | Implementation authority | Closure authority |\n"
            "| --- | --- | --- |\n"
            + rows,
            encoding="utf-8",
        )

    def ledger_row(self, qualified: str, implementation: str) -> str:
        return (
            f"| {qualified} | [`{implementation}`]({implementation}) | "
            f"`not closed` |\n"
        )

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
        self.write_ledger()
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_duplicate_implementation_owner_fails_with_both_paths(self) -> None:
        self.write_ledger()
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
        self.write_ledger()
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_closure_in_another_subsystem_fails(self) -> None:
        self.add("implementation/example/900-plan.md")
        self.write_ledger()
        misplaced = "closure/other/900-status.md"
        self.add(misplaced)
        result = self.run_checker()
        self.assertEqual(result.returncode, 1)
        self.assertIn(misplaced, result.stderr)

    def test_exact_historical_collisions_are_allowed(self) -> None:
        self.add("implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md")
        self.add("implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md")
        self.add("implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md")
        self.add("implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md")
        self.add("implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md")
        self.add("implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md")
        self.add("implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md")
        self.add("implementation/i2pcontrol-proposal-170/350-floodfill-type5-serve-path.md")
        self.add("implementation/portable-service-tunnels/350-service-tunnel-package-api-and-dependency-stabilization.md")
        self.add("implementation/i2pcontrol-proposal-170/351-els2-consumer-service-wiring.md")
        self.add("implementation/portable-service-tunnels/351-external-adapter-conformance-and-sam-handoff-contract.md")
        self.add("implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md")
        self.add("implementation/managed-native-app-runtime/352-managed-app-mapped-ipv6-policy-canonicalization-corrective.md")
        self.write_ledger(
            "".join(
                self.ledger_row(qualified, path)
                for qualified, path in (
                    ("Proposal 170/296", "implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md"),
                    ("Anonymity/296", "implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md"),
                    ("Proposal 170/297", "implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md"),
                    ("Anonymity/297", "implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md"),
                    ("Proposal 170/349", "implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md"),
                    ("Managed native app runtime/349", "implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md"),
                    ("Portable service-tunnels/349", "implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md"),
                    ("Proposal 170/350", "implementation/i2pcontrol-proposal-170/350-floodfill-type5-serve-path.md"),
                    ("Portable service-tunnels/350", "implementation/portable-service-tunnels/350-service-tunnel-package-api-and-dependency-stabilization.md"),
                    ("Proposal 170/351", "implementation/i2pcontrol-proposal-170/351-els2-consumer-service-wiring.md"),
                    ("Portable service-tunnels/351", "implementation/portable-service-tunnels/351-external-adapter-conformance-and-sam-handoff-contract.md"),
                    ("Proposal 170/352", "implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md"),
                    ("Managed native app runtime/352", "implementation/managed-native-app-runtime/352-managed-app-mapped-ipv6-policy-canonicalization-corrective.md"),
                )
            )
        )
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unrecorded_plan_349_owner_fails(self) -> None:
        self.add("implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md")
        self.add("implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md")
        self.add("implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md")
        self.write_ledger(
            self.ledger_row("Proposal 170/349", "implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md")
            + self.ledger_row("Managed native app runtime/349", "implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md")
            + self.ledger_row("Portable service-tunnels/349", "implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md")
        )
        extra = "implementation/example/349-extra-owner.md"
        self.add(extra)
        result = self.run_checker()
        self.assertEqual(result.returncode, 1)
        self.assertIn(extra, result.stderr)

    def test_fifth_historical_number_owner_fails(self) -> None:
        self.add("implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md")
        self.add("implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md")
        self.write_ledger(
            self.ledger_row("Proposal 170/296", "implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md")
            + self.ledger_row("Anonymity/296", "implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md")
        )
        extra = "implementation/example/296-extra-owner.md"
        self.add(extra)
        result = self.run_checker()
        self.assertEqual(result.returncode, 1)
        self.assertIn(extra, result.stderr)


    def test_missing_ledger_fails_closed(self) -> None:
        """No ledger means the guard cannot check, and must not report success.

        Treating an unreadable authority as an empty allowlist would turn every
        recorded collision into a hard failure; treating it as a permissive one
        would defeat the guard. Exit 2 is the only honest answer.
        """
        self.add("implementation/example/900-unique.md")
        result = self.run_checker()
        self.assertEqual(result.returncode, 2)
        self.assertIn("ledger", result.stderr.lower())

    def test_empty_collision_table_is_a_legitimate_state(self) -> None:
        """A repository with no historical collisions is not a broken one.

        The allowlist is then simply empty, under which a clean tree passes and
        any collision on disk still fails. Refusing to run here would make the
        guard unusable for exactly the repository it was written to protect.
        """
        self.add("implementation/example/900-unique.md")
        self.write_ledger()
        self.assertEqual(self.run_checker().returncode, 0)
        self.add("implementation/first/900-one.md")
        self.add("implementation/second/900-two.md")
        self.assertEqual(self.run_checker().returncode, 1)

    def test_ledger_without_the_collision_section_fails_closed(self) -> None:
        """A renamed or missing section means the parser is reading nothing."""
        self.add("implementation/example/900-unique.md")
        (self.plans / "global-number-collision-ledger.md").write_text(
            "# fixture ledger\n\n## ADR-number collisions\n\n| ADR no. | Qualified ADR |\n",
            encoding="utf-8",
        )
        result = self.run_checker()
        self.assertEqual(result.returncode, 2)

    def test_collision_recorded_only_in_the_guard_is_not_tolerated(self) -> None:
        """The ledger is the only place a collision may be declared.

        Before Plan 372 the allowlist was a hardcoded dict in the guard and the
        ledger was a second copy of the same table; recording a collision in one
        and not the other was possible, and is exactly how the 368 collision
        surfaced as a red floor step. Deriving the allowlist removes the split.
        """
        self.add("implementation/first/900-one.md")
        self.add("implementation/second/900-two.md")
        self.write_ledger()  # records nothing
        result = self.run_checker()
        self.assertEqual(result.returncode, 1)
        self.assertIn("900", result.stderr)


if __name__ == "__main__":
    unittest.main()
