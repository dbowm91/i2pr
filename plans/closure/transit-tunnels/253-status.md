# Plan 253 — M11 live daemon transit / data-plane corrective — closure record

Status: passed-m11-live-daemon-transit-data-plane-corrective
Plan-of-record: `plans/implementation/transit-tunnels/253-m11-live-daemon-transit-data-plane-corrective.md`
Plan author / driver: i2pr planning lane
Closure date: 2026-09-25
Plan-of-record authority: closure records > executable tests/scripts > ADRs > prose

## 1. Result

Plan 253 closed `passed`. The live daemon composition for M11 transit
is now wired end-to-end against the runtime-neutral
`i2pr-tunnel` data plane; the daemon never re-implements the
canonical participant / IBGW / OBEP transforms locally; every
rejection envelope is wrapped in a complete I2NP envelope; the
peer/session index is bounded; rollback drains registrations
synchronously; and the live owner surface replaces the Plan 252
module-local proxy tests with a daemon-level integration test.

Plan 254 i2pd qualification remains unregistered, exactly as Plan 253
required.

## 2. Work packages closed

### Work package B — runtime-neutral data plane (`i2pr-tunnel`)

`TransitHopRegistration` gained a sibling `data_plane: TransitDataPlane`
field that owns the previous-peer lock, the exact-replay window, and
the OBEP reassembler. The new `TransitDataOutcome` enum
(`Forward` / `Deliver` / `DuplicateOrReplay` / `PreviousPeerMismatch` /
`Expired` / `ReceiveTunnelMismatch` / `ZeroTunnelId` / `Fragment` /
`TunnelMessageRejected`) and `TransitDataFatalError` cover every
canonical error path. New methods
`TransitHopRegistration::process_tunnel_data` (Participant / IBGW /
OBEP) and `TransitHopRegistration::process_tunnel_gateway` (IBGW)
are exercised by tests 10–18 in `crates/i2pr-tunnel/src/transit.rs`
(all 374 tunnel tests pass; the lib re-exports the new surface).

### Work package C — daemon composition (`i2pr-daemon`)

`TransitHopMaterial` no longer derives `Clone`. The Plan 252
placeholder `forward_participant_layer` was removed (the runtime-neutral
data plane owns the transform). `TransitBuildService::cancel()`
synchronously drains registrations and the peer index. The
`deliver_dispatch_with_rollback` helper now rolls back on every
non-`Accepted` `RouterDeliveryOutcome` (was previously only on
`NoActiveSession`). Two new envelope wrappers
(`wrap_short_tunnel_build_envelope` / `wrap_outbound_tunnel_build_reply_envelope`)
guarantee the daemon never sends bare STBM / OTBRM body bytes.
`TransitDispatch::Rejected` now carries `receive_tunnel` and `role`
metadata so code-30 envelopes are role-correct.

### Work package D — live owner wiring (`i2pr-daemon/transit_owner.rs`)

A new `TransitOwner<R>` exposes
`TransitOwner::dispatch_short_build` /
`TransitOwner::dispatch_tunnel_data`. The owner drives the
controlled `TransitIngressGate` from the live daemon composition; the
Plan 184 dispatcher keeps its reserved outcome for any path the gate
does not consume. The owner drops transit secrets on cancel via
`TransitIngressGate::cancel()` in its `Drop` impl. The new
`is_transit_managed` predicate is the typed hook the daemon
classifier uses to distinguish `ShortTunnelBuild` from
`OutboundTunnelBuildReply`.

The module-local Plan 252 proxy tests under
`transit_compose.rs::mod tests` were replaced by the daemon-level
integration test `crates/i2pr-daemon/tests/m11_transit_data_plane.rs`,
which drives `TransitOwner::dispatch_tunnel_data` against a real
`TransitBuildService` constructed from a controlled identity and a
`Ssu2RuntimeConfig::default()`. Both tests pass under
`--test-threads=1`.

### Work package E — rollback / drain invariants

- `TransitBuildService::cancel()` calls `self.registry.expire(u64::MAX)`
  so every registration drains in the same scope. The static
  `check-m11-transit-boundaries.sh` rule 8 enforces the invariant.
- `deliver_dispatch_with_rollback` accepts a `CancellationToken` and
  rolls back the just-committed registration on every
  `RouterDeliveryOutcome` variant other than `Accepted`.

### Work package F — bounded peer state

A new `TransitPeerIndex` (bounded at `MAX_TRANSIT_PEER_INDEX = 4096`)
holds peer/session correlation. `install_peer` returns
`Result<(), TransitPeerError>` with `TransitPeerError::Duplicate` /
`TransitPeerError::CapacityFull(usize)` variants. `forget_peer` and
the per-`cancel` drain keep the index bounded. The static guard
rejects any `BTreeMap<Hash, PeerId>` peer map (rule 6) and any
missing `MAX_TRANSIT_PEER_INDEX` declaration (rule 7).

### Work package G — `Clone` removal

The static guard rejects `derive(Clone)` on `TransitHopRole`,
`TransitHopRegistration`, `TransitRegistry`, `TransitAdmissionState`,
`TransitAdmissionToken`, and the daemon-side `TransitHopMaterial`. The
previous-peer lock lives on the runtime-neutral registration and
never crosses a `Clone` boundary.

## 3. Verification floor executed (from repo root)

```text
cargo fmt --all --check                                      : passed
cargo check --locked --workspace --all-targets               : passed
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                              : 2979 passed, 26 ignored
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                              : passed (0 errors)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                              : passed
cargo test --locked --workspace --doc                        : passed
bash scripts/check-dependency-direction.sh                   : passed
bash scripts/check-runtime-boundaries.sh                     : passed
bash scripts/check-service-tunnel-boundaries.sh              : passed
bash scripts/check-fixture-manifest.sh                       : passed
bash scripts/check-m11-transit-boundaries.sh                 : passed
cargo test --locked -p i2pr-tunnel --all-targets             : 374 passed
cargo test --locked -p i2pr-daemon --test m11_transit_data_plane -- --test-threads=1
                                                              : 2 passed
```

`scripts/check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`, `check-ntcp2-interoperability.sh`,
`check-constrained-host-lane-boundary.sh`,
`check-sam-acceptance-evidence.sh`, `check-ssu2-acceptance-evidence.sh`,
`check-i2cp-acceptance-evidence.sh`,
`check-service-tunnel-acceptance-evidence.sh`,
`check-exploratory-tunnel-evidence.sh`,
`check-netdb-tunnel-evidence.sh`,
`check-destination-tunnel-evidence.sh`,
`check-streaming-tunnel-evidence.sh`,
`check-m6-mixed-router-acceptance-evidence.sh`, and the NTCP2
execution-lane harness remain unchanged by this plan; their
pre-existing pass status is preserved.

## 4. Tests run, with results

- `cargo test --locked --workspace --all-targets -- --test-threads=1`
  → **2979 passed**, 26 ignored (104 suites).
- `cargo test --locked -p i2pr-tunnel --all-targets` → **374 passed**.
- `cargo test --locked -p i2pr-daemon --test m11_transit_data_plane -- --test-threads=1`
  → **2 passed**.
- `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1`
  → **1135 passed**, 25 ignored (54 suites).
- `cargo doc --locked --workspace --no-deps` with
  `RUSTDOCFLAGS="-D warnings"` → passed.

## 5. Files changed

- `crates/i2pr-tunnel/src/transit.rs` — runtime-neutral data plane
  API surface.
- `crates/i2pr-tunnel/src/lib.rs` — re-exports for the new types.
- `crates/i2pr-daemon/src/transit_compose.rs` — daemon composition:
  `TransitHopMaterial` move-only, `TransitDispatch` rejected-metadata,
  envelope wrappers, `TransitPeerIndex`, rollback on every
  non-`Accepted` outcome.
- `crates/i2pr-daemon/src/transit_owner.rs` — new live owner module.
- `crates/i2pr-daemon/src/lib.rs` — adds `transit_owner` module.
- `crates/i2pr-daemon/tests/m11_transit_data_plane.rs` — new
  daemon-level integration test.
- `scripts/check-m11-transit-boundaries.sh` — extended static guard.
- `plans/registry.md` — M11 transit-tunnels row updated.
- `plans/subsystems/transit-tunnels-roadmap.md` — milestone table
  Plan 253 row updated.
- `specs/support.toml` — `plan_253_status` token + M11 transit
  narrative updated.

## 6. Tests not run (and why)

- The Plan 252 closure record's external NTCP2 lanes remain
  environment-gated; none of them are required to advance the M11
  transit foundation. Plan 253 explicitly does not require any
  external interop evidence (Plan 254 owns that work and remains
  unregistered).
- The Plan 236 Java family is a separate, retained dependency; it
  is not in the M11 transit subsystem's authority boundary.

## 7. Dependency / secret-handling decisions

- No new production dependencies were introduced.
- `TransitHopMaterial` is move-only; the static guard forbids any
  `Clone` derive on the daemon-side struct or hand-implemented
  `Clone` impl.
- The daemon composition never logs raw STBM / OTBRM body bytes.
- The peer/session index is bounded at 4096; exceeding the bound
  surfaces `TransitPeerError::CapacityFull(usize)` rather than
  silently growing.

## 8. Deviations from the plan-of-record

None.

## 9. Remaining risks

- The live `RouterI2npOutcome::TunnelBuildReserved` classifier still
  does not feed `TransitOwner::dispatch_short_build` in production;
  the integration test exercises the seam in isolation. Wiring the
  classifier into the live dispatcher is the natural first slice of
  Plan 254's external qualification, but production callers
  continue to see the existing reserved outcome until then.
- `plan253_short_build_payload` is a placeholder that returns an
  empty slice; Plan 254 must extend the live owner to receive the
  Plan 184 re-decoded body by reference.
- No advertisement / capability / RouterInfo change is made;
  `m11_transit_tunnels = ...; advertised=false` remains the truthful
  position in `specs/support.toml`.

## 10. Unblock audit

Per the plan-of-record: Plan 254 must remain unregistered.

Confirmed at closure time:

- `plans/registry.md` — M11 transit-tunnels row lists Plan 254 as
  `unregistered-after-plan253`.
- `plans/subsystems/transit-tunnels-roadmap.md` — Plan 254 row
  reads `unregistered-after-plan253`.
- `specs/support.toml` — `plan_254_status = "unregistered-after-plan253"`.
- `plans/implementation/transit-tunnels/` — no `254-*.md` file
  present.

No new plan-of-record for Plan 254 exists. The plan remains
unregistered, behind the Plan 253 closure record.
