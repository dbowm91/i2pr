# Plan 526 — the router advertises its own introducer role

**Closure record.** The plan of record is
`plans/implementation/network-participation/526-the-router-advertises-its-own-introducer-role.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 526 is **closed**. The router now **advertises its own introducer role**:
the specification's Relay capability — the `"C"` letter — rides the `caps`
option of the router's **own** SSU2 address, and it rides it **only** while the
router actually serves the role.

Classification: **corrective** (the SSU2 introducer composition's advertisement
half). Plan 490 gave the router the role itself, Plan 498 made the role owner
reachable in a live daemon, Plan 499 made it work, Plan 523 corrected the
**form** of the introducer set, Plan 524 delivered the **acquisition** and Plan
525 the **publication**. This plan delivers the **advertisement**, and — with
it — the role's own reachability, because a router that advertises a role it
does not serve makes a claim it cannot honour.
**Subsystem:** network-participation. **Closure date:** 2026-11-10.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, the published `caps` string changes **only**
behind the existing default-off `ssu2.advertise` opt-in, and `Cargo.toml` and
`Cargo.lock` stay byte-identical.

## What the plan closed

**The router advertises the introducer role.** The specification's Relay
capability is a `"C"` in the address's `caps` option ("Relay — As advertised
with a `"C"` in the `"caps"` parameter"), and the option's alphabet is
`[B,C,4,6]`. The tree now publishes it.

**The letter rides the direct form, and only the direct form.** The reference
publishes the role letters on the **published** form only
(`libi2pd/RouterInfo.cpp:1350`-`:1373`):

```cpp
                if (isPublished)
                {
                    if (address.IsPeerTesting ()) caps += CAPS_FLAG_SSU2_TESTING;
                    if (address.IsIntroducer ()) caps += CAPS_FLAG_SSU2_INTRODUCER;
                }
                else
                {
                    if (address.IsV4 ()) caps += CAPS_FLAG_V4;
                    if (address.IsV6 () || address.host.is_v6 ()) caps += CAPS_FLAG_V6;
```

and its peer-side check demands an endpoint for an introducer
(`libi2pd/RouterInfo.cpp:1059`):

```cpp
                return addr && addr->IsIntroducer () && !addr->host.is_unspecified () && addr->port;
```

A router that publishes no host and no port is **not** an introducer to a
reference peer, however its `caps` read. The tree's introducer-only form
publishes no endpoint, so the letter never rides it — and the netdb's own
introducer rule, which refuses any role letter, enforces the exclusion without
being weakened.

**The tree publishes the combination, not the reference's role-only form.** The
reference's published form carries `"C"`; the tree's carries `"4C"`, because the
tree's stricter rule requires the capability string to name the address's own
family. Both are inside the specification's alphabet, and both references parse
a combination: i2pd's `ExtractAddressCaps` is a per-character switch
(`libi2pd/RouterInfo.cpp:562`-`:581`) and the pinned Java I2P 2.13.0 reads the
capability with a containment test
(`router/java/src/net/i2p/router/transport/udp/UDPTransport.java:2588`,
`:4185`).

**The letter is a claim, so it is published only while the role is live.** The
runtime owns that fact (`Ssu2RuntimeService::introducer_service_enabled`) and
owns the wire, so it composes the capability in one place: the family letter,
plus the role letter **only** while the role is live.

**A stale letter cannot reach the wire.** The runtime's install gate refuses a
record whose SSU2 address advertises the role while the role is not live, so
the installed record cannot stand with a claim the router has stopped making.

**The netdb gains a third rule and a third builder**, beside the untouched
direct and introducer rules. The third rule is **exact** — the address's own
family letter followed by the role letter — so `"BC"`, `"4CB"`, `"46C"` and
the bare family letters are all refused and an unearned role cannot reach the
wire through a permissive rule. The third builder carries **no** floodfill
permit, so a floodfill router's address can never carry the role letter, which
matches the reference (`libi2pd/RouterContext.cpp:655`).

**The daemon publishes the third form and counts it under its own reason.** The
owner matches on a **typed** published form, hands the address to the builder
that carries that form's own rule, and counts the three published forms — the
direct form, the **role-bearing** direct form and the introducer-only form —
separately. The third rule's refusal has its own reason and its own counter.

### The registered plan's corrections

**The plan's registration recorded a latent defect in Plan 499's live path,
and the plan fixed it.** The registration's §3.6 recorded that the daemon's
composition refused the SSU2 service outright whenever
`ssu2.introducer_service` was set:

```rust
                    // Plan 520: the router's **own** address publication is
                    // behind an explicit opt-in and is no longer refused
                    // here. The introducer role still is.
                    if ssu2_config.introducer_service {
```

so the role owner the composition constructs a few hundred lines further down
was **unreachable in every configuration**, and Plan 499's closed record —
"the daemon's live loop then constructs the role owner and the carriage" — was
false of the tree. The blanket refusal is replaced by the role's own explicit,
default-off opt-in. Plan 499's closed record is **not** rewritten; the
correction lives in this record and in the registry row.

**Two guard files were extended, explicitly.** The readiness contract's control
that pinned the blanket refusal was repinned at the place where the invariant
is now enforced — the runtime's install gate — and a second control was added
for the opt-in that now gates the role's construction. Plan 520's guard and
Plan 525's guard each carried a rule that named a location the plan moved: the
family-capability composition and the owner's install. Both rules were
**extended**, not weakened: each now fails closed when the item it names is
absent, and each names the new location as well as the old one.

## What the plan did not close

- **The `B` (peer-test) advertisement.** The specification's other role letter.
  The tree's peer test is Plan 521's opt-in, and advertising it is a separate
  matter with its own plan-of-record.
- **The `caps=f` advertisement.** Plan 494 owns it and it is blocked on Plan
  493's two-family evidence.
- **The withdrawal form.** Unchanged: a firewalled router with **no** introducer
  keeps publishing the netdb's ordinary **zero-address** record, where the
  reference publishes an unpublished `s`+`i`+`v` form. That divergence belongs
  to Plan 520's path.
- **The stock-peer half.** The plan's evidence is a local, in-process lane, plus
  one **black-box** row that drives the shipped `i2pr run` executable. Evidence
  against a stock peer needs the authorized two-host topology Plan 493 owns, and
  it stays blocked there.
- **A publication-capable profile.** The advertisement rides the published
  address, and `ssu2.advertise` is refused outside the controlled loopback
  profile. The `C` is therefore a **local qualification claim** and not yet a
  network-reachable one. A profile that can publish to the public network is
  Plan 493's and Plan 494's territory.
- **The pool's dial scope** (Plan 431's), the **NetDB composition** (Plan 433's)
  and the **`caps=f` advertisement** (Plan 494's). None of them is this plan's
  subject.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `cacb2a3c` | the plan of record, the registry row and the roadmap row |
| `7501f8c1` | the production change: the role's reachability, the runtime's capability composition, the install gate's role check, the netdb's third rule and third builder, the daemon's typed form, its counters and its apply seam |
| `40744fe1` | the lane, the guard, Plan 520's and Plan 525's guards extended, and the readiness contract repinned |
| `93a3d3ef` | the documentation, the support inventory, and the plan's amendment log |

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the transport's option vocabulary, its parser and its publication builder are untouched | `git diff cacb2a3c..93a3d3ef -- crates/i2pr-transport-ssu2/` is empty; the transport already modelled the `C` letter (`Ssu2Capabilities::RELAY`) and parsed a combination per character |
| 2 | the netdb's **first** two rules and builders are untouched | `crates/i2pr-netdb/src/floodfill_role.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `the_third_rule_is_exact` |
| 3 | the role letter rides the direct form only | `crates/i2pr-runtime/src/ssu2_runtime.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `only_the_direct_form_carries_the_role_letter` |
| 4 | the letter is published only while the role is live | `crates/i2pr-runtime/src/ssu2_runtime.rs` (`capabilities_for`); `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_published_capability_carries_the_role_letter_only_while_the_role_is_live` |
| 5 | the introducer-only form never carries the role letter | `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_introducer_only_form_never_carries_the_role_letter`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `only_the_direct_form_carries_the_role_letter` |
| 6 | a stale role letter cannot reach the wire | `crates/i2pr-runtime/src/ssu2_runtime.rs` (`install_local_router_info_inner`); `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_install_gate_refuses_a_stale_role_letter` and `the_install_gate_accepts_a_role_letter_while_the_role_is_live` |
| 7 | the netdb's third rule accepts the role-bearing address and the first two refuse it | `crates/i2pr-netdb/src/floodfill_role.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_netdb_third_rule_accepts_the_role_bearing_address` and `the_netdb_third_rule_is_exact` |
| 8 | the netdb's third builder signs it and the first two refuse it | `crates/i2pr-netdb/src/local.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_third_builder_signs_the_role_bearing_address_and_the_first_two_refuse` |
| 9 | the third builder carries no floodfill permit | `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `the_third_builder_carries_no_permit` |
| 10 | the daemon matches on a typed form and each form has its own builder | `crates/i2pr-daemon/src/self_publication.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `each_form_has_its_own_builder` |
| 11 | the three published forms are counted separately | `crates/i2pr-daemon/src/self_publication.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_router_publishes_the_role_bearing_record_and_counts_it` and `a_router_without_the_role_publishes_no_role_letter` |
| 12 | the third rule's refusal has its own reason | `crates/i2pr-daemon/src/self_publication.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_role_rule_s_refusal_is_counted_under_its_own_reason` |
| 13 | the role is reachable in the composition behind its opt-in | `crates/i2pr-daemon/src/lib.rs`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_daemon_starts_with_the_introducer_role_enabled`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `the_role_is_reachable_behind_its_opt_in` |
| 14 | the role's switch stays off by default and is refused without the SSU2 service | `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_introducer_role_switch_stays_off_by_default`; `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `the_switch_stays_off_by_default` |
| 15 | the owner adds no task, timer, listener or socket, and the configuration gains no new key | `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `the_owner_adds_no_infrastructure` |
| 16 | the daemon writes no SSU2 option vocabulary | `crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`, `the_daemon_writes_no_option_vocabulary` |
| 17 | the published record carries no `f` capability | `crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs`, `the_router_publishes_the_role_bearing_record_and_counts_it`; `crates/i2pr-netdb/src/local.rs` (the third builder takes no permit) |
| 18 | the transport and the netdb stay runtime-neutral | `bash scripts/check-runtime-boundaries.sh` — PASS |
| 19 | the reference pins are untouched | no external router or client is patched, vendored or modified; the pins are unchanged |
| 20 | no boundary script is weakened | the readiness contract's control was **replaced** by two stronger controls and its mutation self-test still proves every control has teeth; no other script changed |
| 21 | nothing is advertised | `specs/support.toml`, the `daemon.introducer-advertisement` surface is `advertised = false` and the inventory's advertised count is **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo test --locked -p i2pr-daemon --test ssu2_introducer_advertisement -- --test-threads=1` | PASS — 15 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_introducer_advertisement_boundary -- --test-threads=1` | PASS — 24 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_introducer_publication -- --test-threads=1` | PASS — 10 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_introducer_publication_boundary -- --test-threads=1` | PASS — 28 rows |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_address -- --test-threads=1` | PASS — 8 rows |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_address_boundary -- --test-threads=1` | PASS — 25 rows |
| `cargo test --locked -p i2pr-daemon --all-targets --no-fail-fast -- --test-threads=1` | PASS — 152 test binaries, 0 failures |
| `python3 scripts/check-router-readiness-contract.py` | PASS |
| `python3 scripts/check-router-readiness-contract.py --self-test` | PASS |

## The routine floor

The floor ran on the **closing** tree, and every step is recorded in
`/tmp/526-floor.log` with its own log under `/tmp/526f-<step>.log`. The floor
is a plain sequence of `run` steps that records each step's exit status; no
step is skipped, no `|| true`, no `continue-on-error`, and no filename
filtering.

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
| `plan526-introducer-advertisement-lane` | PASS |
| `plan526-introducer-advertisement-boundary` | PASS |
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

**FLOOR PASS 90 — FLOOR FAIL 0.**

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be reported.
A rule with no probe is a rule nobody has shown to fire. Every probe mutates an
**in-memory copy** of the source, so no file on disk is written.

| Rule | Probe |
| --- | --- |
| the role's switch stays off by default | the missing-item probe |
| the role is reachable behind its opt-in | `the_guard_reports_a_composition_that_refuses_the_role` |
| the role letter is composed only while the role is live | `the_guard_reports_a_role_letter_that_is_not_gated` |
| only the direct form carries the role letter | `the_guard_reports_a_role_letter_on_the_introducer_only_form` |
| the third rule is exact | `the_guard_reports_a_third_rule_that_is_not_exact` |
| the third builder carries no permit | `the_guard_reports_a_third_builder_with_a_floodfill_permit` |
| the install gate refuses a stale role letter | `the_guard_reports_an_install_gate_that_accepts_a_stale_role_letter` |
| each form has its own builder | `the_guard_reports_a_form_without_its_own_builder` |
| the counters distinguish the forms | `the_guard_reports_counters_that_do_not_distinguish_the_forms` |
| the third rule has its own reason | `the_guard_reports_a_third_rule_without_its_own_reason` |
| the owner adds no infrastructure | `the_guard_reports_an_owner_that_adds_infrastructure` |
| the daemon writes no option vocabulary | `the_guard_reports_a_daemon_that_writes_option_vocabulary` |

**The guard fails closed on an absent item.** `the_guard_reports_a_missing_named_item`
drives every rule with a source that names nothing, and every rule is
**reported** rather than passed. A rule whose named item is missing is a
failure, not a silence.

**The readiness contract's self-test still has teeth.**
`python3 scripts/check-router-readiness-contract.py --self-test` mutates every
control's needle and requires the checker to fail each time, so the two new
controls cannot be satisfied by a stale copy of the old ones.

## Acceptance criteria

| # | Criterion | Evidence | Result |
| --- | --- | --- | --- |
| 1 | the introducer role is **reachable** in the composition behind its explicit, default-off opt-in, and the service still starts | `the_daemon_starts_with_the_introducer_role_enabled`; `the_role_is_reachable_behind_its_opt_in` | pass |
| 2 | a router whose role is live publishes the role letter alongside its family letter, and a router whose role is not live publishes **no** role letter | `the_published_capability_carries_the_role_letter_only_while_the_role_is_live`; `the_router_publishes_the_role_bearing_record_and_counts_it`; `a_router_without_the_role_publishes_no_role_letter` | pass |
| 3 | the role letter rides the **direct** form only, and the introducer-only form never carries it | `the_introducer_only_form_never_carries_the_role_letter`; `only_the_direct_form_carries_the_role_letter` | pass |
| 4 | the netdb's **third** rule accepts the role-bearing address and the **third** builder signs it, while the first rule, the first builder, the introducer rule and the floodfill builder all still refuse it | `the_netdb_third_rule_accepts_the_role_bearing_address`; `the_netdb_third_rule_is_exact`; `the_third_builder_signs_the_role_bearing_address_and_the_first_two_refuse`; `the_third_builder_carries_no_permit` | pass |
| 5 | a role-bearing record is **refused** by the install gate while the role is not live | `the_install_gate_refuses_a_stale_role_letter`; `the_install_gate_accepts_a_role_letter_while_the_role_is_live` | pass |
| 6 | the counters distinguish the three published forms and the withdrawal, and the role rule's refusal has its own reason | `the_router_publishes_the_role_bearing_record_and_counts_it`; `the_role_rule_s_refusal_is_counted_under_its_own_reason`; `the_counters_distinguish_the_forms` | pass |
| 7 | the lane and the guard are green, every guard rule has an inverted probe that fires it, every rule fails closed on an absent item, and no file on disk is written by a probe | the lane's 15 rows and the guard's 24 rows; `the_guard_reports_a_missing_named_item` | pass |
| 8 | the routine floor is green on the closing tree, and the readiness contract's self-test still gives every control teeth | the floor table above; `router-readiness-self-test` | pass |
| 9 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero** | `specs/support.toml` — 119 surfaces, 114 experimental, 4 deferred, 1 ready, 0 advertised | pass |
| 10 | `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` are byte-identical | `git diff cacb2a3c..93a3d3ef -- Cargo.toml Cargo.lock` is empty | pass |
| 11 | no boundary script is weakened | the readiness contract's control was **replaced** by two stronger controls; its mutation self-test passes; no other script changed | pass |

## Invariant review

1. **The direct rule stays the direct rule.** `is_qualified_ssu2_address` and
   `build_with_ssu2_address` are unchanged, and the guard proves the direct
   rule's body still carries no role letter.
2. **The introducer rule stays the introducer rule.** The introducer-only form
   never carries the role letter, and the rule that refuses it is unchanged.
3. **The floodfill path never carries the role letter.** The floodfill builder
   qualifies with the direct rule and is unchanged.
4. **The role letter is advertised only while the role is live.** The runtime
   owns the fact and the wire.
5. **A record that carries the role letter is never installed while the role is
   not live.** The runtime's install gate refuses it.
6. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   `caps` string changes only behind the existing default-off opt-in, and the
   inventory's advertised count stays **zero**.
7. **Every refusal is counted under its own reason.** The role rule's refusal
   is not the direct rule's refusal.
8. **The role stays default-off.** `ssu2.introducer_service` keeps its default
   and its parse-time refusals.
9. **No new configuration key.** The role's switch is `ssu2.introducer_service`
   and the publication's switch is `ssu2.advertise`.
10. **No new task, timer, listener or socket.** The advertisement rides the
    daemon's existing publication cycle.
11. **No secret crosses a boundary.** The published material carries the
    router's **public** keys only.
12. **The wire vocabulary is untouched.** The plan renders an address; it
    defines no block, no option and no codec.

## Failure, cancellation, restart, and contention review

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A netdb refusal never signs a record, and a record carrying a
  role letter the runtime refuses is never installed.
- **Cancellation.** The plan adds no task of its own. The publication rides the
  daemon's **existing** publication cycle, and the runtime's material builders
  are synchronous and non-awaiting.
- **Restart.** The role's liveness is a configuration fact, so a restart with
  the opt-in on resumes advertising the role, and a router that restarts with
  the opt-in off publishes no role letter. There is no stale state to
  reconcile, and no persisted role claim.
- **Contention.** The plan adds no queue, no task and no lock of its own. The
  role's liveness is read from the runtime's existing configuration.

## Migration and compatibility review

- **No wire change.** The plan renders a capability string the specification
  already defines. The proto crate, the option vocabulary and the parser are
  untouched, and `git diff` over `crates/i2pr-transport-ssu2/` is empty.
- **A reading change for peers.** A peer that reads this router's record may now
  find the role letter in the `caps` string. Both references parse it, and the
  specification's alphabet contains it.
- **A writing change for this router's own record**, behind the existing
  default-off opt-in and only while the role is live. A router with
  `ssu2.advertise = true` and `ssu2.introducer_service = true` publishes a
  different `caps` string than it did before. This is the plan's purpose.
- **A public API change in the runtime and the daemon.** The material grows a
  typed field, the runtime gains `advertised_capabilities`, the netdb gains a
  rule and a builder, the owner gains `apply`, and the counters split. The
  crates are experimental and the lanes are updated with the plan; no external
  consumer exists.
- **No configuration migration.** No new key, no changed default, no changed
  meaning. `ssu2.introducer_service` keeps its default and its refusals.
- **No persistence.** The published form is recomputed from live state on every
  publication cycle.

## Security review

- **The letter is a claim, and the claim is honoured.** The router advertises
  the role only while the role owner is running, and the runtime's install gate
  refuses a role-bearing record while it is not. A peer that reads the letter
  can rely on it, which is the whole point of the letter.
- **The role is opt-in and default-off.** An ordinary deployment constructs no
  role owner and advertises no letter.
- **The advertisement is refused outside the controlled loopback profile.**
  `ssu2.advertise` is refused there, so the letter is confined to a local
  qualification profile.
- **The netdb's third rule is exact.** No arrangement other than the family
  letter and the role letter is accepted, so a peer cannot talk this router into
  an unearned role.
- **The floodfill path cannot carry the letter.** Its builder qualifies with
  the direct rule, which refuses the role-bearing address.
- **No secret crosses a boundary.** The published material carries the router's
  **public** keys only; the private key never leaves the runtime.
- **No secret is logged.** The counters are counts and stable reason labels
  only. No key, tag, peer hash, address or payload is counted or printed.
- **The counters carry counts only.** The third published form and its refusal
  add counts and a stable reason label, nothing else.

## Limitations

1. **The stock-peer half stays blocked** on Plan 493's authorized two-host
   topology. The plan's evidence is a local, in-process lane plus one
   black-box row that drives the shipped executable.
2. **The advertisement is a local qualification claim.** The letter rides the
   published address, and publication is confined to the controlled loopback
   profile, so the router that advertises the role in the current tree binds
   loopback. The letter is correct; the *reachability* of the router that
   carries it is not yet.
3. **The runtime's direct form still needs three corroboration classes** to be
   published at all, and no lane in this tree earns them: the explicit bind,
   the peer's observed external address and the confirmed peer test. The
   runtime's capability composition is therefore exercised through the
   **install gate** — which reads the published address's own capability — and
   the daemon's hand-off through the owner's **apply** seam. The tree's own
   Plan 520 lane has the same limitation for the same reason.
4. **The `B` advertisement is out of scope**, and the `caps=f` advertisement
   stays Plan 494's.

## Findings by severity

### Medium — none open

None.

### Low — recorded, not fixed

1. **The runtime's direct form is not published in any lane.** The `Reachable`
   state needs three distinct corroboration classes and no lane in this tree
   earns them. The advertisement's mechanism is therefore proved through the
   install gate and the apply seam rather than through a live published
   address. **Owner:** the next plan that needs a live `Reachable` router —
   which is the plan that earns the third class in a lane, and the plan that
   owns a publication-capable profile.
2. **The `C` is a local claim.** The advertised router binds loopback, so a
   stock peer could not reach it anyway. **Owner:** Plan 493's two-family
   qualification and Plan 494's publication.

### Informational

1. **Plan 499's closed record overstated the role's reachability.** The record
   says the daemon's live loop constructs the role owner; the source refused
   the role outright. The closed record is **not** rewritten; the correction
   lives in this record and in the registry row.
2. **Two guard files were extended, explicitly and by name.** The readiness
   contract's control was replaced by two stronger controls, and Plan 520's and
   Plan 525's guards each had one rule extended to follow a moved invariant.
   Each extension fails closed on an absent item, and the mutation self-tests
   still prove the controls have teeth.
3. **The transport needed no change.** The capability type already carried the
   `C` letter and the parser already accepted a combination, so the tree's
   transport crate is byte-identical.
4. **The plan's own production change moved some of its citations.** The
   registration's §3 is evidence and is **not** rewritten. The plan's
   amendment log carries an additive pointer table (§17.3) for every citation
   the change moved, and a verified list of the citations that did **not**
   move.

## Roadmap disposition

The network-participation roadmap's **completion definition** requires that the
router "publishes the validated introducer set in its RouterInfo address".

- **Plan 523** corrected the **form** of that set — closed.
- **Plan 524** delivered the **acquisition** — closed.
- **Plan 525** delivered the **publication** — closed.
- **Plan 526** delivered the **advertisement** — closed.

With this plan the SSU2 introducer composition is **complete on the router's
side**: the router learns that a peer is an introducer, asks it for a relay
tag, publishes the introducers it learned, and advertises that it is itself an
introducer. The **stock-peer** half of every one of those remains blocked on
Plan 493's authorized two-host topology, and the `caps=f` advertisement remains
Plan 494's.

## Unblock audit

| Item | Status | Note |
| --- | --- | --- |
| Plan 493 — the two-family public qualification | **blocked** | on the authorized two-host topology and Plan 435's transport activation; unchanged by this plan |
| Plan 494 — the floodfill role and the `caps=f` advertisement | **blocked** | on Plan 493; unchanged |
| Plan 431 — the public dial scope | **blocked** | unchanged |
| Plan 433 — the NetDB lookup composition | **blocked** | unchanged |
| The `B` advertisement | **not registered** | a separate matter; it needs its own plan-of-record |
| A publication-capable profile | **not registered** | the `C` is a local claim until one exists; Plan 493's territory |
| The stock-peer half of Plan 526 | **blocked** | on Plan 493's authorized two-host topology |
| Plan 491 F1 / F3 / F4 / F5 | **closed** | discharged by Plans 503, 497, 510 and 511; unchanged |
| Plans 433, 435–439 | **blocked** | unchanged |

## Handoff notes

- The plan's subject is the **advertisement** of the router's own introducer
  role. The **form** is Plan 523's, the **acquisition** is Plan 524's, the
  **publication** is Plan 525's, and the **role** is Plan 490's; all are
  closed.
- The plan also repairs the **reachability** of the role, because a truthful
  advertisement requires a live role. Plan 499's closed record claims the
  composition constructs the role owner; that claim was false of the tree, and
  the correction lives here.
- The role letter is **never** a promotion: the inventory's advertised count
  stays zero and `router.version` stays pinned.
- The **`B` advertisement**, the **`caps=f` advertisement** and the
  **stock-peer half** all stay out of scope, with their owners named.
- Two guard files were extended, and the extension is named in the findings
  above. Any successor that moves another named invariant must extend the
  relevant guard the same way, and must say so in its own closure record.
