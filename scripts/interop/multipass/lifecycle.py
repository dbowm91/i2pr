#!/usr/bin/env python3
"""Fail-closed host lifecycle primitives for the Plan 049 Multipass lane.

This module deliberately contains no Multipass or guest execution calls.  It
owns the small, deterministic part of the lifecycle contract that can be
tested without a VM: identifiers, state transitions, structured state
normalization, ownership proof, atomic records, and per-run locking.
"""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
import re
import secrets
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


SCHEMA_VERSION = 1
ENVIRONMENT_ID = "i2pr-plan048-rootless-v1"
MAX_INSTANCE_NAME = 63
MAX_ALLOCATION_ATTEMPTS = 16
PENDING = "pending"

LIFECYCLE_STATES = frozenset(
    {
        "reserved",
        "launching",
        "provisioning",
        "provisioned",
        "source_ready",
        "cache_ready",
        "source_and_cache_ready",
        "probe_passed",
        "offline_ready",
        "running",
        "exporting",
        "exported",
        "stopped",
        "destroying",
        "destroyed",
        "blocked",
        "abandoned",
    }
)

_TRANSITIONS: dict[str, frozenset[str]] = {
    "reserved": frozenset({"launching", "destroying", "blocked", "abandoned"}),
    "launching": frozenset({"provisioning", "destroying", "blocked", "abandoned"}),
    "provisioning": frozenset({"provisioned", "destroying", "blocked", "abandoned"}),
    "provisioned": frozenset({"source_ready", "cache_ready", "source_and_cache_ready", "probe_passed", "destroying", "blocked", "abandoned"}),
    "source_ready": frozenset({"cache_ready", "source_and_cache_ready", "probe_passed", "destroying", "blocked", "abandoned"}),
    "cache_ready": frozenset({"source_ready", "source_and_cache_ready", "probe_passed", "destroying", "blocked", "abandoned"}),
    "source_and_cache_ready": frozenset({"probe_passed", "offline_ready", "destroying", "blocked", "abandoned"}),
    "probe_passed": frozenset({"offline_ready", "source_ready", "source_and_cache_ready", "destroying", "blocked", "abandoned"}),
    "offline_ready": frozenset({"running", "probe_passed", "destroying", "blocked", "abandoned"}),
    "running": frozenset({"exporting", "stopped", "destroying", "blocked", "abandoned"}),
    "exporting": frozenset({"exported", "running", "destroying", "blocked", "abandoned"}),
    "exported": frozenset({"exported", "stopped", "destroying", "blocked"}),
    "stopped": frozenset({"destroying", "reserved", "launching", "blocked", "abandoned"}),
    "destroying": frozenset({"destroyed", "blocked"}),
    "destroyed": frozenset({"reserved"}),
    "blocked": frozenset({"reserved", "launching", "destroying", "abandoned"}),
    "abandoned": frozenset({"reserved", "destroying"}),
}

_RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{6,46})[a-z0-9]$")
_INSTANCE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,62}$")
_SHA256 = re.compile(r"^[0-9a-f]{64}$")
_RESERVED_NAMES = frozenset({"primary", "default", "localhost", "i2pr-interop-rootless"})


class LifecycleError(ValueError):
    """A typed, sanitized lifecycle rejection."""

    def __init__(self, outcome: str, message: str | None = None) -> None:
        self.outcome = outcome
        super().__init__(message or outcome)


class UnknownMultipassState(LifecycleError):
    def __init__(self, state: object) -> None:
        super().__init__("blocked_unknown_multipass_instance_state", f"unknown Multipass state: {state!r}")


def utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def validate_run_id(value: str) -> str:
    if not isinstance(value, str) or not _RUN_ID.fullmatch(value):
        raise LifecycleError("blocked_invalid_run_id", "run ID is not a bounded lowercase identifier")
    return value


def validate_instance_name(value: str, *, allow_reserved: bool = False) -> str:
    if not isinstance(value, str) or not _INSTANCE_NAME.fullmatch(value):
        raise LifecycleError("blocked_invalid_instance_name", "instance name is not a bounded identifier")
    if not allow_reserved and value in _RESERVED_NAMES:
        raise LifecycleError("blocked_reserved_instance_name", "instance name is reserved")
    return value


def generate_run_id(now: dt.datetime | None = None, token: str | None = None) -> str:
    stamp = (now or dt.datetime.now(dt.UTC)).strftime("%Y%m%d%H%M%S")
    suffix = token or secrets.token_hex(4)
    value = f"plan049-{stamp}-{suffix.lower()}"
    return validate_run_id(value)


def derive_instance_name(run_id: str, generation: int = 1, *, attempt: int | None = None) -> str:
    validate_run_id(run_id)
    if not isinstance(generation, int) or generation < 1 or generation > 999:
        raise LifecycleError("blocked_invalid_generation", "generation is outside the bounded range")
    suffix = f"-g{generation}"
    if attempt is not None:
        if not isinstance(attempt, int) or attempt < 1 or attempt > MAX_ALLOCATION_ATTEMPTS:
            raise LifecycleError("blocked_instance_name_allocation_exhausted")
        suffix += f"-a{attempt}"
    raw = f"i2pr-interop-{run_id}{suffix}"
    if len(raw) <= MAX_INSTANCE_NAME:
        return raw
    digest = hashlib.sha256(raw.encode("ascii")).hexdigest()[:10]
    prefix = raw[: MAX_INSTANCE_NAME - len(digest) - 1]
    return validate_instance_name(f"{prefix}-{digest}")


def instance_name_digest(instance_name: str) -> str:
    validate_instance_name(instance_name, allow_reserved=True)
    return sha256_bytes(instance_name.encode("utf-8"))


def _state_directory_names(state_root: Path) -> set[str]:
    if not state_root.is_dir():
        return set()
    return {path.name for path in state_root.iterdir() if path.is_dir() and _RUN_ID.fullmatch(path.name)}


def _state_instance_names(state_root: Path | None) -> set[str]:
    if not state_root or not state_root.is_dir():
        return set()
    names: set[str] = set()
    for directory in state_root.iterdir():
        record_path = directory / "lifecycle.json"
        if not record_path.is_file():
            continue
        try:
            value = json.loads(record_path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError):
            continue
        if isinstance(value, dict) and isinstance(value.get("instance_name"), str):
            names.add(value["instance_name"])
    return names


def allocate_instance_name(
    run_id: str,
    *,
    active_instance_names: Iterable[str] = (),
    state_root: Path | None = None,
    evidence_root: Path | None = None,
    generation: int = 1,
    explicit_name: str | None = None,
    max_attempts: int = MAX_ALLOCATION_ATTEMPTS,
) -> tuple[str, int]:
    """Return a collision-free name and generation without mutating a VM."""

    validate_run_id(run_id)
    active = {str(name) for name in active_instance_names}
    state_runs = _state_directory_names(state_root) if state_root else set()
    state_instances = _state_instance_names(state_root)
    evidence_runs = {
        path.name for path in evidence_root.iterdir() if path.is_dir() and _RUN_ID.fullmatch(path.name)
    } if evidence_root and evidence_root.is_dir() else set()
    if explicit_name is not None:
        name = validate_instance_name(explicit_name)
        if name in active:
            raise LifecycleError("blocked_instance_name_owned_by_other_workflow")
        if run_id in state_runs or run_id in evidence_runs:
            raise LifecycleError("blocked_stale_state_ambiguity")
        return name, generation
    for attempt in range(1, max_attempts + 1):
        candidate_generation = generation if attempt == 1 else generation + attempt - 1
        candidate = derive_instance_name(run_id, candidate_generation)
        if candidate not in active and candidate not in state_instances and run_id not in evidence_runs:
            return candidate, candidate_generation
    raise LifecycleError("blocked_instance_name_allocation_exhausted")


_STATE_ALIASES = {
    "running": "running",
    "run": "running",
    "stopped": "stopped",
    "stop": "stopped",
    "suspended": "suspended",
    "starting": "starting",
    "restarting": "restarting",
    "delayed-shutdown": "delayed-shutdown",
    "delayed_shutdown": "delayed-shutdown",
    "deleted": "deleted-unpurged",
    "deleted-unpurged": "deleted-unpurged",
}


def normalize_instance_state(value: object) -> str:
    if not isinstance(value, str):
        raise UnknownMultipassState(value)
    key = value.strip().lower().replace(" ", "-")
    try:
        return _STATE_ALIASES[key]
    except KeyError as exc:
        raise UnknownMultipassState(value) from exc


def parse_multipass_list(raw: str | bytes) -> list[dict[str, str]]:
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise LifecycleError("blocked_unknown_multipass_instance_state", "Multipass JSON is malformed") from exc
    entries = value.get("list") if isinstance(value, dict) else value
    if not isinstance(entries, list):
        raise LifecycleError("blocked_unknown_multipass_instance_state", "Multipass list shape is unknown")
    result: list[dict[str, str]] = []
    for entry in entries:
        if not isinstance(entry, dict) or not isinstance(entry.get("name"), str):
            raise LifecycleError("blocked_unknown_multipass_instance_state", "Multipass list entry is malformed")
        name = validate_instance_name(entry["name"], allow_reserved=True)
        result.append({"name": name, "state": normalize_instance_state(entry.get("state"))})
    return sorted(result, key=lambda item: item["name"])


def parse_multipass_info(raw: str | bytes, instance_name: str) -> dict[str, Any]:
    """Normalize the supported ``multipass info --format json`` shapes."""

    validate_instance_name(instance_name, allow_reserved=True)
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise LifecycleError("blocked_unknown_multipass_instance_state", "Multipass info JSON is malformed") from exc
    entries = value.get("info") if isinstance(value, dict) else value
    entry: dict[str, Any] | None = None
    if isinstance(entries, dict):
        candidate = entries.get(instance_name)
        if isinstance(candidate, dict):
            entry = candidate
    elif isinstance(entries, list):
        for candidate in entries:
            if isinstance(candidate, dict) and candidate.get("name", instance_name) == instance_name:
                entry = candidate
                break
    if entry is None:
        raise LifecycleError("blocked_instance_missing")
    normalized = {"name": instance_name, "state": normalize_instance_state(entry.get("state"))}
    snapshots = entry.get("snapshots", [])
    if isinstance(snapshots, list):
        normalized["snapshots"] = sorted(
            str(item.get("name")) for item in snapshots if isinstance(item, dict) and isinstance(item.get("name"), str)
        )
    else:
        normalized["snapshots"] = []
    return normalized


def classify_collision(
    *,
    instance_state: str,
    host_state_exists: bool,
    ownership_verified: bool = False,
    contract_verified: bool = False,
) -> str:
    """Classify a name collision without deciding to mutate the resource."""

    try:
        normalized = normalize_instance_state(instance_state)
    except UnknownMultipassState:
        return "blocked_existing_instance_state_ambiguous"
    if normalized == "deleted-unpurged":
        return "blocked_deleted_instance_requires_purge"
    if normalized == "unknown":
        return "blocked_existing_instance_state_ambiguous"
    if not host_state_exists:
        return "blocked_instance_without_host_state"
    if not ownership_verified:
        return "blocked_instance_name_owned_by_other_workflow"
    if not contract_verified:
        return "blocked_existing_instance_contract_mismatch"
    return "ownership_verified"


def transition(record: dict[str, Any], new_state: str, *, operation: str, outcome: str) -> dict[str, Any]:
    old_state = record.get("state")
    if old_state not in LIFECYCLE_STATES:
        raise LifecycleError("blocked_unknown_lifecycle_state")
    if new_state not in LIFECYCLE_STATES:
        raise LifecycleError("blocked_unknown_lifecycle_state")
    if new_state != old_state and new_state not in _TRANSITIONS[old_state]:
        raise LifecycleError("blocked_invalid_lifecycle_transition")
    updated = dict(record)
    updated.update({"state": new_state, "updated_at_utc": utc_now(), "last_operation": operation, "last_typed_outcome": outcome})
    return updated


def write_json_atomic(path: Path, value: dict[str, Any], *, mode: int = 0o600) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp.{os.getpid()}.{secrets.token_hex(4)}")
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n"
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    fd = os.open(temporary, flags, mode)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
        directory_fd = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        if temporary.exists():
            temporary.unlink()


def initial_record(
    *,
    environment_id: str,
    run_id: str,
    instance_name: str,
    generation: int,
    source_commit: str,
    environment_manifest_sha256: str,
    cloud_init_sha256: str,
    owner_token_sha256: str,
    host_multipass_version: str,
) -> dict[str, Any]:
    if environment_id != ENVIRONMENT_ID:
        raise LifecycleError("blocked_environment_id_mismatch")
    validate_run_id(run_id)
    validate_instance_name(instance_name)
    if not isinstance(generation, int) or generation < 1 or generation > 999:
        raise LifecycleError("blocked_invalid_generation")
    if not re.fullmatch(r"[0-9a-f]{40}", source_commit):
        raise LifecycleError("blocked_invalid_source_commit")
    if not _SHA256.fullmatch(owner_token_sha256) or not _SHA256.fullmatch(environment_manifest_sha256) or not _SHA256.fullmatch(cloud_init_sha256):
        raise LifecycleError("blocked_invalid_lifecycle_digest")
    return {
        "schema_version": SCHEMA_VERSION,
        "environment_id": environment_id,
        "run_id": run_id,
        "instance_name": instance_name,
        "instance_generation": generation,
        "state": "reserved",
        "source_commit": source_commit,
        "source_archive_sha256": PENDING,
        "environment_manifest_sha256": environment_manifest_sha256,
        "cloud_init_sha256": cloud_init_sha256,
        "reference_cache_manifest_sha256": PENDING,
        "owner_token_sha256": owner_token_sha256,
        "created_at_utc": utc_now(),
        "updated_at_utc": utc_now(),
        "last_operation": "reserve",
        "last_typed_outcome": "reserved",
        "host_multipass_version": host_multipass_version,
        "adoption_mode": "fresh",
    }


def update_record(path: Path, new_state: str, *, operation: str, outcome: str, updates: dict[str, Any] | None = None) -> dict[str, Any]:
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise LifecycleError("blocked_host_state_without_instance") from exc
    updated = transition(record, new_state, operation=operation, outcome=outcome)
    if updates:
        updated.update(updates)
    write_json_atomic(path, updated)
    return updated


def ownership_proof(
    record: dict[str, Any],
    guest_contract: dict[str, Any] | None,
    *,
    guest_token_sha256: str | None,
    token_owner: str | None = "root:root",
    token_mode: int | None = 0o600,
    contract_owner: str | None = "root:root",
    contract_mode: int | None = 0o644,
) -> tuple[bool, str]:
    """Return ``(verified, typed_outcome)`` without exposing guest details."""

    if not record or not guest_contract:
        return False, "blocked_ownership_token_mismatch"
    fields = {
        "environment_id": record.get("environment_id"),
        "run_id": record.get("run_id"),
        "instance_name": record.get("instance_name"),
        "environment_manifest_sha256": record.get("environment_manifest_sha256"),
        "cloud_init_sha256": record.get("cloud_init_sha256"),
        "owner_token_sha256": record.get("owner_token_sha256"),
    }
    if any(guest_contract.get(key) != expected for key, expected in fields.items()):
        return False, "blocked_existing_instance_contract_mismatch"
    if record.get("state") in {"source_ready", "cache_ready", "source_and_cache_ready", "probe_passed", "offline_ready", "running", "exporting", "exported"} and guest_contract.get("source_commit_expected") != record.get("source_commit"):
        return False, "blocked_existing_instance_contract_mismatch"
    if guest_token_sha256 != record.get("owner_token_sha256"):
        return False, "blocked_ownership_token_mismatch"
    if token_owner != "root:root" or token_mode != 0o600:
        return False, "blocked_existing_instance_contract_mismatch"
    if contract_owner != "root:root" or contract_mode is None or contract_mode & 0o022:
        return False, "blocked_existing_instance_contract_mismatch"
    if record.get("state") not in LIFECYCLE_STATES:
        return False, "blocked_unknown_lifecycle_state"
    return True, "ownership_verified"


class LifecycleLock:
    """An OS-backed per-run lock; abandoned processes release it automatically."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self._handle: Any = None

    def acquire(self) -> None:
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        self._handle = self.path.open("a+", encoding="utf-8")
        try:
            fcntl.flock(self._handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as exc:
            self._handle.close()
            self._handle = None
            raise LifecycleError("blocked_lifecycle_lock_held") from exc
        self._handle.seek(0)
        self._handle.truncate()
        self._handle.write(json.dumps({"pid": os.getpid(), "started_at_utc": utc_now()}, separators=(",", ":")))
        self._handle.flush()

    def release(self) -> None:
        if self._handle is not None:
            fcntl.flock(self._handle.fileno(), fcntl.LOCK_UN)
            self._handle.close()
            self._handle = None

    def __enter__(self) -> "LifecycleLock":
        self.acquire()
        return self

    def __exit__(self, *_: object) -> None:
        self.release()


def blocker_record(
    *,
    run_id: str,
    environment_id: str,
    generation: int,
    phase: str,
    outcome: str,
    remediation_class: str,
    environment_manifest_sha256: str,
    cloud_init_sha256: str,
    host_baseline_probe_outcome: str,
    guest_probe_outcome: str = "not-reached",
) -> dict[str, Any]:
    validate_run_id(run_id)
    return {
        "schema": 1,
        "run_id": run_id,
        "environment_id": environment_id,
        "instance_generation": generation,
        "phase": phase,
        "outcome": outcome,
        "remediation_class": remediation_class,
        "host_baseline_probe_outcome": host_baseline_probe_outcome,
        "guest_probe_outcome": guest_probe_outcome,
        "environment_manifest_sha256": environment_manifest_sha256,
        "cloud_init_sha256": cloud_init_sha256,
    }


def _load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise LifecycleError("blocked_host_state_without_instance")
    return value


def _main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    generate = sub.add_parser("generate-run-id")
    generate.add_argument("--token")
    derive = sub.add_parser("derive-instance")
    derive.add_argument("--run-id", required=True)
    derive.add_argument("--generation", type=int, default=1)
    derive.add_argument("--attempt", type=int)
    allocate = sub.add_parser("allocate-instance")
    allocate.add_argument("--run-id", required=True)
    allocate.add_argument("--generation", type=int, default=1)
    allocate.add_argument("--active-json", default="[]")
    allocate.add_argument("--state-root", type=Path)
    allocate.add_argument("--evidence-root", type=Path)
    allocate.add_argument("--instance-name")
    allocate.add_argument("--max-attempts", type=int, default=MAX_ALLOCATION_ATTEMPTS)
    validate = sub.add_parser("validate-run-id")
    validate.add_argument("value")
    validate_instance = sub.add_parser("validate-instance-name")
    validate_instance.add_argument("value")
    reserve = sub.add_parser("reserve")
    reserve.add_argument("--output", type=Path, required=True)
    for name in (
        "environment-id", "run-id", "instance-name", "generation", "source-commit",
        "environment-manifest-sha256", "cloud-init-sha256", "owner-token-sha256",
        "host-multipass-version",
    ):
        reserve.add_argument(f"--{name}", required=True)
    update = sub.add_parser("update")
    update.add_argument("--state-file", type=Path, required=True)
    update.add_argument("--state", required=True)
    update.add_argument("--operation", dest="record_operation", required=True)
    update.add_argument("--outcome", required=True)
    update.add_argument("--updates-json", default="{}")
    args = parser.parse_args()
    try:
        if args.operation == "generate-run-id":
            print(generate_run_id(token=args.token))
        elif args.operation == "derive-instance":
            print(derive_instance_name(args.run_id, args.generation, attempt=args.attempt))
        elif args.operation == "allocate-instance":
            active = json.loads(args.active_json)
            name, generation = allocate_instance_name(
                args.run_id,
                active_instance_names=active,
                state_root=args.state_root,
                evidence_root=args.evidence_root,
                generation=args.generation,
                explicit_name=args.instance_name,
                max_attempts=args.max_attempts,
            )
            print(json.dumps({"instance_name": name, "generation": generation}, separators=(",", ":")))
        elif args.operation == "validate-instance-name":
            print(validate_instance_name(args.value))
        elif args.operation == "reserve":
            value = initial_record(
                environment_id=args.environment_id,
                run_id=args.run_id,
                instance_name=args.instance_name,
                generation=int(args.generation),
                source_commit=args.source_commit,
                environment_manifest_sha256=args.environment_manifest_sha256,
                cloud_init_sha256=args.cloud_init_sha256,
                owner_token_sha256=args.owner_token_sha256,
                host_multipass_version=args.host_multipass_version,
            )
            write_json_atomic(args.output, value)
            print(json.dumps({"schema": 1, "type": "multipass-lifecycle", "state": "reserved"}, separators=(",", ":")))
        elif args.operation == "update":
            updates = json.loads(args.updates_json)
            if not isinstance(updates, dict):
                raise LifecycleError("blocked_invalid_lifecycle_update")
            value = update_record(args.state_file, args.state, operation=args.record_operation, outcome=args.outcome, updates=updates)
            print(json.dumps(value, sort_keys=True, separators=(",", ":")))
        else:
            print(validate_run_id(args.value))
    except (LifecycleError, OSError, TypeError, ValueError, json.JSONDecodeError) as exc:
        outcome = exc.outcome if isinstance(exc, LifecycleError) else "blocked_lifecycle_input"
        print(json.dumps({"schema": 1, "type": "multipass-lifecycle", "outcome": outcome}, separators=(",", ":")))
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(_main())
