# Plan 370 — managed-app v1 hello instance-id codec corrective

Status: **registered-managed-app-v1-hello-instance-id-codec-corrective**.

Classification: **corrective invariant + infrastructure**. This plan repairs one
undecodable message in the unreleased managed-app v1 contract. It adds no
capability, no process, no launcher, no router adapter, and no advertisement.

Corrects:

- managed-runtime Plan 345 (contract owner of the defective representation):
  - `plans/implementation/managed-native-app-runtime/345-native-app-runtime-foundation-and-capability-contract.md`
  - `plans/closure/managed-native-app-runtime/345-status.md`
- managed-runtime Plan 369 (the first runtime consumer that made the defect
  observable; WP4 is gated by this plan):
  - `plans/implementation/managed-native-app-runtime/369-trusted-application-runtime-manager-and-apphost-lifecycle-foundation.md` §10 and acceptance criterion 7
  - closure record not yet written (Plan 369 is active, WP1 landed)

Roadmap:

- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:

- Plan 345 is closed as `passed-managed-native-app-runtime-contract-foundation`.
- Plan 368 is closed as `passed-trusted-appmanager-bridge-and-manager-protocol-foundation`.

Interface dependencies:

- ADR 0035 (private manager protocol and inherited authority) — for the already
  ratified decimal-digit instance-id representation this plan propagates.
- ADR 0032 (managed native app runtime; qualified per the number-collision ledger,
  which is distinct from the Proposal 170 ADR 0032 ELS2 type-11 record).
- `specs/references/managed-native-app-runtime-v1.md` — the normative
  language-neutral wire reference this plan keeps truthful.

This plan does not require Proposal 170 completion, package trust, a sandbox
backend, or any router runtime change.

## Objective

Make `AppToHostMessage::Hello` decodable, without changing the wire form of any
other managed-app v1 message.

Today the message encodes successfully and **fails to decode for every possible
value**, including `instance_id = 1`:

```text
encode  {"type":"hello","request_id":1,"app_id":"a","instance_id":1,
         "protocol_major":1,"protocol_minor":0}      -> Ok
decode  same bytes                                      -> Err(InvalidControl)
```

## The defect

`AppInstanceId` is a `u128` that serialises through serde as a JSON **number**:

```rust
// crates/i2pr-app-proto/src/lib.rs:141
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u128", into = "u128")]
pub struct AppInstanceId(u128);
```

`AppToHostMessage` is an **internally tagged** enum (`#[serde(tag = "type")]`),
and `Hello` carries that id as a field:

```rust
// crates/i2pr-app-proto/src/lib.rs:310, :316
#[serde(tag = "type", deny_unknown_fields)]
pub enum AppToHostMessage {
    #[serde(rename = "hello")]
    Hello { …, instance_id: AppInstanceId, … },
```

Serde deserialises an internally tagged enum by **buffering the whole payload
into `serde::__private::de::Content`** and replaying it. That `Content`
deserializer has no `visit_u128`, so the `u128` field cannot be produced and the
message is rejected. The magnitude is irrelevant — the buffer is the problem:

```text
serde_json standalone u128        -> Ok(18446744073709551616)
externally tagged struct w/ u128 -> Ok
internally tagged enum, id:5      -> Err("u128 is not supported")
internally tagged enum, id:u128::MAX -> Err("u128 is not supported")
```

The runtime symptom is a typed `ContractError::InvalidControl`, which is
indistinguishable from malformed JSON. An application that sends a perfectly
well-formed hello is refused by the host for a reason that looks like corruption.

## Measured blast radius

Every variant of every internally tagged enum in both managed-app protocol
crates was decoded to establish the exact scope. The result:

| Message | Result |
|---|---|
| `AppToHostMessage::Hello` | **FAILS — `u128 is not supported`** |
| `AppToHostMessage::{Open, PermissionRequest, Close, Reset, UiMessage}` | OK |
| `HostToAppMessage::{Reply (both outcomes), PermissionReply, Capabilities, StreamClosed, StreamReset, Health}` | OK |
| `AdminToHostMessage::ReservedRequest` | OK |
| `AppPrincipal` (struct) | OK |
| `PrincipalOwnedResource` (struct, embeds `OwnedResourceId: u128`) | OK |

**`Hello` is the only undecodable message in managed-app v1.** Every other
internally tagged enum variant carries only `u32`, `u8`, `String`, or
serde-representable `u64` values. The two `u128` types that appear in *struct*
position (`AppPrincipal`, `PrincipalOwnedResource`) are unaffected, because
structs are not buffered.

## Why Plan 345's own verification missed it

Plan 345 closed a contract with **no runtime consumer**. Its unit tests never
round-tripped a `Hello` through `decode_app_to_host_control`; the encode/decode
round-trip test at `crates/i2pr-app-proto/src/lib.rs:1833` exercises
`AppToHostMessage::Close`. Nothing in the repository could observe the defect
until Plan 369 became the first consumer of the app v1 codec.

This is recorded as a verification-coverage finding, **not** as grounds to
rewrite the Plan 345 closure record. Plan 345's evidence is what it executed;
this plan corrects the defect forward, the same shape Plan 367 uses for
documentation that went stale behind earlier closures.

## Why Plan 368 already solved this class and did not reach app v1

Plan 368 hit the identical failure in the **new** private manager protocol:
`create_session` failed `InvalidControl` for every request. It was fixed by
introducing `ManagerInstanceId`, a **bounded canonical decimal-digit string**,
precisely because a `u128` cannot survive the internally tagged enum buffer
(`crates/i2pr-app-manager-proto/src/lib.rs:183`). That decision is recorded in
ADR 0035 and the Plan 368 closure as defect D1.

That fix was applied to the new crate only. `i2pr-app-proto` — the older,
unreleased contract — was left on the broken representation. **This plan does not
invent a new representation; it propagates the already-ratified Plan 368
decision to the contract it was never applied to.** That is why the manager and
app protocol will be consistent, and why the choice needs no new ADR.

## Options considered

**Selected — bounded canonical decimal digits.** Change `AppInstanceId`'s serde
representation to a canonical, bounded decimal-digit **string**, mirroring
`ManagerInstanceId`. Consistency with the ratified Plan 368 decision; works in
buffered and unbuffered position alike; language-neutral; keeps the full 128-bit
range.

Rejected alternatives:

1. **Re-tag the message enums** (externally or adjacently tagged instead of
   internally). It would fix the buffer, but it changes the wire form of *every*
   managed-app v1 message, contradicts the normative language-neutral reference
   `specs/references/managed-native-app-runtime-v1.md`, and breaks the shape a
   future SDK is written against — for no security gain whatsoever. Blast radius
   is far wider than the defect.
2. **Enable serde_json `arbitrary_precision`.** It does not work: the defect is
   in serde's `Content` buffering layer, not in serde_json's number parsing, so
   the feature cannot add the missing `visit_u128`. It would also be a
   workspace-wide feature unification changing number handling for every
   `serde_json` consumer, including Plan 368's `StrictKeyScanner`.
3. **Hand-roll a special-case `Hello` decoder.** It would create two divergent
   codec paths inside a contract crate whose entire value is that it has exactly
   one, and it would reintroduce the exact-consumption and duplicate-key
   discipline by hand.

## Invariants that must not regress

1. The v1 wire form of every message **other than** the `hello` instance-id
   field must not change.
2. `AppInstanceId` must remain a full 128-bit value with no truncation, and
   `0` must remain rejected.
3. There must remain **no decoder** from application-supplied bytes into
   effective authority. This plan changes representation only.
4. Canonical form is a **total function**: exactly one accepted encoding per
   id value. Two spellings of the same id is a defect, not a tolerance.
5. Managed-app v1 remains **unreleased and non-advertised**. No
   `specs/support.toml`, `specs/CONFORMANCE.md`, or RouterInfo change.

## Scope

### In scope

- `AppInstanceId`'s serde representation: bounded canonical decimal digits.
- Canonical-form validation: ASCII decimal digits only, no leading zero (except
  the literal `0`, which is then rejected as an invalid id), no sign, no
  whitespace, no exponent or fractional form, bounded length.
- The normative reference `specs/references/managed-native-app-runtime-v1.md`
  field spelling for `hello.instance_id`.
- Exhaustive executable regression coverage over every variant of every
  internally tagged enum in `i2pr-app-proto` and `i2pr-app-manager-proto`.

### Out of scope

- Any other protocol, message, or field change.
- The Plan 368 manager protocol, which is already correct.
- Any `i2pr-appd` / `i2pr-apphost` process, transport, supervisor, or fixture
  work — that remains Plan 369 WP2–WP6.
- `OwnedResourceId`'s representation. It is not broken; changing it would be an
  unforced wire change. Its safety is protected by WP2 below.
- Capability, grant, package, sandbox, broker, UI, or Proposal-170 work.
- Rewriting the Plan 345 or Plan 368 closure records.

## Required production changes

### A. Change `AppInstanceId` to a canonical decimal-digit representation

Replace `#[serde(try_from = "u128", into = "u128")]` with a
`String`-based conversion pair, bounded by a `MAX_DECIMAL_DIGITS` constant
(`u128::MAX` is 39 digits). `AppInstanceId` gains `TryFrom<&str>` /
`From<AppInstanceId> for String` (or the `String`-pair serde attribute) so
`AppPrincipal`, `PrincipalOwnedResource`, and `Hello` all present **one**
representation rather than two.

This is the same shape as `ManagerInstanceId`
(`crates/i2pr-app-manager-proto/src/lib.rs:183-235`). The existing
`From<AppInstanceId> for ManagerInstanceId` conversion continues to work and
gains a real, exercised path.

### B. Exhaustive recurrence-prevention coverage

An exhaustive `match` round-trip test covering **every variant** of
`AppToHostMessage`, `HostToAppMessage`, `AdminToHostMessage`, and
`HostToAdminMessage`, in both encode→decode and decode→encode directions.

The test is exhaustive by construction: adding a variant makes the match
non-exhaustive and **fails to compile** until the new variant is exercised. That
is what makes this a guard rather than a sample — a sampled round-trip test is
what let the defect through Plan 345.

### C. Why no `scripts/check-*.py` is added for this

A static checker that "no internally tagged enum in the contract crates holds a
128-bit field" would have to parse Rust type syntax to be correct, and a
substring or regex approximation of that rule is precisely the
*assertion-looks-like-enforcement-but-enforces-nothing* defect that Plans 360,
361, 362, 365, and 366 exist in this repository to close. The exhaustive test in
WP2 is stronger, simpler, and cannot be satisfied by grep. **Prefer the test.**
If a source-level guard is later wanted, it must be added with a `--self-test`
mutation harness, not on the strength of a pattern.

## Work packages

### WP1 — representation change and direct regressions

Change `AppInstanceId`'s serde representation; add the canonical-form
accept/reject table; verify `AppPrincipal` and `PrincipalOwnedResource` still
round-trip.

### WP2 — exhaustive message round-trip coverage

The compile-checked exhaustive match across all four message enums, plus
`hello` at `1`, `u64::MAX`, `u64::MAX + 1`, and `u128::MAX`, in both
directions.

### WP3 — reference and closure

Update the normative v1 reference so the documented field spelling matches the
implementation; write the closure record; run the unblock audit for Plan 369.

## Failure, cancellation, restart, and contention semantics

None. This plan has no concurrency, no process, no transport, and no state
machine. The only semantic change is that a previously-always-failing decode now
succeeds for a well-formed payload and continues to fail closed for every
malformed, non-canonical, or over-length payload.

## Compatibility and migration

Managed-app v1 is **unreleased**. Plan 369 is its first actual runtime consumer
and does not promote it to a stable external SDK guarantee. There is therefore
**no deployed peer to migrate**.

The `hello.instance_id` field spelling changes from a JSON number to a canonical
decimal-digit string. `AppPrincipal.instance_id` and
`PrincipalOwnedResource.{principal,resource_id}` change identically, because a
second representation for the same id would be worse than one.

There are **no existing router configuration changes** and **no support
advertisement changes**.

## Required tests

### Representation

- `AppInstanceId` round-trips at `1`, `u64::MAX`, `u64::MAX + 1`, `u128::MAX`.
- Canonical form is unique: exactly one accepted encoding per value.

### Canonical-form rejection (each must fail closed)

- empty string;
- `"0"` (already an invalid id);
- leading zeros (`"007"`);
- sign forms (`"+1"`, `"-1"`);
- surrounding or embedded whitespace;
- non-ASCII digits (including non-ASCII-Indic and full-width digits);
- fractional and exponent forms (`"1.0"`, `"1e3"`);
- over-length (> 39 digits);
- `null`, `true`, a JSON object, and a JSON array in place of a number or
  string;
- trailing data after the payload.

### Message-level

- Every variant of all four enums round-trips in both directions (exhaustive
  match; compile-enforced coverage).
- `hello` decodes at every boundary value in both directions.
- A `hello` with an unknown extra field is still rejected
  (`deny_unknown_fields` preserved).
- A `hello` with a duplicate JSON key is still rejected, if Plan 368's
  duplicate-key discipline is mirrored into the app v1 codec; if it is not
  mirrored, that is recorded as an explicit limitation rather than silently
  inherited.

### Non-regression

- `i2pr-app-proto` and `i2pr-app-manager-proto` unit and integration tests pass.
- `i2pr-daemon` gateway/manager-bridge suites still pass, proving no consumer
  depended on the old spelling.

## Exact verification commands

```text
cargo fmt --all --check
cargo check --locked -p i2pr-app-proto -p i2pr-app-manager-proto -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1
cargo test --locked -p i2pr-app-manager-proto --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon app_gateway -- --test-threads=1
cargo test --locked -p i2pr-daemon app_manager -- --test-threads=1
cargo clippy --locked -p i2pr-app-proto -p i2pr-app-manager-proto --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-proto -p i2pr-app-manager-proto --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-managed-app-manager-boundary.py
python3 scripts/check-managed-app-private-client-seams.py
python3 scripts/check-managed-app-gateway-boundary.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Then the complete current routine floor from `AGENTS.md`.

## Documentation updates

Required:

- `specs/references/managed-native-app-runtime-v1.md` — normative `hello`
  instance-id spelling, and a note that 128-bit identifiers are canonical
  decimal-digit strings wherever they appear on the v1 wire.
- `plans/subsystems/managed-native-app-runtime-roadmap.md` — §4 current state
  and the §7 milestone row for 370.
- `plans/registry.md` — 370 row, and the Plan 369 row's dependency note.
- `docs/architecture/i2pr-app-proto.md` — if present, the representation note.
- `plans/closure/managed-native-app-runtime/370-status.md` — closure record.

Do not claim a protocol capability, SDK stability, or support promotion.

## Acceptance criteria

Plan 370 passes only when:

1. `AppToHostMessage::Hello` decodes successfully for every valid
   `AppInstanceId`, including `1` and `u128::MAX`.
2. The pre-fix failing case is retained as an **executable regression** that
   fails against the old representation.
3. Every non-canonical or over-length encoding still fails closed with a typed
   error.
4. The wire form of every other managed-app v1 message is byte-identical to its
   pre-plan form, proven by golden-byte assertions.
5. `AppPrincipal` and `PrincipalOwnedResource` present exactly one
   representation for the same id.
6. The exhaustive round-trip test covers every variant of all four message enums
   and is compile-checked for exhaustiveness.
7. `AppInstanceId` still rejects `0` and still carries the full 128-bit range
   with no truncation.
8. No decoder from application-supplied bytes into effective authority was
   introduced.
9. The normative v1 reference matches the implementation.
10. No support, advertisement, conformance, or capability claim changed.
11. Focused verification commands and the full routine floor pass.

## Stop conditions

Stop and register a new plan if:

- the canonical decimal-digit representation cannot be made **unique** (that
  is, if two distinct encodings of one id value would be accepted);
- `AppInstanceId` cannot retain its full 128-bit range in the new form;
- fixing the buffer requires re-tagging the message enums (option (b) above),
  which is a v1 wire break and belongs in its own plan with a migration story;
- the exhaustive coverage cannot be made compile-checked, in which case a
  sampled test must not be presented as coverage;
- any consumer turns out to depend on the old numeric spelling, which would
  mean a deployed v1 peer and invalidate the "unreleased, no migration" premise.

## Closure evidence required

The closure must include:

- the pre-fix and post-fix decode result for the same payload, as executed test
  output, not as prose;
- the measured variant table from §Blast radius, re-run after the change;
- the exhaustive round-trip test with its variant count;
- the canonical-form accept/reject table with each rejection proven by a test;
- golden-byte evidence that non-`hello` messages are unchanged;
- the reason Plan 345's verification missed it, recorded as a coverage finding;
- docs updated and the routine-floor results.

The closure must additionally record the **unblock audit for Plan 369 WP4**.

## Handoff notes

This plan repairs a contract defect and nothing else. On its closure, Plan 369
resumes at WP2; WP1 is already landed and remains valid.

Recorded findings that are **not** this plan's scope and must not be lost:

- **`control_scoped` is not unrepresentable at the app-proto layer.**
  `Capability::ControlScoped` and `AppService::ControlScoped` both exist and
  `from_administrator_policy` will mint a grant for the former; the block is one
  layer up, in `app_gateway.rs`. Plan 369 WP6 owns extending the manager/
  bridge boundary checker to the new app-runtime crates.
- **`check-dependency-direction.sh` fails open for a new workspace member**
  (it iterates its own expected map, not the workspace) and **ignores
  `[dev-dependencies]`/`[build-dependencies]`**. A pre-existing live instance is
  `i2pr-addressbook` declaring `i2pr-i2pcontrol` as a dev-dependency while its
  allowlist is `{"i2pr-proto"}`, and the checker passes. Plan 369 WP6 owns the
  new-crate case; the dev-dependency case is an independent workspace-foundation
  gap and belongs to its own plan-of-record.
- **`scripts/check-managed-app-manager-boundary.py` has no `--self-test`**, so
  Plan 368's 12-injection negative evidence was executed manually and is not
  embodied in a runnable harness.
- **`AGENTS.md`'s "22 keys for 22 members" for the dependency map is stale**;
  the map holds 23 for 23 after Plan 368.
