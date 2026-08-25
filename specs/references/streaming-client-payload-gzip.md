# I2P Streaming client payload wire format (RFC 1952 gzip member)

The I2P I2CP client payload format is one canonical RFC 1952 gzip
member whose fixed 10-byte header is repurposed as follows:

```text
bytes 0..=2  = 1f 8b 08    (gzip magic + deflate compression method)
byte  3      = FLG         (must be 0; FTEXT/FHCRC/FEXTRA/FNAME/FCOMMENT rejected)
bytes 4..=5  = I2P source port        (big-endian)
bytes 6..=7  = I2P destination port   (big-endian)
byte  8      = XFL         (2 = maximum compression, matches Java output)
byte  9      = OS/Protocol (6 = Streaming, 17 = Datagram, etc.)
body          = raw DEFLATE stream (as defined by RFC 1951)
trailer       = RFC 1952 trailer: CRC-32 (LE) + ISIZE (LE)
```

There is no extra SHA-256 integrity field and no custom
compressed-length prefix. Optional gzip layouts (FTEXT/FHCRC/FEXTRA/
FNAME/FCOMMENT) are rejected.

## Frozen independently-derived fixture

`crates/i2pr-proto/src/streaming/payload.rs::known_good_payload_matches_i2p_destination_ports`
constructs a fixture from a hand-coded RFC 1951 §3.2.4 stored
deflate block (BFINAL=1, BTYPE=00, raw literal bytes) so the
decoder can be verified without depending on `flate2` (which the
encoder under test also uses). The CRC32 + ISIZE trailer is
computed by the test fixture itself.

Provenance:

```text
RFC 1951 §3.2.4 stored block format
RFC 1952 §2.2 CRC32 + ISIZE trailer (little-endian)
RFC 1952 §2.3.1 FLG flag byte (no optional fields)
I2P Streaming lib (Java): source port in bytes 4-5, dest port in 6-7
```

## Header byte order verification

The fixture uses `source_port = 0x1234` and
`destination_port = 0xabcd` so a byte-order reversal cannot pass
silently: the recovered envelope must carry exactly those two
values at the exact header offsets.

## Tests

```text
known_good_payload_matches_i2p_destination_ports
encoded_magic_is_1f_8b_08
source_destination_ports_round_trip
streaming_payload_round_trip_matches_source_bytes
gzip_crc_corruption_rejected
gzip_isize_corruption_rejected
bounded_decompression_bomb_rejected    (via bounded_uncompressed_size_rejects_huge_application_bytes)
zlib_wrapped_old_i2pr_format_is_rejected
trailing_bytes_are_rejected
```

The decoder fails closed on every malformed input and enforces the
per-payload byte ceiling during decompression, not after unlimited
growth.