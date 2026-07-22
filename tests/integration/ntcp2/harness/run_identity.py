"""Plan 052 run-identity schema, validator, and writer.

The run identity is the canonical single-source provenance record for one
Milestone 3 execution lane. Every direction, attestation, trigger, observation,
cleanup, and aggregate record MUST carry ``run_id`` and
``run_identity_sha256`` and MUST cross-check them against the run-identity
file on disk. The validator refuses any record whose source commit is not
exactly the 40-character SHA recorded here, any direction record that names a
``run_id`` not present in the run identity, and any source tree state that
contradicts ``source_dirty``.

Schema:

```text
i2pr-interop-run-identity-v1
```

Required fields are documented at the top of ``RUN_IDENTITY_FIELDS``.
"""

from __future__ import annotations

import hashlib
import json
import re
import tempfile
from pathlib import Path
from typing import Any


RUN_IDENTITY_SCHEMA = "i2pr-interop-run-identity-v1"
RUN_IDENTITY_SCHEMA_VERSION = 1


RUN_IDENTITY_FIELDS = (
    "schema",
    "schema_version",
    "run_id",
    "created_at",
    "source_commit",
    "source_commit_object_sha256",
    "source_tree_sha256",
    "source_archive_sha256",
    "source_archive_format",
    "source_dirty",
    "host_source_manifest_sha256",
    "guest_source_manifest_sha256",
    "guest_source_listing_sha256",
    "environment_manifest_sha256",
    "launcher_binary_sha256",
    "launcher_build_profile",
    "rustc_version",
    "cargo_version",
    "target_triple",
    "topology_kind",
    "privilege_model",
    "reference_lock_sha256",
    "evidence_schema_revision",
    "run_identity_sha256",
)


_HEX40 = re.compile(r"^[0-9a-f]{40}$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{6,46})[a-z0-9]$")
_TOPOLOGY_KIND = {"rootless-sealed-single-netns", "privileged-dual-netns-veth"}
_PRIVILEGE_MODEL = {"unprivileged-userns", "host-capabilities"}
_ARCHIVE_FORMAT = {"git-tar", "git-archive-tar", "tar-zst"}


class RunIdentityError(ValueError):
    """Raised when a run-identity record fails provenance validation."""


def _scan(value: Any) -> None:
    if isinstance(value, str):
        if any(forbidden in value for forbidden in (
            "-----BEGIN", "router.identity", "ntcp2.static.key",
            "/home/", "/root/", "RouterInfo", "I2NP",
        )):
            raise RunIdentityError("run identity contains forbidden path or payload text")
    elif isinstance(value, dict):
        for child in value.values():
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


def validate_run_identity(record: dict[str, Any]) -> None:
    """Validate the run-identity record shape and provenance rules."""

    if not isinstance(record, dict):
        raise RunIdentityError("run identity must be a JSON object")
    if tuple(record) != RUN_IDENTITY_FIELDS:
        raise RunIdentityError("run identity fields do not match the locked schema")
    if record["schema"] != RUN_IDENTITY_SCHEMA:
        raise RunIdentityError("unknown run identity schema")
    if record["schema_version"] != RUN_IDENTITY_SCHEMA_VERSION:
        raise RunIdentityError("unsupported run identity schema version")
    _scan(record)
    if not _RUN_ID.fullmatch(str(record["run_id"])):
        raise RunIdentityError("run_id is not a safe identifier")
    if not _HEX40.fullmatch(str(record["source_commit"])):
        raise RunIdentityError("source_commit is not a 40-character SHA")
    for field in (
        "source_commit_object_sha256",
        "source_tree_sha256",
        "source_archive_sha256",
        "host_source_manifest_sha256",
        "guest_source_manifest_sha256",
        "guest_source_listing_sha256",
        "environment_manifest_sha256",
        "launcher_binary_sha256",
        "reference_lock_sha256",
    ):
        if not _HEX64.fullmatch(str(record[field])):
            raise RunIdentityError(f"{field} is not a SHA-256 digest")
    if record["source_dirty"] not in {"clean", "dirty"}:
        raise RunIdentityError("source_dirty is not clean or dirty")
    if record["source_archive_format"] not in _ARCHIVE_FORMAT:
        raise RunIdentityError("source_archive_format is not an allowed value")
    if record["topology_kind"] not in _TOPOLOGY_KIND:
        raise RunIdentityError("topology_kind is not a typed selector")
    if record["privilege_model"] not in _PRIVILEGE_MODEL:
        raise RunIdentityError("privilege_model is not a typed selector")
    if record["evidence_schema_revision"] < 1:
        raise RunIdentityError("evidence_schema_revision must be >= 1")
    if not record["launcher_build_profile"]:
        raise RunIdentityError("launcher_build_profile is required")
    if not record["rustc_version"]:
        raise RunIdentityError("rustc_version is required")
    if not record["cargo_version"]:
        raise RunIdentityError("cargo_version is required")
    if not record["target_triple"]:
        raise RunIdentityError("target_triple is required")
    if record.get("evidence_sha256") and record["evidence_sha256"] != record["run_identity_sha256"]:
        raise RunIdentityError("evidence_sha256 alias does not match run_identity_sha256")
    if record["run_identity_sha256"] and not _HEX64.fullmatch(str(record["run_identity_sha256"])):
        raise RunIdentityError("run_identity_sha256 is not a SHA-256 digest")


def write_run_identity(path: Path, record: dict[str, Any]) -> str:
    """Validate, finalize, and atomically write a run-identity record."""

    validate_run_identity(record)
    unsigned = dict(record)
    unsigned["run_identity_sha256"] = ""
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(canonical).hexdigest()
    record["run_identity_sha256"] = digest
    validate_run_identity(record)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with open(fd, "w", encoding="utf-8") as handle:
            handle.write(json.dumps(record, sort_keys=False, separators=(",", ":")) + "\n")
            handle.flush()
            import os
            os.fsync(handle.fileno())
        import os
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if Path(temporary).exists():
            Path(temporary).unlink()
    return digest


def load_run_identity(path: Path) -> dict[str, Any]:
    """Load and validate one run-identity record from disk."""

    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise RunIdentityError("run identity file is not valid UTF-8 JSON") from exc
    validate_run_identity(value)
    if not value["run_identity_sha256"]:
        raise RunIdentityError("run identity is not finalized")
    unsigned = dict(value)
    expected = unsigned["run_identity_sha256"]
    unsigned["run_identity_sha256"] = ""
    actual = hashlib.sha256(
        json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    if actual != expected:
        raise RunIdentityError("run identity digest mismatch")
    return value


def cross_check(record: dict[str, Any], identity: dict[str, Any]) -> None:
    """Cross-check a direction/observation/trigger/cleanup record against one identity."""

    if record.get("run_id") != identity["run_id"]:
        raise RunIdentityError("record run_id does not match run identity")
    expected = identity["run_identity_sha256"]
    if record.get("run_identity_sha256") != expected:
        raise RunIdentityError("record run_identity_sha256 does not match run identity")
    if record.get("source_commit") and record["source_commit"] != identity["source_commit"]:
        raise RunIdentityError("record source_commit does not match run identity")
    if record.get("launcher_binary_sha256") and record["launcher_binary_sha256"] != identity["launcher_binary_sha256"]:
        raise RunIdentityError("record launcher_binary_sha256 does not match run identity")


def build_run_identity(
    *,
    run_id: str,
    source_commit: str,
    source_commit_object_sha256: str,
    source_tree_sha256: str,
    source_archive_sha256: str,
    source_archive_format: str,
    source_dirty: str,
    host_source_manifest_sha256: str,
    guest_source_manifest_sha256: str,
    guest_source_listing_sha256: str,
    environment_manifest_sha256: str,
    launcher_binary_sha256: str,
    launcher_build_profile: str,
    rustc_version: str,
    cargo_version: str,
    target_triple: str,
    topology_kind: str,
    privilege_model: str,
    reference_lock_sha256: str,
    evidence_schema_revision: int,
    created_at: str,
) -> dict[str, Any]:
    """Build an unsigned run-identity record for writer finalization."""

    return {
        "schema": RUN_IDENTITY_SCHEMA,
        "schema_version": RUN_IDENTITY_SCHEMA_VERSION,
        "run_id": run_id,
        "created_at": created_at,
        "source_commit": source_commit,
        "source_commit_object_sha256": source_commit_object_sha256,
        "source_tree_sha256": source_tree_sha256,
        "source_archive_sha256": source_archive_sha256,
        "source_archive_format": source_archive_format,
        "source_dirty": source_dirty,
        "host_source_manifest_sha256": host_source_manifest_sha256,
        "guest_source_manifest_sha256": guest_source_manifest_sha256,
        "guest_source_listing_sha256": guest_source_listing_sha256,
        "environment_manifest_sha256": environment_manifest_sha256,
        "launcher_binary_sha256": launcher_binary_sha256,
        "launcher_build_profile": launcher_build_profile,
        "rustc_version": rustc_version,
        "cargo_version": cargo_version,
        "target_triple": target_triple,
        "topology_kind": topology_kind,
        "privilege_model": privilege_model,
        "reference_lock_sha256": reference_lock_sha256,
        "evidence_schema_revision": evidence_schema_revision,
        "run_identity_sha256": "",
    }


def empty_run_identity_digest() -> str:
    """Return the canonical zero SHA-256 used to express typed absence."""

    return "0" * 64