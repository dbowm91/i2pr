# Plan 537 — the relay requester must ask without holding the router's NetDB lock

**Closure record.** The plan of record is
`plans/implementation/network-participation/537-the-relay-requester-must-ask-without-holding-the-routers-netdb-lock.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 537 is **closed**. The router's relay **requester** role asks a peer for a
relay with **no lock held**: each read of the router's NetDB is a short,
self-contained read whose guard is released before the read returns, and the
signature and the transport's writes happen afterwards. And the requester's
refusal to ask is **never skipped silently**: the outbound step runs whenever
the requester role exists, and a router that holds no usable validated store
refuses the request under a **typed**, **counted** reason.

Classification: **corrective** (the requester role's machinery was complete and
the composition root drove its outbound step inside the router's NetDB lock,
across an O(population) store scan, a private-key signature and two of the
transport's own mutex acquisitions; and the step was gated on the daemon
holding a `Bootstrap` at all, so a request that could have been made left no
trace).
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The router's NetDB lock no longer wraps the transport's locks.** This was
the defect's sharpest edge. The composition root drove the requester's
outbound step inside the bootstrap's mutex — the router's NetDB lock — for the
whole step, and the step's last two actions take the **transport's own
mutexes**: `start_relay_request` locks the peer-relay service's requester
table (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:501`) and `queue_relay_request`
locks the runtime's **state** lock
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2765`). So the router's NetDB lock
**wrapped** the runtime's state lock, where the runtime's own order is
sockets → state, and where the composition root's own tick takes the state
lock, and releases it, *before* it takes the NetDB lock. After this plan, no
code path holds the router's NetDB lock while acquiring the transport's locks.

**The request is produced with no lock held.** Each read of the router's NetDB
is a short, self-contained read whose guard is released before the read
returns, so the RelayRequest's **signature** — a private-key operation — and
the transport's requester-table write and its carriage happen with **no** NetDB
lock held. The pool read is one iteration of the store's index; the per-candidate
reads are one keyed lookup and one strict address parse each.

**The refusal to ask is never skipped silently.** The outbound step used to be
gated on the daemon **holding** a `Bootstrap`: a router that did not hold one
did not ask, did not refuse and did not count, so a request that could have
been made left no trace at all. The step now runs whenever the requester role
exists, and a missing store — or a **poisoned** lock — is a **typed**,
**counted** refusal (`StoreUnavailable`, counted under
`refusals_store_unavailable`).

**The refusal is a fact about this router, never about a peer.** It is never
written into a peer's profile, it never counts as a peer fault, and it is
counted under its own reason alone.

**The reads stay read-only.** Nothing is mutated, no peer profile is written,
and the store's own contents are unchanged by a window.

**The requester's semantics are untouched.** Its window, its rotation, its
nonce, its preimage, its signature, its session gate, its refusals and its
counters are exactly what the tree already had. Plan 531's lane and guard stay
**byte-identical**, so the predecessor's evidence remains exactly as it was,
and Plan 531's guard still finds the strings it looks for. Plan 536's lane and
guard stay **byte-identical** too, because this plan touches the same
composition root.

**The requester's need for the router's peer population is unchanged.**
Asking a peer for a relay genuinely needs to know which peers the router's
NetDB holds, so the pool read stays. Only the **lock scope** and the **silent
skip** changed.

## What the plan did not close

- **The router's validated store is still the bootstrap's store.** The
  bootstrap owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change and was not attempted here.
  This is Plan 536's recorded **Low** finding 1, carried forward unchanged.
  **Owner:** the successor that gives the router a NetDB owner of its own.
- **The composition root's shape is proven statically, not behaviourally.**
  The lane drives the **production source** directly, so it cannot see a
  regression in the composition root; the composition root's shape is proven
  by the **guard**. This is Plan 536's recorded **Low** finding 2, carried
  forward unchanged. **Owner:** the successor that makes the composition
  root's relay arm reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.
- **The pool read's bound.** The candidate pool is the NetDB's peer population
  and the window's rotation needs the whole pool, so the pool read is
  O(the store's size). It is now a single, self-contained read, but it is not
  a constant bound. See **Findings by severity**.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `9e79534b` | the plan of record, the registry's row and the roadmap's row |
| `e50b5799` | the production change: the material-source trait, the store's implementation, the production source, the typed refusal and its counter, and the composition root's ungated outbound step |
| `e5c38d44` | the lane and the guard |
| `1d6ad935` | the documentation, the inventory and the plan's amendment log |
| `c4710932` | the guard's preimage-binding rule and its inverted probe |
| `896e3562` | the plan's §3 citations expanded to full paths, and one wrong line corrected |
| `f2a4a173` | the plan's amendment log's row for that citation drift |

The floor ran on the committed tree `f2a4a173`. The closure commit adds **only**
this record and the two closed rows, exactly as Plan 536's closure commit did.

The plan of record's **invariants** are at
`plans/implementation/network-participation/537-the-relay-requester-must-ask-without-holding-the-routers-netdb-lock.md:149`,
its **amendment log** at
`plans/implementation/network-participation/537-the-relay-requester-must-ask-without-holding-the-routers-netdb-lock.md:561`,
and the daemon deep-dive's account of the new shape at
`docs/architecture/i2pr-daemon.md:4357`.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| the request is produced with **no** lock held, and the lane measures it | `crates/i2pr-daemon/tests/ssu2_relay_requester_lock.rs`, `the_request_is_signed_with_the_netdb_lock_free` |
| the measurement is **not vacuous** | the same lane file, `the_lock_measurement_detects_a_lock_held_across_the_request` |
| the outbound step **runs** with no store and counts the typed refusal | the same lane file, `the_outbound_step_runs_without_a_store_and_counts_the_typed_refusal` |
| a **poisoned** lock is the same typed refusal | the same lane file, `a_poisoned_netdb_lock_is_the_same_typed_refusal` |
| the request is still produced correctly with a store | the same lane file, `the_request_is_still_produced_correctly_with_a_store` |
| the reads mutate nothing and write no peer profile | the same lane file, `the_reads_mutate_nothing_and_write_no_peer_profile` |
| every refusal is typed and counted under its own reason | the same lane file, `every_refusal_is_typed_and_counted` |
| the outbound step is **not** gated on the bootstrap | `crates/i2pr-daemon/tests/ssu2_relay_requester_lock_boundary.rs`, `the_outbound_step_is_not_gated_on_the_bootstrap` |
| the production source's reads **release** the lock before they return | the same guard file, `the_source_releases_the_lock_before_the_request` |
| the reads are **read-only** | the same guard file, `the_reads_are_read_only` |
| the missing store is typed and **counted** | the same guard file, `the_missing_store_is_a_typed_and_counted_refusal` |
| the requester's machinery is **untouched** | the same guard file, `the_requesters_machinery_is_untouched` |
| the transport's requester table and refusal order are **untouched** | the same guard file, `the_transports_requester_table_is_untouched` |
| **no** new surface, and the step rides the **existing** tick | the same guard file, `no_new_surface_and_the_step_rides_the_existing_tick` |
| Plan 531's and Plan 536's evidence stay **byte-identical** | the same guard file, `the_predecessors_evidence_is_untouched` |
| every rule has an inverted probe and fails closed | the same guard file, the `the_guard_reports_*` and the `the_guard_fails_closed_when_*` rows |
| the router's NetDB lock never wraps the transport's locks | the guard's `the_outbound_step_is_not_gated_on_the_bootstrap` and `the_source_releases_the_lock_before_the_request`, plus the negative control NC-4, which restores the historical defect and fires both |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester_lock -- --test-threads=1` | PASS — 7 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester_lock_boundary -- --test-threads=1` | PASS — 33 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester --test ssu2_relay_requester_boundary --test ssu2_relay_target_lock --test ssu2_relay_target_lock_boundary --test ssu2_relay_target --test ssu2_relay_target_boundary --test ssu2_relay_establishment --test ssu2_relay_establishment_boundary -- --test-threads=1` | PASS — 8 rows, 26 rows, 7 rows, 29 rows, 7 rows, 36 rows, 8 rows and 41 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/537-negative.py` | PASS — 13 controls, each firing the surface that owns the defect, each restored byte-identically |

The closure record's **citations** are verified mechanically, not by reading. The
verifier recomputes every cited line from the tree and requires the record to
name exactly that line:

- the **closure record's** citations are recomputed against the **current
  tree** — the first line that carries the cited text is the line the record
  must name;
- the **plan of record's** citations are recomputed against the **registration
  commit** `9e79534b`, because the plan's §3 is the evidence of the tree
  *before* the plan and is never rewritten;
- the **lane's and the guard's rows** are checked by name, so the record cannot
  claim a row the evidence file does not carry.

One rule is scoped rather than absolute, and the scope is stated. The rule is
that **no citation may be a bare `:line` shorthand**, because a shorthand is not
mechanically checkable and a reader cannot confirm it. The plan's body — §1
through §16 and §17.1 — carries **no** bare shorthand at all. The plan's
amendment log is the one place where a bare token is **quoted** rather than
cited: row 17.2.9 reports the drift the plan itself found — that §3 had been
written with bare line numbers — and a findings row quotes the form it
reports. The scoped check therefore requires that every bare shorthand in §17.2
sits on a line that also names the shorthand form it reports, so the token can
only be there as the quoted form of the drift the log records. Both checks are
mechanical and fail closed; neither is an unbounded exemption.

## The routine floor

The floor ran on the committed tree `f2a4a173` and it is **green**. Every
step is recorded in `/tmp/537-floor.log`, with each step's own log under
`/tmp/537f-<step>.log`. The floor is a plain sequence of `step` runs that
records each step's exit status; no step is skipped, no `|| true`, no
`continue-on-error` and no filename filtering. The workspace step runs with
`--no-fail-fast`, so every test binary's result is reported rather than the
run stopping at the first failure.

Plan 537's floor is Plan 536's floor plus two steps — the daemon lane and
the daemon guard for the relay requester's ask lock scope. **No other step
changed, and no other plan's guard file was touched.**

| Step | Outcome |
| --- | --- |
| `floor-tree-clean` | PASS |
| `fmt` | PASS |
| `check` | PASS |
| `app-siblings` | PASS |
| `app-fixture-static` | PASS |
| `app-fixture-copy` | PASS |
| `app-fixture-rebuild` | PASS |
| `plan522-peer-pool-lane` | PASS |
| `plan522-peer-pool-boundary` | PASS |
| `plan522-peer-pool-unit` | PASS |
| `transport-ssu2-unit` | PASS |
| `runtime-unit` | PASS |
| `netdb-unit` | PASS |
| `plan523-introducer-lane` | PASS |
| `plan523-introducer-boundary` | PASS |
| `plan524-introducer-request-lane` | PASS |
| `plan524-introducer-request-boundary` | PASS |
| `plan520-publication-lane` | PASS |
| `plan520-publication-boundary` | PASS |
| `plan525-introducer-publication-lane` | PASS |
| `plan525-introducer-publication-boundary` | PASS |
| `plan526-introducer-advertisement-lane` | PASS |
| `plan526-introducer-advertisement-boundary` | PASS |
| `plan521-peer-test-role-boundary` | PASS |
| `plan527-reachability-contradiction-lane` | PASS |
| `plan527-reachability-contradiction-boundary` | PASS |
| `plan528-peer-test-verdict-lane` | PASS |
| `plan528-peer-test-verdict-boundary` | PASS |
| `plan529-observed-address-lane` | PASS |
| `plan529-observed-address-boundary` | PASS |
| `plan530-explicit-bind-lane` | PASS |
| `plan530-explicit-bind-boundary` | PASS |
| `plan531-relay-requester-lane` | PASS |
| `plan531-relay-requester-boundary` | PASS |
| `plan532-relay-target-lane` | PASS |
| `plan532-relay-target-boundary` | PASS |
| `plan533-relay-establishment-lane` | PASS |
| `plan533-relay-establishment-boundary` | PASS |
| `plan534-relay-session-peer-lane` | PASS |
| `plan534-relay-session-peer-boundary` | PASS |
| `plan535-explicit-bind-refresh-lane` | PASS |
| `plan535-explicit-bind-refresh-boundary` | PASS |
| `plan536-relay-target-lock-lane` | PASS |
| `plan536-relay-target-lock-boundary` | PASS |
| `plan537-relay-requester-lock-lane` | PASS |
| `plan537-relay-requester-lock-boundary` | PASS |
| `test-workspace` | PASS |
| `clippy` | PASS |
| `doc` | PASS |
| `test-doc` | PASS |
| `dependency-direction` | PASS |
| `plan-number-uniqueness` | PASS |
| `adr-number-uniqueness` | PASS |
| `portable-service-tunnel-api` | PASS |
| `portable-service-tunnel-consumer` | PASS |
| `planning-tests` | PASS |
| `runtime-boundaries` | PASS |
| `console-boundaries` | PASS |
| `console-browser-security` | PASS |
| `tooling-inventory` | PASS |
| `license-metadata` | PASS |
| `managed-app-private-client-seams` | PASS |
| `router-readiness-contract` | PASS |
| `router-readiness-self-test` | PASS |
| `core-router-recovery-contract` | PASS |
| `core-router-recovery-self-test` | PASS |
| `plan449-ssu2-preflight` | PASS |
| `plan495-peer-eligibility` | PASS |
| `plan476-gate-binding` | PASS |
| `plan476-gate-binding-self-test` | PASS |
| `managed-app-process-boundary` | PASS |
| `managed-app-process-boundary-self-test` | PASS |
| `managed-app-package-boundary` | PASS |
| `managed-app-package-boundary-self-test` | PASS |
| `managed-app-policy-boundary` | PASS |
| `managed-app-policy-boundary-self-test` | PASS |
| `managed-app-secured-sandbox` | PASS |
| `managed-app-secured-sandbox-self-test` | PASS |
| `service-tunnel-boundaries` | PASS |
| `managed-app-gateway-boundary` | PASS |
| `managed-app-manager-boundary` | PASS |
| `m11-per-epoch-composition` | PASS |
| `service-anonymity-boundaries` | PASS |
| `fixture-manifest` | PASS |
| `ntcp2-vectors` | PASS |
| `ssu2-vectors` | PASS |
| `i2cp-vectors` | PASS |
| `ntcp2-interoperability` | PASS |
| `constrained-host-lane-boundary` | PASS |
| `m11-transit-boundaries` | PASS |
| `m11-transit-qualification-evidence` | PASS |
| `sam-acceptance-evidence` | PASS |
| `ssu2-acceptance-evidence` | PASS |
| `i2cp-acceptance-evidence` | PASS |
| `i2pcontrol-acceptance-evidence` | PASS |
| `els2-type11-transcript-boundary` | PASS |
| `els2-live-lane-evidence` | PASS |
| `encrypted-service-consumer-caller` | PASS |
| `outproxy-request-path` | PASS |
| `outproxy-wire-lane-evidence` | PASS |
| `config-secret-hygiene` | PASS |
| `workflow-validity` | PASS |
| `floodfill-type5-serve` | PASS |
| `service-tunnel-acceptance-evidence` | PASS |
| `exploratory-tunnel-evidence` | PASS |
| `netdb-tunnel-evidence` | PASS |
| `destination-tunnel-evidence` | PASS |
| `streaming-tunnel-evidence` | PASS |
| `m6-mixed-router-acceptance-evidence` | PASS |
| `m12-floodfill-qualification-evidence` | PASS |
| `m12-floodfill-boundaries` | PASS |
| `ntcp2-harness` | PASS |
| `cargo-deny` | PASS |

The floor's own summary is **113 passed, 0 failed**, and the table above lists every step it ran.

## The negative controls

The controls live in `/tmp/537-negative.py` and their log is
`/tmp/537-negative.log`. Each control mutates one or more sources, runs the
plan's guard and the plan's lane — and, where the plan touches a predecessor's
evidence, the predecessor's own guard and lane — and records exactly which
rows failed. The tree is restored from an in-memory backup and the **SHA-256**
of every touched file is compared before and after, so the restoration is
**proved** rather than assumed.

Every mutation **compiled**. A control that breaks the build proves nothing,
so a compile error is reported as an inconclusive control rather than as a
firing rule; the two controls that initially failed to compile (the gate
mutation and the configuration mutation) were corrected until they compiled,
and the corrected controls are the ones recorded below.

| # | What the control breaks | What fired |
| --- | --- | --- |
| NC-1 | the outbound step is gated on the bootstrap again | the guard's `the_outbound_step_is_not_gated_on_the_bootstrap` |
| NC-2 | the outbound step holds the NetDB lock across the request | the guard's `the_outbound_step_is_not_gated_on_the_bootstrap` |
| NC-3 | the production source holds the guard past the material read | the guard's `the_source_releases_the_lock_before_the_request` |
| NC-4 | **the historical defect is restored**: the driver reads the store directly, under a NetDB lock the composition root holds across the whole step | the guard's `the_outbound_step_is_not_gated_on_the_bootstrap` **and** `the_source_releases_the_lock_before_the_request` |
| NC-5 | the material read mutates the router's NetDB | the guard's `the_reads_are_read_only` |
| NC-6 | the missing store is refused under the wrong reason | the guard's `the_missing_store_is_a_typed_and_counted_refusal`, and the lane's `the_outbound_step_runs_without_a_store_and_counts_the_typed_refusal` and `a_poisoned_netdb_lock_is_the_same_typed_refusal` |
| NC-7 | the missing store's refusal is not counted under its own reason | the guard's `the_missing_store_is_a_typed_and_counted_refusal`, and the lane's `the_outbound_step_runs_without_a_store_and_counts_the_typed_refusal` and `a_poisoned_netdb_lock_is_the_same_typed_refusal` |
| NC-8 | the request's preimage binds the introducer and the target the wrong way round | the guard's `the_requesters_machinery_is_untouched`, and Plan 531's lane's `the_router_reaches_a_peer_through_its_introducer` |
| NC-9 | the request rides any session, not the named one | the guard's `the_requesters_machinery_is_untouched`, and Plan 531's lane's `the_router_reaches_a_peer_through_its_introducer` and `a_request_is_refused_when_the_tag_does_not_identify_the_target` |
| NC-10 | the transport's requester refusal order is changed | the guard's `the_transports_requester_table_is_untouched` |
| NC-11 | the daemon's configuration surface gains a key | the guard's `no_new_surface_and_the_step_rides_the_existing_tick` |
| NC-12 | Plan 531's guard is neutered | the guard's `the_predecessors_evidence_is_untouched`, and Plan 531's own guard's `the_owner_is_wired_to_the_existing_tick` |
| NC-13 | the production source leaks the guard past the material read | the lane's `the_request_is_signed_with_the_netdb_lock_free` and `the_reads_mutate_nothing_and_write_no_peer_profile` |

**NC-4 is the control against the real tree.** It restores the **historical**
defect — the composition root takes the bootstrap's mutex and holds it across
the whole outbound step, and the driver reads the validated store directly —
and it fires **two** of the guard's rules. The control is the plan's
behavioural statement of the defect, made and then broken.

**NC-13 is the control against the real lane.** It leaks the production
source's guard past the material read, so the router's NetDB lock is held at
the moment the request is signed, and the lane's probe observes it. The lane
takes the lock with `try_lock` and never with `lock`, so the control **fails
two rows** instead of hanging the suite; a lane that hangs proves nothing.

## Acceptance criteria

| Criterion | Result |
| --- | --- |
| the request is produced with **no** lock held, and the lane measures it | PASS — `the_request_is_signed_with_the_netdb_lock_free`, and NC-13 fires it |
| the outbound step runs without a store and **counts** a typed refusal | PASS — `the_outbound_step_runs_without_a_store_and_counts_the_typed_refusal`, and NC-6 and NC-7 fire it |
| the requester's semantics are unchanged, and Plan 531's lane and guard are byte-identical | PASS — the predecessor's digests are pinned by the guard and its rows are unchanged |
| the transport and the runtime are untouched, and no configuration key, task, timer, listener or socket is added | PASS — the guard's rules 5, 6 and 7, and NC-10 and NC-11 |
| `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | PASS — the inventory holds 130 surfaces and 0 advertised |
| the full floor is green, with the closure record's table complete | PASS |
| the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | PASS — 13 controls, each restored byte-identically |

## Invariant review

1. **The request is produced with no lock held.** Holds. Each read is a short,
   self-contained read whose guard is released before the read returns; the
   signature and the transport's writes follow the reads with no guard alive.
   The lane measures the lock at the moment of signing and observes it free.
2. **The requester never nests the NetDB lock inside the transport's.**
   Holds. The driver's body takes no lock of its own; the production source
   takes the bootstrap's mutex only for a read and releases it before the read
   returns; the transport's requester-table mutex and the runtime's state lock
   are therefore acquired with **no** NetDB lock held. The guard's rules 1 and
   2 and the control NC-4 prove it.
3. **The refusal to ask is never skipped silently.** Holds. The outbound step
   is not gated on the bootstrap; a missing store and a poisoned lock are both
   the typed `StoreUnavailable` refusal, counted under
   `refusals_store_unavailable`.
4. **The reads stay read-only.** Holds. The production source and the store's
   implementation touch no store state; the guard forbids every mutating call
   in both, and NC-5 fires it.
5. **The requester's machinery is untouched.** Holds. The preimage's argument
   list is pinned, the nonce discipline is pinned, the session gate is pinned,
   and the runtime's two seams are pinned; NC-8 and NC-9 fire them.
6. **The refusals stay typed.** Holds. Every outcome is a typed variant, and
   each is counted under its own reason.
7. **No transport file changes.** Holds. No file under `crates/i2pr-transport/`
   and no file under `crates/i2pr-runtime/` is touched by this plan.
8. **No configuration key is added.** Holds. The guard counts the requester's
   keys in the SSU2 configuration surface and NC-11 fires it.
9. **No new task, timer, listener or socket.** Holds. The guard counts the
   outbound call sites and the owner module's forbidden constructs.
10. **Plan 531's lane and guard stay byte-identical.** Holds. Their SHA-256
    digests are pinned by the guard, and NC-12 fires it.
11. **No advertisement.** Holds. `router.version` stays `0.9.62` and the
    inventory's advertised count stays **zero**.

## Failure, cancellation, restart, and contention review

**Failure.** A source that cannot answer returns
`RelayRequesterRefusal::StoreUnavailable`; the outbound step counts it under
its own reason and ends the window. A poisoned bootstrap lock is the same
typed refusal. Nothing is skipped without a trace. The window also ends when
the store goes away **under** it — a read that fails mid-window is counted and
the window is abandoned, rather than repeated against a store that is gone.

**Cancellation.** The outbound step is a synchronous step inside the product's
existing tick. It awaits nothing and holds no lock across an await, so a
cancellation cannot strand it. The lane's own NetDB snapshots take the lock
with `try_lock`, so a regression that holds it fails a row rather than hanging
the suite.

**Restart.** Nothing is persisted. The requester's live table is live state
and is rebuilt empty; a restart asks nobody until the next window.

**Contention.** The bootstrap's mutex is now taken for one self-contained read
at a time and released before the next. The NetDB discovery tick, the
publication tick, the peer reconcile and the peer pool can no longer be held
behind an O(population) scan, a private-key signature and two transport mutex
acquisitions. The reads are **read-only**, so they cannot deadlock against a
writer: the mutex is taken once per read and released before the next. And
because the reads are the only thing that holds the NetDB lock, the router's
NetDB lock no longer wraps the runtime's locks — the runtime's own order,
sockets → state, is now the only order its locks observe.

## Migration and compatibility review

**No wire change.** The RelayRequest's bytes, the preimage, the signature, the
transport's requester table and the carriage are exactly what the tree already
produced.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged.

**A refusal that did not exist before.** A router whose NetDB lock is poisoned
now **counts** `StoreUnavailable` where it used to say nothing. That is a new
observation, not a new behaviour: the request was not made before either.

**No change to the requester's semantics.** The same candidates are examined,
the same requests are signed and the same requests are carried.

**No change to the transport's model.** No file under `crates/i2pr-transport/`
and no file under `crates/i2pr-runtime/` is touched.

**No dependency change.** `Cargo.toml` and `Cargo.lock` are byte-identical;
the plan adds no dependency and removes none.

## Security review

**No secret is exposed.** The change touches no key material. The router's own
identity bundle still signs the request through the existing
`RelayRequestSigner` seam, and the seed never leaves the bundle: the seam
signs one preimage and returns the signature bytes.

**No payload is logged.** The counters carry counts only. The new refusal
counts a reason, not a peer. The composition root's existing `tracing::debug!`
for a refused request is unchanged, and it names no peer, no address, no tag
and no key.

**No peer profile is written.** The reads touch no store state and write no
peer profile. The lane measures the store's contents before and after a
window and asserts they are identical.

**A local condition is never a peer's fault.** `StoreUnavailable` is a fact
about **this router**. It is never written into a peer's record, and it never
counts as a peer fault. The requester records no corroborating class from a
relay — the transport's `RelayFirewalledSignal` is a **contradiction** of
reachability and is recorded through the runtime's own recorder, which this
plan does not touch.

**The transport's verification is untouched.** The request's signature is still
verified by the transport, the preimage is still the specification's own, and
the transport's requester table keeps its own ceilings and its own refusal
order.

**No advertisement.** Nothing in the change alters what the router advertises.

## Limitations

- The evidence is **local**: the lane drives a **real** runtime over **real**
  loopback UDP with the **production** source, and the guard is a static
  authority for the shape. No **stock peer** is involved.
- The **stock-peer** half remains blocked externally on Plan 493's authorized
  two-host topology. It was **not** attempted here, and no stock-peer
  behaviour is claimed.
- The delivery stays confined to the **controlled loopback profile** — Plan
  431's bound.
- The **production** SSU2 peer-test driver stays unbuilt.
- The `B` advertisement and the `caps=f` advertisement stay with their owners.
- The **router's validated store is still the bootstrap's store**, and the
  composition root's relay arm is still not reachable from a test.
- The router's reachability model is untouched: the signal vocabulary, the
  corroboration classes, the floors, the states and the expiry semantics are
  exactly what the tree already had.

## Findings by severity

### None — nothing was found that blocks the plan

Nothing blocks the plan. The floor is green, the lane and the guard are green,
the thirteen negative controls each fire the surface that owns the defect, and
the predecessors' evidence is byte-identical.

### Medium — discharged

**The nested lock acquisition.** The requester's outbound step held the
router's NetDB lock while it acquired the transport's requester-table mutex
and the runtime's state lock. That is a **lock-order inversion** against the
runtime's own documented order (sockets → state) and against the composition
root's own tick, which takes the state lock and releases it before it takes
the NetDB lock. It is not a demonstrated deadlock — no path in the tree takes
the state lock and then the NetDB lock today — but it is a latent one, and it
is the reason the lock scope mattered. **Discharged**: after this plan the
router's NetDB lock is taken only for reads, and every read returns before the
transport is touched.

### Low — recorded by this plan

1. **The pool read is O(the router's peer population).** The candidate pool is
   the NetDB's peer population and the window's rotation needs the whole pool,
   so the pool read iterates the store's index. It is now a **single**,
   self-contained read whose guard is released before it returns, but it is not
   a constant bound: a router with a very large NetDB still holds the lock for
   the duration of the scan. The scan copies only hashes and performs no
   crypto, no I/O and no record parse. **Owner:** the successor that gives the
   router a NetDB owner of its own — the same owner that would let the pool be
   read without the bootstrap's mutex at all.
2. **The composition root's shape is proven statically, not behaviourally.**
   The lane drives the **production source** directly and cannot see a
   regression in the composition root; the guard is the authority for that
   shape. This is Plan 536's recorded **Low** finding 2, carried forward
   unchanged and not re-discharged. **Owner:** the successor that makes the
   composition root's relay arm reachable from a test.
3. **The router's validated store is still the bootstrap's store.** Plan 536's
   recorded **Low** finding 1, carried forward unchanged: the bootstrap owns
   the only `RouterInfoStore` in the daemon
   (`crates/i2pr-daemon/src/bootstrap.rs:215`). **Owner:** the successor that
   gives the router a NetDB owner of its own.
4. **The absent-store half of the typed refusal is defensive in the current
   composition.** The requester owner is constructed only when
   `requester_signers` resolves, and that resolves only when the daemon holds
   a `Bootstrap`, so the production composition never asks without a store.
   The refusal is correct in **both** cases and the lane discharges both, but
   the reachable half in production is the **poisoned lock**. **Owner:** the
   successor that gives the router a NetDB owner of its own — a composition in
   which the requester can exist without a bootstrap would make the absent
   half reachable.

## Roadmap disposition

The **network participation** roadmap's milestone row for Plan 537 moves from
`registered` to `closed`, with the token
`passed-the-relay-requester-must-ask-without-holding-the-routers-netdb-lock`.
The roadmap's own completion definition is unchanged: the requester role still
asks a peer for a relay, it still reads only validated material, it still
rides an authenticated session, and it still refuses under typed, counted
reasons. What changed is **where** the request is produced and the fact that
the refusal to ask is now visible.

The **relay requester** phase of the roadmap gains one explicit property: the
router's NetDB lock is not held across the request, and the router's NetDB
lock never nests inside the transport's locks.

The three named out-of-scope items keep their owners: the **production** SSU2
peer-test driver (Plan 530's Low finding 2), the `B` advertisement and
`caps=f` (their own owners), and the **stock-peer** half (Plan 493).

## Unblock audit

The registry's **blocked work** was audited. Every blocked row is blocked
**externally** — the Java I2P and i2pd stock peers (Plan 493's authorized
two-host topology), the M12 floodfill provider, the Proposal 170 Red25519
provider, and the controlled anonymity lanes. **No** registered plan lists
Plan 537 as a hard or interface dependency, and Plan 537 closes no blocker:
the NetDB lock scope is **internal**, and no blocked row's blocker is a
lock scope.

**No plan moves to `ready` on this closure.** The blocked rows stay blocked,
with their blockers unchanged.

## Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 531's closed closure record is **not** rewritten. The discharge of
  Plan 536's **Low** finding 3 lives in this plan's closure record and in the
  registry's row.
- Plan 536's closed closure record is **not** rewritten. Its **Low** findings 1
  and 2 are carried forward here, with their owners named, and its **Low**
  finding 3 is discharged in its recorded terms.
- The plan **extends** no other plan's guard: Plan 531's and Plan 536's lanes
  and guards stay byte-identical.
- The next gap on the **network participation** line is the **router's own
  NetDB owner**: the router's validated store is still the bootstrap's store,
  and separating the router's NetDB from the bootstrap's lifetime is what the
  remaining **Low** findings 1, 3 and 4 all point at.
