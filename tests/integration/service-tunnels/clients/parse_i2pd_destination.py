#!/usr/bin/env python3
"""Plan 211 — extract i2pd PrivateKeys (.dat) public destination.

i2pd PrivateKeys layout (per i2pd 2.61.0
`libi2pd/IdentityEx::ToBuffer` + `PrivateKeys::ToBuffer`):

    - 387 bytes: standard identity (256 byte publicKey + 128 byte
      signingKey + 3 byte certificate header).
    - N bytes: extended certificate data, where N is the BE16
      integer at certificate header offset 1
      (`IdentityEx::m_ExtendedLen`). For ECIES_X25519_AEAD +
      Ed25519 (i2pd 2.61.0 default), N = 4 and the extended cert
      contains the signing key type (BE16) and crypto key type
      (BE16).
    - cryptoKeyLen bytes: crypto private key (256 for ElGamal /
      ECIES P256, 32 for ECIES_X25519_AEAD).
    - signingPrivateKeySize bytes: signing private key (32 for
      Ed25519, 32 for ECDSA P256, etc.).

The public destination is the canonical SHA-256 hash of the first
387 + N bytes (`IdentityEx::GetFullLen()`). The first 387 + N
bytes are the canonical `IdentityEx` encoding that the SAM `PUB`
base64 string is derived from.

This helper is used by the Plan 211 harness (`run-independent.sh`)
to read the i2pd-generated `keys =` files for the HTTP / IRC
server tunnels without parsing private material: only the public
parts (hash + b32 + base64) cross the trust boundary.

Usage:
    parse_i2pd_destination.py <private-key-file>
"""

import base64
import hashlib
import sys


def parse(path):
    with open(path, "rb") as handle:
        data = handle.read()
    if len(data) < 387 + 2:
        raise SystemExit(
            f"{path}: file too small ({len(data)} bytes) for i2pd PrivateKeys header"
        )
    cert_len = int.from_bytes(data[385:387], "big")
    pub_len = 387 + cert_len
    if len(data) < pub_len:
        raise SystemExit(
            f"{path}: file too small for cert_len={cert_len}, got {len(data)} bytes"
        )
    pub_bytes = bytes(data[:pub_len])
    dest_hash = hashlib.sha256(pub_bytes).digest()
    dest_b32_raw = base64.b32encode(dest_hash).decode("ascii").rstrip("=").lower()
    return {
        "pub_len": pub_len,
        "cert_len": cert_len,
        "dest_hash": dest_hash.hex(),
        "dest_b32": f"{dest_b32_raw}.b32.i2p",
        # I2P SAM base64 uses `-` / `~` instead of `+` / `/` so the
        # output is byte-compatible with i2pr's `i2pr_api::sam::base64`.
        "dest_b64": _i2p_b64encode(pub_bytes),
    }


def _i2p_b64encode(data: bytes) -> str:
    alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~"
    total = len(data)
    tail = total % 3
    main_end = total - tail if tail else total
    out = bytearray()
    for offset in range(0, main_end, 3):
        b0 = data[offset]
        b1 = data[offset + 1]
        b2 = data[offset + 2]
        n = (b0 << 16) | (b1 << 8) | b2
        out.append(alphabet[(n >> 18) & 0x3F])
        out.append(alphabet[(n >> 12) & 0x3F])
        out.append(alphabet[(n >> 6) & 0x3F])
        out.append(alphabet[n & 0x3F])
    if tail == 2:
        b0 = data[total - 2]
        b1 = data[total - 1]
        n = (b0 << 16) | (b1 << 8)
        out.append(alphabet[(n >> 18) & 0x3F])
        out.append(alphabet[(n >> 12) & 0x3F])
        out.append(alphabet[(n >> 6) & 0x3F])
        out.append(ord(b"="))
    elif tail == 1:
        b0 = data[total - 1]
        n = b0 << 16
        out.append(alphabet[(n >> 18) & 0x3F])
        out.append(alphabet[(n >> 12) & 0x3F])
        out.append(ord(b"="))
        out.append(ord(b"="))
    return bytes(out).decode("ascii")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.stderr.write("usage: parse_i2pd_destination.py <private-key-file>\n")
        sys.exit(2)
    info = parse(sys.argv[1])
    for key, value in info.items():
        # Use `:` as the separator so the base64 value (which may
        # end in `==` padding) is never re-split by the harness's
        # awk field splitter.
        print(f"{key}:{value}")