"""Strict parser and verifier for source-build cache metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path


class MetadataError(ValueError):
    """A cache metadata file is malformed or does not describe its cache."""


_HEX40 = re.compile(r"^[0-9a-f]{40}$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_REFERENCE = re.compile(r"^(?:java_i2p|i2pd)$")

REQUIRED_KEYS = frozenset(
    {
        "schema",
        "reference",
        "source_revision",
        "source_repository",
        "lock_sha256",
        "build_command_version",
        "host_contract",
        "artifact_sha256",
        "artifact_path",
        "installed_tree_sha256",
        "launcher",
        "execution_network",
        "toolchain",
        "launcher_probe",
        "version_check",
        "test_disposition",
    }
)


@dataclass(frozen=True)
class CacheMetadata:
    schema: int
    reference: str
    source_revision: str
    source_repository: str
    lock_sha256: str
    build_command_version: str
    host_contract: str
    artifact_sha256: str
    artifact_path: str
    installed_tree_sha256: str
    launcher: str
    execution_network: str
    toolchain: str
    launcher_probe: str
    version_check: str
    test_disposition: str


def _parse_pairs(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as exc:
        raise MetadataError("metadata is not readable UTF-8") from exc
    for line_number, line in enumerate(lines, 1):
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise MetadataError(f"metadata line {line_number} is not key=value")
        key, value = line.split("=", 1)
        if not key or key in values:
            raise MetadataError(f"duplicate or empty metadata key at line {line_number}")
        values[key] = value
    if set(values) != REQUIRED_KEYS:
        unknown = sorted(set(values) - REQUIRED_KEYS)
        missing = sorted(REQUIRED_KEYS - set(values))
        raise MetadataError(f"metadata shape drift: unknown={unknown}, missing={missing}")
    return values


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as exc:
        raise MetadataError(f"metadata hash input is missing: {path}") from exc
    return digest.hexdigest()


def hash_runtime_tree(cache_root: Path) -> str:
    """Hash every runtime file in stable path order, excluding metadata itself."""

    if not cache_root.is_dir():
        raise MetadataError("cache root is not a directory")
    entries: list[tuple[str, str]] = []
    for path in sorted(path for path in cache_root.rglob("*") if path.is_file()):
        if path.name == "build-metadata.txt":
            continue
        relative = path.relative_to(cache_root).as_posix()
        entries.append((relative, _sha256_file(path)))
    canonical = "".join(f"{digest}  {relative}\n" for relative, digest in entries).encode()
    return hashlib.sha256(canonical).hexdigest()


def parse_metadata(
    path: Path,
    *,
    selected_reference: str | None = None,
    cache_root: Path | None = None,
    expected_lock_sha256: str | None = None,
    expected_host_contract: str | None = None,
) -> CacheMetadata:
    values = _parse_pairs(path)
    try:
        schema = int(values["schema"])
    except ValueError as exc:
        raise MetadataError("metadata schema is not an integer") from exc
    if schema != 2:
        raise MetadataError("unsupported metadata schema")
    reference = values["reference"]
    if not _REFERENCE.fullmatch(reference):
        raise MetadataError("non-canonical reference identifier")
    if selected_reference is not None and reference != selected_reference:
        raise MetadataError("metadata reference does not match selected reference")
    revision = values["source_revision"]
    if not _HEX40.fullmatch(revision):
        raise MetadataError("source revision is not a full object ID")
    for key in ("lock_sha256", "artifact_sha256", "installed_tree_sha256"):
        if not _HEX64.fullmatch(values[key]):
            raise MetadataError(f"{key} is not a SHA-256 digest")
    if expected_lock_sha256 is not None and values["lock_sha256"] != expected_lock_sha256:
        raise MetadataError("metadata lock digest mismatch")
    if expected_host_contract is not None and values["host_contract"] != expected_host_contract:
        raise MetadataError("metadata host contract mismatch")
    if values["execution_network"] != "forbidden":
        raise MetadataError("cache metadata permits execution networking")
    launcher = values["launcher"]
    if not launcher or Path(launcher).is_absolute() or "\\" in launcher:
        raise MetadataError("launcher is not a relative POSIX path")
    if cache_root is not None:
        cache_root = cache_root.resolve()
        launcher_path = (cache_root / launcher).resolve()
        if launcher_path != cache_root and cache_root not in launcher_path.parents:
            raise MetadataError("launcher escapes cache root")
        if not launcher_path.is_file() or not launcher_path.stat().st_mode & 0o111:
            raise MetadataError("launcher is missing or not executable")
        artifact_path = (cache_root / values["artifact_path"]).resolve()
        if artifact_path != cache_root and cache_root not in artifact_path.parents:
            raise MetadataError("artifact escapes cache root")
        if _sha256_file(artifact_path) != values["artifact_sha256"]:
            raise MetadataError("artifact hash mismatch")
        if hash_runtime_tree(cache_root) != values["installed_tree_sha256"]:
            raise MetadataError("installed runtime tree hash mismatch")
    return CacheMetadata(
        schema=schema,
        reference=reference,
        source_revision=revision,
        source_repository=values["source_repository"],
        lock_sha256=values["lock_sha256"],
        build_command_version=values["build_command_version"],
        host_contract=values["host_contract"],
        artifact_sha256=values["artifact_sha256"],
        artifact_path=values["artifact_path"],
        installed_tree_sha256=values["installed_tree_sha256"],
        launcher=launcher,
        execution_network=values["execution_network"],
        toolchain=values["toolchain"],
        launcher_probe=values["launcher_probe"],
        version_check=values["version_check"],
        test_disposition=values["test_disposition"],
    )


def _main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--cache-root", type=Path, required=True)
    parser.add_argument("--reference", required=True)
    parser.add_argument("--lock-sha256", required=True)
    parser.add_argument("--host-contract", required=True)
    args = parser.parse_args()
    try:
        value = parse_metadata(
            args.metadata,
            selected_reference=args.reference,
            cache_root=args.cache_root,
            expected_lock_sha256=args.lock_sha256,
            expected_host_contract=args.host_contract,
        )
    except MetadataError as exc:
        parser.error(str(exc))
    print(json.dumps(value.__dict__, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(_main())
