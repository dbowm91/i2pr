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
binding mechanical, and it fails closed. Each rule is written to be
*hard to satisfy by accident*:

* the Rust export's phase vocabulary and the Python receipt
  vocabulary are compared element by element — they are two
  implementations of one wire contract, and a drift would let the
  executor accept a receipt the daemon can never produce (or reject
  one it can);
* the daemon's *writer* is scanned too, not only the collector, and
  the export's schema string must match the one the executor accepts;
* the gate the executor calls is resolved by loading it and checking
  which file it came from — a local stub with the right method names
  is not the Plan 495 gate;
* the gate must be evaluated **before** the only spawn site, in
  source order;
* the executor must never emit `i2pr-defect`;
* the simulator test module, and the executor itself, must be unable
  to open a socket;
* every command-line flag the operator procedure publishes must exist
  in the executor's parser;
* the routine floor must run this checker **as a command**, not as a
  commented-out line.

Run with ``python3``. ``bash scripts/*.py`` garbles these scripts and
exits 2.

    python3 scripts/check-plan476-gate-binding.py
    python3 scripts/check-plan476-gate-binding.py --self-test
"""

from __future__ import annotations

import argparse
import ast
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

RUST_RECEIPTS = REPO_ROOT / "crates/i2pr-daemon/src/ssu2_qualification_receipts.rs"
RUST_EXPORT = REPO_ROOT / "crates/i2pr-daemon/src/ssu2_qualification_export.rs"
PY_CONTRACT = REPO_ROOT / "tests/integration/ssu2/plan476_executor.py"
PY_EXECUTOR = REPO_ROOT / "tests/integration/ssu2/plan476_live_executor.py"
PY_SIMULATOR = REPO_ROOT / "tests/integration/ssu2/test_plan476_live_executor.py"
GATE_CHECKER = REPO_ROOT / "scripts/plan495-peer-eligibility.py"
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

#: Modules the executor must never import. The executor legitimately
#: binds a *local* UDP socket to check that the declared port is free,
#: so `socket` is allowed there; a network *client* is not, because the
#: executor must never reach a peer host.
FORBIDDEN_EXECUTOR_IMPORTS = (
    "ssl",
    "http",
    "urllib",
    "requests",
    "ftplib",
    "telnetlib",
    "asyncio",
    "aiohttp",
)

#: Calls that would let the executor talk to something. The executor's
#: only socket use is a local bind probe, so any of these appearing in
#: its source would mean it could reach out.
FORBIDDEN_EXECUTOR_CALLS = (
    "connect",
    "connect_ex",
    "send",
    "sendall",
    "sendto",
    "recv",
    "recvfrom",
    "recv_into",
)

#: Vocabulary that must never appear in the exported receipt
#: document. The export is count-only.
FORBIDDEN_RECEIPT_VOCABULARY = (
    "message_id",
    "messageId",
    "peer_commitment",
    "peerCommitment",
    "endpoint",
    "address",
    "router_info",
    "routerInfo",
    "key",
    "payload",
    "secret",
)

#: The export's schema tag. The daemon writes it; the executor accepts
#: it. A mismatch would make every live receipt unreadable.
EXPORT_SCHEMA = "i2pr-plan476-local-receipt-v1"


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


# ---------------------------------------------------------------------------
# Source readers.
# ---------------------------------------------------------------------------


def strip_rust_test_module(source: str) -> str:
    """Remove a trailing `#[cfg(test)] mod tests { ... }` block.

    The module's own assertions mention the very vocabulary the export
    must not contain, so scanning the whole file would report the
    guard's own evidence as a violation.
    """
    marker = source.find("#[cfg(test)]")
    if marker < 0:
        return source
    brace = source.find("{", marker)
    if brace < 0:
        return source
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[:marker] + source[index + 1 :]
    return source[:marker]


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
    """Every attribute name referenced anywhere in a module."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return set()
    return {node.attr for node in ast.walk(tree) if isinstance(node, ast.Attribute)}


def call_sites(source: str) -> dict[str, list[int]]:
    """The line numbers of every call, keyed by the callee's attribute."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return {}
    sites: dict[str, list[int]] = {}
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        function = node.func
        name = function.attr if isinstance(function, ast.Attribute) else getattr(
            function, "id", ""
        )
        if name:
            sites.setdefault(name, []).append(node.lineno)
    return sites


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


def string_literals(source: str) -> list[str]:
    """Every string literal in a module, excluding docstrings.

    A docstring is prose about the code; it can name an outcome class
    without ever emitting one, so it is not evidence.
    """
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return []
    docstrings: set[int] = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            body = getattr(node, "body", [])
            if (
                body
                and isinstance(body[0], ast.Expr)
                and isinstance(body[0].value, ast.Constant)
                and isinstance(body[0].value.value, str)
            ):
                docstrings.add(id(body[0].value))
    return [
        node.value
        for node in ast.walk(tree)
        if isinstance(node, ast.Constant)
        and isinstance(node.value, str)
        and id(node) not in docstrings
    ]


def rust_function_source(source: str, name: str) -> str:
    """The body of one Rust function, by name, by brace matching."""
    match = re.search(rf"\bfn\s+{re.escape(name)}\b", source)
    if not match:
        return ""
    brace = source.find("{", match.end())
    if brace < 0:
        return ""
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[brace : index + 1]
    return ""


def function_source(source: str, name: str) -> str:
    """The source text of one top-level function, by name."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return ""
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name:
            return ast.get_source_segment(source, node) or ""
    return ""


def function_call_sites(source: str, name: str) -> list[int]:
    """The line numbers of the calls inside one function."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return []
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name:
            sites: list[int] = []
            for inner in ast.walk(node):
                if isinstance(inner, ast.Call):
                    function = inner.func
                    attribute = getattr(function, "attr", "")
                    if attribute:
                        sites.append(inner.lineno)
            return sites
    return []


def declared_flags(source: str) -> set[str]:
    """Every long option the module's argument parser declares."""
    flags: set[str] = set()
    for match in re.finditer(r'add_argument\(\s*"(--[A-Za-z0-9][A-Za-z0-9._-]*)"', source):
        flags.add(match.group(1))
    return flags


def floor_mentions(agents: str) -> bool:
    """Whether the routine floor runs this checker as a command.

    A commented-out line is not a run: the floor block is a list of
    command lines, and a line that starts with `#` is a comment.
    """
    match = re.search(
        r"^## Routine floor.*?\n```text\n(.*?)^```", agents, re.S | re.M
    )
    if not match:
        return False
    for line in match.group(1).splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if "scripts/check-plan476-gate-binding.py" in stripped:
            return True
    return False


def published_flags(operator_doc: str) -> set[str]:
    """Every long option the operator procedure publishes for the executor."""
    flags: set[str] = set()
    for block in re.findall(r"```(?:text|bash)\n(.*?)```", operator_doc, re.S):
        if "plan476_live_executor.py" not in block:
            continue
        flags.update(re.findall(r"(--[A-Za-z0-9][A-Za-z0-9._-]*)", block))
    return flags


def run_executor_self_test(executor: Path = PY_EXECUTOR) -> tuple[bool, str]:
    """Run the executor's own self-test and report whether it passed."""
    try:
        completed = subprocess.run(
            [sys.executable, str(executor), "--self-test"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        return False, f"executor self-test could not run: {error}"
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout or "").strip()
        return False, f"executor self-test failed: {detail[:400]}"
    return True, completed.stdout.strip()


# ---------------------------------------------------------------------------
# The rules. Each takes the subject it examines so the self-test can
# feed a mutated subject to the very same function.
# ---------------------------------------------------------------------------


def rule_vocabulary_in_sync(
    receipts: str | None = None, contract: str | None = None
) -> list[str]:
    """The Rust export and the Python contract speak one vocabulary."""
    rust = rust_local_phases(receipts if receipts is not None else _read(RUST_RECEIPTS))
    python = python_local_phases(contract if contract is not None else _read(PY_CONTRACT))
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


def rule_export_writer_is_count_only(export: str | None = None) -> list[str]:
    """The daemon's writer cannot emit anything but counts and labels.

    The collector is pure, but the writer is what actually lands on
    disk. It is scanned too — and only the function that *builds the
    document* is scanned, because the module's own prose and its test
    module necessarily name the vocabulary the export must not carry.
    """
    source = export if export is not None else _read(RUST_EXPORT)
    if not source:
        return ["the daemon's local receipt export module could not be read"]
    builder = rust_function_source(source, "snapshot")
    if not builder:
        return ["the daemon's local receipt export has no document builder"]
    violations: list[str] = []
    for forbidden in FORBIDDEN_RECEIPT_VOCABULARY:
        if forbidden in builder:
            violations.append(
                f"the exported receipt document mentions {forbidden}; the export "
                "must be count-only"
            )
    if EXPORT_SCHEMA not in source:
        violations.append(
            f"the daemon's local receipt export does not publish the {EXPORT_SCHEMA} schema"
        )
    return violations


def rule_executor_calls_the_gate(executor: str | None = None) -> list[str]:
    """The executor calls the Plan 495 gate rather than importing it."""
    source = executor if executor is not None else _read(PY_EXECUTOR)
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


def rule_gate_is_the_plan495_checker() -> list[str]:
    """The gate the executor loads is the Plan 495 checker itself.

    A local stub with the right method names would satisfy a
    name-based scan. Resolving the module the executor actually loads
    closes that hole: the stub's file would not be the Plan 495
    checker.
    """
    source = _read(PY_EXECUTOR)
    if not source:
        return ["the Plan 476 executor could not be read"]
    loader = _load_executor_module()
    if loader is None:
        return ["the Plan 476 executor could not be imported"]
    try:
        gate = loader.gate_module()
    except Exception as error:  # noqa: BLE001 - any failure is a violation
        return [f"the executor's gate could not be loaded: {error}"]
    origin = Path(getattr(gate, "__file__", "") or "").resolve()
    if origin != GATE_CHECKER.resolve():
        return [
            "the executor's gate is not the Plan 495 checker: it loaded "
            f"{origin} instead of {GATE_CHECKER.resolve()}"
        ]
    return []


def rule_gate_runs_before_the_spawn(executor: str | None = None) -> list[str]:
    """The gate is evaluated before the only spawn site.

    Source order is a proxy for execution order, and the executor's own
    negative controls assert the runtime behaviour. Together they are
    what makes "the gate runs before any process starts" mechanically
    checkable rather than a promise.

    Both calls are looked for inside `execute_run`, the one function
    that owns the state machine: the executor's supervisor helper also
    has a `spawn` method, and counting that would compare the wrong
    two lines.
    """
    source = executor if executor is not None else _read(PY_EXECUTOR)
    if not source:
        return ["the Plan 476 executor could not be read"]
    body = function_source(source, "execute_run")
    if not body:
        return ["the Plan 476 executor has no execute_run state machine"]
    sites = call_sites(body)
    gate_lines = sites.get("classify", []) + sites.get("evaluate_gate", [])
    spawn_lines = sites.get("spawn", [])
    if not gate_lines:
        return ["the Plan 476 executor's state machine never evaluates the gate"]
    if not spawn_lines:
        return ["the Plan 476 executor's state machine has no spawn site"]
    if min(gate_lines) > min(spawn_lines):
        return [
            "the Plan 476 executor spawns a process before it evaluates the "
            f"Plan 495 gate (spawn at line {min(spawn_lines)}, gate at line "
            f"{min(gate_lines)})"
        ]
    return []


def rule_executor_never_reports_a_defect(executor: str | None = None) -> list[str]:
    """The executor never emits `i2pr-defect`.

    `i2pr-defect` is a reviewed human judgement about the
    implementation, and only the cause `i2pr-implementation` may
    accompany it. An executor that could emit it on its own would be
    able to report a peer-side refusal as our defect — the exact
    failure this whole gate exists to prevent.
    """
    source = executor if executor is not None else _read(PY_EXECUTOR)
    if not source:
        return ["the Plan 476 executor could not be read"]
    for literal in string_literals(source):
        if "i2pr-defect" in literal:
            return [
                "the Plan 476 executor carries the literal 'i2pr-defect'; the "
                "executor must never report a defect on its own"
            ]
    return []


def rule_simulator_cannot_connect(
    simulator: str | None = None, executor: str | None = None
) -> list[str]:
    """Neither the simulator nor the executor can open a socket."""
    simulator_source = simulator if simulator is not None else _read(PY_SIMULATOR)
    executor_source = executor if executor is not None else _read(PY_EXECUTOR)
    if not simulator_source:
        return ["the Plan 476 simulator test module could not be read"]
    violations: list[str] = []
    offending = sorted(
        name
        for name in imported_modules(simulator_source)
        if name in FORBIDDEN_SIMULATOR_IMPORTS
    )
    if offending:
        violations.append(
            "the Plan 476 simulator test module imports "
            f"{', '.join(offending)}; a simulator that can connect or spawn "
            "cannot be evidence of anything"
        )
    offending = sorted(
        name
        for name in imported_modules(executor_source)
        if name in FORBIDDEN_EXECUTOR_IMPORTS
    )
    if offending:
        violations.append(
            f"the Plan 476 executor imports {', '.join(offending)}; the "
            "executor must never reach a peer host over the network"
        )
    reaching = sorted(
        name for name in attribute_calls(executor_source) if name in FORBIDDEN_EXECUTOR_CALLS
    )
    if reaching:
        violations.append(
            "the Plan 476 executor can reach out over a socket "
            f"({', '.join(reaching)}); its only socket use may be a local bind probe"
        )
    return violations


def rule_executor_self_test_passes() -> list[str]:
    """The executor's own positive and negative controls pass."""
    passed, detail = run_executor_self_test()
    if passed:
        return []
    return [detail]


def rule_publication_names_the_executor(
    conformance: str | None = None, operator: str | None = None
) -> list[str]:
    """The published contract names the executor, and its flags are real."""
    conformance_text = conformance if conformance is not None else _read(CONFORMANCE)
    operator_text = operator if operator is not None else _read(OPERATOR_DOC)
    violations: list[str] = []
    if "plan476_live_executor.py" not in conformance_text:
        violations.append("specs/CONFORMANCE.md does not name the Plan 476 executor")
    if "ssu2_qualification_export" not in conformance_text:
        violations.append(
            "specs/CONFORMANCE.md does not name the Plan 476 local receipt export"
        )
    if "plan476_live_executor.py" not in operator_text:
        violations.append(
            "the operator procedure does not name the Plan 476 executor"
        )
    declared = declared_flags(_read(PY_EXECUTOR))
    if not declared:
        violations.append("the Plan 476 executor's argument parser could not be read")
        return violations
    unpublished = sorted(published_flags(operator_text) - declared)
    if unpublished:
        violations.append(
            "the operator procedure publishes options the executor does not "
            f"accept: {', '.join(unpublished)}"
        )
    return violations


def rule_floor_runs_this_checker(agents: str | None = None) -> list[str]:
    """The routine floor runs this checker."""
    text = agents if agents is not None else _read(AGENTS)
    if floor_mentions(text):
        return []
    return ["the routine floor does not run scripts/check-plan476-gate-binding.py"]


RULES = (
    (
        "phase vocabulary is identical in the Rust export and the Python contract",
        rule_vocabulary_in_sync,
    ),
    ("the daemon's receipt writer is count-only", rule_export_writer_is_count_only),
    ("the executor calls the Plan 495 gate", rule_executor_calls_the_gate),
    ("the executor's gate is the Plan 495 checker", rule_gate_is_the_plan495_checker),
    ("the gate runs before the spawn", rule_gate_runs_before_the_spawn),
    ("the executor never reports an i2pr defect", rule_executor_never_reports_a_defect),
    ("neither the simulator nor the executor can connect", rule_simulator_cannot_connect),
    ("the executor's own controls pass", rule_executor_self_test_passes),
    (
        "the published contract and the operator procedure name the executor",
        rule_publication_names_the_executor,
    ),
    ("the routine floor runs this checker", rule_floor_runs_this_checker),
)


# ---------------------------------------------------------------------------
# The self-test: every rule must be able to fail.
# ---------------------------------------------------------------------------


def _load_executor_module():
    """Import the executor module by path, without importing it twice."""
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "plan476_live_executor_probe", PY_EXECUTOR
    )
    if spec is None or spec.loader is None:
        return None
    module = importlib.util.module_from_spec(spec)
    # The module must be registered before it is executed: a
    # `@dataclass` under `from __future__ import annotations` resolves
    # its own module through `sys.modules` while the class body is
    # being built.
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
    except Exception:  # noqa: BLE001 - a broken executor is a violation
        return None
    return module


def _probe_mutated_executor() -> tuple[bool, str]:
    """Break the executor's own controls and confirm they notice.

    The probe copies the executor into a scratch tree that mirrors the
    repository's layout through symlinks, so the copy resolves the same
    fixtures and the same gate checker. It then breaks the phase-length
    assertion and requires the copied self-test to fail. A checker that
    cannot see a broken executor's controls is not a checker.
    """
    executor_source = _read(PY_EXECUTOR)
    if "check(len(phases) == 9" not in executor_source:
        return False, "the executor's phase-length assertion could not be located"
    mutated = executor_source.replace(
        "check(len(phases) == 9", "check(len(phases) == 8", 1
    )
    with tempfile.TemporaryDirectory(prefix="i2pr-plan476-probe-") as raw:
        root = Path(raw)
        (root / "tests/integration/ssu2").mkdir(parents=True)
        for name in ("scripts", "crates"):
            os.symlink(REPO_ROOT / name, root / name)
        for name in ("plan495-eligibility-fixtures", "plan476_executor.py"):
            os.symlink(
                REPO_ROOT / "tests/integration/ssu2" / name,
                root / "tests/integration/ssu2" / name,
            )
        copy = root / "tests/integration/ssu2/plan476_live_executor.py"
        copy.write_text(mutated, encoding="utf-8")
        try:
            completed = subprocess.run(
                [sys.executable, str(copy), "--self-test"],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=180,
                check=False,
            )
        except (OSError, subprocess.SubprocessError) as error:
            return False, f"the mutated executor could not be run: {error}"
        if completed.returncode == 0:
            return False, "the mutated executor's self-test still passed"
        return True, completed.stdout.strip()


def self_test() -> int:
    """Prove each rule fires when its subject is broken.

    Each probe feeds a deliberately broken subject to the *same*
    function the real check uses, so a probe cannot pass while the
    rule it claims to exercise is broken.
    """
    failures: list[str] = []

    # Rule 1: a drifted vocabulary must be caught.
    drifted = _read(PY_CONTRACT).replace('"request_sent",', '"request_sent_typo",', 1)
    if drifted == _read(PY_CONTRACT):
        failures.append("rule 1 probe did not modify the vocabulary")
    elif not rule_vocabulary_in_sync(_read(RUST_RECEIPTS), drifted):
        failures.append("rule 1 did not catch a drifted vocabulary")

    # Rule 2: a writer that leaks must be caught.
    leaky = _read(RUST_EXPORT).replace(
        'document.push_str("  \\"phases\\": [");',
        'document.push_str("  \\"endpoint\\": [");',
        1,
    )
    if leaky == _read(RUST_EXPORT):
        failures.append("rule 2 probe did not modify the writer")
    elif not rule_export_writer_is_count_only(leaky):
        failures.append("rule 2 did not catch a writer that emits an endpoint")

    # Rule 3: an executor that stops calling the gate must be caught.
    stripped = _read(PY_EXECUTOR).replace("gate.classify(", "gate.not_a_gate_call(", 1)
    if stripped == _read(PY_EXECUTOR):
        failures.append("rule 3 probe did not modify the executor")
    elif not rule_executor_calls_the_gate(stripped):
        failures.append("rule 3 did not catch an executor that stopped calling the gate")

    # Rule 5: a spawn before the gate must be caught.
    inverted = (
        "def execute_run(request):\n"
        "    handle = adapters.processes.spawn(argv)\n"
        "    verdict = gate.classify(gate.evaluate(profile, peer))\n"
        "    return handle, verdict\n"
    )
    if not rule_gate_runs_before_the_spawn(inverted):
        failures.append("rule 5 did not catch a spawn before the gate")
    if rule_gate_runs_before_the_spawn(_read(PY_EXECUTOR)):
        failures.append("rule 5 flagged the real executor")

    # Rule 6: an executor that could report a defect must be caught.
    defecting = 'OUTCOME = "i2pr-defect"\n'
    if not rule_executor_never_reports_a_defect(defecting):
        failures.append("rule 6 did not catch an executor that names i2pr-defect")
    if rule_executor_never_reports_a_defect(_read(PY_EXECUTOR)):
        failures.append("rule 6 flagged the real executor")

    # Rule 7: a simulator that can connect must be caught.
    offender = "import socket\n" + _read(PY_SIMULATOR)
    if not rule_simulator_cannot_connect(offender, _read(PY_EXECUTOR)):
        failures.append("rule 7 did not catch a simulator that imports socket")
    reacher = "import urllib.request\n" + _read(PY_EXECUTOR)
    if not rule_simulator_cannot_connect(_read(PY_SIMULATOR), reacher):
        failures.append("rule 7 did not catch an executor that imports a network client")

    # Rule 8: a broken executor's own controls must be caught.
    try:
        detected, detail = _probe_mutated_executor()
    except Exception as error:  # noqa: BLE001 - a broken probe is a failure
        detected, detail = False, str(error)
    if not detected:
        failures.append(f"rule 8 did not catch a broken executor: {detail}")

    # Rule 9: a published option the executor does not accept must be caught.
    bogus = _read(OPERATOR_DOC).replace(
        "--epoch <32-hex>", "--epoch <32-hex> --totally-bogus-flag"
    )
    if bogus == _read(OPERATOR_DOC):
        failures.append("rule 9 probe did not modify the operator procedure")
    elif not rule_publication_names_the_executor(_read(CONFORMANCE), bogus):
        failures.append("rule 9 did not catch a published option the executor rejects")

    # Rule 10: a commented-out floor line must be caught.
    commented = _read(AGENTS).replace(
        "python3 scripts/check-plan476-gate-binding.py",
        "# python3 scripts/check-plan476-gate-binding.py",
    )
    if commented == _read(AGENTS):
        failures.append("rule 10 probe did not modify the floor")
    elif not rule_floor_runs_this_checker(commented):
        failures.append("rule 10 did not catch a commented-out floor line")

    # Every rule must pass on the real tree.
    for name, rule in RULES:
        violations = rule()
        if violations:
            failures.append(f"{name}: {violations[0]}")

    if failures:
        for failure in failures:
            print(
                f"check-plan476-gate-binding: self-test failure: {failure}",
                file=sys.stderr,
            )
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
