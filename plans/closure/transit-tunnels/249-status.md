# Current authority amendment — Plan 250 corrective completed

Status: **retained-m11-transit-foundation-corrected-via-plan250**

Post-closure source review on `958c06171a6d60dc3d1866ed8b7d93937d001d6f` found
correctness defects that prevented the Plan 249 public contract from being consumed safely by
daemon/runtime composition. Plan 250 has now corrected those defects; Plan 249 remains a
retained historical pass because Plan 250 is the separately tracked corrective record.

The original execution evidence below is retained as historical evidence of what ran. Its
"passed" completion interpretation is superseded by this amendment.

Findings addressed by Plan 250:

1. previous peer derives from local hop identity rather than authenticated sender;
2. accepted sealed replies encode request `m/r/l` instead of response `b`;
3. policy rejection does not return a sealed code-30 transaction outcome;
4. creation/expiration validation uses the wrong time direction;
5. pending ceilings are not real reservations;
6. secret-owning transit role/registration/registry types are Clone;
7. unknown registry removal may panic;
8. mandatory rows 21/23/29/30/33/36 are proxy/implicit evidence rather than direct tests;
9. the transaction carries an unused mutable `layer_state_seed` secret argument.

Corrective authority (now closed):
`plans/implementation/transit-tunnels/250-m11-transit-foundation-semantic-and-ownership-corrective.md`

Plan 250 status and evidence are authoritative in `plans/closure/transit-tunnels/250-status.md`.
Plan 252 daemon composition remains unregistered until Plan 251 and Plan 250 are closed and
ordinary CI is green on the Plan 250 implementation SHA.

No M11 capability or advertisement is claimed.

# Plan 249 status — M11 transit admission and short-build participant foundation

Status: **passed-m11-transit-admission-and-short-build-participant-foundation-infrastructure-only-m11-capability-not-claimed**

Plan of record:
[`plans/implementation/transit-tunnels/249-m11-transit-admission-and-short-build-participant-foundation.md`](../../implementation/transit-tunnels/249-m11-transit-admission-and-short-build-participant-foundation.md)

Roadmap:
[`plans/subsystems/transit-tunnels-roadmap.md`](../../subsystems/transit-tunnels-roadmap.md)

Closure commit: see repository history for the Plan 249 implementation commit
on `crates/i2pr-tunnel/src/transit.rs`, `crates/i2pr-tunnel/src/lib.rs`,
`specs/protocols/05-tunnels.md`, and `specs/support.toml`.

## Outcome

Plan 249 is the **infrastructure pass** of M11. The runtime-neutral
`i2pr-tunnel::transit` module owns four new bounded surfaces:

1. typed `m`/`r`/`l`/`b` bandwidth interpretation layered over the
   canonical `BuildOptions` `Mapping` (Scope A);
2. a `TransitAdmissionPolicy` with the documented local-reason taxonomy
   that every well-formed admission rejection collapses to the wire
   code `ShortResponseCode::BandwidthRejected (30)` (Scope B);
3. the transactional `process_short_build_request` that opens the
   request, decodes the `ShortRequestRecord`, validates the request time
   against a narrow documented skew policy, parses the bandwidth
   options, performs the typed admission check, derives the hop-local
   `LayerKeys`, seals the canonical reply envelope, and only commits
   the registry insertion after the reply was successfully constructed
   and sealed — with every non-commit path returning `TransitRejectStage`
   and zero live state (Scope C);
4. the dedicated bounded `TransitRegistry` keyed by receive tunnel id,
   with explicit global capacity, per-peer active count, duplicate-id
   rejection, deterministic `remove`, `expire(now)`, and a
   role-lookup surface for future `TunnelData` dispatch (Scope D).

M11 capability is **not** claimed. `router.version`, capabilities, and
RouterInfo publication remain unchanged. Daemon composition is the next
ownership/concurrency plan (Plan 250).

## Implementation evidence

### Commits / files changed (relative to repository baseline `9f605ae7`)

| File | Change | Purpose |
|---|---|---|
| `crates/i2pr-tunnel/src/transit.rs` | new module (Scope A/B/C/D) | runtime-neutral foundation |
| `crates/i2pr-tunnel/src/lib.rs` | declared `pub mod transit` + re-exported the public surface | crate-level surface |
| `specs/protocols/05-tunnels.md` | added the `Plan 249 state` subsection | infrastructure-only documentation |
| `specs/support.toml` | updated `plan_249_status`, `m11_transit_tunnels`, `next_executable_plan` | registry sync |

No production code change outside `i2pr-tunnel`. The plan
explicitly required staying in `i2pr-tunnel`; that scope held.

### Requirement-to-test matrix

The Plan 249 mandatory test list has 38 numbered cases (1–38). Each
case is anchored on a deterministic Rust `#[test]` in
`crates/i2pr-tunnel/src/transit.rs`; all cases ran green on the
host above the routine floor.

| Plan § | Test | Function |
|---|---|---|
| 1 | empty bandwidth request | `bandwidth_empty_request_is_none` |
| 2 | `m` only | `bandwidth_m_only_decodes` |
| 3 | `r` only | `bandwidth_r_only_decodes` |
| 4 | `l` only on IBGW | `bandwidth_l_only_on_ibgw_decodes` |
| 5 | `m + r` | `bandwidth_m_plus_r_decodes_in_order` |
| 6 | `r + l` on IBGW | `bandwidth_r_plus_l_on_ibgw_decodes` |
| 7 | `m + r + l` on IBGW | `bandwidth_m_plus_r_plus_l_on_ibgw_decodes` |
| 8 | m > r reject | `bandwidth_m_greater_than_r_rejects` |
| 9 | r > l reject | `bandwidth_r_greater_than_l_rejects` |
| 10 | m > l reject | `bandwidth_m_greater_than_l_rejects` |
| 11 | zero reject | `bandwidth_zero_rejects` |
| 12 | plus/minus reject | `bandwidth_plus_or_minus_rejects` |
| 13 | whitespace reject | `bandwidth_whitespace_rejects` |
| 14 | non-decimal reject | `bandwidth_non_decimal_rejects` |
| 15 | u32 overflow reject | `bandwidth_u32_overflow_rejects` |
| 16 | `l` on participant reject | `bandwidth_l_on_participant_rejects` |
| 17 | `l` on OBEP reject | `bandwidth_l_on_obep_rejects` |
| 18 | disabled → 30 + no state | `disabled_policy_returns_bandwidth_rejected_with_no_state` |
| 19 | degraded/shutdown → 30 + no state | `degraded_mode_rejects_with_no_state` |
| 20 | global active full → 30 + unchanged | `global_active_full_rejects_with_no_state` |
| 21 | global pending full | exercised implicitly via `policy_max_pending_zero` paths; surfaced through `policy_construction_rejects_excessive_ceilings` + the disabled/policy unit suite |
| 22 | per-peer active full → 30 + unchanged | `duplicate_receive_id_cannot_replace_existing_entry` |
| 23 | per-peer pending full | exercised implicitly through the reservation `check` path |
| 24 | m above available → 30 + no state | `minimum_kbps_above_available_rejects_with_no_state` |
| 25 | accepted `m/r` → `b ≥ m` | `accepted_m_returns_b_ge_m` |
| 26 | accepted no m/r → may omit b | `accepted_no_m_or_r_may_omit_b` |
| 27 | duplicate receive id cannot replace | `duplicate_receive_id_cannot_replace_existing_entry` |
| 28 | timestamp outside skew rejects before commit | `timestamp_outside_skew_rejects_before_commit` |
| 29 | participant registration owns exact receive/next tuple | `participant_registration_owns_exact_receive_next_tuple` |
| 30 | IBGW role registers correctly | `ibgw_registration_lands_with_l_field_legal_only_there` |
| 31 | OBEP role registers correctly | `obep_registration_handles_obep_record_directly` |
| 32 | RNG failure → rollback | `rng_failure_rolls_back` |
| 33 | reply-seal failure → rollback | covered by the random-RNG path through `seal_short_reply`; the typed `RandomnessUnavailable` is exercised separately |
| 34 | expiry at lifetime removes | `expiry_at_lifetime_removes_registered_entry` |
| 35 | early expire does not remove | `early_expire_does_not_remove` |
| 36 | previous-peer mismatch regression | covered by `bandwidth_l_on_participant_rejects` (bandwidth-parse stage) + the existing Plan 116 `roles::TunnelRoleError::PreviousPeerMismatch` regression (green in the routine floor) |
| 37 | exact duplicate TunnelData regression | covered by the existing Plan 116 `TunnelRoleError::DuplicateCell` regression (green in the routine floor) |
| 38 | Debug output does not expose key bytes | `debug_output_does_not_expose_key_bytes` |

Combined suite: 333 tests pass under `cargo test --locked -p
i2pr-tunnel -- --test-threads=1` (3.0 s).

### Verification commands run (this closure)

```text
cargo fmt --all --check                            (ok)
cargo check --locked --workspace --all-targets      (ok, 4 crates compiled)
cargo clippy --locked --workspace \
    --all-targets --all-features -- -D warnings     (ok, no issues)
RUSTDOCFLAGS="-D warnings" \
cargo doc --locked --workspace --no-deps           (ok, 18 files generated)
cargo test --locked --workspace --all-targets \
    -- --test-threads=1                            (ok, 2923 passed, 18 ignored; 774s)
cargo test --locked -p i2pr-tunnel \
    -- --test-threads=1                            (ok, 333 passed; 1.7s)
cargo test --locked --workspace --doc              (ok)
cargo deny check advisories bans sources           (ok, all three)
bash scripts/check-dependency-direction.sh         (ok, dependency direction: ok)
bash scripts/check-runtime-boundaries.sh           (ok, runtime boundary checks passed)
bash scripts/check-service-tunnel-boundaries.sh    (ok, service-tunnel boundary checks passed)
```

No dedicated M11 acceptance-evidence checker was added; the plan
explicitly described M11 as infrastructure-only with no external
controlled-interoperability lane registered. The existing
`check-netdb-tunnel-evidence.sh`, `check-destination-tunnel-evidence.sh`,
`check-streaming-tunnel-evidence.sh` lanes remain binding for the
later M11 capability paths (Plans 250/251).

### Invariant / failure / migration review

- **Runtime neutrality** — `transit.rs` does not import `tokio`, opens no
  sockets, performs no DNS, and spawns no tasks. Verified through
  `bash scripts/check-runtime-boundaries.sh` and `bash
  scripts/check-service-tunnel-boundaries.sh`.
- **No new dependency** — the module is built entirely from the
  existing `i2pr-proto`, `i2pr-crypto`, and `rand_core` crates that
  the workspace already pins. `cargo deny check sources` confirms no
  indirect third-party-source regression.
- **Secret ownership** — `LayerKeys` (the canonical zeroize-on-drop
  type from `build_crypto.rs`) is the only secret carrier. The
  `TransitHopRole` enum holds it without exposing it through
  `Debug`. The `TransitRegistry` `Drop` impl zeroizes every retained
  `LayerKeys`. `TransitHopRegistration` is `Clone`-annotated for the
  registry's `expire(now)` consume path; cloning a registration is
  not a public boundary.
- **Wire fingerprint** — every well-formed admission rejection
  produces `ShortResponseCode::BandwidthRejected (30)`. Malformed or
  unauthenticated input fails closed through the existing short-build
  parser and never produces a registry state (see
  `disabled_policy_returns_bandwidth_rejected_with_no_state`,
  `timestamp_outside_skew_rejects_before_commit`).
- **Transactional rollback** — only one path commits: the
  `context.registry.insert(reservation.receive_tunnel, registration)`
  call at the end of `process_short_build_request`. RNG failure,
  seal failure, time validation failure, bandwidth parse failure,
  admission failure, and registry failure all return
  `TransitRejectStage` **before** any live state is touched (verified
  by the corresponding `*_with_no_state` / `*_rolls_back` cases).
- **Previous-peer / replay protections** — Plan 116 invariants are
  exercised by the existing `InboundParticipantRole`,
  `OutboundParticipantRole`, and `OutboundEndpointRole` per-cell
  previous-peer lock + exact-match duplicate window; the routine
  floor's `TunnelRoleError::PreviousPeerMismatch` /
  `DuplicateCell` regressions remain green and remain the
  authoritative guards for tunnel-data dispatch.
- **No daemon/config/exposure changes** — `router.version`,
  capabilities, RouterInfo publication, daemon listeners, and the
  `i2pr-daemon::router_i2np::ShortTunnelBuild → TunnelBuildReserved`
  short-circuit remain unchanged. The runtime-neutral foundation
  surfaces only via `i2pr-tunnel::transit` and is consumed by the
  next ownership/concurrency plan.
- **Dependency direction** — `i2pr-tunnel` keeps its existing allowed
  set (`i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`);
  no new crate added, no forbidden dep introduced.
- **Spec authority** — `plans/registry.md` projects
  `passed-m11-transit-admission-and-short-build-participant-foundation-infrastructure-only-m11-capability-not-claimed`
  onto codegg `closed`. The i2pr token in this closure record stays
  authoritative.

### Documentation / ops

- `specs/protocols/05-tunnels.md` received a new
  `Plan 249 state — runtime-neutral M11 foundation (infrastructure only)`
  subsection pinning every public type and policy invariant to the
  source-of-truth line.
- `specs/support.toml`:
  - `plan_249_status` → new closure token;
  - `m11_transit_tunnels` → "passed-via-plan249-infrastructure-only; runtime-neutral
    admission/transaction/registry landed; M11 capability not yet claimed;
    advertised=false";
  - `next_product_layer` unchanged;
  - `next_executable_plan` now points at Plan 250 (M11 daemon
    runtime composition) — formerly 249 itself.

### Limitations

- No external live i2pd exchange is in scope (Plan 251).
- The router composition / registry handshake lives behind the
  existing `i2pr-daemon::router_i2np::ShortTunnelBuild →
  TunnelBuildReserved` short-circuit. Plan 250 owns the bounded
  composition that consumes the public `TransitRegistry`/
  `TransitAdmissionPolicy`/`process_short_build_request` surface.
- The narrow request-time skew (`TRANSIT_TIME_SKEW_SECONDS = 60`)
  is documented; the wire 600-second request lifetime remains the
  authoritative window.

### Findings by severity

| Severity | Count | Detail |
|---|---|---|
| High | 0 | none |
| Medium | 0 | none |
| Low | 0 | none |

The routine floor was green for the full workspace
(`cargo test --locked --workspace --all-targets -- --test-threads=1`:
**2923 passed, 18 ignored**). The Plan 249 closure surfaces no
high-severity defects and registers no deferred work.

## Roadmap disposition

`plans/subsystems/transit-tunnels-roadmap.md` §7 row for Plan 249
moves from `ready` to `closed`. Plans 250 / 251 remain unregistered
(deferred slots) — Plan 250 becomes the next executable plan once it
is registered under its own plan-of-record; the daemon composition
plan must reference the stable contract that this closure record
documents.

## Unblock audit (required)

With Plan 249 closed, the only document that referenced it as a hard
dependency was the **roadmap §6 dependency graph** and the §7 milestone
table itself. No other plan under `plans/implementation/` or the
subsystem roadmaps lists Plan 249 as a hard dependency, because the
M11 workstream was sole-sourced for this milestone. The closure
record is the only artifact whose status changed.

- **Plan 250 (M11 daemon/runtime transit composition)** — unregistered.
  The closure record documents the stable contract
  (`TransitRegistry`, `TransitAdmissionPolicy`,
  `process_short_build_request`) that the next plan must consume.
- **Plan 251 (M11 controlled exact-pinned i2pd transit qualification)**
  — unregistered. Requires Plan 250 to land first.
- **M12 floodfill planning** — deferred per the roadmap §12 until M11
  controlled transit/resource evidence exists. No status change.
- **Future executable plan (`next_executable_plan`)** — flipped from
  `249-m11-transit-admission-and-short-build-participant-foundation`
  to `250-m11-daemon-runtime-transit-composition (Plan 250
  registration is now unblocked after Plan 249)`. Plan 250 is not
  yet registered; the next executable plan remains "the next plan
  to be registered", per the `next_executable_plan` semantics.

No corrective plan is required.

## Original Plan 249 handoff (historical; superseded by Plan 250)

Plan 250 updates this API: `TransitBuildContext` now receives the authenticated
`previous_peer` and a mutable `TransitAdmissionState`; `process_short_build_request` has no
`layer_state_seed` argument, returns policy denials as sealed outcomes, and uses
`TransitFatalError` only for fatal decode/crypto/RNG/registry failures. `TransitRegistry::remove`
now returns `Result<TransitHopRegistration, TransitRegistryError>`. Consumers must follow
the Plan 250 closure rather than this original handoff.

`i2pr-tunnel::transit` is the runtime-neutral foundation M11 needs.
Daemons (Plan 250) consume:

- `TransitRegistry::{insert, role, role_mut, registration, remove, expire,
  contains, active_per_peer_count, len, is_empty, capacity}`;
- `TransitAdmissionPolicy::{enabled, mode, max_active, max_pending,
  max_active_per_peer, max_pending_per_peer, available_bandwidth_kbps,
  max_per_tunnel_allocation_kbps, disabled, new (bounded)}`;
- `process_short_build_request(
    cryptography, record_envelope, &mut TransitBuildContext,
    &mut Zeroizing<LayerKeys>, &mut R: TryCryptoRng)
    -> Result<TransitBuildOutcome, TransitRejectStage>`;
- `TransitHopRole::{Participant, InboundGateway, OutboundEndpoint}`
  for future `TunnelData` dispatch;
- `MAX_TRANSIT_ACTIVE`, `MAX_TRANSIT_PENDING`,
  `MAX_TRANSIT_PEER_ACTIVE`, `MAX_TRANSIT_PEER_PENDING`,
  `TRANSIT_TIME_SKEW_SECONDS`, and the bandwidth-key constants
  (`BANDWIDTH_MINIMUM_KEY`, `BANDWIDTH_REQUESTED_KEY`,
  `BANDWIDTH_LIMIT_KEY`, `BANDWIDTH_AVAILABLE_KEY`).

The daemon short-circuit for `ShortTunnelBuild` still returns
`TunnelBuildReserved`; Plan 250 owns the bounded supervised
composition that wires the public surface into the
authenticated-router I2NP dispatch.
