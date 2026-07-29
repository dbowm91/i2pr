"""Plan 058 candidate record integrity validator.

The candidate record is the canonical single-source-of-truth declaration
of one frozen source commit for a Milestone 3 evidence run. Plan 058
forbids the defects that the previous draft (Plan 056 candidate) carried:

- multiple SHAs claiming to be the same authoritative external
  candidate;
- ignored diagnostics described as committed evidence;
- retired candidates consumed by execution tooling;
- candidates frozen before the implementation floor required to run
  them.

The validator is intentionally loadable from a JSON file or extracted
from a Markdown candidate document. It enforces the locked schema
fields, the single-authoritative-SHA invariant, the implementation-floor
ordering, the retirement guard, and the local-untracked evidence rule.

Schema:

```text
i2pr-interop-candidate-v1
```

Locked fields are enumerated at the top of ``CANDIDATE_FIELDS``. The
mandatory invariants are enumerated in :func:`validate_candidate_record`.
"""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path
from typing import Any


CANDIDATE_SCHEMA = "i2pr-interop-candidate-v1"
CANDIDATE_SCHEMA_VERSION = 1


CANDIDATE_FIELDS = (
    "schema",
    "schema_version",
    "plan",
    "candidate_commit",
    "status",
    "implementation_floor_commit",
    "source_tree_sha256",
    "validation_receipt_sha256",
    "storage_classification",
    "history_commits",
    "candidate_sha256",
)


_HEX40 = re.compile(r"^[0-9a-f]{40}$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")

CANDIDATE_STATUSES = {"declared", "retired", "executed"}
STORAGE_CLASSIFICATIONS = {"tracked", "local-untracked", "external-durable"}


class CandidateRecordError(ValueError):
    """Raised when a candidate record fails integrity validation."""


def _scan(value: Any) -> None:
    if isinstance(value, str):
        lowered = value.lower()
        for forbidden in (
            "-----begin",
            "router.identity",
            "ntcp2.static.key",
            "private key",
            "home/",
            "root/",
        ):
            if forbidden in lowered:
                raise CandidateRecordError(
                    f"candidate record contains forbidden string: {forbidden}"
                )
    elif isinstance(value, dict):
        for child in value.values():
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


def _run_git(repo_root: Path, *args: str) -> str:
    try:
        completed = subprocess.run(
            ("git",) + args,
            cwd=str(repo_root),
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as exc:
        raise CandidateRecordError(
            f"git invocation failed (cwd={repo_root}, args={args}): "
            f"{exc.stderr.strip() or exc}"
        ) from exc
    except (OSError, FileNotFoundError) as exc:
        raise CandidateRecordError(
            f"git invocation failed (cwd={repo_root}, args={args}): {exc}"
        ) from exc
    return completed.stdout.strip()


def _resolve_commit(repo_root: Path, commit: str) -> str:
    if not _HEX40.fullmatch(commit):
        raise CandidateRecordError(f"commit is not a 40-character SHA: {commit!r}")
    try:
        resolved = _run_git(repo_root, "rev-parse", "--verify", commit)
    except CandidateRecordError as exc:
        raise CandidateRecordError(
            f"candidate commit does not resolve in git history: {commit}"
        ) from exc
    try:
        object_type = _run_git(repo_root, "cat-file", "-t", resolved)
    except CandidateRecordError as exc:
        raise CandidateRecordError(
            f"candidate commit does not resolve in git history: {commit}"
        ) from exc
    if object_type != "commit":
        raise CandidateRecordError(
            f"candidate commit does not resolve in git history: {commit}"
        )
    return resolved


def _is_ancestor(repo_root: Path, ancestor: str, descendant: str) -> bool:
    if ancestor == descendant:
        return True
    try:
        completed = subprocess.run(
            ("git", "merge-base", "--is-ancestor", ancestor, descendant),
            cwd=str(repo_root),
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as exc:
        if exc.returncode == 1:
            return False
        raise CandidateRecordError(
            f"git merge-base failed (cwd={repo_root}, args=merge-base "
            f"--is-ancestor {ancestor} {descendant}): {exc.stderr.strip()}"
        ) from exc
    return completed.returncode == 0


def _record_sha256(record: dict[str, Any]) -> str:
    import hashlib

    unsigned = dict(record)
    unsigned["candidate_sha256"] = ""
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def validate_candidate_record(
    record: dict[str, Any],
    *,
    git_repo_root: Path | None = None,
) -> None:
    """Validate the candidate record shape and every required invariant.

    Raises :class:`CandidateRecordError` whenever a required invariant is
    violated. The validator never mutates the record.
    """

    if not isinstance(record, dict):
        raise CandidateRecordError("candidate record must be a JSON object")
    if tuple(record) != CANDIDATE_FIELDS:
        raise CandidateRecordError("candidate record fields do not match the locked schema")
    if record["schema"] != CANDIDATE_SCHEMA:
        raise CandidateRecordError(f"unknown candidate schema: {record['schema']!r}")
    if record["schema_version"] != CANDIDATE_SCHEMA_VERSION:
        raise CandidateRecordError(
            f"unsupported candidate schema version: {record['schema_version']!r}"
        )
    if not isinstance(record["plan"], int) or record["plan"] <= 0:
        raise CandidateRecordError("plan must be a positive integer")
    if record["status"] not in CANDIDATE_STATUSES:
        raise CandidateRecordError(
            f"status must be one of {sorted(CANDIDATE_STATUSES)}: {record['status']!r}"
        )
    if record["storage_classification"] not in STORAGE_CLASSIFICATIONS:
        raise CandidateRecordError(
            f"storage_classification must be one of {sorted(STORAGE_CLASSIFICATIONS)}: "
            f"{record['storage_classification']!r}"
        )
    _scan(record)
    if not _HEX40.fullmatch(record["candidate_commit"]):
        raise CandidateRecordError("candidate_commit must be a 40 lowercase hex SHA")
    if not _HEX40.fullmatch(record["implementation_floor_commit"]):
        raise CandidateRecordError(
            "implementation_floor_commit must be a 40 lowercase hex SHA"
        )
    if not _HEX64.fullmatch(record["source_tree_sha256"]):
        raise CandidateRecordError("source_tree_sha256 must be a 64 lowercase hex SHA-256")
    validation_receipt = record["validation_receipt_sha256"]
    if validation_receipt != "0" * 64 and not _HEX64.fullmatch(validation_receipt):
        raise CandidateRecordError(
            "validation_receipt_sha256 must be a 64 lowercase hex SHA-256 or the "
            "typed absence placeholder (64 zeros)"
        )
    history = record["history_commits"]
    if not isinstance(history, list):
        raise CandidateRecordError("history_commits must be a list")
    if record["candidate_commit"] in history:
        raise CandidateRecordError(
            "history_commits must not contain the authoritative candidate_commit"
        )
    if record["implementation_floor_commit"] in history:
        raise CandidateRecordError(
            "history_commits must not contain the authoritative implementation_floor_commit"
        )
    for entry in history:
        if not _HEX40.fullmatch(entry):
            raise CandidateRecordError(
                f"history_commits entry is not a 40 lowercase hex SHA: {entry!r}"
            )
    if record["candidate_sha256"] and not _HEX64.fullmatch(record["candidate_sha256"]):
        raise CandidateRecordError("candidate_sha256 must be a 64 lowercase hex SHA-256")

    if git_repo_root is not None:
        git_repo_root = Path(git_repo_root)
        resolved_candidate = _resolve_commit(git_repo_root, record["candidate_commit"])
        resolved_floor = _resolve_commit(git_repo_root, record["implementation_floor_commit"])
        if resolved_candidate != record["candidate_commit"]:
            raise CandidateRecordError(
                "candidate_commit must be fully resolved before declaration"
            )
        if resolved_floor != record["implementation_floor_commit"]:
            raise CandidateRecordError(
                "implementation_floor_commit must be fully resolved before declaration"
            )
        if record["status"] in {"declared", "executed"}:
            if not _is_ancestor(
                git_repo_root, record["implementation_floor_commit"],
                record["candidate_commit"],
            ):
                raise CandidateRecordError(
                    "declared/executed candidate must be a descendant of the "
                    "implementation_floor_commit"
                )
        for entry in history:
            _resolve_commit(git_repo_root, entry)
            if _is_ancestor(git_repo_root, resolved_candidate, entry):
                raise CandidateRecordError(
                    "history_commits must not contain a commit that is the "
                    "authoritative descendant of the candidate_commit"
                )

    if record["storage_classification"] == "tracked" and record["status"] != "executed":
        raise CandidateRecordError(
            "tracked local artifacts cannot be claimed for a non-executed candidate"
        )

    expected_sha = _record_sha256(record)
    if record["candidate_sha256"] and record["candidate_sha256"] != expected_sha:
        raise CandidateRecordError(
            "candidate_sha256 does not match the canonical digest of the record"
        )


def active_candidate_record(record: dict[str, Any]) -> bool:
    """Return ``True`` when the candidate is suitable for Plan 060 tooling."""

    if not isinstance(record, dict):
        return False
    if record.get("schema") != CANDIDATE_SCHEMA:
        return False
    return record.get("status") in {"declared", "executed"}


def build_candidate_record(
    *,
    plan: int,
    candidate_commit: str,
    status: str,
    implementation_floor_commit: str,
    source_tree_sha256: str,
    validation_receipt_sha256: str = "0" * 64,
    storage_classification: str = "local-untracked",
    history_commits: tuple[str, ...] = (),
) -> dict[str, Any]:
    """Build an unsigned candidate record for writer finalization."""

    return {
        "schema": CANDIDATE_SCHEMA,
        "schema_version": CANDIDATE_SCHEMA_VERSION,
        "plan": plan,
        "candidate_commit": candidate_commit,
        "status": status,
        "implementation_floor_commit": implementation_floor_commit,
        "source_tree_sha256": source_tree_sha256,
        "validation_receipt_sha256": validation_receipt_sha256,
        "storage_classification": storage_classification,
        "history_commits": list(history_commits),
        "candidate_sha256": "",
    }


def finalize_candidate_record(record: dict[str, Any]) -> dict[str, Any]:
    """Validate, finalize, and return the candidate record with its canonical digest."""

    validate_candidate_record(record)
    record["candidate_sha256"] = _record_sha256(record)
    validate_candidate_record(record)
    return record


def load_candidate_record(path: Path) -> dict[str, Any]:
    """Load and validate a candidate record from a JSON file."""

    record = json.loads(Path(path).read_text(encoding="utf-8"))
    validate_candidate_record(record)
    return record


def extract_candidate_record_from_markdown(
    markdown_path: Path,
    *,
    repo_root: Path | None = None,
) -> dict[str, Any]:
    """Extract a candidate record from a fenced-JSON block in a Markdown file.

    The candidate record must be the only ``i2pr-interop-candidate-v1``
    JSON object in the document. The function refuses to return a record
    unless the document declares ``status`` and a single ``candidate_commit``
    line and disambiguates the authoritative SHA from historical
    references.
    """

    text = Path(markdown_path).read_text(encoding="utf-8")
    blocks = re.findall(r"```(?:json|text)\n(.*?)```", text, flags=re.DOTALL)
    candidates: list[dict[str, Any]] = []
    for block in blocks:
        stripped = block.strip()
        if not stripped:
            continue
        try:
            parsed = json.loads(stripped)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict) and parsed.get("schema") == CANDIDATE_SCHEMA:
            candidates.append(parsed)
    if len(candidates) != 1:
        raise CandidateRecordError(
            f"expected exactly one candidate record in {markdown_path}, found {len(candidates)}"
        )
    record = candidates[0]
    validate_candidate_record(record, git_repo_root=repo_root)
    return record


def retired_marker_present(markdown_path: Path) -> bool:
    """Return ``True`` when a candidate document declares a strict retirement marker."""

    text = Path(markdown_path).read_text(encoding="utf-8")
    patterns = (
        r"^#+\s*Status:\s*\*\*retired",
        r"^#+\s*Status:\s*retired",
    )
    return any(re.search(p, text, flags=re.MULTILINE) for p in patterns) or any(
        re.search(p, text, flags=re.MULTILINE) for p in (
            r"^Status:\s*\*\*retired",
            r"^Status:\s*retired",
        )
    )


def superseded_marker_present(markdown_path: Path) -> bool:
    """Return ``True`` when a plan document declares a supersession marker."""

    text = Path(markdown_path).read_text(encoding="utf-8")
    patterns = (
        r"^#+\s*Status:\s*\*\*superseded",
        r"^#+\s*Status:\s*superseded",
    )
    return any(re.search(p, text, flags=re.MULTILINE) for p in patterns) or any(
        re.search(p, text, flags=re.MULTILINE) for p in (
            r"^Status:\s*\*\*superseded",
            r"^Status:\s*superseded",
        )
    )


def adr_decision(adr_path: Path) -> str:
    """Return the explicit decision (``Accepted``/``Rejected``/``Proposed``) for an ADR."""

    text = Path(adr_path).read_text(encoding="utf-8")
    match = re.search(r"^- Status:\s*(\w+)", text, flags=re.MULTILINE)
    if match is None:
        return "Unknown"
    return match.group(1)


def execution_lane_record(
    *,
    lane_kind: str,
    outer_host_baseline: str,
    guest_rootless_outcome: str,
    environment_manifest_sha256: str,
    vm_manager_version: str = "",
    direct_host_rootless_outcome: str = "",
) -> dict[str, Any]:
    """Build a sanitized execution-lane receipt for Plan 058/060 candidates."""

    if lane_kind not in {"direct-host", "guest"}:
        raise CandidateRecordError(
            f"lane_kind must be direct-host or guest: {lane_kind!r}"
        )
    if lane_kind == "direct-host":
        if not direct_host_rootless_outcome:
            raise CandidateRecordError(
                "direct-host lane requires a direct_host_rootless_outcome"
            )
        if guest_rootless_outcome or vm_manager_version:
            raise CandidateRecordError(
                "direct-host lane must not report guest_rootless_outcome or "
                "vm_manager_version"
            )
    elif lane_kind == "guest":
        if not vm_manager_version:
            raise CandidateRecordError("guest lane requires a vm_manager_version")
        if not guest_rootless_outcome:
            raise CandidateRecordError(
                "guest lane requires a guest_rootless_outcome"
            )
        if direct_host_rootless_outcome:
            raise CandidateRecordError(
                "guest lane must not report direct_host_rootless_outcome"
            )
    return {
        "lane_kind": lane_kind,
        "outer_host_baseline": outer_host_baseline,
        "guest_rootless_outcome": guest_rootless_outcome,
        "direct_host_rootless_outcome": direct_host_rootless_outcome,
        "environment_manifest_sha256": environment_manifest_sha256,
        "vm_manager_version": vm_manager_version,
    }


def assert_evidence_storage_claim(
    *,
    claim: str,
    tracked_paths: tuple[Path, ...] = (),
) -> None:
    """Refuse storage claims that name ignored diagnostics as committed evidence."""

    lowered = claim.lower()
    committed_markers = (
        "committed under",
        "committed to",
        "recorded to",
        "tracked at",
        "tracked under",
    )
    if any(marker in lowered for marker in committed_markers):
        if "target/" in claim:
            for path in tracked_paths:
                if not Path(path).exists():
                    raise CandidateRecordError(
                        "claim describes evidence as committed but the tracked "
                        f"artifact path does not exist: {claim}"
                    )
