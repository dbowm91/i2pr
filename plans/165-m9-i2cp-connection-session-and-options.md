# Plan 165 — Milestone 9 I2CP connection, session, and option state machines

Status: **blocked until Plan 164 passes**.

Depends on Plan 164 protocol/wire foundation.

## 1. Goal

Implement runtime-neutral I2CP connection/session control semantics and a bounded, auditable projection from client SessionConfig options into existing `i2pr-client::DestinationConfig` policy.

No TCP listener and no destination private/decryption-key installation belong in this pass.

Expected closure:

```text
plan_165 = passed-m9-i2cp-connection-session-and-options
next_executable_plan = 166
```

## 2. Connection state machine

Add a consuming/typed connection state under `i2pr-api::i2cp` with explicit transitions equivalent to:

```text
AwaitProtocolByte
  -> AwaitGetDate
  -> ReadyForSession
  -> SessionPending
  -> Active
  -> Closing/Closed
```

Do not accept message families in states where the protocol does not permit them.

GetDate/SetDate behavior must:

- parse a bounded version string;
- return the exact declared i2pr profile/version behavior from Plan 164;
- handle unsupported authentication/options explicitly;
- reject malformed ordering rather than silently resynchronizing.

## 3. SessionConfig verification

Implement canonical SessionConfig validation around the existing Destination/signature primitives.

Required checks, before any destination resource reservation:

1. Destination decodes strictly and uses supported signing/encryption types.
2. options Mapping uses the canonical byte representation defined in Plan 164.
3. creation date is within the specification's allowed clock window (target ±30 seconds unless the refreshed normative spec changes it).
4. signature verifies against the Destination signing public key.
5. duplicate Destination/session ownership is rejected.
6. total SessionConfig size/options count/key/value sizes stay within named ceilings.

Expose only a verified typed value to later layers; do not let Plan 166 accept raw unverified SessionConfig.

Use an injected clock for runtime-neutral tests. No wall-clock sleeps.

## 4. Session identifiers and registry

Implement a bounded runtime-neutral registry for I2CP sessions/connections.

M9 default policy is one primary I2CP session per TCP connection unless the exact selected-client profile proves multi-session is necessary.

Requirements:

- monotonically/securely assigned session IDs according to spec/reference behavior;
- configured hard ceiling per connection and router;
- duplicate Destination prevention across active/reserved sessions;
- reserve -> commit -> rollback transaction shape;
- session ownership tied to the connection capability, not caller-supplied IDs alone;
- no stale SessionID reuse while an old session can still generate events;
- teardown releases every reservation.

If a second CreateSession on one connection is unsupported, return the exact typed/protocol status rather than partially sharing resources.

## 5. SessionStatus behavior

Implement exact result-code mapping for:

- created/accepted;
- invalid SessionConfig/signature/date;
- unsupported destination/key/profile;
- duplicate/conflict;
- invalid options;
- resource exhaustion;
- reconfiguration success/failure;
- destruction/closure where the selected profile defines a status.

Do not map internal failures to success. Avoid leaking sensitive/internal details in reply strings or logs.

## 6. Option disposition table

Create one authoritative option parser/projector, not ad-hoc string lookups in daemon code.

At minimum classify and test:

```text
inbound.length
outbound.length
inbound.quantity
outbound.quantity
inbound.backupQuantity
outbound.backupQuantity
inbound.lengthVariance
outbound.lengthVariance
inbound.allowZeroHop
outbound.allowZeroHop
i2cp.messageReliability
i2cp.fastReceive
i2cp.leaseSetType
i2cp.leaseSetEncType
unknown keys
```

Rules:

- numeric parsing rejects signed/whitespace/overflow forms unless normative syntax permits them;
- map supported tunnel length/quantity directly into `DestinationConfig` through checked constructors;
- router-wide maxima remain authoritative even if the client asks for more;
- zero-hop is not enabled merely because requested; require explicit existing router policy support or reject it;
- backup quantity/variance may not be silently treated as target quantity/length if the semantics differ;
- Standard LeaseSet2 and X25519/enc type 4 are the baseline Plan 166 profile;
- encrypted/meta LeaseSets and PQ enc types 5–7 are explicit unsupported M9 paths;
- `fastReceive` behavior must agree with Plan 168; if M9 supports only fast receive, requesting the incompatible mode gets an explicit disposition;
- reliability options map to actual MessageStatus semantics; do not accept a stronger reliability mode that cannot be delivered.

Publish the table in the I2CP protocol dossier/CONFORMANCE docs.

## 7. Reconfiguration model

This pass defines the runtime-neutral reconfiguration request and validates which fields are mutable. Actual tunnel/runtime replacement is Plan 169.

Classify each option as:

```text
mutable-with-rebuild
mutable-immediate
immutable-after-create
unsupported
```

A reconfigure request must be fully validated before emitting an action. Partial option application is prohibited.

## 8. Actions/events boundary

The state machine may emit narrow actions such as:

```text
ReserveClientDestination { verified_session, projected_config }
ReconfigureClientDestination { ... }
DestroyClientDestination { ... }
RequestBandwidthSnapshot
RequestDestinationLookup
```

Names may follow repository convention, but action payloads must contain verified/typed values, not raw client strings.

Do not call daemon/runtime APIs from `i2pr-api`.

## 9. Tests

Focused tests must cover:

- valid protocol/GetDate/session sequence;
- CreateSession before GetDate;
- duplicate GetDate/CreateSession;
- canonical signed SessionConfig pass;
- one-byte mutation in Destination/options/date/signature fails;
- date at boundaries and just outside;
- duplicate Destination across reservations;
- registry max and max+1;
- every supported option at min/max/max+1;
- unknown option disposition;
- unsupported LeaseSet/PQ types;
- invalid reliability/fastReceive values;
- reconfiguration all-or-nothing validation;
- destroy/connection-close rollback.

Use deterministic signing fixtures and injected clock.

Then run:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
bash scripts/check-i2cp-vectors.sh
```

plus the Plan 163 workspace floor.

## 10. Documentation

Update:

- I2CP dossier with exact state/option/status behavior;
- `docs/architecture/i2pr-api.md` with the session state machine;
- `specs/support.toml`/`CONFORMANCE.md` with Plan 165 status/profile;
- executor guidance where current plan authority is recorded.

No listener/support advertisement yet.

## 11. Acceptance criteria

Plan 165 closes only when:

1. connection ordering is represented by typed states and invalid-order messages fail explicitly;
2. SessionConfig canonical signature verification and date checks are executable;
3. only a verified SessionConfig can reach a destination-reservation action;
4. session registry reservation/commit/rollback is bounded and duplicate-safe;
5. exact SessionStatus mapping is tested for success and failure classes;
6. every targeted option has an explicit supported/rejected/ignored disposition;
7. `DestinationConfig` ceilings cannot be bypassed;
8. reconfiguration validation is transactional and classifies mutable/immutable fields;
9. no sockets/Tokio or destination decryption-key ownership is added;
10. Plan 164 vectors/checker remain green;
11. SAM/API and client regressions remain green;
12. workspace floor and exact-head routine CI pass;
13. `plans/165-status.md` records closure evidence.

## 12. Stop conditions

Stop if an exact external client requires multi-session semantics, legacy LeaseSet creation, or an option whose safe projection changes `i2pr-client` architecture. Write a narrow corrective/decision rather than smuggling that work into option parsing.

## 13. Handoff

After Plan 165 passes, execute Plan **166**.