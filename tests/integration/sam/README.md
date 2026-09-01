# SAM 3.1 independent-client interoperability lane

This directory records the lightweight independent-client checks for
the Plan 143 / Plan 144 closure lane. It is localhost-only and does
not download, start, or configure an I2P router. The Rust loopback
tests remain the CI-capable product lane:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback --test sam_stream --test sam_forward_naming
```

Plan 142 closed the SAM Base64 / private-destination compatibility
sub-claim. See [`plans/142-status.md`](../../plans/142-status.md) for
the closure record; the `i2pr-api` SAM codec now uses the I2P Base64
alphabet (`A-Z a-z 0-9 - ~`, `=` padding) — the spelling every Java
I2P / i2pd / independent Python client reference implementation
emits. The Plan 142 reference-vector set in
`crates/i2pr-api/src/sam/base64.rs::tests` locks the alphabet,
padding, and length behavior; that is the routine development
evidence lane. Plan 143 (live same-socket STREAM CONNECT/ACCEPT
product bridge) and Plan 144 (two-independent-client final
Milestone 7 closure) remain open.

## External client provenance

The discovery source is the official I2P SAM API documentation:
<https://geti2p.net/en/docs/api/samv3>.

The two selected SAM 3.1 STREAM candidates are inspected from their
pinned upstream revisions, without copying source into this
repository:

| Client | Revision/version | Language | License | Local result |
| --- | --- | --- | --- | --- |
| `i2plib` | `6edf51cd5d21cc745aa7e23cb98c582144884fa8` (`v0.0.14`) | Python | MIT | imports; SAM wire helpers in `i2plib/sam.py` confirmed correct (HELLO/SESSION CREATE/STREAM CONNECT/STREAM ACCEPT/NAMING LOOKUP/DEST GENERATE/STREAM FORWARD). Used as one of three independent references for the SAM Base64 alphabet (`I2P_B64_CHARS = "-~"` in `i2plib/sam.py`). Used as Client A in the Plan 144 STREAM CONNECT/ACCEPT product probe. |
| `libsam3` | `e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7` (`v1.0.0`) | C | Mixed (public-domain + MIT components) | builds cleanly via `make build`; STREAM CONNECT+ACCEPT example `sam3/streamcs.c` available. Selected as Client B in the Plan 144 STREAM CONNECT/ACCEPT product probe. |
| `txi2p` | `0611b9a86172cb70d2f5e415a88eee9f230590b3` | Python/Twisted | ISC (`COPYING`) | import blocked locally by missing legacy `ometa` dependency; full STREAM CONNECT/ACCEPT product probe deferred to a successor plan |

The exact read-only inspection commands are:

```text
git clone https://github.com/l-n-s/i2plib /tmp/i2pr-sam-i2plib
git clone https://github.com/str4d/txi2p /tmp/i2pr-sam-txi2p
git -C /tmp/i2pr-sam-i2plib checkout 6edf51cd5d21cc745aa7e23cb98c582144884fa8
git -C /tmp/i2pr-sam-txi2p checkout 0611b9a86172cb70d2f5e415a88eee9f230590b3
PYTHONPATH=/tmp/i2pr-sam-i2plib python3 -c 'import i2plib'
PYTHONPATH=/tmp/i2pr-sam-txi2p python3 -c 'import txi2p'
```

## Plan 142 evidence lane (closed)

Plan 142 closes the SAM Base64 / private-destination compatibility
sub-claim using three independent reference implementations, none of
which informed the others:

1. **i2pd** (`PurpleI2P/i2pd`, `openssl` branch) —
   `libi2pd/Base.h::IsBase64` accepts only `A-Z a-z 0-9 - ~ =`;
   `libi2pd/Base.cpp::T64` maps slot 62 to `-` and slot 63 to `~`;
   padding character `P64 = '='`.
2. **Java I2P** (`i2p/i2p.i2p`) —
   `core/java/src/net/i2p/data/PrivateKeyFile.java` decodes
   `PrivateKeyFile` payloads with the I2P Base64 substitution table
   and `=` padding.
3. **i2plib** (`tomi/i2plib`, Python SAM client) —
   `i2plib/sam.py` builds the SAM alphabet with
   `I2P_B64_CHARS = "-~"` and validates through Python's standard
   `base64` decoder via `altchars=("-~")` with `validate=True`.

The frozen vectors in `crates/i2pr-api/src/sam/base64.rs::tests` lock
the alphabet, padding, and length behavior against any future
regression:

- `rfc4648_plus_slash_characters_are_rejected` — `+AAA` and `/AAA`
  surface as `InvalidCharacter`.
- `i2p_alphabet_characters_are_accepted` — `----`, `~~8=`, and
  `~~~8` round-trip to their expected byte payloads.
- `i2pd_corpus_round_trip` — short-vector cross-check against i2pd
  `Base.cpp` semantics for every tail length.
- `pub_priv_lengths_remain_unchanged` — `encode(391 bytes) ==
  524 chars`; `encode(455 bytes) == 608 chars`.

## Plan 143 / Plan 144 evidence lane (open)

A future closure attempt must use two clients that actually reach
the real listener and must prove cross-client binary STREAM
CONNECT/ACCEPT bytes. In particular, the current `i2pr-daemon`
session-created destination must be consumable by the client's I2P
Base64 representation, and `STREAM CONNECT` / `STREAM ACCEPT` must
hand the socket to a bounded raw bridge backed by the M6
destination / Streaming path.

Plan 144 closed the **in-process Plan 129 handshake** path. The new
test `crates/i2pr-daemon/tests/sam_stream_independent.rs` builds two
cooperating SAM destinations through the same `SamDestinations`
registry, drives a real `StreamingManager::connect` on bridge A,
drains the resulting SYN into a `TransportSendRequest`, and routes
the SYN through `bridge_to_peer` into bridge B. Bridge B accepts the
inbound SYN through the full destination stack and queues a SYN
response into its outbound queue. The SYN response is then drained
and routed back through `bridge_to_peer` into bridge A. The new
delivery path routes the recovered streaming packet onto bridge A's
**canonical** outbound `StreamingManager` (where the outbound
connection state lives) rather than the receiver-side mirror — this
fixes the Plan 143 routing asymmetry that prevented full
bidirectional handshake verification. Both sides reach
`ConnectionState::Established`. The Product evidence is
`crates/i2pr-daemon/tests/sam_stream_product.rs` (Plan 143); the
independent-bridge canonical-streaming-routing evidence is
`sam_stream_independent.rs` (Plan 144).

The **per-stream TCP<->Streaming raw byte bridge** that drives a
real `tokio::net::TcpStream` post-`STREAM STATUS RESULT=OK` through
the canonical streaming path remains deferred. The
`DispatchOutcome::StreamRawMode` arm in the daemon's
`RequireStreamConnect` dispatch replies OK and returns the
control-socket state to `UtilityReady`; the underlying TCP stream
is left at the command-mode egress. A follow-up plan (Plan 145
candidate) owns the per-destination driver task that owns the raw
TCP stream, feeds incoming bytes into
`StreamingConnection::send_data`, drains outbound queue through
`bridge_to_peer` (or the production Runtime equivalent), and pumps
delivered application bytes back into the TCP socket under bounded
resource ceilings.

Do not promote this lane to `passed` until the Plan 143 and
Plan 144 closure records are committed.