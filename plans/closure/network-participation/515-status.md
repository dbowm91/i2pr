# Plan 515 — the router republishes its own RouterInfo

**Closure record.** The plan of record is
`plans/implementation/network-participation/515-router-info-publication-refresh.md`.
This record is the authority on what the plan closed, what it did not, and
on what the evidence is.

## Status

Plan 515 is **closed**. The router's own RouterInfo is now a **refreshed**
network record rather than a one-shot announcement, and the publication no
longer depends on a service tunnel being provisioned.

Two defects, one subject:

1. **The record was never refreshed.** The router published its own
   RouterInfo **once**, from the service-tunnel provisioning path
   (`crates/i2pr-daemon/src/service_product.rs`, `publish_creator_router_info`,
   called from `provision_all_service_router_material`), and never again. A
   record that is never republished ages out of the network's view, so no
   peer could find the router however long it had been running.
2. **The publication was not a router behaviour.** It was a side effect of
   building service tunnels. The local RouterInfo was read in exactly two
   places in the whole tree — that publication, and the SSU2 service
   installing the record for handshakes — so a router with **no** service
   tunnels never published at all.

The reference republishes on a timer (`ROUTER_INFO_PUBLISH_INTERVAL = 39 *
60` seconds, `libi2pd/RouterContext.h:34`, plus up to `105` seconds of
variance, `:36`) to the **closest** floodfill it has not already asked that
cycle (`libi2pd/RouterContext.cpp:1502`), with a `1600` millisecond resend
burst (`:37`) up to fifteen floodfills (`:38`).

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; `Cargo.toml` and `Cargo.lock` stay
byte-identical; the inventory's advertised count stays **zero**.

## Scope

**Lands:**

- a **runtime-neutral publication schedule** — a period, a due test, the
  counted outcomes — with no clock of its own;
- a **router-level refresh** that republishes the router's own RouterInfo
  on that period, driven by the router's own tick;
- the **one publication path**: the one-shot publication and the refresh
  share the store construction and the delivery, and select through the
  same version-gated function;
- the **records**: the inventory surface, the conformance note, the
  qualification gate, and the deep-dive.

**Does not land:**

- the reference's **resend burst** (decision **D7**): one pass reaches
  several peers immediately where the reference reaches one and then waits;
  the divergence is recorded as a finding;
- a **configuration key** for the period: the period is a constant, the
  reference's interval;
- any change to the **published RouterInfo's** content;
- any **live** stock-peer evidence, which remains Plan 493's topology.

## Baseline at registration

Plan 515 was registered on 2026-11-02 (`c80adf86`) and its design decisions
were made explicit in an amendment the same day (`a0b7bb91`), after the
placement question had been traced to the production composition:

- the publication's **transport** is router-wide: the service product
  attaches to the router's *existing* SSU2 daemon
  (`crates/i2pr-daemon/src/lib.rs`, `ServiceProduct::start_over_existing_daemon`),
  and the router's own owner reads the installed local RouterInfo from that
  daemon;
- the selection's **store** is the router-wide **bootstrap's** store — the
  store the live discovery spine already selects from — and the version
  gate is a **free function over a store**
  (`i2pr_netdb::select_version_gated_floodfills`), so Plan 503's
  API-version floor applies to it unchanged.

**Therefore** the refresh is a router-level owner, and the coupling to
service tunnels is removed.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `c80adf86` | docs(plans): register Plan 515, the RouterInfo publication refresh |
| `a0b7bb91` | docs(plans): decide Plan 515's placement and resend questions |
| `c1068390` | feat(daemon): refresh the router's own RouterInfo on a period (Plan 515) |
| `b8b79a3f` | docs(specs): record Plan 515's RouterInfo publication refresh |

The closing commit adds this record and the registry and roadmap rows.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The schedule is due at its start and exactly one period after the last attempt | the lane's `the_first_publication_is_due_at_the_schedule_start` and `a_publication_is_due_exactly_one_period_after_the_last_attempt` |
| Every outcome is counted by its own reason, and a refusal is never a publication | the lane's `every_outcome_is_counted_by_its_own_reason` and `a_refused_publication_is_never_counted_as_a_publication` |
| The publication names the **closest** eligible floodfills | the lane's `the_plan_names_the_closest_eligible_floodfills`, computed independently from the store's records |
| A publication with no local record, or with no eligible floodfill, is refused | the lane's `a_publication_without_a_local_record_is_refused` and `a_publication_with_no_eligible_floodfill_is_refused` |
| The version gate holds for the publication | the lane's `a_floodfill_that_publishes_no_version_is_never_a_target`; the guard's rules 3 and 4 |
| The refresh is the **router's own** | the guard's rule 1 |
| The one-shot publication and the refresh share the delivery path | the guard's rule 2 |
| The fan-out stays bounded | the guard's rule 5; the lane's `the_plan_names_the_closest_eligible_floodfills` |
| The schedule never reads a clock | the guard's rule 6 |
| The publication stops when it is cancelled | the guard's rule 7 |
| Nothing is advertised | the inventory's advertised count; `router.version` unchanged; `Cargo.toml` and `Cargo.lock` byte-identical |

## Work packages and outcomes

| Work package | Outcome |
| --- | --- |
| **WP1** — the runtime-neutral schedule | landed in `crates/i2pr-daemon/src/router_info_publication.rs`: the period, the due test, the outcome type and the counters. The schedule takes `now_ms` from its caller and never reads a clock. |
| **WP2** — the placement | decided by **D4** and traced to the production composition: the transport is router-wide and the selection's store is the router-wide bootstrap's. |
| **WP3** — the periodic owner | landed as the SSU2 service's `publication_tick`, beside the relay tick. The refresh is the router's own and stops with the service's cancellation. |
| **WP4** — one publication path | landed: the creator publication goes through `publish_router_info`, the shared delivery; the store construction is the shared `router_info_store_message`; both select through `select_version_gated_floodfills`. |
| **WP5** — the guards and the records | landed: the new guard (7 rules, 8 inverted probes), the inventory surface `daemon.router-info-publication`, the conformance note, the qualification gate §14, and the deep-dive's module table, section and re-pointed citations. |
| **WP6** — the controls | landed: thirteen negative controls, each caught by an assertion, every file reverted byte-for-byte with its SHA-256 asserted. |

## Invariant review

1. **The version gate.** Every publication — the one-shot and the refresh —
   selects through `select_version_gated_floodfills`, so Plan 503's
   API-version floor holds and a refusal is counted by reason. The guard's
   rules 3 and 4 assert the rule structurally; the lane proves it.
2. **Bounded fan-out.** The publication names at most
   `PUBLICATION_FLOODFILLS` (3) targets. The guard's rule 5 asserts the
   bound; the lane's plan test asserts it too.
3. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo are unchanged; the inventory's advertised count
   stays zero; `Cargo.toml` and `Cargo.lock` are byte-identical.
4. **No direct clearnet.** The publication travels the router's own I2P
   transport, exactly as the one-shot did.
5. **Fail closed, counted.** Every outcome is named and counted: published,
   no local record, no eligible target, delivery refused, cancelled.
6. **Owned and cancellable.** The refresh is a tick inside the SSU2
   service's loop, so it is owned by that service and stops with the
   service's cancellation token. The shared delivery checks the cancellation
   before each target (the guard's rule 7).
7. **The one-shot publication still happens.** The creator's record is
   still published before the first build, to the same targets, with the
   same DSM receipt.

## Verification

### Focused runs

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | clean |
| `cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1` | 8 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1` | 15 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --test netdb_discovery_boundary -- --test-threads=1` | 24 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` | every daemon test binary green |

### The AGENTS.md routine floor

Run on the closing tree, after the tree-cleanliness step that Plan 514's
diagnosis added. The floor gains two steps over Plan 514's 134:
`plan515-publication` and `plan515-publication-boundary`.

**136 steps, 136 PASS, 0 FAIL.**

| Step | Result |
| --- | --- |
| `floor-tree-clean` | PASS (0s) |
| `cargo-fmt` | PASS (4s) |
| `cargo-check` | PASS (18s) |
| `cargo-build-siblings` | PASS (0s) |
| `cargo-build-static-fixture` | PASS (0s) |
| `cargo-copy-static-fixture` | PASS (0s) |
| `cargo-rebuild-plain-fixture` | PASS (0s) |
| `cargo-test-workspace` | PASS (1293s) |
| `cargo-clippy` | PASS (12s) |
| `cargo-doc` | PASS (7s) |
| `cargo-test-doc` | PASS (3s) |
| `check-dependency-direction` | PASS (0s) |
| `check-global-plan-number-uniqueness` | PASS (1s) |
| `check-adr-number-uniqueness` | PASS (0s) |
| `check-portable-service-tunnel-api` | PASS (0s) |
| `check-portable-service-tunnel-consumer` | PASS (6s) |
| `planning-tests` | PASS (6s) |
| `check-runtime-boundaries` | PASS (1s) |
| `check-console-boundaries` | PASS (1s) |
| `check-console-browser-security` | PASS (0s) |
| `check-tooling-inventory` | PASS (0s) |
| `check-license-metadata` | PASS (1s) |
| `check-managed-app-private-client-seams` | PASS (0s) |
| `check-router-readiness-contract` | PASS (0s) |
| `check-router-readiness-contract-self` | PASS (0s) |
| `check-core-router-recovery-contract` | PASS (0s) |
| `check-core-router-recovery-contract-self` | PASS (0s) |
| `plan449-ssu2-preflight-self` | PASS (0s) |
| `plan495-peer-eligibility-self` | PASS (0s) |
| `plan495-peer-eligibility-profile-drift` | PASS (0s) |
| `plan476-gate-binding` | PASS (0s) |
| `plan476-gate-binding-self` | PASS (1s) |
| `check-managed-app-package-boundary` | PASS (0s) |
| `check-managed-app-package-boundary-self` | PASS (0s) |
| `check-managed-app-policy-boundary` | PASS (0s) |
| `check-managed-app-policy-boundary-self` | PASS (0s) |
| `check-managed-app-secured-sandbox` | PASS (0s) |
| `check-managed-app-secured-sandbox-self` | PASS (0s) |
| `check-service-tunnel-boundaries` | PASS (0s) |
| `check-managed-app-gateway-boundary` | PASS (0s) |
| `check-managed-app-manager-boundary` | PASS (0s) |
| `check-m11-per-epoch-composition` | PASS (0s) |
| `check-service-anonymity-boundaries` | PASS (1s) |
| `check-fixture-manifest` | PASS (0s) |
| `check-ntcp2-vectors` | PASS (0s) |
| `check-ssu2-vectors` | PASS (0s) |
| `check-i2cp-vectors` | PASS (0s) |
| `check-ntcp2-interoperability` | PASS (0s) |
| `check-constrained-host-lane-boundary` | PASS (0s) |
| `check-m11-transit-boundaries` | PASS (2s) |
| `check-m11-transit-qualification-evidence` | PASS (2s) |
| `check-sam-acceptance-evidence` | PASS (0s) |
| `check-ssu2-acceptance-evidence` | PASS (0s) |
| `check-i2cp-acceptance-evidence` | PASS (0s) |
| `check-i2pcontrol-acceptance-evidence` | PASS (0s) |
| `check-els2-type11-transcript-boundary` | PASS (0s) |
| `check-els2-live-lane-evidence` | PASS (2s) |
| `check-encrypted-service-consumer-caller` | PASS (0s) |
| `check-outproxy-request-path` | PASS (0s) |
| `check-outproxy-wire-lane-evidence` | PASS (1s) |
| `check-config-secret-hygiene` | PASS (0s) |
| `check-workflow-validity` | PASS (0s) |
| `check-floodfill-type5-serve` | PASS (0s) |
| `check-service-tunnel-acceptance-evidence` | PASS (1s) |
| `check-exploratory-tunnel-evidence` | PASS (0s) |
| `check-netdb-tunnel-evidence` | PASS (0s) |
| `check-destination-tunnel-evidence` | PASS (0s) |
| `check-streaming-tunnel-evidence` | PASS (5s) |
| `check-m6-mixed-router-acceptance-evidence` | PASS (4s) |
| `check-m12-floodfill-qualification-evidence-self` | PASS (1s) |
| `check-m12-floodfill-boundaries-self` | PASS (1s) |
| `ntcp2-harness-tests` | PASS (0s) |
| `plan488-mesh-lane` | PASS (0s) |
| `plan488-evidence-deterministic` | PASS (0s) |
| `plan489-mesh-lane` | PASS (0s) |
| `plan489-evidence-deterministic` | PASS (0s) |
| `plan490-mesh-lane` | PASS (0s) |
| `plan490-evidence-deterministic` | PASS (0s) |
| `plan490-ssu2-transport-suite` | PASS (6s) |
| `plan490-transport-boundary-guard` | PASS (0s) |
| `plan491-mesh-lane` | PASS (0s) |
| `plan491-evidence-deterministic` | PASS (0s) |
| `plan491-boundary-guard` | PASS (0s) |
| `plan491-proto-suite` | PASS (1s) |
| `plan491-netdb-suite` | PASS (87s) |
| `plan491-client-suite` | PASS (24s) |
| `plan492-posture-suite` | PASS (1s) |
| `plan492-m11-lane` | PASS (4s) |
| `plan502-transit-composition-lane` | PASS (0s) |
| `plan502-transit-composition-unit` | PASS (0s) |
| `plan502-transit-boundary-self` | PASS (1s) |
| `plan496-private-lan-profile` | PASS (0s) |
| `plan496-ntcp2-activation-gate` | PASS (0s) |
| `plan497-routing-consumer-lane` | PASS (1s) |
| `plan497-netdb-preferred-order` | PASS (0s) |
| `plan497-daemon-participation-unit` | PASS (2s) |
| `plan498-introducer-handoff-lane` | PASS (1s) |
| `plan498-introducer-mesh-lane` | PASS (0s) |
| `plan498-bootstrap-resolver-row` | PASS (25s) |
| `plan498-runtime-relay-accessor` | PASS (5s) |
| `plan503-version-eligibility-live` | PASS (1s) |
| `plan503-version-eligibility-unit` | PASS (0s) |
| `plan504-liveness-boundary` | PASS (0s) |
| `plan504-liveness-live` | PASS (1s) |
| `plan504-liveness-unit` | PASS (0s) |
| `plan505-driver-boundary` | PASS (0s) |
| `plan505-driver-live` | PASS (3s) |
| `plan505-driver-unit` | PASS (1s) |
| `plan506-reconcile-boundary` | PASS (4s) |
| `plan506-reconcile-live` | PASS (1s) |
| `plan507-retention-feasibility` | PASS (1s) |
| `plan507-retention-boundary` | PASS (1s) |
| `plan507-retention-live` | PASS (0s) |
| `plan508-creator-retention-feasibility` | PASS (1s) |
| `plan508-creator-retention-boundary` | PASS (0s) |
| `plan508-creator-retention-live` | PASS (1s) |
| `plan509-lookup-target-decision` | PASS (0s) |
| `plan509-lookup-target-boundary` | PASS (1s) |
| `plan510-managed-staging-load-lane` | PASS (0s) |
| `plan510-managed-reason-boundary` | PASS (1s) |
| `plan510-tunnelmanager-black-box` | PASS (3s) |
| `plan511-version-ceiling-derivation` | PASS (0s) |
| `plan511-version-ceiling-boundary` | PASS (0s) |
| `plan512-destination-liveness-live` | PASS (0s) |
| `plan512-destination-liveness-boundary` | PASS (1s) |
| `plan489-peer-budget-receipt` | PASS (0s) |
| `plan513-discovery-live` | PASS (2s) |
| `plan513-discovery-boundary` | PASS (1s) |
| `plan513-discovery-unit` | PASS (2s) |
| `plan513-discovery-seam-unit` | PASS (0s) |
| `plan514-exploratory-target` | PASS (7s) |
| `plan514-exploratory-target-boundary` | PASS (0s) |
| `plan515-publication` | PASS (1s) |
| `plan515-publication-boundary` | PASS (0s) |
| `plan514-switch-unit` | PASS (0s) |
| `cargo-deny` | PASS (1s) |

### The controls

Thirteen negative controls, each mutating the **real** tree and each
requiring its detector to fail **with an assertion**. A control that breaks
the build is weaker than one that breaks an assertion, and is reported as a
failure.

**13 controls, 13 caught by an assertion.**

| ID | Outcome | What the control proves |
| --- | --- | --- |
| **A** | CAUGHT | caught: the schedule must be due exactly one period after the last attempt |
| **B** | CAUGHT | caught: the first publication must be due at the schedule's start |
| **C** | CAUGHT | caught: a refusal must never be counted as a publication |
| **D** | CAUGHT | caught: every outcome must be counted by its own reason |
| **E** | CAUGHT | caught: the plan must name the closest eligible floodfills, nearest first |
| **F** | CAUGHT | caught: a floodfill that publishes no version must never be a target |
| **G** | CAUGHT | caught: a publication with no local record must be refused |
| **H** | CAUGHT | caught: the fan-out must stay bounded |
| **I** | CAUGHT | caught: the schedule must not read a clock |
| **J** | CAUGHT | caught: the publication must stop when it is cancelled |
| **K** | CAUGHT | caught: the one-shot publication must use the shared delivery path |
| **L** | CAUGHT | caught: the refresh must be the router's own, driven by a tick |
| **M** | CAUGHT | caught: the refresh's selection must go through the version gate |

### The false-pass audit

Every rule in the new guard has an **inverted probe**, and every control in
the table above names the detector that must fire. The controls and the
probes overlap on purpose: the probe shows the rule fires, the control
shows it fires **on the real tree** with the mutation applied.

The restore in the controls harness **writes the file's bytes**, which
stamps a fresh mtime, so cargo rebuilds the crate after every control.
Plan 514's first floor run measured a stale artifact for exactly this
reason — the harness restored with a metadata-preserving copy, cargo
treated the file as unchanged, and the floor linked a binary that still
held the last control's mutation (Plan 514's closure record, finding
**M1**). The harness here does not repeat it, and the floor begins with a
step that fails on a dirty tree.

## Migration and compatibility

- No configuration key is required: the period is a constant, the
  reference's interval. The reference's `105` seconds of variance is **not**
  adopted — a deterministic period is testable, and the variance exists to
  de-synchronise a floodfill with the network, which a router that is not a
  floodfill has no need for.
- `Cargo.toml` and `Cargo.lock` stay byte-identical. No new dependency.
- The one-shot publication's behaviour is unchanged: the same targets, the
  same bound, the same DSM receipt, and provisioning still continues when
  it fails.

## Security and contention

- **No secret crosses the boundary.** The publication carries the router's
  own public RouterInfo, compressed, in a `DatabaseStore` — exactly as the
  one-shot did. Nothing else is published, and the counters retain no peer
  identity, address, key or payload.
- **No unbounded fan-out.** The targets are bounded by a constant.
- **The bootstrap's lock is released before anything is sent.** A
  publication that dials a peer can take seconds; the records are read
  under the lock and the lock is dropped before the delivery, so the
  bootstrap is never held across an await.
- **The publication is the router's own.** It needs no authority beyond the
  record it already installs.

## Documentation and operational changes

| File | Change |
| --- | --- |
| `specs/support.toml` | the new surface `daemon.router-info-publication` — experimental, advertised **false** |
| `specs/CONFORMANCE.md` | the section *The router republishes its own RouterInfo (Plan 515)*, and the inventory's published count (107 surfaces, 102 experimental, 4 deferred, 1 ready, 0 advertised) |
| `specs/QUALIFICATION-GATES.md` | §14, *How often the router republishes its own RouterInfo* |
| `docs/architecture/i2pr-daemon.md` | the new module's row, the three changed files' line counts, the new section, and **31** citations re-pointed by exact-content match against the live file and re-verified |

## Deviations from the plan of record

1. **The plan's §6 table lists `crates/i2pr-daemon/src/destination_tunnels.rs`
   as a changed file. It is not.** The plan's own design (**D4**) says the
   version gate is a free function over a store, and the refresh applies it
   over the router-wide bootstrap's store. The coordinator's own selection
   was therefore left **byte-identical**, which keeps Plan 513's guard
   green and keeps that plan's guard file byte-identical too.
2. **The plan's §6 table lists a configuration key as a possible change.
   There is none.** The period is a constant — the reference's interval —
   and the operator cannot change it. The plan's **D1** decision already
   allowed this; the record states it plainly rather than leaving it
   implied.
3. **The plan's §11 lists six guard rules. The guard has seven.** The
   plan's rule 3 ("the one-shot publication and the refresh share the
   selection") was **not** implementable as written: the coordinator cannot
   call the shared helper without breaking Plan 513's guard, which
   requires the coordinator's own body to contain the gated call. The rule
   was restated as the invariant it was reaching for — **both** selections
   go through the **same** version-gated function — and the guard now
   checks it across both files.
4. **The plan's §11 lists six lane tests; the lane has eight.** The two
   extra tests cover the version gate's refusal of a floodfill that
   publishes no version, and the schedule's first publication being due at
   the schedule's own start.
5. **The reference's resend burst is not adopted** (decision **D7**), and
   the divergence is recorded as a finding rather than papered over.
6. **The plan's §7 decision D1 says the period "defaults to" the
   reference's interval.** There is no default and no key: the period *is*
   the reference's interval.

## Known limitations

1. **The refresh is not exercised against a live peer.** The lane and the
   guard are structural and deterministic; the delivery itself is covered
   by the existing service-product tests, and no stock peer has answered a
   republication. The **stock-peer** half remains blocked on Plan 493's
   authorized two-host topology.
2. **The reference's resend burst is not implemented.** A peer that refuses
   the publication is counted and retried at the next period, not at the
   reference's `1600` millisecond cadence.
3. **The period is not configurable.**
4. **The refresh runs inside the SSU2 service's loop.** A router with no
   SSU2 transport has no way to publish, which is correct — but it means the
   refresh's cadence is the SSU2 service's cadence, and a paused SSU2
   service pauses the publication with it.

## Findings by severity

### Medium — open

**F1 — The reference's resend burst is not adopted, and the fan-out is
three rather than fifteen.** The reference asks **one** floodfill and then,
every `1600` milliseconds, the next closest ones, up to fifteen
(`libi2pd/RouterContext.h:37`, `:38`). This tree asks the closest three in
**one** pass, once per period. One pass is not weaker in reach — it asks
three peers immediately where the reference asks one and then waits — but
it does not re-ask on failure, and it asks three where the reference may
reach fifteen. A successor plan may adopt the resend burst; the fan-out
bound is the tree's own and is asserted by the guard.

### Informational

**I1 — The publication's fan-out constant moved.** The one-shot's private
`CREATOR_RI_PUBLICATION_FLOODFILLS` is gone; the shared
`PUBLICATION_FLOODFILLS` in the new module replaces it, so the one-shot and
the refresh cannot disagree about how many peers a publication reaches.

**I2 — The creator publication's authority helper is gone.** The shared
dial builds the authority itself, so the one-shot and the refresh name the
peer the same way: explicitly, never derived from a gateway, a path role or
a tunnel identifier.

**I3 — The records commit's citation count was corrected before
publication.** The commit message claimed **40** re-pointed citations in
`docs/architecture/i2pr-daemon.md`; the committed diff re-points **31**. The
count was verified twice before the commit was amended: the diff's citation
multiset (32 tokens removed, 32 added, **31** genuinely re-pointed) and the
two files' citation multisets (103 citations before and after, **31** of the
old ones gone). The commit was amended rather than replaced, because the
branch is 160 commits ahead of its upstream and nothing is pushed, and the
tree was asserted identical across the amend. The correction is here so the
commit's own claim and this record agree.

## Roadmap disposition

The network-participation roadmap's row for this plan moves from
**registered** to **closed**. The roadmap's completion definition — "a peer
can find `i2pr` through the DHT" — gains its **publishing** half: the
router's own record is now refreshed, and a router with no service tunnels
publishes at all. The **lookup** half remains Plan 493's topology.

## Unblock audit

| Open item | Owner | What would close it |
| --- | --- | --- |
| **F1** — the resend burst and the fan-out width | a successor plan | a cadence that re-asks on failure at the reference's `1600` millisecond interval, proved over the tree's own surfaces |
| the **stock-peer** half | Plan 493 | an authorized two-host topology with a stock peer that answers a republication |
