# Plan 164 — Milestone 9 I2CP protocol and wire foundation

Status: **next executable M9 pass**.

Depends on Plan 163 `registered-m9-i2cp-roadmap`.

Blocks Plans 165–170.

## 1. Goal

Establish one authoritative, runtime-neutral I2CP wire/profile foundation before adding sessions, destination ownership, or sockets.

This pass must answer precisely what i2pr implements and how every accepted frame is bounded. It must not create a listener or claim client interoperability.

Expected closure:

```text
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
next_executable_plan = 165
```

## 2. Source refresh first

Before code changes, verify and record immutable sources in:

```text
specs/SOURCES.md
specs/IMPLEMENTATIONS.md
specs/protocols/10-i2cp-service-tunnels.md
specs/support.toml
specs/CONFORMANCE.md
```

Use the official I2CP specification/overview as normative authority. Verify the Plan 163 starting snapshot (`i2p/i2p.website @ 26467e4b275e3a58280b9d4e6d4745d58bb8c499`) or replace it with a newer immutable official snapshot and record why.

Pin implementation references:

```text
Java I2P 2.13.0 = 9134f808337b401e8e53c73734c81fab04280c9d
go-i2cp          = b529ee1c10a6011558b4d69fc9436a4afc489eac
```

Do not copy implementation source.

The existing `10-i2cp-service-tunnels.md` dossier is retained in place. Add a clear M9 I2CP section and mark service-tunnel material M10; do not gratuitously renumber the specs directory.

## 3. Lock the M9 compatibility profile

Create an explicit table of I2CP features/messages with states:

```text
implemented-m9
planned-later
explicitly-unsupported
spec-defined-ignore
legacy-deprecated
```

Do not assert full API 0.9.67 compliance. Probe the exact Java and Go clients to determine the lowest honest behavior/profile supporting modern Ed25519 + X25519 + Standard LeaseSet2.

At minimum M9 targets codecs for:

- GetDate / SetDate;
- CreateSession / ReconfigureSession / DestroySession;
- SessionStatus;
- RequestVariableLeaseSet / CreateLeaseSet2;
- SendMessage / SendMessageExpires;
- MessagePayload / MessageStatus;
- GetBandwidthLimits / BandwidthLimits;
- DestLookup / DestReply;
- Disconnect.

Record HostLookup/HostReply disposition after client probing. Mark legacy CreateLeaseSet, ReceiveMessageBegin/End, ReportAbuse, encrypted/meta LeaseSets, PQ encryption types, and multi-session behavior explicitly rather than accidentally accepting them.

Proposal 171 outbound-tunnel switching remains draft/deferred. If its flag appears, follow current required ignore semantics; do not implement switching.

## 4. Runtime-neutral module

Add:

```text
crates/i2pr-api/src/i2cp/
```

Suggested files, adjusted only if local style makes another split clearer:

```text
mod.rs
frame.rs
message.rs
ids.rs
payload.rs
mapping.rs
error.rs
```

`i2pr-api` remains `#![forbid(unsafe_code)]`, with no Tokio, sockets, timers, or async runtime ownership.

## 5. Preamble and frame codec

Implement the I2CP TCP protocol byte as a typed value:

```text
0x2a
```

Implement the common frame:

```text
uint32 body_length, big-endian
uint8  message_type
body[body_length]
```

Requirements:

- named maximum body size derived from the official limit/current reference behavior; default to no more than 64 KiB unless the normative source requires a smaller ceiling;
- reject size before allocating body storage;
- incremental decoder supports partial header/body reads;
- zero-copy/slice parsing where practical but never borrow across mutable receive-buffer compaction unsafely;
- exact body consumption for known messages;
- unknown message type is typed, not a panic;
- deprecated/unsupported type is distinguishable from malformed known type;
- encoding rejects bodies above the same ceiling;
- integer overflow and length conversion are checked.

No unbounded `Vec` growth driven by the peer.

## 6. Primitive structures

Implement or reuse canonical bounded structures needed by later passes:

- `SessionId` and reserved/invalid values;
- `MessageId` / nonce fields with exact width;
- date/version string bounds;
- I2CP Mapping encoding with canonical sorted-key signing representation;
- Destination decoding through existing `i2pr-proto` types;
- payload wrapper with explicit maximum;
- LeaseSet2 envelope fields sufficient to defer semantic validation to Plan 166.

Do not duplicate existing common I2P codecs where they already exist in `i2pr-proto`.

## 7. Message codecs

Land structural decode/encode for the required M9 messages without implementing their router behavior.

Each codec must have:

- exact type ID;
- named minimum/maximum lengths/counts;
- strict trailing-byte policy;
- typed malformed errors;
- round-trip tests;
- at least one malformed/boundary test.

For messages containing opaque data that later layers validate (SessionConfig signature, LeaseSet2, payload), the structural codec must still apply absolute length ceilings.

## 8. Payload format contract

Record and test the I2CP payload/GZIP metadata format needed by Plan 168, including source/destination ports, I2P protocol number, xflags, and integrity framing according to the pinned specification.

This pass may implement structural parse/encode helpers, but must not connect them to destination routing.

Do not treat arbitrary gzip decompression as unbounded input. Any decompression helper must have an explicit maximum decompressed payload and reject expansion beyond it.

## 9. Fixtures and checker

Create a small committed corpus:

```text
tests/fixtures/i2cp/
```

At minimum include:

- protocol byte;
- GetDate / SetDate;
- representative SessionConfig-containing frame;
- SessionStatus;
- RequestVariableLeaseSet / CreateLeaseSet2 structural sample;
- SendMessage / SendMessageExpires;
- MessagePayload / MessageStatus;
- bandwidth and destination lookup replies;
- malformed/oversized examples or a manifest describing generated-negative cases.

Add:

```text
scripts/check-i2cp-vectors.sh
```

The checker verifies manifest hashes and invokes a narrow Rust vector test. Add it to routine Linux CI. Do not duplicate codec logic in shell/Python.

## 10. Documentation/architecture

Update:

- `docs/architecture/i2pr-api.md` with the `i2cp` module and runtime-neutral boundary;
- `docs/architecture/tooling.md` for fixtures/checker;
- `specs/support.toml` with Plan 163/164 authority and exact M9 profile status;
- generated protocol-support docs through the repository generator if one exists.

No protocol support row may become advertised/public merely because codecs exist.

## 11. Tests

Focused minimum:

```text
cargo test --locked -p i2pr-api --all-targets
bash scripts/check-i2cp-vectors.sh
```

Required negative matrix includes:

- wrong protocol byte;
- 0/1/4-byte partial headers;
- body length max and max+1;
- truncated body;
- unknown type;
- unsupported/deprecated type;
- known message with trailing garbage;
- oversized mapping key/value/count;
- duplicate critical mapping key where semantics require uniqueness;
- malformed Destination/LeaseSet structural payload;
- gzip expansion beyond ceiling if decompression is introduced.

Then run the Plan 163 workspace floor.

## 12. Acceptance criteria

Plan 164 closes only when:

1. official specification and both reference-client pins are recorded immutably;
2. the exact M9 feature/API profile is explicit and does not claim blanket 0.9.67 support;
3. `i2pr-api::i2cp` exists and owns no runtime/socket behavior;
4. protocol byte and frame codec are strict and bounded before allocation;
5. required M9 message structures have typed codecs and exact type IDs;
6. unsupported/deprecated/unknown messages are separately classified;
7. canonical Mapping/signing representation needed by SessionConfig is defined and tested;
8. payload structural format and expansion bounds are recorded;
9. committed I2CP fixtures and manifest pass;
10. `scripts/check-i2cp-vectors.sh` is routine-Linux-CI enforced;
11. existing SAM API tests remain green;
12. dependency/runtime boundary scripts remain green;
13. no listener, destination activation, or external-client interoperability claim is introduced;
14. workspace floor and routine CI pass on the exact closing commit;
15. `plans/164-status.md` records exact commands, closing SHA, and CI run.

## 13. Stop conditions

Stop rather than broadening this pass if source comparison reveals a materially ambiguous wire rule, if selected clients require a feature outside the proposed profile, or if implementing a common structure would duplicate an existing canonical codec. Record a narrow decision/corrective before proceeding.

## 14. Handoff

After explicit Plan 164 closure, execute Plan **165**.