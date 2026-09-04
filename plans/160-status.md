# Plan 160 status — Milestone 8 SSU2 peer test and relay reachability

Status: **`passed-m8-ssu2-peer-test-and-relay-reachability`**.

Registered: **2026-09-04**. Closed: **2026-09-04**.

Plan of record:
[`plans/160-m8-ssu2-peer-test-and-relay-reachability.md`](160-m8-ssu2-peer-test-and-relay-reachability.md).

## Current authority

```text
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability

milestone8_protocol = ssu2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_implementation = plan160-peer-test-relay-reachability-landed

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 161
next_product_layer = milestone8-ssu2-v2
```

## What this pass did

1. Added the runtime-neutral PeerTest machine (plan §§2–4) in the new
   `crates/i2pr-transport-ssu2/src/peer_test.rs` (~1400 lines with unit
   tests): Alice/Bob/Charlie roles with nonce correlation (8 global, 2
   per peer, 10 s central-expiry deadlines, no task/timer per test),
   exact spec `PeerTestValidate` preimages (Msgs 1,2 Alice-signed
   without `ahash`; Msgs 3,4 Charlie-signed with `ahash`; Msgs 5–7
   optional), ±120 s freshness, role/state/sender/family gates,
   idempotent duplicates, and typed outcomes
   (`DirectReachabilityConfirmed` / `AddressMismatch` /
   `FirewalledLikely` / `Inconclusive` / `Rejected`). Msg 4 alone never
   confirms; unsigned Msgs 5–7 never confirm alone; contradictory
   authenticated observations yield `AddressMismatch`, never
   last-write-wins. Out-of-session type-7 long-header codecs
   (`build/parse_out_of_session_peer_test`, nonce-derived connection
   IDs) mirror the HolePunch path. Twelve unit tests pin preimages,
   trajectories, unsigned-downgrade, contradiction, negative matrix,
   idempotency, isolation, quotas, expiry/cancel, refusal, signer, and
   v6 separation.
2. Added the runtime-neutral relay machines (plan §5) in the new
   `crates/i2pr-transport-ssu2/src/relay.rs` (~1400 lines with unit
   tests): requester (bounded concurrent requests, response/tag/nonce
   correlation, distinct-tag isolation, HolePunch verification,
   transition to the normal handshake path), introducer (disabled by
   default; live-tag checks bound to Alice, per-peer/global quotas, 3x
   anti-amplification budget before crypto, replay idempotency,
   deterministic expiry, shutdown clears state), target
   (introducer-context validation, single HolePunch emission per intro
   with replay suppression). Exact `RelayRequestData` /
   `RelayAgreementOK` preimages; nonce-derived HolePunch connection IDs
   (`Dest = (nonce << 32) | nonce`, `Src = !Dest`); intro-key AEAD
   HolePunch codec (`build/parse_hole_punch`: DateTime + Address +
   RelayResponse). Seven unit tests pin preimages, conn IDs,
   HolePunch round trip, requester correlation, disabled-by-default
   quotas, target replay, and expiry.
3. Added the single bounded validated introducer-record owner (plan §6)
   in the new `crates/i2pr-transport-ssu2/src/introducer.rs`:
   `IntroducerTable` retains 8 records (peer hash, endpoint, intro key,
   nonzero tag, expiry, provenance) with deterministic oldest-expiry
   replacement, failed-peer removal, live selection capped at the spec
   3 (`MAX_PUBLISHED_INTRODUCERS`), expiry withdrawal, and
   `validated_introducers` conversion for the Plan 159 publication
   builder. Four unit tests pin selection/expiry/removal/overflow.
4. Typed the reachability policy inputs (plan §§4/11) in
   `crates/i2pr-transport/src/reachability.rs`: `PeerTestResult` now
   carries `PeerTestOutcomeKind` (`Confirmed` supports;
   `AddressMismatch`/`FirewalledLikely` contradict;
   `Inconclusive`/`Rejected` neutral so inconclusive tests never flip
   state arbitrarily); `RelayFirewalledSignal` is unchanged and relay
   success never proves direct reachability. Added the neutral-state
   unit test; updated the three existing peer-test tests to the typed
   shape.
5. Carried full relay/peer-test blocks through the data phase (plan
   §§2/5) in `crates/i2pr-transport-ssu2/src/session.rs`:
   `SessionEvent` variants now carry cloned full blocks
   (`RelayRequest`/`RelayResponse`/`RelayIntro`/`RelayTagRequest`/
   `RelayTag(u32)`/`PeerTest(PeerTestBlock)`, no longer opaque
   type/length stubs) and four single-shot bounded queue APIs
   (`queue_relay_request/response/intro`, `queue_peer_test`) emit them
   through the existing MTU-aware transmit path.
6. Added the runtime coordinator (plan §§2/7/11) in the new
   `crates/i2pr-runtime/src/ssu2_peer_relay.rs` (~1100 lines with unit
   tests): `Ssu2PeerRelayService` owns all five tables plus the
   `ReachabilityTracker`, per-source rate limits (8/sec, 1024 sources,
   cheap-drop before parsing), a bounded signer registry (128 keys;
   production RouterInfo plumbing deferred to Plan 161), trial-commit
   multi-key verification (wrong keys never consume a peer's message),
   `RelayFirewalledSignal` mirroring (relay success proves firewalled,
   never direct), validated-introducer selection, one central
   `poll_expired` scheduler input with `next_deadline_ms`, shutdown to
   baseline, and privacy-safe snapshots/`Debug` (counts only).
   Introducer service stays disabled by default. Seven unit tests pin
   disabled refusal, rate limiting, quotas/shutdown, replay
   idempotency, redaction, deadlines, and unknown signers.
7. Added the sealed-packet acceptance suite (plan §§9–11)
   `crates/i2pr-transport-ssu2/tests/peer_relay.rs` (8 tests): relay
   request/response/intro and peer-test Msg 4 traversing real sealed
   `Ssu2Session` datagrams into their tables, out-of-session
   PeerTest/HolePunch round trips with wrong-key rejection,
   introducer→publication→expiry integration, conservative
   reachability consumption (including the full Alice trajectory
   mapping to `Confirmed`), and the privacy regression.
8. Added the real-UDP NAT-like acceptance suite (plan §8)
   `crates/i2pr-runtime/tests/ssu2_peer_relay.rs` (7 tests): a
   test-only UDP forwarder rewrites sources (Alice/Bob/Charlie/Target
   behind the mapper); every PeerTest/Relay/HolePunch wire byte
   crosses real `UdpSocket` datagrams (asserted byte-equal on receipt,
   mapped source observed) — direct PeerTest with NAT rewrite,
   mismatch/inconclusive, 40-datagram flood cheap-dropped with zero
   state, the full relay product path (sealed RelayRequest →
   introducer → sealed RelayIntro → HolePunch → live
   `Ssu2RuntimeService` dial with bidirectional I2NP, distinct-tag
   isolation, relay-success-never-direct), introducer
   expiry/disabled/shutdown, concurrent isolation with crossing
   schedules, and publication/privacy integration.
9. Documented (plan file list): new peer_test/relay/introducer modules
   with crate-root re-exports; extended reachability/session/runtime
   surface; runtime peer-relay service; updated `i2pr-transport`,
   `i2pr-transport-ssu2`, and `i2pr-runtime` deep-dives;
   `specs/protocols/09-ssu2.md` Plan 160 scope section;
   `specs/support.toml` (new
   `ssu2.v2-peer-test-relay-reachability` surface, experimental,
   `advertised = false`); both skill copies (authority rollover, three
   Plan 160 lessons); `README.md`, `AGENTS.md`, `plans/README.md`; and
   this status record. No new dependencies; `cargo deny` passes.

## Non-goals kept

No public-network RouterInfo publication (snapshots only; the builder
never publishes), no daemon `[ssu2]` activation (`enabled = true` and
`introducer_service = true` still fail closed), no non-loopback dial
relaxation, no external-router interop (Plan 161), no TURN/STUN/UPnP/
NAT-PMP, no SSU1, no PQ SSU2. `advertised = false` in the support
ledger. Only Ed25519 router signatures verify (other signing-key types
fail as `UnsupportedSigner`; Plan 161 owns multi-algorithm interop);
runtime RouterInfo signing-key plumbing is deferred (tests register
keys explicitly). In-session auto-wiring inside `Ssu2RuntimeService`
sessions (automatic RelayIntro/HolePunch emission from session events)
is deferred to Plan 161; this plan proves carriage at the sealed-packet
layer plus coordination through the dedicated runtime service.
Deliberate non-changes (no plan deviation): the loopback dial gate and
daemon fail-closed activation stay until Plan 161 owns its evidence,
per plan §§7/13/15.15.

## Interpretation notes (not stop conditions)

1. The runtime answers path challenges even from unvalidated
   candidates (Plan 159 note, retained): the challenge arrived inside
   an authenticated session packet, so the responder is the peer, not
   an amplifier victim.
2. PeerTest sender-family gating compares the datagram source family
   to the tested family (v4 evidence never satisfies a v6 test); the
   session binding itself is the runtime's job before table ingest.
3. Out-of-session Msg 5–7 trial verification tries registered keys in
   hash order with trial-commit isolation: a wrong key never mutates
   the peer's test, so concurrent tests stay isolated even under key
   confusion.
4. The `seen` replay set in `RelayTarget` clears on overflow (512
   entries): bounded state is preserved at the cost of at most one
   extra HolePunch per replayed nonce after the clear, never unbounded
   growth.
5. `endpoint_parts` in `relay.rs` spells the IPv6 `asz` as `16 + 2`
   for review clarity; it equals the spec 18.

No stop condition fired: relay signatures reconciled verbatim from the
refreshed spec on the first pass, relay success transitions into the
normal handshake (proven by the live-dial leg of the product test),
NAT-like tests needed only loopback forwarding (no namespaces), and
abuse tests show flat state under floods (40 datagrams, 8 admitted,
zero tests created).

## Validation record

Local validation on the closing tree (all green):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
  (1515 passed, 59 suites, 0 failed)
cargo test --locked -p i2pr-transport --all-targets (44 passed, 2 suites)
cargo test --locked -p i2pr-transport-ssu2 --all-targets (163 passed, 5 suites)
cargo test --locked -p i2pr-runtime --all-targets (86 passed, 3 suites)
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay (7 passed)
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

Deltas from the Plan 159 baseline (1469 passed, 57 suites): +23
transport-ssu2 lib tests (12 peer-test + 7 relay + 4 introducer), +8
sealed-packet peer/relay trajectories (new suite), +1 transport test
(typed-outcome neutral-state), +7 runtime lib tests (peer-relay
service), +7 real-UDP NAT-like tests (new suite).

Hosted lanes: routine CI plus the manual SAM external workflow must
pass on the exact closing commit; the run IDs are recorded in the
handoff commit message and below once available.

Hosted lanes:

- Closing commit `<pending>` — routine CI run `<pending>`: conclusion
  `<pending>`.
- The manual SAM external lane was not rerun: this plan touches no
  SAM/client/product code paths (SSU2 peer-test/relay protocol and
  policy modules, SSU2 session additive APIs, runtime peer-relay
  service only), and the Plan 151 evidence-integrity checker plus the
  full SAM suites pass locally on this tree.

## Acceptance criteria (all true, plan §15)

1. PeerTest roles/state/correlation are explicit and bounded (typed
   `PeerTestRole`/`PeerTestState`/nonce keys; 8-global/2-per-peer
   quotas; unit + sealed + real-UDP isolation tests).
2. Concurrent tests cannot consume/corrupt one another's state (nonce
   correlation, per-state expected-message gates, crossing-schedule
   tests at all three layers).
3. Signatures/freshness/role/endpoint checks are enforced (exact spec
   preimages, ±120 s skew, sender/family gates; invalid/stale/
   wrong-role/unknown bounded without evidence).
4. Direct/firewalled/inconclusive/mismatch outcomes are typed and
   policy-safe (five-variant `PeerTestOutcome`; Msg 4 alone and
   unsigned Msgs 5–7 never confirm; mismatch never last-wins).
5. No single unauthenticated observation can confirm/publicize
   reachability (table emits only after auth gates; policy floor of
   two corroborating classes; single-observation tests).
6. Relay requester/introducer/target state machines are implemented
   for required v2 roles (bounded tables with tag/nonce correlation
   and replay idempotency).
7. Relay success transitions into the normal SSU2 handshake, not a
   special fake link (product test ends with a live
   `Ssu2RuntimeService` dial + bidirectional I2NP).
8. Relay/peer-test responses obey anti-amplification and quota rules
   (3x budgets before crypto, 8/sec per-source rate limits, per-peer/
   global ceilings, quota-release on timeout/cancel/close tests).
9. Exact-capacity/max+1 tests exist for active tests/relays/tags/rate
   state (peer-test global/per-peer, relay global/tags, rate windows).
10. Deterministic real-UDP NAT-like topology proves the successful
    PeerTest and relay trajectories (7-test suite, mapper-rewritten
    sources, byte-equal socket assertions, no direct wire-byte calls).
11. Invalid signatures/stale/replays/unknown tags are bounded and do
    not leak state (negative matrices at all layers; flood creates
    zero tests).
12. Validated introducers feed the Plan 159 publication builder and
    expire cleanly (conversion + opt-in gating + expiry-withdrawal
    tests).
13. Introducer service remains disabled by default (config default
    `false`; daemon rejects `true`; service-level refusal tests).
14. Cancellation/shutdown returns all test/relay/tag resources to
    baseline (cancel/poll/shutdown tests + snapshot assertions).
15. Privacy/logging regression is green (redacted `Debug`/snapshots at
    all layers; sealed + runtime regression tests).
16. No public-network/independent-router claim is made
    (`advertised = false`; daemon/loopback gates unchanged).
17. Full workspace/SSU2 quality floor passes (see validation record).
18. This record advances only to Plan 161.

## Handoff

Plan 160 is closed. Execute Plan **161** under the Plan 154 roadmap
authority. Plan 161 owns independent IPv4 interop (exact-pinned i2pd
2.61.0 both directions over real localhost UDP with authenticated
I2NP exchange; Java I2P 2.13.0 preferred secondary) and the deferred
surfaces noted above (multi-algorithm signatures, RouterInfo
signing-key plumbing, in-session auto-wiring).
