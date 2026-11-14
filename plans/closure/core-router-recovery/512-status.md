# Plan 512 — the destination inbound pool's liveness coverage

| Field | Value |
| --- | --- |
| Plan | 512 |
| Subsystem | core-router-recovery |
| Status | closed |
| Discharges | Plan 487 **F3** (the destination inbound pool has no transit-hop failure detection) |
| Plan of record | `plans/implementation/core-router-recovery/512-destination-inbound-liveness.md` |
| Floor | `125 steps, 125 PASS, 0 FAIL` |

## Status

Plan 512 is **closed**. The mechanism is landed, the lane and the guard are in
the floor, the controls were applied to the real tree and reverted
byte-for-byte, and the routine floor is green.

Plan 487's **F3** is discharged **conditionally**, in the same sense Plan 510
discharged Plan 491's **F4**: the source-side work Plan 487 asked for is
complete, and the exact future evidence that would close it unconditionally —
a stock-peer qualification against a **real** dead transit hop — is blocked
externally on Plan 493's authorized two-host topology. The closure record
names that evidence in §"Limitations" and §"Unblock audit".

## Commits

| Commit | Subject |
| --- | --- |
| `f4c8cc3e` | docs(plans): register Plan 512, the destination inbound pool's liveness |
| `fe4137e1` | feat(daemon): give the destination inbound pool its failure detection |
| `e167ca8c` | docs(daemon): record Plan 512's mechanism, the references' evidence and the drift |
| `941082e0` | test(daemon): tighten two Plan 512 guard rules before the controls |
| `(this commit)` | docs(plans): close Plan 512 with the floor table and the conditional close |

## What changed

### The gap

Plan 487 recorded, and left open, the finding that **the destination inbound
pool has no transit-hop failure detection**. Its own words
(`plans/closure/core-router-recovery/487-status.md:378-400`):

> its destination inbound pool has no transit-hop failure detection — a dead
> transit hop is invisible to the router until the tunnel expires

and the finding's conclusion:

> a router whose destination tunnels die with a transit hop does not replenish
> them until the lease expires.

This is a **product** gap. Nothing in this plan attributes the fault to a
peer.

### What the tree already had

The gap was **not** a missing scheduler. The tree already had:

- **Plan 504's pair-level liveness scheduler**
  (`crates/i2pr-daemon/src/tunnel_liveness.rs:274`). A probe leaves through
  one tunnel and must come back through the other; the answer is correlated
  on the **acknowledged** message id; a pair that misses **two consecutive**
  responses is reported unhealthy. The scheduler is **role-agnostic**:
  `register_pair` (`:342`) checks only the pair ceiling, the slot inequality
  and the duplicate key.
- **The exploratory population**
  (`crates/i2pr-daemon/src/exploratory_build.rs:1332`
  `exploratory_liveness_pairs`), which is the *only* population the sync
  registered.
- **Every consequence of a failure**, all live before this plan:
  `DestinationRuntime::mark_tunnel_failed`
  (`crates/i2pr-client/src/registry.rs:602`), the changed lease set
  (`LeaseSetRotationCause::TunnelSetChanged`,
  `crates/i2pr-client/src/leaseset.rs:87-101`), the daemon tick's
  republication trigger
  (`crates/i2pr-daemon/src/service_product.rs:4577-4585`), the replenishment
  deficit (`crates/i2pr-client/src/registry.rs:645`), and the replacement
  build (`crates/i2pr-daemon/src/service_product.rs:6592`).

So the **detection** existed for one role and the **consequences** existed for
every role. What did not exist was the destination role's **population** and
the **routing of its failure**.

### The derivation — what the reference does, and what was taken from it

The design is the **pinned references'**, read from their source.

**i2pd 2.61.0** at the exact pin `635b013a612ff47278ef02acf8580a28e10e26c5`
(`/tmp/i2pd-ref`):

- `libi2pd/TunnelPool.cpp:400-430` — the established outbound and inbound
  lists are shuffled and paired **one to one**; there is **no shared-hop
  requirement**.
- `libi2pd/TunnelPool.cpp:428-458` — the probe is a garlic-wrapped
  tunnel-test message sent **out** through the outbound tunnel and
  **addressed to the inbound tunnel's gateway** (`:452`:
  `outbound->SendTunnelDataMsgTo(it.second->GetNextIdentHash(),
  it.second->GetNextTunnelID(), msg)`). The gateway forwards it into the
  inbound tunnel and it comes back to the local destination. **No remote
  cooperation is required.**
- `libi2pd/TunnelPool.cpp:342-345` — a pending test unanswered at the next
  tick is a miss; the **first** miss sets `eTunnelStateTestFailed`
  (`libi2pd/Tunnel.h:63`) and is not fatal; the **second** sets
  `eTunnelStateFailed` (`libi2pd/Tunnel.h:64`) and erases the tunnel from the
  pool (`:357-366`, `:374-386`) — **unless it is the last one**
  (`:360`, `:377`: *"don't fail last tunnel"*).
- `libi2pd/TunnelPool.cpp:389-393` — when the **inbound** fails,
  `m_LocalDestination->SetLeaseSetUpdated(true)` **republishes the lease
  set**.
- `libi2pd/TunnelPool.cpp:439-451` — a probe dropped locally blames the
  **outbound**; the fault is never attributed to a **hop**.

**Java I2P.** The pinned **2.13.0** artifact is **not** on this host; the
local install is **2.13.1** and the shipped jar carries **no `.java`
sources**, so the reading is **class and method from `javap`**, not
`file:line`. It is labelled as such wherever it is relied upon.
`net.i2p.router.tunnel.pool.TestJob` is the analogue of `TestTunnels`;
`TunnelPool.tunnelFailed(TunnelInfo)` calls `fail(TunnelInfo)`; and for an
**inbound, non-exploratory** pool, `fail` calls `locked_buildNewLeaseSet()`
and then `requestLeaseSet(LeaseSet)` — the direct analogue of i2pd's
`SetLeaseSetUpdated(true)`.

**What was taken:** the pair-level granularity, the two consecutive misses,
the gateway addressing, and the "don't fail last tunnel" rule.

**What was not taken, and why:**

- **No per-hop attribution.** Neither reference attributes the fault to a
  hop. The router has no per-hop delivery-success signal, and the tunnel
  layer's IV and key state are end-to-end.
- **No both-sides failure.** i2pd fails the outbound *and* the inbound of
  the pair. `i2pr` fails the **inbound** only: the recorded gap is about the
  inbound pool and its advertised lease; the inbound's failure already
  produces both a replacement and a republication, while the outbound's
  failure does not; and the outbound is the router's path to the remote
  service, whose failure is a **different** gap. The cost — a dead
  *outbound* produces repeated inbound rebuilds rather than one outbound
  rebuild — is recorded in §"Limitations", not absorbed.
- **No new wire mechanism.** The probe is the router's existing
  `DeliveryStatus` with the existing correlation id.

### The three pieces

**WP1 — the population.** A destination role's own **outbound** role is
paired with its own **inbound** role, by
`service_product::destination_liveness_pairs`, over a bounded binding table
(`destination_role_bindings`) that maps a coordinator destination-role slot
to the destination that owns it, the destination runtime's pool slot, the
direction, and the inbound's local receive id.

The pairing is **within one destination** by construction. Pairing across
destinations would let one destination's dead outbound fail another
destination's inbound — a false failure the plan must not produce. The lane
proves it.

**WP2 — the composition.** `ExploratoryBuildCoordinator::compose_destination_liveness_probe`.
The destination role's inbound tunnel does not live in the exploratory pool:
it is bound in the shared data-plane registry under the destination-role slot
namespace, so its routing comes from the registry's typed `InboundGatewayRoute`
rather than the pool's `PublicTunnelRouting`. The probe is addressed to the
inbound tunnel's **gateway**, the way i2pd addresses it, so the answer
returns through the inbound tunnel the destination advertises.

The exploratory composition keeps its **same-peer** precondition unchanged.
That rule is correct for the exploratory population — both its tunnels are
built to one peer — and would leave the destination population empty, because
the destination role's two directions are built in **separate rounds from
separate peer selections**
(`crates/i2pr-daemon/src/service_product.rs:6605-6610`).

**WP3 — the failure arm.** `ServiceProduct::fail_destination_liveness_pair`.
An unhealthy destination pair fails the destination's inbound tunnel through
the runtime's own `mark_tunnel_failed` seam, so the **existing**
republication and replenishment paths are the ones that fire. The
"don't fail last tunnel" rule is adopted verbatim, and the refusal is
**counted**.

### The safety property is inherited, not relaxed

Only an **authenticated** response proves a pair, and only a **proven** pair
may be failed (`crates/i2pr-daemon/src/tunnel_liveness.rs:581`). The
destination population inherits that gate unchanged.

This is the property that bounds the blast radius of the gateway-addressed
probe: a destination probe path that is systematically broken proves nothing,
and therefore **fails nothing**. The guard pins that there is exactly one
such gate in the scheduler's production code.

### The lane

`crates/i2pr-daemon/tests/destination_liveness_live.rs` — **15 tests**. The
lane drives the **public** daemon API only: no private internals, no
`pub(crate)` reach-ins, no socket, no environment variable, no `#[ignore]`,
and no sleeping. It proves:

1. the population pairs a destination's own outbound with its own inbound;
2. the population never pairs **across** destinations;
3. a destination with no outbound role, or an inbound with no local receive
   id, produces no pair;
4. the probe reads the inbound's gateway route from the shared registry, and
   refuses a receive id the registry does not hold;
5. the destination composition does **not** impose the exploratory same-peer
   precondition, and the exploratory composition refuses the same pair;
6. a **matching** authenticated response proves the pair;
7. a pair that has **never** answered is **never** torn down;
8. a **proven** pair that misses two consecutive responses is reported
   unhealthy, with the pair's slots;
9. the failure decision fails the destination's inbound pool slot;
10. the destination's **last** inbound tunnel is never failed, and a
    destination with none is never failed either;
11. an unbound slot is not a destination failure;
12. failing the destination's tunnel removes its lease from the pool, and the
    changed lease set is classified `Regenerate(TunnelSetChanged)` — the
    republication the daemon's tick already drives;
13. the binding table is bounded.

### The guard

`crates/i2pr-daemon/tests/destination_liveness_boundary.rs` — **16 tests**,
**15 rules**, each with an **inverted probe**. The guard is **structural**: a
rule asserts that a **named function's own body** contains the required
**call**, matched with its open parenthesis, with comments stripped and every
`#[cfg(test)]` block excluded. An absent function yields `None`, so every
caller fails closed.

| # | Rule | Detects |
| --- | --- | --- |
| 1 | `sync_liveness_pairs` calls `destination_liveness_pairs(` | a tick that never tracks the destination role |
| 2 | the population is grouped by destination | a population that zips globally |
| 3 | `drive_tunnel_liveness` calls `fail_destination_liveness_pair(` | a tick that treats a destination pair as an exploratory one |
| 4 | the arm calls `destination_liveness_failure_decision(` **and** `mark_tunnel_failed(` | a failure that never reaches the pool |
| 5 | the decision consults `inbound_len` and returns `LastTunnelRefused` | a missing last-tunnel rule |
| 6 | the composition calls `compose_outbound_build_deliveries(` and the emission calls `deliver_with_ticket(` | a private cell pipeline, or a probe that is never delivered |
| 7 | the composition reads `inbound_gateway_route(` and addresses `DeliveryInstruction::Tunnel {` | a probe that is not the reference's probe |
| 8 | the composition does **not** read `self.pool.routing(` | a probe reading the wrong routing source |
| 9 | the exploratory composition keeps `gateway != outbound_first_hop` | a weakened exploratory precondition |
| 10 | the exploratory population never mentions a destination role | an exploratory population that reports one |
| 11 | `pair.proven &&` appears **exactly once** in the scheduler's production code | a weakened or duplicated proven-pair gate |
| 12 | the binding table's comparison is `>= MAX_DESTINATION_ROLE_BINDINGS` | an unbounded binding table, or a literal bound |
| 13 | the failure decision names a pool slot and no peer identity | a peer-side fact leaking into the router's reaction |
| 14 | Plan 504's guard and lane are **byte-identical** (SHA-256 pinned) | a quietly rewritten predecessor guard |
| 15 | the lane is present and sleep-free | a lane that proves nothing |

Rule 14 is the one that makes invariant 1 *checkable* rather than asserted.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The destination role's inbound pool detects that its path stopped answering | lane: `a_proven_destination_pair_that_misses_two_consecutive_responses_is_reported_unhealthy`; guard rule 3 |
| The detection is **pair-level**, never per-hop, and never a peer's fault | lane: `a_matching_authenticated_response_proves_the_destination_pair`; guard rule 13 |
| The failure reaches the destination's pool through the **existing** seam | guard rule 4; lane: `failing_the_destination_tunnel_changes_the_lease_set_and_must_be_republished` |
| The lease set is **republished** by the existing path | lane: `failing_the_destination_tunnel_changes_the_lease_set_and_must_be_republished` (`Regenerate(TunnelSetChanged)`) |
| The destination's **last** inbound is never failed | lane: `the_last_inbound_tunnel_of_a_destination_is_never_failed`; guard rule 5 |
| The proven-pair gate governs the destination arm | lane: `an_unproven_destination_pair_is_never_torn_down`; guard rule 11 |
| The probe is the reference's probe — addressed to the inbound's gateway | lane: `the_destination_probe_reads_the_gateways_route_from_the_shared_registry`, `the_destination_composition_does_not_require_the_pair_to_share_one_peer`; guard rules 6, 7, 8 |
| The population never pairs across destinations | lane: `the_destination_population_never_pairs_across_destinations`; guard rule 2 |
| The exploratory role is unchanged | Plan 504's guard (26) and lane (9) green and **byte-identical**; guard rules 9, 10, 14 |
| The binding table is bounded | lane: `the_binding_table_is_bounded`; guard rule 12 |
| Nothing is advertised | `router.version` `0.9.62`; `caps` unchanged; `specs/support.toml` 0 advertised; `Cargo.toml`/`Cargo.lock` byte-identical |
| The reference evidence is recorded | `specs/QUALIFICATION-GATES.md` §11 |

## Commands run

The routine floor from `AGENTS.md`, run on the closing tree.

| Step | Result | Duration |
| --- | --- | --- |
| `cargo-fmt` | PASS | 4 s |
| `cargo-check` | PASS | 8 s |
| `cargo-build-siblings` | PASS | 0 s |
| `cargo-build-static-fixture` | PASS | 0 s |
| `cargo-copy-static-fixture` | PASS | 0 s |
| `cargo-rebuild-plain-fixture` | PASS | 0 s |
| `cargo-test-workspace` | PASS | 1663 s |
| `cargo-clippy` | PASS | 15 s |
| `cargo-doc` | PASS | 11 s |
| `cargo-test-doc` | PASS | 3 s |
| `check-dependency-direction` | PASS | 0 s |
| `check-global-plan-number-uniqueness` | PASS | 0 s |
| `check-adr-number-uniqueness` | PASS | 0 s |
| `check-portable-service-tunnel-api` | PASS | 0 s |
| `check-portable-service-tunnel-consumer` | PASS | 7 s |
| `planning-tests` | PASS | 7 s |
| `check-runtime-boundaries` | PASS | 1 s |
| `check-console-boundaries` | PASS | 2 s |
| `check-console-browser-security` | PASS | 0 s |
| `check-tooling-inventory` | PASS | 0 s |
| `check-license-metadata` | PASS | 0 s |
| `check-managed-app-private-client-seams` | PASS | 0 s |
| `check-router-readiness-contract` | PASS | 0 s |
| `check-router-readiness-contract-self` | PASS | 0 s |
| `check-core-router-recovery-contract` | PASS | 0 s |
| `check-core-router-recovery-contract-self` | PASS | 0 s |
| `plan449-ssu2-preflight-self` | PASS | 1 s |
| `plan495-peer-eligibility-self` | PASS | 0 s |
| `plan495-peer-eligibility-profile-drift` | PASS | 0 s |
| `plan476-gate-binding` | PASS | 0 s |
| `plan476-gate-binding-self` | PASS | 1 s |
| `check-managed-app-package-boundary` | PASS | 0 s |
| `check-managed-app-package-boundary-self` | PASS | 0 s |
| `check-managed-app-policy-boundary` | PASS | 0 s |
| `check-managed-app-policy-boundary-self` | PASS | 0 s |
| `check-managed-app-secured-sandbox` | PASS | 0 s |
| `check-managed-app-secured-sandbox-self` | PASS | 0 s |
| `check-service-tunnel-boundaries` | PASS | 0 s |
| `check-managed-app-gateway-boundary` | PASS | 0 s |
| `check-managed-app-manager-boundary` | PASS | 0 s |
| `check-m11-per-epoch-composition` | PASS | 0 s |
| `check-service-anonymity-boundaries` | PASS | 0 s |
| `check-fixture-manifest` | PASS | 0 s |
| `check-ntcp2-vectors` | PASS | 0 s |
| `check-ssu2-vectors` | PASS | 0 s |
| `check-i2cp-vectors` | PASS | 1 s |
| `check-ntcp2-interoperability` | PASS | 0 s |
| `check-constrained-host-lane-boundary` | PASS | 0 s |
| `check-m11-transit-boundaries` | PASS | 2 s |
| `check-m11-transit-qualification-evidence` | PASS | 2 s |
| `check-sam-acceptance-evidence` | PASS | 0 s |
| `check-ssu2-acceptance-evidence` | PASS | 0 s |
| `check-i2cp-acceptance-evidence` | PASS | 0 s |
| `check-i2pcontrol-acceptance-evidence` | PASS | 0 s |
| `check-els2-type11-transcript-boundary` | PASS | 0 s |
| `check-els2-live-lane-evidence` | PASS | 2 s |
| `check-encrypted-service-consumer-caller` | PASS | 0 s |
| `check-outproxy-request-path` | PASS | 0 s |
| `check-outproxy-wire-lane-evidence` | PASS | 0 s |
| `check-config-secret-hygiene` | PASS | 0 s |
| `check-workflow-validity` | PASS | 1 s |
| `check-floodfill-type5-serve` | PASS | 0 s |
| `check-service-tunnel-acceptance-evidence` | PASS | 0 s |
| `check-exploratory-tunnel-evidence` | PASS | 0 s |
| `check-netdb-tunnel-evidence` | PASS | 0 s |
| `check-destination-tunnel-evidence` | PASS | 0 s |
| `check-streaming-tunnel-evidence` | PASS | 5 s |
| `check-m6-mixed-router-acceptance-evidence` | PASS | 3 s |
| `check-m12-floodfill-qualification-evidence-self` | PASS | 0 s |
| `check-m12-floodfill-boundaries-self` | PASS | 1 s |
| `ntcp2-harness-tests` | PASS | 0 s |
| `plan488-mesh-lane` | PASS | 0 s |
| `plan488-evidence-deterministic` | PASS | 0 s |
| `plan489-mesh-lane` | PASS | 0 s |
| `plan489-evidence-deterministic` | PASS | 0 s |
| `plan490-mesh-lane` | PASS | 1 s |
| `plan490-evidence-deterministic` | PASS | 0 s |
| `plan490-ssu2-transport-suite` | PASS | 6 s |
| `plan490-transport-boundary-guard` | PASS | 0 s |
| `plan491-mesh-lane` | PASS | 1 s |
| `plan491-evidence-deterministic` | PASS | 0 s |
| `plan491-boundary-guard` | PASS | 0 s |
| `plan491-proto-suite` | PASS | 0 s |
| `plan491-netdb-suite` | PASS | 89 s |
| `plan491-client-suite` | PASS | 24 s |
| `plan492-posture-suite` | PASS | 1 s |
| `plan492-m11-lane` | PASS | 4 s |
| `plan502-transit-composition-lane` | PASS | 0 s |
| `plan502-transit-composition-unit` | PASS | 1 s |
| `plan502-transit-boundary-self` | PASS | 0 s |
| `plan496-private-lan-profile` | PASS | 0 s |
| `plan496-ntcp2-activation-gate` | PASS | 0 s |
| `plan497-routing-consumer-lane` | PASS | 1 s |
| `plan497-netdb-preferred-order` | PASS | 1 s |
| `plan497-daemon-participation-unit` | PASS | 2 s |
| `plan498-introducer-handoff-lane` | PASS | 1 s |
| `plan498-introducer-mesh-lane` | PASS | 0 s |
| `plan498-bootstrap-resolver-row` | PASS | 25 s |
| `plan498-runtime-relay-accessor` | PASS | 5 s |
| `plan503-version-eligibility-live` | PASS | 1 s |
| `plan503-version-eligibility-unit` | PASS | 0 s |
| `plan504-liveness-boundary` | PASS | 0 s |
| `plan504-liveness-live` | PASS | 1 s |
| `plan504-liveness-unit` | PASS | 0 s |
| `plan505-driver-boundary` | PASS | 0 s |
| `plan505-driver-live` | PASS | 3 s |
| `plan505-driver-unit` | PASS | 0 s |
| `plan506-reconcile-boundary` | PASS | 5 s |
| `plan506-reconcile-live` | PASS | 1 s |
| `plan507-retention-feasibility` | PASS | 1 s |
| `plan507-retention-boundary` | PASS | 0 s |
| `plan507-retention-live` | PASS | 1 s |
| `plan508-creator-retention-feasibility` | PASS | 0 s |
| `plan508-creator-retention-boundary` | PASS | 1 s |
| `plan508-creator-retention-live` | PASS | 1 s |
| `plan509-lookup-target-decision` | PASS | 0 s |
| `plan509-lookup-target-boundary` | PASS | 0 s |
| `plan510-managed-staging-load-lane` | PASS | 1 s |
| `plan510-managed-reason-boundary` | PASS | 1 s |
| `plan510-tunnelmanager-black-box` | PASS | 3 s |
| `plan511-version-ceiling-derivation` | PASS | 0 s |
| `plan511-version-ceiling-boundary` | PASS | 0 s |
| `plan512-destination-liveness-live` | PASS | 1 s |
| `plan512-destination-liveness-boundary` | PASS | 0 s |
| `cargo-deny` | PASS | 1 s |

**125 steps, 125 PASS, 0 FAIL.** The workspace test step took 1663 s; the plan's two own
steps are `plan512-destination-liveness-live` and
`plan512-destination-liveness-boundary`.


### The focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked -p i2pr-daemon --all-targets` | PASS |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test destination_liveness_live -- --test-threads=1` | 15 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --test destination_liveness_boundary -- --test-threads=1` | 16 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --test tunnel_liveness_live -- --test-threads=1` | 9 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --test tunnel_liveness_boundary -- --test-threads=1` | 26 passed, 0 failed |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `python3 -c "import tomllib; tomllib.load(open('specs/support.toml','rb'))"` | PASS (104 surfaces) |

### The eleven negative controls, verified on the real tree

Each control is a **real violation**, applied to the **real tree**, reverted
byte-for-byte, and asserted by SHA-256. The detector's build-failure matcher
is tight: it accepts a compiler error **with a code**
(`error[E0308]:`), the driver's `could not compile` / `aborting` /
`linking` messages, or an abort signal, and it does **not** accept the test
runner's own `error: test failed, to rerun pass ...` footer.

| # | Control | Caught by |
| --- | --- | --- |
| A | the live tick stops registering the destination-role population | guard, 1 failing |
| B | the population pairs one destination's outbound with another destination's inbound | guard, 1 failing **and** lane, 1 failing |
| C | the destination probe stops reading the registry's gateway route | guard, 1 failing |
| D | the destination failure arm never reaches the pool's failure seam | guard, 1 failing |
| E | the last-tunnel rule is dropped | guard, 1 failing **and** lane, 1 failing |
| F | the proven-pair gate is weakened | guard, 1 failing **and** lane, 1 failing |
| G | the failure decision carries a peer identity | guard, 1 failing |
| H | the binding table's ceiling becomes a literal | guard, 1 failing |
| I | the exploratory composition loses its same-peer precondition | guard, 1 failing |
| J | the destination probe is composed but never delivered | guard, 1 failing |
| K | the predecessor's guard file is modified | guard, 1 failing |

**11 controls, 11 caught by an assertion, 0 refused by the build, 0
vacuous.** Controls B, E and F are caught by **both** the guard and the lane,
which is what makes the two surfaces non-vacuous with respect to each other.

### Two weaknesses found while building the controls, and fixed

1. **Control G was refused by the build, not caught by an assertion.** The
   first version of G added a `peer_hash` field to the failure-decision enum
   without updating the match arm that destructures it, so the tree stopped
   compiling and guard rule 13 was never reached. A build-refusing control is
   weaker than an assertion-breaking one, so the control was corrected to
   update the pattern as well, and re-run: it is now caught by an assertion.
2. **Control J's anchor was ambiguous.** The delivery block it targeted
   appears **twice** — once in the exploratory emission and once in the
   destination emission — so the control was **skipped** rather than
   mis-applied. The anchor was widened to the destination emission's
   distinctive counter and error text, and the control now runs and is
   caught.

Neither was absorbed. The first run's results were not used as evidence; the
controls were re-run after both fixes, and only the second run is cited
above.

## Invariant review

| Invariant | Held | Evidence |
| --- | --- | --- |
| The exploratory role's behaviour is unchanged | yes | Plan 504's guard and lane byte-identical and green; guard rules 9, 10 |
| Plan 504's guard and lane stay byte-identical | yes | guard rule 14, SHA-256 pinned |
| The proven-pair gate is inherited, not relaxed | yes | guard rule 11; the gate appears exactly once |
| No peer is blamed; no peer profile is written | yes | guard rule 13; the decision type carries a pool slot and no peer identity |
| The last tunnel of a direction is never failed | yes | lane; guard rule 5 |
| No unbounded state | yes | the binding table's ceiling, the scheduler's own ceilings, the population bounded by the table |
| No new configuration key, timer, task, listener or socket | yes | the mechanism rides the existing tick; the population is derived, not configured |
| Nothing is advertised | yes | `router.version` `0.9.62`; `caps` unchanged; 0 advertised surfaces; `Cargo.toml`/`Cargo.lock` byte-identical |
| No dependency change | yes | `Cargo.toml` and `Cargo.lock` byte-identical |
| No boundary script weakened | yes | no script was touched |

## Failure review

- **A probe that cannot be emitted is abandoned, not counted.** The
  scheduler's existing `abandon_test` semantics apply unchanged: the peer
  never saw the probe, so the peer is not at fault and the test must not
  count against the pair's threshold.
- **A pair that goes away between the decision and the emission** is
  unregistered without counting a failure.
- **A destination that is stopping** fails no tunnel: the failure arm
  reaches the runtime through the existing manager seam, and a missing
  runtime is counted (`runtime_missing`).
- **A stale binding** is drained when a destination is retired, and removed
  when the role is removed by the activation-failure paths, so a stale
  binding can never fail a tunnel that has already gone.
- **A restart** resets the scheduler and the binding table, as it resets the
  exploratory population. The router re-learns the destination roles from the
  live composition.
- **Contention.** The failure arm takes the destination runtime's lock
  through the existing `with_destination_runtime` seam and never holds it
  across an await.

## Migration review

None required. No configuration key changes meaning and none is added; no
wire format changes (the probe is the router's existing `DeliveryStatus`
with the existing correlation id); no persisted state changes (the binding
table is in memory and rebuilt on start); the published RouterInfo, `caps`
and `router.version` are untouched. A router built before this plan and one
built after it interoperate: the probe is an ordinary `DeliveryStatus`, and a
peer that does not echo it simply never proves the pair, which is a no-op.

## Security review

- **No secret is exposed.** The counters name no peer, no tunnel, no address
  and no payload. The failure decision names a pool slot and nothing else.
- **No peer identity enters the reaction.** Guard rule 13 pins that the
  decision enum names no `Hash`, no `PeerId` and no address. A local
  condition is therefore never written into a peer.
- **No new wire surface.** The probe reuses the existing `DeliveryStatus`
  payload and the existing correlation id.
- **The probe is authenticated before it counts.** Only an authenticated
  response proves a pair, so a forged or mis-correlated echo cannot fail a
  tunnel.
- **The blast radius of a false failure is bounded** by the proven-pair gate
  and the last-tunnel rule: a pair that never proved is never torn down, and
  a destination's only tunnel is never failed.
- **No new listener, socket, task, timer or configuration key.**
- **No new dependency**, so no new `unsafe` exposure and no licence change.

## Documentation and operations

- `specs/QUALIFICATION-GATES.md` §11 — the references' failure-detection
  mechanism, with i2pd's `file:line` citations at the exact pin and Java's
  `javap` reading labelled with its version caveat.
- `specs/CONFORMANCE.md` — the Plan 512 section: what is and is not claimed.
- `specs/support.toml` — one new surface, `experimental`,
  `advertised = false`. The inventory now holds **104 surfaces, 98
  experimental, 5 deferred, 1 ready, 0 advertised**.
- `docs/architecture/i2pr-daemon.md` — the destination role in the
  liveness scheduler.
- `docs/architecture/i2pr-tunnel.md` — the shared registry's two routing
  sources.
- `docs/architecture/i2pr-client.md` — the destination pool's failure path.
- The daemon deep-dive's `service_product.rs` citations were **re-pointed at
  lines verified against the live source**, and its module table's counts for
  the two files this plan changed were re-corrected.

**Operations.** The mechanism is inert until a destination with both an
outbound and an inbound role exists, which requires the destination
subsystem — disabled by default and non-advertised. The counters are
reachable through `ServiceProduct::destination_liveness_snapshot`.

## Limitations

1. **The stock-peer qualification is external.** The mechanism is landed and
   proven in-tree; a qualification against a **real** dead transit hop needs
   the authorized two-host topology that Plan 493 owns, and Plan 493 is
   blocked on an unavailable environment.
2. **The failure is pair-level, never per-hop.** Neither reference attributes
   the fault to a hop, and the router has no per-hop delivery-success signal.
   A per-hop mechanism is a new protocol mechanism and needs its own
   plan-of-record.
3. **Only the inbound is failed; i2pd fails both sides.** The cost is that a
   dead *outbound* produces repeated inbound rebuilds rather than one
   outbound rebuild. The inbound's rebuild path is bounded by the
   replenishment deficit and the build concurrency, and every rebuild is a
   legitimate response to "this path is not answering", but the churn is
   real and is recorded rather than absorbed.
4. **The Java reading is 2.13.1, not the pinned 2.13.0.** The pinned artifact
   is not on this host and the shipped jar carries no `.java` sources, so
   the reading is class and method from `javap`. The **primary** derivation is
   i2pd's, at the exact pin.
5. **The probe's reachability assumption.** The destination probe is
   addressed to the inbound's gateway from the outbound's exit peer, which
   assumes the ordinary I2P `TunnelGateway` forwarding path. That is the
   reference's own model, and the proven-pair gate bounds the consequence if
   the assumption does not hold.
6. **The product's binding table is filled by the live build path**, which
   needs a socket. The table's *shape* and the decision it drives are public
   and are proved by the lane; the wiring that fills it is covered by the
   structural guard, not by an end-to-end lane.
7. **Plan 487's F2 and Plan 491's F2 remain blocked externally**, on Plan
   493's topology.

## Findings by severity

### Medium

**M1 — The destination population's pairing is a product decision that the
guard pins but the reference does not dictate.** i2pd shuffles the two lists
and pairs them one to one, which is arbitrary; `i2pr` pairs **within** one
destination, which is a stronger choice with a stated reason (a
cross-destination pair would let one destination's dead outbound fail another
destination's inbound). The choice is pinned by guard rule 2 and proved by
the lane, but it is `i2pr`'s, not the reference's, and it is recorded here so
a successor does not mistake it for a port.

### Low

**L1 — The exploratory population's staleness is latent.** The scheduler's
`register_pair` returns `Ok(())` when the outbound is already tracked, so a
pair whose *inbound* changed keeps the old inbound. The destination arm
detects and repairs this (it compares the tracked inbound with the installed
pairing and unregisters a stale pair). The **exploratory** arm does not. The
stale pair is harmless in both arms — the composition cannot find the removed
inbound's routing, so the probe is abandoned rather than counted — but it
wastes a probe per tick until the outbound is removed. Fixing the
exploratory arm would change Plan 504's behaviour, which invariant 1
forbids, so it is **recorded for a successor** rather than fixed here.

**L2 — The deep-dive's remaining drift.** The daemon deep-dive's module-table
line counts remain stale for the rows this plan did not touch, and the
`src/tunnel_liveness.rs` row is one of them. This plan re-corrected only the
two rows for the files it changed, following Plan 511's precedent. The
pre-existing drift audit remains open.

**L3 — The pre-existing unresolvable citations in the daemon deep-dive.**
Four bare-filename `file:line` citations were already unresolvable before
this plan. The `service_product.rs` citations this plan shifted have been
re-pointed at verified lines; the others are untouched and remain part of the
recorded drift.

### Recorded, not defects

**R1 — Plan 487's F2** (no stock-peer evidence) and **Plan 491's F2** are
blocked externally on Plan 493's authorized two-host topology.

**R2 — The per-component width divergence** (the comparator's `u16` bound
against Java's `long`) is recorded for a successor, as Plan 511 left it.

**R3 — The routing table's live reader** (Plan 497 limitation 5's read
half) remains open for its own successor.

**R4 — Plan 487 F3's stock-peer half** is discharged conditionally; the
exact future evidence is named in §"Unblock audit".

## Roadmap disposition

Plan 512's row in `plans/subsystems/core-router-recovery-roadmap.md` moves
to **closed**, and §12 carries Plan 512's sentence. Plan 487's registry row
carries F3's **conditional** discharge.

The **network-participation** roadmap's statement that *"Plan 487 F3 is
advanced, not closed"* is superseded by Plan 512's advance; the sentence is
left in place because the roadmap records the state of the line and Plan 512
does not close the stock-peer half.

## Unblock audit

**What Plan 512 closes unconditionally.**

- The destination inbound pool's **detection**: a destination role's pair is
  tracked, probed, and — once proven and then missed twice — the
  destination's inbound tunnel is failed, the pool's usable set changes, the
  lease set's decision becomes `Regenerate(TunnelSetChanged)`, and the
  existing republication path runs.
- The **last-tunnel** rule, the **proven-pair** gate, the **binding table**'s
  bound, and the absence of any peer attribution.
- The structural wiring: fifteen guarded rules, each with an inverted probe,
  and eleven controls applied to the real tree and reverted byte-for-byte.

**What Plan 512 closes only conditionally.**

Plan 487's **F3** as a *stock-peer* finding. The original observation was
made against a stock peer in a two-host topology; this plan reproduces the
mechanism over the tree's own deterministic surfaces, not against a stock
peer. The finding is therefore discharged **conditionally**, in the same sense
Plan 510 discharged Plan 491's **F4**.

**The exact future evidence that would close it unconditionally.**

1. A **two-host** run — the topology Plan 493 owns, with one host running
   `i2pr` and the other a pinned stock router — in which a transit peer
   carrying the destination's inbound tunnel is killed while a **different**
   transit peer keeps the path alive.
2. The observation that the **stock peer** rebuilt its own tunnels and
   republished its lease set, and that **`i2pr` republished its lease set**
   without waiting for the lease to expire — the exact step Plan 487 found
   missing.
3. The observation that the **stream resumed**, and that `i2pr` counted the
   event in its own counters.

That evidence is **external** to this repository and to this environment, and
Plan 493 is blocked on an unavailable two-host topology. No part of it is
claimed here.

**The unblock path.** Plan 493's topology becomes available → run
`bash tests/integration/m6-interop/run-tunnels.sh` (or the successor lane)
against a destination whose inbound tunnel is carried by the killed peer →
record the republication and the resumption in the evidence file → update
this closure record's status line and Plan 487's registry row from
**conditional** to **closed**. The mechanism, the lane and the guard do not
change; only the evidence does.

**Nothing outside `plans/` was updated to carry this plan's state.** No
`AGENTS.md` edit, no `README.md` edit, no `specs/support.toml` status
promotion, and no advertisement.
