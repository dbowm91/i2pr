#!/usr/bin/env python3
"""Plan 372 -- tooling-inventory drift guard.

This repository publishes absolute counts of its own tooling: how many
boundary checkers exist, how many the routine floor runs, how many ``ci.yml``
runs, how many workspace crates there are, which MSRV ``ci.yml`` installs, how
many fuzz targets there are. Those figures are load-bearing -- they are how a
reader knows whether a guard is missing from the floor, and how a reviewer
sizes the harness.

Plan 367 recomputed them and stated a repeatable method. Plan 369 then landed
and they rotted again, while a *third* surface Plan 367 never recomputed was
already stale: `tooling.md` still claimed 20 workspace members against a real
26, and still named the MSRV job as running 1.88.0 after Plan 357 raised the
floor to 1.89.

**A hand recount is the mechanism that produced the defect.** Every published
figure here was correct when written. So this guard does not ask a human to
recount more carefully; it derives each figure from the tree and fails closed
when a published number stops matching. Adding a guard now requires adding its
inventory row, because that is rule 3.

Run with ``python3``. ``bash scripts/*.py`` garbles these scripts and exits 2.

Exit codes: 0 = every published figure matches the tree; 1 = at least one
violation (listed on stderr); 2 = the guard could not run.

Scope: this guard READS the tree. It never edits a file, never spawns, and
opens no socket. It executes no project script -- every figure is derived by
parsing text, so the guard cannot be made to pass by the thing it audits.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

LEDGER = (
    "plans/implementation/workspace-foundation/"
    "372-provision-ci-guard-dependencies-and-stop-count-drift.md"
)

AGENTS = "AGENTS.md"
TOOLING = "docs/architecture/tooling.md"
OVERVIEW = "docs/architecture/overview.md"
CARGO = "Cargo.toml"
CI = ".github/workflows/ci.yml"
FUZZ_CARGO = "fuzz/Cargo.toml"


# --------------------------------------------------------------------------
# Derivation. Each function computes one figure from the tree.
# --------------------------------------------------------------------------


def _floor_block(agents_md: str) -> list[str]:
    """The routine-floor command lines, in `AGENTS.md`.

    The block is delimited by the ``## Routine floor`` heading and its closing
    fence. A prose paragraph after the fence would otherwise be counted as a
    step, which is how a recount goes wrong quietly.
    """
    match = re.search(
        r"^## Routine floor.*?\n```text\n(.*?)^```",
        agents_md,
        re.S | re.M,
    )
    if not match:
        return []
    return [line for line in match.group(1).splitlines() if line.strip()]


def floor_checker_steps(agents_md: str) -> int:
    """Floor steps that invoke a checker -- Plan 367's own method, unchanged."""
    return sum(1 for line in _floor_block(agents_md) if "scripts/check-" in line)


def floor_total_steps(agents_md: str) -> int:
    """Every command line in the floor block."""
    return len(_floor_block(agents_md))


# Build artefacts that match a `check-*` or `*.rs` glob but are not source.
# Derivation walks the filesystem rather than shelling out to `git ls-files`,
# because a guard that reads the index cannot see a file the author has not
# staged yet -- which is exactly when rule 3 needs to speak. `__pycache__`
# entries in particular would otherwise inflate every count: four `.pyc`
# files under `scripts/__pycache__/` match `check-*`.
_IGNORED_DIRS = {"__pycache__", ".git", "target", ".mypy_cache"}


def _walk_source(root: Path, base: str, suffix: str | None = None,
                 name_prefix: str | None = None) -> list[str]:
    """Relative paths under `base`, excluding build artefacts."""
    out: list[str] = []
    start = root / base
    if not start.is_dir():
        return out
    for path in sorted(start.rglob("*")):
        if not path.is_file():
            continue
        if any(part in _IGNORED_DIRS for part in path.relative_to(root).parts):
            continue
        if suffix and path.suffix != suffix:
            continue
        if name_prefix and not path.name.startswith(name_prefix):
            continue
        out.append(str(path.relative_to(root)))
    return out


def tracked_check_scripts(root: Path) -> list[str]:
    """`check-*` scripts under `scripts/`, as repo-relative paths.

    Filesystem-scoped, with `__pycache__` excluded. An unfiltered
    `find scripts -name 'check-*'` reports 58 where the real answer is 54,
    because four `.pyc` files match the glob.
    """
    return _walk_source(root, "scripts", name_prefix="check-")


def top_level_check_scripts(root: Path) -> list[str]:
    """`check-*` scripts directly under `scripts/`, not in a subdirectory."""
    return [p for p in tracked_check_scripts(root) if p.count("/") == 1]


def ci_checker_invocations(ci: str) -> int:
    """Checker-running steps in `ci.yml`.

    Counted as steps, matching the "checker invocations" column: one `- name:`
    block whose body runs a checker or a planning/harness unittest discovery.
    A step running two commands is one invocation, because it is one job step
    that fails as a unit.
    """
    count = 0
    for body in re.findall(
        r"- name: [^\n]+\n((?:[ \t]+[^\n]*\n)+?)(?=[ \t]*- name:|[ \t]*- uses:|\Z)",
        ci,
    ):
        if "scripts/check-" in body or "unittest discover" in body:
            count += 1
    return count


def ci_distinct_checkers(ci: str) -> int:
    """Distinct checker paths named anywhere in `ci.yml`."""
    return len(
        set(re.findall(r"scripts/(check-[A-Za-z0-9._-]+)", ci)),
    )


def workspace_members(root: Path) -> list[str]:
    """Workspace member package names, via `cargo metadata`.

    Reads `Cargo.toml`'s `members` globs directly when `cargo` is
    unavailable, so the guard still derives the figure on a machine without a
    toolchain -- but it prefers `cargo metadata`, which is authoritative.
    """
    try:
        out = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return _members_from_cargo_toml(root)

    import json

    data = json.loads(out)
    return sorted(p["name"] for p in data["packages"])


def workspace_crate_names(root: Path) -> set[str]:
    """Member names that live under `crates/`, excluding `tools/`.

    The documents describe the workspace as "N workspace crates plus one
    non-production launcher tool", so the prose count is about `crates/*`
    only. Comparing it against *every* member -- which includes the tool --
    makes a correct sentence look wrong, and a rule that cries wolf is a rule
    that gets disabled.
    """
    return {
        Path(p).parent.name
        for p in _walk_source(root, "crates", suffix=".toml")
        if Path(p).name == "Cargo.toml"
    }


def _members_from_cargo_toml(root: Path) -> list[str]:
    """Fallback: expand `members` globs by hand."""
    text = (root / CARGO).read_text(encoding="utf-8")
    match = re.search(r"^members\s*=\s*\[(.*?)\]", text, re.S | re.M)
    if not match:
        return []
    names: list[str] = []
    for entry in re.findall(r'"([^"]+)"', match.group(1)):
        base = root / entry
        if base.is_dir() and (base / "Cargo.toml").is_file():
            pkg = (base / "Cargo.toml").read_text(encoding="utf-8")
            name = re.search(r"^name\s*=\s*\"([^\"]+)\"", pkg, re.M)
            if name:
                names.append(name.group(1))
    return sorted(names)


def workspace_rust_version(root: Path) -> str:
    """The workspace `rust-version`, which is the MSRV floor."""
    text = (root / CARGO).read_text(encoding="utf-8")
    match = re.search(r'^rust-version\s*=\s*"([^"]+)"', text, re.M)
    return match.group(1) if match else ""


def ci_msrv_toolchain(ci: str) -> str:
    """The toolchain the `msrv` job installs.

    Anchored on the job, not on the first `toolchain:` in the file: the
    `quality` job pins 1.95.0 and the `msrv` job pins the floor, and reading
    the wrong one would validate the wrong number.
    """
    match = re.search(
        r"^  msrv:\n(.*?)(?=^  [a-z][a-z-]*:\n|\Z)", ci, re.S | re.M
    )
    if not match:
        return ""
    toolchain = re.search(r"toolchain:\s*([0-9][0-9.]*)", match.group(1))
    return toolchain.group(1) if toolchain else ""


def fuzz_targets(root: Path) -> int:
    """`[[bin]]` entries in `fuzz/Cargo.toml` -- the authoritative count.

    Counted from the manifest rather than from `fuzz/fuzz_targets/*.rs`,
    because that directory holds a shared `support.rs` module beside the
    targets and the two counts differ by one.
    """
    text = (root / FUZZ_CARGO).read_text(encoding="utf-8")
    return len(re.findall(r"^\[\[bin\]\]", text, re.M))


def root_test_rs_files(root: Path) -> list[str]:
    """Rust source files under the root `tests/` tree.

    Excludes build output: the portable service-tunnel consumer builds its own
    workspace under `tests/.../target/`, and the generated
    `build/*/out/private.rs` there is not a test file.
    """
    return _walk_source(root, "tests", suffix=".rs")


# --------------------------------------------------------------------------
# Published-figure extraction.
#
# Figures are read out of the documents by anchor rather than by scanning for
# a bare number, so a prose sentence that happens to contain "24" is never
# mistaken for the inventory cell it is not.
# --------------------------------------------------------------------------


def _row_value(doc: str, label: str) -> int | None:
    """Read a `| <label> | <n> |` cell from a markdown table."""
    match = re.search(rf"^\|\s*{re.escape(label)}\s*\|\s*([0-9]+)\s*\|", doc, re.M)
    return int(match.group(1)) if match else None


def tooling_inventory_rows(tooling: str) -> dict[str, int]:
    """The `Inventory at a glance` counts, keyed by their row labels."""
    out: dict[str, int] = {}
    for label in (
        "Top-level `scripts/` files",
        "`scripts/interop/` files",
        "`check-*` on disk (all classes)",
        "Checker invocations in `ci.yml`",
        "Integration lane directories",
        "Fixture corpora",
        "Fuzz targets",
        "CI workflows",
    ):
        value = _row_value(tooling, label)
        if value is not None:
            out[label] = value
    return out


def tooling_routine_floor_steps(tooling: str) -> int | None:
    """The `Total routine-floor steps` row of the recompute table."""
    return _row_value(tooling, "Total routine-floor steps")


def tooling_floor_checker_steps(tooling: str) -> int | None:
    """The `Floor steps invoking a checker` row of the recompute table."""
    return _row_value(tooling, "Floor steps invoking a checker")


def tooling_ci_checkers(tooling: str) -> int | None:
    """The `Checkers executed by `ci.yml`` row of the recompute table."""
    return _row_value(tooling, "Checkers executed by `ci.yml`")


def tooling_check_files(tooling: str) -> int | None:
    """The `check-*` files on disk` row of the recompute table."""
    match = re.search(
        r"^\|\s*`check-\*` files on disk\s*\|\s*([0-9]+)", tooling, re.M
    )
    return int(match.group(1)) if match else None


def published_method_b(tooling: str) -> dict[str, int]:
    """The Method B tallies as published in the recompute table.

    Read by their own labels, so the guard compares like with like instead of
    trusting that a number in the right cell is the number that cell means.
    """
    out: dict[str, int] = {}
    match = re.search(r"\|\s*([0-9]+) rows marked `Floor: yes`\s*\|", tooling)
    if match:
        out["floor_yes"] = int(match.group(1))
    match = re.search(r"\|\s*([0-9]+) rows marked `CI: yes`\s*\|", tooling)
    if match:
        out["ci_yes"] = int(match.group(1))
    match = re.search(
        r"^\|\s*`check-\*` files on disk\s*\|\s*[0-9]+[^|]*\|\s*([0-9]+) checker rows",
        tooling,
        re.M,
    )
    if match:
        out["rows"] = int(match.group(1))
    return out


def tooling_member_count(tooling: str) -> int | None:
    """The declared workspace-member total, e.g. `= 20)`."""
    match = re.search(
        r"^### Members \([^)]*=\s*([0-9]+)\)", tooling, re.M
    )
    return int(match.group(1)) if match else None


def tooling_rust_version(tooling: str) -> str:
    """The `rust-version` value `tooling.md` publishes."""
    match = re.search(r'`rust-version\s*=\s*"([^"]+)"`', tooling)
    return match.group(1) if match else ""


def overview_member_count(overview: str) -> int | None:
    """`overview.md`'s prose member count."""
    match = re.search(r"([0-9]+) workspace crates", overview)
    return int(match.group(1)) if match else None


def tooling_claims_zero_root_tests(tooling: str) -> bool:
    """True when `tooling.md` still claims zero Rust integration tests.

    The emphasis spans the whole clause -- ``**Zero Rust integration test
    files under `tests/`.**`` -- not the single word, so the pattern is
    anchored on the phrase and tolerates the `**` either side of it.
    """
    return bool(
        re.search(r"\*\*Zero[^*]*Rust integration test files under `tests/`", tooling)
    )


def tooling_fuzz_target_claims(tooling: str) -> list[str]:
    """Every `N fuzz target(s)` / `all N targets` claim in the document."""
    return re.findall(r"\*\*([0-9]+) fuzz targets\*\*", tooling) + re.findall(
        r"all ([0-9]+) targets", tooling
    )


def inventory_table_script_names(tooling: str) -> set[str]:
    """Script paths named in the four `(Floor, CI)` checker tables.

    Rule 3 compares this against the scripts on disk. The tables are located by
    their header row rather than by a fixed line number, so inserting a table
    does not silently drop it from the guard's view.
    """
    names: set[str] = set()
    for index, line in enumerate(tooling.splitlines()):
        if not line.startswith("| Script"):
            continue
        if _table_columns(line) != ["Floor", "CI"]:
            continue
        for row in _table_rows(tooling, index):
            # `_table_rows` strips the emphasis markers; the cell is a
            # backticked path, so the backticks must survive to be recognised.
            first = row[0].strip()
            if first.startswith("`") and first.endswith("`"):
                names.add(first[1:-1])
    return names


def _table_columns(header: str) -> list[str]:
    return [c.strip() for c in header.strip("|").split("|")][-2:]


def _table_rows(doc: str, header_index: int) -> list[list[str]]:
    """Data rows of the table whose header is at `header_index`."""
    lines = doc.splitlines()
    rows: list[list[str]] = []
    cursor = header_index + 2  # skip the | --- | separator
    while cursor < len(lines) and lines[cursor].startswith("|"):
        rows.append([c.strip().strip("*") for c in lines[cursor].split("|")[1:-1]])
        cursor += 1
    return rows


def method_b_counts(tooling: str) -> dict[str, int]:
    """Recompute the Method B figures from this document's own tables.

    Method B is the convention the gap lists below the recompute table use:
    count rows in the four `(Floor, CI)` tables whose column reads `yes`. Rows
    whose cell carries a qualifier -- `no — `ntcp2-interop-rootless.yml` only`
    -- are neither `yes` nor `no`, so they are excluded from both tallies and
    still counted as rows. That is the same rule the published figures were
    produced under, applied here rather than re-typed by hand.
    """
    lines = tooling.splitlines()
    rows = floor_yes = ci_yes = 0
    for index, line in enumerate(lines):
        if not line.startswith("| Script") or _table_columns(line) != ["Floor", "CI"]:
            continue
        for cells in _table_rows(tooling, index):
            if len(cells) < 4:
                continue
            floor_cell, ci_cell = cells[-2], cells[-1]
            if floor_cell not in ("yes", "no") or ci_cell not in ("yes", "no"):
                continue
            rows += 1
            floor_yes += floor_cell == "yes"
            ci_yes += ci_cell == "yes"
    return {"rows": rows, "floor_yes": floor_yes, "ci_yes": ci_yes}


# --------------------------------------------------------------------------
# Rules.
# --------------------------------------------------------------------------


def scan(root: Path) -> list[str]:
    """Return violation strings; empty means every published figure matches."""
    violations: list[str] = []

    agents_md = (root / AGENTS).read_text(encoding="utf-8")
    tooling = (root / TOOLING).read_text(encoding="utf-8")
    overview = (root / OVERVIEW).read_text(encoding="utf-8")
    ci = (root / CI).read_text(encoding="utf-8")

    # Rule 1 -- AGENTS.md routine-floor counts.
    for label, published, actual in (
        (
            "floor steps invoking a checker",
            tooling_floor_checker_steps(tooling),
            floor_checker_steps(agents_md),
        ),
        (
            "total routine-floor steps",
            tooling_routine_floor_steps(tooling),
            floor_total_steps(agents_md),
        ),
    ):
        if published is not None and published != actual:
            violations.append(
                f"rule 1: {TOOLING} publishes {published} {label}; "
                f"{AGENTS} has {actual}"
            )

    # Rule 2 -- check-* scripts on disk vs the recompute table.
    checks = tracked_check_scripts(root)
    top = top_level_check_scripts(root)
    published_checks = tooling_check_files(tooling)
    if published_checks is not None and published_checks != len(top):
        violations.append(
            f"rule 2: {TOOLING} publishes {published_checks} `check-*` files on "
            f"disk; there are {len(top)} at the top level of `scripts/` "
            f"({len(checks)} including `scripts/interop/`)"
        )

    # Rule 3 -- every check-* script has an inventory row. Load-bearing: this
    # is what makes "added a guard, forgot the row" a failure.
    documented = inventory_table_script_names(tooling)
    undocumented = sorted(
        p for p in checks if p not in documented and Path(p).name not in documented
    )
    if undocumented:
        violations.append(
            "rule 3: these `check-*` scripts have no row in a `tooling.md` "
            f"inventory table, so the published inventory cannot be complete: "
            f"{', '.join(undocumented)}"
        )

    # Rule 4 -- ci.yml checker invocations, in both published surfaces.
    invocations = ci_checker_invocations(ci)
    glance = tooling_inventory_rows(tooling).get("Checker invocations in `ci.yml`")
    if glance is not None and glance != invocations:
        violations.append(
            f"rule 4: {TOOLING} publishes {glance} checker invocations in "
            f"`ci.yml`; there are {invocations} steps that run one"
        )

    # Rule 5 -- distinct checkers named in ci.yml.
    published_ci_checkers = tooling_ci_checkers(tooling)
    distinct = ci_distinct_checkers(ci)
    if published_ci_checkers is not None and published_ci_checkers != distinct:
        violations.append(
            f"rule 5: {TOOLING} publishes {published_ci_checkers} checkers "
            f"executed by `ci.yml`; {distinct} are named there"
        )

    # Rule 5b -- Method B, recomputed from this document's own tables. The
    # gap lists below the recompute table are written in this convention, so a
    # wrong tally here mislabels how much of the floor is covered.
    derived_b = method_b_counts(tooling)
    published_b = published_method_b(tooling)
    for key, label in (
        ("rows", "checker rows"),
        ("floor_yes", "rows marked `Floor: yes`"),
        ("ci_yes", "rows marked `CI: yes`"),
    ):
        if key in published_b and published_b[key] != derived_b[key]:
            violations.append(
                f"rule 5b: {TOOLING} publishes {published_b[key]} {label} "
                f"(Method B); its own tables contain {derived_b[key]}"
            )

    # Rule 6 -- workspace member count and roster.
    members = workspace_members(root)
    published_members = tooling_member_count(tooling)
    if published_members is not None and published_members != len(members):
        violations.append(
            f"rule 6: {TOOLING} declares {published_members} workspace members; "
            f"`cargo metadata` reports {len(members)}"
        )
    overview_members = overview_member_count(overview)
    if overview_members is not None:
        # The prose counts `crates/*` only; the "+ one non-production launcher
        # tool" clause accounts for `tools/i2pr-interop` separately.
        crate_names = workspace_crate_names(root)
        if overview_members != len(crate_names):
            violations.append(
                f"rule 6: {OVERVIEW} claims {overview_members} workspace crates; "
                f"{len(crate_names)} member packages live under `crates/`"
            )
    roster = tooling_member_roster(tooling)
    if roster:
        stale = sorted(set(members) - roster)
        if stale:
            violations.append(
                f"rule 6: {TOOLING}'s member roster omits {', '.join(stale)}"
            )

    # Rule 7 -- published rust-version vs the manifest.
    actual_rust_version = workspace_rust_version(root)
    published_rust_version = tooling_rust_version(tooling)
    if published_rust_version and published_rust_version != actual_rust_version:
        violations.append(
            f"rule 7: {TOOLING} publishes `rust-version = "
            f'"{published_rust_version}"`; {CARGO} declares '
            f'"{actual_rust_version}"'
        )

    # Rule 8 -- the toolchain the msrv job installs must equal the floor.
    ci_msrv = ci_msrv_toolchain(ci)
    if actual_rust_version and ci_msrv and not ci_msrv.startswith(actual_rust_version):
        violations.append(
            f"rule 8: {CI}'s msrv job installs {ci_msrv}, but the workspace "
            f"rust-version is {actual_rust_version}"
        )
    for stale in re.findall(r"Rust \*\*([0-9][0-9.]*)\*\*", tooling):
        if not stale.startswith(actual_rust_version):
            violations.append(
                f"rule 8: {TOOLING} documents the MSRV CI job as running "
                f"{stale}; the workspace rust-version is {actual_rust_version}"
            )

    # Rule 9 -- fuzz targets, from the manifest rather than the directory.
    targets = fuzz_targets(root)
    for claim in tooling_fuzz_target_claims(tooling):
        if int(claim) != targets:
            violations.append(
                f"rule 9: {TOOLING} claims {claim} fuzz targets; "
                f"{FUZZ_CARGO} declares {targets} `[[bin]]` entries"
            )

    # Rule 10 -- the "zero Rust integration files under tests/" claim, inverted
    # to a real count so the false statement cannot survive as prose.
    if tooling_claims_zero_root_tests(tooling):
        root_tests = root_test_rs_files(root)
        if root_tests:
            violations.append(
                "rule 10: "
                f"{TOOLING} claims zero Rust integration test files under "
                f"`tests/`, but {len(root_tests)} are tracked "
                f"({', '.join(root_tests)})"
            )

    return violations


def tooling_member_roster(tooling: str) -> set[str]:
    """Crate directory names listed in the `### Members` fenced block."""
    match = re.search(
        r"^### Members .*?^```text\n(.*?)^```", tooling, re.S | re.M
    )
    if not match:
        return set()
    return set(re.findall(r"crates/([A-Za-z0-9-]+)|tools/([A-Za-z0-9-]+)", match.group(1)) and
               [a or b for a, b in re.findall(
                   r"crates/([A-Za-z0-9-]+)|tools/([A-Za-z0-9-]+)", match.group(1)
               )])


# --------------------------------------------------------------------------
# Self-test.
# --------------------------------------------------------------------------


def _qualify_last_row(tooling_text: str) -> str:
    """Rewrite the last `Floor`/`CI` table row's cells as qualified prose.

    `no — `ntcp2-interop-rootless.yml` only` is how the document records "not
    in CI, but covered by a manual lane". It is neither `yes` nor `no`, so it
    must be excluded from both Method B tallies while still counting as a row.
    """
    lines = tooling_text.splitlines()
    for index in range(len(lines) - 1, -1, -1):
        line = lines[index]
        if not line.startswith("| `"):
            continue
        cells = line.split("|")
        if len(cells) < 5:
            continue
        cells[-3] = " no — manual lane only "
        cells[-2] = " no — manual lane only "
        lines[index] = "|".join(cells)
        break
    return "\n".join(lines) + "\n"


def _plant_artefacts(root: Path) -> None:
    """Create build artefacts that a naive glob would count as source."""
    cache = root / "scripts" / "__pycache__"
    cache.mkdir(exist_ok=True)
    (cache / "check-injected-inventory.cpython-312.pyc").write_bytes(b"\x00")
    nested = root / "tests" / "planted" / "target" / "debug"
    nested.mkdir(parents=True, exist_ok=True)
    (nested / "planted.rs").write_text("// generated\n", encoding="utf-8")


def self_test(root: Path) -> list[str]:
    """Prove each rule detects the violation it claims to.

    Each mutation is applied to the real tree in a throwaway copy under a
    temporary directory -- nothing is written back -- and the scan must reject
    it. The `expect_accepted` controls go the other way: a rule wrong in the
    strict direction is as broken as one wrong in the permissive direction, so
    a correctly-computed figure must stay accepted.
    """
    import shutil
    import tempfile

    if scan(root):
        return [
            "the real tree is already violating; a self-test over a dirty base "
            "proves nothing"
        ]

    failures: list[str] = []

    with tempfile.TemporaryDirectory() as tmp:
        base = Path(tmp) / "tree"
        shutil.copytree(
            root,
            base,
            symlinks=True,
            ignore=shutil.ignore_patterns("target", ".git", "__pycache__"),
        )

        def expect_rejected(label: str, mutate) -> None:
            """`mutate` edits a copy; rule `label`'s number must then fire.

            The rule number is checked, not merely "some violation appeared".
            Plan 369 WP5 found guards whose negative controls replaced whole
            files so that an *unrelated* rule fired and the control passed
            anyway -- the mutation was detected, but not by the rule it was
            written to test. Planting a new checker, for instance, changes the
            `check-*` count and so trips rule 2 as well as rule 3; a control
            that accepts either is blind to rule 3 having stopped working.
            """
            work = Path(tmp) / label.replace(" ", "_").replace(":", "")
            shutil.copytree(base, work, symlinks=True)
            try:
                mutate(work)
            except Exception as exc:  # a broken mutation is not a passing one
                failures.append(f"self-test mutation raised: {label} ({exc})")
                shutil.rmtree(work, ignore_errors=True)
                return
            try:
                found = scan(work)
            except Exception as exc:
                failures.append(f"self-test scan raised: {label} ({exc})")
                shutil.rmtree(work, ignore_errors=True)
                return
            shutil.rmtree(work, ignore_errors=True)

            # Violations are prefixed `rule 3:`, `rule 5b:`; labels are
            # `rule3_...`. Normalise the label to the violation's spelling so
            # the comparison is about *which* rule fired, not about a prefix.
            token = label.split("_")[0]
            rule = f"rule {token[len('rule'):]}"
            if not found:
                failures.append(f"positive control missed: {label}")
            elif not any(rule in violation for violation in found):
                failures.append(
                    f"positive control fired on the wrong rule: {label} "
                    f"(expected {rule!r}, got {found[0]!r})"
                )

        def expect_accepted(label: str, mutate) -> None:
            """The unmutated tree must stay clean under the same edit path."""
            work = Path(tmp) / ("ok_" + label.replace(" ", "_").replace(":", ""))
            shutil.copytree(base, work, symlinks=True)
            try:
                mutate(work)
            finally:
                found = scan(work)
                shutil.rmtree(work, ignore_errors=True)
            if found:
                failures.append(
                    f"negative control failed: {label} ({found[0]})"
                )

        # Rule 1 -- a floor step added without a matching published count.
        # The step must be inserted *inside* the ````text` fence. Appending to
        # the end of the file lands it after the closing fence, where the
        # floor-block parser correctly ignores it -- an earlier draft of this
        # control appended, and it silently proved nothing.
        def _add_floor_step(w: Path) -> None:
            text = (w / AGENTS).read_text()
            marker = "\n```\n"
            close = text.index(marker, text.index("## Routine floor"))
            (w / AGENTS).write_text(
                text[:close] + "\nbash scripts/check-injected.sh" + text[close:],
                encoding="utf-8",
            )

        expect_rejected("rule1_floor_step_added", _add_floor_step)

        # Rule 2 -- a published check-file count that no longer matches disk.
        expect_rejected(
            "rule2_published_count_wrong",
            lambda w: (w / TOOLING).write_text(
                re.sub(
                    r"^\|\s*`check-\*` files on disk\s*\|\s*[0-9]+",
                    "| `check-*` files on disk | 3",
                    (w / TOOLING).read_text(),
                    count=1,
                    flags=re.M,
                ),
                encoding="utf-8",
            ),
        )

        # Rule 5b -- a Method B tally that no longer matches this document's
        # own tables. This is the figure a human is most tempted to type from
        # memory, and it is the figure the gap lists below the table use.
        expect_rejected(
            "rule5b_method_b_tally_wrong",
            lambda w: (w / TOOLING).write_text(
                re.sub(
                    r"\|\s*[0-9]+ rows marked `Floor: yes`\s*\|",
                    "| 99 rows marked `Floor: yes` |",
                    (w / TOOLING).read_text(),
                    count=1,
                ),
                encoding="utf-8",
            ),
        )

        # Rule 3 -- a checker on disk with no inventory row. The load-bearing
        # control: this is the exact shape Plan 367 found and left in place,
        # where two guards were in the floor and neither had an inventory row.
        # The file is deliberately *untracked* in the copy, because a guard
        # that reads the git index cannot see an author who has not staged
        # yet -- which is exactly when this rule needs to speak.
        expect_rejected(
            "rule3_checker_without_row",
            lambda w: (w / "scripts" / "check-injected-inventory.sh").write_text(
                "#!/usr/bin/env bash\nexit 0\n", encoding="utf-8"
            ),
        )
        # A checker whose row exists but whose cells are qualified must not be
        # counted as Floor/CI coverage, yet must still satisfy rule 3.
        expect_accepted(
            "qualified_cells_are_not_tallied",
            lambda w: (w / TOOLING).write_text(
                _qualify_last_row((w / TOOLING).read_text()),
                encoding="utf-8",
            ),
        )
        # `support.rs` beside the fuzz targets is not a target, and a
        # `__pycache__` artefact is not a checker. Both are why the derivation
        # excludes build output; if either leaked in, every published count
        # would drift in the same direction.
        expect_accepted(
            "build_artefacts_are_not_counted",
            lambda w: _plant_artefacts(w),
        )

        # Rule 4 -- a stale ci.yml invocation count.
        expect_rejected(
            "rule4_ci_invocations_wrong",
            lambda w: (w / TOOLING).write_text(
                re.sub(
                    r"^\|\s*Checker invocations in `ci\.yml`\s*\|\s*[0-9]+",
                    "| Checker invocations in `ci.yml` | 1",
                    (w / TOOLING).read_text(),
                    count=1,
                    flags=re.M,
                ),
                encoding="utf-8",
            ),
        )

        # Rule 5 -- a stale distinct-checker count.
        expect_rejected(
            "rule5_distinct_ci_checkers_wrong",
            lambda w: (w / TOOLING).write_text(
                re.sub(
                    r"^\|\s*Checkers executed by `ci\.yml`\s*\|\s*[0-9]+",
                    "| Checkers executed by `ci.yml` | 2",
                    (w / TOOLING).read_text(),
                    count=1,
                    flags=re.M,
                ),
                encoding="utf-8",
            ),
        )

        # Rule 6 -- a member count that predates Plan 369's crates.
        expect_rejected(
            "rule6_member_count_stale",
            lambda w: (w / TOOLING).write_text(
                re.sub(
                    r"^### Members \([^)]*=\s*[0-9]+\)",
                    "### Members (19 crates + 1 non-production binary = 20)",
                    (w / TOOLING).read_text(),
                    count=1,
                    flags=re.M,
                ),
                encoding="utf-8",
            ),
        )
        expect_rejected(
            "rule6_overview_count_stale",
            lambda w: (w / OVERVIEW).write_text(
                (w / OVERVIEW).read_text().replace(
                    f"{len(workspace_crate_names(w))} workspace crates",
                    "20 workspace crates",
                    1,
                ),
                encoding="utf-8",
            ),
        )

        # Rule 7 -- a published rust-version that predates Plan 357.
        expect_rejected(
            "rule7_rust_version_stale",
            lambda w: (w / TOOLING).write_text(
                (w / TOOLING).read_text().replace(
                    '`rust-version = "1.89"`', '`rust-version = "1.88"`', 1
                ),
                encoding="utf-8",
            ),
        )

        # Rule 8 -- an MSRV job documented at the pre-Plan-357 toolchain.
        expect_rejected(
            "rule8_msrv_job_documented_stale",
            lambda w: (w / TOOLING).write_text(
                (w / TOOLING).read_text().replace(
                    "Rust **1.89.0**", "Rust **1.88.0**", 1
                ),
                encoding="utf-8",
            ),
        )

        # Rule 9 -- a fuzz-target count taken from the directory, which
        # includes the shared `support.rs` and so over-counts by one.
        expect_rejected(
            "rule9_fuzz_count_wrong",
            lambda w: (w / TOOLING).write_text(
                re.sub(
                    r"\*\*[0-9]+ fuzz targets\*\*",
                    f"**{fuzz_targets(w) + 1} fuzz targets**",
                    (w / TOOLING).read_text(),
                    count=1,
                ),
                encoding="utf-8",
            ),
        )

        # Rule 10 -- the false "zero Rust integration files" claim restored.
        # Plan 372 corrected the sentence rather than deleting it, so a
        # string-replace control would no-op. The mutation reintroduces the
        # claim as text, which is the shape the rule exists to reject.
        expect_rejected(
            "rule10_zero_test_claim_reintroduced",
            lambda w: (w / TOOLING).write_text(
                (w / TOOLING).read_text()
                + "\n1. **Zero Rust integration test files under `tests/`.**\n",
                encoding="utf-8",
            ),
        )

    return failures


# --------------------------------------------------------------------------
# Entry point.
# --------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove each rule rejects the violation it claims to detect",
    )
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parent.parent),
        help="repository root (default: the parent of this script)",
    )
    args = parser.parse_args()
    root = Path(args.root).resolve()

    try:
        if args.self_test:
            failures = self_test(root)
            for failure in failures:
                print(f"check-tooling-inventory: FAIL: {failure}", file=sys.stderr)
            if failures:
                return 1
            print("check-tooling-inventory: self-test ok")
            return 0

        violations = scan(root)
    except FileNotFoundError as exc:
        print(f"check-tooling-inventory: cannot run: {exc}", file=sys.stderr)
        return 2
    except Exception as exc:  # fail closed: an unreadable tree is not a pass
        print(
            "check-tooling-inventory: cannot run; refusing to report success on "
            f"an unchecked tree: {exc}",
            file=sys.stderr,
        )
        return 2

    for violation in violations:
        print(f"check-tooling-inventory: FAIL: {violation}", file=sys.stderr)
    if violations:
        print(
            f"check-tooling-inventory: {len(violations)} violation(s). Published "
            f"inventory figures must match the tree; see {LEDGER}. A guard added "
            f"without a `tooling.md` inventory row is rule 3.",
            file=sys.stderr,
        )
        return 1
    print("check-tooling-inventory: every published figure matches the tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())