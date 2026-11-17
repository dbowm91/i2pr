# Plan 549 — a validated path must stay validated while the session lives

## 1. Objective

Give reachability corroboration class 2 (`ValidatedPath`) a **periodic
producer for the path in use**, so a healthy long-lived SSU2 session sustains
`Reachable` instead of decaying to `CandidateReachable` when the 30-minute
observation TTL lapses.

Plan 547 measured the consequence: its floodfill host reached
`role="Active"` 30.0 s after a confirmed peer test, withdrew 29 m 30 s later,
re-activated for 1 m 30 s, withdrew again, and stayed `Disabled` for the
remaining 2 h 45 m of a 3 h 04 m run.

## 2. Why ready

The mechanism is fully located, the reference behaviour is established from
the **pinned source**, and the fix has a precedent in this repository's own
recent history: Plans 530 and 535 each gave an existing corroboration class a
producer in ordinary operation, on an existing tick, without touching the
model.

`Reachable` requires **three** distinct supporting classes of the latest
family inside `DEFAULT_OBSERVATION_TTL` (30 min,
`crates/i2pr-transport/src/reachability.rs:60`). The production producers and
their refresh triggers:

| Class | Producer | Refreshed while a session is **stable**? |
| --- | --- | --- |
| 0 `LocalConfiguredBind` | `note_bound_endpoints_as_explicit_bind` + `refresh_explicit_bind_locked` (`ssu2_runtime.rs:8353`), on the maintenance pass (Plan 535) | **yes** |
| 1 `AuthenticatedPeerObservedExternalAddress` | `DecodedBlock::Address` → `SessionEvent::AddressObserved` → `note_observed_external_address_locked` (`:3499`) | **no** — only when a peer sends an Address block |
| 2 `ValidatedPath` | `accept_path_response_locked` (`:7586`, recorded at `:7596`) | **no** — only while a *candidate* path is being migrated to |
| 3 `PeerTestResult{Confirmed}` | `note_confirmed_peer_test` (`:3903`) | **yes**, every 5 min |

Steady state is class 0 + class 3 = **2**, which is exactly
`min_corroboration`, so the state is pinned at `CandidateReachable`;
`publication_material` (`:3699`) polls expiry and returns
`ReachabilityUnqualified`. The property is inverted: only a path that keeps
re-handshaking or migrating can hold `Reachable`.

The gap is **documented and was never given an owner**:
`specs/protocols/09-ssu2.md:633` — *"a router that binds once and never
observes a peer loses its evidence — the re-record is a follow-up"* — and
Plan 530's registry row calls it *"a named follow-up"*.

## 3. Current implementation evidence

### 3.1 The model, and why two classes decay

`supporting_classes` counts classes only for the **latest** signal's family
and only while each signal is inside the TTL (`reachability.rs:572-589`);
`expire_locked` drops the oldest first (`:558-569`). Nothing refreshes a
class that its own trigger does not re-emit.

### 3.2 The reference answers every path challenge unconditionally

i2pd `2.61.0` (`635b013a612ff47278ef02acf8580a28e10e26c5`),
`libi2pd/SSU2Session.cpp`:

```text
case eSSU2BlkPathChallenge:
    LogPrint (eLogDebug, "SSU2: Path challenge");
    SendPathResponse (buf + offset, size);
break;
```

There is no precondition beyond holding the session: any conformant peer
answers. And the answer is harmless to the peer, because i2pd acts on a
PathResponse **only when it issued the challenge**:

```text
case eSSU2BlkPathResponse:
    if (m_PathChallenge) { if (buf64toh (buf + offset) == m_PathChallenge->first) { ... } }
break;
```

So a re-validation challenge from this router costs the peer one small
datagram and changes no peer state.

### 3.3 The reference does **not** send Address blocks on ordinary data packets

This is why the obvious fix — "periodically send an Address block" — is
**rejected** rather than adopted. In i2pd, `CreateAddressBlock` is emitted only
from the handshake (`SSU2Session.cpp:999`), the terminate packet (`:1572`),
`SendPathResponse` (`:3332`), `SendPathChallenge` (`:3365`), the hole punch
(`SSU2OutOfSession.cpp:296`) and peer-test messages 6/7
(`SSU2OutOfSession.cpp:179`). The ordinary Data sender builds
**DateTime + Ack + Padding** and nothing else (`SSU2Session.cpp:3290-3310`).
A stock peer therefore never refreshes our class 1 by steady-state traffic, and
adding a non-reference Address report would be this router inventing wire
behaviour to satisfy its own model.

### 3.4 The current path cannot be challenged today

`note_authenticated_packet` returns `Ok(None)` **before doing anything** when
the endpoint is the validated one (`crates/i2pr-transport-ssu2/src/path.rs`):

```text
if endpoint == self.validated.endpoint() {
    return Ok(None);
}
```

and challenges are further bounded by `MAX_PATH_CHALLENGES_PER_SESSION`. A
re-validation therefore needs its own bounded entry point; it cannot be
smuggled through the migration API.

### 3.5 What the plan does not claim

It does not claim that `Reachable` becomes true from one observation, that the
corroboration floor moves, that class 1 gains a producer, or that a
public-network claim is supported. It does not touch Plan 548's
publication-target bootstrap.

## 4. Invariants

- **The reachability model is untouched.** The signal kinds, the
  corroboration classes, `min_corroboration` (2), the contradiction floor (1)
  and the TTL are unchanged. `Reachable` still needs **three distinct
  classes**. Plan 521's guard rule 11 keeps holding.
- **The daemon writes no reachability.** The producer lives in the runtime,
  which owns the tracker and the sockets.
- **One observation is one fact.** A matched re-validation records
  `ValidatedPath` and **only** `ValidatedPath`. It must not also record a peer
  test, an address observation, or any second class.
- **No migration from a re-validation.** A re-validation of the current
  endpoint must never move the endpoint, reset congestion state, or touch
  per-IP accounting. Only a genuinely different candidate path may migrate.
- **Bounded.** One outstanding re-validation per session, a fixed cadence
  derived from the TTL (well under it), a per-session budget, and a failure
  that costs nothing but the cadence. No new task, no timer, no listener, no
  socket: it rides the runtime's **existing** maintenance pass.
- **No secret, no identity, no payload in any report.** Counts, elapsed time
  and stable labels only.
- **Nothing advertised.** `router.version` stays `0.9.62`; the inventory stays
  `advertised = false` with the advertised count at **zero**; `Cargo.toml` and
  `Cargo.lock` stay byte-identical.
- **The predecessors are not rewritten.** Plans 544–548's lanes, guards and
  closure records are untouched; corrections live here.

## 5. Scope

### 5.1 In scope

1. A bounded re-validation of the **current** path in the path module, with its
   own budget and its own indication, distinct from migration candidates.
2. The runtime driver that queues the challenge, stages it at the existing
   minimum-MTU staging path, and records `ValidatedPath` on an exact match —
   with no migration.
3. The lane: a real session sustains `Reachable` past the observation TTL
   without any new handshake or migration.
4. The guard: the §4 invariants, with an inverted probe each.
5. Documentation, inventory, the full floor.

### 5.2 Out of scope

- **Class 1's producer.** Giving it steady-state traffic would require wire
  behaviour the reference does not exhibit (§3.3). A successor may implement
  the SSU2 in-band peer test (block type 13, messages 4–7), which carries the
  Address block in messages 6/7 (`SSU2OutOfSession.cpp:179`) and would refresh
  class 1 *and* class 3 together — the reference's own arrangement. That is a
  large, separate plan and this one must not pretend to it.
- Plan 548's publication-target bootstrap.
- Any policy change, stock-peer qualification, or public-network claim.

## 6. Required production changes

### 6.1 The path module gains a re-validation

A `PathTracker` entry point that challenges the **validated** endpoint, holds
the expected challenge for a bounded window, and answers a matching response
with a distinct outcome (`Revalidated`) that the runtime records as
`ValidatedPath` — separately from `Validated`, which migrates. Its budget is
its own; `MAX_PATH_CHALLENGES_PER_SESSION` continues to govern migration.

### 6.2 The runtime drives it on the existing pass

`drive_timeouts_locked` (`ssu2_runtime.rs:8382`) already re-asserts the bind
(Plan 535) on every maintenance pass. The re-validation is queued from the
same pass, one session at a time, on a cadence far below the TTL.

## 7. Design decisions

### 7.1 Re-validate the path in use, not a candidate

The evidence a floodfill needs is "a peer can reach me at the address I
publish, right now". The path in use is the only path that statement is about.

### 7.2 Migrating is not this

The re-validation outcome must be structurally distinct from `Validated`, or a
periodic probe would repeatedly re-run migration accounting against an
unchanged address — pointless work, and a latent source of drift in the
per-IP and per-subnet accounting.

### 7.3 The peer is not asked to change

Because the reference answers unconditionally and ignores unsolicited
responses, this is invisible to the peer's state machine. That is what makes it
safe to run against a stock router.

### 7.4 Class 1 stays unowned, deliberately

The tempting fix — file the peer-test round trip under `ValidatedPath` as well
— is **rejected**: it files one observation under two classes, which is
exactly what the classes exist to prevent. The honest alternative is the SSU2
in-band peer test, and that is a successor's scope, named in §5.2.

## 8. Work packages

| WP | Deliverable |
| --- | --- |
| WP1 | The reference question, answered from the pin — **done at registration**, §3.2 and §3.3 |
| WP2 | The path module's bounded re-validation, structurally distinct from migration |
| WP3 | The runtime driver on the existing maintenance pass; records `ValidatedPath` only |
| WP4 | The lane: `Reachable` sustained past the TTL on a real session, no handshake, no migration |
| WP5 | The guard: §4, one inverted probe per rule |
| WP6 | Negative controls: each mutation fires its own rule, tree restored byte-identically |
| WP7 | Docs, inventory, full 76-step floor |
| WP8 | The three-host re-measurement: the role window, and a publication attempt |

## 9. Failure, cancellation, restart and contention semantics

- An unanswered challenge is a lapse, not an error: the cadence simply issues
  the next one, and the counters record the miss.
- A challenge that cannot be staged (queue full, budget exhausted) fails
  closed and cancels the re-validation; the path keeps carrying traffic.
- Cancellation is the service's; the probe holds no task of its own.
- Shutdown refuses a re-validation rather than recording one.
- The re-validation must never delay the maintenance pass: it stages at most
  one small datagram per pass.

## 10. Compatibility and migration

No configuration key is added and no default moves. No codec, block type,
length or option changes: the PathChallenge and PathResponse blocks already
exist and are already decoded. `router.version`, the advertised count and both
manifests are unchanged.

## 11. Required tests

### 11.1 The lane

Two real routers over real UDP: a session is established, the reachability
state reaches `Reachable`, the lane advances past `DEFAULT_OBSERVATION_TTL`
**with no further handshake and no migration**, and the state is still
`Reachable`. A control arm with the re-validation disabled must fall to
`CandidateReachable` at the same instant — otherwise the lane proves nothing.

### 11.2 The guard

Every §4 invariant, each with an inverted probe, plus: the model's constants
unchanged, `Reachable` still requiring three classes, the re-validation
outcome distinct from `Validated`, and no daemon-side signal write.

## 12. Exact verification commands

```text
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-daemon --test <lane> -- --test-threads=1
cargo test --locked -p i2pr-daemon --test <guard> -- --test-threads=1
cargo clippy --locked -p i2pr-runtime -p i2pr-transport-ssu2 --all-targets --all-features -- -D warnings
```

plus the full 76-step floor before closure.

## 13. Documentation updates

`specs/protocols/09-ssu2.md` (the reachability section, discharging the
"re-record is a follow-up" note with a pointer), `specs/support.toml` (one
surface, `advertised = false`), `specs/QUALIFICATION-GATES.md` (a new
section), `specs/CONFORMANCE.md`, `docs/architecture/i2pr-runtime.md` (the
reachability chapter), `plans/registry.md` and the network-participation
roadmap.

## 14. Acceptance criteria and stop conditions

Accepted when a live session holds `Reachable` across at least two observation
TTLs with **no new handshake and no migration**, the floodfill role does not
withdraw on that host, and the counters show the re-validations that carried
it — or, if the role still withdraws, the record states which class lapsed and
why.

**Stops** if the fix needs the model changed (a floor, a class, a TTL), needs
the daemon to write a signal, needs a second class recorded from one
observation, or needs a migration to be triggered by a re-validation.

## 15. Closure evidence required

Commits; the floor table; lane and guard rows; negative controls with the
restoration proof; the requirement-to-evidence matrix; the invariant, failure,
migration and security reviews; limitations, findings, roadmap disposition and
the unblock audit.

## 16. Handoff notes

- A **stable** session must be able to prove itself. If a change makes proof
  depend on churn, it is backwards.
- **Class 1's** producer is genuinely missing; only the SSU2 in-band peer test
  (block type 13, msgs 4–7, Address block in 6/7) supplies it the way the
  reference does. Do not fake it.
- The migration path and the re-validation path must stay distinguishable in
  the receipt, or the accounting drift will be invisible.

## 17. Amendment log

### 17.1 — 2026-11-13, registration, with WP1 answered

Registered from Plan 547's measurement and its closure record's Findings
(high, "the floodfill role does not sustain"). WP1 was executed **at
registration**, against the pinned i2pd `2.61.0`, and it changed the plan's
first choice:

- The **initially preferred** fix — send a data-phase Address block on a
  bounded cadence — is **rejected**. The reference does not do it: i2pd's
  ordinary Data sender emits DateTime + Ack + Padding only
  (`libi2pd/SSU2Session.cpp:3290-3310`), and every `CreateAddressBlock` site is
  a handshake, termination, hole punch, path challenge/response, or peer-test
  messages 6/7. Adopting it would have made this router invent wire behaviour
  to satisfy its own model.
- The **chosen** fix is a bounded re-validation of the path in use. The
  reference answers every PathChallenge unconditionally
  (`SSU2Session.cpp:1899-1901`) and ignores a PathResponse it did not request
  (`:1904-1913`), so the peer is unaffected beyond one datagram, and the
  matching response is an authenticated round trip on the path the router
  actually publishes.
- The **current path cannot be challenged today**: `note_authenticated_packet`
  returns `Ok(None)` for the validated endpoint and the challenge budget is
  per-session, so §6.1's entry point is required rather than convenient.
- **Class 1 stays unowned by this plan.** The reference's own arrangement is
  that the SSU2 in-band peer test carries the Address block in messages 6/7
  (`SSU2OutOfSession.cpp:179`); implementing that is a successor's scope
  (§5.2), and this plan must not pretend to it.

Plan 548 (the publication-target bootstrap) is independent and unaffected.

### 17.2 — 2026-11-13, WP2 and WP3 implemented

**The path module gained a re-validation that cannot migrate.**
`PathValidator` now holds an optional `Revalidation` beside its migration
candidates, and the two are deliberately different types with different
outcomes: a candidate is an endpoint the router does not yet trust and may
move to (`PathEvent::Validated`), while a re-validation names the endpoint the
router already publishes and must never move (`PathEvent::Revalidated`).
`request_revalidation` refuses a weak challenge, refuses a second request while
one is in flight (`PathError::RevalidationInFlight`), and is bounded by
`MAX_PATH_REVALIDATIONS_PER_SESSION` — its own ceiling, so
`MAX_PATH_CHALLENGES_PER_SESSION` keeps governing migration alone.
`on_path_response` resolves a re-validation **first** but only for an exact
match from the validated endpoint, so a genuine candidate response is never
eaten; an expired answer is refused and counted under its own reason, and a
wrong value leaves the probe open so the peer can still answer inside the
window.

**The runtime drives it from the existing maintenance pass.**
`drive_path_revalidations_locked` probes **at most one** session per pass, only
when its cadence is due and it holds no probe already outstanding, so the work
per pass is bounded however many sessions exist. A staging or queueing failure
costs nothing but the cadence. `record_revalidated_path_locked` records
`ValidatedPath` for the family of the endpoint that proved itself and
**nothing else** — one observation is one fact — and moves no accounting.

**The scheduler deadline had to learn about it.** `drive_timeouts_locked` is
reached from the loop's `notify` arm **and** from its deadline arm, so a probe
that only fired on a wake would be exactly the dependency this plan exists to
break: an idle session would never prove itself. The `sleep_for` computation
now includes `next_revalidation_ms` when no probe is in flight, and the path
module's `next_deadline_ms` includes the in-flight deadline, so an idle
session still wakes to its own cadence.

**Verified so far.** `i2pr-transport-ssu2` lib tests **167/167** (8 new:
re-validation proves without migrating, a second request is refused, an answer
from another endpoint is not the proof, a wrong value keeps the probe open, a
late answer is refused and counted, a zero challenge is refused, the budget is
a ceiling, and a candidate response is not eaten by an outstanding
re-validation); `i2pr-runtime --test ssu2_local` **19/19**; the daemon's
`ssu2_only_reachability` lane **6/6**. `cargo fmt` and `clippy -D warnings`
clean on both changed crates.

**Still open.** WP4 (the lane that holds `Reachable` across two TTLSs with no
handshake and no migration, with a disabled arm that decays — without which the
lane proves nothing), WP5 (the guard), WP6 (the negative controls), WP7 (docs,
inventory, the 76-step floor) and WP8 (the three-host re-measurement).

### 17.3 — 2026-11-13, the lane, the guard, and the diagnostics the plan was missing

**WP4 and WP5 landed.** The lane
(`crates/i2pr-runtime/tests/ssu2_path_revalidation.rs`) drives two real runtime
services over loopback UDP: a live session issues a PathChallenge, the peer
answers, `path_revalidations` increments, and `path_migrations` stays at
**zero** — re-proving the path in use is not a migration. Its control arm sets
the cadence out of reach and pins `path_revalidations` to zero, so the lane
cannot pass on traffic the runtime would have produced anyway. A third arm
proves the composition at unit speed rather than the transport's half-hour:
three classes re-recorded on the cadence hold `Reachable` across **two**
observation lifetimes, and the same signals without the path re-record fall to
the corroboration floor.

The guard
(`crates/i2pr-daemon/tests/ssu2_path_revalidation_boundary.rs`) is six rules
with an inverted probe each: the model untouched (floor still two,
`ValidatedPath` still class 2), a re-validation cannot migrate (distinct
outcome, one assignment of the validated endpoint), one observation records one
class, the daemon records no signal, the probe is bounded (a named ceiling that
is enforced, one session per pass, no task or listener), and the cadence is a
scheduler deadline.

**Writing the guard caught two of its own needles, not two tree defects.** The
class-map rule counted a *grouped* match (`| Self::ValidatedPath { .. } =>`)
as a second mapping, and the "returns its own outcome" rule looked for
`return Ok(` **after** the variant name instead of before it. Both were fixed
in the rule; neither was a licence to change the code.

**The diagnostics gap, and the same lesson twice.** `17.2` claimed the
re-validation was observable because it is counted. That was wrong in the way
Plan 545 recorded one layer down: `path_revalidations` was incremented and
**never reported**, so "the probe ran and was answered" was indistinguishable
from "the probe never ran". Two further gaps made the 3 h 04 m measurement only
half-readable:

- `publish_transport` is fed the **literal** `"loopback-only"`
  (`crates/i2pr-daemon/src/lib.rs:890`), so the published
  `transport.reachability` is a constant and is **not** the tracker's state.
  There is therefore no read path for reachability in the lab at all.
- the tracker's class occupancy and `expires_at` are reported by **nothing**,
  which is why 547's withdrawal had to be attributed by inference.

The plan therefore adds, counts and labels only:

- `path_revalidation_completed` (INFO, with the running total) when a probe is
  answered, and `path_revalidation_refused` (WARN) when it cannot be queued, so
  a probe that never reached the wire says so;
- `reachability_state_changed` (INFO) with `from`, `to`, `supporting_classes`
  and `evidence_expires_in_millis`, emitted **on transition** rather than on a
  poll, from both the maintenance pass and the recorder. That is what turns a
  withdrawal from "the role changed, cause unknown" into "the state went from
  `Reachable`/3 classes to `CandidateReachable`/2 with this much evidence left".

No identity, no address, no key and no payload is carried by any of them, and
the guards' structural rules are unaffected — the model is still untouched and
the recorder still names exactly one signal.

**Verified.** `i2pr-transport-ssu2` 167/167; the lane 3/3; the guard 7/7;
`i2pr-runtime --test ssu2_local` 19/19; `clippy -D warnings` clean across the
workspace (which caught a `type_complexity` on the guard's rule table — fixed
with a named alias rather than an `allow`).

**A checker gap the plan exposed, recorded rather than papered over.**
`scripts/check-m12-floodfill-boundaries.sh` keeps corroboration recording
inside the runtime service owner by scanning `crates/` for the tracker's
recording call with `--glob '!*/tests/*'`. **That exclusion cannot match
`crates/<crate>/tests/<file>`** — the glob allows exactly one path segment
before `tests`, so no crate test file has ever been exempt — and the guard's
mutation literal was therefore read as a production recording site. Because the
rule is right and the script must not be weakened, the **code** moved: the
probe assembles its needles with `concat!`, and its prose no longer names the
call. The broken glob is a real coverage hole in a boundary checker and needs
its own plan-of-record to fix, with a positive control that shows a test file
mentioning the call is exempt and a production file mentioning it is not.

### 17.4 — 2026-11-13, the measurement, and the second gap it exposed

**The re-validation works, on the wire, in production.** The instrumented run
(`be77cfd4…`, boot `21:53:07`) reports, counts only:

```text
21:53:07  reachability_state_changed from=None to=ObservedUnconfirmed        classes=1
21:53:07  reachability_state_changed from=ObservedUnconfirmed to=CandidateReachable  classes=2
21:53:37  reachability_state_changed from=CandidateReachable to=Reachable    classes=3
21:54:07  floodfill qualification activated requested=true role="Active"
22:03:25  path_revalidation_completed path_revalidations=1
22:03:25  reachability_state_changed Reachable -> Reachable                  classes=4
```

The probe lands on the 10-minute cadence and takes the supporting-class count
from three to **four** — the class Plan 549 was registered to give a producer is
now produced on a live session with no handshake and no migration.

**And it did not stop the withdrawal.** At `22:23:07` — boot + 30 min to the
second — the advertisement withdrew with **no `reachability_state_changed` line
at all**. The state was, and stayed, `Reachable`. So the flap is **not** a
reachability lapse, and `17.2`/`17.3`'s framing ("a withdrawal is the class
lapsing") was wrong for this case.

**The withdrawal could not say why, which is the second gap of the same kind.**
`FloodfillServiceStep::Withdraw` is chosen by
`FloodfillServiceOwner::evaluate` (`crates/i2pr-daemon/src/floodfill.rs:1531`),
which computes the categorical `NormalIneligibility` list and stores it in
`status().reasons` — and the daemon's withdrawal arm logged `requested` and
`role` and **dropped the reasons**. So the one line that would explain a 30 s
flap carried everything except the explanation. The arm now logs `reasons`,
which is a fixed categorical enum: no peer, address, key or payload.

**A hypothesis falsified from the source, not by waiting.**
`explicit_bind_refresh_due` (`ssu2_runtime.rs:478`) re-asserts class 0 every
`observation_ttl / 2` — 15 minutes — so the bind class cannot be what lapses.
The earlier guess that it could was wrong and is recorded as wrong.

**What is now measurable that was not.** The flap recurs on a 30 s–4 min beat,
so one instrumented run of about ten minutes names its cause instead of costing
another 40-minute inference. The remaining candidates are all in the readiness
set `evaluate_normal_eligibility` grades — `netdb_ready`, `storage_ready`,
`maintenance_ready`, `resource_headroom`, `clock_sane`,
`supervision_healthy`, and `material` — and they are now distinguishable.
