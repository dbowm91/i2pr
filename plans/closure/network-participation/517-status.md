# Plan 517 — an unconfirmed publication is retried at the next closest floodfill

**Closure record.** The plan of record is
`plans/implementation/network-participation/517-router-info-publication-retry.md`.
This record is the authority on what the plan closed, what it did not, and
on what the evidence is.

## Status

Plan 517 is **closed**. The router's own RouterInfo publication now
**retries**: a publication the network does not acknowledge is followed by
the next batch, at the next closest floodfills, instead of waiting out the
full period.

Two facts, one gap:

1. **The publication was already repeatable.** Plan 515 made the router's own
   RouterInfo a refreshed record on the reference's period.
2. **The publication was already confirmable.** Plan 516 put the
   correlator's minted token on the wire and joined the inbound owner to the
   correlator, so the floodfill's answer is correlated with the attempt it
   names.

What was still missing is the other half of the reference's mechanism. The
reference does **not** publish once and wait 39 minutes: it publishes, and
**while the answer has not arrived** it asks the next closest floodfill. At
the pinned commit (`635b013a612ff47278ef02acf8580a28e10e26c5`, i2pd 2.61.0)
`libi2pd/RouterContext.cpp:1470` (`HandlePublishTimer`) starts the cycle by
clearing the exclusion set and arming the resend timer; `:1492` (`Publish`)
picks the closest eligible floodfill **not** in the exclusion set (`:1502`),
sends it with a fresh token and **inserts it into the exclusion set**
(`:1540`); `:1560` (`HandlePublishResendTimer`) repeats that every
`ROUTER_INFO_CONFIRMATION_TIMEOUT` milliseconds (`1600`,
`libi2pd/RouterContext.h:37`), and a **drop** triggers it at once
(`:1516-1521`); `:1321` (`PostDeliveryStatusMessage`) reports *"Publishing
confirmed"*, clears the exclusion set and schedules the next publish at the
full interval.

Before this plan, a publication that was delivered and never confirmed was
simply unconfirmed until the next period — `39 * 60` seconds
(`libi2pd/RouterContext.h:34`) — away.

**Plan 517 closes the retry.** Nothing is promoted: `router.version` stays
`0.9.62`, `caps` and the published RouterInfo stay unchanged, the
inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` stay byte-identical.

## What the plan closed

- A publication is a **cycle**: a bounded ladder of **batches**, each batch
  being the closest eligible targets the cycle has not already asked.
- The cycle carries an **exclusion set**, so a target is never asked twice in
  one cycle.
- The **confirmation deadline** is the reference's `1600` milliseconds,
  measured from the moment the batch went **out** — the reading taken after
  the send, exactly as the reference arms its resend timer after the publish
  returns (`libi2pd/RouterContext.cpp:1476`).
- The **retry** itself: `advance_publication_cycle` abandons the awaited batch
  and asks the next one.
- A **delivery refusal** advances at once rather than waiting out the
  timeout (decision **D5**).
- A **confirmation ends the cycle**, and the reference's exclusion set is
  cleared with it (decision **D6**).
- The retry rides the loop's **existing** `100` millisecond tick, so **no new
  timer** is introduced and the loop's wake-up cadence does not change
  (decision **D7**).
- The cycle is **runtime-neutral**: it takes the reading from its caller
  through a `PublicationClock` and never reads a clock itself (decision
  **D8**), so the whole ladder is provable without a wall clock, a socket or
  a sleep.
- The cycle **cannot get stuck**: an outcome that leaves nothing to await
  halts it, so it never holds the next period's cycle out.
- Every rung is counted under its own reason: `retries` for a batch beyond
  the first, `cycles_exhausted` for a cycle that reached the reference's
  bound, and the existing per-outcome counters beside them.

## What the plan did not close

- The **stock-peer** half. That a pinned stock floodfill answers, and that
  the retry therefore reaches one, needs Plan 493's authorized two-host
  topology, and no live claim is made.
- The reference's **direct-or-tunnelled** choice of the publication's
  transport. This tree publishes over its direct SSU2 session, and its
  non-loopback binds are forbidden, so the tunnelled branch is unreachable
  here.
- The reference's **variance** on the publish interval. Plan 515 decided
  against it and that decision stands.
- The reference's **congestion** update. That is the bandwidth-metering line,
  which the roadmap records as genuinely hard and still open elsewhere.
- The publication's delivery still **shares the SSU2 service's loop** with
  the relay drain and the inbound owner. See the **Medium** finding below.
- Any change to the published RouterInfo's content, and any advertisement.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `fbcef6af` | docs(plans): register Plan 517, the publication's retry |
| `26f4bbbc` | feat(daemon): retry an unacknowledged RouterInfo publication |
| `f8ee167a` | docs(specs): record Plan 517's publication retry |
| `099d3fd7` | fix(daemon): the publication's cycle must not get stuck |
| `66a3d2d5` | docs(specs): record the cycle's halt rule |

The closing commit adds this record and the registry and roadmap rows.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| a cycle asks the **closest** eligible targets | `crates/i2pr-daemon/tests/router_info_publication_retry.rs`, `a_cycle_asks_the_closest_eligible_targets` — the expected set is computed from the store's own records, not read back from the answer |
| the next batch **excludes** the targets already asked | the same lane, `the_second_batch_excludes_the_targets_already_asked` |
| a cycle is **not** retried before the reference's deadline | the same lane, `a_cycle_is_not_retried_before_the_reference_deadline` |
| a cycle **is** retried when the deadline passes | the same lane, `a_cycle_is_retried_when_the_deadline_passes_with_no_confirmation` |
| a **delivery refusal** advances at once | the same lane, `a_delivery_refusal_advances_at_once`; a batch that was never asked cannot be abandoned, `a_batch_that_was_never_asked_cannot_be_abandoned` |
| a **confirmation ends the cycle** | the same lane, `a_confirmation_ends_the_cycle`; an unmatched acknowledgement does **not**, `an_unmatched_acknowledgement_does_not_end_the_cycle` |
| the cycle is **bounded** by the reference's fifteen | the same lane, `the_cycle_is_bounded_by_the_reference_fifteen`; a cycle that ran out of targets ends without exhausting, `a_cycle_with_no_eligible_target_left_ends_without_exhausting` |
| the fan-out stays bounded at three | the same lane, `a_cycle_asks_the_closest_eligible_targets` |
| a batch beyond the first is counted as a **retry** | the same lane, `a_batch_beyond_the_first_is_counted_as_a_retry` |
| a retry does **not** move the publication's period | the same lane, `a_retry_does_not_move_the_publication_period` |
| the cycle depends only on the **reading it is given** | the same lane, `the_cycle_depends_only_on_the_reading_it_is_given` |
| a running cycle is not started over | the same lane, `a_cycle_does_not_start_over_a_running_one` |
| the retry rides the loop's **existing** tick, and no new timer | `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`, `the_retry_rides_the_existing_tick` |
| the deadline is the reference's **constant** | the same guard, `the_deadline_is_the_reference_constant` |
| the exclusion set reaches the **selection** | the same guard, `the_exclusion_set_reaches_the_selection` |
| the bound is the reference's **constant** | the same guard, `the_bound_is_the_reference_constant` |
| a confirmation ends the cycle | the same guard, `a_confirmation_ends_the_cycle` |
| the cycle **never reads a clock** | the same guard, `the_cycle_never_reads_a_clock` |
| the fan-out stays **bounded** | the same guard, `the_fan_out_stays_bounded` |
| the cycle cannot **get stuck** | the same guard, `the_cycle_cannot_get_stuck` |
| every rule fires | the same guard's nine **inverted probes**, and the negative controls below |
| Plan 515's and Plan 516's evidence does not regress | Plan 515's lane (`crates/i2pr-daemon/tests/router_info_publication.rs`, 8) and guard (`.../router_info_publication_boundary.rs`, 15), Plan 516's lane (`.../router_info_publication_confirmation.rs`, 8) and guard (`.../router_info_publication_confirmation_boundary.rs`, 15) — all green on the closing tree |

## Focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo check --locked --workspace --all-targets` | clean |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_retry -- --test-threads=1` | 14 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_retry_boundary -- --test-threads=1` | 17 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation -- --test-threads=1` | 8 passed; 0 failed (Plan 516's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation_boundary -- --test-threads=1` | 15 passed; 0 failed (Plan 516's guard) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1` | 8 passed; 0 failed (Plan 515's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1` | 15 passed; 0 failed (Plan 515's guard) |
| `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` | 134 binaries; 2775 passed; 0 failed |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `python3 scripts/check-tooling-inventory.py` | passed |

## The routine floor

**141 steps, 141 PASS, 0 FAIL.**

| Step | Result |
| --- | --- |
| `floor-tree-clean` | PASS (0s) |
| `cargo-fmt` | PASS (3s) |
| `cargo-check` | PASS (1s) |
| `cargo-build-siblings` | PASS (0s) |
| `cargo-build-static-fixture` | PASS (0s) |
| `cargo-copy-static-fixture` | PASS (0s) |
| `cargo-rebuild-plain-fixture` | PASS (0s) |
| `cargo-test-workspace` | PASS (1154s) |
| `cargo-clippy` | PASS (0s) |
| `cargo-doc` | PASS (0s) |
| `cargo-test-doc` | PASS (4s) |
| `check-dependency-direction` | PASS (0s) |
| `check-global-plan-number-uniqueness` | PASS (0s) |
| `check-adr-number-uniqueness` | PASS (0s) |
| `check-portable-service-tunnel-api` | PASS (0s) |
| `check-portable-service-tunnel-consumer` | PASS (6s) |
| `planning-tests` | PASS (5s) |
| `check-runtime-boundaries` | PASS (1s) |
| `check-console-boundaries` | PASS (1s) |
| `check-console-browser-security` | PASS (0s) |
| `check-tooling-inventory` | PASS (0s) |
| `check-license-metadata` | PASS (0s) |
| `check-managed-app-private-client-seams` | PASS (0s) |
| `check-router-readiness-contract` | PASS (0s) |
| `check-router-readiness-contract-self` | PASS (0s) |
| `check-core-router-recovery-contract` | PASS (0s) |
| `check-core-router-recovery-contract-self` | PASS (0s) |
| `plan449-ssu2-preflight-self` | PASS (0s) |
| `plan495-peer-eligibility-self` | PASS (0s) |
| `plan495-peer-eligibility-profile-drift` | PASS (0s) |
| `plan476-gate-binding` | PASS (1s) |
| `plan476-gate-binding-self` | PASS (0s) |
| `check-managed-app-package-boundary` | PASS (0s) |
| `check-managed-app-package-boundary-self` | PASS (0s) |
| `check-managed-app-policy-boundary` | PASS (0s) |
| `check-managed-app-policy-boundary-self` | PASS (0s) |
| `check-managed-app-secured-sandbox` | PASS (1s) |
| `check-managed-app-secured-sandbox-self` | PASS (0s) |
| `check-service-tunnel-boundaries` | PASS (0s) |
| `check-managed-app-gateway-boundary` | PASS (0s) |
| `check-managed-app-manager-boundary` | PASS (0s) |
| `check-m11-per-epoch-composition` | PASS (0s) |
| `check-service-anonymity-boundaries` | PASS (0s) |
| `check-fixture-manifest` | PASS (0s) |
| `check-ntcp2-vectors` | PASS (0s) |
| `check-ssu2-vectors` | PASS (0s) |
| `check-i2cp-vectors` | PASS (0s) |
| `check-ntcp2-interoperability` | PASS (0s) |
| `check-constrained-host-lane-boundary` | PASS (0s) |
| `check-m11-transit-boundaries` | PASS (3s) |
| `check-m11-transit-qualification-evidence` | PASS (1s) |
| `check-sam-acceptance-evidence` | PASS (0s) |
| `check-ssu2-acceptance-evidence` | PASS (0s) |
| `check-i2cp-acceptance-evidence` | PASS (0s) |
| `check-i2pcontrol-acceptance-evidence` | PASS (0s) |
| `check-els2-type11-transcript-boundary` | PASS (0s) |
| `check-els2-live-lane-evidence` | PASS (2s) |
| `check-encrypted-service-consumer-caller` | PASS (0s) |
| `check-outproxy-request-path` | PASS (0s) |
| `check-outproxy-wire-lane-evidence` | PASS (0s) |
| `check-config-secret-hygiene` | PASS (1s) |
| `check-workflow-validity` | PASS (0s) |
| `check-floodfill-type5-serve` | PASS (0s) |
| `check-service-tunnel-acceptance-evidence` | PASS (0s) |
| `check-exploratory-tunnel-evidence` | PASS (0s) |
| `check-netdb-tunnel-evidence` | PASS (0s) |
| `check-destination-tunnel-evidence` | PASS (1s) |
| `check-streaming-tunnel-evidence` | PASS (5s) |
| `check-m6-mixed-router-acceptance-evidence` | PASS (4s) |
| `check-m12-floodfill-qualification-evidence-self` | PASS (0s) |
| `check-m12-floodfill-boundaries-self` | PASS (1s) |
| `ntcp2-harness-tests` | PASS (0s) |
| `plan488-mesh-lane` | PASS (0s) |
| `plan488-evidence-deterministic` | PASS (0s) |
| `plan489-mesh-lane` | PASS (0s) |
| `plan489-evidence-deterministic` | PASS (0s) |
| `plan490-mesh-lane` | PASS (0s) |
| `plan490-evidence-deterministic` | PASS (0s) |
| `plan490-ssu2-transport-suite` | PASS (6s) |
| `plan490-transport-boundary-guard` | PASS (1s) |
| `plan491-mesh-lane` | PASS (0s) |
| `plan491-evidence-deterministic` | PASS (0s) |
| `plan491-boundary-guard` | PASS (0s) |
| `plan491-proto-suite` | PASS (1s) |
| `plan491-netdb-suite` | PASS (87s) |
| `plan491-client-suite` | PASS (23s) |
| `plan492-posture-suite` | PASS (1s) |
| `plan492-m11-lane` | PASS (4s) |
| `plan502-transit-composition-lane` | PASS (0s) |
| `plan502-transit-composition-unit` | PASS (1s) |
| `plan502-transit-boundary-self` | PASS (0s) |
| `plan496-private-lan-profile` | PASS (0s) |
| `plan496-ntcp2-activation-gate` | PASS (0s) |
| `plan497-routing-consumer-lane` | PASS (1s) |
| `plan497-netdb-preferred-order` | PASS (0s) |
| `plan497-daemon-participation-unit` | PASS (2s) |
| `plan498-introducer-handoff-lane` | PASS (1s) |
| `plan498-introducer-mesh-lane` | PASS (1s) |
| `plan498-bootstrap-resolver-row` | PASS (24s) |
| `plan498-runtime-relay-accessor` | PASS (5s) |
| `plan503-version-eligibility-live` | PASS (1s) |
| `plan503-version-eligibility-unit` | PASS (0s) |
| `plan504-liveness-boundary` | PASS (0s) |
| `plan504-liveness-live` | PASS (1s) |
| `plan504-liveness-unit` | PASS (0s) |
| `plan505-driver-boundary` | PASS (0s) |
| `plan505-driver-live` | PASS (3s) |
| `plan505-driver-unit` | PASS (0s) |
| `plan506-reconcile-boundary` | PASS (5s) |
| `plan506-reconcile-live` | PASS (1s) |
| `plan507-retention-feasibility` | PASS (1s) |
| `plan507-retention-boundary` | PASS (0s) |
| `plan507-retention-live` | PASS (1s) |
| `plan508-creator-retention-feasibility` | PASS (0s) |
| `plan508-creator-retention-boundary` | PASS (1s) |
| `plan508-creator-retention-live` | PASS (1s) |
| `plan509-lookup-target-decision` | PASS (0s) |
| `plan509-lookup-target-boundary` | PASS (0s) |
| `plan510-managed-staging-load-lane` | PASS (1s) |
| `plan510-managed-reason-boundary` | PASS (1s) |
| `plan510-tunnelmanager-black-box` | PASS (2s) |
| `plan511-version-ceiling-derivation` | PASS (0s) |
| `plan511-version-ceiling-boundary` | PASS (1s) |
| `plan512-destination-liveness-live` | PASS (0s) |
| `plan512-destination-liveness-boundary` | PASS (0s) |
| `plan489-peer-budget-receipt` | PASS (1s) |
| `plan513-discovery-live` | PASS (2s) |
| `plan513-discovery-boundary` | PASS (0s) |
| `plan513-discovery-unit` | PASS (2s) |
| `plan513-discovery-seam-unit` | PASS (0s) |
| `plan514-exploratory-target` | PASS (7s) |
| `plan514-exploratory-target-boundary` | PASS (1s) |
| `plan516-netdb-publication` | PASS (1s) |
| `plan515-publication` | PASS (0s) |
| `plan515-publication-boundary` | PASS (1s) |
| `plan516-confirmation` | PASS (3s) |
| `plan516-confirmation-boundary` | PASS (0s) |
| `plan517-publication-retry` | PASS (4s) |
| `plan517-publication-retry-boundary` | PASS (0s) |
| `plan514-switch-unit` | PASS (0s) |
| `cargo-deny` | PASS (0s) |

### The first floor run, and the flake it reported

The floor's **first** run on this tree reported two failures, and they were
the **same** test in the **same** binary:

- `cargo-test-workspace` — the daemon's `i2pcontrol_tunnels` binary failed
  `tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets` and
  the sweep stopped there;
- `plan510-tunnelmanager-black-box` — the same binary, run focused, failed
  a **different** test, `tunnel_plan290_family_lifecycle_over_wire`.

Both failures carry the same message: *"runtime transition failed: manager
configuration rejected"*.

That is the **known** port-collision flake, and it is **not** Plan 517's
path. The test harness computes a **guess** for the tunnel's local port
(`crates/i2pr-daemon/tests/i2pcontrol_tunnels.rs:148`, `distinct_port`:
`33_000 + (pid % 5_000)`, then `+ n * 37`) instead of binding an ephemeral
port, so a collision surfaces as a configuration rejection. Plan 351's
closure record **F7** measured it at base and named the cause: TIME-WAIT
retention across consecutive runs of the same binary. Plan 491's closure
record **F4** recorded the same failure as *"an intermittent
`i2pcontrol_tunnels` managed-tunnel failure"*, *"outside the plan's path"*,
and *"recorded, not fixed"*, because it belongs to the I2PControl /
service-tunnel line.

The evidence here matches that description exactly:

- the failing test **moved** between the two runs of the same binary, which
  is what a port collision does and what a logic regression does not;
- `ss -tan` showed a `TIME-WAIT` socket at `127.0.0.1:37740`, inside the
  harness's guessed range, at the time of the failure;
- the binary then passed **17 of 17** on **three consecutive isolated
  runs** on this tree, with no source change between them;
- Plan 517's change is on the **publication** path — the selection, the
  store message, the delivery and the confirmation. It does not touch the
  tunnel manager, the port allocation, or the service product's tunnel
  staging.

**The flake is recorded, not fixed here.** Fixing the harness's port
allocation would mix an I2PControl-line defect into a publication-closure
commit, which is exactly what Plan 351's record warned against. The second
floor run is the one that counts for the closure; the first run is recorded
here in full.

The first run's workspace sweep also **stopped at the failing binary**, so
the daemon's remaining test binaries did not run in it. The second run
covered them.

## The negative controls

**14 controls, 14 caught by an assertion.**

| ID | Outcome | What the control proves |
| --- | --- | --- |
| **A** | CAUGHT | caught: the deadline must be the reference's timeout |
| **B** | CAUGHT | caught: the retry must not arm a timer of its own |
| **C** | CAUGHT | caught: the cycle's exclusion set must reach the selection |
| **D** | CAUGHT | caught: the cycle's bound must be the reference's fifteen |
| **E** | CAUGHT | caught: a confirmation must end the cycle |
| **F** | CAUGHT | caught: the cycle must never read a clock |
| **G** | CAUGHT | caught: the fan-out must stay bounded |
| **H** | CAUGHT | caught: the retry must ride the loop's existing tick |
| **I** | CAUGHT | caught: the second batch must exclude the targets already asked |
| **J** | CAUGHT | caught: a cycle must not be retried before the reference's deadline |
| **K** | CAUGHT | caught: a delivery refusal must advance at once |
| **L** | CAUGHT | caught: a retry must not move the publication's period |
| **M** | CAUGHT | caught: a batch beyond the first must be counted as a retry |
| **N** | CAUGHT | caught: the cycle must not be able to get stuck |

## Invariant review

1. **A publication is never retried before its deadline.** The deadline is
   the reference's `1600` milliseconds, measured from the moment the batch
   went out, and the loop's `100` millisecond tick observes it. *Held:* the
   lane's `a_cycle_is_not_retried_before_the_reference_deadline` and the
   guard's rule 2.
2. **A target is never asked twice in one cycle.** The cycle's exclusion set
   reaches the selection, and the selection keeps going through the version
   gate. *Held:* the lane's `the_second_batch_excludes_the_targets_already_asked`
   and the guard's rule 3.
3. **The cycle is bounded.** The reference's bound of fifteen asked targets
   holds, and the fan-out stays at the tree's three. *Held:* the lane's
   `the_cycle_is_bounded_by_the_reference_fifteen` and the guard's rules 4
   and 7.
4. **A confirmation ends the cycle.** A confirmed publication is not
   retried, and the reference's exclusion set is cleared with it. *Held:* the
   lane's `a_confirmation_ends_the_cycle` and the guard's rule 5.
5. **The retry rides the loop's existing tick.** No new timer is introduced
   and the loop's wake-up cadence does not change. *Held:* the guard's rule
   1.
6. **The cycle never reads a clock.** The reading comes from the caller, so
   the whole ladder is provable without a wall clock, a socket or a sleep.
   *Held:* the guard's rule 6 and the lane's
   `the_cycle_depends_only_on_the_reading_it_is_given`.
7. **The cycle cannot get stuck.** An outcome that leaves nothing to await
   halts the cycle, so it never holds the next period's cycle out. *Held:*
   the guard's rule 8.
8. **A retry does not move the publication's period.** The reference arms
   its publish timer once, at the cycle's start
   (`libi2pd/RouterContext.cpp:1470-1476`), so the next period is anchored
   to the cycle and not to the last rung of the ladder. *Held:* the lane's
   `a_retry_does_not_move_the_publication_period`.
9. **The version gate still holds.** Every target of every batch is selected
   through the same version-gated function, so Plan 503's API-version floor
   holds for the retry as well. *Held:* the guard's rule 3.
10. **Nothing is advertised.** `router.version` stays `0.9.62`; the
    published RouterInfo is unchanged; the inventory's advertised count
    stays zero. *Held:* the inventory's own count and the `Cargo.toml` and
    `Cargo.lock` byte-identity.

## Failure, cancellation, restart, and contention review

- **Cancellation.** A cancelled send is `Cancelled`, never confirmed, and it
  halts the cycle: the router is shutting down and there is nothing to
  await.
- **Restart.** A router that restarts loses its cycle, and a confirmation
  for a previous process is never counted: the token space is per-process.
- **Contention.** Two cycles cannot overlap. A cycle in progress holds the
  publication, and the periodic tick does not start a second one over it.
- **A poisoned lock.** The publication is refused under its own reason and
  the cycle is halted, rather than left running with nothing to await.
- **A missing local record.** The publication is refused under its own
  reason and the cycle is halted.
- **No eligible target.** The cycle ends without exhausting its bound — it
  ran out of targets, it did not reach the reference's fifteen — and the
  next period starts a fresh cycle.
- **A refused delivery.** The batch is abandoned at once, so the next batch
  is asked without waiting out the timeout.
- **Bound.** Both the cycle's exclusion set and the correlator's attempt
  book keep their own ceilings.

## Migration and compatibility review

- **No wire-format change.** The retry asks the **same** store message at a
  different peer. Nothing about the message changes.
- **No configuration key.** The cadence is a constant, like the period.
- `Cargo.toml` and `Cargo.lock` are byte-identical. No new dependency.
- The published RouterInfo's content is unchanged, so a peer that already
  holds the record sees the same record.
- Plan 515's and Plan 516's closure records are **not** rewritten, and their
  lanes and guards stay green and byte-identical.

## Security review

- **No secret is logged.** The publication's log lines carry the outcome
  enum and the transition label only; no peer identity, no key, no payload.
- **No new trust boundary.** The retry asks peers the router would have
  asked anyway, and the selection keeps going through the version gate.
- **The exclusion set is bounded.** Fifteen asked targets, and the
  correlator's attempt book keeps its own ceiling, so a cycle cannot grow
  the router's state without bound.
- **The reply gateway is unchanged.** The store message still carries the
  local hash as the gateway and the correlator's minted token, in the
  reference's direct-reply form. The retry changes **which** peer is asked,
  not **what** is sent.
- **A local condition is never written into a peer's profile.** A refusal
  or an unconfirmed batch is counted by the router's own counters only.

## Limitations

- The **stock-peer** half is blocked on Plan 493's authorized two-host
  topology: that a pinned stock floodfill answers, and that the retry
  therefore reaches one, is not proved here and is not claimed.
- The reference's **direct-or-tunnelled** choice is not implemented; the
  tunnelled branch is unreachable in this tree.
- The reference's **variance** on the publish interval is not adopted.
- The reference's **congestion** update is not implemented.
- The deadline is observed to within the loop's `100` millisecond tick's
  granularity, not to the millisecond.
- The retry's **rate** differs from the reference's: the tree's batch is
  Plan 515's fan-out of three, where the reference asks one target at a
  time.
- The retry shares the SSU2 service's loop with the relay drain and the
  inbound owner.

## Findings by severity

### Medium — open

- **M1 — The publication's delivery shares the SSU2 service's loop.** The
  refresh (Plan 515) and now the retry run on the SSU2 service's own loop,
  so a publication that has to establish a session — up to the dial timeout
  per target — delays the relay drain and the inbound owner that carries the
  confirmation. The ladder's *timing* is unaffected (the deadline is
  measured from the moment the batch went out), but the loop's other work
  waits. **Not fixed here**: separating the publication onto its own task
  needs an explicit owner for the publication's state, and the confirmation
  path owns the same state today. A successor plan should give the
  publication its own owner and its own task with explicit cancellation.

### Informational

- **I1 — The batch is three targets, not one.** The reference asks one
  floodfill at a time (`libi2pd/RouterContext.cpp:1492`). This tree asks
  the closest three in one pass, which Plan 515 already justified as not
  weaker in reach. The divergence is recorded, not imitated.
- **I2 — The bound ends the cycle.** The reference clears its exclusion set
  when the set grows past fifteen (`libi2pd/RouterContext.cpp:1495-1498`)
  and publishes again inside the same period. This tree ends the cycle and
  lets the next period start a fresh one (decision **D4**). The reference's
  behaviour is recorded here rather than imitated.
- **I3 — The first negative-controls run found the cycle could get stuck.**
  A publication whose outcome left nothing to await returned without
  arming the deadline and without ending the cycle, so the router stopped
  publishing entirely. The fix is commit `099d3fd7`; the guard gained an
  eighth rule for it and the lane's bound test was bounded in its
  iterations so that a mutation which stops the exclusion set from growing
  fails an assertion instead of spinning forever.
- **I4 — The deadline is observed to the tick's granularity.** The
  reference's `1600` milliseconds are honoured to within the loop's `100`
  millisecond tick.
- **I5 — The floor's first run reported the known `i2pcontrol_tunnels`
  port-collision flake.** It is Plan 491 **F4** and Plan 351 **F7**, it is
  on the I2PControl / service-tunnel line, and it is not Plan 517's path.
  The binary passed 17 of 17 on three consecutive isolated runs on this
  tree, and the second floor run was green. **Recorded, not fixed**: the
  harness's port guess is a separate defect with its own plan-of-record.

## Roadmap disposition

- The network-participation roadmap's row for Plan 517 moves from
  *registered* to **closed**, with the closure record named.
- Plan 516's finding **F1** — the reference's resend burst — is
  **discharged** by this plan. Plan 516's closure record is **not**
  rewritten; the disposition lives here and in the registry row.
- The roadmap's open item for a **live** stock-peer confirmation stays open:
  it is Plan 493's topology.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 491 **F1** — the publication's resend burst | **closed** by Plan 517 |
| Plan 516 **F1** — the reference's resend burst | **closed** by Plan 517 |
| Plan 491 **F2** — the stock-peer half | **still blocked** on Plan 493's authorized two-host topology |
| Plan 497 **M2** — the routing table's live reader | **not reopened**; Plan 517's selection is the router-wide store's, exactly as Plan 515's was |
| The **Medium** finding **M1** above | **open**, owned by a successor plan |
| Plan 491 **F4** — the intermittent `i2pcontrol_tunnels` failure | **still open**, and it belongs to the I2PControl / service-tunnel line. Plan 517 observed it once and did not touch that path |
| Plan 351 **F7** — the `i2pcontrol_tunnels` port guess | **still open**, and it belongs to the I2PControl / service-tunnel line |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | **still blocked** on their external environments |

Nothing in this plan unblocks a plan that was blocked on an external
environment, and nothing in this plan was done under another plan's number.
