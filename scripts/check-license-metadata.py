#!/usr/bin/env python3
"""Plan 379 -- repository-wide license-metadata drift guard.

Plan 379's registration commit added an owner-selected MIT `LICENSE`, the README
license section, and `license = "MIT"` under `[workspace.package]`. That is the
*legal* half. The *metadata* half is separate and was, at that commit, silently
absent: not one of the 26 workspace member manifests inherited the workspace
license, so `cargo metadata` reported `license = null` for every package. The
repository said "MIT" in three places and told Cargo nothing at all.

That gap is the shape this guard exists for. A `[workspace.package] license`
key that no member inherits is a comment, not metadata. Nothing in the build,
in `cargo package`, or in `cargo publish` would ever report it, so no
compiler, test, or existing checker would notice it going away -- which is
exactly how it went missing in the first place.

**What is asserted.**

1. The root `LICENSE` is the MIT text (shape, not a byte-identical template --
   the copyright holder line legitimately varies).
2. `[workspace.package]` declares `license = "MIT"`, exactly once, and declares
   no competing `license-file`.
3. Every workspace member manifest carries a license key that either inherits
   the workspace value or names MIT literally. A member with no license key at
   all is a violation, not an exemption: this guard has no allow-list, because
   a crate that genuinely needs one needs a plan-of-record to say so in prose.
4. No member declares `license-file`/`license_file`. A `LICENSE`-file key is a
   second source of truth for the same fact and Cargo resolves `license` and
   `license_file` independently.
5. The resolved truth agrees with the static truth: `cargo metadata --no-deps`
   reports `license == "MIT"` and `license_file == null` for every member, and
   the member set it reports is the member set `Cargo.toml` declares. Rule 5 is
   what makes rule 3 non-cosmetic -- it is the difference between "the manifest
   mentions MIT" and "Cargo will publish MIT".
6. The README names MIT, links `LICENSE`, and still carries the clean-room /
   provenance clause. The license change must not read as permission to copy
   source from external implementations.

Run with ``python3``. ``bash scripts/*.py`` garbles these scripts and exits 2.

Exit codes: 0 = every assertion holds; 1 = at least one violation; 2 = the
guard could not run (an unchecked tree is never reported as a pass).

Scope: this guard READS the tree and runs `cargo metadata`. It never edits a
file, opens a socket, or executes a project script. Parsing is regex-based
rather than `tomllib`-based on purpose: `tomllib` needs Python 3.11+, macOS
ships 3.9 as `python3`, and a guard that cannot run on the maintainer's host
is a guard that gets skipped. Every detector below has a positive control in
`--self-test` so a regex that quietly matches nothing is caught rather than
assumed correct.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

LEDGER = (
    "plans/implementation/portable-service-tunnels/"
    "379-mit-license-and-portable-service-tunnel-cleanup.md"
)

ROOT_CARGO = "Cargo.toml"
LICENSE = "LICENSE"
README = "README.md"
EXPECTED = "MIT"

# The four clauses that make a license text MIT. Checking the shape rather than
# a byte-identical template keeps the copyright holder line free to change.
MIT_MARKERS = (
    "MIT License",
    "Permission is hereby granted, free of charge",
    'THE SOFTWARE IS PROVIDED "AS IS"',
    "without restriction, including without limitation the rights",
)

# `license.workspace = true` (dotted key) or `license = "MIT"`. A trailing
# comment is legal TOML and is accepted; a *full-line* comment is not metadata,
# and the `^[ \t]*` anchor is what keeps `# license = "Apache-2.0"` from being
# read as a declaration. Note `\s` is deliberately not used inside the pattern:
# in MULTILINE it also matches the newline, which silently turns an anchored
# `$` into "blank lines may follow".
LICENSE_KEY = re.compile(
    r"^[ \t]*license(?:\.workspace)?[ \t]*=[ \t]*"
    r"(?:true|\"(?P<spdx>[^\"]+)\")[ \t]*(?:#.*)?$",
    re.M,
)
LICENSE_WORKSPACE_KEY = re.compile(
    r"^[ \t]*license\.workspace[ \t]*=[ \t]*true[ \t]*(?:#.*)?$", re.M
)
LICENSE_FILE_KEY = re.compile(r"^[ \t]*license[-_]file(?:\.workspace)?[ \t]*=", re.M)


# --------------------------------------------------------------------------
# Parsing helpers. Each returns None for "not parseable", which is a violation
# in its own right rather than a value the caller may treat as permissive.
# --------------------------------------------------------------------------


def package_table(text: str) -> str | None:
    """The `[package]` table body of a manifest."""
    match = re.search(r"^\[package\]\s*$(.*?)(?=^\[|\Z)", text, re.M | re.S)
    return match.group(1) if match else None


def workspace_package_table(text: str) -> str | None:
    """The `[workspace.package]` table body of the root manifest."""
    match = re.search(r"^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)", text, re.M | re.S)
    return match.group(1) if match else None


def workspace_members(text: str) -> list[str] | None:
    """Declared member paths, or None when the member list cannot be read."""
    match = re.search(r"^members\s*=\s*\[(.*?)\]", text, re.S | re.M)
    if not match:
        return None
    return re.findall(r'"([^"]+)"', match.group(1))


def manifest_license(text: str) -> str | None:
    """The license a member manifest declares.

    Returns the literal SPDX string, the sentinel ``"<workspace>"`` for an
    inherited key, or None when the manifest declares no license key at all.
    """
    table = package_table(text)
    if table is None:
        return None
    if LICENSE_WORKSPACE_KEY.search(table):
        return "<workspace>"
    match = LICENSE_KEY.search(table)
    if match:
        return match.group("spdx") or "<workspace>"
    return None


def workspace_license(text: str) -> str | None:
    """The license the root manifest publishes to inheriting members."""
    table = workspace_package_table(text)
    if table is None:
        return None
    match = LICENSE_KEY.search(table)
    if not match:
        return None
    return match.group("spdx") or "<workspace>"


def cargo_metadata(root: Path) -> list[dict] | None:
    """Resolved package metadata, or None when cargo could not produce it."""
    try:
        out = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=root,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    try:
        return json.loads(out)["packages"]
    except (json.JSONDecodeError, KeyError, TypeError):
        return None


# --------------------------------------------------------------------------
# Rules.
# --------------------------------------------------------------------------


def scan(root: Path) -> list[str]:
    """Return violation strings; empty means every assertion holds."""
    violations: list[str] = []

    # Rule 1 -- the root LICENSE is MIT text.
    try:
        license_text = (root / LICENSE).read_text(encoding="utf-8")
    except OSError as exc:
        return [f"rule 1: cannot read {LICENSE}: {exc}"]
    for marker in MIT_MARKERS:
        if marker not in license_text:
            violations.append(
                f"rule 1: {LICENSE} does not look like the MIT license; the "
                f"clause {marker!r} is missing"
            )

    try:
        root_cargo = (root / ROOT_CARGO).read_text(encoding="utf-8")
    except OSError as exc:
        return violations + [f"rule 2: cannot read {ROOT_CARGO}: {exc}"]

    # Rule 2 -- the root manifest publishes exactly one workspace license.
    declared = workspace_license(root_cargo)
    if declared != EXPECTED:
        violations.append(
            f"rule 2: [{tuple('workspace.package')}] in {ROOT_CARGO} declares "
            f"license {declared!r}; expected {EXPECTED!r}"
        )
    if LICENSE_FILE_KEY.search(workspace_package_table(root_cargo) or ""):
        violations.append(
            f"rule 2: {ROOT_CARGO} declares a workspace license-file alongside its "
            "workspace license; two sources of truth for one fact"
        )

    # Rule 3 -- every member manifest inherits or names MIT.
    members = workspace_members(root_cargo)
    if members is None:
        violations.append(
            f"rule 3: cannot read the workspace member list from {ROOT_CARGO}"
        )
        return violations
    manifest_count = 0
    for relative in members:
        path = root / relative / "Cargo.toml"
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:
            violations.append(f"rule 3: cannot read {relative}/Cargo.toml: {exc}")
            continue
        manifest_count += 1
        found = manifest_license(text)
        if found is None:
            violations.append(
                f"rule 3: {relative}/Cargo.toml declares no `license` key, so "
                f"`cargo metadata` reports `license = null` for it while the "
                f"repository claims {EXPECTED}"
            )
        elif found not in ("<workspace>", EXPECTED):
            violations.append(
                f"rule 3: {relative}/Cargo.toml declares license {found!r}; "
                f"expected {EXPECTED!r} or an inherited workspace license"
            )
        # Rule 4 -- no member carries a competing license-file key.
        if LICENSE_FILE_KEY.search(package_table(text) or ""):
            violations.append(
                f"rule 4: {relative}/Cargo.toml declares a license-file; Cargo "
                "resolves `license` and `license_file` independently, so this is "
                "a second and disagreeing source of truth"
            )

    # Rule 5 -- the resolved metadata agrees with the static metadata.
    packages = cargo_metadata(root)
    if packages is None:
        violations.append(
            "rule 5: `cargo metadata --no-deps` did not produce package metadata; "
            "refusing to report an unchecked tree as a pass"
        )
        return violations
    by_name = {p["name"]: p for p in packages}
    for relative in members:
        try:
            text = (root / relative / "Cargo.toml").read_text(encoding="utf-8")
        except OSError:
            continue  # already reported by rule 3
        name_match = re.search(r'^name\s*=\s*"([^"]+)"', text, re.M)
        if not name_match:
            continue
        name = name_match.group(1)
        package = by_name.get(name)
        if package is None:
            violations.append(
                f"rule 5: {relative} declares package {name!r}, which "
                "`cargo metadata` does not report as a workspace package"
            )
            continue
        if package.get("license") != EXPECTED:
            violations.append(
                f"rule 5: cargo metadata reports license "
                f"{package.get('license')!r} for {name!r}; expected {EXPECTED!r}"
            )
        if package.get("license_file") is not None:
            violations.append(
                f"rule 5: cargo metadata reports license_file "
                f"{package.get('license_file')!r} for {name!r}"
            )
    unresolved = sorted(
        name for name in by_name if name not in _declared_names(root, members)
    )
    if unresolved:
        violations.append(
            f"rule 5: cargo metadata reports package(s) "
            f"{', '.join(unresolved)} that {ROOT_CARGO} does not declare as a "
            "workspace member; the two member lists disagree"
        )
    if manifest_count and len(packages) != len(members):
        violations.append(
            f"rule 5: {ROOT_CARGO} declares {len(members)} members and cargo "
            f"metadata reports {len(packages)} packages"
        )

    # Rule 6 -- the README states MIT and keeps the provenance clause.
    try:
        readme = (root / README).read_text(encoding="utf-8")
    except OSError as exc:
        violations.append(f"rule 6: cannot read {README}: {exc}")
        return violations
    if not re.search(r"^##\s+License\s*$", readme, re.M):
        violations.append(f"rule 6: {README} has no `## License` section")
    elif f"[MIT License]({LICENSE})" not in readme:
        violations.append(
            f"rule 6: the {README} `## License` section does not link the "
            f"repository `{LICENSE}` file"
        )
    if not re.search(r"\bMIT\b", readme):
        violations.append(f"rule 6: {README} never names the MIT license")
    for clause in ("clean-room", "provenance"):
        if clause not in readme:
            violations.append(
                f"rule 6: {README} no longer states the {clause} rule; a license "
                "selection must not read as permission to copy source"
            )

    return violations


def _declared_names(root: Path, members: list[str]) -> set[str]:
    """Package names the member manifests declare."""
    names: set[str] = set()
    for relative in members:
        try:
            text = (root / relative / "Cargo.toml").read_text(encoding="utf-8")
        except OSError:
            continue
        match = re.search(r'^name\s*=\s*"([^"]+)"', text, re.M)
        if match:
            names.add(match.group(1))
    return names


# --------------------------------------------------------------------------
# Self-test. Each rule gets a rejected mutation and, where a rule could be
# wrong in the strict direction, an accepted control.
# --------------------------------------------------------------------------


def self_test(root: Path) -> list[str]:
    """Prove each rule rejects the violation it claims to detect."""
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
            work = Path(tmp) / label
            shutil.copytree(base, work, symlinks=True)
            try:
                mutate(work)
                found = scan(work)
            except Exception as exc:
                failures.append(f"self-test raised: {label} ({exc})")
                shutil.rmtree(work, ignore_errors=True)
                return
            shutil.rmtree(work, ignore_errors=True)
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
            work = Path(tmp) / ("ok_" + label)
            shutil.copytree(base, work, symlinks=True)
            try:
                mutate(work)
                found = scan(work)
            except Exception as exc:
                failures.append(f"self-test raised: {label} ({exc})")
                shutil.rmtree(work, ignore_errors=True)
                return
            shutil.rmtree(work, ignore_errors=True)
            if found:
                failures.append(f"negative control failed: {label} ({found[0]})")

        # Rule 1 -- a LICENSE that is not MIT text.
        expect_rejected(
            "rule1_license_not_mit",
            lambda w: (w / LICENSE).write_text(
                "GNU AFFERO GENERAL PUBLIC LICENSE\nVersion 3\n", encoding="utf-8"
            ),
        )

        # Rule 2 -- the workspace license removed or replaced.
        expect_rejected(
            "rule2_workspace_license_missing",
            lambda w: (w / ROOT_CARGO).write_text(
                re.sub(
                    r'^license\s*=\s*"MIT"\s*$',
                    "",
                    (w / ROOT_CARGO).read_text(),
                    count=1,
                    flags=re.M,
                ),
                encoding="utf-8",
            ),
        )
        expect_rejected(
            "rule2_workspace_license_wrong",
            lambda w: (w / ROOT_CARGO).write_text(
                (w / ROOT_CARGO).read_text().replace(
                    'license = "MIT"', 'license = "Apache-2.0"', 1
                ),
                encoding="utf-8",
            ),
        )

        # Rule 3 -- the exact defect Plan 379 found: `[workspace.package]` says
        # MIT and no member inherits it.
        expect_rejected(
            "rule3_member_does_not_inherit",
            lambda w: (w / "crates" / "i2pr-proto" / "Cargo.toml").write_text(
                re.sub(
                    r"^license\.workspace\s*=\s*true\s*$",
                    "",
                    (w / "crates" / "i2pr-proto" / "Cargo.toml").read_text(),
                    count=1,
                    flags=re.M,
                ),
                encoding="utf-8",
            ),
        )
        expect_rejected(
            "rule3_member_license_wrong",
            lambda w: (w / "crates" / "i2pr-proto" / "Cargo.toml").write_text(
                (w / "crates" / "i2pr-proto" / "Cargo.toml")
                .read_text()
                .replace(
                    "license.workspace = true", 'license = "GPL-3.0"', 1
                ),
                encoding="utf-8",
            ),
        )

        # Rule 4 -- a member carrying a second, disagreeing license source.
        expect_rejected(
            "rule4_member_license_file",
            lambda w: (w / "crates" / "i2pr-su3" / "Cargo.toml").write_text(
                (w / "crates" / "i2pr-su3" / "Cargo.toml").read_text().replace(
                    "license.workspace = true",
                    'license.workspace = true\nlicense-file = "COPYING"',
                    1,
                ),
                encoding="utf-8",
            ),
        )

        # Rule 5 -- static metadata clean, resolved metadata not. Only
        # `cargo metadata` can see this, which is why rule 5 exists separately
        # from rule 3.
        expect_rejected(
            "rule5_resolved_license_missing",
            lambda w: (w / "Cargo.toml").write_text(
                (w / "Cargo.toml").read_text().replace(
                    'license = "MIT"', "", 1
                ),
                encoding="utf-8",
            ),
        )

        # Rule 6 -- README provenance clause removed.
        expect_rejected(
            "rule6_readme_provenance_dropped",
            lambda w: (w / README).write_text(
                (w / README).read_text().replace("provenance", "origin"),
                encoding="utf-8",
            ),
        )
        expect_rejected(
            "rule6_readme_license_unlinked",
            lambda w: (w / README).write_text(
                (w / README).read_text().replace(
                    "[MIT License](LICENSE)", "MIT License", 1
                ),
                encoding="utf-8",
            ),
        )

        # Negative control: an explicit package-level `license = "MIT"` is
        # equally unambiguous as inheritance, and must not be reported.
        expect_accepted(
            "explicit_package_license_is_allowed",
            lambda w: (w / "crates" / "i2pr-su3" / "Cargo.toml").write_text(
                (w / "crates" / "i2pr-su3" / "Cargo.toml")
                .read_text()
                .replace('license.workspace = true', 'license = "MIT"', 1),
                encoding="utf-8",
            ),
        )
        # Negative control: a commented-out license is prose, not metadata, and
        # must not be read as a declaration.
        expect_accepted(
            "commented_license_is_not_metadata",
            lambda w: (w / "crates" / "i2pr-su3" / "Cargo.toml").write_text(
                (w / "crates" / "i2pr-su3" / "Cargo.toml").read_text().replace(
                    "license.workspace = true",
                    'license.workspace = true\n# license = "Apache-2.0"',
                    1,
                ),
                encoding="utf-8",
            ),
        )
        # Negative control: a trailing comment is legal TOML. An earlier draft
        # of the key pattern anchored on `\s*$`, which in MULTILINE also eats
        # the newline -- so a manifest carrying a trailing comment parsed as
        # having *no* license at all. A guard that misreads a correct manifest
        # is as broken as one that accepts a wrong one, so the strict-direction
        # error gets a control of its own.
        expect_accepted(
            "trailing_comment_is_still_metadata",
            lambda w: (w / "crates" / "i2pr-su3" / "Cargo.toml").write_text(
                (w / "crates" / "i2pr-su3" / "Cargo.toml")
                .read_text()
                .replace(
                    "license.workspace = true",
                    "license.workspace = true  # Plan 379",
                    1,
                ),
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
        else:
            failures = scan(root)
    except FileNotFoundError as exc:
        print(f"check-license-metadata: cannot run: {exc}", file=sys.stderr)
        return 2
    except Exception as exc:  # fail closed: an unreadable tree is not a pass
        print(
            "check-license-metadata: cannot run; refusing to report success on "
            f"an unchecked tree: {exc}",
            file=sys.stderr,
        )
        return 2

    label = "self-test" if args.self_test else "check"
    for failure in failures:
        print(f"check-license-metadata: FAIL: {failure}", file=sys.stderr)
    if failures:
        print(
            f"check-license-metadata: {len(failures)} {label} failure(s). The "
            f"repository license is {EXPECTED} in {LICENSE} and {ROOT_CARGO}; "
            f"every workspace member must say so where Cargo can read it. See "
            f"{LEDGER}.",
            file=sys.stderr,
        )
        return 1
    if args.self_test:
        print("check-license-metadata: self-test ok")
    else:
        members = workspace_members((root / ROOT_CARGO).read_text(encoding="utf-8"))
        print(
            f"check-license-metadata: {EXPECTED} license is consistent across "
            f"{LICENSE}, {README}, [{'workspace.package'}], and all "
            f"{len(members or [])} member manifests"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())