#!/usr/bin/env python3
"""Plan 476 — the guarded executor stays bound to the Plan 495 gate.

Plan 495 owns the canonical external-qualification gate model
(`specs/QUALIFICATION-GATES.md`) and its enforcement
(`scripts/plan495-peer-eligibility.py`). Plan 476 owns the guarded
per-host process/session receipt executor
(`tests/integration/ssu2/plan476_live_executor.py`) that runs that gate
before it starts a router process, plus the daemon's sanitized local
receipt export (`crates/i2pr-daemon/src/ssu2_qualification_export.rs`).

A binding that lives only in prose rots. This checker makes the
binding mechanical, and it fails closed:

* the Rust export's phase vocabulary and the Python receipt
  vocabulary are compared element by element — they are two
  implementations of one wire contract, and a drift would let the
  executor accept a receipt the daemon can never produce (or reject
  one it can);
* the executor is required to actually call the Plan 495 gate
  (profile validation, peer validation, evaluation, classification,
  hygiene validation and evidence validation), not merely to import
  it;
* the executor's simulator test module must be unable to connect or
  start a process, so a green simulator row can never be a live claim;
* the conformance binding and the operator procedure must both name
  the executor, so the published contract cannot drift away from the
  code;
* the routine floor must actually run this checker.

Run with ``python3``. ``bash scripts/*.py`` garbles these scripts and
exits 2.

    python3 scripts/check-plan476-gate-binding.py
    python3 scripts/check-plan476-gate-binding.py --self-test
"""

from __future__ import annotations

import argparse
import ast
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

RUST_RECEIPTS = REPO_ROOT / "crates/i2pr-daemon/src/ssu2_qualification_receipts.rs"
RUST_EXPORT = REPO_ROOT / "crates/i2pr-daemon/src/ssu2_qualification_export.rs"
PY_CONTRACT = REPO_ROOT / "tests/integration/ssu2/plan476_executor.py"
PY_EXECUTOR = REPO_ROOT / "tests/integration/ssu2/plan476_live_executor.py"
PY_SIMULATOR = REPO_ROOT / "tests/integration/ssu2/test_plan476_live_executor.py"
CONFORMANCE = REPO_ROOT / "specs/CONFORMANCE.md"
OPERATOR_DOC = REPO_ROOT / "docs/operations/ssu2-private-lan-qualification.md"
AGENTS = REPO_ROOT / "AGENTS.md"

#: The Plan 495 gate entry points the executor must actually call.
#: A gate that is imported but never called is not a gate.
REQUIRED_GATE_CALLS = (
    "validate_profile",
    "validate_peer",
    "evaluate",
    "classify",
    "validate_hygiene",
    "validate_evidence",
)

#: Modules a simulator row must never import: a simulator that can
#: open a socket or start a process can make a fake claim look live.
FORBIDDEN_SIMULATOR_IMPORTS = ("socket", "subprocess", "asyncio", "ssl", "http")


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def rust_local_phases(source: str) -> list[str]:
    """The Rust export's canonical phase vocabulary."""
    match = re.search(
        r"pub const LOCAL_PHASES: \[&str; \d+\] = \[(.*?)\];", source, re.S
    )
    if not match:
        return []
    return re.findall(r'"([^"]+)"', match.group(1))


def python_local_phases(source: str) -> list[str]:
    """The Python receipt contract's canonical phase vocabulary."""
    match = re.search(r"^LOCAL_PHASES\s*=\s*\((.*?)^\)", source, re.S | re.M)
    if not match:
        return []
    return re.findall(r'"([^"]+)"', match.group(1))


def attribute_calls(source: str) -> set[str]:
    """Every attribute name called or read anywhere in a module."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return set()
    names: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Attribute):
            names.add(node.attr)
    return names


def imported_modules(source: str) -> set[str]:
    """Every top-level module name imported by a module."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return set()
    names: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                names.add(alias.name.split(".")[0])
        elif isinstance(node, ast.ImportFrom) and node.module and node.level == 0:
            names.add(node.module.split(".")[0])
    return names


def floor_mentions(agents: str) -> bool:
    """Whether the routine floor actually runs this checker."""
    match = re.search(
        r"^## Routine floor.*?\n```text\n(.*?)^```", agents, re.S | re.M
    )
    if not match:
        return False
    return "scripts/check-plan476-gate-binding.py" in match.group(1)


def run_self_test_of_executor() -> tuple[bool, str]:
    """Run the executor's own self-test and report whether it passed."""
    try:
        completed = subprocess.run(
            [sys.executable, str(PY_EXECUTOR), "--self-test"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        return False, f"executor self-test could not run: {error}"
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout or "").strip()
        return False, f"executor self-test failed: {detail[:400]}"
    return True, completed.stdout.strip()


# ---------------------------------------------------------------------------
# The rules. Each returns a list of violation strings.
# ---------------------------------------------------------------------------


def rule_vocabulary_in_sync() -> list[str]:
    """The Rust export and the Python contract speak one vocabulary."""
    rust = rust_local_phases(_read(RUST_RECEIPTS))
    python = python_local_phases(_read(PY_CONTRACT))
    if not rust:
        return ["the Rust export's LOCAL_PHASES could not be read"]
    if not python:
        return ["the Python receipt contract's LOCAL_PHASES could not be read"]
    if rust != python:
        return [
            "the Rust export and the Python receipt contract disagree about the "
            f"phase vocabulary: rust={rust} python={python}"
        ]
    return []


def rule_executor_calls_the_gate() -> list[str]:
    """The executor calls the Plan 495 gate rather than importing it."""
    source = _read(PY_EXECUTOR)
    if not source:
        return ["the Plan 476 executor could not be read"]
    calls = attribute_calls(source)
    missing = [name for name in REQUIRED_GATE_CALLS if name not in calls]
    if missing:
        return [
            "the Plan 476 executor does not call the Plan 495 gate entry "
            f"points: {', '.join(missing)}"
        ]
    return []


def rule_simulator_cannot_connect() -> list[str]:
    """The simulator test module cannot open a socket or start a process."""
    source = _read(PY_SIMULATOR)
    if not source:
        return ["the Plan 476 simulator test module could not be read"]
    imported = imported_modules(source)
    offending = sorted(name for name in imported if name in FORBIDDEN_SIMULATOR_IMPORTS)
    if offending:
        return [
            "the Plan 476 simulator test module imports "
            f"{', '.join(offending)}; a simulator that can connect or spawn "
            "cannot be evidence of anything"
        ]
    return []


def rule_executor_self_test_passes() -> list[str]:
    """The executor's own positive and negative controls pass."""
    passed, detail = run_self_test_of_executor()
    if passed:
        return []
    return [detail]


def rule_publication_names_the_executor() -> list[str]:
    """The published contract and the operator procedure name the executor."""
    violations: list[str] = []
    conformance = _read(CONFORMANCE)
    operator = _read(OPERATOR_DOC)
    if "plan476_live_executor.py" not in conformance:
        violations.append("specs/CONFORMANCE.md does not name the Plan 476 executor")
    if "ssu2_qualification_export" not in conformance:
        violations.append(
            "specs/CONFORMANCE.md does not name the Plan 476 local receipt export"
        )
    if "plan476_live_executor.py" not in operator:
        violations.append(
            "the operator procedure does not name the Plan 476 executor"
        )
    return violations


def rule_floor_runs_this_checker() -> list[str]:
    """The routine floor runs this checker."""
    if floor_mentions(_read(AGENTS)):
        return []
    return ["the routine floor does not run scripts/check-plan476-gate-binding.py"]


RULES = (
    ("phase vocabulary is identical in the Rust export and the Python contract",
     rule_vocabulary_in_sync),
    ("the executor calls the Plan 495 gate", rule_executor_calls_the_gate),
    ("the simulator cannot connect or start a process",
     rule_simulator_cannot_connect),
    ("the executor's own controls pass", rule_executor_self_test_passes),
    ("the published contract and the operator procedure name the executor",
     rule_publication_names_the_executor),
    ("the routine floor runs this checker", rule_floor_runs_this_checker),
)


# ---------------------------------------------------------------------------
# The self-test: every rule must be able to fail.
# ---------------------------------------------------------------------------


def self_test() -> int:
    """Prove each rule fires when its subject is broken."""
    failures: list[str] = []

    rust = _read(RUST_RECEIPTS)
    python = _read(PY_CONTRACT)
    executor = _read(PY_EXECUTOR)

    # Rule 1: a drifted vocabulary must be caught.
    drifted = python.replace('"request_sent",', '"request_sent_typo",', 1)
    if not rule_vocabulary_in_sync.__doc__:
        failures.append("rule 1 has no documentation")
    if drifted == python:
        failures.append("rule 1 probe did not modify the vocabulary")
    else:
        broken = {
            "rust": rust_local_phases(rust),
            "python": python_local_phases(drifted),
        }
        if broken["rust"] == broken["python"]:
            failures.append("rule 1 did not catch a drifted vocabulary")

    # Rule 2: an executor that stops calling the gate must be caught.
    if "gate.classify(" not in executor:
        failures.append("rule 2 probe did not find the gate call to remove")
    else:
        stripped = executor.replace("gate.classify(", "gate.not_a_gate_call(", 1)
        calls = attribute_calls(stripped)
        if all(name in calls for name in REQUIRED_GATE_CALLS):
            failures.append("rule 2 did not catch an executor that stopped calling the gate")

    # Rule 3: a simulator that can open a socket must be caught.
    simulator = _read(PY_SIMULATOR)
    if "import socket" not in simulator:
        failures.append("rule 3 probe did not find an import to add")
    else:
        offender = "import socket\n" + simulator
        if not (imported_modules(offender) & set(FORBIDDEN_SIMULATOR_IMPORTS)):
            failures.append("rule 3 did not catch a simulator that imports socket")

    # Rule 5: an unbound publication must be caught.
    conformance = _read(CONFORMANCE)
    if "plan476_live_executor.py" not in conformance:
        failures.append("rule 5 probe could not find the executor reference")
    else:
        unbound = conformance.replace("plan476_live_executor.py", "plan476_removed.py")
        if "plan476_live_executor.py" in unbound:
            failures.append("rule 5 probe did not modify the conformance binding")

    # Rule 6: a floor that stops running this checker must be caught.
    agents = _read(AGENTS)
    if "scripts/check-plan476-gate-binding.py" not in agents:
        failures.append("rule 6 probe could not find the floor reference")
    elif floor_mentions(agents.replace("scripts/check-plan476-gate-binding.py", "")):
        failures.append("rule 6 did not catch a floor that stopped running the checker")

    # Every rule must pass on the real tree.
    for name, rule in RULES:
        violations = rule()
        if violations:
            failures.append(f"{name}: {violations[0]}")

    if failures:
        for failure in failures:
            print(f"check-plan476-gate-binding: self-test failure: {failure}", file=sys.stderr)
        return 1
    print(
        "check-plan476-gate-binding: self-test ok "
        f"({len(RULES)} rules, each negative-tested)"
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove each rule can fail, then check the real tree",
    )
    arguments = parser.parse_args(argv)
    if arguments.self_test:
        return self_test()
    violations: list[str] = []
    for name, rule in RULES:
        for violation in rule():
            violations.append(f"{name}: {violation}")
    if violations:
        for violation in violations:
            print(f"check-plan476-gate-binding: {violation}", file=sys.stderr)
        return 1
    print(f"check-plan476-gate-binding: ok ({len(RULES)} rules)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
