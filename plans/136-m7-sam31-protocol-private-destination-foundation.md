# Plan 136 — SAM 3.1 protocol and private-destination foundation

Status: **next executable implementation plan** under Plan 135.

Depends on: Plan 134 local Milestone 6 closure; Plan 135 roadmap.

## 1. Goal

Create the smallest trustworthy foundation on which the SAM server can be built:

1. add the `i2pr-api` crate at the intended application-adapter layer;
2. implement a strict bounded SAM v3.1 line/command/reply model;
3. implement exact version negotiation for the declared baseline;
4. reconcile and implement the SAM private-destination representation required by `DEST GENERATE` and `SESSION CREATE`;
5. expose a narrow destination construction/import/export seam without weakening Milestone 6 secret ownership.

This plan does **not** open a TCP listener, create long-lived SAM sessions, move stream bytes, implement naming/network lookup, or add any router transport behavior.

## 2. Mandatory pre-code protocol reconciliation

Before writing the private-destination codec, inspect the current official I2P SAM v3 documentation, common-structures key definitions, PrivateKeyFile documentation/reference implementation, and at least one deployed independent implementation (prefer Java I2P plus i2pd where practical).

Resolve and document the exact binary format for the i2pr destination profile currently used by `DestinationIdentity`:

```text
signing type     = 7 / Ed25519
crypto type      = 4 / X25519 in current i2pr Destination construction
identity padding = current key-certificate layout
```

Do **not** infer the SAM private-destination format from current Rust field sizes alone.

The SAM documentation describes `$privkey` as a Base64-encoded PrivateKeyFile-style concatenation of Destination + Private Key + Signing Private Key (plus optional offline section). It also documents legacy Destination behavior in which the encryption-private-key portion is unused and may be random/zero. The common-structures specification separately defines type-specific private-key lengths, including 32-byte X25519. These facts must be reconciled against the exact serialized Destination/key-certificate form used by i2pr before a codec is committed.

Required evidence file:

`specs/references/sam31-private-destination.md`

It must record:

- official URLs and retrieval date;
- exact field ordering;
- how field lengths are inferred;
- Base64 alphabet/padding behavior used by SAM;
- whether the current i2pr type-4 Destination requires preservation of an X25519 private key for exact identity reconstruction;
- how the PrivateKeyFile placeholder/legacy encryption-key field applies to this profile;
- Ed25519 signing-private-key representation and endianness;
- offline-signature section status (explicitly unsupported in M7 baseline unless already required by the resolved ordinary format);
- one independently generated/frozen byte fixture or transcript sufficient to detect accidental format drift.

If the current `DestinationIdentity` representation cannot round-trip through the standard SAM private-destination format without changing its identity model, stop this plan at that concrete finding and write one narrowly scoped corrective plan. Do not invent an i2pr-only `PRIV` format.

## 3. Crate and dependency layout

Add:

```text
crates/i2pr-api/
  Cargo.toml
  src/lib.rs
  src/sam/mod.rs
  src/sam/version.rs
  src/sam/command.rs
  src/sam/reply.rs
  src/sam/parser.rs
  src/sam/private_destination.rs
```

Names may be adjusted modestly if existing conventions demand it, but keep protocol parsing/formatting separate from future socket/session runtime code.

Update root `Cargo.toml` workspace membership and dependency-direction checks as required.

Preferred dependencies are existing workspace crates only. Do not add a parser-combinator framework, general HTTP/text protocol library, regex engine, or another Base64 package unless the repository truly lacks the required I2P/SAM Base64 behavior. Prefer small explicit parsing with hard limits.

`i2pr-api` may depend on `i2pr-client`, `i2pr-proto`, and narrowly on `i2pr-crypto` only where secret-owning conversion genuinely requires it. It must not depend on `i2pr-daemon`.

## 4. Version model

Define typed SAM versions, not free-form strings.

Required behavior:

- parse `major.minor` with bounded decimal components;
- reject malformed, signed, overflow, extra-component, empty, or whitespace-contaminated version values;
- support negotiation of the intersection between client `MIN`/`MAX` and server support;
- for this milestone, server advertised support is exactly 3.1 unless a later plan explicitly expands it;
- if no overlap exists, return the canonical SAM HELLO failure result rather than accepting the nearest version;
- do not claim 3.0 merely because some commands are compatible;
- do not claim 3.2/3.3.

Add table-driven tests around lower/upper boundary comparisons and malformed inputs.

## 5. Bounded SAM line parser

Implement parsing as a deterministic transformation from one complete line (without socket ownership) into a typed command.

Define conservative named hard ceilings. Exact numeric values should be selected and documented in code, with this shape:

```text
MAX_SAM_LINE_BYTES
MAX_SAM_TOKENS
MAX_SAM_OPTIONS
MAX_SAM_KEY_BYTES / MAX_SAM_KEY_TEXT
MAX_SAM_SESSION_ID_BYTES
MAX_SAM_NAME_BYTES
MAX_SAM_QUOTED_VALUE_BYTES
```

Requirements:

1. Reject an oversized line before building unbounded token/value vectors.
2. Reject NUL and control characters not explicitly permitted by the SAM grammar.
3. Correctly handle ordinary SAM 3.1 whitespace.
4. Support the SAM 3.1 quoted-value and backslash-escaped-quote rules required by current official documentation.
5. Normalize command/action keywords case-insensitively or to uppercase as the SAM guidance recommends, while preserving option values byte-for-byte where semantics require it.
6. Option keys should be normalized consistently.
7. Reject malformed quoting and trailing escape characters.
8. Critical duplicate options (`ID`, `DESTINATION`, `MIN`, `MAX`, `SIGNATURE_TYPE`, `STYLE`, `NAME`, `SILENT`, etc.) must be rejected, not silently resolved by last-write-wins.
9. Unknown options are retained only if a later state handler needs to emit an explicit unsupported-option result; never silently honor them.
10. Full input must be consumed. No hidden second command after a parseable prefix.

The parser must not allocate proportional to attacker-declared lengths; it only handles the already bounded line.

## 6. Typed command surface for Plan 136

The parser should recognize the Milestone 7 command vocabulary even when later plans own execution:

```text
HELLO VERSION
DEST GENERATE
SESSION CREATE
STREAM CONNECT
STREAM ACCEPT
STREAM FORWARD
NAMING LOOKUP
PING
PONG
QUIT / STOP / EXIT
```

Recognition does not imply feature support. The typed model must carry enough information for later handlers to distinguish:

- known/supported baseline command;
- known command with unsupported style/version/option;
- unknown command/action;
- malformed command.

For `SESSION CREATE`, parse at least `STYLE`, `ID`, `DESTINATION`, `SIGNATURE_TYPE`, and retained option pairs. M7 execution will permit only `STYLE=STREAM`.

For `STREAM CONNECT`, parse `ID`, `DESTINATION`, and `SILENT`; 3.2-only port parameters should be recognized only to reject them explicitly under a negotiated 3.1 session unless a later plan deliberately documents compatibility behavior.

For `STREAM ACCEPT`, parse `ID` and `SILENT` according to the current specification.

For `STREAM FORWARD`, parse the current ordinary forwarding fields needed by Plan 139 but leave semantic validation there.

For `NAMING LOOKUP`, parse `NAME`; recognize `OPTIONS=true` as outside the M7 3.1 baseline and retain enough information for explicit rejection/not-supported behavior.

## 7. Reply model

Do not hand-format arbitrary strings in socket tasks. Define typed reply/status structures with one canonical encoder.

At minimum model:

```text
HELLO REPLY
DEST REPLY
SESSION STATUS
STREAM STATUS
NAMING REPLY
PONG
```

Support the exact result vocabulary needed by the baseline, including protocol-defined failure categories such as version failure, duplicated ID/destination, invalid key, key not found, connection failure/refusal, invalid ID, timeout/cant-reach where the later adapter can distinguish them, and generic I2P error only when no more specific mapping exists.

Response encoding requirements:

- exactly one line termination in command/status responses;
- no secret-bearing values except where protocol requires `PRIV`/session destination output;
- correct quoting/escaping for MESSAGE values;
- deterministic option ordering for tests;
- no debug rendering of the private destination.

## 8. SAM Base64

SAM/I2P uses the I2P Base64 alphabet, not an arbitrary URL-safe or MIME assumption. Reuse an existing repository implementation if present. Otherwise add the smallest explicit adapter around the already-used Base64 primitive and freeze vectors for characters that differ from standard Base64.

Decoder requirements:

- strict accepted alphabet;
- deterministic padding policy matching current SAM implementations;
- bounded decoded size before allocation;
- reject invalid characters/trailing garbage;
- no whitespace folding unless the SAM spec explicitly permits it in this field.

## 9. Secret-owning private destination type

Introduce a type in the appropriate ownership layer with semantics equivalent to:

```rust
pub struct SamPrivateDestination { /* secret fields */ }
```

Mandatory properties:

- `#![forbid(unsafe_code)]` remains intact;
- no derived `Clone`;
- no secret-bearing `Debug`;
- zeroize secret storage on drop where possible using existing dependencies;
- public destination may be borrowed/copied separately;
- encode-to-SAM is an explicit operation;
- decode-from-SAM validates all structure and key types;
- reconstruction into `DestinationIdentity` is explicit and preferably consuming;
- no generic public methods expose raw signing/X25519 secrets to unrelated crates.

The existing `DestinationIdentity::from_private_bytes()` requires signing secret, X25519 static secret, and exact identity padding. If the resolved standard format carries enough information, add a narrowly scoped conversion seam. Prefer a method such as a consuming constructor/import type over public raw secret getters.

If a controlled export seam is unavoidable for `DEST GENERATE`, constrain it to an owned serialization operation rather than individual raw-key accessors.

## 10. `DEST GENERATE` core operation

Implement a runtime-neutral core operation callable by Plan 137:

Input:

- requested `SIGNATURE_TYPE`;
- injected CSPRNG.

M7 behavior:

- absent signature type: do **not** silently generate legacy DSA if i2pr does not support it; return a typed unsupported/default-policy result that Plan 137 maps protocol-correctly, or document an explicit server policy requiring type 7;
- `SIGNATURE_TYPE=7` or the accepted case-insensitive canonical name for type 7: generate one i2pr destination;
- unsupported type: typed rejection;
- randomness failure: typed failure.

Output must contain the SAM public destination and private destination representation, with exact identity equality proven by re-import.

No generated key may be inserted into the router destination registry in this plan. `DEST GENERATE` is a utility operation; session ownership begins only in Plan 137.

## 11. `SESSION CREATE` private destination import foundation

Implement only the pure conversion/validation layer:

```text
TRANSIENT + SIGNATURE_TYPE=7
    -> generate DestinationIdentity + private destination export metadata

serialized PRIV
    -> strict decode
    -> validate supported crypto/signature profile
    -> reconstruct exact DestinationIdentity
    -> recompute public Destination/hash
    -> assert equality with encoded public structure
```

Do not create a session or register the destination yet.

Reject:

- truncated key material;
- overlong material;
- mismatched certificate/key lengths;
- unsupported signature/crypto types;
- invalid I2P Base64;
- all malformed optional/offline sections;
- public/private mismatch;
- malformed Destination encoding;
- trailing bytes not accounted for by a supported format.

## 12. Tests

Required focused test groups:

### Parser/version

- canonical HELLO 3.1;
- keyword case normalization;
- split whitespace forms at the pure-line level;
- quoted MESSAGE/option values and escaped quote;
- missing action/required option;
- duplicate critical option;
- unknown command;
- line exactly at maximum and maximum + 1;
- excessive tokens/options;
- embedded NUL/control byte;
- malformed versions and no-overlap negotiation;
- 3.0/3.2/3.3 not advertised.

### Base64/private destination

- independently sourced/frozen `PUB`/`PRIV` fixture;
- generated type-7 destination round-trip;
- re-import yields exact same Destination bytes/hash;
- malformed alphabet;
- truncated each structural boundary;
- one-byte trailing garbage;
- unsupported key types;
- private/public mismatch or corrupted private key;
- `Debug` contains redaction and not known secret byte sequences;
- temporary decode buffers are held in zeroizing containers where practical.

### Reply encoding

Golden lines for success and representative error statuses. Test quote escaping and exactly one newline.

## 13. Expected files changed

Likely:

```text
Cargo.toml
Cargo.lock
crates/i2pr-api/Cargo.toml
crates/i2pr-api/src/lib.rs
crates/i2pr-api/src/sam/*.rs
crates/i2pr-client/src/identity.rs        # only if narrow import/export seam required
crates/i2pr-client/src/lib.rs             # exports only as needed
specs/references/sam31-private-destination.md
scripts/check-dependency-direction.sh     # only if new crate must be registered
```

Do not modify daemon socket behavior, NTCP2, tunnels, NetDB routing, ECIES session logic, or Streaming state machines in this plan.

## 14. Acceptance criteria

Plan 136 closes only if:

1. `i2pr-api` is a workspace crate at the correct dependency layer.
2. dependency-direction checks prevent `i2pr-client -> i2pr-api` and `i2pr-api -> i2pr-daemon` inversion.
3. strict bounded parsing covers the full M7 command vocabulary.
4. SAM 3.1 is the only advertised version.
5. canonical typed reply encoding exists.
6. `SIGNATURE_TYPE=7` generation produces standard SAM-compatible `PUB` and `PRIV` representations.
7. generated `PRIV` re-import reconstructs exactly the same Destination/hash.
8. at least one independently derived/frozen private-destination fixture validates provenance and format.
9. malformed/truncated/unsupported private destinations fail closed with no panic.
10. private key material is redacted and zeroized according to the ownership policy.
11. no generic secret getters are added merely for SAM convenience.
12. no session/socket/network behavior is introduced.
13. all workspace gates pass:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

14. `plans/136-status.md` records source commit, evidence fixtures, exact validation commands, and `next_executable_plan = 137`.

## 15. Handoff checklist

Before implementing Plan 137, verify:

```text
[ ] i2pr-api exists and builds independently
[ ] SAM 3.1 parser/reply model is deterministic and bounded
[ ] exact private-destination format has independent provenance
[ ] DEST GENERATE type 7 round-trips
[ ] TRANSIENT/private import core operations are runtime-neutral
[ ] no SAM socket tasks exist yet
[ ] no M6 protocol behavior was loosened
[ ] Plan 136 status file is committed
```

Do not proceed to Plan 137 with a custom/private `PRIV` encoding or unresolved key-length ambiguity.