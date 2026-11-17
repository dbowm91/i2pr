# Plan 525 — the router publishes the introducer set it learned

**Closure record.** The plan of record is
`plans/implementation/network-participation/525-the-router-publishes-the-introducer-set-it-learned.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 525 is **closed**. The router now **publishes** the validated introducer
set it learned, in the specification's introducer-only address form: no
`host`, no `port`, and the `ih[N]`/`iexp[N]`/`itag[N]` groups the reference
publishes.

Classification: **corrective** (the SSU2 introducer composition's publication
half). Plan 490 gave the router the other half — answering a peer's Relay
Request as the introducer — Plan 523 corrected the **form** of the introducer
set, and Plan 524 delivered the **acquisition**. This plan delivers the
**publication**: the building, the choosing and the signing. The `C`
advertisement — this router advertising its own introducer role — is a
**third**, separate matter, and this record says so plainly so that a
successor does not mistake it for a fourth half.
**Subsystem:** network-participation. **Closure date:** 2026-11-09.

Nothing is promoted: `router.version` stays `0.9.62`, `caps` and the published
RouterInfo's **content** stay unchanged, the router still advertises **no**
introducer capability of its own, the inventory's advertised count stays
**zero**, and `Cargo.toml` and `Cargo.lock` stay byte-identical.

## What the plan closed

The tree held every fact the publication needs, and each fact had exactly one
missing production caller. The plan closed six of them, and it fixed **two
latent defects** on the way that had been invisible because the path had no
production caller.

**The runtime's introducer-only builder could never succeed.**
`Ssu2RuntimeService::introducer_publication_material` existed and had no
caller. It could not have worked:

- `crates/i2pr-runtime/src/ssu2_runtime.rs:2618` — the builder now derives
  the snapshot's **evidence expiry** from the introducer set's own expiry.
  The earlier shape passed the *build time* as the evidence expiry, and the
  transport's own tested gate
  (`crates/i2pr-transport-ssu2/src/publication.rs:239`, and the test at
  `:544`) therefore withdrew **every** snapshot, so the function always
  refused with `InvalidPublication` — a reason that names a malformed
  address when the real cause is a withdrawn snapshot. The gate was never
  wrong; the value handed to it was.
- The expiry is the **earliest** among the published introducers, the
  fail-closed choice: the form is withdrawn the moment its first listed
  introducer dies, so the next publication cycle re-selects. The maximum
  would let a dead introducer sit in a published address until the whole
  set had expired.

**The runtime's install gate could not admit the form at all.**
`install_local_router_info_inner` required **every** SSU2 address the record
carries to name the router's own bound **endpoint**. The introducer-only
form has none — that is what the form *is*
(`libi2pd/RouterInfo.cpp:1377`, `:1444` omit `host` and `port` when the
router is not published directly). The gate therefore refused the record
before it could be installed.

- `crates/i2pr-runtime/src/ssu2_runtime.rs:3781` — the gate now binds the
  router's **own** identity and contact material **by the address's form**.
  The transport's parser refuses an address carrying both a direct endpoint
  and introducers (`Ssu2AddressError::HostPortWithIntroducers`), so the two
  forms are exclusive and `Ssu2RouterAddress::address_class()` is a total
  classification: a `Direct` address must still name the router's own bound
  endpoint with the router's own keys; an `IntroducerOnly` address must
  carry the router's own keys and at least one introducer; and an
  `UnpublishedStatic` address is refused, because it names no contact at
  all.

**The netdb's second rule and second builder had no production caller.**
They existed since Plan 523 and nothing called them.

- `crates/i2pr-netdb/src/local.rs:292` —
  `build_with_ssu2_introducer_address` is now the builder the daemon signs
  the introducer-only form with. It carries **no** floodfill permit, so a
  record it builds is an ordinary router's record and cannot mint `caps=f`.
- `crates/i2pr-netdb/src/floodfill_role.rs:277` —
  `is_qualified_ssu2_introducer_address` is the only authority that accepts
  the form, beside the untouched direct rule.

**The daemon never asked which form to publish.** The self-publication path
asked the runtime for the **direct** form only, so a firewalled router
received a refusal and withdrew.

- `crates/i2pr-runtime/src/ssu2_runtime.rs:2746` —
  `self_publication_material` is the **composition**. It asks for the direct
  form first and falls back to the introducer-only form **only** when the
  router's reachability evidence refuses the direct one. A refusal that is
  not about reachability — a closed runtime, an unbound runtime, a poisoned
  state, an invalid endpoint — is propagated unchanged: such a router is
  **broken**, not firewalled, and publishing an introducer-only address
  would hide the fault behind a plausible record.
- `crates/i2pr-runtime/src/ssu2_runtime.rs:1105` — `Ssu2PublicationForm` is
  the typed answer, so the daemon matches on the form and never infers one
  from a refusal.
- `crates/i2pr-daemon/src/self_publication.rs:459` — `build_record` hands
  the address to the builder that carries **its own** rule: the netdb's
  direct builder for the direct form, its introducer builder for the
  introducer-only one. The daemon names no option and assembles no address.

**The family capability was a hardcoded claim.** The builder published
`"4"` and reported `bound_family: Ipv4` whatever the router had bound.

- `crates/i2pr-runtime/src/ssu2_runtime.rs:2705` — `bound_family` derives
  the family from the router's **bound** socket, so the router never claims
  a family it has no socket for.

**The empty-introducer counter could never move.** The daemon's counters
carried a reason for the empty introducer set, and nothing could ever
produce the refusal.

- `crates/i2pr-daemon/src/self_publication.rs:181` —
  `withheld_no_introducers` is now **reachable**, and the lane proves it
  moves.
- `crates/i2pr-daemon/src/self_publication.rs:149` — the counters
  **distinguish** the two published forms and the withdrawal. `published` is
  now the **sum** of the two published forms; it used to count every
  successful install, which included the withdrawal and so did not mean
  "an address was published".
- `crates/i2pr-daemon/src/i2pcontrol_inspection.rs:94` — the counters reach
  the inspection plane, so the posture is observable on a running router.
  Before this plan the self-publication counters existed and **no production
  reader could see them**.

### The registered plan's corrections

The registered plan's §3 recorded **one** latent defect. The implementation
found a **second** one — the install gate — which §3 did not record. §3 is
evidence and is **not** rewritten; the correction lives in the plan's
amendment log (§17.2) and in this record.

The amendment log also records that Plan 520's guard **rule 2** is updated
and **strengthened** rather than weakened: the owner must ask the runtime
for the **composed** form, which is strictly stronger than asking for one
form alone, and a second inverted probe reports an owner that asks for one
form only.

## What the plan did not close

- **The `C` advertisement.** This router advertising **its own** introducer
  role is a **third**, separate matter. The reference publishes `C` on the
  **published** (reachable) form (`libi2pd/RouterInfo.cpp:1358`), so it is
  a different behaviour from this plan's, and it needs its own
  plan-of-record.
- **The withdrawal form.** A firewalled router with **no** introducer keeps
  publishing the netdb's ordinary **zero-address** record. The reference
  publishes an unpublished `s`+`i`+`v` form
  (`libi2pd/RouterInfo.cpp:1459`), and the tree's own withdrawal renderer
  (`crates/i2pr-transport-ssu2/src/publication.rs:389`) writes `s` and `v`
  only. Changing the withdrawal is Plan 520's path and needs its own
  plan-of-record.
- **The stock-peer half.** The plan's evidence is a local, in-process lane
  over a real loopback SSU2 session. Evidence against a stock peer needs the
  authorized two-host topology Plan 493 owns, and it stays blocked there.
- **The private-LAN profile.** The introducer-only form publishes no
  endpoint and is therefore safe in any profile, but the `ssu2.advertise`
  switch stays refused in the private-LAN profile. The refusal is
  deliberate: a private-LAN router's introducers are typically its LAN
  peers, and publishing them to the public network is an operator decision
  this plan does not make on the operator's behalf.
- **The pool's dial scope** (Plan 431's), the **NetDB composition** (Plan
  433's) and the **`caps=f` advertisement** (Plan 494's). None of them is
  this plan's subject.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `7fbe0b45` | the plan of record, the registry row and the roadmap row |
| `28881c1c` | the production change: the runtime's builder, the composition, the bound family, the install gate, the daemon's hand-off, the counters and the inspection seam |
| `4b035ec1` | the lane, the guard, and Plan 520's lane and guard updated for the composed form |
| `09376776` | the documentation, the support inventory, and the plan's amendment log |

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the proto crate is untouched | `git diff 7fbe0b45..09376776 -- crates/i2pr-proto/` is empty; the daemon writes no wire vocabulary, which the guard proves (`the_daemon_writes_no_option_vocabulary`) |
| 2 | the transport's parser, the option vocabulary and the publication builder are untouched | `git diff 7fbe0b45..09376776 -- crates/i2pr-transport-ssu2/` is empty |
| 3 | the netdb's two rules and two builders are untouched | `git diff 7fbe0b45..09376776 -- crates/i2pr-netdb/` is empty |
| 4 | the introducer-only builder succeeds | `crates/i2pr-runtime/src/ssu2_runtime.rs:2618`; `crates/i2pr-daemon/tests/ssu2_introducer_publication.rs`, `the_introducer_only_builder_succeeds_where_it_used_to_fail` |
| 5 | the published address carries the specification's groups and no endpoint | `crates/i2pr-daemon/tests/ssu2_introducer_publication.rs`, `the_introducer_only_builder_succeeds_where_it_used_to_fail` and `the_router_publishes_the_introducer_only_record` |
| 6 | the netdb's own introducer rule accepts it and the direct rule refuses it | `crates/i2pr-netdb/src/floodfill_role.rs:277`; `crates/i2pr-daemon/tests/ssu2_introducer_publication.rs`, `the_introducer_only_builder_succeeds_where_it_used_to_fail` and `each_builder_refuses_the_other_form` |
| 7 | the direct form is still preferred, and a broken router is not a firewalled one | `crates/i2pr-runtime/src/ssu2_runtime.rs:2746`; `crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs`, `the_composition_tries_the_direct_form_first` and `only_a_reachability_refusal_falls_through` |
| 8 | the daemon holds no copy of the publication rule | `crates/i2pr-daemon/src/self_publication.rs:459`; `crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs`, `the_daemon_matches_on_the_typed_form` |
| 9 | the install gate binds the router's own address by its form | `crates/i2pr-runtime/src/ssu2_runtime.rs:3781`; `crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs`, `the_install_gate_accepts_the_introducer_only_form` |
| 10 | the empty introducer set is refused under its own reason, and the counter moves | `crates/i2pr-daemon/src/self_publication.rs:181`; `crates/i2pr-daemon/tests/ssu2_introducer_publication.rs`, `a_router_with_no_introducer_is_refused_under_its_own_reason` and `the_empty_introducer_set_withdraws_and_is_counted` |
| 11 | the counters distinguish the forms and are observable | `crates/i2pr-daemon/src/self_publication.rs:149`; `crates/i2pr-daemon/src/i2pcontrol_inspection.rs:94`; `crates/i2pr-daemon/tests/ssu2_introducer_publication.rs`, `the_router_publishes_the_introducer_only_record` and `the_counters_are_readable_through_the_inspection_seam` |
| 12 | no configuration key is added, and the switch stays default-off | `crates/i2pr-daemon/tests/ssu2_introducer_publication.rs`, `the_publication_switch_stays_off_by_default`; `crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs`, `the_switch_stays_off_by_default` and `the_owner_adds_no_infrastructure` |
| 13 | the owner adds no task, timer, listener or socket | `crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs`, `the_owner_adds_no_infrastructure` |
| 14 | the published record carries no `f` capability | `crates/i2pr-daemon/tests/ssu2_introducer_publication.rs`, `the_router_publishes_the_introducer_only_record`; `crates/i2pr-daemon/src/self_publication.rs:459` (the introducer builder takes no permit) |
| 15 | the transport and the netdb stay runtime-neutral | `bash scripts/check-runtime-boundaries.sh` — PASS |
| 16 | the reference pins are untouched | no external router or client is patched, vendored or modified; the pins are unchanged |
| 17 | no boundary script is weakened | `git diff 7fbe0b45..09376776 -- scripts/` is empty |
| 18 | nothing is advertised | `specs/support.toml`, the `daemon.introducer-publication` surface is `advertised = false` and the inventory's advertised count is **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical |

## Focused verification

```text
cargo test --locked -p i2pr-daemon --test ssu2_introducer_publication -- --test-threads=1
    10 passed; 0 failed
cargo test --locked -p i2pr-daemon --test ssu2_introducer_publication_boundary -- --test-threads=1
    28 passed; 0 failed
cargo test --locked -p i2pr-daemon --test router_info_publication_address -- --test-threads=1
    8 passed; 0 failed
cargo test --locked -p i2pr-daemon --test router_info_publication_address_boundary -- --test-threads=1
    25 passed; 0 failed
cargo test --locked -p i2pr-runtime --all-targets
    clean
cargo check --locked --workspace --all-targets
    clean
cargo fmt --all --check
    clean
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    clean
```

## The routine floor

**88 steps, 88 PASS, 0 FAIL.** The reconciliation is
exact: the script declares its steps and the log carries one result for each,
the step names match one for one, and no step failed.

| Step | Result |
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

## The negative controls

Every guard rule has an **inverted probe** that applies a mutation and
asserts the rule **fires**. The mutations are applied to an **in-memory copy**
of the source — the probe passes a `String` literal or a mutated `String` to
the rule function — so **no file on disk is ever written**: the restoration is
byte-for-byte by construction, not by a cleanup step that could be skipped.

The guard's fourteen rules and their probes:

| Rule | The probe that fires it |
| --- | --- |
| the switch stays off by default | the default-off and the conditional-refusal checks are read from the real `config.rs` |
| the evidence expiry comes from the introducer set | `the_guard_reports_an_evidence_expiry_that_is_the_build_time` |
| the family capability is derived | `the_guard_reports_a_hardcoded_family_capability` |
| the composition tries the direct form first | `the_guard_reports_a_composition_that_tries_the_introducer_form_first` |
| only a reachability refusal falls through | `the_guard_reports_a_composition_that_falls_through_on_any_refusal` |
| the daemon matches on the typed form | `the_guard_reports_a_daemon_that_infers_the_form` |
| each form has its own builder | `the_guard_reports_a_form_built_by_the_wrong_builder` |
| the introducer builder carries no permit | `the_guard_reports_an_introducer_builder_with_a_permit` |
| the install gate accepts the introducer-only form | `the_guard_reports_a_gate_that_refuses_the_introducer_only_form` |
| the counters distinguish the forms | `the_guard_reports_counters_that_do_not_distinguish_the_forms` |
| the empty introducer reason is counted | `the_guard_reports_an_uncounted_empty_introducer_set` |
| the counters reach the inspection seam | `the_guard_reports_counters_that_never_reach_the_inspection_seam` |
| the owner adds no infrastructure | `the_guard_reports_an_owner_that_spawns_a_task` |
| the daemon writes no option vocabulary | `the_guard_reports_a_daemon_that_writes_the_option_vocabulary` |

And the fail-closed property: `the_guard_reports_a_missing_named_item`
asserts that **five** of the rules report an **absent** item rather than
passing it. A rule whose named item is missing is **reported**, not passed.

## Acceptance criteria

The plan's §14 listed eleven criteria. Each is met:

1. **the runtime's introducer-only builder succeeds where it previously
   always refused, and its evidence expiry comes from the introducer set's
   own expiry** — the lane's
   `the_introducer_only_builder_succeeds_where_it_used_to_fail` and the
   guard's rule 2;
2. **the built address carries the specification's introducer groups and no
   host and no port, and the netdb's introducer rule accepts it** — the
   lane's `the_introducer_only_builder_succeeds_where_it_used_to_fail` and
   `each_builder_refuses_the_other_form`;
3. **a router whose reachability is unqualified and that holds introducers
   publishes the introducer-only address through the netdb's introducer
   builder** — the lane's `the_router_publishes_the_introducer_only_record`;
4. **a router whose reachability admits the direct form still publishes the
   direct address, and the direct form is unchanged** — the guard's rule 4
   and the lane's `the_direct_form_is_still_published_through_the_direct_builder`;
5. **the empty introducer set is refused under its own reason, and the
   counter that names it moves** — the lane's
   `the_empty_introducer_set_withdraws_and_is_counted`;
6. **the counters distinguish the two published forms and the withdrawal,
   and they are readable through the inspection seam** — the lane's
   `the_router_publishes_the_introducer_only_record` and
   `the_counters_are_readable_through_the_inspection_seam`;
7. **the lane and the guard are green, every guard rule has an inverted
   probe that fires it, every rule fails closed on an absent item, and no
   file on disk is written by a probe** — 10 lane tests and 28 guard tests,
   and the probes mutate in-memory copies;
8. **the routine floor is green on the closing tree** — the floor section
   above;
9. **`specs/support.toml` records the surface as `experimental` and
   `advertised = false`; the advertised count stays zero** — the inventory
   reads **118 surfaces, 113 experimental, 4 deferred, 1 ready, 0
   advertised**;
10. **`router.version` stays `0.9.62`; the published RouterInfo's `caps` is
    unchanged; `Cargo.toml` and `Cargo.lock` are byte-identical** — the
   matrix rows 14 and 18;
11. **no boundary script is weakened** — `git diff 7fbe0b45..09376776 --
    scripts/` is empty.

## Invariant review

The plan's §4 listed ten invariants. Each holds:

1. **The direct form stays the preferred form.** The composition asks for it
   first; the introducer-only form is a fallback. *Guard rule 4.*
2. **The netdb's two rules stay two rules.** `git diff` over
   `crates/i2pr-netdb/` is empty; the introducer record is qualified by the
   introducer rule only. *Lane: `each_builder_refuses_the_other_form`.*
3. **No floodfill permit.** The introducer builder takes none. *Guard rule
   8; lane: `the_router_publishes_the_introducer_only_record` asserts the
   published record carries no `f`.*
4. **No host, no port.** The renderer omits them, the netdb's rule refuses
   them, and the lane asserts both. *Lane: the builder test and the
   publication test.*
5. **The publication is opt-in and default-off.** `ssu2.advertise` is
   unchanged and no key is added. *Guard rule 1.*
6. **Nothing is advertised.** `router.version` is `0.9.62`, the record's
   `caps` string is unchanged, and the inventory's advertised count is
   **zero**.
7. **Every refusal is counted under its own reason.** The netdb's two rules
   produce two distinct reasons
   (`crates/i2pr-daemon/src/self_publication.rs:94`), and the empty
   introducer set has its own. *Guard rules 10 and 11.*
8. **No new task, timer, listener or socket.** The publication rides the
   daemon's existing publication cycle. *Guard rule 13.*
9. **No secret crosses a boundary.** The material carries the router's
   **public** keys only; the counters carry counts only.
10. **The wire vocabulary is untouched.** `git diff` over
    `crates/i2pr-proto/` and `crates/i2pr-transport-ssu2/` is empty.

## Failure, cancellation, restart, and contention review

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A poisoned state mutex degrades to "no material", never to a
  fabricated one. A netdb refusal never signs a record: the record is built
  only from the material the runtime admitted, and the netdb's own rule is
  the only authority that accepts it.
- **Cancellation.** The plan adds no task of its own. The publication rides
  the daemon's **existing** publication cycle
  (`crates/i2pr-daemon/src/lib.rs:3667`), and the runtime's material
  builders are synchronous and non-awaiting. There is nothing to cancel and
  nothing to await.
- **Restart.** The introducer table is in-memory and is lost on restart, the
  same as every other session-scoped table in the tree. A router that
  restarts publishes the **withdrawal** until it has learned introducers
  again, which is the honest posture: publishing a stale introducer set
  would be worse than publishing none.
- **Contention.** The published set is bounded by the transport's own table
  ceiling, which **evicts the oldest** record when it is full
  (`crates/i2pr-transport-ssu2/src/introducer.rs:185`) and by the
  transport's own published-introducer bound. The plan adds no second bound
  and no second selection policy: the set the runtime publishes is the set
  the transport already selected.

## Migration and compatibility review

- **No wire change.** The plan renders an address in the form Plan 523 put
  there. The proto crate, the option vocabulary and the parser are
  untouched.
- **A reading change, not a writing change, for peers.** A peer that reads
  this router's record may now find an address with introducers and no
  endpoint. The tree's own parser already accepts that form (Plan 523), and
  the specification defines it.
- **A writing change for this router's own record**, behind the existing
  default-off opt-in. A router with the opt-in on, that is firewalled and
  that has learned introducers, publishes a different address shape than it
  did before. This is the plan's purpose.
- **A public API change in the runtime.** `Ssu2PublicationMaterial` gained
  a `form` field, and `Ssu2PublicationForm` is a new public enum. The
  change is **additive**: every existing construction site was updated, no
  field was removed, and no signature changed. The crate is experimental.
- **A public API change in the daemon.** The counters split, and
  `SelfPublicationFailure` gained `UnqualifiedIntroducerAddress`. The
  daemon's own `decide` and `refresh` signatures are **unchanged**, so the
  seam Plan 520's lane drives still drives.
- **No configuration migration.** No new key, no changed default, no changed
  meaning. The existing `ssu2.advertise` switch keeps its meaning and its
  refusals.
- **No persistence.** The published form is recomputed from live state on
  every publication cycle, so a restart cannot leave a stale form installed.

## Security review

- **No secret is published.** The material carries the router's **public**
  static key and **public** intro key — both are published in the address by
  design — and no private key crosses the runtime boundary.
- **The published address names peers.** The introducer groups carry the
  introducers' router hashes, expiries and relay tags, which is what the
  specification's form requires. The plan adds no peer profile, no peer
  identity and no peer score, and the counters carry counts only.
- **The install gate binds the router's own address.** A direct address must
  name the router's own bound endpoint with the router's own keys; an
  introducer-only address must carry the router's own keys and at least one
  introducer. No caller can install an address with fabricated introducer
  hashes: the self-publication owner builds the address from the runtime's
  material, whose only source is the runtime's validated introducer table,
  and the re-dating path
  (`crates/i2pr-daemon/src/router_i2np.rs:1723`) re-signs the **installed**
  record rather than building a new address. See the Informational finding
  below.
- **The opt-in is refused rather than silently inert.** `ssu2.advertise`
  still requires the SSU2 service and is still refused in the private-LAN
  profile.
- **No boundary script is weakened.** `git diff 7fbe0b45..09376776 --
  scripts/` is empty, and the floor's boundary checkers all pass.

## Limitations

- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local, in-process lane over a real
  loopback SSU2 session.
- The **withdrawal** remains the tree's zero-address record, where the
  reference publishes an unpublished `s`+`i`+`v` form. That divergence is
  Plan 520's path.
- The **`C` advertisement** is not this plan's subject.
- The **private-LAN profile** keeps its `ssu2.advertise` refusal.
- The lane proves the **introducer-only** path end to end over a real
  session, but the **direct** path's production evidence route is Plan 521's
  and Plan 435's, not this plan's; the direct form is proven here through
  the daemon's decision seam and the guard's structural rule.

## Findings by severity

### Medium — none open

No Medium finding is open. The two defects this plan fixed were found and
fixed **in** the plan, before the plan closed, and neither was
production-reachable: both sat behind a path with no production caller.

### Low — recorded, not fixed

1. **The withdrawal is not the reference's unpublished form.** A firewalled
   router with no introducer publishes the netdb's ordinary zero-address
   record, where the reference publishes an unpublished `s`+`i`+`v` form.
   **Owner:** Plan 520's path. The tree's own withdrawal renderer
   (`crates/i2pr-transport-ssu2/src/publication.rs:389`) writes `s` and `v`
   only, and changing the withdrawal would change Plan 520's shipped path.
2. **The `C` advertisement is not done.** This router still advertises no
   introducer capability of its own. **Owner:** a successor's own
   plan-of-record; it is a third, separate matter.
3. **The private-LAN profile keeps its `ssu2.advertise` refusal** even
   though the introducer-only form publishes no endpoint and would be safe
   there. **Owner:** a successor's own plan-of-record; the refusal is
   defensible on the operator's privacy grounds.
4. **There is no stock-peer evidence.** **Owner:** Plan 493's authorized
   two-host topology.

### Informational

1. **The install gate does not re-verify the published introducer set.**
   The gate binds the router's own keys and requires at least one
   introducer, but it does not check the published introducer hashes against
   the runtime's own table. The chain holds: the self-publication owner
   builds the address from the runtime's material, the runtime's material
   builder's only source is the validated introducer table, and the
   re-dating path re-signs the **installed** record rather than building a
   new address. The gate is the **identity** boundary, not the
   introducer-set boundary, and a set re-verification there would race the
   table's own expiry sweep.
2. **The family capability is a single letter.** The reference publishes
   both `4` and `6` for a dual-stack router
   (`libi2pd/RouterInfo.cpp:1362`); the tree publishes exactly one, taken
   from the bound sockets with IPv4 preferred. The tree therefore
   **under-claims** rather than over-claims, and it is consistent with the
   tree's own direct form, which also publishes one family letter.
3. **The `published` counter's meaning changed.** It counted every
   successful install, which included the withdrawal; it is now the **sum**
   of the two published forms. Nothing consumed it before this plan, so
   there is no reader to migrate.
4. **`Ssu2PublicationMaterial` gained a `form` field.** The change is
   additive: no field was removed and no signature changed.

## Roadmap disposition

The network-participation roadmap's **completion definition** requires that
the router "publishes the validated introducer set in its RouterInfo
address". Plan 523 corrected the **form** of the set, Plan 524 delivered the
**acquisition**, and Plan 525 delivers the **publication**. The completion
definition's **publishing half** is therefore **discharged** by this plan,
with one boundary stated plainly: the publication is behind the existing
default-off `ssu2.advertise` opt-in and the router's own reachability
evidence, and the evidence is a local, in-process lane rather than
stock-peer evidence, which stays blocked on Plan 493.

What the roadmap's **Transports** row still names as remaining is unchanged
except for this plan's discharge: the **publication** of the introducer set
the router learns is now **delivered**, and the row's remaining items are the
pool's **dial scope** (Plan 431's) and the **stock-peer** half (Plan 493's).

The **`C` advertisement** remains outside the roadmap's completion
definition's publishing half and needs its own plan-of-record.

## Unblock audit

The audit is the same one the line has run since Plan 488: nothing is
unblocked by this plan that an external environment does not already
unblock, and nothing is blocked by this plan that a successor could take on.

- **Plan 491 F2, Plan 497's M2, Plan 487's stock-peer half, and Plans
  519–522's stock-peer halves** remain blocked on Plan 493's authorized
  two-host topology. Plan 525's stock-peer half joins that list. Plan 521's
  stock-peer half remains blocked on Plan 493 **and** on the second ground
  that no stock router implements a router-level I2NP 231 responder.
- **Plan 431** owns the public dial scope, and its closure forbids a
  corrective plan before the prerequisite exists. Untouched.
- **Plan 433** owns the NetDB lookup composition and the OBEP consumer.
  Untouched.
- **Plan 494** owns `caps=f`. Untouched.
- **Plan 435** owns the transport's public activation. Untouched.
- The **`C` advertisement** is now the one clearly named successor in this
  lane, and it is a **third** matter rather than a fourth half of the
  composition.

No plan is renumbered, no closure record is rewritten, and no boundary script
is weakened.

## Handoff notes

- The plan's subject is the **publication** of the introducer set the router
  learned. The **acquisition** is Plan 524 and it is closed.
- The **`C` advertisement** — this router advertising its own introducer
  role — is a **third**, separate matter and needs its own plan-of-record.
- The plan **fixed** two latent defects in a path that had no production
  caller. Both fixes are in the **caller** and the **gate's form-aware
  binding**, not in the transport's gates, and the transport's own gates are
  untouched.
- The **withdrawal** is unchanged: a firewalled router with no introducer
  publishes the netdb's ordinary zero-address record.
- No configuration key was added. `ssu2.advertise` already meant "the router
  publishes its own SSU2 address", and the introducer-only form is that
  address in its other shape.
- The plan's §3 is evidence and was **not** rewritten; the second defect and
  the Plan 520 guard update live in the plan's §17.2, and the citation
  drift lives in §17.3.
