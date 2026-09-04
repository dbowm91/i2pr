# Plan 158 status — Milestone 8 SSU2 UDP runtime and local session product

Status: **`passed-m8-ssu2-udp-runtime-and-local-session-product`**.

Registered: **2026-09-04**. Closed: **2026-09-04**.

Plan of record:
[`plans/158-m8-ssu2-udp-runtime-and-local-session-product.md`](158-m8-ssu2-udp-runtime-and-local-session-product.md).

## Current authority

```text
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product

milestone8_protocol = ssu2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_implementation = plan158-udp-runtime-landed

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 159
next_product_layer = milestone8-ssu2-v2
```

## What this pass did

1. Added the `[ssu2]` daemon/runtime configuration surface (plan
   §3) in `crates/i2pr-daemon/src/config.rs`: `enabled = false` by
   default, loopback-only `bind_ipv4`/`bind_ipv6` (empty disables a
   family), port `0` for ephemeral tests with a `>= 1024` normal
   range otherwise, `advertise = false` and
   `introducer_service = false` rejected fail-closed, conservative
   resource/deadline defaults with hard ceilings
   (pending/handshakes/active/per-IP/per-subnet/staging/inbound
   bounds, handshake/idle/scheduler-poll windows), and strict
   unknown-field handling. `enabled = true` is rejected fail-closed
   with the NTCP2 precedent reason: production activation needs the
   Plan 159 identity/publication plumbing, and accepting it while the
   daemon cannot construct the service would silently misconfigure
   the router. Token-table, reassembly, and resend-schedule policy
   stay pinned in `i2pr-transport-ssu2::constants` (no knob
   explosion). Seven config tests pin defaults, rejections,
   ceilings, and scope ordering.
2. Implemented the runtime UDP owner (plan §4–§11) in the new
   `crates/i2pr-runtime/src/ssu2_runtime.rs` (~4100 lines with unit
   tests): `Ssu2RuntimeService` (validated construction, no socket)
   plus `start()` binding loopback UDP sockets with one loop task per
   family under the caller-owned `ChildScope`. No Tokio, socket,
   timer, or task entered `i2pr-transport-ssu2` (boundary scripts
   green); the new runtime→protocol/crypto/proto edges are recorded
   in `scripts/check-dependency-direction.sh` as the approved Plan
   158 UDP-ownership boundary.
3. Implemented cheap receive classification (plan §5): datagram
   length → side-effect-free `matches_inbound` trial over active
   sessions → pending-outbound routing by source under
   single-flight-per-address dials → pending SessionConfirmed trial
   (empty action vector counts as fragment-accepted) → intro-key
   TokenRequest/SessionRequest prevalidation → admission. Retry
   answers respect the 3× amplification budget; per-source
   TokenRequest answers are rate-limited (8/s, 1024 sources).
   Tokenless Retries stay stateless (scratch responder, no DH, real
   ephemeral); only admitted token-bearing SessionRequests create
   bounded pending entries. Privacy-safe counters separate cheap
   drops, authentication failures, token/replay rejections, and
   protocol drops; no payload, key, token, or endpoint history is
   logged.
4. Implemented OS randomness/time fulfillment (plan §6):
   `OsRng`-generated ephemeral secrets, connection IDs, packet
   numbers, token/padding bytes, and `X25519PrivateKey` material;
   monotonic service-clock deadlines plus wall-clock seconds only
   for protocol timestamps. Per-handshake static secrets are rebuilt
   from stored bytes (`X25519PrivateKey` is non-`Clone`); the stored
   bytes are zeroized on service drop. Tests use the same production
   OS randomness against real sockets (no seeded bypass).
5. Implemented the handshake registry and admission (plan §7):
   global/per-IP/per-subnet pending caps, peer-agnostic
   `PendingHandshakes` leases from shared `TransportResources` for
   inbound (peer unknown until SessionConfirmed), peer-bound
   `PendingHandshake` leases for outbound, duplicate-Request
   resends without new admission or DH, absolute backstop
   deadlines, and exact-once lease release on every terminal path.
   Backoff clears only at the authenticated gate.
6. Integrated the generic transport manager (plan §8): promotion
   registers `TransportKind::Ssu2` through `duplicate_resolution`
   with hash-ordered convergence (simultaneous inbound/outbound
   pairs keep the same session on both sides), active-link
   accounting includes SSU2, outbound delivery admits through
   `delivery_capability` + `enqueue_on_link` before entering the
   bounded session queue, and close removes the exact link
   (`CloseOutcome::Stale` never disturbs a replacement). No
   SSU2-only manager and no generic-contract change were needed;
   the only transport-crate addition is the read-only
   `PeerId::hash()` accessor for backoff/peer binding (redacted
   diagnostics unchanged).
7. Implemented the outbound dial/session API (plan §9):
   `Ssu2DialTarget::new` validates port/IP/loopback/peer-hash
   before any socket activity; `dial_ssu2` checks backoff, acquires
   pending admission, follows TokenRequest/Retry or cached-token
   paths, authenticates, registers only at the gate, and returns
   typed `Ssu2DialOutcome`s with no task per dial (retransmits are
   scheduler-driven while the dial future waits one-shot).
8. Implemented the inbound I2NP handoff (plan §10): authenticated
   complete messages leave through the bounded
   `Ssu2InboundI2np` channel to caller-owned receivers; nothing
   delivers into NetDB/tunnel/client code.
9. Implemented the central scheduler (plan §11): each loop task
   recomputes one Tokio sleep to the earliest handshake/ACK/RTO/
   idle/Confirmed-resend deadline; `Ssu2Session::poll` /
   `poll_transmit` / `next_deadline_ms` drive everything else.
10. Added the minimal protocol-crate enablers, all additive and
    runtime-neutral: `Ssu2Session::queue_new_token` (single-shot
    in-band future-handshake token, mirroring the path-control
    pattern, with a round-trip unit test),
    `Ssu2Session::matches_inbound` (side-effect-free short-header
    trial match, pinned side-effect-free by unit test), and
    `Ssu2Session::outbound_pending` (read-only queue-depth
    estimate for send admission). No handshake, KDF, ACK, replay,
    congestion, fragmentation, or termination semantic changed.
11. Added the real-UDP acceptance suite (plan §12):
    `crates/i2pr-runtime/tests/ssu2_local.rs` (9 tests, two
    independent services per test, real ephemeral loopback sockets,
    no private byte-moving helpers): tokenless establishment with
    manager-registration proof; cached-token establishment with
    one-use consumption plus rotate-then-fallback recovery inside
    one dial timeout; bidirectional multi-I2NP exchange with
    fragmentation; DATA-loss, ACK-loss, reorder, and duplicate
    recovery with exact once-delivery; malformed/random/spoof burst
    boundedness with post-attack health; exact admission-ceiling
    denial; graceful close, abrupt-peer idle cleanup, and
    cancel-with-state teardown to zero tables/tasks/links.
12. Found and fixed one real runtime defect with the suite (no plan
    stop: the defect lived in the new Plan 158 scheduler, not in
    Plan 156/157 protocol): resend arms were min-merged with stale
    past values, so a due timer refired instantly and burned the
    handshake to `RetriesExhausted` in milliseconds
    (`ob_timeout now=1825 next=1824` trace). Each arm batch now
    replaces the pending deadline. The lesson is recorded in both
    skill copies and the runtime deep-dive.
13. Documented (plan file list): new `ssu2_runtime.rs` module with
    crate-root re-exports; `[ssu2]` daemon config; extended
    `check-dependency-direction.sh` allowlist (runtime boundary
    script unchanged — still green unmodified); rewrote the
    runtime deep-dive SSU2 sections; updated the SSU2 deep-dive,
    overview, dependency graph, daemon deep-dive, both skill
    copies, `README.md`, `AGENTS.md`, `plans/README.md`,
    `specs/support.toml` (new `ssu2.v2-udp-runtime` surface,
    experimental, `advertised = false`), and this status record.
    `Cargo.lock` gained only dependency-edge lines (no new
    packages); `cargo deny` passes.

## Non-goals kept

No public RouterInfo advertisement, no publication/reachability
policy, no endpoint migration, no PeerTest/relay roles, no daemon
startup wiring (`enabled = true` fails closed), no external
interop. `advertised = false` in the support ledger. Plan 159 owns
publication/reachability policy.

## Interpretation notes (not stop conditions)

1. Dial targets must be loopback literals while non-loopback SSU2
   operation is unsupported; Plan 159 relaxes this with its
   reachability policy.
2. Outbound dials are single-flight per destination address while a
   handshake is pending; concurrent same-address dials are denied
   (the generic duplicate policy additionally keeps one link per
   peer/direction). Multi-session tests therefore use distinct
   peers.
3. The manager delivery contract is the per-message admission gate;
   the session outbound queue is the transport buffer; per-session
   send staging is capped at 64 messages / 256 KiB estimated bytes
   (fixed local policy, documented at the constants).
4. The test-only fault policy (`Ssu2TestFaults`) defaults to off,
   arms relative to the next transmission, and is never touched by
   production composition; the daemon has no path that sets it.
5. Simultaneous inbound/outbound promotions converge through the
   generic hash-ordered duplicate resolution; both sides keep the
   same session without SSU2-specific ownership.

No stop condition fired: no `TransportManager` bypass was needed,
no task/timer-per-packet scheduling was needed, no Plan 156/157
protocol defect surfaced (the one scheduling defect was owned and
fixed inside Plan 158), and IPv4/IPv6 share the service through
explicit per-family sockets and staging queues.

## Validation record

Local validation on the closing tree (all green):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
  (1411 passed, 56 suites, 0 failed; Plan 157 baseline was 1389 passed,
  55 suites; delta is +22: 2 session unit tests, 7 daemon config tests,
  4 runtime unit tests, 9 real-UDP ssu2_local tests)
cargo test --locked -p i2pr-transport-ssu2 --all-targets (103 passed, 3 suites)
cargo test --locked -p i2pr-transport --all-targets (11 passed)
cargo test --locked -p i2pr-runtime --all-targets (60 lib passed)
cargo test --locked -p i2pr-runtime --test ssu2_local (9 passed; also green
  with default parallel scheduling and across 3 serial reruns)
cargo test --locked -p i2pr-daemon --lib (91 passed, incl. 44 config tests)
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

Hosted lanes: routine CI plus the manual SAM external workflow must
pass on the exact closing commit; the run IDs are recorded in the
handoff commit message and below once available.

Hosted lanes:

- Closing commit `1a5d392` — routine CI run `33850018749` (push,
  `main`,
  `1a5d392167eb1d0c1b685ab9a0c635b856563401`): conclusion `success`.
  Dependency policy, MSRV, Quality ubuntu, and Quality macos all
  green on the first attempt — no runner-specific defect (in
  particular, the new `ssu2_local` real-UDP suite passed on both
  operating systems).
- The manual SAM external lane was not rerun: this plan touches no
  SAM/client/product code paths (runtime SSU2 service, SSU2 session
  additive APIs, daemon `[ssu2]` config surface only), and the Plan
  151 evidence-integrity checker plus the full SAM suites pass
  locally on this tree.

## Acceptance criteria (all true, plan §16)

1. All production UDP sockets are owned by `i2pr-runtime`
   (`Ssu2RuntimeService::start`; boundary scripts green).
2. `i2pr-transport-ssu2` still has no Tokio/socket dependency
   (runtime-boundary script green; additive sync-only APIs).
3. `[ssu2]` is disabled by default and validated strictly
   (7 config tests; `enabled`/`advertise`/`introducer_service`
   fail closed).
4. The receive loop cheaply rejects random/impossible traffic
   before session allocation/crypto (`malformed_and_random_traffic`
   test: zero state growth, cheap-drop counters move, service
   stays healthy).
5. Pending handshakes and active sessions are resource-charged and
   bounded per source/global (admission unit tests plus the exact
   active-ceiling denial test).
6. Successful authentication atomically promotes through the
   generic `TransportManager` as `TransportKind::Ssu2`
   (tokenless test asserts the authenticated manager link; no
   await sits between admission and registration).
7. Outbound delivery uses the existing transport-neutral I2NP
   delivery contract (`delivery_capability` + `enqueue_on_link`
   gate every `send_i2np`).
8. One central bounded scheduler owns handshake/ACK/loss/idle/
   reassembly deadlines (single recomputed sleep per loop;
   `wake_in` honors the poll-max bound).
9. No task/timer-per-packet architecture exists (loop-per-socket
   only; dials wait one-shot; boundary script green).
10. Real localhost UDP tokenless establishment passes
    (`tokenless_establishment_over_real_udp`).
11. Legitimate cached-token/reuse semantics are proven, including
    consumed-token accounting and stale-token fallback
    (`cached_token_establishment_with_stale_recovery`).
12. Real localhost UDP bidirectional multi-I2NP exchange passes,
    including fragmentation
    (`bidirectional_i2np_exchange_with_fragmentation`).
13. Real-socket fault tests recover from loss/reorder/duplicate
    within explicit bounds (`data_loss_recovers...`,
    `ack_loss_reorder_and_duplicate_recover_exactly_once`).
14. Malformed/spoof/random datagrams do not create unbounded state
    (same malformed test: pending/active/token tables unchanged).
15. Cancellation/abrupt close returns tasks/resources/session
    tables to baseline (`graceful_close_abrupt_peer_and_cancel...`
    plus per-test scope joins with `joined() >= 1`).
16. No SSU2 public RouterInfo advertisement/reachability claim is
    introduced (`advertise = false` in config, ledger, and docs).
17. Full workspace/SSU2 boundary/vector floor passes (see
    validation record).
18. This record advances only to Plan 159.

## Handoff

Plan 158 is closed. Execute Plans **159 → 160 → 161** in order
under the Plan 154 roadmap authority. Plan 159 owns authenticated
path validation and migration, conservative reachability/address
publication (including relaxing the loopback dial gate and wiring
daemon activation behind explicit configuration), IPv4/IPv6
structural separation at the service layer, and deterministic
NTCP2/SSU2 selection/fallback.
