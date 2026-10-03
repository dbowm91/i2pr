#!/usr/bin/env python3
"""Plan 308 bounded, identity-free HTTP request capture canonicalizer.

Raw bytes belong in the caller's private scratch directory. This helper
retains only request shape, ordered header names/values, body length and
digest, and a digest of the complete request bytes. Destination authorities
are replaced with a fixed marker before they enter the canonical record.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

MAX_HEAD = 65_536
MAX_BODY = 1_048_576
AUTHORITY = re.compile(rb"(?i)(?:[a-z2-7]{52}\.b32\.i2p|[a-z0-9.-]+\.i2p)(?::[0-9]{1,5})?")


def canonicalize(raw: bytes, request_name: str) -> dict[str, object]:
    if len(raw) > MAX_HEAD + MAX_BODY:
        raise ValueError("capture-exceeds-bound")
    marker = raw.find(b"\r\n\r\n")
    if marker < 0 or marker + 4 > MAX_HEAD:
        raise ValueError("missing-or-overlong-header")
    head, body = raw[:marker], raw[marker + 4 :]
    lines = head.split(b"\r\n")
    try:
        method_b, target_b, version_b = lines[0].split(b" ")
    except ValueError as exc:
        raise ValueError("invalid-request-line") from exc
    if not method_b.isalpha() or version_b not in (b"HTTP/1.0", b"HTTP/1.1"):
        raise ValueError("invalid-request-line")
    if len(lines) > 129:
        raise ValueError("too-many-headers")
    headers: list[list[str]] = []
    lengths: list[int] = []
    for line in lines[1:]:
        if not line or line[:1] in (b" ", b"\t") or b":" not in line:
            raise ValueError("invalid-header-line")
        name, value = line.split(b":", 1)
        if not re.fullmatch(rb"[!#$%&'*+.^_`|~0-9A-Za-z-]+", name):
            raise ValueError("invalid-header-name")
        if any(byte < 0x20 and byte != 0x09 or byte == 0x7F for byte in value):
            raise ValueError("invalid-header-value")
        value = value.strip(b" \t")
        if name.lower() == b"content-length":
            if not value.isdigit():
                raise ValueError("invalid-content-length")
            lengths.append(int(value))
        if name.lower() == b"transfer-encoding":
            raise ValueError("unsupported-transfer-encoding")
        safe_value = AUTHORITY.sub(b"<FIXTURE_AUTHORITY>", value)
        headers.append([name.decode("ascii").lower(), safe_value.decode("latin-1")])
    if len(set(lengths)) > 1 or len(lengths) > 1:
        raise ValueError("ambiguous-content-length")
    body_length = lengths[0] if lengths else 0
    if body_length > MAX_BODY or len(body) != body_length:
        raise ValueError("body-length-mismatch-or-exceeds-bound")
    try:
        method = method_b.decode("ascii")
        target = target_b.decode("ascii")
    except UnicodeDecodeError as exc:
        raise ValueError("non-ascii-request-line") from exc
    if target.startswith("http://"):
        form = "absolute"
        authority_end = target.find("/", len("http://"))
        path = target[authority_end:] if authority_end >= 0 else "/"
        path = AUTHORITY.sub(b"<FIXTURE_AUTHORITY>", path.encode("ascii")).decode("ascii")
    else:
        form = "origin"
        path = target
    return {
        "name": request_name,
        "method": method,
        "target_form": form,
        "path": path,
        "headers": headers,
        "body_len": body_length,
        "body_sha256": hashlib.sha256(body).hexdigest() if body else "",
        "request_sha256": hashlib.sha256(raw).hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--request", action="append", nargs=2, metavar=("NAME", "FILE"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        corpus = tomllib.loads(args.corpus.read_text(encoding="utf-8"))
        expected = {row["name"] for row in corpus["request"]}
        actual = [name for name, _ in args.request]
        if len(actual) != len(set(actual)) or set(actual) != expected:
            raise ValueError("corpus-request-set-mismatch")
        requests = [
            canonicalize(Path(path).read_bytes(), name)
            for name, path in args.request
        ]
        record = {"schema": 1, "requests": requests}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
        return 0
    except (OSError, KeyError, TypeError, ValueError, tomllib.TOMLDecodeError) as exc:
        print(f"P308-CAPTURE-STOP {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
