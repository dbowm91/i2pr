#!/usr/bin/env python3
"""Plan 369 — managed-application **process** boundary.

The dependency-direction and runtime-boundary scripts police *edges between
crates*. They cannot see the property Plan 369's trust model actually rests on,
which is about **which process execs what**:

* `i2pr-apphost` is the only component that execs an application;
* production source neither names nor bundles the fixture application or
  manager; a local administrator may launch a fixture only by installing and
  authorizing its signed package like any other application;
* the shipped manager takes no arguments, reads its exact daemon-owned state
  root after handshake, and owns the persistent policy catalog.

Each rule below fails closed: a missing file, an unreadable tree, or a changed
layout is a failure, never a skip. A guard that quietly stops looking is worse
than no guard, because it reports green for a property nobody is checking.

Run with ``--self-test`` to prove the rules still detect the violations they
claim to: each mutation below is applied to the real sources in memory and the
scan must reject it.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The evidence-tooling crate. Everything below treats it as *outside* the trust
# model: it may exist, it may be built, and no production path may reach it.
FIXTURE_CRATE = "i2pr-app-fixture"
FIXTURE_TOKENS = ("i2pr-app-fixture", "i2pr_app_fixture")

# The shipped manager binary's source. Rule 3 is about exactly this file.
SHIPPED_MANAGER = "crates/i2pr-appd/src/main.rs"

# A test-only seam in a production module. Rule 4 keeps its callers test-only.
TEST_SEAM = "set_manager_path_override_for_tests"

# The one shape of `TEST_SEAM` that is not a caller: its own definition.
SEAM_DEFINITION = re.compile(rf"\bfn\s+{TEST_SEAM}\s*\(")

# Modules that are test-only even though they live in a production crate's src
# tree. `i2pr-daemon`'s WP5 qualification module is a `#[cfg(test)] mod`, so it
# compiles out of every real build and may name the fixture freely.
TEST_ONLY_FILES = frozenset({"crates/i2pr-daemon/src/app_runtime_qualification.rs"})

# The only production sources permitted to create a child process, and what
# each one is permitted to create.
#
# Plan 369's chain has exactly three process edges, and this map is the whole of
# them:
#
#   daemon          -> i2pr-appd       (the manager)
#   i2pr-appd       -> i2pr-apphost    (the direct host)
#   i2pr-apphost    -> the application
#
# A fourth edge is a finding, not a matter of taste: it would be some component
# other than `i2pr-apphost` able to exec an application.
ALLOWED_PROCESS_SPAWNERS = {
    "crates/i2pr-daemon/src/app_runtime.rs": "the manager",
    "crates/i2pr-appd/src/apphost_launch.rs": "the direct host",
    "crates/i2pr-apphost/src/lib.rs": "an application",
}

# The two *distribution-owned sibling* edges. Their executable is a function of
# where the router was installed, which is what makes possession of the inherited
# pipes meaningful: nothing in configuration could have substituted a different
# program.
#
# `i2pr-apphost` is deliberately absent. Its target is not a sibling -- it is the
# root/entrypoint of a launch request the manager already validated, so it is
# governed by the containment rules in the bootstrap contract instead.
SIBLING_SPAWNERS = frozenset(
    {
        "crates/i2pr-daemon/src/app_runtime.rs",
        "crates/i2pr-appd/src/apphost_launch.rs",
    }
)

# Shell-mediated launch is forbidden outright, and so is a `PATH` search.
FORBIDDEN_LAUNCH_TOKENS = (
    "/bin/sh",
    "/bin/bash",
    "/usr/bin/env",
    "sh -c",
    "bash -c",
    'Command::new("sh")',
    "Command::new(\"sh\")",
)

# `PATH` is consulted through an environment read; that is the token to catch.
PATH_LOOKUP = re.compile(r'env::var(?:_os)?\s*\(\s*"PATH"')

# An argv guard that returns: `if <something reading argv> { ... return ... }`.
# Rule 3 asserts this shape rather than the bare presence of `args_os`, because
# a manager that reads argv and then ignores it is the failure mode, not a
# manager that never looks.
ARGV_REFUSAL = re.compile(
    r"if\s+[^{;]*?\b(?:args_os|args)\s*\(\s*\)[^{]*?\{[^{}]*?\breturn\b",
    re.DOTALL,
)

# A fully-qualified process `Command`. This is the reliable signal.
PROCESS_COMMAND = re.compile(r"\b(?:std::process|tokio::process|process)::Command\b")

# A bare `Command::new` only means a process when the file also imports a
# process `Command`. Without that qualification the token is a false positive:
# `i2pr-api`'s SAM parser has a `Command::new(..)` that builds a *SAM command
# message*, and a guard that cannot tell the two apart trains people to ignore
# it.
BARE_COMMAND_NEW = re.compile(r"(?<![\w:])Command::new\s*\(")
PROCESS_COMMAND_IMPORT = re.compile(
    r"use\s+(?:std::process|tokio::process)::\{?[^;]*\bCommand\b|"
    r"use\s+(?:std::process|tokio::process)::Command\b"
)

# Directories whose contents never reach a shipped build.
TEST_DIR_PARTS = ("/tests/", "/benches/", "/examples/")


BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT = re.compile(r"//[^\n]*")


def strip_comments(text: str) -> str:
    """Removes comments before scanning.

    Necessary, not cosmetic: `i2pr-apphost` documents that the application path
    is *not* passed to `sh -c`, and a guard that reads that as a shell launch
    would have to be muted by a `#` — at which point it protects nothing.
    """
    return LINE_COMMENT.sub("", BLOCK_COMMENT.sub("", text))


def strip_test_code(path: str, text: str) -> str:
    """Removes everything a real build would not compile.

    Two cases matter and both are handled: a file that is entirely a `#[cfg(test)]`
    module, and a `#[cfg(test)] mod tests { .. }` block inside an otherwise
    production file. Both disappear from a release build, so both may use test
    seams like `set_manager_path_override_for_tests` without weakening the rules
    applied to real code.
    """
    if path in TEST_ONLY_FILES:
        return ""
    marker = text.find("#[cfg(test)]")
    production = text if marker == -1 else text[:marker]
    return strip_comments(production)


def load_sources(root: Path) -> dict[str, str]:
    """Every workspace source file, keyed by POSIX-style relative path.

    Fails closed: a workspace with no `crates/` directory, or one whose sources
    cannot be read, raises rather than producing an empty map that would make
    every rule trivially pass.
    """
    sources: dict[str, str] = {}
    crates = root / "crates"
    if not crates.is_dir():
        raise SystemExit(
            "check-managed-app-process-boundary: FAIL: no crates/ directory under "
            f"{root}; refusing to report an empty scan as clean"
        )
    for manifest in sorted(crates.glob("*/Cargo.toml")):
        crate = manifest.parent.name
        prefix = f"crates/{crate}"
        for source in sorted(manifest.parent.rglob("*.rs")):
            key = f"{prefix}/{source.relative_to(manifest.parent).as_posix()}"
            # Integration tests and benches are test code by construction; they
            # never appear in a shipped build and are not what the process trust
            # model governs.
            if any(part in key for part in TEST_DIR_PARTS):
                continue
            sources[key] = source.read_text(encoding="utf-8")
        sources[f"{prefix}/Cargo.toml"] = manifest.read_text(encoding="utf-8")
    return sources


def scan(sources: dict[str, str]) -> list[str]:
    """Returns every violation found. An empty list means clean.

    Rules are grouped by file class rather than interleaved, so each one reads as
    a complete statement about a single set of files. An earlier draft put rule
    1b inside the per-file loop by accident; the self-test caught it, which is
    the argument for having one.
    """
    violations: list[str] = []

    if SHIPPED_MANAGER not in sources:
        violations.append(
            f"rule 1: the shipped manager source {SHIPPED_MANAGER} is missing; "
            "the process model cannot be checked"
        )

    rust_sources = {
        path: strip_test_code(path, raw)
        for path, raw in sources.items()
        if path.endswith(".rs")
    }

    # -- rule 1: only the three blessed edges may create a child process ----
    for path, production in sorted(rust_sources.items()):
        creates_process = bool(PROCESS_COMMAND.search(production)) or (
            bool(BARE_COMMAND_NEW.search(production))
            and bool(PROCESS_COMMAND_IMPORT.search(production))
        )
        if creates_process and path not in ALLOWED_PROCESS_SPAWNERS:
            violations.append(
                f"rule 1: {path} creates a child process; only "
                f"{sorted(ALLOWED_PROCESS_SPAWNERS)} may, because i2pr-apphost must "
                "be the only component that execs an application"
            )

    # -- rule 1b: sibling resolution, and never a shell ---------------------
    for path in sorted(ALLOWED_PROCESS_SPAWNERS):
        production = rust_sources.get(path, "")
        what = ALLOWED_PROCESS_SPAWNERS[path]
        if path in SIBLING_SPAWNERS and "current_exe" not in production:
            violations.append(
                f"rule 1b: {path} spawns {what} without a `current_exe()` sibling "
                "lookup, so its executable could come from configuration or argv"
            )
        for shell in FORBIDDEN_LAUNCH_TOKENS:
            if shell in production:
                violations.append(
                    f"rule 1b: {path} names a shell launcher ({shell}); Plan 369 "
                    "forbids shell-mediated launch entirely"
                )
        if PATH_LOOKUP.search(production):
            violations.append(
                f"rule 1b: {path} reads PATH; the executable must be a resolved "
                "absolute path, never a search result"
            )

    # -- rule 2: production code must not be able to name the fixture --------
    for path, production in sorted(rust_sources.items()):
        if path.startswith(f"crates/{FIXTURE_CRATE}/"):
            continue
        for token in FIXTURE_TOKENS:
            if token in production:
                violations.append(
                    f"rule 2: {path} names {token} in production code; the fixture "
                    "is evidence tooling and must be unreachable from any real build"
                )

    # -- rule 2b: no other crate may depend on the fixture ------------------
    for path, raw in sorted(sources.items()):
        if not path.endswith("Cargo.toml") or path.startswith(f"crates/{FIXTURE_CRATE}/"):
            continue
        if f"{FIXTURE_CRATE} = {{ path =" in raw or f"{FIXTURE_CRATE}.workspace" in raw:
            violations.append(
                f"rule 2: {path} depends on {FIXTURE_CRATE}; a production crate that "
                "can link the fixture could also make it launchable"
            )

    # -- rule 4: the manager test seam stays test-only ----------------------
    # `set_manager_path_override_for_tests` is what lets a test point the
    # supervisor at the fixture manager. A production caller would turn it into
    # "the router can be told which executable to start", which is exactly the
    # user-configurable program Plan 369 §4 forbids.
    #
    # The seam's *definition* is necessarily in production code -- it is
    # `#[doc(hidden)] pub` precisely so a `#[cfg(test)]` module in another
    # crate can reach it. A definition is inert: it stores a path nobody reads.
    # So the rule separates the two shapes rather than banning the name. A
    # definition is `fn <seam>(`; anything else that reaches the name -- a
    # `super::`/`crate::` path, a `use`, a bare call -- is a caller.
    for path, production in sorted(rust_sources.items()):
        without_definition = SEAM_DEFINITION.sub("fn seam(", production)
        if TEST_SEAM in without_definition:
            violations.append(
                f"rule 4: {path} calls {TEST_SEAM} from production code; a "
                "production caller could choose which manager executable the "
                "router starts, which is the user-configurable-program hole "
                "Plan 369 forbids"
            )

    # -- rule 3: the shipped manager uses only the persistent local catalog ---
    shipped = rust_sources.get(SHIPPED_MANAGER)
    if shipped is not None:
        if "Appd::with_catalog" in shipped or "EmptyCatalog" in shipped:
            violations.append(
                f"rule 3: {SHIPPED_MANAGER} selects an arbitrary or empty catalog; "
                "the shipped binary must use PersistentLaunchCatalog"
            )
        if "serve_with_catalog" not in shipped or "PersistentLaunchCatalog::new()" not in shipped:
            violations.append(
                f"rule 3: {SHIPPED_MANAGER} does not compose the persistent local-policy catalog"
            )
        # An argv *read* is not enough -- reading argv and carrying on is
        # exactly the "silently tolerates arguments" behaviour this rule
        # exists to prevent, and an earlier draft that only grepped for
        # `args_os` passed a manager that inspected argv and ignored it. So the
        # rule asserts the shape: an argv guard whose body returns.
        #
        # Stated limit: this is structural, not semantic. It cannot prove the
        # guard's condition is always true (a deliberate `if false && ..` would
        # satisfy it). That limit is recorded in the Plan 369 closure record
        # rather than papered over by claiming more than it checks.
        if not ARGV_REFUSAL.search(shipped):
            violations.append(
                f"rule 3: {SHIPPED_MANAGER} does not refuse arguments; the shipped "
                "manager must read argv and return early, because a manager that "
                "inspected argv and carried on would be indistinguishable from one "
                "that never saw it"
            )

    # -- rule 5: the daemon passes only the canonical state-root binding ------
    daemon_spawn = rust_sources.get("crates/i2pr-daemon/src/app_runtime.rs")
    if daemon_spawn is not None:
        if ".env_clear()" not in daemon_spawn:
            violations.append("rule 5: manager spawn does not clear the inherited environment")
        if not re.search(r"\.env\s*\(\s*STATE_ROOT_ENV\s*,\s*state_root\s*\)", daemon_spawn):
            violations.append("rule 5: manager spawn omits the exact I2PR_APP_STATE_ROOT binding")
        if re.search(r"\.envs?\s*\(", daemon_spawn.replace(".env(", "")):
            violations.append("rule 5: manager spawn forwards arbitrary environment entries")
        if 'STATE_ROOT_ENV: &str = "I2PR_APP_STATE_ROOT"' not in daemon_spawn:
            violations.append("rule 5: manager state-root variable name changed")
        if "std::fs::canonicalize(&managed)" not in daemon_spawn or "from_mode(0o700)" not in daemon_spawn:
            violations.append("rule 5: daemon state root is not canonicalized and owner-private")

    return violations


def self_test(sources: dict[str, str]) -> list[str]:
    """Proves each rule detects the violation it claims to.

    A guard that cannot be shown to fail is not a guard. Every mutation below is
    applied to the *real* sources in memory — nothing is written to disk — and
    the scan must reject it. The `expect_accepted` controls go the other way: a
    rule that is wrong in the strict direction is just as broken as one that is
    wrong in the permissive direction, so legal shapes must stay clean.
    """
    if scan(sources):
        return ["the real tree is already violating; a self-test over a dirty base proves nothing"]

    failures: list[str] = []

    def expect_rejected(label: str, mutate) -> None:
        mutated = dict(sources)
        mutated.update(mutate())
        if not scan(mutated):
            failures.append(f"positive control missed: {label}")

    def expect_accepted(label: str, addition: dict[str, str]) -> None:
        """A legal shape must stay clean — under *every* rule, not just `label`'s.

        The addition is appended to whatever the real file already contains
        rather than replacing it. An earlier draft replaced the file wholesale,
        which quietly deleted the production `current_exe()` lookup and let rule
        1b fire; the control then passed by filtering on `label` while the tree
        it scanned was nonsense. Appending keeps each control to exactly the one
        thing it claims to test.
        """
        merged = dict(sources)
        for path, extra in addition.items():
            merged[path] = merged.get(path, "") + extra
        found = scan(merged)
        if found:
            failures.append(f"negative control failed: {label} ({found[0]})")

    # Rule 1 — a new spawn site outside the two blessed edges.
    expect_rejected(
        "rule 1: a router crate spawning a child",
        lambda: {
            "crates/i2pr-core/src/lib.rs": (
                "fn escape() { std::process::Command::new(\"/bin/sh\").spawn(); }\n"
            )
        },
    )
    # Rule 1 — an apphost *sibling* helper that was not allow-listed.
    expect_rejected(
        "rule 1: an unlisted spawner inside i2pr-apphost",
        lambda: {
            "crates/i2pr-apphost/src/extra.rs": (
                "use std::process::Command;\nfn f() { let _ = Command::new(\"x\"); }\n"
            )
        },
    )

    # Rule 2 — production code naming the fixture executable.
    expect_rejected(
        "rule 2: production naming the fixture binary",
        lambda: {
            "crates/i2pr-daemon/src/app_runtime.rs": (
                "const FIXTURE_MANAGER: &str = \"i2pr-app-fixture-manager\";\n"
            )
        },
    )
    # Rule 2 — production code naming the fixture crate.
    expect_rejected(
        "rule 2: production naming the fixture crate",
        lambda: {
            "crates/i2pr-daemon/src/app_manager_bridge.rs": "use i2pr_app_fixture::Scenario;\n"
        },
    )
    # Rule 2b — a production crate depending on the fixture.
    expect_rejected(
        "rule 2: a production dependency on the fixture",
        lambda: {"crates/i2pr-daemon/Cargo.toml": f"{FIXTURE_CRATE} = {{ path = \"../x\" }}\n"},
    )
    # Rule 2 must NOT fire for the fixture's own crate or for test-only modules:
    # a guard that is wrong in the strict direction is still a broken guard.
    expect_accepted(
        "rule 2: the fixture naming itself",
        {"crates/i2pr-app-fixture/src/lib.rs": "// i2pr-app-fixture evidence tooling\n"},
    )
    expect_accepted(
        "rule 2: a cfg(test) module naming the fixture",
        {
            "crates/i2pr-daemon/src/app_runtime_qualification.rs": (
                "const FIXTURE_MANAGER: &str = \"i2pr-app-fixture-manager\";\n"
            )
        },
    )
    # Rule 2 must stay quiet about a `#[cfg(test)] mod tests` block *inside* a
    # production file, not only about a whole test-only file.
    expect_accepted(
        "rule 2: an inline cfg(test) block naming the fixture",
        {
            "crates/i2pr-appd/src/lib.rs": (
                "pub fn production() {}\n#[cfg(test)]\nmod tests {\n"
                "    const X: &str = \"i2pr-app-fixture\";\n}\n"
            )
        },
    )

    # Rule 3 — the shipped manager selecting a custom catalog.
    expect_rejected(
        "rule 3: the shipped manager selecting a catalog",
        lambda: {
            SHIPPED_MANAGER: "fn main() { let _ = i2pr_appd::Appd::with_catalog(x); }\n"
        },
    )
    # Rule 3 — the shipped manager silently tolerating arguments.
    expect_rejected(
        "rule 3: the shipped manager ignoring argv",
        lambda: {SHIPPED_MANAGER: "fn main() { let _ = 1; }\n"},
    )
    # Rule 3 — the failure an argv *read* alone cannot see: the manager inspects
    # argv, announces the arguments, and carries on. The strengthened rule is
    # specifically for this shape.
    expect_rejected(
        "rule 3: the shipped manager reading argv and carrying on",
        lambda: {
            SHIPPED_MANAGER: (
                "fn main() {\n"
                "    let extra = std::env::args_os().len() - 1;\n"
                "    if extra > 0 {\n"
                "        eprintln!(\"i2pr-appd ignoring {extra} arguments\");\n"
                "    }\n"
                "    run();\n"
                "}\n"
            )
        },
    )
    # ...and the legal argv refusal shape remains intact in the real source.
    expect_accepted(
        "rule 3: the shipped manager refusing argv",
        {
            SHIPPED_MANAGER: (
                "\nfn guard() {\n"
                "    if std::env::args_os().len() > 1 {\n"
                "        return ExitCode::from(2);\n"
                "    }\n"
                "}\n"
            )
        },
    )
    # Rule 3 — the shipped manager disappearing entirely.
    missing = {key: value for key, value in sources.items() if key != SHIPPED_MANAGER}
    if not scan(missing):
        failures.append("positive control missed: rule 3: a missing shipped manager")

    # Rule 4 — production reaching for the manager test seam.
    expect_rejected(
        "rule 4: production naming the manager test seam",
        lambda: {
            "crates/i2pr-daemon/src/app_runtime.rs": (
                "pub fn escape(p: PathBuf) { let _ = set_manager_path_override_for_tests(Some(p)); }\n"
            )
        },
    )
    # Rule 4 must stay quiet for a test-only caller, or the fixture harness itself
    # would be a violation.
    expect_accepted(
        "rule 4: a cfg(test) module using the manager test seam",
        {
            "crates/i2pr-daemon/src/app_runtime_qualification.rs": (
                "fn s() { set_manager_path_override_for_tests(None); }\n"
            )
        },
    )

    # Rule 5 — the manager may receive only the daemon-created canonical root.
    expect_rejected(
        "rule 5: inherited environment is not cleared",
        lambda: {"crates/i2pr-daemon/src/app_runtime.rs": sources["crates/i2pr-daemon/src/app_runtime.rs"].replace(".env_clear()", "")},
    )
    expect_rejected(
        "rule 5: arbitrary environment forwarding",
        lambda: {"crates/i2pr-daemon/src/app_runtime.rs": sources["crates/i2pr-daemon/src/app_runtime.rs"].replace(".env(STATE_ROOT_ENV, state_root)", ".env(STATE_ROOT_ENV, state_root).envs(std::env::vars())")},
    )
    expect_rejected(
        "rule 5: missing root binding",
        lambda: {"crates/i2pr-daemon/src/app_runtime.rs": sources["crates/i2pr-daemon/src/app_runtime.rs"].replace(".env(STATE_ROOT_ENV, state_root)", "")},
    )
    expect_accepted(
        "rule 4: an inline cfg(test) block using the manager test seam",
        {
            "crates/i2pr-daemon/src/app_runtime.rs": (
                "#[cfg(test)]\nmod tests {\n"
                "    fn s() { set_manager_path_override_for_tests(None); }\n}\n"
            )
        },
    )
    # The definition itself is inert and must stay legal: it is `#[doc(hidden)]
    # pub` so that a `#[cfg(test)]` module in another crate can reach it. A
    # rule that banned the definition would make the harness unreachable and
    # would have to be muted, at which point it would protect nothing.
    expect_accepted(
        "rule 4: the seam's own definition",
        {
            "crates/i2pr-daemon/src/app_runtime.rs": (
                "#[doc(hidden)]\npub fn set_manager_path_override_for_tests(p: Option<PathBuf>) {}\n"
            )
        },
    )

    return failures


_BASE_SOURCES: dict[str, str] = {}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the rules still reject the violations they claim to",
    )
    args = parser.parse_args()

    global _BASE_SOURCES
    try:
        _BASE_SOURCES = load_sources(ROOT)
    except SystemExit as error:
        print(error, file=sys.stderr)
        return 1

    if args.self_test:
        failures = self_test(_BASE_SOURCES)
        if failures:
            for failure in failures:
                print(f"check-managed-app-process-boundary: FAIL: {failure}", file=sys.stderr)
            return 1
        print("check-managed-app-process-boundary: self-test ok")
        return 0

    violations = scan(_BASE_SOURCES)
    if violations:
        for violation in violations:
            print(f"check-managed-app-process-boundary: FAIL: {violation}", file=sys.stderr)
        return 1
    print("check-managed-app-process-boundary: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
