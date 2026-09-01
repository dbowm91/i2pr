# SAM 3.1 localhost reference / independent-client evidence lane

This directory is the lightweight external/reference evidence surface for the Plan 145 Milestone 7 corrective sequence.

Current authority:

- `plans/145-status.md` — active corrective roadmap;
- `plans/146-status.md` — closed as
  `passed-m7-sam31-private-destination-reference-requalification`;
- `plans/147-status.md` — closed as
  `passed-m7-sam31-dedicated-raw-stream-driver` (the canonical raw
  TCP↔Streaming product lane);
- `plans/148-status.md` — two-independent-client final closure, currently
  `blocked-external-client-build-failure` because the pinned i2plib and
  libsam3 sources and build artifacts are not present in this checkout
  and no build/install lane exists for them on this host.

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

### Private destination (Plan 146 closed)

`crates/i2pr-daemon/tests/sam_plan146_reference.rs` and the throwaway
Java helper `reference/Plan146ReferenceHelper.java` now satisfy the
bidirectional reference contract (see "Plan 146 evidence contract"
below). The pinned revisions are:

- Java I2P `2800040deee9bb376567b671ef2e9c34cf3e30b6` (release 2.12.0)
- i2pd `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` (release 2.60.0)

Both follow the same `target/interop/cache/current-cache.json` pins
recorded for the closed development interop lane; the SAM lane
reuses the same pinned artifacts and never commits raw `PRIV`
material.

Plan 146 also relaxed the reconstruction invariant — the new
`DestinationIdentity::from_imported` constructor preserves the
destination's embedded encryption public field verbatim and only
checks `signing_public == EdDSA(signing_seed)`. The standard Java
I2P `PrivateKeyFile` and i2pd `IdentityEx` layouts populate the
destination encryption public field with random bytes for
destinations; Plan 146 records that tolerance.

### Local product regressions

The current Rust tests retain useful lower-level evidence:

- `crates/i2pr-daemon/tests/sam_stream_product.rs` — Plan 143 local Plan-129 delivery seam;
- `crates/i2pr-daemon/tests/sam_stream_independent.rs` — Plan 144 in-process SYN/SYN-response handshake / canonical-streaming routing.

They do not move application bytes through independent SAM clients and do not replace the Plan 147 raw-socket lane.

## What remains unproven

### Raw STREAM product

The Plan 147 dedicated post-command `TcpStream` owner is implemented
and proven through `crates/i2pr-daemon/tests/sam_stream_raw_product.rs`
(closed under `passed-m7-sam31-dedicated-raw-stream-driver`):

- permanent detachment of line parsing after successful establishment;
- preservation of already-buffered post-command bytes through
  `LineReader::take_buffered()` into the raw driver;
- raw TCP fed into `StreamingManager::send_data()` under the negotiated
  Streaming payload bound;
- ordered delivered Streaming bytes written back to the TCP socket;
- delayed ACK/retransmit/timeouts supervised by the per-destination
  runtime driver.

The raw product lane is canonical Plan 147 local evidence. It does not
satisfy Plan 148's two-independent-client gate.

### Independent clients

No independent SAM implementation has yet moved application bytes
through the real i2pr listener. The pinned i2plib and libsam3
revisions are recorded in the **Selected independent clients** table
below, but the sources and build artifacts are not present in this
checkout and no build/install lane exists for them on this host. Plan
148 is `blocked-external-client-build-failure` per
`plans/148-status.md`.

Current count:

```text
sam_independent_clients = 0-passed
```

Plan 148 remains the two-independent-client final closure authority
once the prerequisite cache/build steps land.

## Selected independent clients

The preferred candidates are:

| Client | Revision/version | Language | License | Current local result |
| --- | --- | --- | --- | --- |
| `i2plib` | `6edf51cd5d21cc745aa7e23cb98c582144884fa8` (`v0.0.14`) | Python | MIT | pinned source is not present in the local cache; no build/install wrapper is checked in; selected as Client A, no application-byte pass yet |
| `libsam3` | `e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7` (`v1.0.0`) | C | mixed public-domain/MIT components | pinned source and build artifact are not present in the local cache; no build/install wrapper is checked in; selected as Client B, no application-byte pass yet |
| `txi2p` | `0611b9a86172cb70d2f5e415a88eee9f230590b3` | Python/Twisted | ISC | optional historical candidate; import blocked by legacy `ometa`; not a hard prerequisite |

If a selected client becomes unusable for reasons unrelated to i2pr, Plan 148 may replace it with another maintained SAM client after pinning provenance. Do not modify i2pr to emulate a demonstrable client bug.

## Plan 146 evidence contract

Plan 146 has produced real bidirectional PrivateKeyFile evidence without committing secret material.

Required directions (all proven by
`crates/i2pr-daemon/tests/sam_plan146_reference.rs`):

```text
reference-generated PRIV
  -> i2pr import / real SESSION CREATE
  -> exact public Destination equality

i2pr real DEST GENERATE SIGNATURE_TYPE=7
  -> reference parser/session consumer
  -> exact public Destination equality
```

The Plan 146 reference helper (`reference/Plan146ReferenceHelper.java`)
subcommands:

- `version` — emits the pinned reference/revision identifiers and
  the keygen parameters used to construct / consume the destination;
- `generate` — emits a fresh `PRIV` plus its derived lengths,
  certificate fields, and SHA-256 of the ephemeral bytes, then reads
  the bytes back through `PrivateKeyFile` to confirm the helper
  self-round-trips;
- `parse` — accepts an externally-supplied `PRIV` and re-emits the
  derived public-destination length, certificate type, and Base64
  so the test can byte-compare against the i2pr `PUB` reply.

Record (recorded by the bidirectional test):

- reference implementation/version/commit (Java I2P
  `2800040deee9bb376567b671ef2e9c34cf3e30b6`, i2pd
  `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`);
- command/API (`generate` / `parse`);
- signature/crypto types (`ED_DSA_SHA512_ED25519` / `ECIES_X25519`);
- binary/Base64 lengths (`PRIV 455/608`, `PUB 391/524`);
- private-key-field width (`private_key_field_is_256 = false`, the
  Plan 142 compact form is reference-compatible);
- public Destination hash (compared byte-equal between helper output
  and i2pr `DestinationId` after import);
- SHA-256 digest of ephemeral private bytes (recorded by the helper
  for record-keeping; never persisted);
- representation classification (`canonical_compact_455_byte`);
- pass/fail.

Never commit the raw `PRIV` value. The helper prints the Base64 PRIV
to its own stdout, captured only inside a single Rust test run;
ephemeral secret material is never written to a checked-in file.

## Plan 147 product contract (closed)

The canonical Rust localhost lane moves application bytes only through
SAM TCP:

```text
control A -> HELLO + SESSION CREATE
control B -> HELLO + SESSION CREATE
stream B  -> HELLO + STREAM ACCEPT
stream A  -> HELLO + STREAM CONNECT B.PUB
raw A <-> raw B
```

Every raw application byte passes through:

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

The Plan 129 authenticated-router-link-bypassed local seam is allowed
below the destination/tunnel stack. Direct application-byte transfer
between managers is not. The `sam_stream_raw_product` lane is the
canonical Plan 147 product test.

Plan 147's acceptance matrix covered:

- command->raw ownership transfer;
- same-read post-command bytes;
- binary payloads including NUL/non-UTF8/SAM-looking text;
- multi-packet logical transfers;
- simultaneous bidirectional traffic;
- production CSPRNG policy.

Slow-reader/slow-writer bounds, loss/duplicate/reorder/ACK drop,
sibling streams, and SILENT-byte-exact behavior at the SAM socket
boundary remain Plan 148 follow-up work (pending external clients).

## Plan 148 independent-client contract (blocked)

Plan 148 must run at least:

```text
i2plib CONNECT  -> libsam3 ACCEPT
libsam3 CONNECT -> i2plib ACCEPT
```

where APIs permit, plus the Plan 147 acceptance matrix and the
Plan 148 §12 resource/lifecycle/privacy/log evidence.

Both clients must be **independently implemented** (different language,
different codebase, different author). Two instances of the same Rust
test helper do not satisfy this gate.

This lane is currently `blocked-external-client-build-failure` per
`plans/148-status.md`: neither pinned source is in the local cache and
no build/install lane exists for them on this host. Plan 148 also
re-runs:

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
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product
cargo test --locked -p i2pr-daemon --test sam_forward_naming
```

Plan 147 should add a dedicated raw-socket product test and make it the canonical application-byte lane.

## Closure rule

Do not promote this lane to `passed` until:

1. Plan 146 reference private-destination evidence passes (closed);
2. Plan 147 real raw-socket product acceptance passes (closed);
3. two independent SAM implementations move exact application bytes
   through the real listener (Plan 148 — currently
   `blocked-external-client-build-failure`);
4. Plan 148 final evidence/status closes Milestone 7.
