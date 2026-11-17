# Plan 510 — the managed-tunnel staging path under load, and its diagnosability

**Subsystem:** `core-router-recovery`
**Status:** registered
**Predecessor:** Plan 491 (F4 — the intermittent `i2pcontrol_tunnels` managed-tunnel staging failure), Plan 487, Plan 289 (the TunnelManager black-box lane)
**Successor:** none yet

---

## 1. Objective

Close Plan 491's **F4** — *"an intermittent `i2pcontrol_tunnels` managed-tunnel
failure"* — by making the managed-tunnel staging path **observable under load**
and its failures **diagnosable**.

Plan 491 recorded F4 as *"recorded, not fixed"*: the floor's first run
reported `tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets`
failing with *"runtime transition failed: manager configuration rejected"*; it
did not reproduce in three isolated runs, in the full `--no-fail-fast`
workspace sweep, or in the two later floor runs. The residual risk was
described as *"a load-sensitive race in the managed-service staging path"*,
and the finding was left for a plan-of-record because the path belongs to the
I2PControl / service-tunnel line.

Plan 491's own words are the requirement, and they name two things:

1. **the race is load-sensitive** — so the plan must exercise the path
   *under load*, not in isolation; and
2. **the failure was undiagnosable** — every manager rejection collapsed
   into one static label, so an occurrence could not be attributed.

This plan delivers both: a **load-sensitive black-box lane** that drives
concurrent managed-tunnel creates against a real daemon, and a **value-free
reason classification** so that a rejection names its class.

## 2. Why ready

- **The finding is registered and owned.** Plan 491's closure record names
  F4, quotes the exact failure, states that it did not reproduce, and names
  the owner and the reason it needs a plan-of-record.
- **The path is live and black-box reachable.** `crates/i2pr-daemon/tests/i2pcontrol_tunnels.rs`
  already starts a real daemon on `127.0.0.1:0` and drives the seven
  lifecycle actions through real TLS and TCP after listener startup. A load
  variant reuses that machinery; it invents nothing.
- **The loss of information is in the tree, not in a guess.**
  `crates/i2pr-daemon/src/i2pcontrol_tunnels.rs`'s `static_manager_reason`
  maps every `ServiceTunnelError::InvalidConfig(_)` to the single label
  *"manager configuration rejected"*. The detail is discarded. The runtime-neutral
  error type the daemon wraps is already well classified (it has distinct
  `ExceedsCeiling`, `ContradictoryOptions`, `DuplicateId` and
  `DuplicateListener` variants); the daemon's own wrapper is the lossy one.
- **Nothing outside the daemon is affected.** The reason string is produced
  by the daemon and is not pinned by any fixture, spec, contract crate or
  other test: `grep` for *"manager configuration rejected"* and *"runtime
  transition failed"* across `crates/`, `specs/`, `docs/` and `tests/`
  returns only the daemon's own definition.

## 3. Current implementation evidence

### 3.1 The failure is undiagnosable

`crates/i2pr-daemon/src/i2pcontrol_tunnels.rs:5107`:

```rust
fn static_manager_reason(error: &crate::service_tunnels::ServiceTunnelError) -> &'static str {
    use crate::service_tunnels::ServiceTunnelError as E;
    match error {
        E::InvalidConfig(_) => "manager configuration rejected",
        E::DestinationRuntime(_) => "destination runtime failed",
        E::Bind(_) => "listener bind failed",
        E::Storage(_) => "service storage failed",
    }
}
```

The `InvalidConfig` arm discards the detail, and the detail is where the
class lives. The daemon's `InvalidConfig` messages name their class in
prose — *"candidate validation failed"*, *"bind collision"*, *"use_ssl
requires a daemon TLS policy"*, *"Destination group missing"*, *"ephemeral
identity generation failed"*, *"fabric prepare failed"*, *"LeaseSet2
encryption does not match its selected crypto policy"*, *"destination
registry config rejected"*, *"deferred activation"*, *"already registered"*,
*"poisoned"* — but the *whole* string is dropped.

### 3.2 The staging path is the load-sensitive part

The I2PControl create path is
`crates/i2pr-daemon/src/i2pcontrol_tunnels.rs`'s `commit_locked`, which
calls `self.manager.reconcile(Arc::new(candidate), CONTROL_DRAIN_DEADLINE)`.
`CONTROL_DRAIN_DEADLINE` is `Duration::ZERO`, so the failure is not a drain
wait. The reconcile's `Add` arm stages every new spec, and staging calls
`build_service_runtime` → `create_bridge_for_group`, which **generates a
destination identity, builds a lease set and validates the crypto policy**.
That is real, CPU-bound work per create, and it is the part that stretches
when the machine is busy.

### 3.3 The lane machinery exists

`crates/i2pr-daemon/tests/i2pcontrol_tunnels.rs` provides
`start_service`, `authenticate`, `tunnel_raw` and `distinct_port`. A create
needs a distinct name, a distinct loopback port and a distinct
`TargetDestination`; the file's existing tests already do that one at a
time. The load lane does it **concurrently**.

### 3.4 The lane must not be another flake

The repo's own rule is explicit: *"prefer `#[tokio::test(start_paused = true)]`
/ manual clock + explicit bounded deadlines; never wall-clock sleeps for
overload/state-machine tests"*. The lane therefore drives real concurrency
against real sockets — the load is **real work**, not a simulated sleep —
and every wait is an **explicit bounded deadline**, never a fixed sleep.

## 4. Invariants that must not regress

1. The I2PControl reason **never echoes a value**: no id, no address, no
   port, no key, no payload, no error detail. Only a stable class label.
2. The **known** classes keep today's behaviour exactly; an **unknown**
   class keeps the single generic label, so the change can never make a
   rejection *less* informative than it is now.
3. The daemon's error type keeps its shape for every caller but the
   reason: the change is confined to the reason mapping.
4. No new configuration key, dependency, task, timer, listener, socket, or
   production wire change beyond the reason string.
5. The declared `router.version` stays `0.9.62`; `caps` and the published
   RouterInfo are unchanged; the advertised count stays zero.
6. Every lane the plan adds is **deterministic**: it either passes
   always or fails always, and it fails with a named reason.

## 5. Scope

### In scope

- The **load-sensitive black-box lane** for the managed-tunnel staging path.
- The **value-free class classification** of the manager's rejections, and
  the I2PControl reason that carries it.
- A **static guard** that pins the class list and asserts the reasons carry
  no value.
- The documentation, the support inventory, the registry and the roadmap.

### Explicitly out of scope

- **Reproducing the original failure is not a precondition.** The lane is
  the plan's contribution whether or not it reproduces: if it reproduces,
  the plan has found the defect; if it does not, the path is under a
  load-sensitive lane that did not exist before, and the reason is
  diagnosable if it ever does.
- **Any change to the staging path's behaviour.** The plan does not make
  staging faster, lazier, or more tolerant. It observes it and reports it.
- **The transit-hop failure detection gap** (Plan 487's F3), which needs a
  new plan-of-record and is not a staging-path concern.
- **Any stock-peer claim.** The lane is over the tree's own deterministic
  surfaces.

## 6. Production changes

One file, one function, one table:

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/i2pcontrol_tunnels.rs` | `static_manager_reason`'s `InvalidConfig` arm maps the detail to a **value-free class** by a pinned marker list; an unknown detail keeps today's single label. |

No other production file is touched. The plan-of-record's production-change
table is therefore one row, and the closure record must say so if that
changes.

## 7. Design decisions the plan must make explicit

### 7.1 The classification is by **marker**, not by prefix

The daemon's `InvalidConfig` messages embed the service id at the front
(*"canonical use_ssl requires a daemon TLS policy"*). A *prefix* match
would therefore have to know the id, and a class that depends on the id is
not a class. The classification matches a **stable marker phrase** that
appears **inside** the detail and carries no value — *"use_ssl requires a
daemon TLS policy"*, *"candidate validation failed"*, *"bind collision"* —
and the class is the matched marker's label.

### 7.2 An unknown detail keeps the generic label

The mapping is **total**: every detail yields a label. The known markers
yield their own class; anything else yields the current
*"manager configuration rejected"*. A new rejection therefore cannot make
the operator's view worse, and a new marker can be added deliberately with
its own guard entry.

### 7.3 The class is a label, not a diagnostic

The label is a short, stable, value-free phrase. The **detail** — which
may name an id, a socket or a crypto error — stays inside the daemon and is
never put on the wire. This is the same discipline the version receipts
follow: counts and stable labels, never identities.

### 7.4 The lane creates *real* load

The lane's load is **real work**: concurrent managed-tunnel creates, each of
which generates a destination identity and builds a lease set, against one
real daemon with one real listener. It is not a sleep, a simulated delay, or
a mocked clock. The lane's own concurrency is bounded, and every wire
operation has an explicit deadline.

### 7.5 The lane is a **lane**, not a repair

If the lane reproduces the original failure, the plan's closure records the
reproduction and the defect it found. If it does not, the closure says so
plainly, and Plan 491's F4 is closed **conditionally**, with the named
condition and the exact future evidence.

## 8. Work packages

### WP1 — the value-free reason classification

`static_manager_reason`'s `InvalidConfig` arm classifies the detail by the
pinned marker list. The list is a named constant, the labels are stable,
and an unknown detail keeps the generic label.

### WP2 — the load-sensitive lane

`crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs` starts one real
daemon and drives **N concurrent** managed-tunnel creates, each with its own
name, its own loopback port and its own target destination. Every create
must succeed. A deliberately contradictory configuration must be **rejected**
in the same lane, so the reject path is exercised rather than assumed.

The lane is deterministic: bounded concurrency, explicit deadlines, and no
wall-clock sleep.

### WP3 — the static guard

A guard asserting: the marker list is pinned and non-empty; every label is
value-free (no digit, no colon, no `://`, no path separator, bounded
length); the generic label is still reachable for an unknown detail; the
`InvalidConfig` arm no longer collapses **every** detail; and the reason
strings carry no value.

### WP4 — the documentation and the inventory

The daemon deep-dive, `specs/CONFORMANCE.md`, one `experimental`,
`advertised = false` support surface, and the registry and roadmap rows.

### WP5 — the floor and the closure record

The routine floor on the closing tree, the closure record, and the unblock
audit.

## 9. Failure, cancellation, restart, and contention semantics

- The lane's concurrency is **bounded** and its waits are **explicit
  deadlines**. A lane that times out fails with the deadline named; it
  never spins and never sleeps for a fixed period.
- The classification is a **pure function** of the detail: no lock, no
  allocation beyond the label, no I/O, and it cannot fail.
- An unknown detail is **not** an error: the generic label is returned, so
  the plan cannot turn an unclassified rejection into a different failure.
- The plan adds no task, no timer and no listener, so it adds no
  cancellation or restart surface.

## 10. Compatibility and migration

- **One wire-visible change**: the I2PControl error's `reason` string for a
  manager rejection may now name a class. The string is not pinned by any
  fixture, spec, contract crate or other test, and the I2PControl contract
  does not specify it. The **known** classes keep today's labels.
- **No peer-observable change**: a peer never sees the reason.
- **No configuration change**, no dependency change, no data migration, and
  `Cargo.toml` / `Cargo.lock` stay byte-identical.

## 11. Required tests

| Test | File | Asserts |
| --- | --- | --- |
| Concurrent managed-tunnel creates all succeed | `crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs` | the staging path survives concurrent load |
| A contradictory configuration is rejected | the same | the reject path is exercised |
| The classification is value-free | `crates/i2pr-daemon/tests/managed_tunnel_reason_boundary.rs` | no reason carries a value |
| The marker list is pinned | the same | the classes cannot drift |
| An unknown detail keeps the generic label | the same | the change can never lose information |
| The existing black-box lane is unchanged | `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels` | the TunnelManager contract is intact |

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels_load -- --test-threads=1
cargo test --locked -p i2pr-daemon --test managed_tunnel_reason_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Plus the repository's routine floor, in full, on the closing tree.

## 13. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the managed-tunnel staging path's
  reason classification and the load lane.
- `specs/CONFORMANCE.md` — the Plan 510 section, stating what is claimed
  and what is not.
- `specs/support.toml` — one `experimental` surface,
  `service-tunnels.managed-staging-reason-classification`, with
  `advertised = false`.
- `plans/registry.md` and `plans/subsystems/core-router-recovery-roadmap.md`
  — the Plan 510 rows.

## 14. Acceptance criteria and stop conditions

### 14.1 Acceptance

1. The classification is implemented, and every label is value-free.
2. The load-sensitive lane is green and deterministic.
3. The guard holds on the real tree, and each of its rules has an inverted
   probe.
4. The controls are applied to the **real** tree, run against the **real**
   lanes, and reverted byte-for-byte, with the failing-test counts recorded.
5. The routine floor passes in full on the closing tree.
6. Nothing is advertised: `router.version` stays `0.9.62`, `caps` and the
   published RouterInfo are unchanged, the advertised count stays zero, and
   `Cargo.toml` / `Cargo.lock` stay byte-identical.
7. Plan 491's F4 is closed, **conditionally** if the original failure did
   not reproduce, with the named condition and the exact future evidence.

### 14.2 Stop conditions

- **The lane reproduces the original failure.** The plan's scope becomes the
  defect it found, and the closure records the reproduction, the cause and
  the fix. A fix that changes the staging path's behaviour is a
  **corrective successor** under this plan's number, per the repository's
  corrective-pass rule.
- **The classification would need to echo a value.** The class is dropped
  and the generic label is kept; a value is never echoed.
- **A known class's label would have to change.** The label is kept and the
  class is not shipped; the reason text is not a contract, but it is the
  operator's view, and changing it silently is worse than not shipping it.

### 14.3 What this plan does **not** close

- Plan 491 **F2** (no stock-peer evidence) — needs Plan 493's authorized
  two-host topology.
- Plan 491 **F5** (the comparator's four-component ceiling) — recorded, not
  fixed, and out of this plan's path.
- Plan 487 **F3** (the transit-hop failure detection) and Plan 497
  limitation 5's **read** half.
- Every externally-blocked plan.

## 15. Closure evidence required

1. The commits, in order.
2. A requirement-to-evidence matrix.
3. The commands run, with their outcomes, labelled local or CI truthfully.
4. The negative controls, with the mutation, the failing-test count, and the
   byte-for-byte restoration.
5. The invariant, failure, migration and security reviews.
6. The limitations, the findings by severity, the roadmap disposition, and
   the unblock audit.

## 16. Handoff notes

- **The plan may close F4 conditionally.** If the lane does not reproduce the
  original failure, the closure says so plainly, names the condition, and
  states the exact future evidence that would close it unconditionally.
- **The lane is the durable contribution.** A load-sensitive lane over the
  managed-tunnel staging path did not exist before, and it is what a future
  occurrence would be caught by.
- **The reason classification is the diagnosability.** A future occurrence
  names its class, so the next engineer does not start where Plan 491
  started — with a single generic label and no attribution.
- **Plan 491's closure record is not rewritten.** F4's discharge lives in
  this plan's closure record and in the registry row.
