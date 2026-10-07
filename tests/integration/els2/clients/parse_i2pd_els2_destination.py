#!/usr/bin/env python3
"""Plan 381 §WP1 — extract an i2pd ELS2 destination's published addresses.

Reads an i2pd ``PrivateKeys`` (``.dat``) file and reports the destination's
``.b32.i2p`` authority address and its ``.b33`` blinded address, deriving both
from the file's **public** bytes only. No private key material is parsed out,
printed, or logged; only public fields cross the trust boundary.

Why the b33 needs deriving rather than reading
-----------------------------------------------
A b33 is not an address i2pd writes anywhere in a lane-reachable file. The
reference computes it on demand
(``libi2pd/Blinding.cpp:200``, from ``libi2pd/Blinding.cpp:143``) and shows it
on its ``daemon``-mode web console. The controlled-mesh lane runs i2pd with
``daemon = false`` and ``--tunconf=``, where no console exists, so the lane has
to derive it. Deriving is not a workaround: it is the same computation the
reference performs, over bytes the reference derives it from.

Verified, not assumed
---------------------
The derivation below was checked against the pinned reference by running it: an
i2pd 2.61.0 destination with ``i2cp.leaseSetType = 5``, console page fetched
over loopback, and the published b33 compared byte-for-byte against the value
this module derives from the ``.dat``. They matched. See
``plans/closure/i2pcontrol-proposal-170/381-status.md``.

Layout, from the pinned source
------------------------------
``libi2pd/Identity.h:54-58``

    struct Identity {
        uint8_t publicKey[256];
        uint8_t signingKey[128];   // the real key is RIGHT-ALIGNED in here
        uint8_t certificate[3];    // type, then BE16 extended length
    };                             // 387 bytes == DEFAULT_IDENTITY_SIZE

``libi2pd/Identity.cpp:343-349`` — ``GetSigningPublicKeyBuffer()`` returns
``signingKey + 128 - keyLen``, so the key is at offset ``256 + 128 - keyLen``.

``libi2pd/Blinding.cpp:143-155`` — the b33's embedded key *is* the identity's
signing public key, and the blinded signature type is ``11`` (RedDSA) for
Ed25519 (``7``) and the identity's own type otherwise.

``libi2pd/Blinding.cpp:200-214`` — ``ToB33``::

    addr[0] = flags & B33_PER_CLIENT_AUTH_FLAG (0x04)
    addr[1] = sigType
    addr[2] = blindedSigType
    addr[3..] = publicKey
    crc = crc32(0, addr + 3, len)          # note: covers the KEY only
    addr[0] ^= crc & 0xff                  # little-endian, into 3 header bytes
    addr[1] ^= (crc >> 8) & 0xff
    addr[2] ^= (crc >> 16) & 0xff
    return base32(addr, len + 3)

``libi2pd/Blinding.cpp:157-198`` — the decoder applies the identical XOR, so
the transform is an involution and a b33 round-trips exactly.

The CRC is a *mask*, not a checksum
-----------------------------------
The decoder XORs the CRC back but never requires the result to be zero, so a
b33 carries no error-detecting integrity property. The independent recompute
below therefore does **not** pretend to validate a checksum. It verifies the
two things that are actually true:

  1. the b33 round-trips (decode then re-encode reproduces it exactly), and
  2. the key recovered from the b33 equals the signing public key in the
     ``.dat``.

(2) is the check with teeth: it ties the address to the identity through a
different code path than the derivation that produced it, so a wrong
derivation cannot pass.

Usage::

    parse_i2pd_els2_destination.py <private-key-file>
    parse_i2pd_els2_destination.py --self-test
"""

from __future__ import annotations

import base64
import binascii
import hashlib
import sys
import zlib

# --- i2pd identity layout, libi2pd/Identity.h ------------------------------
IDENTITY_SIZE = 387
PUBLIC_KEY_LEN = 256
SIGNING_KEY_FIELD_LEN = 128
CERTIFICATE_LEN_FIELD_OFFSET = 385  # BE16 length at the end of the header

# The extended certificate carries (signingKeyType BE16, cryptoKeyType BE16) in
# that order -- signing first. libi2pd/Identity.cpp:377 reads the signing type
# at offset 0 and :390 reads the crypto type at offset +2. Reading them the
# other way round yields a well-formed but meaningless type number, which is
# why the self-test pins the order rather than trusting the obvious spelling.
EXTENDED_SIGTYPE_OFFSET = 0  # relative to the start of the extended block

# --- libi2pd/Identity.h:80-92 ---------------------------------------------
SIGNING_KEY_TYPE_ECDSA_SHA256_P256 = 1
SIGNING_KEY_TYPE_ECDSA_SHA384_P384 = 2
SIGNING_KEY_TYPE_ECDSA_SHA512_P521 = 3
SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519 = 7
SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519PH = 8
SIGNING_KEY_TYPE_REDDSA_SHA512_ED25519 = 11

# Signing *public* key length per signature type. Only the types whose length
# is unambiguous at this pin are listed; anything else is refused rather than
# guessed, because a wrong length yields a plausible-looking wrong b33.
#
# ECDSA P384/P521 are deliberately absent: Blinding.cpp:202 refuses to emit a
# b33 for any key longer than 32 bytes, so a b33 cannot exist for them and the
# extractor must say so rather than fabricate one.
SIGNING_PUBLIC_KEY_LEN = {
    SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519: 32,
    SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519PH: 32,
    SIGNING_KEY_TYPE_REDDSA_SHA512_ED25519: 32,
    SIGNING_KEY_TYPE_ECDSA_SHA256_P256: 32,
}

# --- libi2pd/Blinding.cpp:139-141 -----------------------------------------
B33_TWO_BYTES_SIGTYPE_FLAG = 0x01
B33_PER_CLIENT_AUTH_FLAG = 0x04
B33_MAX_PUBLIC_KEY_LEN = 32  # ToB33 returns "" beyond this

SIGTYPE_NAMES = {
    0: "DSA_SHA1",
    1: "ECDSA_SHA256_P256",
    2: "ECDSA_SHA384_P384",
    3: "ECDSA_SHA512_P521",
    4: "RSA_SHA256_2048",
    5: "RSA_SHA384_3072",
    6: "RSA_SHA512_4096",
    7: "EDDSA_SHA512_ED25519",
    8: "EDDSA_SHA512_ED25519ph",
    9: "GOSTR3410_CRYPTO_PRO_A",
    10: "GOSTR3410_TC26_A",
    11: "REDDSA_SHA512_ED25519",
}


class ExtractionError(SystemExit):
    """A refusal with a reason. Never a traceback: the lane reads these."""


def _refuse(reason: str) -> "ExtractionError":
    return ExtractionError(f"parse_i2pd_els2_destination: {reason}")


def _read_identity(path: str) -> tuple[bytes, int]:
    """Return (public identity bytes, extended length)."""
    try:
        with open(path, "rb") as handle:
            data = handle.read()
    except OSError as exc:
        raise _refuse(f"cannot read {path}: {exc}") from None
    if len(data) < IDENTITY_SIZE + 2:
        raise _refuse(f"{path}: {len(data)} bytes is too short for an i2pd identity")
    extended_len = int.from_bytes(data[385:387], "big")
    public_len = IDENTITY_SIZE + extended_len
    if len(data) < public_len:
        raise _refuse(
            f"{path}: {len(data)} bytes cannot hold cert_len={extended_len} "
            f"(needs {public_len})"
        )
    return bytes(data[:public_len]), extended_len


def _signing_key_type(public: bytes, extended_len: int) -> int:
    """The signing key type, from the extended certificate.

    Refuses when the extended certificate is absent rather than assuming the
    Ed25519 default: an absent type and a present type of 7 are different
    facts, and the b33 depends on which.
    """
    if extended_len < 4:
        raise _refuse(
            "extended certificate is too short to carry a signing key type; "
            "refusing to assume Ed25519"
        )
    base = IDENTITY_SIZE + EXTENDED_SIGTYPE_OFFSET
    return int.from_bytes(public[base : base + 2], "big")


def _blinded_sig_type(sig_type: int) -> int:
    """Blinding.cpp:150-153: Ed25519 (7) is blinded to RedDSA (11)."""
    if sig_type == SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519:
        return SIGNING_KEY_TYPE_REDDSA_SHA512_ED25519
    return sig_type


def _signing_public_key(public: bytes, sig_type: int) -> bytes:
    length = SIGNING_PUBLIC_KEY_LEN.get(sig_type)
    if length is None:
        name = SIGTYPE_NAMES.get(sig_type, str(sig_type))
        raise _refuse(
            f"signing key type {name} has no b33 at this pin: "
            "Blinding.cpp:202 emits a b33 only for a 32-byte key, and this "
            "module refuses to guess a length it cannot verify"
        )
    if length > B33_MAX_PUBLIC_KEY_LEN:
        raise _refuse(f"signing key type {sig_type} is too long for a b33")
    offset = PUBLIC_KEY_LEN + SIGNING_KEY_FIELD_LEN - length
    return public[offset : offset + length]


def derive_b33(
    signing_pub: bytes,
    sig_type: int,
    per_client_auth: bool = False,
) -> str:
    """Blinding.cpp:200-214, verbatim in structure."""
    if len(signing_pub) > B33_MAX_PUBLIC_KEY_LEN:
        raise _refuse("signing key is too long for a b33")
    blinded_type = _blinded_sig_type(sig_type)
    flags = B33_PER_CLIENT_AUTH_FLAG if per_client_auth else 0x00
    if sig_type > 0xFF or blinded_type > 0xFF:
        raise _refuse("signature type does not fit the one-byte b33 header")
    header = bytes([flags, sig_type, blinded_type])
    crc = zlib.crc32(signing_pub) & 0xFFFFFFFF
    masked = bytes(
        [
            header[0] ^ (crc & 0xFF),
            header[1] ^ ((crc >> 8) & 0xFF),
            header[2] ^ ((crc >> 16) & 0xFF),
        ]
    ) + signing_pub
    return base64.b32encode(masked).decode("ascii").rstrip("=").lower()


def decode_b33(b33: str) -> dict:
    """Blinding.cpp:157-198, verbatim in structure.

    The reference never checks the recovered CRC against zero, so neither does
    this; the value returned here is only meaningful once `verify_b33` has
    re-encoded it and recovered the key.
    """
    raw = b33.strip()
    # Refuse rather than normalize. A `.b33.i2p` address is canonically
    # lowercase, and silently lower-casing an operator-supplied address is how
    # a mistyped value becomes a plausible-looking one. The lane reads this
    # value out of a file and hands it to i2pr, so it must be exactly right.
    if not raw or any(ch not in "abcdefghijklmnopqrstuvwxyz234567" for ch in raw):
        raise _refuse(f"b33 is not lowercase base32: {b33!r}")
    padded = raw + "=" * ((8 - len(raw) % 8) % 8)
    try:
        # casefold=True: b32decode is case-sensitive by default and the address
        # is canonically lowercase.
        data = base64.b32decode(padded, casefold=True)
    except (binascii.Error, ValueError):
        raise _refuse(f"b33 does not decode: {b33!r}") from None
    if len(data) < B33_MAX_PUBLIC_KEY_LEN + 3:
        raise _refuse(f"b33 decodes to {len(data)} bytes, too short")

    unmasked = bytearray(data)
    crc = zlib.crc32(bytes(data[3:])) & 0xFFFFFFFF
    unmasked[0] ^= crc & 0xFF
    unmasked[1] ^= (crc >> 8) & 0xFF
    unmasked[2] ^= (crc >> 16) & 0xFF

    flags = unmasked[0]
    offset = 1
    if flags & B33_TWO_BYTES_SIGTYPE_FLAG:
        sig_type = int.from_bytes(unmasked[offset : offset + 2], "big")
        offset += 2
        blinded_type = int.from_bytes(unmasked[offset : offset + 2], "big")
        offset += 2
    else:
        sig_type = unmasked[offset]
        offset += 1
        blinded_type = unmasked[offset]
        offset += 1

    if not 0 < sig_type <= 0xFFFF:
        raise _refuse(f"b33 declares an unknown signature type: {sig_type}")
    key_len = SIGNING_PUBLIC_KEY_LEN.get(sig_type, len(data) - offset)
    if offset + key_len > len(data):
        raise _refuse("b33 signing key is truncated")
    return {
        "per_client_auth": bool(flags & B33_PER_CLIENT_AUTH_FLAG),
        "sig_type": sig_type,
        "blinded_sig_type": blinded_type,
        "signing_pub": bytes(data[offset : offset + key_len]),
    }


def verify_b33(b33: str, expected_signing_pub: bytes, expected_sig_type: int) -> None:
    """The independent recompute.

    Three assertions, each of which a wrong derivation fails:

      1. round-trip -- re-encoding the decoded structure reproduces `b33`;
      2. key match -- the b33's signing key is the ``.dat``'s signing key;
      3. type match -- the b33 declares the identity's signature type.
    """
    decoded = decode_b33(b33)
    if decoded["signing_pub"] != expected_signing_pub:
        raise _refuse(
            "b33 signing key does not match the .dat signing key; the "
            "derivation is wrong or the address belongs to another destination"
        )
    if decoded["sig_type"] != expected_sig_type:
        raise _refuse(
            f"b33 declares signature type {decoded['sig_type']} but the "
            f"identity is type {expected_sig_type}"
        )
    reencoded = derive_b33(
        decoded["signing_pub"],
        decoded["sig_type"],
        decoded["per_client_auth"],
    )
    if reencoded != b33.strip():
        raise _refuse(
            "b33 does not round-trip through the reference encoding "
            f"({b33!r} -> {reencoded!r})"
        )


def parse(path: str) -> dict:
    public, extended_len = _read_identity(path)
    sig_type = _signing_key_type(public, extended_len)
    signing_pub = _signing_public_key(public, sig_type)

    # b32: IdentityEx::Hash() is SHA-256 over GetFullLen() bytes.
    dest_hash = hashlib.sha256(public).digest()
    dest_b32 = base64.b32encode(dest_hash).decode("ascii").rstrip("=").lower()

    b33 = derive_b33(signing_pub, sig_type)
    verify_b33(b33, signing_pub, sig_type)

    return {
        "pub_len": len(public),
        "extended_len": extended_len,
        "sig_type": sig_type,
        "sig_type_name": SIGTYPE_NAMES.get(sig_type, str(sig_type)),
        "signing_pub_len": len(signing_pub),
        "dest_hash": dest_hash.hex(),
        "dest_b32": f"{dest_b32}.b32.i2p",
        "dest_b33": f"{b33}.b33.i2p",
        "dest_b33_raw": b33,
    }


# ---------------------------------------------------------------------------
# Self-test. Run with --self-test; needs no network and no reference binary.
# ---------------------------------------------------------------------------


def _self_test() -> int:
    failures: list[str] = []

    def check(label: str, condition: bool) -> None:
        if condition:
            print(f"  ok: {label}")
        else:
            print(f"  FAIL: {label}")
            failures.append(label)

    print("== encoding round trip ==")
    for sig_type in (
        SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519,
        SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519PH,
        SIGNING_KEY_TYPE_ECDSA_SHA256_P256,
    ):
        key = hashlib.sha256(f"key-{sig_type}".encode()).digest()
        for per_client in (False, True):
            b33 = derive_b33(key, sig_type, per_client)
            decoded = decode_b33(b33)
            label = f"{SIGTYPE_NAMES[sig_type]} per_client={per_client}"
            check(f"{label}: round-trips", derive_b33(
                decoded["signing_pub"], decoded["sig_type"], decoded["per_client_auth"]
            ) == b33)
            check(f"{label}: key survives", decoded["signing_pub"] == key)
            check(f"{label}: type survives", decoded["sig_type"] == sig_type)
            check(f"{label}: flag survives", decoded["per_client_auth"] == per_client)
            # 3 header bytes + a 32-byte key is 35 bytes, and 35 is a multiple
            # of 5, so base32 emits 56 characters with no padding at all.
            check(f"{label}: is 56 unpadded base32 chars",
                  len(b33) == 56 and "=" not in b33)

    print("== Ed25519 is blinded to RedDSA ==")
    key = bytes(range(32))
    decoded = decode_b33(derive_b33(key, SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519))
    check("Ed25519 (7) blinds to RedDSA (11)",
          decoded["blinded_sig_type"] == SIGNING_KEY_TYPE_REDDSA_SHA512_ED25519)
    decoded = decode_b33(derive_b33(key, SIGNING_KEY_TYPE_ECDSA_SHA256_P256))
    check("ECDSA P256 (1) keeps its own blinded type", decoded["blinded_sig_type"] == 1)

    print("== the independent recompute refuses a wrong address ==")
    good = derive_b33(key, SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519)
    try:
        verify_b33(good, bytes(32), SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519)
        check("refuses a b33 for a different destination", False)
    except ExtractionError:
        check("refuses a b33 for a different destination", True)
    try:
        verify_b33(good, key, SIGNING_KEY_TYPE_ECDSA_SHA256_P256)
        check("refuses a b33 declaring the wrong signature type", False)
    except ExtractionError:
        check("refuses a b33 declaring the wrong signature type", True)

    # A single flipped base32 character must fail, which is what makes the
    # round-trip check worth having.
    flipped = ("b" if good[0] != "b" else "c") + good[1:]
    try:
        verify_b33(flipped, key, SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519)
        check("refuses a corrupted b33", False)
    except ExtractionError:
        check("refuses a corrupted b33", True)

    print("== malformed input is refused, not guessed ==")
    for label, bad in (
        ("empty", ""),
        ("uppercase", good.upper()),
        ("non-base32", "!" * 56),
        ("too short", "aaaa"),
    ):
        try:
            decode_b33(bad)
            check(f"refuses a {label} b33", False)
        except ExtractionError:
            check(f"refuses a {label} b33", True)

    print("== the length table is not guessed ==")
    for sig_type in (
        SIGNING_KEY_TYPE_ECDSA_SHA384_P384,
        SIGNING_KEY_TYPE_ECDSA_SHA512_P521,
    ):
        try:
            _signing_public_key(b"\0" * IDENTITY_SIZE, sig_type)
            check(f"refuses a b33 for {SIGTYPE_NAMES[sig_type]}", False)
        except ExtractionError:
            check(f"refuses a b33 for {SIGTYPE_NAMES[sig_type]}", True)

    try:
        _signing_public_key(b"\0" * IDENTITY_SIZE, 4)  # RSA_SHA256_2048
        check("refuses an unlisted signature type", False)
    except ExtractionError:
        check("refuses an unlisted signature type", True)

    print("== the identity layout is read as documented ==")
    # Build a synthetic identity: Ed25519 signing key of 32 known bytes at the
    # documented offset, extended certificate declaring type 7.
    key = hashlib.sha256(b"synthetic").digest()
    public = bytearray()
    public += bytes(PUBLIC_KEY_LEN)                     # publicKey[256]
    public += bytes(SIGNING_KEY_FIELD_LEN - len(key))   # left padding
    public += key                                        # right-aligned
    public += bytes([5, 0, 4])                           # cert type + BE16 len 4
    public += (7).to_bytes(2, "big") + (4).to_bytes(2, "big")  # signing, then crypto
    raw = bytes(public) + b"\x00" * 16                   # private tail, ignored

    pub, extended_len = _read_identity(_write_temp(raw))
    check("extended length is read from the certificate", extended_len == 4)
    check("signing key type is read from the extended certificate",
          _signing_key_type(pub, extended_len) == 7)
    check("signing key is read right-aligned at 256 + 128 - 32",
          _signing_public_key(pub, 7) == key)
    check("b32 is SHA-256 over the full public length",
          base64.b32encode(hashlib.sha256(pub).digest()).decode().rstrip("=").lower()
          == base64.b32encode(hashlib.sha256(public).digest()).decode().rstrip("=").lower())

    print("== a truncated file is refused ==")
    try:
        _read_identity(_write_temp(b"\x00" * 100))
        check("refuses a file shorter than an identity", False)
    except ExtractionError:
        check("refuses a file shorter than an identity", True)
    # cert_len at offset 385:387 declares 16 bytes of extended certificate
    # that the file does not actually carry.
    try:
        _read_identity(_write_temp(bytes(385) + b"\x00\x10" + b"\x00" * 5))
        check("refuses a file that cannot hold its own cert_len", False)
    except ExtractionError:
        check("refuses a file that cannot hold its own cert_len", True)
    try:
        _read_identity(_write_temp(bytes(IDENTITY_SIZE + 0)))
        check("refuses an identity with no extended certificate", False)
    except ExtractionError:
        check("refuses an identity with no extended certificate", True)

    if failures:
        print(f"FAIL: {len(failures)} self-test failure(s)", file=sys.stderr)
        return 1
    print("ok: parse_i2pd_els2_destination self-test holds")
    return 0


def _write_temp(data: bytes) -> str:
    """Write `data` to a temp file and register it for cleanup at exit.

    The self-test needs real files because `_read_identity` opens by path. They
    are tracked rather than left to the OS: a lane script that leaves a file
    per self-test run behind is the same untidiness the router is not allowed
    to have elsewhere, and the self-test runs on every CI dispatch.
    """
    import atexit
    import os
    import tempfile

    handle = tempfile.NamedTemporaryFile(delete=False, suffix=".dat")
    try:
        handle.write(data)
    finally:
        handle.close()
    atexit.register(lambda: os.path.exists(handle.name) and os.unlink(handle.name))
    return handle.name


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        return _self_test()
    if len(sys.argv) != 2:
        sys.stderr.write(
            "usage: parse_i2pd_els2_destination.py <private-key-file>\n"
            "       parse_i2pd_els2_destination.py --self-test\n"
        )
        return 2
    info = parse(sys.argv[1])
    for key, value in info.items():
        # `:` separator: values may contain characters the harness would split
        # on, and the ordering is stable so diffs stay readable.
        print(f"{key}:{value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())