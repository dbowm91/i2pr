"""Plan 056 two-bundle Milestone 3 certificate verifier.

A Plan 056 certificate requires two independently executed, complete,
sanitized Plan 052 evidence bundles produced from the same exact i2pr
source commit. Each bundle must contain four primary IPv4 mixed-router
directions. This verifier accepts both bundle paths, verifies them
individually, then enforces the cross-bundle provenance, direction
predicate, and independence rules required by
``plans/056-ntcp2-milestone-3-two-run-external-evidence-closure-pass.md``.

The verifier never trusts the bundle contents before re-hashing them.
Every cross-bundle check is structural and deterministic; it does not
narrate. The CLI returns non-zero on any failure and prints a
sanitized JSON certificate to stdout on success. The optional
``--output`` flag writes the certificate to disk beside the run
artefacts.

Schema:

```text
i2pr-milestone3-certificate-v1
```

The verifier does not invent a separate transport or I/O boundary. It
reuses :mod:`evidence_bundle` and :mod:`run_identity` so the same
sanitization, manifest classification, and run-identity validation that
produced the bundles also audit them.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

try:
    from .evidence_bundle import (
        DIRECTION_CLASSES,
        PRIMARY_DIRECTIONS,
        BundleError,
        verify_bundle,
    )
    from .observation import (
        OBSERVATION_SCHEMA,
        receiver_passes_data_phase,
        sender_emitted_data_frame,
        both_authenticated,
    )
    from .run_identity import (
        RUN_IDENTITY_SCHEMA,
        load_run_identity,
    )
except ImportError:
    from evidence_bundle import (
        DIRECTION_CLASSES,
        PRIMARY_DIRECTIONS,
        BundleError,
        verify_bundle,
    )
    from observation import (
        OBSERVATION_SCHEMA,
        receiver_passes_data_phase,
        sender_emitted_data_frame,
        both_authenticated,
    )
    from run_identity import (
        RUN_IDENTITY_SCHEMA,
        load_run_identity,
    )


CERTIFICATE_SCHEMA = "i2pr-milestone3-certificate-v1"
CERTIFICATE_SCHEMA_VERSION = 1

_HEX40 = re.compile(r"^[0-9a-f]{40}$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")

_FORBIDDEN_TEXT = (
    "-----BEGIN",
    "router.identity",
    "ntcp2.static.key",
    "/home/",
    "/root/",
)

# Fields that MUST be byte-identical across the two bundles.
MATCHED_PROVENANCE_FIELDS: tuple[str, ...] = (
    "source_commit",
    "source_commit_object_sha256",
    "source_tree_sha256",
    "source_archive_sha256",
    "source_archive_format",
    "launcher_binary_sha256",
    "launcher_build_profile",
    "rustc_version",
    "cargo_version",
    "target_triple",
    "topology_kind",
    "privilege_model",
    "reference_lock_sha256",
    "evidence_schema_revision",
)

# Fields whose differences are allowed only for lifecycle generation.
ALLOWLISTED_DIVERGENT_FIELDS: tuple[str, ...] = (
    "run_id",
    "created_at",
    "environment_manifest_sha256",
    "host_source_manifest_sha256",
    "guest_source_manifest_sha256",
    "guest_source_listing_sha256",
    "run_identity_sha256",
)


class CertificateError(ValueError):
    """Raised when a Plan 056 certificate check fails."""


@dataclass
class BundleDigest:
    path: Path
    run_id: str
    identity: dict[str, Any]
    directions: dict[str, dict[str, dict[str, Any]]] = field(default_factory=dict)

    @property
    def run_identity_sha256(self) -> str:
        return str(self.identity.get("run_identity_sha256", ""))


def _scan(value: Any) -> None:
    if isinstance(value, str):
        if any(token in value for token in _FORBIDDEN_TEXT):
            raise CertificateError("bundle contains forbidden secret or path text")
        if "RouterInfo" in value or "I2NP" in value:
            raise CertificateError("bundle contains forbidden RouterInfo/I2NP text")
    elif isinstance(value, dict):
        for child in value.values():
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


def _sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _check_bundle_visibility(root: Path) -> list[str]:
    """Walk the staging tree and reject raw-diagnostics / hidden files."""

    failures: list[str] = []
    forbidden_suffixes = (".pcap", ".pcapng", ".log", ".dump", ".cap")
    if not root.exists():
        return failures
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        lower = rel.lower()
        if any(lower.endswith(suffix) for suffix in forbidden_suffixes):
            failures.append(f"{root.name}: forbidden raw-diagnostics file {rel}")
        if rel.startswith(".") or "/." in rel:
            failures.append(f"{root.name}: hidden/temp file rejected: {rel}")
        if any(part.startswith(".") for part in Path(rel).parts):
            failures.append(f"{root.name}: hidden path component rejected: {rel}")
    return failures


def _load_bundle(root: Path) -> tuple[BundleDigest, list[str]]:
    """Load and verify one bundle, returning the digest and any structural failures.

    Bundle-level failures (missing files, manifest corruption, forbidden
    artefacts) are returned as a list of human-readable strings rather than
    raised, so the certificate can report every problem in a single pass.
    """
    root = root.resolve()
    failures: list[str] = []
    if not root.is_dir():
        failures.append(f"bundle path is not a directory: {root}")
        return BundleDigest(path=root, run_id="", identity={}), failures
    identity_path = root / "run-identity.json"
    if not identity_path.is_file():
        failures.append(f"bundle is missing run-identity.json: {root}")
        return BundleDigest(path=root, run_id="", identity={}), failures
    try:
        identity = load_run_identity(identity_path)
    except Exception as exc:
        failures.append(f"bundle run-identity invalid: {exc}")
        return BundleDigest(path=root, run_id="", identity={}), failures
    visibility_failures = _check_bundle_visibility(root)
    failures.extend(visibility_failures)
    try:
        verify_bundle(root)
    except BundleError as exc:
        failures.append(f"bundle verification failed: {exc}")
    digest = BundleDigest(
        path=root,
        run_id=str(identity["run_id"]),
        identity=identity,
    )
    for direction in PRIMARY_DIRECTIONS:
        digest.directions[direction] = {}
        for direction_class in DIRECTION_CLASSES:
            class_root = root / direction_class
            record_path = class_root / f"{direction}.json"
            if not record_path.is_file():
                failures.append(
                    f"bundle is missing {direction_class}/{direction}.json"
                )
                continue
            payload = json.loads(record_path.read_text(encoding="utf-8"))
            try:
                _scan(payload)
            except CertificateError as exc:
                failures.append(
                    f"bundle {direction_class}/{direction}.json: {exc}"
                )
            digest.directions[direction][direction_class] = payload
    return digest, failures


def _direction_result(direction_record: dict[str, Any]) -> str:
    return str(direction_record.get("actual_typed_result", ""))


def _cleanup_result(cleanup_record: dict[str, Any]) -> str:
    return str(cleanup_record.get("topology_cleanup", cleanup_record.get("final_result", "")))


def _select_sides(
    observation_payload: dict[str, Any],
    *,
    direction: str,
) -> tuple[str, dict[str, Any], str, dict[str, Any]]:
    sides = observation_payload.get("sides", {})
    initiator = "i2pr" if direction.startswith("i2pr-to-") else (
        "java_i2p" if direction.startswith("java-to-") else "i2pd"
    )
    if initiator == "i2pr":
        sender_side = "i2pr"
        receiver_kind = "java_i2p" if "java" in direction else "i2pd"
    else:
        sender_side = initiator
        receiver_kind = "i2pr"
    if sender_side not in sides or receiver_kind not in sides:
        raise CertificateError(
            f"observation missing required sides for {direction}: have {sorted(sides)}"
        )
    return sender_side, sides[sender_side], receiver_kind, sides[receiver_kind]


def _validate_direction_predicates(bundle: BundleDigest) -> list[str]:
    failures: list[str] = []
    for direction, classes in bundle.directions.items():
        direction_record = classes["directions"]
        cleanup_record = classes["cleanup"]
        observation_payload = classes["observations"]
        attestation_record = classes["attestations"]
        trigger_record = classes["triggers"]
        if _direction_result(direction_record) != "passed":
            failures.append(
                f"{bundle.run_id}/{direction}: actual_typed_result != passed"
            )
        cleanup_topology = _cleanup_result(cleanup_record)
        if cleanup_topology != "clean":
            failures.append(
                f"{bundle.run_id}/{direction}: cleanup_result={cleanup_topology!r} != clean"
            )
        if str(attestation_record.get("schema", "")) != "i2pr-mixed-router-attestation-v2":
            failures.append(
                f"{bundle.run_id}/{direction}: attestation schema is not v2"
            )
        if not bool(attestation_record.get("parent_network_state_unchanged", False)):
            failures.append(
                f"{bundle.run_id}/{direction}: parent_network_state_unchanged is False"
            )
        if str(trigger_record.get("schema", "")) != "i2pr-reference-trigger-v3":
            failures.append(
                f"{bundle.run_id}/{direction}: trigger schema is not v3"
            )
        sender_side, sender_obs, receiver_kind, receiver_obs = _select_sides(
            observation_payload, direction=direction
        )
        if not receiver_passes_data_phase(receiver_obs):
            failures.append(
                f"{bundle.run_id}/{direction}: receiver {receiver_kind} does not satisfy data-phase predicate"
            )
        if not sender_emitted_data_frame(sender_obs):
            failures.append(
                f"{bundle.run_id}/{direction}: sender {sender_side} did not emit a frame"
            )
        if not both_authenticated(sender_obs, receiver_obs):
            failures.append(
                f"{bundle.run_id}/{direction}: both sides did not reach ntcp2_authenticated"
            )
        if observation_payload.get("schema") != OBSERVATION_SCHEMA:
            failures.append(
                f"{bundle.run_id}/{direction}: observation schema is not v2"
            )
    return failures


def _cross_check_provenance(run_a: BundleDigest, run_b: BundleDigest) -> list[str]:
    failures: list[str] = []
    identity_a = run_a.identity
    identity_b = run_b.identity
    for field in MATCHED_PROVENANCE_FIELDS:
        value_a = identity_a.get(field)
        value_b = identity_b.get(field)
        if value_a != value_b:
            failures.append(
                f"provenance mismatch: {field!r} {value_a!r} vs {value_b!r}"
            )
    if identity_a.get("source_dirty") != "clean":
        failures.append("run A source_dirty is not clean")
    if identity_b.get("source_dirty") != "clean":
        failures.append("run B source_dirty is not clean")
    if identity_a.get("schema") != RUN_IDENTITY_SCHEMA:
        failures.append("run A run identity schema is not locked")
    if identity_b.get("schema") != RUN_IDENTITY_SCHEMA:
        failures.append("run B run identity schema is not locked")
    return failures


def _cross_check_independence(run_a: BundleDigest, run_b: BundleDigest) -> list[str]:
    failures: list[str] = []
    if run_a.run_id == run_b.run_id:
        failures.append("run_a.run_id == run_b.run_id; independence requires distinct IDs")
    if run_a.identity["run_identity_sha256"] == run_b.identity["run_identity_sha256"]:
        failures.append("run_a.run_identity_sha256 == run_b.run_identity_sha256; runs share identity")
    for direction in PRIMARY_DIRECTIONS:
        observation_a = run_a.directions[direction]["observations"]
        observation_b = run_b.directions[direction]["observations"]
        sha_a = observation_a.get("observation_sha256")
        sha_b = observation_b.get("observation_sha256")
        if sha_a and sha_b and sha_a == sha_b:
            failures.append(
                f"{direction}: identical observation_sha256 across runs indicates a copied artifact"
            )
        nonce_a = observation_a.get("run_correlation", {}).get("delivery_status_message_id")
        nonce_b = observation_b.get("run_correlation", {}).get("delivery_status_message_id")
        if nonce_a and nonce_b and nonce_a == nonce_b:
            failures.append(
                f"{direction}: identical correlation message id across runs"
            )
        trigger_a = run_a.directions[direction]["triggers"]
        trigger_b = run_b.directions[direction]["triggers"]
        trigger_nonce_a = trigger_a.get("correlation_nonce")
        trigger_nonce_b = trigger_b.get("correlation_nonce")
        if (
            trigger_nonce_a
            and trigger_nonce_b
            and trigger_nonce_a != "0" * 16
            and trigger_nonce_b != "0" * 16
            and trigger_nonce_a == trigger_nonce_b
        ):
            failures.append(
                f"{direction}: identical correlation_nonce in trigger records across runs"
            )
        for field in ("i2pr_router_info_sha256", "reference_router_info_sha256"):
            value_a = run_a.directions[direction]["directions"].get(
                "router_info", {}
            ).get("sha256")
            value_b = run_b.directions[direction]["directions"].get(
                "router_info", {}
            ).get("sha256")
            if value_a and value_b and value_a == value_b and value_a != "0" * 64:
                failures.append(
                    f"{direction}: copied {field} across runs"
                )
    return failures


def _cross_check_bundle_visibility(
    run_a: BundleDigest, run_b: BundleDigest,
) -> list[str]:
    """Aggregate any per-bundle visibility findings already collected in load."""

    return []


def _collect_direction_outcomes(bundle: BundleDigest) -> dict[str, dict[str, str]]:
    table: dict[str, dict[str, str]] = {}
    for direction, classes in bundle.directions.items():
        direction_record = classes["directions"]
        cleanup_record = classes["cleanup"]
        observation_payload = classes["observations"]
        _, _, receiver_kind, receiver_obs = _select_sides(
            observation_payload, direction=direction
        )
        receiver_levels = receiver_obs.get("levels", {})
        table[direction] = {
            "result": _direction_result(direction_record),
            "cleanup": _cleanup_result(cleanup_record),
            "receiver_data_phase": "observed"
            if receiver_passes_data_phase(receiver_obs)
            else "not-observed",
            "receiver_i2np_decoded": str(
                receiver_levels.get("i2np_message_decoded", {}).get("state", "missing")
            ),
            "receiver_frame_decrypted": str(
                receiver_levels.get("frame_authenticated_and_decrypted", {}).get(
                    "state", "missing"
                )
            ),
            "parent_network_state_unchanged": str(
                classes["attestations"].get("parent_network_state_unchanged")
            ),
        }
    return table


def verify_certificate(run_a_path: Path, run_b_path: Path) -> dict[str, Any]:
    """Run every Plan 056 certificate check and return the certificate payload."""

    run_a, failures_a = _load_bundle(run_a_path)
    run_b, failures_b = _load_bundle(run_b_path)
    failures: list[str] = []
    failures.extend(failures_a)
    failures.extend(failures_b)
    if failures_a or failures_b:
        certificate = {
            "schema": CERTIFICATE_SCHEMA,
            "schema_version": CERTIFICATE_SCHEMA_VERSION,
            "issued_at": dt.datetime.now(dt.UTC)
            .replace(microsecond=0)
            .isoformat()
            .replace("+00:00", "Z"),
            "run_a": run_a.run_id,
            "run_b": run_b.run_id,
            "verified": False,
            "failures": failures,
        }
        return certificate
    failures.extend(_cross_check_provenance(run_a, run_b))
    failures.extend(_cross_check_independence(run_a, run_b))
    failures.extend(_validate_direction_predicates(run_a))
    failures.extend(_validate_direction_predicates(run_b))
    failures.extend(_cross_check_bundle_visibility(run_a, run_b))

    identity_a = run_a.identity
    identity_b = run_b.identity
    divergent = {
        field: (identity_a.get(field), identity_b.get(field))
        for field in ALLOWLISTED_DIVERGENT_FIELDS
        if identity_a.get(field) != identity_b.get(field)
    }
    unexpected_divergent = sorted(
        field
        for field in divergent
            if field not in {
                "run_id",
                "created_at",
                "run_identity_sha256",
                "guest_source_manifest_sha256",
                "guest_source_listing_sha256",
                "environment_manifest_sha256",
            }
    )
    if unexpected_divergent:
        failures.append(
            "unauthorized divergent identity fields: "
            + ", ".join(unexpected_divergent)
        )

    certificate = {
        "schema": CERTIFICATE_SCHEMA,
        "schema_version": CERTIFICATE_SCHEMA_VERSION,
        "issued_at": dt.datetime.now(dt.UTC)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "run_a": run_a.run_id,
        "run_b": run_b.run_id,
        "source_commit": identity_a.get("source_commit"),
        "source_tree_sha256": identity_a.get("source_tree_sha256"),
        "launcher_binary_sha256": identity_a.get("launcher_binary_sha256"),
        "reference_lock_sha256": identity_a.get("reference_lock_sha256"),
        "topology_kind": identity_a.get("topology_kind"),
        "privilege_model": identity_a.get("privilege_model"),
        "verified": not failures,
        "failures": failures,
        "allowlisted_divergent_fields": divergent,
        "run_a_directions": _collect_direction_outcomes(run_a),
        "run_b_directions": _collect_direction_outcomes(run_b),
    }
    return certificate


def _write_certificate(certificate: dict[str, Any], output: Path | None) -> None:
    encoded = json.dumps(certificate, sort_keys=False, separators=(",", ":"))
    sys.stdout.write(encoded + "\n")
    sys.stdout.flush()
    if output is not None:
        output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        fd, temporary = tempfile.mkstemp(prefix=f".{output.name}.", dir=output.parent)
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                handle.write(encoded + "\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.chmod(temporary, 0o600)
            os.replace(temporary, output)
        finally:
            if Path(temporary).exists():
                Path(temporary).unlink()


def _main(argv: Iterable[str] | None = None) -> int:
    import tempfile

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-a", type=Path, required=True)
    parser.add_argument("--run-b", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(list(argv) if argv is not None else None)
    try:
        certificate = verify_certificate(args.run_a.resolve(), args.run_b.resolve())
    except (BundleError, CertificateError, OSError, ValueError) as exc:
        sys.stderr.write(f"certificate-verification-failed: {exc}\n")
        return 2
    _write_certificate(certificate, args.output.resolve() if args.output else None)
    return 0 if certificate["verified"] else 3


if __name__ == "__main__":
    raise SystemExit(_main())
