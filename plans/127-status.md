# Plan 127 status — Milestone 6 destination-session routing final closure

Status: **passed-destination-session-routing-final-closure**
(local protocol conformance; no network-facing or external
interoperability claim).

Plan of record:
[`plans/127-m6-destination-session-routing-final-closure.md`](127-m6-destination-session-routing-final-closure.md).
Source floor: `8d5e7d2fd1493e77267c01e642561308fc639ef9` (Plan 126
closure commit).

## Handoff statuses

```text
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_124 = passed-corrected-destination-routing-local-closure
plan_127 = passed-destination-session-routing-final-closure
milestone6_local_product = not-closed
next = plans/128-m6-streaming-wire-protocol-corrective-closure.md
```

The word `local` is mandatory: mixed-router destination ECIES
interoperability remains separate evidence debt.

## What landed

- `crates/i2pr-client/src/session.rs` — unambiguous outbound form
  state machine (Plan 127 §5):
  - `planned_outbound_form()` reports which destination-scoped form
    the next send uses, with strict precedence: retained New Session
    Reply context → live paired Existing Session → fresh bound New
    Session.
  - `encrypt_to_remote()` seals exactly that precedence; the first
    reply to a bound NS rides the retained Plan 126 reply context and
    never degenerates into an unrelated fresh handshake.
  - `EciesOutboundMessage::NewSessionReply` variant plus a typed
    `form_name()` diagnostic derived from the variant (never from a
    magic first byte).
  - `drop_provisional_responder()` / `has_provisional_responder()`
    so a failed binding cannot leave a sealable reply context behind
    (Plan 127 §2: no NSR for an unbindable session).
- `crates/i2pr-client/src/routing.rs`:
  - `compose_outbound_delivery` queries the planned form before
    building payload blocks. A fresh bound New Session carries the
    DateTime block, the application Data clove, **and** a
    DatabaseStore clove with the local destination's current signed
    Standard LeaseSet2; NSR/ES traffic stays lean. A fresh bound NS
    without the bundled LS2 fails closed with the new typed
    `SendError::MissingBundledLeaseSet2`.
  - `EncryptedOutbound::NewSessionReply` variant with `form_name()`
    diagnostics so tests assert the exact emitted sequence
    (`new-session` → `new-session-reply` → `existing-session` ×4).
  - `install_remote_lease_set2()` — the Plan 127 §4 explicit typed
    handoff between the router-side LeaseSet2 store and the
    per-destination active-remote cache (single validation, no raw
    reparse).
  - Active-remote ceiling (Plan 127 §10): new
    `max_active_remotes` config field bounded by
    `MAX_ACTIVE_REMOTES = 256`, enforced in both registration paths,
    with `LookupIngestError::ActiveRemoteCapacity` and two new
    `DestinationRoutingError` variants.
- `crates/i2pr-client/src/dispatch.rs` — corrected inbound
  processing (Plan 127 §2/§3/§6/§8):
  - Classification remains structure/tag-driven through
    `EciesSessionManager::classify`; there is no legacy message-type
    byte anywhere on the path.
  - Bound New Session processing order is now exactly: authenticate/
    decrypt → obtain authenticated Alice static key → decode all
    payload blocks → require exactly one bundled DatabaseStore(Standard
    LeaseSet2) → validate it under **its own contained Destination
    hash** (`expected_key = None`) → verify its usable type-4 X25519
    key equals the authenticated static key → only then bind.
  - The remote identity derives exclusively from the validated LS2's
    contained Destination; it is never taken from NS static-key
    bytes, an NSR tag, or an ES tag. Any binding failure drops the
    retained reply context.
  - `record_accepted_lease_set2()` is real: validated sender records
    are stored under the derived remote DestinationHash and exposed
    through `accepted_lease_set2_for()`. The outcome carries
    `NewSessionProcessed { local_destination, remote_destination_hash,
    validated_remote_lease_set2, clove_count }` so callers install the
    record via the routing handoff without reparsing bytes.
  - Local target ownership (Plan 127 §6) resolves strictly through
    the delivery instruction against the tunnel-owned local
    destination (`Local` → the owner; `Destination(h)` → must equal
    the owner); sender identity never selects the local target and no
    trial decryption across destination keys occurs.
  - ES/NSR outcomes report the session's remote static key plus a
    best-effort `sender_destination` resolved only from previously
    validated knowledge (accepted records then the router-side
    store) — never by hashing key bytes.
  - New typed errors: `MissingSenderLeaseSet2`, `SenderKeyMismatch`.
- `crates/i2pr-client/src/streaming_adapter.rs` — `send()` threads
  the local destination's current signed LS2 into the request so the
  adapter satisfies the bundling contract.
- `crates/i2pr-client/tests/plan127_trajectory.rs` — 16 deterministic
  tests including the master trajectory
  `plan_127_master_trajectory_ns_nsr_es_bidirectional_exact_once`:
  two independent destinations each own identity, current signed
  Standard LeaseSet2, tunnel pools with one real established outbound
  and inbound tunnel, routing, session manager, and dispatcher. The
  trajectory proves §7.1/§7.2/§7.3 end to end: bound NS (bundled LS2,
  static key present only cryptographically) through the real
  participant→OBEP chain, the exact-byte
  `authenticated-router-link-bypassed-local-seam`, B's IBGW→
  participant→endpoint chain, LS2 validation/binding at B, production
  reverse routing (non-expired lease selection), retained-context NSR,
  pending-tag match at A, then four Existing Session messages (two per
  direction) with exact-once delivery and replay rejection.
  Negative controls cover every §9 case: valid-LS2/key-mismatch,
  invalid LS2 signature, missing bundled LS2 (dispatcher and composer
  sides), expired sender LS2 blocking reverse route and NSR, tampered
  NS, wrong-tag NSR, NSR replay, unknown/tampered/replayed ES,
  wrong-owner delivery without trial decryption, removed destination
  owner (`UnknownDestination`), full application queue
  (`QueueFull`), session expiry with deliberate re-establishment,
  active-remote ceiling enforcement, and malformed-input isolation.
- Existing fixtures updated to the corrected contract: Plan 124
  trajectories bundle the sender's own signed LS2 (strengthening the
  master trajectory — B now validates and binds A's LS2), and the
  Plan 122 composition fixture bundles the local rather than the
  remote record.

## Preserved invariants

- Plan 124 §1: `forward_cells()` receives the standard-encoded I2NP
  Garlic carrier, never the plaintext inner Data envelope;
  `plan_124_phase_a_b_compose_emits_garlic_through_obep` stays green.
- Plan 126: the normative ECIES-X25519-AEAD-Ratchet wire formats,
  frozen conformance vectors, and manager lifecycle are unchanged;
  Plan 127 composes them, it does not alter them.

## Out of scope (unchanged)

Streaming packet fixes (Plan 128), SAM, I2CP socket server,
HTTP/SOCKS, NTCP2/SSU2 activation, external routers, Python
harnesses, Docker/VM/namespaces. NTCP2 stays experimental and
non-advertised; `specs/support.toml` surface rows remain
experimental/non-advertised.

## Verification record

Commands (repo root, toolchain 1.95.0, locked):

```text
cargo +1.95.0 fmt --all --check                       # clean
cargo +1.95.0 check --locked --workspace --all-targets   # clean
cargo +1.95.0 test --locked --workspace               # all green
cargo +1.95.0 test --locked -p i2pr-client --all-targets
    # 60 lib + 16 plan120 + ... + 16 plan127 (all binaries green)
cargo +1.95.0 clippy --locked --workspace --all-targets \
    --all-features -- -D warnings                     # clean
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc \
    --locked --workspace --no-deps                    # clean
bash scripts/check-dependency-direction.sh            # ok
bash scripts/check-runtime-boundaries.sh              # passed
```

## Acceptance criteria

All thirteen boxes in the plan-of-record §13 are satisfied by the
tests listed above; the master trajectory alone covers the first
eleven. No external interoperability claim is made.
