"""Pure validation helpers for the Plan 043 build-system lane."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any

try:
    from .evidence import EvidenceError, validate_file
    from .metadata import MetadataError, parse_metadata
except ImportError:  # unittest discovery loads this directory as a flat path.
    from evidence import EvidenceError, validate_file  # type: ignore
    from metadata import MetadataError, parse_metadata  # type: ignore

HOST_CONTRACT = "ubuntu-24.04-amd64"

PROFILE_GATES: dict[str, tuple[str, ...]] = {
    "environment-smoke": ("environment-smoke",),
    "reference-crosscheck-ipv4": ("environment-smoke", "reference-crosscheck-ipv4"),
    "handshake-smoke": (
        "environment-smoke",
        "reference-crosscheck-ipv4",
        "handshake-smoke",
    ),
    "full": (
        "environment-smoke",
        "reference-crosscheck-ipv4",
        "handshake-smoke",
        "full",
    ),
}

# Plan 046: explicit rootless profiles that route through
# `rootless-sealed-single-netns` and require `unprivileged-userns`.
ROOTLESS_PROFILE_GATES: dict[str, tuple[str, ...]] = {
    "rootless-environment-smoke": ("rootless-environment-smoke",),
    "rootless-reference-crosscheck-ipv4": (
        "rootless-environment-smoke",
        "rootless-reference-crosscheck-ipv4",
    ),
    "rootless-handshake-smoke": (
        "rootless-environment-smoke",
        "rootless-reference-crosscheck-ipv4",
        "handshake-smoke-rootless",
    ),
}

GATE_SCENARIOS: dict[str, tuple[str, ...]] = {
    "environment-smoke": ("java-ipv4-inbound-outbound", "i2pd-ipv4-inbound-outbound"),
    "reference-crosscheck-ipv4": (
        "reference-java-i2pd-ipv4",
        "reference-i2pd-java-ipv4",
    ),
    "handshake-smoke": (
        "i2pr-to-java-ipv4",
        "java-to-i2pr-ipv4",
        "i2pr-to-i2pd-ipv4",
        "i2pd-to-i2pr-ipv4",
    ),
    "handshake-smoke-rootless": (
        "i2pr-to-java-ipv4",
        "java-to-i2pr-ipv4",
        "i2pr-to-i2pd-ipv4",
        "i2pd-to-i2pr-ipv4",
    ),
    "full": (
        "java-ipv4-inbound-outbound",
        "java-ipv6-inbound-outbound",
        "java-adversarial-and-resource",
        "java-duplicate-link-race",
        "i2pd-ipv4-inbound-outbound",
        "i2pd-ipv6-inbound-outbound",
        "i2pd-adversarial-and-resource",
        "i2pd-duplicate-link-race",
        "i2pr-to-java-ipv4",
        "java-to-i2pr-ipv4",
        "i2pr-to-i2pd-ipv4",
        "i2pd-to-i2pr-ipv4",
    ),
}

# Plan 046: gate catalog. Each entry declares the allowed topologies,
# required privilege model, sandbox attestation requirement, and the
# scenarios that gate must produce. The rootless gate is the primary
# evidence path; the privileged gate remains explicit and opt-in.
GATE_CATALOG: dict[str, dict[str, object]] = {
    "handshake-smoke-rootless": {
        "runner_type": "mixed",
        "allowed_topologies": ("rootless-sealed-single-netns",),
        "required_privilege_models": ("unprivileged-userns",),
        "requires_sandbox_attestation": True,
        "scenario_ids": GATE_SCENARIOS["handshake-smoke-rootless"],
        "allowed_evidence_schemas": (1,),
        "predecessor_gates": (
            "rootless-environment-smoke",
            "rootless-reference-crosscheck-ipv4",
        ),
    },
    "handshake-smoke": {
        "runner_type": "mixed",
        "allowed_topologies": ("privileged-dual-netns-veth",),
        "required_privilege_models": ("host-capabilities",),
        "requires_sandbox_attestation": False,
        "scenario_ids": GATE_SCENARIOS["handshake-smoke"],
        "allowed_evidence_schemas": (1,),
        "predecessor_gates": ("environment-smoke", "reference-crosscheck-ipv4"),
    },
}

_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_CACHE_KEY = re.compile(r"^[0-9a-f]{64}$")
_GATE_FILENAME = re.compile(r"^(environment-smoke|reference-crosscheck-ipv4|handshake-smoke|full)--[^/]+\.json$")


class BuildGateError(ValueError):
    """A build or aggregate manifest violates the locked gate contract."""


def gates_for_profile(profile: str) -> tuple[str, ...]:
    try:
        return PROFILE_GATES[profile]
    except KeyError as exc:
        raise BuildGateError("unknown workflow profile") from exc


def scenarios_for_gate(gate: str) -> tuple[str, ...]:
    try:
        return GATE_SCENARIOS[gate]
    except KeyError as exc:
        raise BuildGateError("unknown workflow gate") from exc


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as exc:
        raise BuildGateError(f"cannot hash required file: {path.name}") from exc
    return digest.hexdigest()


def _relative_repo_path(repo_root: Path, path: Path) -> str:
    try:
        relative = path.resolve().relative_to(repo_root.resolve())
    except ValueError as exc:
        raise BuildGateError("manifest path escapes repository") from exc
    if relative.is_absolute() or ".." in relative.parts:
        raise BuildGateError("manifest path is not repository-relative")
    return relative.as_posix()


def _load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise BuildGateError(f"invalid JSON manifest: {path.name}") from exc
    if not isinstance(value, dict):
        raise BuildGateError(f"JSON manifest is not an object: {path.name}")
    return value


def _selected_cache_paths(repo_root: Path, summary: dict[str, Any]) -> tuple[Path, ...]:
    if summary.get("schema") != 2 or summary.get("host_contract") != HOST_CONTRACT:
        raise BuildGateError("current cache summary contract mismatch")
    references = summary.get("references")
    if (
        not isinstance(references, list)
        or len(references) != 2
        or any(not isinstance(entry, dict) for entry in references)
        or {entry.get("reference") for entry in references} != {"java_i2p", "i2pd"}
    ):
        raise BuildGateError("current cache summary does not select both canonical references")
    selected: list[Path] = [repo_root / "target/interop/cache/current-cache.json"]
    lock_sha256 = hashlib.sha256(
        (repo_root / "tests/integration/ntcp2/references.lock.toml").read_bytes()
    ).hexdigest()
    for entry in references:
        if not isinstance(entry, dict) or not _CACHE_KEY.fullmatch(str(entry.get("cache_key", ""))):
            raise BuildGateError("current cache summary contains an invalid cache key")
        reference = entry.get("reference")
        if reference not in {"java_i2p", "i2pd"}:
            raise BuildGateError("current cache summary contains a non-canonical reference")
        cache = repo_root / "target/interop/cache" / reference / entry["cache_key"]
        metadata = cache / "build-metadata.txt"
        expected_metadata = Path(str(entry.get("metadata", "")))
        if expected_metadata != Path(_relative_repo_path(repo_root, metadata)):
            raise BuildGateError("current cache summary metadata path does not match its cache key")
        try:
            parsed = parse_metadata(
                metadata,
                selected_reference=reference,
                cache_root=cache,
                expected_lock_sha256=lock_sha256,
                expected_host_contract=HOST_CONTRACT,
            )
        except (MetadataError, OSError) as exc:
            raise BuildGateError("selected cache metadata is invalid") from exc
        if entry.get("artifact_sha256") != parsed.artifact_sha256 or entry.get("installed_tree_sha256") != parsed.installed_tree_sha256:
            raise BuildGateError("current cache summary hash does not match metadata")
        selected.append(cache)
    return tuple(selected)


def build_cache_manifest(repo_root: Path, output: Path) -> dict[str, Any]:
    """Build a manifest for exactly the two cache trees selected by current-cache.json."""

    summary_path = repo_root / "target/interop/cache/current-cache.json"
    summary = _load_json(summary_path)
    selected = _selected_cache_paths(repo_root, summary)
    files: list[dict[str, str]] = []
    for root in selected:
        if not root.is_file() and not root.is_dir():
            raise BuildGateError("selected cache path is missing")
        candidates = [root] if root.is_file() else sorted(path for path in root.rglob("*") if path.is_file())
        for path in candidates:
            if path.is_symlink():
                raise BuildGateError("cache manifest refuses symbolic links")
            files.append({"path": _relative_repo_path(repo_root, path), "sha256": _sha256(path)})
    files.sort(key=lambda value: value["path"])
    manifest = {
        "schema": 1,
        "host_contract": HOST_CONTRACT,
        "lock_sha256": hashlib.sha256(
            (repo_root / "tests/integration/ntcp2/references.lock.toml").read_bytes()
        ).hexdigest(),
        "cache_summary_sha256": _sha256(summary_path),
        "references": [
            {
                "reference": entry["reference"],
                "cache_key": entry["cache_key"],
                "artifact_sha256": entry["artifact_sha256"],
                "installed_tree_sha256": entry["installed_tree_sha256"],
            }
            for entry in sorted(summary["references"], key=lambda value: value["reference"])
        ],
        "files": files,
    }
    output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    output.chmod(0o600)
    return manifest


def validate_cache_manifest(repo_root: Path, manifest_path: Path) -> None:
    manifest = _load_json(manifest_path)
    if manifest.get("schema") != 1 or manifest.get("host_contract") != HOST_CONTRACT:
        raise BuildGateError("unsupported cache manifest")
    summary_path = repo_root / "target/interop/cache/current-cache.json"
    if manifest.get("cache_summary_sha256") != _sha256(summary_path):
        raise BuildGateError("cache summary digest mismatch")
    expected_lock = hashlib.sha256(
        (repo_root / "tests/integration/ntcp2/references.lock.toml").read_bytes()
    ).hexdigest()
    if manifest.get("lock_sha256") != expected_lock:
        raise BuildGateError("cache manifest lock digest mismatch")
    expected = build_cache_manifest(repo_root, manifest_path.with_suffix(".recomputed.json"))
    try:
        if expected != manifest:
            raise BuildGateError("selected cache file manifest mismatch")
        summary = _load_json(summary_path)
        recorded_manifest_digest = summary.get("cache_manifest_sha256")
        if recorded_manifest_digest is not None and recorded_manifest_digest != _sha256(manifest_path):
            raise BuildGateError("build summary cache manifest digest mismatch")
    finally:
        manifest_path.with_suffix(".recomputed.json").unlink(missing_ok=True)


def _record_paths(evidence_root: Path) -> list[tuple[str, Path]]:
    records: list[tuple[str, Path]] = []
    for path in sorted(evidence_root.glob("*.json")):
        if path.name == "run-manifest.json":
            continue
        if not _GATE_FILENAME.fullmatch(path.name):
            raise BuildGateError("evidence record is not assigned to a workflow gate")
        records.append((path.name.split("--", 1)[0], path))
    return records


def validate_aggregate_manifest(repo_root: Path, manifest_path: Path, profile: str | None = None) -> None:
    manifest = _load_json(manifest_path)
    selected_profile = profile or str(manifest.get("profile", ""))
    gates = gates_for_profile(selected_profile)
    expected_pairs = {(gate, scenario) for gate in gates for scenario in scenarios_for_gate(gate)}
    if manifest.get("schema") != 1 or manifest.get("profile") != selected_profile:
        raise BuildGateError("aggregate manifest schema or profile is invalid")
    if manifest.get("host_contract") != HOST_CONTRACT:
        raise BuildGateError("aggregate host contract is invalid")

    build_root = repo_root / "target/interop/build"
    host_metadata = build_root / "host-metadata.json"
    summary_path = build_root / "reference-build-summary.json"
    current_cache_path = repo_root / "target/interop/cache/current-cache.json"
    clean_marker = build_root / "clean-host-verification.json"
    try:
        summary = _load_json(summary_path)
        current_cache = _load_json(current_cache_path)
        clean = _load_json(clean_marker)
    except BuildGateError as exc:
        raise BuildGateError("aggregate inputs are missing or malformed") from exc
    if not host_metadata.is_file():
        raise BuildGateError("aggregate host metadata is missing")
    if summary.get("schema") != 1 or summary.get("host_contract") != HOST_CONTRACT:
        raise BuildGateError("reference build summary contract is invalid")
    expected_lock = hashlib.sha256(
        (repo_root / "tests/integration/ntcp2/references.lock.toml").read_bytes()
    ).hexdigest()
    if summary.get("lock_sha256") != expected_lock or current_cache.get("lock_sha256") != expected_lock:
        raise BuildGateError("aggregate lock digest does not match the repository lock")
    if summary.get("host_metadata_sha256") != _sha256(host_metadata):
        raise BuildGateError("reference build summary host digest mismatch")
    if current_cache.get("schema") != 2 or current_cache.get("host_contract") != HOST_CONTRACT:
        raise BuildGateError("current cache summary contract is invalid")
    if summary.get("references") != current_cache.get("references"):
        raise BuildGateError("reference build summary does not match current cache selection")
    _selected_cache_paths(repo_root, current_cache)
    if manifest.get("host_contract_digest") != _sha256(host_metadata):
        raise BuildGateError("aggregate host contract digest mismatch")
    if manifest.get("lock_sha256") != expected_lock:
        raise BuildGateError("aggregate lock digest mismatch")
    if manifest.get("reference_cache") != summary.get("references"):
        raise BuildGateError("aggregate cache metadata does not match build summary")
    expected_scenarios = {gate: list(scenarios_for_gate(gate)) for gate in gates}
    if manifest.get("expected_scenarios") != expected_scenarios:
        raise BuildGateError("aggregate expected scenario set is invalid")
    if manifest.get("per_gate_disposition") != {gate: "passed" for gate in gates}:
        raise BuildGateError("aggregate gate disposition is not fully passing")
    if clean.get("schema") != 1 or clean.get("result") != "clean" or manifest.get("cleanup_verification") != "clean":
        raise BuildGateError("aggregate cleanup verification is not clean")
    if not re.fullmatch(r"[0-9a-f]{40}", str(manifest.get("i2pr_commit", ""))):
        raise BuildGateError("aggregate commit is not an exact object ID")
    evidence_root = repo_root / "target/interop/evidence"
    records = _record_paths(evidence_root)
    actual_pairs: set[tuple[str, str]] = set()
    record_entries = manifest.get("records")
    if not isinstance(record_entries, list):
        raise BuildGateError("aggregate manifest records are not a list")
    if len(record_entries) != len(records):
        raise BuildGateError("aggregate manifest does not reference every evidence record")
    for gate, path in records:
        try:
            validate_file(path)
            value = _load_json(path)
        except (EvidenceError, BuildGateError) as exc:
            raise BuildGateError("aggregate contains invalid evidence") from exc
        scenario = str(value.get("scenario_id", ""))
        pair = (gate, scenario)
        if pair in actual_pairs or pair not in expected_pairs:
            raise BuildGateError("aggregate evidence coverage is incomplete or unexpected")
        actual_pairs.add(pair)
        matching = [entry for entry in record_entries if isinstance(entry, dict) and entry.get("filename") == path.name]
        if len(matching) != 1 or matching[0].get("gate") != gate or matching[0].get("scenario_id") != scenario:
            raise BuildGateError("aggregate record reference does not match evidence")
        if matching[0].get("sha256") != _sha256(path):
            raise BuildGateError("aggregate evidence digest mismatch")
        allowed = {"passed"}
        if selected_profile == "full" and gate == "full" and scenario.endswith("-ipv6-inbound-outbound"):
            allowed.add("skipped_ipv6")
        if value.get("actual_typed_result") not in allowed:
            raise BuildGateError("aggregate contains a non-passing gate result")
    if actual_pairs != expected_pairs:
        raise BuildGateError("aggregate is missing an expected scenario record")
    unsigned = dict(manifest)
    expected_digest = unsigned.get("aggregate_manifest_sha256")
    unsigned["aggregate_manifest_sha256"] = ""
    actual_digest = hashlib.sha256(json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if expected_digest != actual_digest or not _HEX64.fullmatch(str(expected_digest)):
        raise BuildGateError("aggregate manifest digest mismatch")
