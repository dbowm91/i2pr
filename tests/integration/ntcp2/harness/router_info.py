"""Minimal bounded RouterInfo naming proof shared by reference adapters."""

from __future__ import annotations

import base64
import hashlib
import ipaddress
import json
import subprocess
from pathlib import Path
from dataclasses import dataclass


class RouterInfoPathError(ValueError):
    """A RouterInfo is too short or has an invalid identity certificate shape."""


@dataclass(frozen=True)
class RouterInfoValidation:
    """Privacy-safe result of the bounded preflight parser."""

    size: int
    ntcp2_address_count: int
    endpoint_match: bool
    signature_length: int


def _mapping(data: bytes, offset: int) -> tuple[dict[str, str], int]:
    if offset + 2 > len(data):
        raise RouterInfoPathError("RouterInfo mapping length is truncated")
    length = int.from_bytes(data[offset : offset + 2], "big")
    offset += 2
    end = offset + length
    if end > len(data):
        raise RouterInfoPathError("RouterInfo mapping exceeds the record")
    values: dict[str, str] = {}
    previous = ""
    while offset < end:
        if offset + 1 > end:
            raise RouterInfoPathError("RouterInfo mapping key is truncated")
        key_length = data[offset]
        offset += 1
        if offset + key_length + 1 > end:
            raise RouterInfoPathError("RouterInfo mapping key is malformed")
        key = data[offset : offset + key_length].decode("utf-8")
        offset += key_length
        if data[offset] != ord("="):
            raise RouterInfoPathError("RouterInfo mapping separator is malformed")
        offset += 1
        if offset + 1 > end:
            raise RouterInfoPathError("RouterInfo mapping value is truncated")
        value_length = data[offset]
        offset += 1
        if offset + value_length + 1 > end:
            raise RouterInfoPathError("RouterInfo mapping value is malformed")
        value = data[offset : offset + value_length].decode("utf-8")
        offset += value_length
        if offset >= end or data[offset] != ord(";") or key in values or key <= previous:
            raise RouterInfoPathError("RouterInfo mapping is not canonical")
        offset += 1
        values[key] = value
        previous = key
    if offset != end:
        raise RouterInfoPathError("RouterInfo mapping has trailing bytes")
    return values, end


def validate_router_info_structure(path: Path, *, expected_address: str, expected_port: int) -> RouterInfoValidation:
    """Validate bounded RouterInfo structure and the exact synthetic NTCP2 endpoint.

    Signature verification is deliberately performed by the Rust
    ``i2pr-interop ntcp2 inspect`` helper. This Python preflight only supplies
    endpoint and path checks; it never turns structure into authentication.
    """

    try:
        data = path.read_bytes()
    except OSError as exc:
        raise RouterInfoPathError("RouterInfo input is unreadable") from exc
    if not 391 <= len(data) <= 1024 * 1024:
        raise RouterInfoPathError("RouterInfo size is outside the bounded range")
    if data[384] != 5:
        raise RouterInfoPathError("RouterInfo does not use a key certificate")
    certificate_length = int.from_bytes(data[385:387], "big")
    identity_length = 387 + certificate_length
    if certificate_length < 4 or identity_length + 8 > len(data):
        raise RouterInfoPathError("RouterInfo certificate length is invalid")
    certificate = data[387:identity_length]
    if int.from_bytes(certificate[:2], "big") != 7 or int.from_bytes(certificate[2:4], "big") != 4:
        raise RouterInfoPathError("RouterInfo key certificate is not Ed25519/X25519")
    offset = identity_length + 8
    if offset >= len(data):
        raise RouterInfoPathError("RouterInfo address count is missing")
    address_count = data[offset]
    offset += 1
    ntcp2_count = 0
    endpoint_match = False
    for _ in range(address_count):
        if offset + 9 > len(data):
            raise RouterInfoPathError("RouterInfo address header is truncated")
        offset += 1 + 8
        style_length = data[offset]
        offset += 1
        if offset + style_length > len(data):
            raise RouterInfoPathError("RouterInfo address style is truncated")
        style = data[offset : offset + style_length].decode("utf-8")
        offset += style_length
        values, offset = _mapping(data, offset)
        if style not in {"NTCP", "NTCP2"} or "2" not in values.get("v", "").split(","):
            continue
        if not all(key in values for key in ("host", "port", "s", "i")):
            raise RouterInfoPathError("NTCP2 RouterAddress lacks required material")
        try:
            endpoint_match = endpoint_match or (
                ipaddress.ip_address(values["host"]).compressed == ipaddress.ip_address(expected_address).compressed
                and int(values["port"]) == expected_port
            )
        except (ValueError, TypeError) as exc:
            raise RouterInfoPathError("NTCP2 RouterAddress endpoint is malformed") from exc
        ntcp2_count += 1
    if offset >= len(data):
        raise RouterInfoPathError("RouterInfo peer count is missing")
    peer_count = data[offset]
    offset += 1 + 32 * peer_count
    if offset > len(data):
        raise RouterInfoPathError("RouterInfo peer list is truncated")
    _, offset = _mapping(data, offset)
    signature_length = 64
    if offset + signature_length != len(data):
        raise RouterInfoPathError("RouterInfo signature is missing or trailing bytes remain")
    if ntcp2_count == 0 or not endpoint_match:
        raise RouterInfoPathError("RouterInfo lacks the exact synthetic NTCP2 endpoint")
    return RouterInfoValidation(len(data), ntcp2_count, endpoint_match, signature_length)


def strict_validate_router_info(
    path: Path, *, expected_address: str, expected_port: int, repo_root: Path
) -> RouterInfoValidation:
    """Run Python confinement checks and the repository's strict Rust parser."""

    result = validate_router_info_structure(path, expected_address=expected_address, expected_port=expected_port)
    state_dir = path.parent / ".strict-router-info"
    state_dir.mkdir(mode=0o700, exist_ok=True)
    strict_path = state_dir / "router.info"
    strict_path.write_bytes(path.read_bytes())
    binary = repo_root / "target/debug/i2pr-interop"
    if not binary.is_file() or not binary.stat().st_mode & 0o111:
        raise RouterInfoPathError("strict-router-info-parser-unavailable")
    completed = subprocess.run(
        [str(binary), "ntcp2", "inspect", "--state-dir", str(state_dir)],
        capture_output=True,
        text=True,
        check=False,
    )
    try:
        output = json.loads(completed.stdout.strip())
    except (json.JSONDecodeError, UnicodeError) as exc:
        raise RouterInfoPathError("strict-router-info-parser-returned-invalid-status") from exc
    if completed.returncode != 0 or output.get("result") != "validated":
        raise RouterInfoPathError("strict-router-info-parser-rejected-router-info")
    return result


def netdb_filename(path: Path) -> str:
    """Return the pinned ``routerInfo-<identity-hash>.dat`` filename convention."""

    try:
        data = path.read_bytes()
    except OSError as exc:
        raise RouterInfoPathError("RouterInfo input is unreadable") from exc
    if len(data) < 387:
        raise RouterInfoPathError("RouterInfo is shorter than RouterIdentity header")
    certificate_length = int.from_bytes(data[385:387], "big")
    identity_length = 387 + certificate_length
    if certificate_length > 32 * 1024 or identity_length > len(data):
        raise RouterInfoPathError("RouterInfo certificate length is invalid")
    identity_hash = hashlib.sha256(data[:identity_length]).digest()
    encoded = base64.b64encode(identity_hash).decode("ascii").rstrip("=").translate(str.maketrans("+/", "-~"))
    return f"routerInfo-{encoded}.dat"
