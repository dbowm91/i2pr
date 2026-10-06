# Plan 370 closure — managed-app v1 hello instance-id codec corrective

Status: **passed-managed-app-v1-hello-instance-id-codec-corrective**.

Classification: **corrective invariant + infrastructure**. This closure records
the repair of one undecodable message in the unreleased managed-app v1 contract.
It ships **no** capability, no process, no launcher, no router adapter, no SDK,
and no advertisement. Nothing here may be read as managed-application support.

Plan: `plans/implementation/managed-native-app-runtime/370-managed-app-v1-hello-instance-id-codec-corrective.md`.

## Commits

Implementation and this closure record land together. See `git log --oneline`
on branch `plans/368-369-managed-app-runtime-foundation` for the implementation
SHA recorded at commit time.

## The defect, as executed evidence

Before this change, `AppToHostMessage::Hello` **encoded successfully and failed
to decode for every possible value**, including `instance_id = 1`. Executed in a
disposable `git worktree` at `14bad56f` (the pre-change tree), against
`i2pr-app-proto`:

```text
PREFIX_DECODE instance_id=1      -> Err(InvalidControl)
PREFIX_DECODE instance_id="1"    -> Err(InvalidControl)
```

After this change, the same payload decodes and round-trips:

```text
hello_decodes_for_every_valid_instance_id_including_the_pre_fix_failing_case ... ok
the_pre_fix_numeric_spelling_is_rejected_and_the_canonical_string_is_accepted ... ok
```

The symptom was a typed `InvalidControl`, indistinguishable from malformed JSON.
A perfectly well-formed application `hello` was refused for a reason that looked
like corruption.

### Root cause, recorded precisely

`AppInstanceId` was a `u128` serialized as a JSON **number**, and
`AppToHostMessage` is an **internally tagged** enum (`#[serde(tag = "type")]`).
Serde deserializes such an enum by buffering the whole payload into
`serde::__private::de::Content` and replaying it; that buffer has no
`visit_u128`. The magnitude is irrelevant — the buffer is the defect. `Hello` is
the only message affected, because the only other `u128` fields
(`AppPrincipal.instance_id`, `PrincipalOwnedResource`) sit in **struct**
position, which serde does not buffer.

## Why Plan 345's verification missed it

Plan 345 closed the contract with **no runtime consumer**. Its round-trip test
at `crates/i2pr-app-proto/src/lib.rs:1833` exercises `AppToHostMessage::Close`,
never `Hello`. Nothing in the repository could observe the defect until Plan 369
became the first consumer of the app v1 codec.

This is recorded as a **verification-coverage finding**. The Plan 345 closure
record is **not** rewritten: its evidence is what it executed. The defect is
corrected forward, the same shape Plan 367 uses for documentation that went stale
behind earlier closures.

## Why this is not a new design

Plan 368 hit the identical failure in the *new* manager protocol (`create_session`
failed `InvalidControl` for every request) and fixed it with `ManagerInstanceId`,
a bounded canonical decimal-digit string. That decision is recorded in ADR 0035
and the Plan 368 closure as defect D1.

It was applied to the new crate only. `i2pr-app-proto` — the older, unreleased
contract — was left on the broken representation, because the root cause was
recorded imprecisely (as "`serde_json` cannot deserialize `u128`", which is
false; it is serde's tagged-enum buffer that cannot). Plan 370 therefore
propagates an **already-ratified** decision rather than inventing one. No new
ADR is required, and the two protocols now agree.

## What shipped

- **`AppInstanceId` representation** — `crates/i2pr-app-proto/src/lib.rs`. Now a
  bounded canonical decimal-digit `String`, with `MAX_DECIMAL_DIGITS = 39` (the
  exact digit count of `u128::MAX`), `new`/`parse`/`as_str`/`get`, and
  `TryFrom<u128>` / `TryFrom<&str>` / `TryFrom<String>` / `From<…> for String` /
  `From<…> for u128` / `Display`.
- **One grammar, not two** — `ManagerInstanceId::to_app_instance_id` now
  delegates to `AppInstanceId::parse` instead of re-implementing the grammar.
  Plan 368's own non-canonical/oversize rejection test passes unchanged, which is
  the proof this is behavior-identical.
- **`From<&AppInstanceId> for ManagerInstanceId`** — required because
  `AppInstanceId` is no longer `Copy`. The `&AppPrincipal` conversion path
  needed it.
- **Compile-exhaustive round-trip coverage** —
  `crates/i2pr-app-proto/tests/message_round_trip_exhaustive.rs`, 13 tests.
- **Normative reference** — `specs/references/managed-native-app-runtime-v1.md`
  §1 and §3 now specify the decimal-digit form normatively;
  `specs/references/managed-app-manager-protocol-v1.md` gains a new §4.5 stating
  the same rule and bound, so the cross-reference resolves.
- **Architecture deep-dive** — `docs/architecture/i2pr-app-proto.md`.

## Requirement-to-evidence matrix

| # | Acceptance criterion | Evidence |
| --- | --- | --- |
| 1 | `Hello` decodes for every valid `AppInstanceId`, including `1` and `u128::MAX` | `hello_decodes_for_every_valid_instance_id_including_the_pre_fix_failing_case` — 5 boundary values, both directions |
| 2 | Pre-fix failing case retained as executable regression that fails against the old representation | `hello_decodes_for_every_valid_instance_id_including_the_pre_fix_failing_case` (fails on `14bad56f`: decode returns `InvalidControl`); `the_pre_fix_numeric_spelling_is_rejected_and_the_canonical_string_is_accepted` |
| 3 | Non-canonical/over-length encodings still fail closed with a typed error | `non_canonical_and_over_length_instance_ids_fail_closed` — 15 cases, each asserted to equal `Err(ContractError::InvalidControl)`; `non_scalar_and_trailing_payloads_fail_closed` |
| 4 | Wire form of every other message byte-identical to pre-plan form, proven by golden bytes | **Golden diff across 16 encoded messages, pre vs post**: exactly 2 lines differ, both the `instance_id` spelling. See below. |
| 5 | `AppPrincipal` and `PrincipalOwnedResource` present exactly one representation | `principal_and_owned_resource_present_exactly_one_representation`; the pre-Plan-370 numeric spelling is asserted to be *rejected* for `AppPrincipal` |
| 6 | Exhaustive round-trip test covers every variant of all four enums, compile-checked | `message_round_trip_exhaustive.rs` — 14 message forms; **negative-tested** below |
| 7 | `AppInstanceId` still rejects `0` and keeps the full 128-bit range | `instance_id_keeps_the_full_128_bit_range_and_rejects_zero` — `u128::MAX` round-trips, `0` rejected on both paths |
| 8 | No decoder from application-supplied bytes into effective authority | `an_application_permission_request_cannot_mint_a_grant`; see "Security review" below |
| 9 | Normative v1 reference matches the implementation | `specs/references/managed-native-app-runtime-v1.md` §1/§3; manager reference §4.5 |
| 10 | No support/advertisement/conformance/capability claim changed | No change to `specs/support.toml`, `specs/CONFORMANCE.md`, RouterInfo, or any advertisement. `git diff --stat` touches 5 files, none of them a support or conformance surface. |
| 11 | Focused verification commands and the full routine floor pass | See "Verification" |

### Criterion 4 evidence — the golden diff

The pre-change tree (`14bad56f`) and the post-change tree were each run with the
same probe encoding all 14 message forms plus `AppPrincipal`, and the two outputs
were diffed. **Exactly two lines differ:**

```diff
-APP hello  {"type":"hello","request_id":1,"app_id":"sample-app","instance_id":7,"protocol_major":1,"protocol_minor":0}
+APP hello  {"type":"hello","request_id":1,"app_id":"sample-app","instance_id":"7","protocol_major":1,"protocol_minor":0}
-PRINCIPAL  {"app_id":"sample-app","instance_id":7,"publisher_id":"sample-publisher"}
+PRINCIPAL  {"app_id":"sample-app","instance_id":"7","publisher_id":"sample-publisher"}
```

The other 14 encoded messages — `open`, `permission_request`, `close`, `reset`,
`ui_message`, both `reply` outcome forms, `permission_reply`, `stream_closed`,
`stream_reset`, `capabilities`, `health`, `reserved_request`, and the
host-to-admin `reply` — are byte-identical. The two changed lines are the two
positions where `AppInstanceId` appears, and the change is the documented field
spelling.

The verification worktree was removed after the diff; the probe test was a
temporary artifact and is **not** committed.

### Criterion 6 evidence — the guard is negative-tested

A `GuardProbeVariant` was temporarily added to `AppToHostMessage`, and the
crate's own internal validator arm was extended so the *library* compiled. The
guard then fired exactly as designed:

```text
error[E0004]: non-exhaustive patterns: `&AppToHostMessage::GuardProbeVariant { .. }` not covered
   --> crates/i2pr-app-proto/tests/message_round_trip_exhaustive.rs:82:15
```

This is the property Plan 345 lacked. A sampled round trip cannot fail to
compile when a variant is added, which is precisely why it missed the defect.
The injected variant was reverted; the tree compiles clean.

**No `scripts/check-*.py` was added for this.** A static checker for "no
internally tagged enum in the contract crates holds a 128-bit field" would have
to parse Rust type syntax to be correct, and a substring or regex approximation
of that rule is the *assertion-looks-like-enforcement-but-enforces-nothing*
defect that Plans 360, 361, 362, 365, and 366 exist to close. The compile-checked
`match` is stronger, simpler, and cannot be satisfied by grep.

## Verification

All commands run **locally** on Linux 6.8.0-142-generic x86_64, GNU bash 5.2.21,
rustc/cargo 1.95.0, with `--offline --locked`. **None of these were run on CI.**

Focused commands from the plan:

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo check --locked -p i2pr-app-proto -p i2pr-app-manager-proto -p i2pr-daemon --all-targets` | pass |
| `cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1` | **28 passed, 0 failed** (15 lib + 13 integration) |
| `cargo test --locked -p i2pr-app-manager-proto --all-targets -- --test-threads=1` | **28 passed, 0 failed** (10 apphost bootstrap + 18 manager contract; lib has 0) |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1 app_gateway` | **5 passed, 0 failed** |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1 app_manager` | **16 passed, 0 failed** |
| `cargo clippy --locked -p i2pr-app-proto -p i2pr-app-manager-proto --all-targets --all-features -- -D warnings` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-proto -p i2pr-app-manager-proto --no-deps` | pass |
| `bash scripts/check-dependency-direction.sh` | pass |
| `bash scripts/check-runtime-boundaries.sh` | pass |
| `python3 scripts/check-managed-app-manager-boundary.py` | pass |
| `python3 scripts/check-managed-app-private-client-seams.py` | pass |
| `python3 scripts/check-managed-app-gateway-boundary.py` | pass |
| `python3 scripts/check-global-plan-number-uniqueness.py` | pass |
| `python3 scripts/check-adr-number-uniqueness.py` | pass |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 30 tests, OK |

The 5 gateway and 16 manager-bridge daemon tests passing unchanged is the direct
evidence that **no consumer depended on the old numeric spelling**, which is also
the plan's stop condition for "a deployed v1 peer".

Routine floor: see "Routine floor" below.

## Migration and compatibility

Managed-app v1 is **unreleased**. Plan 369 is its first runtime consumer and does
not promote it to a stable external SDK guarantee. There is therefore **no
deployed peer to migrate** — and that premise is evidenced, not assumed, by the
unchanged daemon gateway and manager-bridge suites above.

The single wire change is the `instance_id` field spelling, from a JSON number
to a canonical decimal-digit string. It applies identically in `hello` and
`AppPrincipal` / `PrincipalOwnedResource`, because two spellings of one id would
be worse than one.

There are **no router configuration changes** and **no support advertisement
changes**.

## Security review

- **Canonical form is a total function.** Exactly one accepted encoding per id
  value. Sign, whitespace, leading zeros, exponent/fractional forms, non-ASCII
  digits (Arabic-Indic and full-width both tested), `null`, `true`, objects,
  arrays, `0`, and over-length input are all rejected with
  `Err(ContractError::InvalidControl)`. Two spellings would be a wire defect,
  not a tolerance; the reject table is the guard.
- **No new authority path.** `GrantedCapability` and `EffectiveCapabilities`
  deliberately derive no `Deserialize`, and `GrantedCapability` is constructible
  only through `from_administrator_policy(&AdministratorPrincipal, _)`. This
  change adds only `AppInstanceId` conversions and touches no grant,
  capability, policy, or administrator type. The
  `an_application_permission_request_cannot_mint_a_grant` test pins the
  adjacent fact that an application `permission_request` decodes to
  `RequestedCapability`, which has no conversion to a grant.
- **`get()` is total, not lossy.** It parses a string that both constructors
  already validated, and uses `expect` naming the invariant rather than a
  silent fallback value that could truncate an id. This is the one place where a
  wrong choice would have been a correctness/security bug.
- **`0` remains rejected** on both construction paths, so an id is never
  ambiguous with "absent".
- **Full 128-bit range retained.** `u128::MAX` round-trips exactly; no
  truncation.
- **Unknown and duplicate keys still rejected** on `hello`
  (`deny_unknown_fields` preserved) — confirmed by an executable test, not by
  reading the derive.

### Concurrency, cancellation, restart

None. This change has no concurrency, no process, no transport, and no state
machine. The only semantic change is that a previously-always-failing decode now
succeeds for a well-formed payload and continues to fail closed for every
malformed, non-canonical, or over-length payload.

## Documentation

- `specs/references/managed-native-app-runtime-v1.md` — §1 gains the normative
  decimal-digit rule, the 39-byte bound, the total-function statement, the full
  reject list, and the reason (the internally tagged buffer). §3 annotates
  `hello.instance_id`. Status header records the Plan 370 correction.
- `specs/references/managed-app-manager-protocol-v1.md` — new §4.5 states the
  same rule and bound and names `AppInstanceId` as the single definition. This
  was added because the app-v1 reference cross-references it, and the reference
  did not previously state the encoding; leaving it silent would have made the
  "cannot drift" claim unverifiable.
- `docs/architecture/i2pr-app-proto.md` — records the representation, why a JSON
  number could not work, that no other wire bytes changed, and that round-trip
  coverage is compile-exhaustive rather than sampled.

No plan state, status token, or ledger was copied outside `plans/`.

## Known limitations

1. **The exhaustive guard proves round-trip coverage, not field-type
   discipline.** It guarantees every *variant* is encoded and decoded. A future
   128-bit field added to a tagged enum would still break round-trip — which is
   exactly the failure mode it is designed to catch — so the guard is
   sufficient for this defect class, but it is not a general "no wide integers in
   tagged enums" checker. Recorded rather than papered over.
2. **`OwnedResourceId` still serializes as a JSON number.** Deliberately
   unchanged: it is not broken, it sits in struct position, and changing it
   would be an unforced wire change. Its safety is protected by the exhaustive
   test rather than by a representation change.
3. **`AppInstanceId` is no longer `Copy`.** A source-level consequence for
   downstream Rust consumers. The only in-repo consumer needing an adjustment
   was `From<&AppPrincipal> for ManagerPrincipal`, fixed here. No external
   consumer exists.
4. **Duplicate-key discipline was already present in the app v1 codec** (proven
   by the existing `control_golden_unknown_duplicate_and_max_plus_one` test and
   re-proven for `hello`), so the plan's conditional requirement needed no new
   implementation.

## Findings by severity

- **Critical:** none.
- **High:** none outstanding.
- **Medium:** none outstanding.
- **Low (recorded, not this plan's scope, carried forward from the Plan 370
  handoff notes so they are not lost):**
  - `scripts/check-dependency-direction.sh` fails open for a new workspace
    member (it iterates its own expected map, not the workspace) and ignores
    `[dev-dependencies]`/`[build-dependencies]`. A live pre-existing instance:
    `i2pr-addressbook` declares `i2pr-i2pcontrol` as a dev-dependency while its
    allowlist is `{"i2pr-proto"}`, and the checker passes. Needs its own
    workspace-foundation plan.
  - `scripts/check-managed-app-manager-boundary.py` has **no `--self-test`**, so
    Plan 368's 12-injection negative evidence was executed manually and is not
    embodied in a runnable harness. Plan 369 WP6 owns extending the checker.
  - `AGENTS.md`'s "22 keys for 22 members" for the dependency map is stale; the
    map holds 23 for 23 after Plan 368.
  - `control_scoped` is not unrepresentable at the app-proto layer
    (`Capability::ControlScoped` and `AppService::ControlScoped` both exist and
    `from_administrator_policy` will mint the former); the block is one layer up
    in `app_gateway.rs`. Plan 369 WP6 owns it.

None of these is a blocker for Plan 369.

## Routine floor

The **complete `AGENTS.md` routine floor was executed locally and every step
passed** — **50 of 50 steps, 0 failures**:

| Step group | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo check --locked --workspace --all-targets` | pass |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | pass — **4488 passed, 0 failed, 35 ignored** |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | pass |
| `cargo test --locked --workspace --doc` | pass |
| All 44 boundary/evidence/guard checkers | pass |
| `cargo deny check advisories bans sources` | pass — advisories ok, bans ok, sources ok |

Host: Linux 6.8.0-142-generic x86_64, GNU bash 5.2.21, rustc/cargo 1.95.0, run
with `--offline --locked`. **None of these steps were run on CI.** The seven
bash-4+ checkers that `AGENTS.md` records as unrunnable under macOS/bash 3.2 —
including `check-fixture-manifest.sh`, the three `check-*-vectors.sh`,
`check-streaming-tunnel-evidence.sh`, `check-java-source-lock-gating.sh`, and
`check-service-tunnel-acceptance-evidence.sh` — all executed and passed here,
because this host has bash 5.

The 35 ignored tests are the repository's pre-existing environment-gated external
interop lanes (i2pd / Java I2P / go-i2cp). They are gated by design and were not
enabled; this change does not touch any of them.

The floor was driven by a temporary reporting wrapper so that a per-step outcome
could be recorded rather than stopping at the first failure. The wrapper is a
verification artifact and is **not** committed.

## Unblock audit — Plan 369 WP4

Per `plans/README.md`, at every closure the registry's blocked work and the
affected roadmap dependency graphs are audited.

**Audited:** Plan 369 WP4 (`app v1` consumer: hello-first, exactly-once, exact
identity match) and its acceptance criterion 7.

- Hard dependency: Plan 345 — **closed** (`passed-managed-native-app-runtime-contract-foundation`).
- Hard dependency: Plan 368 — **closed** (`passed-trusted-appmanager-bridge-and-manager-protocol-foundation`).
- Interface dependency: ADR 0035 — stable, accepted.
- Interface dependency: `specs/references/managed-native-app-runtime-v1.md` —
  now describes the representation the implementation actually uses.
- Interface dependency: `specs/references/managed-app-manager-protocol-v1.md` —
  now describes the shared id grammar in §4.5.
- **The specific gate** — `AppToHostMessage::Hello` was undecodable, so
  hello-first and exact identity match could not be implemented or evidenced.
  It is now decodable at every valid id, with canonical-form rejection proven.

**Result: Plan 369 WP4 and acceptance criterion 7 are UNBLOCKED.** All other
hard dependencies of WP4 are closed and all interface dependencies now have
stable, truthful written contracts.

**Audited:** Plan 370 itself — closed by this record. No corrective pass is
required.

## Roadmap disposition

**Closed.** Corrective plan, all eleven acceptance criteria met with executed
evidence, the defect corrected forward, predecessor closure records left intact.

On its closure Plan 369 resumes: **WP1 (landed, remains valid), WP2, WP3, WP4
(now unblocked), WP5, WP6** are all actionable. Plan 369 as a whole stays
`in-progress` until its own closure record is written.