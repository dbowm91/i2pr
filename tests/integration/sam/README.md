# SAM 3.1 localhost reference / independent-client evidence lane

This directory is the lightweight external/reference evidence surface for the Plan 145 Milestone 7 corrective sequence.

Current authority:

- `plans/145-status.md` — active corrective roadmap;
- Plan 146 — private-destination reference requalification (**next executable**);
- Plan 147 — dedicated raw TCP↔Streaming product driver;
- Plan 148 — two-independent-client final closure.

This lane is localhost-only. It must not require root, namespaces, Docker, a VM, systemd, public I2P participation, or live NTCP2/SSU2.

## What is already proven

### SAM Base64

Plan 142's Base64 correction is retained:

```text
alphabet = A-Z a-z 0-9 - ~
padding  = =
```

The i2pr codec rejects RFC 4648 `+` / `/` as SAM input and emits the I2P spelling.

Reference source inspection currently recorded:

- i2pd `libi2pd/Base.{h,cpp}` — `-` / `~`, `=` padding;
- Java I2P `PrivateKeyFile` / Base64 implementation;
- i2plib `I2P_B64_CHARS = "-~"`.

These references are sufficient to retain the Base64 fix. They are **not** sufficient to close the private-destination binary representation.

### Local product regressions

The current Rust tests retain useful lower-level evidence:

- `crates/i2pr-daemon/tests/sam_stream_product.rs` — Plan 143 local Plan-129 delivery seam;
- `crates/i2pr-daemon/tests/sam_stream_independent.rs` — Plan 144 in-process SYN/SYN-response handshake / canonical-streaming routing.

They do not move application bytes through independent SAM clients and do not replace the Plan 147 raw-socket lane.

## What remains unproven

### Private destination

The current i2pr SAM private-destination implementation has used a compact 455-byte / 608-character representation for its declared type-4/type-7 profile.

Current official SAM documentation describes `PRIV` as Destination + Private Key + Signing Private Key and documents 663+ binary / 884+ Base64 with a 256-byte unused encryption-private-key field. Current common-structures documentation also permits type-specific private-key sizes when context supplies the key type.

Plan 146 must resolve the actual interoperable SAM/PrivateKeyFile form with executable reference evidence in both directions.

Do not call the compact form reference-compatible until Plan 146 proves it.

### Raw STREAM product

The daemon does not yet have the dedicated post-command `TcpStream` owner that:

- permanently detaches line parsing;
- preserves already-buffered post-command bytes;
- feeds raw TCP into `StreamingManager::send_data()`;
- writes ordered delivered Streaming bytes back to TCP;
- drives delayed ACK/retransmit/timeouts under bounded supervision.

Plan 147 owns that work and the real-socket binary/backpressure/fault/SILENT/lifecycle acceptance matrix.

### Independent clients

No independent SAM implementation has yet moved application bytes through the real i2pr listener.

Current count:

```text
sam_independent_clients = 0-passed
```

Plan 148 owns the final external lane.

## Selected independent clients

The preferred candidates are:

| Client | Revision/version | Language | License | Current local result |
| --- | --- | --- | --- | --- |
| `i2plib` | `6edf51cd5d21cc745aa7e23cb98c582144884fa8` (`v0.0.14`) | Python | MIT | imports; SAM 3.1 helpers inspected; selected as Client A, no application-byte pass yet |
| `libsam3` | `e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7` (`v1.0.0`) | C | mixed public-domain/MIT components | builds via `make build`; STREAM example inspected; selected as Client B, no application-byte pass yet |
| `txi2p` | `0611b9a86172cb70d2f5e415a88eee9f230590b3` | Python/Twisted | ISC | optional historical candidate; import blocked by legacy `ometa`; not a hard prerequisite |

If a selected client becomes unusable for reasons unrelated to i2pr, Plan 148 may replace it with another maintained SAM client after pinning provenance. Do not modify i2pr to emulate a demonstrable client bug.

## Plan 146 evidence contract

Plan 146 must produce real bidirectional PrivateKeyFile evidence without committing secret material.

Required directions:

```text
reference-generated PRIV
  -> i2pr import / real SESSION CREATE
  -> exact public Destination equality

i2pr real DEST GENERATE SIGNATURE_TYPE=7
  -> reference parser/session consumer
  -> exact public Destination equality
```

Record:

- reference implementation/version/commit;
- command/API;
- signature/crypto types;
- binary/Base64 lengths;
- public Destination hash;
- SHA-256 digest of ephemeral private bytes;
- representation classification;
- pass/fail.

Never commit the raw `PRIV` value. Ephemeral secret-bearing run roots should live under `target/` or another temporary directory and be deleted after validation.

A source-code inspection or an i2pr self-round-trip is not Plan 146 closure evidence.

## Plan 147 product contract

The canonical Rust localhost lane must behave only through SAM TCP for application bytes:

```text
control A -> HELLO + SESSION CREATE
control B -> HELLO + SESSION CREATE
stream B  -> HELLO + STREAM ACCEPT
stream A  -> HELLO + STREAM CONNECT B.PUB
raw A <-> raw B
```

Every raw application byte must pass through:

```text
StreamingManager
 -> Streaming packet
 -> gzip ClientPayload
 -> I2NP Data
 -> ECIES Garlic
 -> destination tunnel product path
 -> inverse path
 -> peer StreamingManager
```

The Plan 129 authenticated-router-link-bypassed local seam is allowed below the destination/tunnel stack. Direct application-byte transfer between managers is not.

Plan 147 must test:

- command->raw ownership transfer;
- same-read post-command bytes;
- binary payloads including NUL/non-UTF8/SAM-looking text;
- multi-packet and multi-megabyte logical transfers;
- simultaneous bidirectional traffic;
- SILENT true/false;
- loss/duplicate/reorder/ACK drop;
- slow-reader/slow-writer bounds;
- close/reset/control-session cancellation;
- sibling streams;
- production CSPRNG policy.

## Plan 148 independent-client contract

After Plan 147 passes, run at least:

```text
i2plib CONNECT  -> libsam3 ACCEPT
libsam3 CONNECT -> i2plib ACCEPT
```

where APIs permit.

Verify exact bidirectional binary bytes through the real listener and Plan 147 product path.

Plan 148 also re-runs:

- STREAM FORWARD real-byte trajectory against a loopback target;
- NAMING supported surface;
- negative version/style/option matrix;
- resource and lifecycle ceilings;
- privacy/log capture;
- focused Plan 127–134 M6 regressions.

## Routine local commands

The external lane supplements, not replaces, the Rust product gates:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_stream
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_forward_naming
```

Plan 147 should add a dedicated raw-socket product test and make it the canonical application-byte lane.

## Closure rule

Do not promote this lane to `passed` until:

1. Plan 146 reference private-destination evidence passes;
2. Plan 147 real raw-socket product acceptance passes;
3. two independent clients move exact application bytes through the real listener;
4. Plan 148 final evidence/status closes Milestone 7.
