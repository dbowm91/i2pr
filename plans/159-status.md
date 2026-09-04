# Plan 159 status — Milestone 8 SSU2 path validation, publication, and transport selection

Status: **`passed-m8-ssu2-path-validation-publication-and-transport-selection`**.

Registered: **2026-09-04**. Closed: **2026-09-04**.

Plan of record:
[`plans/159-m8-ssu2-path-validation-publication-and-transport-selection.md`](159-m8-ssu2-path-validation-publication-and-transport-selection.md).

## Current authority

```text
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection

milestone8_protocol = ssu2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_implementation = plan159-path-publication-selection-landed

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 160
next_product_layer = milestone8-ssu2-v2
```

## What this pass did

1. Added the runtime-neutral path-validation machine (plan §§2–4)
   in the new `crates/i2pr-transport-ssu2/src/path.rs` (~700 lines
   with unit tests): `PathValidator` keeps one validated path plus
   bounded candidates (4 per session, 2 per address family, 8
   challenges per session, 32-byte caller-supplied challenges, 10 s
   deadlines). `note_authenticated_packet` (called only after the
   session authenticated the datagram) opens at most one candidate
   and emits one `ChallengeToSend`; `on_path_response` promotes only
   on an exact tracked-challenge match and consumes the proof;
   wrong/stale values are rejected and counted while the candidate
   survives to its deadline; expiry retains the old path. Candidate
   MTU is pinned to the 1280 minimum; the validated MTU
   (`1280..=9000`) has no packet-driven writer. Twelve unit tests
   pin spoof/legit/timeout/quota/family/MTU/budget/deadline
   behavior, including v4/v6 structural separation.
2. Fixed one real session defect with the new sealed-packet suite
   (no plan stop: the defect was in the new Plan 159 migration
   hook, not in Plan 156/157 protocol): the first
   `note_path_migrated` draft cleared sent provenance but left
   handed-out fragments with no loss record, stranding in-flight
   messages forever. It now declares every unacked packet lost
   through the existing bounded requeue policy (fresh generation,
   per-fragment ceiling, silent per-message failure past it), then
   resets flight to zero and the window to its minimum — proven by
   `migration_resets_flight_but_keeps_semantic_queues`, which
   retransmits fresh and delivers after migration.
3. Added the deterministic publication snapshot builder (plan §7) in
   the new `crates/i2pr-transport-ssu2/src/publication.rs`:
   `build_publication_snapshot(PublicationRequest)` decides
   `Direct` / `Firewalled` / `Withheld` from public keys only (zero
   keys rejected; private material is never an input). Direct
   host/port requires `Reachable` plus explicit `allow_direct`;
   anything weaker yields the unpublished static-only firewalled
   form (never a fabricated address); non-empty introducers without
   `allow_introducers` fail closed; options render canonical
   (sorted); snapshots carry evidence expiry for withdrawal; and
   `parse_snapshot` round-trips through the strict production
   parser. Seven unit tests pin determinism, gating, expiry, and
   redaction. Supporting enablers: `pub(crate) encode_i2p_base64`
   (promoted from test-only) and `Ssu2Capabilities::parse`.
4. Added the conservative router-level reachability policy (plan
   §§5–6) in the new `crates/i2pr-transport/src/reachability.rs`:
   `ReachabilityTracker` accumulates family-only
   `ReachabilitySignal`s (including structurally present
   `PeerTestResult` / `RelayFirewalledSignal` stubs for Plan 160)
   into Unknown/ObservedUnconfirmed/CandidateReachable/Reachable/
   Firewalled/Unreachable. Corroboration counts distinct signal
   classes per family with a validation-enforced floor of two, so
   one peer's external-address observation can never reach
   `Reachable`; configuration corroborates only under explicit
   opt-in; contradiction downgrades; expiry withdraws. Publication
   consumes snapshot copies, never packet/session objects; an
   `as_transport_observation` bridge feeds the existing manager
   ring buffer. Eleven unit tests pin the ceiling, corroboration,
   gating, contradiction, expiry, and redaction.
5. Added deterministic NTCP2/SSU2 selection (plan §§8–9) in the new
   `crates/i2pr-transport/src/selection.rs`: the pure
   `select_peer_transport` function consumes validated descriptors
   (this crate sits below both protocol crates, so no RouterAddress
   types cross the boundary) and decides Reuse / Dial /
   DialFallback / NoCompatibleAddress / BackedOff / ResourceDenied.
   Reuse precedes any dial; address failures exclude only their
   tags; backoff excludes only backed-off transports; direct sorts
   ahead of introducer-only; ties break deterministically; peer
   limits deny new dials while keeping reuse. Fifteen unit tests
   prove every §9 fallback semantic, including no cross-transport
   poisoning and input-order-independent determinism. The manager
   gained two read-only selection inputs
   (`authenticated_links_for`, `peer_at_link_limit`); no second
   manager and no async trait framework were introduced.
6. Wired path validation into the runtime (plan §§2–5, §11) without
   changing wire semantics: every active session owns a
   `PathValidator` from promotion; `handle_active_datagram`
   classifies the source only after session authentication;
   candidates emit one minimum-MTU OS-CSPRNG challenge;
   challenges are answered once to the challenger (authenticated
   packets only, so no amplification to unauthenticated victims);
   migration moves the endpoint plus per-IP/subnet accounting,
   resets congestion via `note_path_migrated`, records
   `ValidatedPath` evidence (mirrored into the manager ring
   buffer), and counts; wrong/stale values count rejections;
   `AddressObserved` records peer-observed signals; the central
   scheduler expires candidates and sleeps to their deadlines; and
   `Ssu2Snapshot` adds six path counters plus the conservative
   reachability state (counts only, no endpoints).
7. Added the sealed-packet acceptance suite (plan §11)
   `crates/i2pr-transport-ssu2/tests/path_validation.rs` (9
   tests): two paired sessions from a real Noise transcript dance
   exchange byte-identical wire datagrams through validators —
   unauthenticated/replay rejection without candidates, bounded
   candidate creation, wrong-response rejection, exact-once
   migration with continued bidirectional delivery, timeout
   retention, v4/v6 cross-validation refusal, migration congestion
   reset with fresh retransmit, and minimum-MTU control fit.
8. Added the real-UDP runtime suite (plan §11) inline in
   `ssu2_runtime.rs` (3 tests, live services plus a raw third
   socket as the new path; every sealed byte comes from a live
   session): `legitimate_path_migration_over_real_udp` proves
   exact-once migration with behavioral post-migration delivery to
   the raw socket, old-path replay harmlessness, and automatic
   genuine return migration on re-validation;
   `spoof_burst_from_new_sources_never_migrates` proves 800
   spoofed datagrams from four sources open no candidate and
   migrate nothing while the session stays bidirectionally usable;
   `challenge_response_control_round_trip_over_real_udp` proves
   the no-migration control loop.
9. Documented (plan file list): new path/publication modules with
   crate-root re-exports; extended selection/reachability manager
   surface; runtime path-validation sections; updated
   `i2pr-transport`, `i2pr-transport-ssu2`, and `i2pr-runtime`
   deep-dives; `specs/protocols/09-ssu2.md` Plan 158/159 scope
   sections; `specs/support.toml` (new
   `ssu2.v2-path-validation-publication-selection` surface,
   experimental, `advertised = false`); both skill copies
   (authority rollover, deduped scheduler lesson, two Plan 159
   lessons); `README.md`, `AGENTS.md`, `plans/README.md`; and this
   status record. No new dependencies; `cargo deny` passes.

## Non-goals kept

No PeerTest/relay protocols (Plan 160 owns the variants already
reserved here), no public-network RouterInfo publication (snapshots
only; the builder never publishes), no daemon `[ssu2]` activation
(`enabled = true` still fails closed), no non-loopback dial
relaxation, no external-router interop (Plan 161), no
multipath/multihoming beyond spec-required migration.
`advertised = false` in the support ledger. Deliberate non-changes
(no plan deviation): the loopback dial gate and daemon fail-closed
activation stay until Plan 160/161 own their evidence, per plan
§§7/13/15.15.

## Interpretation notes (not stop conditions)

1. The runtime answers path challenges even from unvalidated
   candidates, because the challenge arrived inside an
   authenticated session packet: the responder is the peer, not an
   amplifier victim. Answers stay single-shot minimum-MTU
   datagrams; replays die in the session filter first.
2. A live peer that genuinely re-proves an old path migrates back
   automatically (observed in the return-migration test with zero
   test intervention). Each direction change requires full
   challenge/response proof; flapping links therefore cost one
   round trip per move, bounded by quotas.
3. The service-wide candidate ceiling (256) is a backstop; the
   per-session quotas bind first in every tested trajectory.
4. `ReachabilityTracker` is per service (router-level), not per
   peer: corroboration counts signal classes, and the loopback-only
   runtime emits `ValidatedPath`/`AddressObserved` signals only.
   Publication-grade `Reachable` additionally needs configured or
   peer-test confirmation that does not exist yet, so snapshots
   stay firewalled/withheld in every tested configuration.

No stop condition fired: migration semantics needed no spec
arbitration beyond the implemented machine, selection required no
duplicate-policy change, publication required no NetDB mutation,
and spoof testing showed quota-flat state growth (800 datagrams,
zero candidates).

## Validation record

Local validation on the closing tree (all green):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
  (1469 passed, 57 suites, 0 failed)
cargo test --locked -p i2pr-transport --all-targets (43 passed, 2 suites)
cargo test --locked -p i2pr-transport-ssu2 --all-targets (132 passed, 4 suites)
cargo test --locked -p i2pr-runtime --all-targets (72 passed, 2 suites)
cargo test --locked -p i2pr-runtime --test ssu2_local (9 passed)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh (14 rows pinned, hashes match)
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh (22 rows command-derived)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' (153 tests, OK)
cargo deny check advisories bans sources (ok)
```

Deltas from the Plan 158 baseline (1411 passed, 56 suites): +26
transport tests (15 selection + 11 reachability), +29
transport-ssu2 tests (13 path unit + 7 publication unit + 9
sealed-packet trajectories), +3
runtime lib tests (real-UDP path migration/spoof/round-trip; the 9
ssu2_local product tests stay green).

Hosted lanes: routine CI plus the manual SAM external workflow must
pass on the exact closing commit; the run IDs are recorded in the
handoff commit message and below once available.

Hosted lanes:

- Closing commit `CPENDING` — routine CI run `RPENDING`: conclusion
  `CPENDING`.

## Acceptance criteria (all true, plan §15)

1. Endpoint changes require authenticated spec-defined path
   validation (`note_authenticated_packet` only after session
   authentication; `legitimate_path_migration_over_real_udp`).
2. Candidate path state/count/time/bytes are bounded (4/2-family/8
   challenges/32-byte/10 s/minimum-MTU; quota tests).
3. Wrong/replayed/stale path responses cannot migrate a session
   (`ChallengeMismatch`/`NotACandidate`/`ExpiredCandidate`;
   sealed-packet wrong-response/timeout/replay tests).
4. Legitimate real-UDP path migration is proven (exact-once
   migration to a raw socket with behavioral delivery proof, plus
   automatic genuine return migration).
5. Candidate path MTU/cwnd stay conservative until validation
   (pinned 1280 candidate MTU; `note_path_migrated` resets to the
   minimum; control-fit tests).
6. External-address observations are typed and separate from
   publication authority (`ReachabilitySignal` vs
   `ReachabilityTracker` vs snapshot copies).
7. One unauthenticated/single-peer observation cannot create
   `Reachable` publication state (corroboration floor of two;
   single-observation tests).
8. SSU2 RouterAddress publication snapshots are deterministic and
   policy-gated (canonical round-trip tests; direct needs
   `Reachable` plus opt-in).
9. Direct addresses withdraw when supporting evidence expires
   (`is_expired` withdrawal tests; scheduler-owned expiry).
10. Generic transport selection includes SSU2 without introducing a
    second manager (pure `select_peer_transport` + two read-only
    manager inputs).
11. Existing authenticated link reuse takes precedence over needless
    redial (reuse tests for both transports).
12. NTCP2/SSU2 failure/backoff/fallback semantics are deterministic
    and tested (fallback tests both directions; determinism test).
13. One transport failure does not incorrectly poison the other
    (address-failure isolation test).
14. IPv4/IPv6 candidate states are structurally separated
    (independent family quotas; cross-family refusal tests with
    real seals).
15. No public advertising/interop claim is introduced
    (`advertised = false`; daemon/loopback gates unchanged).
16. Full workspace/SSU2 quality floor passes (see validation
    record).
17. This record advances only to Plan 160.

## Handoff

Plan 159 is closed. Execute Plans **160 → 161** in order under the
Plan 154 roadmap authority. Plan 160 owns PeerTest/relay
requester/introducer/target roles, anti-amplification policy, and
validated introducer records feeding this plan's reachability and
publication inputs.
