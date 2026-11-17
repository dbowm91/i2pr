# Plan 487 — reverse-leg DATA recovery, receiver-ACK contract correction, and §6 lifecycle proof

> **Closed 2026-10-20.** The authoritative outcome is
> `plans/closure/core-router-recovery/487-status.md` (status `passed`). This
> document is the plan **as registered**; the body below is left as written and
> is not the current state. Plan 478 — the parent, which this plan's
> "Relationship to Plan 478" section calls *active* — was closed with this
> plan (see `plans/closure/core-router-recovery/478-status.md`, 2026-10-20
> entry). The lane is
> `tests/integration/core-router-recovery/run_plan487_lifecycle_recovery.py`
> and its evidence file is
> `target/interop/plan487-lifecycle-evidence/result.json`.

**Status:** passed (closure `plans/closure/core-router-recovery/487-status.md`, 2026-10-20; lane `tests/integration/core-router-recovery/run_plan487_lifecycle_recovery.py`, evidence `target/interop/plan487-lifecycle-evidence/result.json`). **Owner:** `i2pr-client` streaming manager (`crates/i2pr-client/src/streaming/manager.rs`), daemon SAM dispatch/bridge seam (`crates/i2pr-daemon/src/sam.rs`, `crates/i2pr-daemon/src/sam/streams.rs`), daemon service-product publication path (`crates/i2pr-daemon/src/service_product.rs`), daemon destination-tunnel coordinator (`crates/i2pr-daemon/src/destination_tunnels.rs`). **Baseline:** `work/plans-477-480-router-recovery@f7403432`. **Authoritative predecessor:** `plans/closure/core-router-recovery/478-status.md` (2026-10-19 entry). **Research input:** `plans/diagnostics/2026-10-19-external-reference-streaming-ack-and-lease-lifecycle.md`. **Predecessor lineage:** 478 (parent, now closed with this plan) → 485 and 486 (closed correctives) → this plan.

**No new ADR is required for the source work.** The receiver-ACK contract correction in WP1 aligns an existing implementation with the reference behaviour already fixed by the Streaming specification and the pinned references; it introduces no new architecture decision. WP4's lifecycle semantics **do** require the open decision at `specs/protocols/07-streaming.md:132` to be settled in writing before its lane can be judged — see WP4 and "Stop conditions".

**Relationship to Plan 478 (important).** 478 is the parent milestone and remains **active**; this plan does not supersede it and does not renumber it. 487 carries the three gates 478 could not close, and 478 closes *after* 487 passes. The repo's own plan-of-record rule ("routine defect remediation within an active coherent pass stays inside the same plan") is deliberately **not** stretched here: the work below is not routine remediation, it is a new coherent ownership pass with three production changes, one new lane contract, one dependency review, and a new floor-red repair that 478's own change `5b874e86` introduced. This is the same shape as the 485/486 correctives that 478 itself commissioned and whose closure 478 now absorbs.

---

## Objective

One bounded outcome: **make the reverse external SAM STREAM leg carry a digest-matched payload, and make Plan 478 closable** — that is, close the reverse DATA phase, correct the receiver-ACK contract so it matches the reference implementations, produce the §6 live lifecycle-recovery proof, and repair the two floor reds that currently block any close-out on this line.

## Why ready

- **Closed hard dependencies.** Plan 477 (writer-receipt consumption, strict terminal reply attribution) is closed. Plan 481 (producer-side source-confirmed writer receipts) is closed and consumed. Plan 482 (dial-intent/authority contract) is closed and consumed. Plan 474 (transactional owner+bridge generation) is closed. Plan 485 (control-socket-gated session teardown) and Plan 486 (waiter-scoped inbound-SYN routing) are closed and are the direct enablers of the reverse handshake. Plan 480's observer/receipt-export infrastructure is closed.
- **Stable interface dependencies.** The `StreamingManager` / `StreamingDestinationAdapter` pair is the authoritative stream implementation and is not being replaced. The `DestinationTunnelCoordinator` publication API (`begin_ls2_publication` / `cancel_ls2_publication` / `retry_ls2_publication`) is stable. The daemon's `ServiceTunnelManager` inbound-arrival mirror (`inbound_arrival_totals`) and the SAM below-bridge reader already exist and are the instrumentation substrate.
- **The diagnostic frontier is narrowed.** 486 proved the reverse handshake live (lane `q3w7m9z2`: zero ACCEPT expiries, pump started, stock SYN answered). The research report narrows the remaining failure to three separable candidates and identifies a fourth, independent defect.
- **Not ready for close-out.** Two floor reds are open (WP6) and the §6 governing semantics are an unresolved open decision (WP4). Both are named here rather than discovered later.

## Current implementation evidence

- Forward external SAM STREAM is **live-proven** against exact-pinned i2pd 2.61.0 (digest-matched, `zgcd_jsn` and `q3w7m9z2`).
- The reverse **handshake** is live-proven (Plan 486). The reverse **DATA** phase is not: stock logs `packet was not ACKed after 10 attempts, terminate` on established-phase stream ids, and the driver leg fails at `short stream read (0/2328)` after ~90 s. The data-phase close is the all-zero torn-down shape with `bridge_present=0`.
- The reference receiver-ACK contract, read at both exact pins, is **transport-state based, never application-drain based**: i2pd schedules the ACK off the RTT when a packet is received and still ACKs when it *drops* an overflow packet (`libi2pd/Streaming.cpp:255-299`, `:256-267`); Java schedules it when `messageReceived` reports a new in-sequence packet and calls `ackImmediately()` on inbound-buffer overflow (`ConnectionPacketHandler.java:180-236`, `:150-160`).
- `i2pr` **diverges** from that contract: `manager.rs:807-809` (`delivered_over_cap`), `:1648` (immediate ACK suppressed), `:1799-1804` (piggyback replaced by `FLAG_NO_ACK`), `:2189-2192` (standalone ACK snoozed) all condition acknowledgement emission on undrained application bytes. This is **not** the cause of the current stall (the failing transfer is one 2328-byte payload against a 128-packet cap), but it is a real protocol-visible defect that will stall any larger transfer against a stock peer.
- The floor is red in two places: `bash scripts/check-els2-live-lane-evidence.sh` → `FAIL: Plan 393: compose, request-construction, and delivery failures must cancel publication intent` (the checker requires ≥3 `.cancel_ls2_publication(request_id)` occurrences in `crates/i2pr-daemon/src/service_product.rs`; the tree has 1, at `:7930-7933`); and `cargo deny check advisories` → FAILED on rustls 0.23.45 via tokio-rustls 0.26.6 (RUSTSEC-2026-0346, -0347, -0348).

---

## Invariants that must not regress

1. **Loopback-only, disabled-by-default, non-advertised.** SAM, I2CP, service tunnels and the console keep their current posture. No listener, bind, or advertisement change.
2. **Exact pins unchanged.** i2pd `2.61.0` = `635b013a612ff47278ef02acf8580a28e10e26c5`; Java I2P `2.13.0` = `9134f808337b401e8e53c73734c81fab04280c9d`. Never patch or vendor a reference router or client.
3. **No capability, version, RouterInfo, SAM, or I2CP behaviour advertisement beyond the tested subset** in `specs/CONFORMANCE.md`. `specs/support.toml` is not modified by this plan. Plan 433/439 gates are untouched.
4. **Bounded everything.** No unbounded channel, queue, buffer, or counter. The delivered-byte cap, the receive window, the backlog cap, the session cap, and the publication-capacity cap all keep their current bounds.
5. **No secret material in logs, receipts, or evidence.** Counts and sanitized digests only; no identities, tunnel ids, keys, tokens, or raw payloads.
6. **Dependency direction preserved.** No new crate edge; `i2pr-client` stays free of `i2pr-api`; the runtime stays in `i2pr-runtime`/`i2pr-daemon`.
7. **No boundary script is weakened.** Every checker is a gate, not a dial. A checker that fails must be fixed in code, never relaxed in the script.
8. **No unchanged-topology retry.** Every external lane run must be licensed by a material change recorded in this plan.
9. **M6 Streaming semantics are not weakened for SAM tests**, and no SAM or lane accommodation may change the wire to go green.
10. **Fail-closed on every new path.** No distinct remotely observable response per cause; rejection stays silent.

---

## Scope

### In scope

- The reverse-leg DATA-phase causal fix (WP3) and the instrumentation that discriminates it (WP2).
- The receiver-ACK contract correction (WP1).
- The §6 live lifecycle-recovery proof and the lifecycle semantics it depends on (WP4).
- The ELS2 Plan 393 publication-intent cancellation repair introduced by this line's own change `5b874e86` (WP5).
- The rustls security bump review and lockfile update (WP6).
- The 478 close-out mechanics that follow from the above (WP7).

### Explicitly out of scope

- Public I2P participation, default-network advertisement, transit, floodfill, or any broad capability posture.
- Java full-router compatibility (retained nonblocking debt under ADR 0026 / Plan 247). No Java result is relabeled.
- NTCP2, SSU2, I2CP, Proposal 170 outproxy, ELS2 type-5 semantics, and the router console.
- The Emissary router as a peer or a dependency: it has **no** Streaming/ministreaming module and cannot carry a STREAM leg; it is referenced only as a design reference for the lease publish-and-verify loop (WP4), and must not be vendored, patched, or built.
- The `plans/diagnostics/` research report itself: it is an input, not a work item, and is not conformance evidence.
- Any second lane motivated by rerun appetite rather than a new material change.

---

## Required production changes

| # | Change | Site |
|---|---|---|
| C1 | Decouple acknowledgement emission from local application drain; express backpressure in the reference's own vocabulary (bounded reorder buffer, receive-window limit, and the congestion/choking delay option) instead of withholding ACKs. | `crates/i2pr-client/src/streaming/manager.rs` |
| C2 | Add a four-stage DATA-phase attribution surface: inbound-gateway arrival, streaming-manager arrival, the ACK the manager generated (its `ackThrough` and whether it carried `NO_ACK`), and the ACK's egress send. Counts only. | `crates/i2pr-daemon/src/service_tunnels.rs`, `crates/i2pr-daemon/src/sam.rs`, `crates/i2pr-client/src/streaming/manager.rs` |
| C3 | The fix selected by WP3's classification: either the inbound gateway → manager hop, the accepted connection's ACK egress, or the SAM bridge's `drain_delivered()` wiring for an accepted stream. Exactly one of these, as the lane decides. | decided by WP3 |
| C4 | Make the lease publication intent cover the compose and request-construction phases, and cancel it on each of the three failure classes. | `crates/i2pr-daemon/src/service_product.rs`, `crates/i2pr-daemon/src/destination_tunnels.rs` |
| C5 | Lifecycle semantics for stream persistence across a destination tunnel-pool rebuild and a daemon restart, written down and implemented where it is currently undefined. | `specs/protocols/07-streaming.md`, then the owning crates |
| C6 | rustls `0.23.45` → `>= 0.23.46` in `Cargo.lock`. | `Cargo.lock` only |

---

## Ordered work packages

### WP1 — Correct the receiver-ACK contract (fail-before first)

1. **Lock the defect with a fail-before deterministic test.** In `crates/i2pr-client/src/streaming/manager.rs`, construct a connection, deliver a full receive window of maximum-size packets so `delivered_over_cap` is true, and assert that an acknowledgement is **still** emitted for the received data. The test must fail on the current tree (`poll_acks` returns empty while over cap; `manager.rs:2189-2192`) and pass after WP1.2. Probe the assertion by inverting it, and by deleting the new emission path, and revert both probes.
2. **Replace the drain-coupled gate with a protocol-shaped bound.** The acknowledgement must be emitted on the reference's schedule (received-sequence driven, RTT-derived delay, immediate on an immediate-ACK request). Memory must stay bounded: the *reorder buffer* and the *receive window* are the bounds, and the peer is kept from over-running by the reference's own congestion/choking mechanism (the `PACKET_FLAG_DELAY_REQUESTED` / `DELAY_CHOKING*` vocabulary) rather than by withholding the ACK. The `NO_ACK` piggyback substitution at `manager.rs:1799-1804` is removed for the same reason: it is the same drain coupling on the send side. Keep `delivered_cap_bytes` and the delivered-byte accounting as an *application-visible* backpressure signal (the runtime may stop reading), not as an acknowledgement gate.
3. **Regression matrix.** Ack-emitted-while-app-has-not-drained; ack-not-emitted-for-an-ack-only-packet (no ACK-of-ACK loop); duplicate/old-sequence packets do not schedule a second acknowledgement; the immediate-ACK request path is honoured; NACK list stays bounded; the delayed-ACK deadline still coalesces; the whole of `crates/i2pr-client`'s existing streaming suite stays green. Any test that depended on the old drain-coupled behaviour is updated to assert the *reference* behaviour, and the change is recorded in the closure — it is a deliberate protocol-conformance change, not a test relaxation.
4. **Re-verify against the references.** Re-read the pinned i2pd and Java sources for the paths this change touches and confirm the new behaviour matches; cite the exact lines in the closure. The research report's §1 is the starting point, not the authority — the authority is the pinned source at the pinned commit.

### WP2 — Four-stage DATA-phase attribution (the material change licensing the lane)

5. **Extend the existing mirrors, do not build a parallel one.** `ServiceTunnelManager::inbound_arrival_totals` (`service_tunnels.rs:3604`) already returns `(tunnel_data_received, cells_accepted, owner_matches, owner_misses, direct_garlic_received)` and the SAM below-bridge reader (`sam.rs:3865+`) already consumes it. Add the two missing stages as **counts only**:
   - **manager arrival** — a counter incremented when an inbound DATA-bearing cell is dispatched to the streaming manager, distinct from `cells_accepted` (which stops at the bridge);
   - **ACK generated** — a counter of acknowledgements the manager produced for a received DATA sequence, plus a count of those that carried `NO_ACK` (must be zero after WP1);
   - **ACK egress sent** — a counter of those acknowledgements that reached the transport send path, with the send outcome (sent / dropped-before-send) kept as two distinct counts.
6. **Keep the receipt shape honest.** The receipt must be a *per-stage* split, never a summed tuple: Plan 486's lesson was that a summed shape concealed a total instance miss for a full turn. The lane parser must refuse a summed-only legacy shape. Extend the offline receipt tests and probe-kill by dropping one stage's columns.
7. **No payload, identity, tunnel id, key, token, or raw byte in the receipt.** Counts and the digest of the compared payload only.

### WP3 — One discriminating lane, then the fix, then the re-run

8. **One bounded, exact-pin lane** (`tests/integration/core-router-recovery/run_plan458_sam_product.py`, provenance-verified fresh binary, `I2PD_PIN` unchanged, attempt budget unchanged). The required reading order:
   1. forward leg digest-matched (the guard — a red forward leg is not a DATA verdict);
   2. reverse ACCEPT parks, SYN answered, pump started (the 486 baseline must still hold);
   3. **the four attribution stages**, read in order;
   4. the DATA decision table;
   5. the teardown rows.
9. **Classify from the stage split, before touching source.** Exactly one of:
   - **(a) arrival loss** — stage 1 false: the DATA never reached the inbound gateway;
   - **(b) egress loss** — stages 1–3 true, stage 4 false: the ACK was generated and never sent;
   - **(c) drain path** — stages 1–2 true, stage 3 false, and the manager's delivered-byte counter for the accepted connection is non-zero while the SAM socket shows no bytes: the data was delivered to `pending_delivered` and never drained to the application.
   A lane that does not fit (a), (b) or (c) is **not** a verdict. Record it as a new classified outcome; do not guess.
10. **Apply C3 for the classified cause, and only that cause.** Re-derive the fix from the pinned reference behaviour for the touched path. Add the deterministic regression that fails before the fix and passes after it; probe it.
11. **Re-run the lane once** after the fix. A second lane is licensed only by a further material change, never by rerun appetite.
12. **The gate closes on a digest-matched payload across the reverse leg** — never on a counter, a counter combination, or a "no error observed" shape.

### WP4 — §6 live lifecycle recovery

13. **Settle the open decision first.** `specs/protocols/07-streaming.md:132` lists "Stream persistence/recovery semantics across destination tunnel-pool rebuilds and router restart" as an **open decision**. Write the intended semantics down before the lane: does a stream survive a destination tunnel-pool rebuild; does a SAM session survive a daemon restart; what is the client-observable result in each case. This is a documentation-first step and it is a **stop condition** if it is skipped — a live recovery lane judged against undefined semantics proves nothing. The decision is recorded in the spec dossier and, where it changes behaviour, in an ADR.
14. **Prove the lease publish-and-acknowledge loop.** Using the reference lifecycle (i2p.net network-database documentation and Proposal 123): a LeaseSet2 carries a `published` timestamp and an `expires` offset; the maximum actual `expires` for a LeaseSet2 is about 660 s (11 minutes) and tunnels live 10 minutes; a publication is confirmed by a `DeliveryStatus` whose message id is the publish's Reply Token, and that `DeliveryStatus` arrives on one of the client's **inbound** tunnels. The proof must show: a rotation republishes with `published` at least one second later than the previous one and the oldest lease's expiration incremented by at least 1 ms (otherwise the floodfill silently keeps the old record); the publish is rate-limited to less than once per second per destination; and the correlated `DeliveryStatus` is observed arriving on an inbound tunnel. `LeaseSetLifecycle` (`crates/i2pr-client/src/leaseset.rs`) is the substrate — `refresh`, `refresh_client`, `evaluate_client_refresh`, the monotonic `published` timestamp and the approaching-expiry rotation are already implemented and tested. Do **not** add a parallel publish path.
15. **Use the Emissary publish-and-verify state machine as a design reference only.** `emissary-core/src/destination/lease_set.rs` models `Inactive → AwaitingLeaseSet → AwaitingTunnels → GetClosestFloodfills → PublishLeaseSet → AwaitingFlooding → VerifyStorage → Retry` with concrete timers (30 s tunnel wait, 1 s retry, 10 s / 2 s / 15 s storage verification). Adopt the *shape* of a router-owned publish→floodfill→verify→retry loop if the current coordinator lacks it. Do not vendor, build, patch, or depend on Emissary; it is experimental and not recommended for production, and it carries no Streaming implementation at all.
16. **The live recovery proof.** One tunnel expiry/replenishment **or** controlled peer loss plus daemon restart, showing the destination and session recover and the stream's client-observable behaviour matches the settled decision. Deterministic rotation/retry/replenishment-order tests stay green throughout. The current-accounted build retry must be visible in the receipt.
17. **Both directions.** The lifecycle proof must hold for the lease publication our router owns; the reverse leg's DATA path and the §6 recovery path must be exercised in the same qualification so the two are not proven against different topologies.

### WP5 — ELS2 Plan 393 publication-intent cancellation (this line's own defect)

18. **Understand the requirement before editing.** `scripts/check-els2-live-lane-evidence.py` requires ≥3 occurrences of `.cancel_ls2_publication(request_id)` in `crates/i2pr-daemon/src/service_product.rs` (the count is the checker's proxy for "compose, request-construction, and delivery failures must cancel publication intent"), and its own negative test proves the count is load-bearing: mutating a cancel into `.retry_ls2_publication(request_id)` must be detected. `begin_ls2_publication` currently takes the composed store message and the floodfill, so the intent is taken *after* compose — the two missing failure classes currently have no intent to release.
19. **Make the semantic property true, then let the count follow.** Every publication attempt must release exactly one coordinator slot on **every** failure path, and the three classes — compose, request-construction, delivery — must each be individually attributable. Implement it so the intent genuinely covers the compose and request-construction phases (for example, a bounded reservation taken before compose and completed by `begin`, or an equivalent coordinator change), with a distinct cancel site and a distinct error classification per class. **Do not reach the count by duplicating a call site**: a repeated `.cancel_ls2_publication(request_id)` with no distinct failure semantics is a fake pass and is rejected at closure.
20. **Deterministic regression.** For each of the three classes, a test that forces the failure and asserts the coordinator's publication count returns to its pre-attempt value and that the failure is separately attributable. Capacity rejection must keep its own counter (`PublicationBeginRejection::Capacity`).
21. **Checker green.** `bash scripts/check-els2-live-lane-evidence.sh` must pass on the closing tree, and the checker's own `--self-test` / negative tests must still detect the mutations. The script is not to be edited.

### WP6 — rustls advisory red (review and bump)

22. **Record the reachability analysis.** The workspace pins `rustls = { version = "0.23", default-features = false, features = ["std", "tls12", "ring"] }` and `tokio-rustls = { version = "0.26", default-features = false, features = ["tls12"] }`. RUSTSEC-2026-0348 concerns the built-in Brotli certificate decompressor, which is behind the non-default `brotli` crate feature — **not enabled here**, so it is not feature-reachable at the current feature set. RUSTSEC-2026-0346 (key-material redaction conflict with rustls's public-API policy) and RUSTSEC-2026-0347 (ECH config `public_name` disclosure) are in the enabled surface.
23. **Bump within the existing range.** `cargo update -p rustls` to `>= 0.23.46`. This is a **lockfile-only** patch bump inside an existing caret range: no manifest change, no feature change, no new crate, no new transitive dependency expected. Review and record: purpose (advisory fix), transitive impact (expected none beyond rustls and tokio-rustls's range), `unsafe` exposure (unchanged), features (unchanged), licence (unchanged, MIT/Apache-2.0 as already locked). Confirm the bump does not disturb the exact **reference** pins (i2pd, Java, go-i2cp) — those are separate and must not move.
24. **`cargo deny check advisories` green**, and the full `AGENTS.md` floor green on the closing tree, including `cargo deny check bans sources`.

### WP7 — Close-out mechanics for 478

25. When WP1–WP6 are green: write this plan's closure record, then the 478 closure record's final form, then advance 478's status token, the registry row, and the roadmap §7 rows. Run the unblock audit and record it. Nothing is promoted: no `specs/support.toml` change, no capability claim, no advertisement.

---

## Failure, cancellation, restart, and contention semantics

- **The publication intent is bounded and exactly-once.** Every attempt releases exactly one slot on every path, including panic and cancellation. A cancel on an unknown `request_id` is a typed error, never a panic.
- **The acknowledgement path is bounded.** Emission is coalesced by the delayed-ACK deadline; at most one coalesced standalone acknowledgement per connection per interval; the NACK list stays bounded; an ack-only packet never schedules an acknowledgement.
- **Cancellation of a lane or a session releases the ACK deadline, the reorder buffer, the delivered-byte accounting, and the publication slot.** No timer, queue, socket, or registration survives cancellation; this is the Plan 129/130/131 baseline the workspace already asserts and it must keep asserting it.
- **Contention.** The ACK emission path and the delivered-byte accounting are both under the manager's single owner; the publication coordinator is behind its existing mutex. Document the lock acquisition order at each touched seam and include a deadlock review in the closure, as Plan 486 required for its seam.
- **Restart.** A daemon restart must not leak publication slots or leave a stream in a state that claims recovery it did not perform. WP4's decision must state the observable restart behaviour, and the receipt must show it.

## Compatibility and migration

- **Wire compatibility:** the reference-aligned ACK behaviour is a change in *when* we acknowledge, not in what we send on the wire. The packet format, flags, options, and MTU are unchanged. The `NO_ACK` flag's use as a *piggyback substitution under drain pressure* is removed; the flag itself remains supported on receive.
- **Peer compatibility:** aligning with the reference contract makes `i2pr` interoperable with stock peers in the case where it currently diverges. It cannot break a peer that already interoperates, because a conforming peer does not depend on the ACK being withheld.
- **Local callers:** any runtime code that relied on "no ACK until drained" as a flow-control signal must be moved to the delivered-byte backpressure signal, which is retained. Audit the SAM pump and the service-tunnel pump call sites and record the audit.
- **Migration:** none. No on-disk format, no configuration surface, no I2CP/I2CP-visible change, no identity or key change.

## Required tests

- `crates/i2pr-client`: the WP1 acknowledgement-contract matrix (fail-before/pass-after, with inversion and deletion probes), plus the whole existing streaming suite (`--lib` and the `plan129/130/131` trajectory tests) green.
- `crates/i2pr-daemon`: the WP2 four-stage receipt split (per-stage, not summed; probe-kill one column), the WP5 three-failure-class capacity-release tests, the WP3 fix's own fail-before/pass-after regression, and the existing `destination_tunnel_unit`, `sam_stream_self_composed`, `sam_stream_final_acceptance` suites green.
- Lane: one exact-pin bounded lane per material change, in the WP3 reading order, with the WP4 lifecycle evidence in the same qualification.
- Checkers: `check-els2-live-lane-evidence.sh` (+ its self-test/negative tests), `check-core-router-recovery-contract.py` (+ `--self-test`), `check-sam-acceptance-evidence.sh`, `check-global-plan-number-uniqueness.py`, and the full `AGENTS.md` floor.

## Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --lib
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-console-boundaries.sh
bash scripts/check-console-browser-security.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 scripts/check-license-metadata.py
python3 scripts/check-els2-live-lane-evidence.sh
python3 scripts/check-core-router-recovery-contract.py
python3 scripts/check-core-router-recovery-contract.py --self-test
bash scripts/check-sam-acceptance-evidence.sh
cargo deny check advisories bans sources
```

Plus the full `AGENTS.md` routine floor verbatim on the closing tree, and one exact-pin bounded lane per material change. The lane must be invoked with the exact pin; an invocation without the pin fails hard and must not be reported as a pass.

## Documentation updates

- `specs/protocols/07-streaming.md` — record the settled lifecycle decision (WP4 step 13) and the receiver-ACK contract as implemented; close the open decision at line 132 if WP4 settles it.
- `specs/CONFORMANCE.md` and `specs/support.toml` — **not modified by this plan**. If a later conformance pass is warranted it needs its own plan-of-record.
- `docs/architecture/i2pr-client.md`, `docs/architecture/i2pr-daemon.md` — reflect the corrected ACK contract and the four-stage attribution surface.
- `plans/closure/core-router-recovery/487-status.md` — the closure record.
- `plans/closure/core-router-recovery/478-status.md` — the parent absorbs 487's outcome and advances its own token.
- `plans/registry.md`, `plans/subsystems/core-router-recovery-roadmap.md` — the 487 and 478 rows and the roadmap §7 disposition.

## Acceptance criteria

The plan passes when **all** of the following hold, each derived from an executed command or a lane receipt:

1. WP1's fail-before/pass-after acknowledgement test is green, its probes are reverted, and the emission behaviour matches the pinned references line-for-line in the closure's citation.
2. The reverse external SAM STREAM leg carries a **digest-matched payload**, proven in the WP3 reading order with the four attribution stages read per-stage. Attribution alone does not close it.
3. The classification (a)/(b)/(c) is recorded with the stage evidence that selected it, and C3 is applied for that cause only.
4. The §6 lifecycle decision is written down, the lease publish-and-acknowledge loop is proven live (rotation with `published` +1 s and oldest-lease expiration +1 ms, rate-limited republish, `DeliveryStatus` correlated on the Reply Token arriving on an inbound tunnel), and the recovery through one tunnel expiry/replenishment or controlled peer loss plus daemon restart is proven against the settled semantics.
5. The ELS2 Plan 393 count and its underlying three-failure-class property both hold, with no duplicated call site and the checker's negative tests still detecting their mutations.
6. `cargo deny check advisories` is green, with the reachability review and the transitive-impact review recorded.
7. The full `AGENTS.md` floor is green on the closing tree, with the seven bash-4+-requiring checkers run under bash 5 or on CI — and reported truthfully if not.
8. No capability, version, advertisement, or `specs/support.toml` change; Plan 433/439 untouched; no new crate edge; no boundary script weakened.
9. The closure record names the exact commands run and their outcomes, labels local vs CI truthfully, and lists the unrun tests and why.

## Stop conditions

Stop and record a classified outcome — do not guess, and do not close — when any of these occurs:

- The lane's four stages do not fit (a), (b) or (c). That is a new classified outcome and it needs its own analysis, not another lane.
- WP4's lifecycle decision cannot be settled in writing. The §6 lane is then **not run**; a recovery proof against undefined semantics is not evidence.
- The forward leg regresses. The DATA verdict is void; fix the forward leg first.
- The stock peer behaves differently from the pinned references for a reason attributable to the peer. That is a reference-side question and it does not license a production wire change.
- The ELS2 count can only be reached by duplicating a call site without distinct failure semantics.
- A lane would need an unchanged topology to be run again.
- Any checker's rule would have to be weakened to pass. Fix the code; never the script.

## Closure evidence required

- `plans/closure/core-router-recovery/487-status.md` with: commits; a requirement-to-evidence matrix keyed to the acceptance criteria above; the exact commands run and their outcomes, local vs CI labelled truthfully; the four-stage lane receipt read in the required order; the classification and its evidence; the pinned-reference citations for the ACK contract; the §6 lifecycle evidence; the ELS2 three-class evidence; the dependency review; invariant, failure, migration and security reviews; documentation updates; limitations; findings by severity; and the roadmap disposition.
- The parent 478 record updated with the same outcome, and the registry and roadmap rows advanced.
- The unblock audit, recorded.

## Handoff notes

- **The lane is the scarce resource.** Every external run is exact-pin, attempt-budgeted, and licensed only by a material change. Budget the runs deliberately: one after WP2, one after the WP3 fix, and one for the WP4 lifecycle qualification. Do not spend a run on a hypothesis that a deterministic test can answer.
- **Do not re-derive the reference behaviour from documentation.** Read the pinned sources. The research report is an index into them, not a substitute.
- **Do not close the reverse DATA gate on a counter.** A digest-matched payload is the gate; a counter combination is a lead.
- **Keep the three failure classes distinct** in WP5, and the four stages distinct in WP2. Both plans' predecessors were made harder by a summed receipt shape that hid a total miss.
- **If a defect surfaces mid-pass that is not one of the three classified causes,** it is repaired and retested inside this plan, with a deterministic regression — not promoted into another numbered plan, and not a reason to stop.
- **The parent stays open.** 478 closes after this plan passes, with its own record, registry transition, and unblock audit.
