# SAM 3.1 localhost reference / independent-client evidence lane

This directory is the lightweight external/reference evidence surface for Milestone 7.

Current authority:

- `plans/145-status.md` — Milestone 7 corrective umbrella;
- `plans/146-status.md` — closed private-destination reference compatibility;
- `plans/147-status.md` — retained raw-socket owner/byte-pump implementation evidence;
- `plans/148-status.md` — blocked audit, superseded for execution;
- `plans/149-status.md` — **closed**, self-composing local SAM product corrective;
- `plans/150-status.md` — **passed**, external-client/final SAM closure;
- `plans/150-m7-sam31-external-client-reproducible-final-closure.md` — closed final independent-client closure.

This lane is localhost-only. It must not require root, namespaces, Docker, a VM, systemd, public I2P participation, or live NTCP2/SSU2.

## What is already proven

### SAM Base64

Plan 142's Base64 correction is retained:

```text
alphabet = A-Z a-z 0-9 - ~
padding  = =
```

The i2pr codec rejects RFC 4648 `+` / `/` as SAM input and emits the I2P spelling.

### Private destination — Plan 146 closed

`crates/i2pr-daemon/tests/sam_plan146_reference.rs` plus `reference/Plan146ReferenceHelper.java` provide bidirectional reference evidence.

Pinned references:

- Java I2P `2800040deee9bb376567b671ef2e9c34cf3e30b6` (2.12.0);
- i2pd `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` (2.60.0).

The reference lane proves the compact Ed25519/ECIES-X25519 representation used by i2pr:

```text
PRIV binary = 455 bytes
PRIV Base64 = 608 chars
PUB binary  = 391 bytes
PUB Base64  = 524 chars
```

The import path preserves the embedded destination encryption public field and validates the signing seed/public relationship. Do not reopen this sub-claim without a concrete new reference mismatch.

### Raw socket implementation — Plan 147 retained

Plan 147 landed:

- permanent line-parser -> raw `TcpStream` ownership transfer;
- preservation of bytes already buffered after the STREAM command newline;
- actual Streaming `Established` wait;
- production OS CSPRNG in CONNECT/delivery;
- TCP -> `StreamingManager::send_data()`;
- `drain_delivered()` -> TCP;
- supervised ACK/retransmit polling;
- a localhost binary byte-pump test when bridge/routing/tunnel prerequisites are installed.

The canonical Plan 147 test is useful lower-level implementation evidence, but it is **not** final black-box product-composition evidence.

## Closed product-composition result — Plan 149

The Plan 147 canonical test manually performs private setup after SAM `SESSION CREATE`:

```text
build/install SamDestinationBridge
install peer LeaseSet2 routing
install deterministic inbound-tunnel factory
spawn per-destination runtime driver
```

A real external SAM client cannot do those things.

Plan 149 makes `execute_session_create()` install the destination runtime,
local-product bridge, validated LeaseSet2, local-delivery provider,
Streaming ownership, SAM/stream registries, and one supervised destination
driver before returning `SESSION STATUS RESULT=OK`.

The resulting black-box product test starts the listener and then drives
**only SAM TCP commands/raw bytes**. After listener startup it may not call
private bridge, LeaseSet2, tunnel-factory, driver, delivery, or byte-moving
APIs.

The local Plan 149 evidence covers:

- byte-exact `SILENT=true/false`;
- authenticated non-silent ACCEPT peer Destination metadata;
- multi-megabyte bounded transfer;
- bounded backpressure and multi-megabyte transfer;
- terminal CLOSE/RESET cleanup and post-shutdown resource baselines.

Plan 150's external-client and broader SAM evidence is recorded in
[`evidence.md`](evidence.md); it must not be inferred as router-to-router
interoperability.

## External clients — Plan 150 guidance

Plan 148's old libsam3 pin was invalid for the official repository and must not be used.

### Pinned external provenance

```text
repository = https://github.com/i2p/libsam3
preferred exact snapshot = 7d6e658798baec31394c5685f9583343cc00900b
language = C
```

Known official release reference:

```text
v0.31.2 = ea52a3251d60906d67f9a1031a6ed7642753f94f
```

The preferred current snapshot includes a post-release destination-key newline validation fix. Pin exactly; do not silently advance.

### Counted client — i2psam

```text
repository = https://github.com/i2p/i2psam
exact snapshot = b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
language = C++
SAM requirement = v3.1
```

The repository documents the normal `make` library build and `eepget` example.

### Qualified substitute — i2plib

```text
repository = https://github.com/l-n-s/i2plib
exact final commit = 6edf51cd5d21cc745aa7e23cb98c582144884fa8
version = 0.0.14
language = Python
```

Its low-level `i2plib.sam` encoding/parsing surface is used by the Plan 150
thin socket harness. Its 2019 high-level async API passes the removed `loop=`
argument to `asyncio.open_connection()`, so the harness deliberately uses the
unmodified message/Base64 surface rather than patching the external client.
It is the qualified Plan 150 substitute for libsam3 because the pinned
libsam3 public API requires an 884-character private key while i2pr's
canonical Ed25519 SAM `PRIV` is 608 characters.

## Reproducible acquisition policy

Plan 150 supports two equivalent lanes:

1. preferred manual GitHub-hosted Ubuntu workflow (`workflow_dispatch`) that checks out exact external revisions into ephemeral workspace paths, builds them, runs localhost SAM, and uploads sanitized evidence;
2. local/pre-cached execution using explicit source paths whose Git revisions are verified before build.

Do not vendor third-party source into i2pr. Do not require privileged runners.

## Plan 150 independent-client contract and result

After Plan 149 closed, the following passed:

```text
 i2plib substitute CONNECT -> i2psam ACCEPT
 i2psam CONNECT             -> i2plib substitute ACCEPT
```

where their public APIs permit. The exact i2psam and i2plib revisions are
independently implemented and neither is patched for i2pr. The i2psam
snapshot's second-seeded session-ID generator is handled by one-second launch
slotting in `run-independent.sh`.

Required final evidence also includes:

- private destination compatibility through client APIs;
- exact bidirectional binary bytes;
- multi-packet and multi-megabyte traffic;
- SILENT compatibility;
- multiple streams/lifecycle;
- STREAM FORWARD to loopback target with real peer metadata;
- NAMING supported surface;
- negative version/style/option matrix;
- Plan 149 resource/fault/privacy regressions;
- Plan 127–134 M6 regressions.

Two instances of one in-repo helper do not count as independent clients. A
tiny transcript helper supplements SILENT, NAMING, and negative API coverage
but does not count toward `sam_independent_clients`. The complete successful
result is summarized in [`evidence.md`](evidence.md).

## Plan 146 evidence contract

The reference helper subcommands are:

- `version` — pinned reference/revision identifiers and keygen profile;
- `generate` — fresh ephemeral reference `PRIV` plus derived non-secret metadata;
- `parse` — parse externally supplied ephemeral `PRIV` and re-emit public Destination metadata for equality checks.

Never commit the raw `PRIV` value. Ephemeral secret material must remain process-local to the test run.

## Routine local commands

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product
cargo test --locked -p i2pr-daemon --test sam_forward_naming
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
```

The self-composed black-box raw product lane is the local Plan 149
acceptance authority; the retained Plan 147 test is a smaller raw-driver
regression.

## Closure rule

Do not promote Milestone 7 until:

1. Plan 146 remains green;
2. Plan 149 proves self-composing black-box SAM STREAM plus its documented
   local raw-path acceptance;
3. Plan 150 records at least two independent external SAM implementations moving application bytes through the real listener;
4. FORWARD/NAMING/resource/privacy/M6 final gates pass;
5. the newest status record explicitly closes Milestone 7.

Current handoff: **Plan 150 is closed; retain this as the reproducibility lane**.
