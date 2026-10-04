#!/usr/bin/env python3
"""Independently re-derive encrypted LeaseSet2 layer values from the frozen specification.

This is a corroborating cross-check, not an official vector. The encrypted LeaseSet
specification publishes no test vectors for the credential, the subcredential, the layer key
derivations, or the layer ciphertexts. Everything here is written directly from the pinned
specification text and shares no code with the Rust implementation, so agreement is evidence
and a disagreement is a real finding.

Only the standard library is used: `hashlib` for SHA-256, `hmac` for HMAC-SHA256, and a
ChaCha20 written from RFC 8439. The blinded public key is an *input* to these cases rather
than a derived value, because the layer derivations depend on the blinded key but not on
the group arithmetic that produces it; the Ed25519 point arithmetic for `GENERATE_ALPHA` is
already covered by tools/generate-red25519-independent-fixture.py.

Usage: python3 tools/generate-els2-independent-fixture.py
"""

import hashlib
import hmac
import json
import os

UNBLINDED_SIGTYPE_RED25519 = 11
UNBLINDED_SIGTYPE_ED25519 = 7
BLINDED_SIGTYPE = 11

INNER_LEASESET2_STORE_TYPE = 3
INNER_META_LEASESET2_STORE_TYPE = 7


# --- primitives ---------------------------------------------------------------------------------


def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def hkdf(salt: bytes, ikm: bytes, info: bytes, length: int) -> bytes:
    """RFC 5869 extract-and-expand, the construction the I2P specifications call HKDF."""
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    okm = b""
    block = b""
    counter = 1
    while len(okm) < length:
        block = hmac.new(prk, block + info + bytes([counter]), hashlib.sha256).digest()
        okm += block
        counter += 1
    return okm[:length]


def _rotl32(value: int, count: int) -> int:
    value &= 0xFFFFFFFF
    return ((value << count) | (value >> (32 - count))) & 0xFFFFFFFF


def _quarter_round(state, a: int, b: int, c: int, d: int) -> None:
    state[a] = (state[a] + state[b]) & 0xFFFFFFFF
    state[d] = _rotl32(state[d] ^ state[a], 16)
    state[c] = (state[c] + state[d]) & 0xFFFFFFFF
    state[b] = _rotl32(state[b] ^ state[c], 12)
    state[a] = (state[a] + state[b]) & 0xFFFFFFFF
    state[d] = _rotl32(state[d] ^ state[a], 8)
    state[c] = (state[c] + state[d]) & 0xFFFFFFFF
    state[b] = _rotl32(state[b] ^ state[c], 7)


def chacha20_xor(key: bytes, nonce: bytes, data: bytes, initial_counter: int = 1) -> bytes:
    """RFC 8439 stream cipher with a 12-byte nonce, starting at `initial_counter`.

    The encrypted LeaseSet2 specification pins the initial counter to 1, so the default
    here is 1 and the cases below rely on that.
    """
    if len(key) != 32:
        raise ValueError("key must be 32 bytes")
    if len(nonce) != 12:
        raise ValueError("nonce must be 12 bytes")
    constants = (0x61707865, 0x3320646E, 0x79622D32, 0x6B206574)
    key_words = [int.from_bytes(key[i : i + 4], "little") for i in range(0, 32, 4)]
    nonce_words = [int.from_bytes(nonce[i : i + 4], "little") for i in range(0, 12, 4)]
    counter = initial_counter
    out = bytearray()
    for offset in range(0, len(data), 64):
        state = list(constants) + key_words + [counter] + nonce_words
        working = list(state)
        for _ in range(10):
            _quarter_round(working, 0, 4, 8, 12)
            _quarter_round(working, 1, 5, 9, 13)
            _quarter_round(working, 2, 6, 10, 14)
            _quarter_round(working, 3, 7, 11, 15)
            _quarter_round(working, 0, 5, 10, 15)
            _quarter_round(working, 1, 6, 11, 12)
            _quarter_round(working, 2, 7, 8, 13)
            _quarter_round(working, 3, 4, 9, 14)
        block = b"".join(
            ((working[i] + state[i]) & 0xFFFFFFFF).to_bytes(4, "little") for i in range(16)
        )
        chunk = data[offset : offset + 64]
        out += bytes(x ^ y for x, y in zip(chunk, block))
        counter += 1
    return bytes(out)


# --- encrypted LeaseSet2 derivations -------------------------------------------------------------


def credential(unblinded_pubkey: bytes, unblinded_sigtype: int) -> bytes:
    """H("credential", A || stA_be16 || 0x000b_be16)"""
    keydata = unblinded_pubkey + unblinded_sigtype.to_bytes(2, "big") + BLINDED_SIGTYPE.to_bytes(2, "big")
    return sha256(b"credential" + keydata)


def subcredential(unblinded_pubkey: bytes, unblinded_sigtype: int, blinded_pubkey: bytes) -> bytes:
    cred = credential(unblinded_pubkey, unblinded_sigtype)
    return sha256(b"subcredential" + cred + blinded_pubkey)


def layer_keys(salt: bytes, layer_input: bytes, info: bytes) -> tuple:
    """HKDF(salt, input, info, 44) sliced as key = okm[0:32], iv = okm[32:44]."""
    okm = hkdf(salt, layer_input, info, 44)
    return okm[0:32], okm[32:44]


def layer1_input(subcred: bytes, published: int) -> bytes:
    return subcred + published.to_bytes(4, "big")


def layer2_input(subcred: bytes, published: int, auth_cookie: bytes = b"") -> bytes:
    return auth_cookie + subcred + published.to_bytes(4, "big")


def encrypt_no_auth(
    subcred: bytes, published: int, inner_store_type: int, inner_record: bytes, outer_salt: bytes, inner_salt: bytes
) -> dict:
    """Build the full no-client-authorization outer ciphertext, layer by layer."""
    layer2_plaintext = bytes([inner_store_type]) + inner_record
    inner_key, inner_iv = layer_keys(inner_salt, layer2_input(subcred, published), b"ELS2_L2K")
    inner_ciphertext = inner_salt + chacha20_xor(inner_key, inner_iv, layer2_plaintext)

    layer1_plaintext = bytes([0]) + inner_ciphertext
    outer_key, outer_iv = layer_keys(outer_salt, layer1_input(subcred, published), b"ELS2_L1K")
    outer_ciphertext = outer_salt + chacha20_xor(outer_key, outer_iv, layer1_plaintext)

    return {
        "inner_salt": inner_salt.hex(),
        "inner_ciphertext": inner_ciphertext.hex(),
        "inner_key": inner_key.hex(),
        "inner_iv": inner_iv.hex(),
        "layer1_plaintext": layer1_plaintext.hex(),
        "outer_salt": outer_salt.hex(),
        "outer_ciphertext": outer_ciphertext.hex(),
        "outer_key": outer_key.hex(),
        "outer_iv": outer_iv.hex(),
    }


# --- cases ---------------------------------------------------------------------------------------

# [1]B, the Ed25519 base point in compressed form. A valid prime-order point, so the Rust
# side can decode it as a blinded public key without this generator needing group arithmetic.
BASE_POINT_COMPRESSED = bytes([0x58]) + bytes([0x66] * 31)

PUBLISHED = 1_700_000_000


def pattern(offset: int, length: int) -> bytes:
    return bytes(((offset + index) * 37 + 11) & 0xFF for index in range(length))


CASES = [
    {
        "name": "type-11-unblinded-no-secret-400-byte-inner",
        "unblinded_sigtype": UNBLINDED_SIGTYPE_RED25519,
        "unblinded_pubkey": BASE_POINT_COMPRESSED,
        "blinded_pubkey": BASE_POINT_COMPRESSED,
        "published": PUBLISHED,
        "outer_salt": pattern(0, 32),
        "inner_salt": pattern(32, 32),
        "inner_store_type": INNER_LEASESET2_STORE_TYPE,
        "inner_record": pattern(64, 400),
    },
    {
        "name": "type-7-unblinded-no-secret-single-byte-inner",
        "unblinded_sigtype": UNBLINDED_SIGTYPE_ED25519,
        "unblinded_pubkey": BASE_POINT_COMPRESSED,
        "blinded_pubkey": BASE_POINT_COMPRESSED,
        "published": PUBLISHED,
        "outer_salt": pattern(128, 32),
        "inner_salt": pattern(160, 32),
        "inner_store_type": INNER_LEASESET2_STORE_TYPE,
        "inner_record": pattern(192, 1),
    },
    {
        "name": "type-11-unblinded-meta-inner",
        "unblinded_sigtype": UNBLINDED_SIGTYPE_RED25519,
        "unblinded_pubkey": BASE_POINT_COMPRESSED,
        "blinded_pubkey": BASE_POINT_COMPRESSED,
        "published": PUBLISHED + 1,
        "outer_salt": pattern(256, 32),
        "inner_salt": pattern(288, 32),
        "inner_store_type": INNER_META_LEASESET2_STORE_TYPE,
        "inner_record": pattern(320, 733),
    },
    {
        "name": "type-11-distinct-keys-and-all-zero-salts",
        "unblinded_sigtype": UNBLINDED_SIGTYPE_RED25519,
        "unblinded_pubkey": pattern(512, 32),
        "blinded_pubkey": BASE_POINT_COMPRESSED,
        "published": 0,
        "outer_salt": bytes(32),
        "inner_salt": bytes(32),
        "inner_store_type": INNER_LEASESET2_STORE_TYPE,
        "inner_record": pattern(544, 64),
    },
    {
        "name": "type-11-block-boundary-inner-64-bytes",
        "unblinded_sigtype": UNBLINDED_SIGTYPE_RED25519,
        "unblinded_pubkey": BASE_POINT_COMPRESSED,
        "blinded_pubkey": BASE_POINT_COMPRESSED,
        "published": 2_147_483_647,
        "outer_salt": pattern(608, 32),
        "inner_salt": pattern(640, 32),
        "inner_store_type": INNER_LEASESET2_STORE_TYPE,
        "inner_record": pattern(672, 64),
    },
]


def main() -> None:
    cases = []
    for case in CASES:
        unblinded = case["unblinded_pubkey"]
        blinded = case["blinded_pubkey"]
        sigtype = case["unblinded_sigtype"]
        cred = credential(unblinded, sigtype)
        subcred = subcredential(unblinded, sigtype, blinded)
        layers = encrypt_no_auth(
            subcred,
            case["published"],
            case["inner_store_type"],
            case["inner_record"],
            case["outer_salt"],
            case["inner_salt"],
        )
        record = {
            "name": case["name"],
            "unblinded_sigtype": sigtype,
            "unblinded_pubkey": unblinded.hex(),
            "blinded_pubkey": blinded.hex(),
            "published": case["published"],
            "inner_store_type": case["inner_store_type"],
            "inner_record": case["inner_record"].hex(),
            "credential": cred.hex(),
            "subcredential": subcred.hex(),
        }
        record.update(layers)
        cases.append(record)

    # A row that pins the counter: the same inputs at initial counter 0 must differ.
    subcred = subcredential(BASE_POINT_COMPRESSED, UNBLINDED_SIGTYPE_RED25519, BASE_POINT_COMPRESSED)
    okm = hkdf(pattern(0, 32), layer1_input(subcred, PUBLISHED), b"ELS2_L1K", 44)
    control = chacha20_xor(okm[0:32], okm[32:44], bytes(64), initial_counter=0)
    cases.append(
        {
            "name": "counter-zero-control",
            "note": (
                "the same layer-1 key material applied at initial counter 0 instead of the "
                "pinned 1; the counter-1 ciphertext of the same plaintext differs, which is "
                "what makes the specification's counter observable"
            ),
            "outer_key": okm[0:32].hex(),
            "outer_iv": okm[32:44].hex(),
            "subcredential": subcred.hex(),
            "counter_zero_keystream": control.hex(),
        }
    )

    document = {
        "schema": "i2pr-els2-independent-derivation/1",
        "provenance": {
            "source": "local independent re-derivation from the frozen encrypted LeaseSet specification",
            "generator": "tools/generate-els2-independent-fixture.py",
            "spec_revision": "8baa1d680db263941daf2fb4462fbd75ba01c47f",
            "generated_on": "2026-10-04",
            "independence": (
                "shares no code with the Rust implementation; standard-library SHA-256 and "
                "HMAC-SHA256 plus an RFC 8439 ChaCha20 written from the RFC text"
            ),
            "authority": (
                "corroborating cross-check only; the encrypted LeaseSet specification "
                "publishes no official vectors for the credential, the subcredential, the "
                "layer key derivations, or the layer ciphertexts"
            ),
            "license": "CC0-1.0 locally authored",
        },
        "cases": cases,
    }

    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(
        root, "crates", "i2pr-netdb", "tests", "data", "els2-independent-derivation.json"
    )
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(document, indent=2) + "\n")
    print(f"wrote {path} with {len(cases)} cases")


if __name__ == "__main__":
    main()
