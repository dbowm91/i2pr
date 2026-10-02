# Plan 283 status — passed with third-class evidence and controlled activation

Status: `passed-m12-third-class-evidence-and-controlled-activation`

- Plan: `plans/implementation/floodfill/283-m12-third-class-evidence-and-controlled-activation.md`
- Predecessors: Plan 277 (`stopped-m12-daemon-runtime-publication-and-reply-adapter-contract-required`),
  Plan 282 (`stopped-m12-activation-blocked-on-above-floor-reachability-evidence-corrective-via-plan283`).
- Disposition: **passed**. Every remaining Plan 282 acceptance item (criteria 2, 3, 4, 11;
  §10 rows 2, 11, the live half of 13; the dial-permit cancel baseline) passes with the
  evidence strategy recorded below. No invariant was weakened: the Plan 159
  direct-publication contract, the type-5 deferral, and all boundary guards hold on the
  closing head. Nothing in this plan ships a production floodfill path (the owner and the
  controlled composition have no production caller by design until Plan 278).

## Implementation commits

| Commit | Content |
|---|---|
| `805e018` | Option 3 evidence driver (WP-A): session `queue_address` egress, service-owned controlled peer-test context + in-session/out-of-session ingest, `ssu2_controlled_peer_test` driver, 3/3 proof tests; activation/withdrawal composition (WP-B) with failure rollback to non-Active; WP-C rows 2/11/13-live/dial-permit/ordinary-refusal; canonical I2P base64 fix; M12 guard extension; roadmap/support/deep-dive reconciliation. |
| `d44b51b` | Activation publish-failure rollback to the same-address non-`f` record (invariant 6), the installed-bytes observability accessor, and the live rollback proof row. |
| This closure commit | Closure record, registry/roadmap/support/README reconciliation, unblock audit. |

## Evidence-strategy decision (Plan 283 §4, Option 3)

Options 1 and 2 were not available to this lane: no heterogeneous reference traffic runs
in the ordinary local suite (the Plan 278 lane owns exact-pinned i2pd traffic separately),
and a loopback migration proof yields at most {bind, migration} with no Address blocks on
i2pr↔i2pr traffic — documented insufficient in the plan itself. **Option 3 (registered
peer-test driver) is selected**, sized inside this plan as the plan requires.

The corroboration classes, by name, with the protocol events that produced them:

- class 0, explicit bind: `note_explicit_bind_for_controlled_qualification` on the live
  bound loopback socket at activation start (refuses non-loopback);
- class 1, authenticated-peer observation: the live session's authenticated peer
  endpoint observed during the controlled exchange;
- class 2 (third), peer-test confirmation: the wire-real 7-message peer-test exchange
  (`run_controlled_peer_test` with ephemeral helpers; Msg7 carries only the
  helper-observed endpoint) reaching `PeerTestOutcome::DirectReachabilityConfirmed`,
  ingested as `PeerTestResult{Confirmed}` **inside the runtime service owner only**.

The tracker transition `CandidateReachable → Reachable` on those three classes is
exercised by `ssu2_controlled_peer_test.rs` (Reachable asserted post-exchange) and
consumed by `activate_controlled` (`publication_material` returns the qualified live
address only above floor). No class is recorded without its fact: binds must be bound,
observations come from authenticated traffic, peer-test results come from real
`Confirmed` outcomes (every other terminal outcome records nothing). The driver module
itself records nothing (M12 guard).

## Requirement-to-evidence

| Open Plan 282 item | Evidence | Result |
|---|---|---|
| Crit 2 (publication material, activation use) | `activate_controlled` consumes `publication_material` from the live owner after the controlled exchange; row 2 installs and serves the `caps=f` RI (`controlled_eligibility_installs_floodfill_router_info_and_serves_it`) | Done |
| Crit 3 (atomic non-`f`↔`f` rotation, identity/address preserved) | Row 2 installs `f` at the live endpoint; row 11 withdraws to non-`f` at the same address (port equality asserted); byte-level serve/install comparisons | Done |
| Crit 4 (future handshakes use updated RI) | Row 13 live half: v1 handshake emits v1 bytes, v2 install, v2 handshake emits v2 bytes, pre-existing session still reports v1 (`future_handshakes_emit_latest_router_info_and_old_sessions_keep_theirs`) | Done |
| Crit 11 (lifecycle: health withdrawal full sequence) | Row 11: Active → health loss → Draining → same-address non-`f` install + publish → bounded drain of the queued ack (delivered live to Bob, `DeliveryStatus` id `0xBEEF22`) → Disabled → admission refused (`health_loss_withdraws_f_stops_admission_and_drains_to_disabled`) | Done |
| Crit 11 (dial-permit cancel baseline, D5 open half) | `cancelled_dial_returns_admission_to_baseline`: mid-dial cancel reports `Cancelled` (never `Timeout`), `pending_outbound` returns to 0, a live dial succeeds immediately after | Done |
| Crit 12 (ordinary/default cannot advertise) | `ordinary_configuration_cannot_activate_or_advertise`: pre-cancelled composition → `Cancelled`; full evidence without eligibility → `EligibilityFailed`, no permit, generation stays 0 | Done |
| Crit 13 (§10 matrix) | Lifecycle suite 14/14 live green (rows 2–9, 11, 12, 13-live, dial-permit, ordinary-refusal) + retained unit rows (10 persist, 14 default-off) | Done |
| Crit 1, 5–10, 15 | Unchanged from the 282 retained slice; guards + full floor re-greened on the closing head | Done |
| Plan 283 §3.6 (activation failure leaves non-`f` runtime, non-Active role) | Every post-`begin` failure calls `fail_activation`; publish-after-install failure additionally installs the same-address non-`f` withdrawal-form record (`activation_publish_failure_rolls_back_to_previous_router_info`: `PublishFailed`, non-Active, no `f` caps, live address kept, generation 2) | Done |
| Plan 283 §8 guards | `activate_controlled`/`withdraw_controlled` defined only in daemon `floodfill.rs`; withdrawal builder requires `&FloodfillAdvertisementPermit`; corroboration recording only in `ssu2_runtime.rs` + pre-existing relay table; driver records nothing; all Plan 282 guards unweakened | Done |

## Verification (all run locally on the closing head)

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — 3229 passed, 27 ignored (pre-existing gates), 0 failed.
- Focused: `floodfill_controlled_lifecycle` 14/14; `ssu2_controlled_peer_test` 3/3; `i2pr-netdb` 171; `i2pr-transport-ssu2` 189; `i2pr-runtime` lib 79 + `ssu2_local` 9 + `ssu2_peer_relay` 7 — all with `--test-threads=1`.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — 0 tests (none defined), passed.
- `git diff --check` — passed.
- `bash scripts/check-m12-floodfill-boundaries.sh` — passed (with the new §8 invariants).
- `bash scripts/check-dependency-direction.sh`, `check-runtime-boundaries.sh`, `check-service-tunnel-boundaries.sh` — passed.
- All routine `scripts/check-*.sh` lanes (fixtures, vectors, NTCP2 interop, constrained-host, M11 boundaries/qualification, SAM/SSU2/I2CP/service-tunnel/exploratory/NetDB/destination/streaming/M6 evidence) — passed.
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — 18 passed.
- `cargo deny check advisories bans sources` — ok.
- Hosted exact-head ordinary CI: green — run `36964249885` on `805e018` and run
  `36964612599` on `d44b51b` both report CI `success`. (The `m11-transit-external`
  manual lane fails on every head including prior green heads for want of its
  provisioned environment; steady state, not a regression.)

## Invariant review

No invariant was weakened. The Plan 159 `Direct`-requires-`Reachable` contract is
unchanged (the third class is real protocol traffic, not a lowered bar); handshake
Address blocks are still not observations; `caps=f` remains unavailable outside the
controlled qualification path (guard + ordinary-refusal row); normal daemon config
cannot construct the permit (guard); daemon still has no `transport-ssu2` dependency
(guard); NetDB stays runtime-neutral (guard); type 5 stays deferred; persistence
stays Plan 276 format v1; no wire-format change.

## Failure / migration / compatibility review

No wire-format, config-surface, persistence-format, or API-compatibility change ships.
`install_local_router_info` is unchanged; the additive `installed_local_router_info`
accessor exposes only public bytes. Activation/withdrawal have no production caller,
so no caller migration exists. Withdrawal installs the safe non-`f` state before its
own publish, so its publish-failure degradation is fail-safe by construction. The
generation counter stays monotonic diagnostics (install + rollback reinstall each
bump once).

## Security review

No new secret handling: the bridge and driver carry only public addresses, hashes,
and categorical/expiry metadata; helper keys are ephemeral per exchange and never
logged; the service boundary still receives no private transport key bytes. No new
network-reachable surface (loopback UDP only, no listener beyond the existing runtime
owner, no advertisement). The controlled APIs are config-gated (`controlled_peer_test`
profile flag) and the ordinary path cannot reach them.

## Documentation / operational evidence

Authoritative surfaces agree on the passed state: this record, the Plan 283 plan,
`plans/registry.md`, the floodfill roadmap §7, `specs/support.toml`
(`plan_283_status` passed), architecture deep-dives (`i2pr-runtime` controlled-driver
row, `i2pr-daemon` activation composition), and the README current-work line. No
operator action follows; floodfill remains non-advertised (`m12_caps_f_advertised =
false`); external floodfill interoperability is not claimed until Plan 278 passes.

## Known limitations

- The rollback proof uses a coordinator/bundle identity mismatch as the deterministic
  publish-failure injector (a pairing unreachable in production); it proves the
  rollback mechanism, not a production failure mode.
- The startup placeholder RouterInfo (unbound port) is not reinstallable through
  validation, which is why rollback installs the freshly built same-address non-`f`
  record rather than byte-restoring; noted in code and in the rollback row.
- Plan 278 owns exact-pinned i2pd qualification; Java-family work stays with Plan 279.

## Findings by severity

- Medium: one, test-only, found and fixed in this plan — three new-row waits in
  `floodfill_controlled_lifecycle.rs` omitted `.await`, so the futures never executed
  and the rows passed without synchronizing on responder promotion (the
  already-established dialer side carried the assertions). The prior session's
  "phantom snapshot" hunt was this defect observed through a stale incremental
  binary; a clean rebuild plus the missing awaits resolved rows 2/11/13
  deterministically (14/14 green since). No production code was involved; the full
  suite plus clippy/`-D warnings` floor is green on the fix.
- Medium: one, product, found and fixed in this plan — `crates/i2pr-netdb/src/base64.rs`
  encoded the wrong I2P alphabet (`-?` + `~` padding) instead of the Plan 142
  canonical form (`-~` + `=` padding). Corrected with frozen-vector coverage
  (`i2pr-netdb` 171 green); loopback suites are symmetric so they passed either way,
  but exact-pinned reference traffic requires the canonical form.
- Low: none open. The `std::net` import in a new transport test violated the runtime
  boundary script; rewritten to the crate-conventional `core::net` path (guard green).
- High/critical: none.

## Roadmap disposition

Passed. Plans 277/282 remain stopped, corrected-via-283. Plan 278 moves to ready (see
unblock audit). Plan 279 stays blocked on 278. Plan 280 stays stopped; type 5 stays
deferred under 281. Normal/public `caps=f` stays unclaimed.

## Unblock audit

Audited `plans/registry.md` blocked work plus the floodfill roadmap §6 dependency
graph against this pass: Plan 278 lists Plan 283 as its only hard dependency, and its
inputs (the completed controlled daemon path on the retained 277/282 work) now exist —
Plan 278 moves to `ready` in the same commit. Plan 279 still lists Plan 278 (not yet
passed) and stays blocked. No other subsystem lists Plan 283. `next_executable_plan`
moves to plan278. No corrective pass is required: no defect or missing requirement
remains open.
