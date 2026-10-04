#!/usr/bin/env python3
"""Independently re-derive Red25519 blinding values from the frozen specification.

This is a corroborating cross-check, not an official vector: the I2P Red25519 and encrypted
LeaseSet specifications publish no test vectors for GENERATE_ALPHA, BLIND_PUBKEY, or the
blinded storage key. The arithmetic here is written directly from the pinned specification
text and shares no code with the Rust implementation, so agreement is evidence and a
disagreement is a real finding.

Usage: python3 tools/generate-red25519-independent-fixture.py
"""

import hashlib
import hmac
import json
import os

# --- Ed25519 group arithmetic, written from RFC 8032 (used only to re-derive blinded keys) ---

P = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = (-121665 * pow(121666, P - 2, P)) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)


def inv(x: int) -> int:
    return pow(x, P - 2, P)


def x_recover(y: int) -> int:
    xx = (y * y - 1) * inv(D * y * y + 1)
    x = pow(xx, (P + 3) // 8, P)
    if (x * x - xx) % P != 0:
        x = (x * SQRT_M1) % P
    if x % 2 != 0:
        x = P - x
    return x


BASE_Y = 4 * inv(5) % P
BASE = (x_recover(BASE_Y), BASE_Y, 1, x_recover(BASE_Y) * BASE_Y % P)


def edwards_add(pt1, pt2):
    x1, y1, z1, t1 = pt1
    x2, y2, z2, t2 = pt2
    a = (y1 - x1) * (y2 - x2) % P
    b = (y1 + x1) * (y2 + x2) % P
    c = t1 * 2 * D * t2 % P
    dd = z1 * 2 * z2 % P
    e, f, g, h = b - a, dd - c, dd + c, b + a
    return (e * f % P, g * h % P, f * g % P, e * h % P)


def scalarmult(point, scalar):
    result = (0, 1, 1, 0)
    addend = point
    while scalar > 0:
        if scalar & 1:
            result = edwards_add(result, addend)
        addend = edwards_add(addend, addend)
        scalar >>= 1
    return result


def encode_point(point) -> bytes:
    x, y, z, _ = point
    zinv = inv(z)
    x = x * zinv % P
    y = y * zinv % P
    return int(y | ((x & 1) << 255)).to_bytes(32, "little")


def decode_point(raw: bytes):
    value = int.from_bytes(raw, "little")
    sign = value >> 255
    y = value & ((1 << 255) - 1)
    if y >= P:
        return None
    x = x_recover(y)
    if x & 1 != sign:
        x = P - x
    point = (x, y, 1, x * y % P)
    if (-x * x + y * y - 1 - D * x * x * y * y) % P != 0:
        return None
    return point


# --- Specification derivations ---

def hstar(prefix1: bytes, prefix2: bytes, message: bytes) -> int:
    length = len(message).to_bytes(2, "little")
    digest = hashlib.sha512(b"I2P_Red25519H(x)" + prefix1 + prefix2 + length + message).digest()
    return int.from_bytes(digest, "little") % L


def hkdf_sha256(salt: bytes, ikm: bytes, info: bytes, length: int) -> bytes:
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    okm = b""
    block = b""
    counter = 1
    while len(okm) < length:
        block = hmac.new(prk, block + info + bytes([counter]), hashlib.sha256).digest()
        okm += block
        counter += 1
    return okm[:length]


def generate_alpha(public_key: bytes, sigtype: int, day: bytes, secret: bytes) -> int:
    keydata = public_key + sigtype.to_bytes(2, "big") + (11).to_bytes(2, "big")
    salt = hashlib.sha256(b"I2PGenerateAlpha" + keydata).digest()
    seed = hkdf_sha256(salt, day + secret, b"i2pblinding1", 64)
    return int.from_bytes(seed, "little") % L


def blind_public_key(public_key: bytes, alpha: int) -> bytes:
    point = decode_point(public_key)
    return encode_point(edwards_add(point, scalarmult(BASE, alpha)))


def storage_key(blinded: bytes) -> bytes:
    return hashlib.sha256((11).to_bytes(2, "big") + blinded).digest()


def convert_ed25519_private(seed: bytes) -> bytes:
    digest = bytearray(hashlib.sha512(seed).digest()[:32])
    digest[0] &= 248
    digest[31] = (digest[31] & 63) | 64
    return bytes(digest)


def main() -> None:
    cases = []
    for index, (seed_byte, sigtype, day, secret) in enumerate(
        [
            (1, 7, b"20251015", b""),
            (2, 7, b"20251015", b"lookup-secret"),
            (3, 11, b"20250101", b""),
            (4, 11, b"20240229", b"another secret"),
            (5, 7, b"19700101", b"x" * 64),
        ],
        start=1,
    ):
        seed = bytes([seed_byte]) * 32
        unblinded_private = convert_ed25519_private(seed)
        unblinded_public = encode_point(scalarmult(BASE, int.from_bytes(unblinded_private, "little")))
        alpha = generate_alpha(unblinded_public, sigtype, day, secret)
        blinded_public = blind_public_key(unblinded_public, alpha)
        cases.append(
            {
                "id": index,
                "edsk": seed.hex(),
                "sk": unblinded_private.hex(),
                "vk": unblinded_public.hex(),
                "unblinded_sigtype": sigtype,
                "day": day.decode(),
                "lookup_secret": secret.decode(),
                "alpha": alpha.to_bytes(32, "little").hex(),
                "blinded_public_key": blinded_public.hex(),
                "storage_key": storage_key(blinded_public).hex(),
            }
        )

    document = {
        "schema": "i2pr-red25519-independent-derivation/1",
        "provenance": {
            "source": "local independent re-derivation from the frozen Red25519 and encrypted LeaseSet specifications",
            "generator": "tools/generate-red25519-independent-fixture.py",
            "spec_revision": "8baa1d680db263941daf2fb4462fbd75ba01c47f",
            "generated_on": "2026-10-04",
            "independence": "shares no code with the Rust implementation; pure-Python SHA-256/HMAC-SHA256 and RFC 8032 group arithmetic",
            "authority": "corroborating cross-check only; the I2P specifications publish no official vectors for these operations",
            "license": "CC0-1.0 locally authored",
        },
        "cases": cases,
    }

    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(root, "crates", "i2pr-crypto", "tests", "data", "red25519-independent-derivation.json")
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(document, indent=2) + "\n")
    print(f"wrote {path} with {len(cases)} cases")


if __name__ == "__main__":
    main()
