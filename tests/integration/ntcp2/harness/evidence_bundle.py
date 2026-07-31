"""Plan 052 atomic evidence-bundle writer.

Each accepted Milestone 3 run produces a sanitized bundle under
``target/interop/evidence/milestone-3/<run-id>/``. The bundle layout mirrors
Plan 052 C1:

```text
run-identity.json
environment/
  environment.json
  source-transfer.json
  cache-transfer.json
  offline-transition.json
  parent-network-before.sha256
  parent-network-after.sha256
attestations/
  <direction>.json
directions/
  <direction>.json
triggers/
  <direction>.json
observations/
  <direction>.json
cleanup/
  <direction>.json
diagnostics/
  sanitized-summary.json
manifest.json
manifest.sha256
```

The bundle is built incrementally under a per-run staging directory. Each
file is finalized before manifest generation. The host-side export copies
the entire staging directory to a temporary target, verifies every hash
against the bundle manifest, and then atomically renames the temporary
directory to the final run directory. An interrupted export leaves a typed
incomplete staging directory and never overwrites an older valid bundle.

Allowed environment block:

- ``environment.json`` — environment record.
- ``source-transfer.json`` — host→guest source transfer record.
- ``cache-transfer.json`` — cache transfer record.
- ``offline-transition.json`` — offline transition record.
- ``parent-network-before.sha256`` / ``parent-network-after.sha256`` —
  parent network state digests.

Allowed per-direction classes: ``attestations``, ``directions``, ``triggers``,
``observations``, ``cleanup``. Each class must contain one entry per declared
direction.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import stat
import tempfile
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


BUNDLE_SCHEMA = "i2pr-interop-evidence-bundle-v1"
BUNDLE_SCHEMA_VERSION = 1

BUNDLE_EXPORT_ACK_SCHEMA = "i2pr-interop-bundle-export-ack-v1"
BUNDLE_EXPORT_ACK_VERSION = 1

PRIMARY_DIRECTIONS: tuple[str, ...] = (
    "i2pr-to-java-ipv4",
    "java-to-i2pr-ipv4",
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
)

DIRECTION_CLASSES: tuple[str, ...] = (
    "attestations",
    "directions",
    "triggers",
    "observations",
    "cleanup",
    "events",
)

ENVIRONMENT_CLASSES: tuple[str, ...] = (
    "environment.json",
    "source-transfer.json",
    "cache-transfer.json",
    "offline-transition.json",
    "parent-network-before.sha256",
    "parent-network-after.sha256",
)

SEMANTIC_SCHEMAS: dict[str, tuple[str, ...]] = {
    "attestations": (
        "i2pr-mixed-router-attestation-v2",
        "i2pr-mixed-router-attestation-v1",
    ),
    "directions": (
        "i2pr-mixed-router-direction-v2",
        "i2pr-mixed-router-direction-v1",
    ),
    "triggers": (
        "i2pr-reference-trigger-v4",
        "i2pr-reference-trigger-v3",
        "i2pr-reference-trigger-v2",
        "i2pr-reference-trigger-v1",
    ),
    "observations": (
        "i2pr-ntcp2-direction-observation-v3",
        "i2pr-ntcp2-direction-observation-v2",
    ),
    "cleanup": (
        "i2pr-mixed-router-cleanup-v2",
        "i2pr-mixed-router-cleanup-v1",
    ),
    "events": (
        "i2pr-reference-event-v1",
    ),
    "run-identity.json": (
        "i2pr-interop-run-identity-v1",
    ),
}


class BundleError(ValueError):
    """Raised when an evidence bundle fails validation."""


_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{6,46})[a-z0-9]$")


@dataclass
class BundleFile:
    relative_path: str
    size: int
    sha256: str
    record_type: str
    schema: str


@dataclass
class BundleManifest:
    run_id: str
    files: list[BundleFile] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema": BUNDLE_SCHEMA,
            "schema_version": BUNDLE_SCHEMA_VERSION,
            "type": "evidence-bundle-manifest",
            "run_id": self.run_id,
            "files": [
                {
                    "relative_path": entry.relative_path,
                    "size": entry.size,
                    "sha256": entry.sha256,
                    "record_type": entry.record_type,
                    "schema": entry.schema,
                }
                for entry in self.files
            ],
        }


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _validate_run_id(run_id: str) -> None:
    if not _RUN_ID.fullmatch(run_id):
        raise BundleError(f"run_id {run_id!r} is not a safe identifier")


def classify_bundle_file(
    staging_root: Path, path: Path, *, exists: bool = True,
) -> tuple[str, str]:
    """Return ``(record_type, schema)`` for a file under the staging tree.

    Validates that *path* is inside *staging_root*, is a single-level or
    two-level relative path matching the allowlisted layout, and raises
    ``BundleError`` for traversal, absolute, unknown, or too-deep paths.
    """
    root = staging_root.resolve()
    if exists:
        candidate = path.resolve(strict=True)
    else:
        candidate = path.resolve()
    try:
        relative = candidate.relative_to(root)
    except ValueError:
        raise BundleError(f"path is not inside staging root: {path}")

    rel_str = relative.as_posix()
    if rel_str.startswith("/"):
        raise BundleError(f"absolute path rejected: {rel_str}")

    parts = relative.parts
    name = relative.name

    if name in {"manifest.json", "manifest.sha256"}:
        return "manifest", "manifest"
    if name == "run-identity.json" and len(parts) == 1:
        return "run-identity", "run-identity"
    if rel_str.startswith("environment/"):
        return "environment", "environment-record"
    if len(parts) == 2 and parts[0] in DIRECTION_CLASSES:
        if name.endswith(".json"):
            return parts[0], f"{parts[0]}-record"
        raise BundleError(
            f"non-JSON file in direction class {parts[0]}: {rel_str}"
        )
    if len(parts) == 2 and parts[0] == "diagnostics":
        return "diagnostics", "sanitized-summary"

    raise BundleError(f"unknown bundle path: {rel_str}")


def _validate_file_tree(staging_root: Path) -> None:
    """Reject symlinks, non-regular files, hard links >1, hidden temp files,
    and case-colliding paths in the staging tree."""
    staging_root = staging_root.resolve()
    seen_lower: dict[str, str] = {}
    for path in sorted(staging_root.rglob("*")):
        rel = path.relative_to(staging_root).as_posix()

        if path.is_symlink():
            raise BundleError(f"symlink rejected: {rel}")

        if path.is_dir():
            continue

        try:
            stat_result = path.stat()
        except OSError as exc:
            raise BundleError(f"cannot stat {rel}: {exc}") from exc

        if not stat.S_ISREG(stat_result.st_mode):
            raise BundleError(f"non-regular file rejected: {rel}")

        if stat_result.st_nlink > 1:
            raise BundleError(f"hard link count >1 rejected: {rel}")

        if rel.startswith(".") or "/." in rel:
            raise BundleError(f"hidden/temp file rejected: {rel}")

        lower_rel = rel.lower()
        if lower_rel in seen_lower:
            raise BundleError(
                f"case-colliding paths rejected: {rel} vs {seen_lower[lower_rel]}"
            )
        seen_lower[lower_rel] = rel


def _record_schema(payload: dict[str, Any]) -> str:
    schema = payload.get("schema")
    if isinstance(schema, str):
        return schema
    record_type = payload.get("type")
    if isinstance(record_type, str):
        return record_type
    return "unknown"


def hash_file(path: Path) -> str:
    return _sha256_bytes(path.read_bytes())


def _scan_bundle_value(value: Any) -> None:
    if isinstance(value, str):
        if any(forbidden in value for forbidden in (
            "-----BEGIN", "router.identity", "ntcp2.static.key",
            "/home/", "/root/", "RouterInfo", "I2NP",
        )):
            raise BundleError("bundle contains forbidden path or payload text")
    elif isinstance(value, dict):
        for child in value.values():
            _scan_bundle_value(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan_bundle_value(child)


def write_json_atomic(path: Path, payload: dict[str, Any]) -> str:
    """Serialize *payload* to JSON, write atomically, return SHA-256 of exact bytes."""
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    encoded = (
        json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n"
    ).encode("utf-8")
    digest = hashlib.sha256(encoded).hexdigest()
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if Path(temporary).exists():
            Path(temporary).unlink()
    return digest


def load_bundle_manifest(path: Path) -> BundleManifest:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise BundleError("bundle manifest is not valid UTF-8 JSON") from exc
    if not isinstance(value, dict):
        raise BundleError("bundle manifest must be a JSON object")
    if value.get("schema") != BUNDLE_SCHEMA:
        raise BundleError("unknown bundle manifest schema")
    if value.get("schema_version") != BUNDLE_SCHEMA_VERSION:
        raise BundleError("unsupported bundle manifest schema version")
    run_id = value.get("run_id", "")
    _validate_run_id(run_id)
    files = value.get("files", [])
    if not isinstance(files, list):
        raise BundleError("bundle files must be a list")
    manifest = BundleManifest(run_id=run_id)
    for entry in files:
        if not isinstance(entry, dict):
            raise BundleError("bundle entry must be a JSON object")
        rel = entry.get("relative_path", "")
        sha = entry.get("sha256", "")
        size = entry.get("size", 0)
        if not isinstance(rel, str) or not isinstance(sha, str) or not isinstance(size, int):
            raise BundleError("bundle entry has invalid types")
        record_type = entry.get("record_type", "unknown")
        schema = entry.get("schema", "unknown")
        if record_type == "unknown":
            raise BundleError(f"manifest entry {rel} has unknown record_type")
        if schema == "unknown":
            raise BundleError(f"manifest entry {rel} has unknown schema")
        if not rel or Path(rel).is_absolute() or "\\" in rel:
            raise BundleError(f"bundle entry {rel!r} is not a normalized relative path")
        try:
            classify_bundle_file(path.parent, path.parent / rel, exists=False)
        except BundleError as exc:
            raise BundleError(f"invalid bundle entry path: {rel}") from exc
        if rel in {"manifest.json", "manifest.sha256"}:
            raise BundleError("manifest cannot declare itself")
        if any(part in {"", ".", ".."} for part in Path(rel).parts):
            raise BundleError(f"bundle entry {rel} contains traversal")
        if any(existing.relative_path == rel for existing in manifest.files):
            raise BundleError(f"duplicate bundle entry: {rel}")
        if not _HEX64.fullmatch(sha):
            raise BundleError(f"bundle entry {rel} has invalid sha256")
        manifest.files.append(BundleFile(
            relative_path=rel,
            size=size,
            sha256=sha,
            record_type=record_type,
            schema=schema,
        ))
    return manifest


def _validate_manifest_sha256(staging_root: Path) -> None:
    """Verify ``manifest.sha256`` contains the correct digest of ``manifest.json``."""
    sha_path = staging_root / "manifest.sha256"
    manifest_path = staging_root / "manifest.json"
    try:
        sha_text = sha_path.read_text(encoding="ascii")
    except (OSError, UnicodeDecodeError) as exc:
        raise BundleError("manifest.sha256 is not valid ASCII") from exc
    lines = sha_text.splitlines()
    if len(lines) != 1:
        raise BundleError("manifest.sha256 must contain exactly one line")
    line = lines[0]
    expected_format = re.compile(r"^[0-9a-f]{64}  manifest\.json$")
    if not expected_format.fullmatch(line):
        raise BundleError("manifest.sha256 has invalid format")
    declared_digest = line[:64]
    actual_digest = _sha256_bytes(manifest_path.read_bytes())
    if declared_digest != actual_digest:
        raise BundleError(
            f"manifest.sha256 digest mismatch: declared={declared_digest} actual={actual_digest}"
        )


def build_bundle_manifest(staging_root: Path, run_id: str) -> BundleManifest:
    """Walk the staging directory and produce a BundleManifest."""

    _validate_run_id(run_id)
    _validate_file_tree(staging_root)
    manifest = BundleManifest(run_id=run_id)
    seen_rels: set[str] = set()
    for path in sorted(staging_root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(staging_root).as_posix()
        if rel in {"manifest.json", "manifest.sha256"}:
            continue
        if rel in seen_rels:
            raise BundleError(f"duplicate manifest entry: {rel}")
        seen_rels.add(rel)
        try:
            size = path.stat().st_size
        except OSError as exc:
            raise BundleError(f"cannot stat {rel}: {exc}") from exc
        sha = hash_file(path)
        record_type, _ = classify_bundle_file(staging_root, path)
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
            schema = _record_schema(payload)
        except (OSError, UnicodeError, json.JSONDecodeError):
            if record_type in SEMANTIC_SCHEMAS:
                raise BundleError(
                    f"{rel} is in class {record_type} but is not valid JSON"
                )
            schema = "binary-or-text"
        manifest.files.append(BundleFile(
            relative_path=rel,
            size=size,
            sha256=sha,
            record_type=record_type,
            schema=schema,
        ))
    return manifest


def _validate_semantic_schemas(staging_root: Path, manifest: BundleManifest) -> None:
    """For direction-class JSON files, compare path class to payload schema/type."""
    for entry in manifest.files:
        if entry.record_type not in SEMANTIC_SCHEMAS:
            continue
        allowed = SEMANTIC_SCHEMAS[entry.record_type]
        if entry.schema not in allowed:
            raise BundleError(
                f"{entry.relative_path} is in class {entry.record_type} "
                f"but declares schema {entry.schema!r}; expected one of {allowed}"
            )


def write_bundle_manifest(staging_root: Path, manifest: BundleManifest) -> Path:
    """Write ``manifest.json`` + ``manifest.sha256`` under the staging root."""

    payload = manifest.to_dict()
    manifest_path = staging_root / "manifest.json"
    write_json_atomic(manifest_path, payload)
    digest = _sha256_bytes(manifest_path.read_bytes())
    digest_path = staging_root / "manifest.sha256"
    digest_path.write_text(f"{digest}  manifest.json\n", encoding="ascii")
    os.chmod(digest_path, 0o600)
    return manifest_path


def verify_bundle(staging_root: Path) -> BundleManifest:
    """Verify every file in the staging directory matches the manifest."""

    _validate_file_tree(staging_root)
    _validate_manifest_sha256(staging_root)
    manifest = load_bundle_manifest(staging_root / "manifest.json")
    _validate_semantic_schemas(staging_root, manifest)
    declared = {entry.relative_path: entry for entry in manifest.files}
    on_disk = set()
    for path in sorted(staging_root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(staging_root).as_posix()
        if rel in {"manifest.json", "manifest.sha256"}:
            continue
        on_disk.add(rel)
        if rel not in declared:
            raise BundleError(f"unexpected file in staging directory: {rel}")
        entry = declared[rel]
        try:
            size = path.stat().st_size
        except OSError as exc:
            raise BundleError(f"cannot stat {rel}: {exc}") from exc
        if size != entry.size:
            raise BundleError(f"size mismatch for {rel}: {size} != {entry.size}")
        if hash_file(path) != entry.sha256:
            raise BundleError(f"sha256 mismatch for {rel}")
    missing = set(declared) - on_disk
    if missing:
        raise BundleError(f"manifest references missing files: {sorted(missing)}")
    return manifest


def export_bundle_atomic(staging_root: Path, export_root: Path) -> Path:
    """Copy ``staging_root`` to ``export_root`` atomically and verify hashes.

    The export flow is:

    1. Verify the staging directory.
    2. Copy to a temporary directory under ``export_root.parent``.
    3. Verify hashes against the staging manifest.
    4. Atomically rename the temporary directory to the final ``export_root``.
    5. Write an export acknowledgement BESIDE (not inside) the bundle.

    Returns the final exported bundle path.
    """

    staging_root = staging_root.resolve()
    export_root = export_root.resolve()
    if not staging_root.exists():
        raise BundleError(f"staging root does not exist: {staging_root}")
    if export_root.exists():
        raise BundleError(f"export target already exists: {export_root}")
    manifest = verify_bundle(staging_root)
    parent = export_root.parent
    parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, temporary_path = tempfile.mkstemp(prefix=f".{export_root.name}.", dir=parent)
    os.close(fd)
    temp_dir = Path(temporary_path)
    if temp_dir.exists() or temp_dir.is_symlink():
        temp_dir.unlink()
    try:
        shutil.copytree(staging_root, temp_dir)
        for entry in manifest.files:
            src = staging_root / entry.relative_path
            dst = temp_dir / entry.relative_path
            if not dst.exists():
                raise BundleError(f"copy missing: {entry.relative_path}")
            if hash_file(dst) != entry.sha256:
                raise BundleError(f"copy hash mismatch: {entry.relative_path}")
            if dst.stat().st_size != entry.size:
                raise BundleError(f"copy size mismatch: {entry.relative_path}")
        os.replace(temp_dir, export_root)
    finally:
        if temp_dir.exists():
            shutil.rmtree(temp_dir, ignore_errors=True)
    export_ack_path = parent / f"{export_root.name}.export-ack.json"
    try:
        bundle_path = export_root.relative_to(parent.parent).as_posix()
    except ValueError:
        bundle_path = export_root.name
    acknowledgement = {
        "schema": BUNDLE_EXPORT_ACK_SCHEMA,
        "schema_version": BUNDLE_EXPORT_ACK_VERSION,
        "run_id": manifest.run_id,
        "bundle_path": bundle_path,
        "manifest_sha256": _sha256_bytes((export_root / "manifest.json").read_bytes()),
        "export_timestamp": datetime.now(timezone.utc).isoformat(),
        "verifier": "evidence_bundle.verify_bundle",
        "verifier_result": "passed",
    }
    write_json_atomic(export_ack_path, acknowledgement)
    return export_root


def validate_direction_catalog(staging_root: Path) -> None:
    """Verify exactly the four primary directions per DIRECTION_CLASSES.

    Raises BundleError if any direction class has a missing, extra, or
    substituted scenario id, or if the bundle lacks any of the four
    primary directions. The ``events`` class is allowlisted as
    optional because blocked or rejected directions may not emit any
    structured reference events and the certificate verifier only
    consumes the four mandatory direction classes.
    """

    for direction_class in DIRECTION_CLASSES:
        if direction_class == "events":
            # Plan 062 structured reference events are optional and
            # never required for the four primary direction records.
            continue
        directory = staging_root / direction_class
        if not directory.exists():
            raise BundleError(f"bundle is missing the {direction_class} directory")
        ids = sorted(path.stem for path in directory.glob("*.json"))
        if ids != sorted(PRIMARY_DIRECTIONS):
            raise BundleError(
                f"{direction_class} must contain exactly the primary catalog; found {ids}"
            )


def validate_environment_block(staging_root: Path) -> None:
    directory = staging_root / "environment"
    if not directory.exists():
        raise BundleError("bundle is missing the environment directory")
    for filename in ENVIRONMENT_CLASSES:
        if not (directory / filename).exists():
            raise BundleError(f"environment block is missing {filename}")


def has_typed_absence(payload: dict[str, Any]) -> bool:
    """Return True if ``payload`` uses Plan 052 typed absence for RouterInfo."""

    router_info = payload.get("router_info")
    if not isinstance(router_info, dict):
        return False
    state = router_info.get("state")
    sha = router_info.get("sha256")
    return state in {"not-produced", "not-applicable"} and sha is None


def finalize_bundle(staging_root: Path, run_id: str) -> BundleManifest:
    """Verify the staging tree, write the manifest, and return the manifest."""

    _validate_run_id(run_id)
    if (staging_root / "manifest.json").exists() or (staging_root / "manifest.sha256").exists():
        raise BundleError("finalized bundle is immutable")
    validate_environment_block(staging_root)
    validate_direction_catalog(staging_root)
    manifest = build_bundle_manifest(staging_root, run_id)
    _validate_semantic_schemas(staging_root, manifest)
    write_bundle_manifest(staging_root, manifest)
    return manifest


def expected_files(directions: Iterable[str]) -> set[str]:
    return {
        f"{direction_class}/{direction}.json"
        for direction_class in DIRECTION_CLASSES
        for direction in directions
    }
