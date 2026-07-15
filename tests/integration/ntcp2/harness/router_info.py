"""Minimal bounded RouterInfo naming proof shared by reference adapters."""

from __future__ import annotations

import base64
import hashlib
from pathlib import Path


class RouterInfoPathError(ValueError):
    """A RouterInfo is too short or has an invalid identity certificate shape."""


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
