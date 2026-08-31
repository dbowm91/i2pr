# Plan 136 status — SAM 3.1 protocol and private-destination foundation

Status: **`passed-m7-sam31-protocol-private-destination-foundation`**.

Registered: **2026-08-27**.

Source floor: Plan 134 local Milestone 6 closure; Plan 135
Milestone 7 SAM 3.1 planning authority; Plan 136 plan-of-record.

Plan of record:
[`plans/136-m7-sam31-protocol-private-destination-foundation.md`](136-m7-sam31-protocol-private-destination-foundation.md).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_135 = active-milestone7-sam31-planning-authority
plan_136 = passed-m7-sam31-protocol-private-destination-foundation

milestone6_local_product    = passed
milestone6_interoperable    = not-yet-claimed
external_acceptance_debt    = retained-separately

milestone7_sam_protocol     = passed
milestone7_loopback_server  = blocked-on-plan137
milestone7_stream_bridge    = blocked-on-plan138
milestone7_forward_naming   = blocked-on-plan139
milestone7_closure          = blocked-on-plan140

next_executable_plan        = 137
```

## Scope shipped

The `i2pr-api` workspace crate at `crates/i2pr-api/` now owns:

- **Bounded SAM 3.1 line/command/reply parser**
  (`crates/i2pr-api/src/sam/parser.rs`).
  Rejects oversized lines, NUL and control bytes, malformed
  quoting/escaping, duplicate critical options (`ID`, `DESTINATION`,
  `MIN`, `MAX`, `SIGNATURE_TYPE`, `STYLE`, `NAME`, `SILENT`), and
  unknown commands. Recognises the full Milestone 7 command
  vocabulary (`HELLO VERSION`, `DEST GENERATE`, `SESSION CREATE`,
  `STREAM CONNECT`, `STREAM ACCEPT`, `STREAM FORWARD`,
  `NAMING LOOKUP`, `PING`, `PONG`, `QUIT`/`STOP`/`EXIT`) and
  carries enough information for Plan 137 to distinguish supported,
  recognised-but-unsupported, and unknown commands.

- **Typed version model** (`crates/i2pr-api/src/sam/version.rs`).
  Server advertises exactly `[3.1, 3.1]`. `parse_version` rejects
  empty, control-byte, signed, overflow, extra-component, and
  whitespace-contaminated inputs. `negotiate` returns the
  intersection or a typed `NoOverlap`.

- **Strict SAM Base64 codec** (`crates/i2pr-api/src/sam/base64.rs`).
  Decoupled from the I2P Base64 variant used for router hashes;
  reuses none of its code because the alphabets differ. Rejects
  invalid characters, padding positions, lengths, and decoded-length
  ceilings. The codec originally emitted the RFC 4648 alphabet
  (`+/`); **Plan 142 corrected the alphabet to I2P Base64
  (`-`/`~`) with `=` padding** — the spelling every Java I2P /
  i2pd / independent Python client reference implementation emits.
  See [`plans/142-status.md`](142-status.md) for the closure
  record.

- **`SamPrivateDestination` wrapper**
  (`crates/i2pr-api/src/sam/private_destination.rs`).
  Standard Java `PrivateKeyFile` concatenation
  (`Destination || X25519_static_secret || Ed25519_signing_seed`,
  455 bytes / 608-character Base64 with `=` padding). Non-`Clone`,
  non-`Debug` for secrets, `Zeroize` on drop, public-only
  `PartialEq`. Narrow `signing_seed_bytes()` accessor added to
  `DestinationIdentity` (`crates/i2pr-client/src/identity.rs`) as
  the sole documented SAM-specific exception.

- **`DEST GENERATE` runtime-neutral core operation**
  (`crates/i2pr-api/src/sam/dest_generate.rs`). Takes a
  `TryCryptoRng`, produces a `SamPrivateDestination`, returns
  `PUB`/`PRIV` strings. Absent signature type returns
  `UnsupportedSignatureType` (legacy DSA is not implemented).

- **`SESSION CREATE` typed request parser**
  (`crates/i2pr-api/src/sam/session_create.rs`). Accepts
  `STYLE=STREAM` with either `DESTINATION=TRANSIENT` or a
  verified imported `PRIV`. Does **not** create a session or
  register the destination — Plan 137 owns the lifecycle.

- **Provenance document**
  ([`specs/references/sam31-private-destination.md`](../../specs/references/sam31-private-destination.md))
  recording the standard Java `PrivateKeyFile` concatenation,
  I2P Base64 (`-`/`~` alphabet, `=` padding), X25519/Ed25519
  key-type semantics, the offline-signature rejection, the
  round-trip invariant, and the test-fixture requirements. Plan
  142 corrected the alphabet from RFC 4648 to I2P Base64 and
  replaced the original circular-evidence model with three
  independent reference vectors (i2pd `libi2pd/Base.{h,cpp}`,
  Java I2P `PrivateKeyFile.java`, i2plib `I2P_B64_CHARS`).

## Files changed

```text
Cargo.toml                                                       # added i2pr-api to workspace members
crates/i2pr-api/Cargo.toml                                       # new crate manifest
crates/i2pr-api/src/lib.rs                                       # new facade
crates/i2pr-api/src/sam/mod.rs                                   # new module facade
crates/i2pr-api/src/sam/version.rs                              # new: SamVersion + negotiation
crates/i2pr-api/src/sam/base64.rs                               # new: SAM I2P Base64 codec (Plan 142 corrective: was RFC 4648)
crates/i2pr-api/src/sam/command.rs                              # new: typed command surface
crates/i2pr-api/src/sam/parser.rs                               # new: bounded line parser
crates/i2pr-api/src/sam/reply.rs                                # new: typed reply encoder
crates/i2pr-api/src/sam/private_destination.rs                  # new: SamPrivateDestination codec
crates/i2pr-api/src/sam/dest_generate.rs                        # new: DEST GENERATE core op
crates/i2pr-api/src/sam/session_create.rs                       # new: SESSION CREATE typed parser
crates/i2pr-client/src/identity.rs                              # added narrow `signing_seed_bytes()` accessor
scripts/check-dependency-direction.sh                           # allowlisted i2pr-api edges
specs/references/sam31-private-destination.md                   # new provenance document
specs/support.toml                                               # added plan_136 status, sam.v31-protocol-foundation surface
README.md                                                        # status + workspace layout
AGENTS.md                                                        # workspace layout, dependency direction, focused seam
docs/protocol-support.md                                         # SAM row replaced with Plan 136 evidence
docs/architecture/overview.md                                    # crate index entry, ASCII graph, Plan 136 mention
docs/architecture/i2pr-api.md                                    # new deep-dive
docs/architecture/dependency-graph.md                            # i2pr-api allowlist
.opencode/skills/i2pr-local-dev/SKILL.md                        # Plan 136 status, i2pr-api module layout
```

## Validation commands

Run from the repository root with the pinned Rust 1.95 toolchain.

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p i2pr-api --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## Evidence

| Group | Result |
| --- | --- |
| `cargo fmt --all --check` | green |
| `cargo check --locked --workspace --all-targets` | green |
| `cargo test --locked --workspace` | 1142 passed (55 suites) |
| `cargo test --locked -p i2pr-api --all-targets` | 51 passed (1 suite) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | green |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | green |
| `bash scripts/check-dependency-direction.sh` | green |
| `bash scripts/check-runtime-boundaries.sh` | green |

The Plan 136 acceptance criteria from
[`plans/136-m7-sam31-protocol-private-destination-foundation.md`](136-m7-sam31-protocol-private-destination-foundation.md)
§14:

1. `i2pr-api` is a workspace crate at the correct dependency layer —
   **satisfied**. Workspace member, depends on `i2pr-client`,
   `i2pr-crypto`, `i2pr-proto` only.
2. Dependency-direction checks prevent `i2pr-client → i2pr-api` and
   `i2pr-api → i2pr-daemon` inversion — **satisfied**. The script
   explicitly allowlists `i2pr-api → {i2pr-client, i2pr-crypto,
   i2pr-proto}` and forbids any other workspace edge.
3. Strict bounded parsing covers the full M7 command vocabulary —
   **satisfied**. `parse_line` recognises `HELLO VERSION`,
   `DEST GENERATE`, `SESSION CREATE`, `STREAM CONNECT`,
   `STREAM ACCEPT`, `STREAM FORWARD`, `NAMING LOOKUP`, `PING`,
   `PONG`, `QUIT`/`STOP`/`EXIT` and returns typed
   `Recognised`/`Unsupported`/`UnknownCommand`/`UnknownAction`/
   `Malformed` outcomes.
4. SAM 3.1 is the only advertised version — **satisfied**.
   `MIN_SUPPORTED_VERSION == MAX_SUPPORTED_VERSION ==
   SamVersion::const_new(3, 1)`; `is_advertised` returns `true` only
   for `3.1`.
5. Canonical typed reply encoding exists — **satisfied**.
   `HelloReply::encode`, `DestReply::encode`, `SessionStatus::encode`,
   `StreamStatus::encode`, `NamingReply::encode`, `PongReply::encode`
   produce the exact wire form with deterministic option ordering,
   single newline terminator, and proper MESSAGE quoting.
6. `SIGNATURE_TYPE=7` generation produces standard SAM-compatible
   `PUB` and `PRIV` representations — **satisfied**.
   `dest_generate` with `DestGenerateSignatureType::Ed25519` produces
   a 391-byte `PUB` (524-character Base64) and 455-byte `PRIV`
   (608-character Base64). Round-trip verified by
   `dest_generate_ed25519_produces_priv_round_trip`.
7. Generated `PRIV` re-import reconstructs exactly the same
   Destination/hash — **satisfied**. `priv_round_trip_through_base64`
   asserts `restored.id() == original_id` after encoding to Base64,
   decoding, and reconstructing.
8. At least one independently derived/frozen private-destination fixture
   validates provenance and format — **originally satisfied by the
   deterministic `from_identity` round-trip plus the
   `pub_and_priv_lengths_match_specification` test**. Plan 142
   strengthened this sub-claim: the round-trip evidence was
   circular (the i2pr codec was its own oracle), so Plan 142 added
   reference vectors derived from three independent sources (i2pd
   `libi2pd/Base.{h,cpp}`, Java I2P `PrivateKeyFile.java`, i2plib
   `I2P_B64_CHARS = "-~"`) and froze them in
   `crates/i2pr-api/tests/`. See [`plans/142-status.md`](142-status.md)
   for the closure record.
9. Malformed/truncated/unsupported private destinations fail closed
   with no panic — **satisfied**. `truncated_priv_is_rejected`,
   `mutated_private_key_is_rejected`, `wrong_length_priv_is_rejected`
   cover truncation, mutation, and length-mismatch failures. The
   codec never panics on malformed input.
10. Private key material is redacted and zeroized according to the
    ownership policy — **satisfied**. `SamPrivateDestination` has a
    manual `Debug` that emits `<redacted>`, is wrapped in a
    `Zeroizing<[u8; PRIV_LENGTH]>`, and has a `Drop` impl that
    zeroizes the buffer. `debug_redacts_secret_bytes` asserts no
    secret byte sequences appear in the `Debug` output.
11. No generic secret getters are added merely for SAM convenience —
    **satisfied**. The only new accessor on `DestinationIdentity` is
    `signing_seed_bytes()`, documented as the narrow SAM-only path.
12. No session/socket/network behavior is introduced — **satisfied**.
    `i2pr-api` has no Tokio dependencies, no socket code, no
    `unbounded_channel`, no `std::net`/`std::fs` use.
13. All workspace gates pass — **satisfied** (table above).
14. This status file is committed at `plans/136-status.md` —
    **satisfied** (this file).

## Handoff checklist (Plan 136 → Plan 137)

```text
[x] i2pr-api exists and builds independently
[x] SAM 3.1 parser/reply model is deterministic and bounded
[x] exact private-destination format has independent provenance
[x] DEST GENERATE type 7 round-trips
[x] TRANSIENT/private import core operations are runtime-neutral
[x] no SAM socket tasks exist yet
[x] no M6 protocol behavior was loosened
[x] Plan 136 status file is committed
```

## Progression policy

Plan 137 (loopback server and session lifecycle) is the next
executable plan. It must wire `i2pr-api` into the supervised loopback
listener through `i2pr-daemon` without bypassing the existing
`StreamingDestinationAdapter` / destination routing. The Plan 136
parser's `CommandOutcome::Unsupported` path is the canonical way to
reject unsupported styles, options, and protocol versions; Plan 137
must not silently accept semantics the implementation does not
support.