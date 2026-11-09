# Plan 521 — the router's own peer test earns its own reachability

## 1. Objective

The ordinary router can run an I2NP peer test against a peer it has a session
with, and a **confirmed** exchange records the router's **own** reachability in
the transport's tracker — so the router's published SSU2 address can be earned
in production, not only inside the controlled floodfill activation.

This is Plan 520's open **Medium** finding **M1**, discharged at its source:
Plan 520 made the published address **qualified** and gated its publication on
the router's own reachability evidence, and recorded that the ordinary
record's address **cannot be earned in production today**, because the direct
form requires `Reachable` and the runtime's production recorders can supply
the three corroboration classes only inside the controlled floodfill
activation.

One bounded outcome: **a production entry point that starts a peer test and
records the confirmed outcome about this router.** Not a second reachability
model, not a second tracker, not a new advertisement.

## 2. Why ready

- Plan 520 closed the publication side and named **M1** as the next thing
  worth doing in this line
  (`plans/closure/network-participation/520-status.md`, findings by severity).
- The peer-test machinery is **complete and unwired** — the same shape Plan
  504 found for the tunnel-liveness scheduler and Plan 505 for the
  exploratory build driver. The codec, the table and the responder's echo all
  exist; only the owner and the recording are missing.
- The transport's tracker already defines the corroboration classes and the
  `Reachable` threshold, so the plan needs no new policy vocabulary.
- No external environment is needed: the plan is provable in the controlled
  loopback lane and in-tree, exactly like Plans 515–520.

The **stock-peer** half — that a **stock** peer answers the probe and that a
stock peer qualifies the router's published record — stays blocked on Plan
493's authorized two-host topology. It is not claimed here.

## 3. Current implementation evidence

> **Registration-time snapshot.** §3 was written when the plan was
> registered and it cites the production tree as it stood then. The
> implementation moved that tree; the drift is recorded in §17.5 with a
> registration-time to current pointer table, and §3 itself is **not**
> rewritten.

### 3.1 The peer test exists, in two halves, and neither is wired

- `crates/i2pr-proto/src/i2np/header.rs:61` — message **231** is
  `TunnelTest`: the classic I2P peer test. It carries a message identifier and
  a timestamp, and the responder echoes both unchanged.
- `crates/i2pr-daemon/src/peer_test.rs:87` — `PeerTestEcho::encode` already
  builds the exact answer owed by an inbound peer test: a fresh message with a
  fresh transport message identifier that repeats the probed identifier and
  timestamp unchanged.
- `crates/i2pr-daemon/src/peer_test.rs:202` —
  `dispatch_router_i2np_with_peer_test` already extracts that answer from an
  authenticated inbound delivery.
- `crates/i2pr-daemon/src/peer_test.rs:122` — `PeerTestTracker` is the bounded
  table of outstanding probes, keyed by the probed identifier, with a
  capacity ceiling (`MAX_OUTSTANDING_PEER_TESTS`, `:38`) and an age ceiling
  (`DEFAULT_PEER_TEST_TIMEOUT_MS`, `:41`).
- `crates/i2pr-daemon/src/router_i2np.rs:748` — the central dispatcher already
  classifies an inbound `TunnelTest` as `RouterI2npOutcome::PeerTest`. The
  variant (`:147`) carries **everything the answer needs**: the probed
  identifier (`peer_test_msg_id`), the probed timestamp
  (`peer_test_timestamp`), the authenticated peer and the exact link.

**Neither half has a production caller.** `PeerTestTracker`,
`PeerTestEcho::encode` and `dispatch_router_i2np_with_peer_test` are
referenced only from their own module's tests, and the module's own header
records the discipline they were built for: *"an inbound test that a peer
expects an answer to gets a bounded, honest answer derived from the
authenticated peer and link that delivered it, so an answer is never routed
from a caller-supplied identity"*, and *"a probe this router originated is
either matched or reported as timed out"*.

One thing in that module must **not** become the production path.
`dispatch_router_i2np_with_peer_test` (`:202`) calls the dispatcher and then
**decodes the same message again** in `peer_test_echo` (`:211`) to recover the
probed fields the dispatcher had already extracted. The live inbound path's
rule is that the pump decodes the inbound message **once** and classifies it
**once**, and a correlator works on that *same* outcome — the liveness
scheduler's `route_inbound_with_liveness_classified`
(`crates/i2pr-daemon/src/tunnel_liveness.rs:785`) says so in its own words:
*"never a parallel inbound path"*. The production responder therefore uses the
dispatcher's **classified** outcome and runs no second decoder.

### 3.2 The transport's three-party peer test is a different thing, and it is controlled

- `crates/i2pr-transport-ssu2/src/peer_test.rs:483` — `PeerTestTable` is the
  transport's **out-of-session** peer-test table (Alice, Bob and Charlie), with
  the signed codec `build_out_of_session_peer_test` /
  `parse_out_of_session_peer_test`.
- It is used **only in tests**: `crates/i2pr-transport-ssu2/tests/peer_relay.rs`
  and `crates/i2pr-runtime/tests/ssu2_peer_relay.rs`.
- The runtime's production entry point for it,
  `start_controlled_peer_test`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2563`), is refused unless the
  service was configured with `controlled_peer_test`, and
  `send_controlled_peer_test` (`:2762`) refuses any target that is not
  loopback.

This plan does **not** touch that surface. The I2NP peer test (231) is the
router-level mechanism the daemon already models, and it is the one a router
runs against a peer it already has a session with.

### 3.3 The tracker already defines what "reachable" needs

- `crates/i2pr-transport/src/reachability.rs:119` — `ReachabilitySignal` has
  five variants, and `:195` maps each to its own **corroboration class**: the
  configured bind (0), a peer's external-address observation (1), a validated
  path (2), a peer-test result (3) and a relay/firewalled indication (4).
- `:475` — `supporting_classes` counts **distinct classes** for the latest
  family, so one peer's repeated observation can never reach the threshold.
- `:528` — `recompute` reaches `Reachable` only when the supporting classes
  are **strictly greater** than `MIN_CORROBORATION_FLOOR` (`:66`, which is 2),
  so `Reachable` needs **three** distinct classes.
- `:136` — `ReachabilitySignal::PeerTestResult` supports reachability **only**
  for `PeerTestOutcomeKind::Confirmed`.

### 3.4 Two of the three classes are already production-reachable

- **Class 1** — `crates/i2pr-runtime/src/ssu2_runtime.rs:6097`: the ordinary
  session event loop records `AuthenticatedPeerObservedExternalAddress` when a
  peer sends an `Address` block. This is not a controlled path.
- **Class 2** — `crates/i2pr-runtime/src/ssu2_runtime.rs:5966`: the runtime
  records `ValidatedPath` when a session's path migrates.
- **Class 3** — `PeerTestResult { outcome: Confirmed }` is recorded in
  production today only by `map_controlled_outcome_locked`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3214`), which mirrors the
  **controlled** table's outcome onto the service tracker. The controlled
  activation is the only caller
  (`crates/i2pr-daemon/src/floodfill.rs:675`), and it installs the
  *floodfill* record, which Plan 520's stand-down rule leaves alone.

**So class 3 is the one the ordinary router cannot earn.** That is this plan.

### 3.5 The live inbound and outbound paths are already there

- `crates/i2pr-daemon/src/service_product.rs:5277` — the SSU2 service's live
  inbound pump routes one authenticated router-I2NP message through the
  central dispatcher and the liveness scheduler.
- `crates/i2pr-runtime/src/ssu2_runtime.rs:3746` —
  `send_i2np_with_link(peer, message, timeout)` queues one I2NP message for a
  **known** peer over its existing delivery capability. It does not select a
  peer; the caller owns the choice.

## 4. Invariants that must not regress

1. **No fabricated evidence.** The daemon records a reachability signal about
   **itself** only when it has genuinely observed the corresponding fact. A
   confirmed peer test is recorded only on a **matched** echo of an
   outstanding probe, and never on a timeout, an unmatched echo, or a
   self-assertion.
2. **The transport's tracker remains the single authority.** The daemon never
   computes a reachability state, never reads one, and never writes a state
   into a published record. It hands the runtime a *typed outcome* and the
   runtime decides.
3. **The answer is derived from the authenticated delivery.** The responder's
   echo travels back on the session that carried the test, and it repeats the
   probed identifier and timestamp unchanged. No answer is ever routed from a
   caller-supplied identity.
4. **The bounded table stays bounded.** The outstanding-probe table keeps its
   capacity ceiling and its age ceiling; a probe is refused rather than
   evicting an older one, so an answer can never be matched against a
   different probe.
5. **The publication is unchanged.** The period, the fan-out, the ladder's
   timing, the correlator, the freshness rule and the task's ownership are
   untouched, and Plan 520's owner still refreshes the record inside the
   cycle's own branch.
6. **`caps=f` stays permit-gated.** The peer test touches no builder and no
   permit.
7. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   RouterInfo's `caps` string is unchanged, the support inventory's advertised
   count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay byte-identical.
8. **The controlled floodfill path is left alone.** Its `PeerTestResult`
   recording, its permit and its activation are untouched.

## 5. Scope

### In scope

- The **responder** role: the live inbound pump answers an inbound I2NP peer
  test on the session that delivered it.
- The **originator** role: a bounded owner starts a peer test against a peer,
  tracks it, and completes it on a matched echo.
- The **recording**: the runtime's production recorder for the router's own
  confirmed peer test, distinct from the controlled one.
- The **opt-in** and the **counters** that make the behaviour observable and
  default-off.
- The lane, the guard, the documentation and the support inventory.

### Not in scope

- The transport's **out-of-session** three-party peer test, its signed codec
  and its table. They stay controlled-only.
- The **controlled floodfill** activation and everything Plan 306 and ADR 0030
  own.
- The **`caps=f`** advertisement (Plan 494, blocked on Plan 493).
- The **stock-peer** half, and any claim that a stock router qualifies the
  published record.
- The **outbound peer-session pool** the roadmap names as missing. It is a
  separate, larger piece of work; this plan needs only *a* peer with a
  session, not a pool.
- **Any change to the transport's corroboration policy.** The threshold, the
  classes and the expiry are the transport's, and this plan does not touch
  them.
- **Any change to the daemon's published record**, beyond what Plan 520
  already owns.

## 6. Required production changes

1. **`crates/i2pr-daemon/src/peer_test.rs`** — the originator's owner. A
   bounded, single-flight owner that:
   - holds the `PeerTestTracker`;
   - starts one probe against one peer, with a fresh probed identifier and a
     fresh timestamp, and sends it through the runtime's ordinary outbound
     path;
   - records the probe in the tracker **before** the send, so an answer that
     arrives first is never dropped;
   - completes the probe on an inbound echo whose identifier matches an
     outstanding probe, and reports the elapsed local time;
   - expires the outstanding probes on its own bounded cadence, and reports
     each expiry under its own reason.
2. **`crates/i2pr-daemon/src/peer_test.rs`** (or the live inbound pump) — the
   **responder**. The inbound path that already classifies
   `RouterI2npOutcome::PeerTest` also builds the echo and sends it back on
   the delivering session. One answer per inbound test, never more.
3. **`crates/i2pr-runtime/src/ssu2_runtime.rs`** — the production recorder. A
   public method that takes the **typed confirmed outcome** the daemon
   observed and records `ReachabilitySignal::PeerTestResult { family,
   outcome: Confirmed }` on the service tracker. It is distinct from
   `map_controlled_outcome_locked`, it is refused after shutdown, and it
   counts its outcome under its own reason.
4. **`crates/i2pr-daemon/src/service_product.rs`** — the wiring: the live
   inbound pump answers a peer test, and the owner is driven from the
   **existing** service loop with explicit ownership and cancellation. No new
   task, no new timer, no new listener.

## 7. Design decisions the plan must make explicit

- **D1 — Which peer test.** The I2NP peer test (message 231) that the daemon
  already models, **not** the transport's out-of-session three-party test. The
  daemon's module, the proto's message and the dispatcher's classification all
  already exist for 231; the transport's surface stays controlled-only.
- **D2 — Who starts the probe.** The owner asks the **existing** selection for
  a peer with a session; it does not build a peer pool and it does not dial.
  If no peer is available, the attempt is refused and counted, not queued.
- **D3 — The opt-in.** The peer test is **off by default** and is refused
  unless the operator enables it explicitly. A router that has not asked for
  reachability evidence must not generate probe traffic.
- **D4 — The recording's only production call site.** The runtime's production
  recorder is called from exactly one place in the daemon: the owner's
  confirmed-match path. A guard proves there is no other production call site
  and that the controlled recorder is untouched.
- **D5 — The signal is about this router.** The recorded `PeerTestResult`
  belongs to the **service** tracker — the router's own reachability — and
  never to a peer's profile. One local condition is never written into a
  peer's record.
- **D6 — The answer is not evidence for the responder.** The responder's echo
  proves nothing about the responder's own reachability; only the originator
  learns anything, and only from a **matched** echo. The responder records
  nothing.
- **D7 — Failure is neutral.** A timeout, an unmatched echo and a refused
  probe leave the tracker exactly as it was. No failure is recorded as a
  signal, so a failed test can never poison later evidence.
- **D8 — The publication is not touched.** The peer test does not run inside
  the publication cycle and does not change the record. It only supplies
  evidence that Plan 520's owner later reads through the transport's own
  snapshot.

## 8. Work packages

### WP1 — The runtime's production recorder

The runtime records the router's own confirmed peer test, in its own method,
with its own counter, refused after shutdown. The method takes the typed
outcome; it never takes a boolean.

### WP2 — The daemon's peer-test owner

The bounded originator: one probe at a time per peer, the table's own
ceilings, the probe recorded before the send, the completion on a matched
echo, the expiry on the owner's own cadence, and a counter per outcome.

### WP3 — The responder on the live inbound path

The inbound pump answers an inbound peer test on the delivering session, with
the probed fields echoed unchanged, and records nothing about itself.

### WP4 — The wiring and the opt-in

The owner is built from the operator's opt-in, runs inside the **existing**
service loop with explicit ownership and cancellation, and adds no task, no
timer and no listener.

### WP5 — The lane, the guard and the documentation

The lane drives the behaviour end to end through the daemon's own seams; the
guard proves the structural invariants with inverted probes; the support
inventory, the conformance ledger, the qualification gates and the deep-dives
are updated.

## 9. Failure, cancellation, restart, and contention semantics

- **A refused probe** (the table is full) is counted under its own reason.
  The table is a ceiling, not a target: a probe is refused rather than
  evicting an older outstanding probe, so an answer can never be matched
  against a different probe.
- **A timeout** expires the probe and counts it. The tracker is left
  otherwise untouched: a failed test records **no** signal, so it can never
  poison later evidence.
- **An unmatched echo** is counted and matched to nothing.
- **A restart** rebuilds the owner with an empty table. No probe survives a
  restart, because a probe is only meaningful within the session that carried
  it.
- **Contention** — one owner, one table, one probe at a time per peer. Two
  concurrent attempts for the same peer are refused, not queued.
- **Cancellation** — the owner holds no task of its own; it runs inside the
  service's existing loop and its cancellation path is the service's.

## 10. Compatibility and migration

- **No wire change.** Message 231 and its echo already exist in the proto
  crate and are already classified by the dispatcher.
- **No dependency change.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
- **No configuration migration.** The opt-in is new and defaults to off, so
  the default configuration changes in no way.
- **The published RouterInfo's `caps` string is unchanged**, and
  `router.version` stays `0.9.62`.
- **The default router's behaviour is unchanged**: with the opt-in off no
  probe is sent and no signal is recorded.

## 11. Required tests

### The lane — the public behaviour

- the opt-in is off by default, and a router with it off sends no probe;
- the responder answers an inbound peer test with the probed fields echoed
  unchanged, on the delivering session;
- a probe that is answered is matched and completes with the elapsed local
  time;
- a probe that is not answered times out and is counted, and **records no
  signal**;
- an unmatched echo matches nothing;
- the table's capacity ceiling refuses a probe rather than evicting one;
- a confirmed probe records the router's **own** reachability, and the
  transport's tracker reaches `Reachable` when the other two classes are also
  present;
- the responder records nothing about itself;
- a peer's record never carries this router's condition.

### The guard — the wiring

- the owner records the probe **before** the send;
- the recording's only production call site is the confirmed-match path;
- the controlled recorder is untouched;
- the owner adds no task, no timer and no listener;
- the daemon records no reachability signal outside the confirmed-match path;
- the responder derives the answer from the authenticated delivery.

### The negative controls

Every guard rule has an inverted probe that must be reported.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test peer_test_role -- --test-threads=1
cargo test --locked -p i2pr-daemon --test peer_test_role_boundary -- --test-threads=1
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the whole `AGENTS.md` routine floor before handoff.

## 13. Documentation updates

- `specs/support.toml` — a new surface for the router's own peer test, with
  `advertised = false`.
- `specs/CONFORMANCE.md` — the inventory entry and the section.
- `specs/QUALIFICATION-GATES.md` — the gates, each naming its evidence.
- `specs/protocols/09-ssu2.md` — the peer test's role in the reachability
  evidence.
- `docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-runtime.md` —
  the new owner and the new recorder, with their line counts derived from the
  tree.
- `plans/registry.md` and
  `plans/subsystems/network-participation-roadmap.md` — the live index.

## 14. Acceptance criteria and stop conditions

The plan closes when, and only when:

1. the lane proves the opt-in is off by default, the responder answers with
   the probed fields unchanged, a matched echo completes, a timeout records
   **no** signal, and a confirmed probe records the router's own reachability;
2. the guard's rules and their inverted probes are green;
3. the routine floor is clean;
4. `specs/support.toml` records the surface as **not advertised**, and the
   advertised count stays zero;
5. `Cargo.toml` and `Cargo.lock` are byte-identical.

**Stop conditions.** The plan stops — and records what it stopped at — if:

- the confirmed outcome cannot be produced without the **controlled**
  activation, in which case the finding is reported against the controlled
  path rather than worked around;
- the echo cannot be delivered on the delivering session;
- the transport's corroboration policy would have to change, which is out of
  scope;
- the stock-peer half is reached for, which is Plan 493's.

## 15. Closure evidence required

- The lane's output.
- The guard's output, with every inverted probe reported.
- The routine floor's full result table.
- The requirement-to-evidence matrix.
- The invariant, failure, migration and security reviews.
- The findings by severity, and the roadmap disposition.
- The unblock audit.

## 16. Handoff notes

- The next free plan number after this one is **522**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is patched
  or vendored.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 520's **M1** is the finding this plan discharges. If the plan closes
  without a production entry point that records a confirmed peer test, **M1
  stays open** and the successor says so.

## 17. Amendment log

_The corrections to this plan, if any, are recorded here and never in a
silent rewrite of the registered text._

### 17.1 The answered-key table (WP3)

**What changed.** The registered §6.2 and §11 named "one answer per inbound
test, never more", but named no bound on the **reply traffic** a peer can
provoke. A peer that sends the same test repeatedly would have been
answered repeatedly, and a peer that echoes the router's own late answer
would have started a second exchange.

**Why it matters.** `AGENTS.md` requires that network bytes be treated as
hostile and bounded. Without a bound, the reply traffic is bounded only by
the peer's patience — that is, by nothing the router controls.

**The correction.** A second bounded table remembers the keys the router has
**already answered**. A repeated key is **suppressed** rather than answered
again, the table is a ceiling that refuses rather than evicting, and it ages
out on the owner's own sweep. The table is separate from the outstanding-
probe table on purpose: a flood of inbound probes must not be able to fill
the probe table and block the router's own probes.

**Where it lives.** `crates/i2pr-daemon/src/peer_test.rs` — the
`PeerTestOwner::responded` table, `MAX_RESPONDED_PEER_TESTS`, the
`answers_suppressed_repeat` and `answers_refused_full` counters, and the
suppression in `PeerTestOwner::handle_inbound`.

### 17.2 The runtime derives the family; the daemon asserts none (WP1)

**What changed.** The registered §6.3 said the runtime's production
recorder "takes the **typed confirmed outcome** the daemon observed". The
implemented method takes the **link** the exchange travelled on and the
sealed proof, and it **derives** the address family from that session's own
address.

**Why it matters.** The reachability tracker is **family-scoped**: the
transport's `supporting_classes` counts the distinct classes of the **latest
family** only (`crates/i2pr-transport/src/reachability.rs:511-528`). A
daemon that named the family could file the evidence under a family the
exchange never used, and the corroboration count would then be taken over
the wrong family. The runtime is the only component that knows the session's
address, so the runtime derives it.

**The correction.** `Ssu2RuntimeService::note_confirmed_peer_test(link_id,
proof)` resolves the family from `state.active[link_id].peer_addr.ip()` and
refuses with `Ssu2PeerTestEvidenceError::NoSession` when the session is
already gone. The daemon names **no** family, and the guard proves it.

### 17.3 One probe at a time per peer (WP2)

**What changed.** The registered §9 named the contention rule — "one probe
at a time per peer. Two concurrent attempts for the same peer are refused,
not queued" — but the registered §6.1 did not say how the table enforces
it, and the tracker is keyed by the probed identifier **and** timestamp.

**The correction.** `PeerTestTracker::has_outstanding_for(peer)` answers
the question directly, and `PeerTestOwner::start_probe` refuses a second
attempt for the same peer with `PeerTestOwnerError::InFlight` and its own
counter. The registered §9's rule is now implemented rather than implied.

### 17.4 The stock-peer half is narrower than §3.1 implied (WP5)

**What changed.** §3.1 describes "the responder's echo" as the answer owed
to an inbound peer test, and the scope says the **stock-peer** half is
Plan 493's. The implementation work found that the stock half is narrower
still: **no stock router implements an I2NP 231 responder at the router
level**.

- i2pd uses message 231 only for a **garlic-carried tunnel** test
  (`libi2pd/TunnelPool.cpp:436`, `:493-501` at the pin
  `635b013a612ff47278ef02acf8580a28e10e26c5`); its
  `TunnelPool::ProcessTunnelTest` is the **initiator** half, matching the
  message id against the tunnel pool's own outstanding tests.
- Java I2P has **no** `TunnelTestMessage` class at all at the pin
  `9134f808337b401e8e53c73734c81fab04280c9d`; its peer test is the
  out-of-session `PeerTestManager`
  (`router/java/src/net/i2p/router/transport/udp/`).

**Why it is recorded here.** The router-level 231 responder is therefore
proven against **this router's own peer**, over a real SSU2 session on
loopback. The stock-peer half stays blocked on Plan 493's authorized
two-host topology, and it is now blocked on it for a **second** reason
besides the topology itself. The plan's stop condition — "the stock-peer
half is reached for, which is Plan 493's" — was therefore never reached:
the lane drives the router-level responder, not a stock peer.

**Nothing else in the plan changes.** The mechanism, the opt-in, the
match, the failure semantics, the bounds and the non-goals are as
registered.

### 17.5 §3's citation drift

**§3 is evidence and is not rewritten.** The registered §3 cited the
production tree at registration time, and the implementation moved that
tree. The drift is recorded here, additively, with a registration-time to
current pointer table. The **banner** on §3 marks it as the
registration-time snapshot.

| §3's pointer | now | what it names |
| --- | --- | --- |
| `crates/i2pr-daemon/src/peer_test.rs:87` | `:157` | `PeerTestEcho::encode` |
| `crates/i2pr-daemon/src/peer_test.rs:122` | `:276` | `PeerTestTracker` |
| `crates/i2pr-daemon/src/peer_test.rs:202` | **retired** | `dispatch_router_i2np_with_peer_test` — replaced by `PeerTestOwner::handle_inbound` at `:638` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:3214` | `:3314` | `map_controlled_outcome_locked` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:3746` | `:3846` | `send_i2np_with_link` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:5966` | `:6066` | the `ValidatedPath` record (class 2) |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:6097` | `:6204` | the `AuthenticatedPeerObservedExternalAddress` record (class 1) |
| `crates/i2pr-daemon/src/service_product.rs:5277` | `:5572` | the `route_inbound_with_liveness_classified` call |
| `crates/i2pr-transport/src/reachability.rs:475` | `:511` | `supporting_classes` |
| `crates/i2pr-transport/src/reachability.rs:528` | `:564` | `recompute` |
| `crates/i2pr-transport/src/reachability.rs:557-561` | `:594-595` | the `Reachable` transition |

**Unchanged:** `crates/i2pr-proto/src/i2np/header.rs:61`,
`crates/i2pr-daemon/src/router_i2np.rs:748`,
`crates/i2pr-daemon/src/tunnel_liveness.rs:785`,
`crates/i2pr-transport-ssu2/src/peer_test.rs:483`,
`crates/i2pr-daemon/src/floodfill.rs:675`, and in
`crates/i2pr-transport/src/reachability.rs` the lines `66`, `119`, `136`
and `195`.

**The one that is not a drift but a retirement.** §3.1 cited
`dispatch_router_i2np_with_peer_test` as the responder's production hook,
and then said in the same paragraph that it **must not** become the
production path, because it re-decodes the message the dispatcher had
already decoded. The registered **D9** therefore required the responder to
use the dispatcher's classified outcome instead. The implementation retired
the function rather than rewiring it: it is gone, and
`PeerTestOwner::handle_inbound` reads the classified outcome. The
registered §6.2 — "the responder" — is satisfied by the new seam, and the
retirement is the **only** way §3.1's own requirement could be met.
