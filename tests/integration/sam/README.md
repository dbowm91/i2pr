# SAM 3.1 interoperability lane

This directory records the lightweight independent-client checks for Plan
140. It is localhost-only and does not download, start, or configure an I2P
router. The Rust loopback tests remain the CI-capable product lane:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback --test sam_stream --test sam_forward_naming
```

## External client provenance

The discovery source is the official I2P SAM API documentation:
<https://geti2p.net/en/docs/api/samv3>.

The two selected SAM 3.1 STREAM candidates were inspected from their pinned
upstream revisions, without copying source into this repository:

| Client | Revision/version | Language | License | Local result |
| --- | --- | --- | --- | --- |
| `i2plib` | `6edf51cd5d21cc745aa7e23cb98c582144884fa8` (`v0.0.14`) | Python | MIT | imports; imported-private probe is blocked by the server's RFC 4648-only spelling and no live STREAM path |
| `txi2p` | `0611b9a86172cb70d2f5e415a88eee9f230590b3` | Python/Twisted | ISC (`COPYING`) | import blocked locally by missing legacy `ometa` dependency |

The exact read-only inspection commands were:

```text
git clone https://github.com/l-n-s/i2plib /tmp/i2pr-sam-i2plib
git clone https://github.com/str4d/txi2p /tmp/i2pr-sam-txi2p
git -C /tmp/i2pr-sam-i2plib checkout 6edf51cd5d21cc745aa7e23cb98c582144884fa8
git -C /tmp/i2pr-sam-txi2p checkout 0611b9a86172cb70d2f5e415a88eee9f230590b3
PYTHONPATH=/tmp/i2pr-sam-i2plib python3 -c 'import i2plib'
PYTHONPATH=/tmp/i2pr-sam-txi2p python3 -c 'import txi2p'
```

These commands are discovery/provenance checks, not passing Plan 140
evidence. A future closure attempt must use two clients that actually reach
the real listener and must prove cross-client binary STREAM CONNECT/ACCEPT
bytes. In particular, the current `i2pr-daemon` session-created destination
must be consumable by the client's I2P Base64 representation, and
`STREAM CONNECT`/`STREAM ACCEPT` must hand the socket to a bounded raw bridge
backed by the M6 destination/Streaming path.

## Current disposition

Plan 140 is intentionally blocked. The in-repository Rust tests prove the
SAM protocol/session/forward/naming seams and resource cleanup, but do not
prove independent-client STREAM interoperability. Do not promote this lane
to `passed` until the blockers and the status record in
[`plans/140-status.md`](../../plans/140-status.md) are cleared.
