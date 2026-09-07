# Plan 165 status — Milestone 9 I2CP connection, session, and option state machines

Status: **`passed-m9-i2cp-connection-session-and-options`**.

Registered: **2026-09-07**.

Plan of record:
[`plans/165-m9-i2cp-connection-session-and-options.md`](165-m9-i2cp-connection-session-and-options.md).

## Current authority

```text
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options

milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 166
next_product_layer = milestone9-i2cp
```

## What landed

`crates/i2pr-api/src/i2cp/` extended with five new runtime-neutral
modules, all `#![forbid(unsafe_code)]`, no Tokio, no sockets, no
async, no destination secret material:

- `connection.rs` — `ConnectionStateMachine` covering
  `AwaitProtocolByte → AwaitGetDate → ReadyForSession →
  SessionPending → Active → Closing → Closed`. The M9 profile
  advertises only I2CP API version `0.9.67`. `GetDate`
  authentication is rejected. Illegal message families are rejected
  with a typed `I2cpError::IllegalInState`; the state machine never
  silently resynchronizes. Duplicate `GetDate`, premature
  `CreateSession`, and multi-session creation are rejected.
- `verify.rs` — canonical `SessionConfig` verification. Consumes the
  parsed `SessionConfig` plus the raw body bytes and verifies (in
  order): canonical representation, supported signing (Ed25519) /
  encryption (X25519) types with matching `KeyCertificate`,
  SessionConfig size / option count / key & value byte ceilings,
  creation timestamp within ±30 seconds of an injected `Clock`
  (`FixedClock` for tests, `SystemClock` for production), and the
  Ed25519 signature over the exact received signed region retained
  by the structural decoder. Output is `VerifiedSessionConfig`; no
  raw unverified SessionConfig may reach a downstream
  destination-reservation action.
- `config.rs` — bounded option disposition table and projection
  into `i2pr-client::DestinationConfig`. Strict unsigned parsing
  (no signed, whitespace, or overflowing values) for every numeric
  option; explicit rejection of unknown signing / encryption
  types, zero-hop tunnels, guaranteed reliability, and
  non-fast-receive; Proposal 171 outbound-tunnel-switching flag
  recorded as ignored; unknown keys logged as informational notes
  without altering the projected policy. The router-wide ceilings
  (`MAX_DESTINATION_INBOUND`, `MAX_DESTINATION_OUTBOUND`,
  `MAX_DESTINATION_BUILD_CONCURRENCY`,
  `MAX_DESTINATION_FAILURE_THRESHOLD`,
  `MAX_PENDING_DESTINATION_MESSAGES`,
  `MAX_PENDING_DESTINATION_BYTES`,
  `MAX_LEASE_PUBLICATION_MARGIN_SECONDS`,
  `MAX_LEASE_ROTATION_MARGIN_SECONDS`) are authoritative and
  enforced through `DestinationConfig::try_new`.
- `session.rs` — bounded runtime-neutral session registry with
  `reserve → commit → rollback` transaction shape. Default M9
  limits: `max_per_connection = 1`, `max_per_router = 16`. Session
  IDs are assigned monotonically (skipping the reserved `0xffff`)
  and never reuse a stale ID while a reservation still exists.
  Duplicate Destination ownership across active / reserved sessions
  is rejected. The `ReconfigurationClass` taxonomy
  (`MutableWithRebuild`, `MutableImmediate`,
  `ImmutableAfterCreate`, `Unsupported`) classifies every known
  option; `validate_reconfigure_classifications` enforces
  all-or-nothing semantics for reconfigure requests.
- `actions.rs` — the typed `I2cpAction` vocabulary the Plan 167
  daemon projects into runtime state. Variants:
  `ReserveClientDestination { verified_session, projected_config,
  connection }`, `ReconfigureClientDestination { ... }`,
  `DestroyClientDestination { connection, session, destination_hash }`,
  `RequestBandwidthSnapshot { connection }`, and
  `RequestDestinationLookup { connection, key }`. Every payload
  carries verified typed values only.

New I2CP error variants:

- `IllegalInState { state, message_type }`
- `VersionNegotiation { reason }`
- `AuthNotSupported { context }`
- `HandshakeOrdering { context }`
- `SessionConfigLimit { context, actual, maximum }`
- `SignatureRejected`
- `CreationTimestampOutOfRange { creation_ms, earliest_ms, latest_ms }`
- `UnsupportedSigningType { signing_type }`
- `UnsupportedCryptoType { crypto_type }`
- `OptionRejected { key, reason }`
- `OptionParseFailed { key, reason }`
- `SessionRegistry { context }`
- `SessionIdExhausted`
- `ReconfigureRejected { context }`

The new modules depend only on `i2pr-proto`, `i2pr-crypto`,
`i2pr-client`, `i2pr-tunnel`, and the existing
`crates/i2pr-api/src/i2cp/` codecs (no new workspace dependencies
beyond the existing `i2pr-tunnel` lifetime constant; the
dependency-direction checker now allows `i2pr-api` →
`i2pr-tunnel`).

## Tests

Every Plan 165 §9 case is exercised by unit tests in
`crates/i2pr-api/src/i2cp/`:

| Group | Tests |
| --- | --- |
| Connection | valid GetDate sequence; duplicate GetDate; GetDate before protocol byte; unknown version; auth in GetDate; CreateSession before SetDate; multi-session rejection; SessionStatus in Ready state; Disconnect transitions; deactivate returns to Ready |
| SessionConfig | canonical signed passes; ±30 s window boundary; one-bit mutation in signature rejected; one-bit mutation in destination rejected; supported signing/encryption types pass; unsupported encryption type rejected; empty options pass; over-size total body rejected |
| Option projection | defaults remain; supported keys project; unknown keys recorded; zero quantity rejected; signed / whitespace / overflow rejected; unsupported LeaseSet/PQ enc types rejected; guaranteed reliability rejected; non-fast receive rejected; ceiling-exceeding quantity rejected; Proposal 171 ignored; mapping shape rejects too many entries / oversize key / oversize value |
| Reconfigure | classes match the spec; diff classifies added / changed / removed; all-or-nothing rejects unsupported and immutable |
| Session registry | reserve / commit / destroy round trip; duplicate destination rejected; duplicate reservation blocks second reserve; rollback releases; per-connection limit; per-router limit; destroying one session does not disturb the other; connection teardown releases every session; commit with unknown reservation fails; session IDs are monotonic and skip reserved |

The Plan 164 I2CP vector corpus (`tests/fixtures/i2cp/` and
`crates/i2pr-api/tests/i2cp_vectors.rs`) continues to pass; no
fixture was modified by Plan 165.

## Closure fields

Populated only from executed evidence:

```text
closing_sha = 5388cbf
routine_ci_run = 34156534970
routine_ci_ubuntu = passed
routine_ci_macos = passed
msrv = passed
dependency_policy = passed
```

Local floor on the closing tree (all executed 2026-09-07):

```text
cargo fmt --all --check = passed
cargo check --locked --workspace --all-targets = passed
cargo test --locked --workspace --all-targets -- --test-threads=1 = passed (1645 passed, 1 ignored: the Plan 162 external test)
cargo test --locked -p i2pr-api --all-targets = passed (244 passed)
bash scripts/check-dependency-direction.sh = passed
bash scripts/check-runtime-boundaries.sh = passed
bash scripts/check-fixture-manifest.sh = passed
bash scripts/check-i2cp-vectors.sh = passed (manifest complete, 15 vector tests passed)
bash scripts/check-ntcp2-vectors.sh = passed
bash scripts/check-ssu2-vectors.sh = passed
bash scripts/check-ntcp2-interoperability.sh = passed
bash scripts/check-constrained-host-lane-boundary.sh = passed
bash scripts/check-sam-acceptance-evidence.sh = passed (22 rows)
bash scripts/check-ssu2-acceptance-evidence.sh = passed (15 rows)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' = passed (153 tests)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings = passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps = passed
cargo test --locked --workspace --doc = passed (0 tests)
cargo deny check advisories bans sources = passed
```

All Plan 165 §11 acceptance criteria are satisfied. No socket,
Tokio task, timer, or destination private/decryption-key ownership
was added to `i2pr-api` (criterion 9); the typed `I2cpAction`
boundary ensures no raw client bytes reach any runtime caller
(criterion 3); router-wide ceilings cannot be bypassed (criterion
7); the reconfiguration pass is all-or-nothing (criterion 8);
Plan 164 vectors / checker remain green (criterion 10); SAM / API
and client regressions remain green (criterion 11); the workspace
floor and routine CI pass (criterion 12); this status file records
the executed closure evidence (criterion 13).

## Updates

- `crates/i2pr-api/Cargo.toml` — added the `i2pr-tunnel` workspace
  dependency.
- `scripts/check-dependency-direction.sh` — allows `i2pr-api` →
  `i2pr-tunnel` alongside the existing `i2pr-client`,
  `i2pr-crypto`, and `i2pr-proto` edges.
- `specs/protocols/10-i2cp-service-tunnels.md` — new
  "M9 I2CP connection/session/options (Plan 165)" section
  documenting the state machine, SessionConfig verification,
  option disposition table, session registry, reconfiguration
  taxonomy, typed actions, and the updated compatibility profile.
- `specs/support.toml` — added `i2cp.connection-session-options`
  surface row and `plan_165_*` authority entries; next executable
  plan advanced to `166`.
- `docs/architecture/i2pr-api.md` — new "Plan 165 I2CP connection /
  session / option surface" section; module layout extended with
  `connection.rs`, `verify.rs`, `config.rs`, `session.rs`, and
  `actions.rs`.
- `README.md`, `AGENTS.md`, `plans/README.md`,
  `.opencode/skills/i2pr-local-dev/SKILL.md`,
  `.opencode/skills/i2pr-architecture/SKILL.md` — current authority
  and handoff advance to Plan 165 / next Plan 166.

## Handoff

Plan 165 is closed. Execute Plan **166**
([`plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md`](166-m9-i2cp-client-owned-destination-and-leaseset2.md))
next, then Plans 167–170 in order:

```text
plan_165 = passed-m9-i2cp-connection-session-and-options
next_executable_plan = 166
next_product_layer = milestone9-i2cp
```

Do not extend Plan 164's structural codecs or Plan 165's state
machines into a listener, destination activation, or
interoperability claim; those belong to the later M9 passes.