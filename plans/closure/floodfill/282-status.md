# Plan 282 status — stopped at the above-floor reachability evidence boundary

Status: `stopped-m12-activation-blocked-on-above-floor-reachability-evidence-corrective-via-plan283`

- Plan: `plans/implementation/floodfill/282-m12-runtime-publication-and-reply-delivery-contract-corrective.md`
- Predecessor: Plan 277 (`stopped-m12-daemon-runtime-publication-and-reply-adapter-contract-required`).
- Disposition: **stopped with retained executable work**. Every Plan 282 work package that is
  provable on static loopback-homogeneous traffic is implemented, guarded, and tested (see
  requirement disposition). The activation/withdrawal slice (acceptance criteria 2, 3, 4, 11;
  §10 rows 2, 11, and the live half of 13) cannot execute there without fabricating evidence,
  which fires the plan's own stop condition 5. Plan 283 is registered as the narrower
  corrective owning the third-class evidence strategy plus activation/withdrawal completion.
  No crypto, codec, wire, or advertisement change was made; nothing in this plan ships a
  production floodfill path (the owner has no production caller by design until Plan 283).

## Implementation commits

| Commit | Content |
|---|---|
| `adf3d29` — prior builder work (retained) | Runtime publication bridge, install API, coordinator reply-route fixes (A1/A2), effect encoder (D2/D3/D4), owner skeleton, boundary-script additions, support/roadmap/registry/docs updates. |
| This closure commit | Slice 1: RI rotation freshness/size policy + generation counter + snapshot exposure, explicit-bind corroboration flag + narrow recording API, publication/reachability negative-case matrix, session-preservation proof. Slice 2: owner outcome accounting + bounded cancel-drain + stats return. Slice 3: `floodfill_controlled_lifecycle` live suite (8 rows), persistence-restart proof, three new boundary guards, docs/support/registry/roadmap reconciliation. |

## Requirement-to-evidence

| Plan 282 requirement | Evidence | Result |
|---|---|---|
| A1 store-ack id == reply token + typed route (crit 5, 6) | `floodfill_service.rs` builds `DeliveryStatusMessage::new(message.reply_token, …)`; daemon encoder rejects id/token mismatch; boundary script locks the token-sourced construction; live rows 5 (direct, `Some(0)` tunnel) and 6 (tunnel-nested, exact id) in `floodfill_controlled_lifecycle.rs` | Done |
| A2 remove `from == peer`, keep provenance/throttle (crit 7, 8) | No `from == peer` assertion in service or daemon; regression with peer ≠ gateway for direct + tunnel; live row 3 (a1 delivers, a2 receives) | Done |
| A3 route abuse bounds (crit 10) | `validated_dial_target` resolves only through answer-eligible main-router RI + strict SSU2 parse; unknown peer → `InvalidRoute` (unit); `daemon_dial_target` loopback-only; direct-dial failure → typed outcome, never a tunnel (live row 9) | Done |
| B runtime publication bridge (crit 2, partial) | `publication_material` from live socket + reachability + owner keys via the Plan 159 builder; private keys never cross; daemon has no `transport-ssu2` dep (guard); negative matrix: Closed/NoBoundSocket/Unqualified/expired-evidence/invalid endpoint/port-zero/partial-evidence (runtime unit tests) | Done except activation use (blocked, see findings) |
| B loopback corroboration pair | `explicit_bind_corroboration` flag (default false; daemon maps true) + `note_explicit_bind_for_controlled_qualification` (loopback-only, fail-closed); pair yields exactly `CandidateReachable`, locked by `explicit_bind_corroboration_pairs_with_peer_observation` | Done; kept as Plan 283 foundation |
| C rotation policy + atomicity + generation (crit 3, 4, 13) | `install_local_router_info(encoded, wall_now_ms)`: 16 KiB pre-check, decode cap, signature, Plan 103 freshness windows (24 h/1 h), identity/netId/binding match, atomic replace, generation bump, snapshot exposure; live session survives install; post-rotation dial succeeds | Done except live emission proof + activation-driven rotation (blocked) |
| D1/D4 bounded direct delivery, no tunnel fallback (crit 10) | Session reuse first, single supervised dial from validated RI, `DirectFloodAction` structurally tunnel-free (guard); live rows 7 (established), 8 (dial), 9 (dead target → typed failure) | Done |
| D2 Garlic-in-TunnelGateway (crit 9) | Exactly-one nesting enforced + live row 4 opens to the expected body via `open_netdb_ecies_reply` | Done |
| D5 concurrency/backpressure (crit 12, partial) | Single owner, serialized dial, leases held across await and released on every path (unit: double-drain fails closed, drop releases budget); outcome accounting; cancel-drain to deadline with stats return (live row 12) | Done except dial-permit baseline on mid-dial cancel (open, owned by 283) |
| E lifecycle, persistence, maintenance (crit 11, partial) | Owner startup validation, single supervised maintenance ticks, cancel-drain + shutdown order, persistence restart with mandatory revalidation + narrowed provenance (persist unit, row 10) | Done except activation/withdrawal composition (blocked) |
| F §10 rows | 1 runtime-unit; 3, 4, 5, 6, 7, 8, 9 live; 10 persist-unit; 12 live cancel-drain + unit; 14 unit default-off | Done except 2, 11, 13-live |
| §14 static guards | Dependency direction, NetDB neutrality, no `from == peer`, token-sourced ack (new), Garlic+TunnelGateway shape, no-`transport-ssu2` dep, tunnel-free `DirectFloodAction` (new), no-config-permit (new), construction confinement to `floodfill.rs` (new), type-5 denial | Done |
| Crit 1, 12, 15 | No forbidden dependency; default profile disabled without permit; findings below (one high boundary, no shipped defect) | Done |

## Verification (all run locally on the closing head)

- `cargo fmt --all --check` — passed.
- `cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` — 170 passed.
- `cargo test --locked -p i2pr-netdb-persist --all-targets -- --test-threads=1` — 9 passed.
- `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` — 93 passed, 1 ignored (pre-existing external gate).
- `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` — 1308 passed, 26 ignored (pre-existing gates).
- `cargo clippy --locked -p i2pr-netdb -p i2pr-runtime -p i2pr-daemon -p i2pr-netdb-persist --all-targets -- -D warnings` — clean.
- `git diff --check` — passed.
- `bash scripts/check-m12-floodfill-boundaries.sh` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- Full workspace floor on the closing head:
  - `cargo check --locked --workspace --all-targets` — passed.
  - `cargo test --locked --workspace --all-targets -- --test-threads=1` — 3215 passed, 27 ignored (pre-existing gates), 0 failed.
  - `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — clean.
  - `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
  - `cargo test --locked --workspace --doc` — 0 tests (none defined), passed.
  - All routine `scripts/check-*.sh` lanes (dependency, runtime, service-tunnel, fixtures, vectors, NTCP2 interop, constrained-host, M11 boundaries/qualification, SAM/SSU2/I2CP/service-tunnel/exploratory/NetDB/destination/streaming/M6 evidence) — passed.
  - `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — 18 passed.
  - `cargo deny check advisories bans sources` — ok.
- Hosted exact-head CI: not run (local-only stop record; required before any Plan 283 pass claim).

## The evidence-boundary finding (drives this stop)

Static loopback-homogeneous traffic can factually produce at most **two** reachability
corroboration classes: the explicitly configured bind (class 0, via the new narrow API)
plus an authenticated-peer observation (class 1). The tracker maps exactly-floor evidence
to `CandidateReachable`; `Reachable` requires a third class (path-validation migration or
a confirmed peer test). The Plan 159 publication builder renders a direct address **only**
for `Reachable` — verified in `crates/i2pr-transport-ssu2/src/publication.rs`
(`direct_allowed` requires `state == Reachable`; otherwise the firewalled static-only
form, which can never satisfy `is_qualified_ssu2_address`). That contract outranks Plan
282 prose, so it was not weakened; the gate keeps requiring `Reachable`, and the
corroboration test locks `CandidateReachable` + `ReachabilityUnqualified` as the
loopback ceiling. Consequences:

- `publication_material` cannot return a usable address on loopback-homogeneous traffic,
  so the §7 activation sequence cannot start there, so §10 rows 2/11 and the live half of
  13 cannot execute, so criteria 2, 3, 4, 11 cannot close. This is exactly stop condition
  5 ("the existing runtime cannot expose factual reachability/address evidence without
  fabricating publication state").
- Recording handshake Address blocks as observations was considered and rejected: the
  peer's self-reported address adds no evidence independent of the dial target/source,
  so counting it would inflate corroboration without new facts.
- Accepting `CandidateReachable` in the transport builder was considered and rejected:
  it would weaken the Plan 159 direct-publication contract from under its closure.
- The retained pair (flag + narrow recording API) is Plan 283's foundation: with a third
  class from heterogeneous reference traffic (or a registered third-class driver), the
  same bridge reaches `Reachable` unchanged.

Two design facts discovered while proving the executable slice (both closed, no code
defect): the I2NP wire format requires `reply_tunnel_id` + `reply_gateway` with any
nonzero token (direct replies use `Some(0)`); replication plans only token-bearing
publisher stores (zero-token floods are never reflooded by design).

## Invariant review

No invariant was weakened to obtain the retained results. The type-5 deferral, the
loopback-only publication clamp, the no-`transport-ssu2`-in-daemon direction, NetDB
runtime neutrality, and the Plan 159 direct-publication contract all hold on the closing
head and are statically guarded. The owner has no production caller, so no live
floodfill behavior exists to regress.

## Failure / migration / compatibility review

No wire-format, config-surface, persistence-format, or API-compatibility change ships:
`install_local_router_info` gained a wall-clock parameter and the owner gained a drain
parameter plus a stats return, but neither has a production caller yet, so no caller
migration exists. The generation counter and snapshot field are additive diagnostics.
Persistence remains Plan 276 format v1.

## Security review

No new secret handling: the bridge carries only public addresses plus categorical/expiry
metadata; ECIES reply keys/tags in tests are random per-run and never logged. No new
network-reachable surface (no listener, no dialer beyond the existing runtime owner,
no advertisement). The explicit-bind recording API refuses non-loopback endpoints and
is confined by guard to the daemon floodfill owner, which has no production caller.

## Documentation / operational evidence

Authoritative surfaces agree on the stopped state: this record, the Plan 283
registration, `plans/registry.md`, the floodfill roadmap §7, `specs/support.toml`
(`plan_282_status` stopped, `plan_283_status` registered), architecture deep-dives
(`i2pr-runtime`, `i2pr-daemon`, `i2pr-netdb`), and `specs/protocols/02-i2np.md`. No
operator action follows; floodfill remains non-advertised (`m12_caps_f_advertised =
false`).

## Known limitations

- Activation, health withdrawal, and live RouterInfo-emission proofs are open (Plan 283).
- Dial-permit baseline return on mid-dial cancellation is asserted only by construction
  (single owner, no per-effect tasks), not by a dedicated test (Plan 283).
- Hosted exact-head CI was not run for this stop record (local-only); it is required
  before any Plan 283 pass claim.
- The §10 row 6 inner injection targets a fixture tunnel id, not a live inbound tunnel;
  outer delivery to the requested gateway with the exact tunnel id is proven live.
- `FloodfillTime` remains defined in both `i2pr-netdb` (public) and the daemon owner
  (private import); no behavior impact.

## Findings by severity

- High: one — the above-floor evidence boundary blocks activation on loopback-homogeneous
  traffic (stop condition 5 fired honestly). No shipped code is affected; the retained
  bridge is the documented foundation for Plan 283. Not a defect: the conservative
  posture is working as designed.
- Medium: none open. The token/tunnel/gateway wire coupling and the
  publisher-only-replication rule were initially missed by the new live tests and are now
  covered (token-bearing stores, `Some(0)` direct tunnel).
- Low: none.

## Roadmap disposition

Stopped. Plan 277 remains `stopped/corrected-via-282-and-283` (its missing integration
is now half corrected: route/delivery/lifecycle mechanics landed; activation evidence
moved to 283). Plan 278 stays blocked, re-pointed to Plan 283. Plan 279 stays blocked
on 278. Plan 280 stays stopped; type 5 stays deferred under 281.

## Unblock audit

Audited `plans/registry.md` blocked work plus the floodfill roadmap §6 dependency graph
against this stop: no plan lists Plan 282 as a satisfied dependency (278/279 were
already blocked and remain so, now explicitly on 283). Plan 283 is registered
`ready` in the same commit: its inputs (retained 277/282 bridge, owner, and tests)
exist, and it has no other hard dependencies. `next_executable_plan` moves to plan283.
No other subsystem is affected.
