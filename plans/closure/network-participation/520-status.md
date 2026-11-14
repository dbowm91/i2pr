# Plan 520 — the router publishes its own qualified SSU2 address

**Closure record.** The plan of record is
`plans/implementation/network-participation/520-router-publishes-its-own-ssu2-address.md`.
This record is the authority on what the plan closed, what it did not, and
on what the evidence is.

## Status

Plan 520 is **closed**. The router's published RouterInfo now carries an
address a peer can **qualify** — it names the family capability and the
transport's MTU — and the publication of that address is an explicit,
default-off opt-in that is gated on the router's own reachability evidence.

The plan's registered premise was wrong, and the correction is the plan's
main content. The published record has carried one SSU2 address since the
tree began; the real gap was that the address was **unqualified**, so it
named an endpoint no peer could qualify. The corrections live in the plan
of record's amendment log, section 17; this record is not a rewrite of the
plan.

**Class:** capability (the router becomes findable by a peer).
**Subsystem:** network-participation. **Closure date:** 2026-11-07.

Nothing is promoted: `router.version` stays `0.9.62`, `caps` and the
published RouterInfo's **content** stay unchanged, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

Plan 520 gave the router's own published RouterInfo an address a peer can
**qualify**, and made the publication of that address an explicit, default-off
opt-in that is gated on the router's own reachability evidence.

**The premise was corrected, and the correction is the plan's main content.**
The registered plan opened with "the published record names no endpoint", and
its second invariant read "the router's published record carries no address,
exactly as today". Both were false. The published record has carried one SSU2
address since the tree began: `generate_identity_with_keys`
(`crates/i2pr-daemon/src/router_i2np.rs:2036-2050`) builds it from the **bound**
endpoint with the options `host`, `port`, `v`, `s` and `i`, and that record is
what the composition installs (`crates/i2pr-daemon/src/lib.rs:4198-4202`) and what
the publication cycle publishes.

The real gap was narrower and sharper: **the published address was
unqualified**. It omitted the `caps` and `mtu` options the specification's
SSU2 form carries and the netdb's own `is_qualified_ssu2_address` demands
(`crates/i2pr-netdb/src/floodfill_role.rs:195-260`), so the record named an
endpoint no peer could qualify. The reference publishes both
(`libi2pd/RouterInfo.cpp:1434-1449`, `:281`), and Java's own API says the
addresses are those "at which this router can be contacted"
(`router/java/src/net/i2p/data/router/RouterInfo.java:210-226`).

So the corrected objective is: **the published address is qualified — it
carries the family capability and the transport's MTU — and it is published
only while the router's own reachability evidence admits it.**

What the plan closed, concretely:

- `crates/i2pr-netdb/src/local.rs` — `LocalRouterInfoBuilder::build_with_ssu2_address`,
  the **non-floodfill** addressed builder. It validates the address with the
  netdb's **own** qualification rule and takes **no** floodfill permit, so
  `caps=f` stays permit-gated. A new error variant,
  `LocalRouterInfoError::UnqualifiedAddress`, names the refusal.
- `crates/i2pr-daemon/src/self_publication.rs` — the owner. It asks the runtime
  for the transport's material, hands the address to the netdb's own builder,
  installs the record, withdraws it when the evidence lapses, stands down for
  the floodfill path's record, and counts every outcome under its own reason.
- `crates/i2pr-daemon/src/config.rs` — `ssu2.advertise` stops being a
  parse-time error and becomes a real opt-in: **off** by default, refused
  unless the SSU2 service itself runs, and refused outside the controlled
  loopback profile.
- `crates/i2pr-daemon/src/router_i2np.rs` — the socket and the service
  boundaries refuse the opt-in by **profile** rather than refusing it outright.
- `crates/i2pr-daemon/src/lib.rs` — the composition root builds the owner from
  the operator's opt-in and refreshes the record inside the publication's
  cycle, **before** the cycle re-dates and publishes it.
- `crates/i2pr-runtime/src/ssu2_runtime.rs` — the install gate no longer
  *requires* an SSU2 address. Every address the record carries is still bound
  to the router's own bound endpoint with the router's own static and intro
  keys, and two conflicting SSU2 addresses are still refused, but the
  transport's **endpoint-less** form is now installable.

That last change is the one that makes the rest honest. The gate was the
reason the daemon's published address was rendered **unconditionally**: the
transport's own policy renders the endpoint-less form when the evidence does
not admit a direct endpoint, and the daemon's ad-hoc address bypassed that
policy. With the gate yielding, the transport's policy is the single authority
on whether an endpoint may be published at all.

The documentation moved with it:

- `specs/support.toml` gains the surface `daemon.own-ssu2-address-publication`
  with `advertised = false`, so the inventory is now **112 surfaces, 107
  experimental, 4 deferred, 1 ready, 0 advertised**;
- `specs/CONFORMANCE.md` gains the inventory entry and its own section, *"The
  router publishes its own qualified SSU2 address"*;
- `specs/QUALIFICATION-GATES.md` gains section 19 — nine sub-sections, eight
  of which name a `*Gate:*` and the ninth of which states what the gate does
  **not** claim;
- `specs/protocols/09-ssu2.md` records that the direct form is rendered in
  production under the explicit, default-off opt-in;
- the daemon, netdb and runtime deep-dives gain the new module, the new
  builder and the relaxed gate, with their line counts derived from the tree.

## What the plan did not close

- **The stock-peer half.** That a **stock** peer qualifies the published
  record in the live network needs Plan 493's authorized two-host topology and
  remains blocked on it. It is not claimed here and it is not worked around.
- **The ordinary record's address in production.** The direct form requires
  `Reachable`, which the transport's tracker defines as **three distinct
  corroboration classes**
  (`crates/i2pr-transport/src/reachability.rs:66`, `:557-561`). The runtime's
  production recorders can supply them only inside the **controlled floodfill
  activation**, and that path installs the *floodfill* record, whose address
  this plan's stand-down rule leaves alone. There is **no production entry
  point** that starts a peer test outside that activation. That is recorded as
  finding **M1** below.
- **The introducer-only form.** It needs validated introducer records, which
  no production code produces yet. The finding is recorded, not worked around.
- **The external-address observation.** No production code produces
  `AuthenticatedPeerObservedExternalAddress` about the router itself.
- **IPv6 publication.** The first case is IPv4 only.
- **`caps=f`.** That is Plan 494's, blocked on Plan 493.

## Implementation commits

The implementation is four commits on this branch:

- `0687c1e45238a994cf3e8b0b6633ae0785c3412d` — feat(publication): the router publishes its own qualified SSU2 address (Plan 520) — the netdb's addressed builder, the
  daemon's owner, the opt-in, the runtime's install gate, the lane, the
  guard, the amendment log and the documentation.
- `f9fb96e110d3421151cc2d52291230d83ca3bebf` — fix(publication): Plan 520's four routine-floor failures (Plan 520) — the four failures the routine
  floor's first run found, fixed.
- `6220783d567e549d8fdc9f79ebd38d2f4ff01602` — the plan of record's section 3 citation record: section 3
  is the plan's pre-plan evidence, and this commit records where each of its
  pointers now lives without rewriting the evidence itself.
- `ae63a75a43a2607969415249b2c2b0dfec4f37ac` — the daemon deep-dive's numbers, derived from the tree.

The plan's own amendment log is in the plan of record, not here: the plan is
not rewritten, and the corrections are recorded in its section 17.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The published address is qualified — it carries the family capability and the transport's MTU | `crates/i2pr-netdb/src/local.rs` — `build_with_ssu2_address` validates with `is_qualified_ssu2_address`; lane `the_direct_form_is_published_when_the_evidence_admits_it` asserts the address's `caps` is exactly `4` and its `mtu` is the transport's, and that the netdb's own qualification accepts it |
| The address is published only while the evidence admits it | `crates/i2pr-daemon/src/self_publication.rs` — `refresh` asks the runtime for the material and never builds a snapshot; guard rule 2 (`the_daemon_holds_no_copy_of_the_publication_rule`) proves the daemon holds no copy of the rule |
| The endpoint is the router's own bound endpoint, never a configured literal | The owner never constructs an address (`RouterAddress::new(` and `Mapping::from_entries(` are absent from it); guard rule 3 (`the_owner_never_builds_an_address`) |
| `caps=f` stays permit-gated | The addressed builder takes no permit; the netdb's `build_floodfill` still takes `&FloodfillAdvertisementPermit`; guard rules 4 and 5; lane `the_published_record_carries_no_f_capability` |
| The opt-in is real, explicit and off by default | `crates/i2pr-daemon/src/config.rs` — `default_ssu2_advertise` returns `false`, and the parse-time refusal is conditional; guard rule 1; lane `the_opt_in_is_off_by_default` and `the_router_publishes_no_address_when_the_opt_in_is_off` |
| The withdrawal is the expiry | The owner builds the netdb's ordinary zero-address form when the transport admits no address; guard rule 9; lane `the_address_is_withdrawn_when_the_evidence_lapses` |
| The installed record is the published record | The owner installs the record and the cycle re-dates it afterwards; guard rule 7; lane `the_redated_record_still_carries_the_address` |
| The re-dating preserves the address | `crates/i2pr-crypto/src/lib.rs` — `resign_router_info_with_published` re-signs the installed record; guard rule 8 |
| The floodfill path's record is left alone | The owner's own `StandDownReason::FloodfillRecord` stand-down, counted under its own reason; lane `the_floodfill_record_is_left_alone`; guard rule 5 (`the_floodfill_builders_still_require_the_permit`) keeps the floodfill builders permit-gated |
| Every outcome is counted under its own reason | `SelfPublicationCounters` — one counter per reason; lane `every_outcome_is_counted_under_its_own_reason` |
| The daemon records no reachability signal of its own | `ReachabilitySignal` appears nowhere in the daemon's production code; guard rule 10 |
| The owner adds no infrastructure | The owner module contains no `tokio::`, no `spawn`, no `TcpListener` and no `std::net`; guard rule 11 |
| The runtime's gate binds every address it accepts | `crates/i2pr-runtime/src/ssu2_runtime.rs` — `ssu2_count > 1` refuses two, and every SSU2 address must be bound; guard rule 12 |
| Nothing is advertised | `specs/support.toml` — the new surface is `advertised = false` and the inventory's advertised count stays **0**; `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` are byte-identical |

## Focused verification

The plan's own commands, each run on the closing tree:

| Command | Result |
| --- | --- |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_address -- --test-threads=1` | 8 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_address_boundary -- --test-threads=1` | 25 passed, 0 failed |
| `cargo test --locked -p i2pr-netdb --all-targets` | 315 passed, 0 failed (lib) plus every integration binary |
| `cargo test --locked -p i2pr-runtime --all-targets` | every binary green, including the install-gate test |
| `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` | 967 passed in the lib and every integration binary green |
| `cargo fmt --all --check` | clean |
| `bash scripts/check-dependency-direction.sh` | ok |
| `bash scripts/check-runtime-boundaries.sh` | ok (12 rules for `i2pr-netdb`, 7 each for `i2pr-api`, the transport and `i2pr-app-proto`, with positive controls) |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |

The four steps the floor's first run failed were each re-run on their own
after the fix:

| Command | Result |
| --- | --- |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | clean |
| `python3 scripts/check-router-readiness-contract.py` | passed |
| `python3 scripts/check-router-readiness-contract.py --self-test` | passed |
| `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | passed (11/11 negative cases each failed on their own gate) |

Every `file:line` citation in this record and in the plan of record was
checked against the closing tree: **40 citations, 40 verified**, including
the reference trees at their pins (i2pd `2.61.0`, Java I2P `2.13.0`). The
guard's twelve rule names, its thirteen probe names and the lane's eight
test names were each confirmed to exist.

The whole of the routine floor below covers the rest, including the workspace
build, the clippy run with `-D warnings`, the rustdoc run with `-D warnings`
and `cargo deny`.

## The routine floor

The routine floor ran on the committed tree at `ae63a75a43a2607969415249b2c2b0dfec4f37ac` with a **fresh**
script (`/tmp/floor-520c.sh`) that was never edited while it ran: bash reads a script
incrementally, so rewriting one under a running instance makes that instance
resume at a byte offset in the new text and silently skip whole blocks of
steps. The log is `/tmp/floor-520c.log`.

**149 steps, 149 PASS, 0 FAIL.** The reconciliation is
exact: the script declares 149 steps and the log carries 149 results, the step names match
one for one, and no step failed.

| Step | Result | Seconds |
| --- | --- | --- |
| `floor-tree-clean` | PASS | 0 |
| `cargo-fmt` | PASS | 3 |
| `cargo-check` | PASS | 1 |
| `cargo-build-siblings` | PASS | 0 |
| `cargo-build-static-fixture` | PASS | 0 |
| `cargo-copy-static-fixture` | PASS | 0 |
| `cargo-rebuild-plain-fixture` | PASS | 0 |
| `cargo-test-workspace` | PASS | 1171 |
| `cargo-clippy` | PASS | 0 |
| `cargo-doc` | PASS | 0 |
| `cargo-test-doc` | PASS | 4 |
| `check-dependency-direction` | PASS | 0 |
| `check-global-plan-number-uniqueness` | PASS | 0 |
| `check-adr-number-uniqueness` | PASS | 0 |
| `check-portable-service-tunnel-api` | PASS | 0 |
| `check-portable-service-tunnel-consumer` | PASS | 6 |
| `planning-tests` | PASS | 6 |
| `check-runtime-boundaries` | PASS | 1 |
| `check-console-boundaries` | PASS | 0 |
| `check-console-browser-security` | PASS | 0 |
| `check-tooling-inventory` | PASS | 0 |
| `check-license-metadata` | PASS | 0 |
| `check-managed-app-private-client-seams` | PASS | 0 |
| `check-router-readiness-contract` | PASS | 0 |
| `check-router-readiness-contract-self` | PASS | 1 |
| `check-core-router-recovery-contract` | PASS | 0 |
| `check-core-router-recovery-contract-self` | PASS | 0 |
| `plan449-ssu2-preflight-self` | PASS | 0 |
| `plan495-peer-eligibility-self` | PASS | 0 |
| `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| `plan476-gate-binding` | PASS | 0 |
| `plan476-gate-binding-self` | PASS | 1 |
| `check-managed-app-package-boundary` | PASS | 0 |
| `check-managed-app-package-boundary-self` | PASS | 0 |
| `check-managed-app-policy-boundary` | PASS | 0 |
| `check-managed-app-policy-boundary-self` | PASS | 0 |
| `check-managed-app-secured-sandbox` | PASS | 0 |
| `check-managed-app-secured-sandbox-self` | PASS | 0 |
| `check-service-tunnel-boundaries` | PASS | 0 |
| `check-managed-app-gateway-boundary` | PASS | 0 |
| `check-managed-app-manager-boundary` | PASS | 0 |
| `check-m11-per-epoch-composition` | PASS | 0 |
| `check-service-anonymity-boundaries` | PASS | 0 |
| `check-fixture-manifest` | PASS | 0 |
| `check-ntcp2-vectors` | PASS | 1 |
| `check-ssu2-vectors` | PASS | 0 |
| `check-i2cp-vectors` | PASS | 0 |
| `check-ntcp2-interoperability` | PASS | 0 |
| `check-constrained-host-lane-boundary` | PASS | 0 |
| `check-m11-transit-boundaries` | PASS | 2 |
| `check-m11-transit-qualification-evidence` | PASS | 1 |
| `check-sam-acceptance-evidence` | PASS | 0 |
| `check-ssu2-acceptance-evidence` | PASS | 1 |
| `check-i2cp-acceptance-evidence` | PASS | 0 |
| `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| `check-els2-type11-transcript-boundary` | PASS | 0 |
| `check-els2-live-lane-evidence` | PASS | 2 |
| `check-encrypted-service-consumer-caller` | PASS | 0 |
| `check-outproxy-request-path` | PASS | 0 |
| `check-outproxy-wire-lane-evidence` | PASS | 0 |
| `check-config-secret-hygiene` | PASS | 0 |
| `check-workflow-validity` | PASS | 0 |
| `check-floodfill-type5-serve` | PASS | 0 |
| `check-service-tunnel-acceptance-evidence` | PASS | 1 |
| `check-exploratory-tunnel-evidence` | PASS | 0 |
| `check-netdb-tunnel-evidence` | PASS | 0 |
| `check-destination-tunnel-evidence` | PASS | 0 |
| `check-streaming-tunnel-evidence` | PASS | 5 |
| `check-m6-mixed-router-acceptance-evidence` | PASS | 4 |
| `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| `check-m12-floodfill-boundaries-self` | PASS | 1 |
| `ntcp2-harness-tests` | PASS | 0 |
| `plan488-mesh-lane` | PASS | 0 |
| `plan488-evidence-deterministic` | PASS | 0 |
| `plan489-mesh-lane` | PASS | 1 |
| `plan489-evidence-deterministic` | PASS | 0 |
| `plan490-mesh-lane` | PASS | 0 |
| `plan490-evidence-deterministic` | PASS | 0 |
| `plan490-ssu2-transport-suite` | PASS | 6 |
| `plan490-transport-boundary-guard` | PASS | 0 |
| `plan491-mesh-lane` | PASS | 1 |
| `plan491-evidence-deterministic` | PASS | 0 |
| `plan491-boundary-guard` | PASS | 0 |
| `plan491-proto-suite` | PASS | 0 |
| `plan491-netdb-suite` | PASS | 88 |
| `plan491-client-suite` | PASS | 23 |
| `plan492-posture-suite` | PASS | 1 |
| `plan492-m11-lane` | PASS | 4 |
| `plan502-transit-composition-lane` | PASS | 0 |
| `plan502-transit-composition-unit` | PASS | 1 |
| `plan502-transit-boundary-self` | PASS | 0 |
| `plan496-private-lan-profile` | PASS | 0 |
| `plan496-ntcp2-activation-gate` | PASS | 0 |
| `plan497-routing-consumer-lane` | PASS | 1 |
| `plan497-netdb-preferred-order` | PASS | 1 |
| `plan497-daemon-participation-unit` | PASS | 2 |
| `plan498-introducer-handoff-lane` | PASS | 0 |
| `plan498-introducer-mesh-lane` | PASS | 1 |
| `plan498-bootstrap-resolver-row` | PASS | 25 |
| `plan498-runtime-relay-accessor` | PASS | 4 |
| `plan503-version-eligibility-live` | PASS | 1 |
| `plan503-version-eligibility-unit` | PASS | 0 |
| `plan504-liveness-boundary` | PASS | 1 |
| `plan504-liveness-live` | PASS | 0 |
| `plan504-liveness-unit` | PASS | 0 |
| `plan505-driver-boundary` | PASS | 1 |
| `plan505-driver-live` | PASS | 2 |
| `plan505-driver-unit` | PASS | 1 |
| `plan506-reconcile-boundary` | PASS | 5 |
| `plan506-reconcile-live` | PASS | 1 |
| `plan507-retention-feasibility` | PASS | 0 |
| `plan507-retention-boundary` | PASS | 1 |
| `plan507-retention-live` | PASS | 1 |
| `plan508-creator-retention-feasibility` | PASS | 0 |
| `plan508-creator-retention-boundary` | PASS | 1 |
| `plan508-creator-retention-live` | PASS | 0 |
| `plan509-lookup-target-decision` | PASS | 1 |
| `plan509-lookup-target-boundary` | PASS | 0 |
| `plan510-managed-staging-load-lane` | PASS | 1 |
| `plan510-managed-reason-boundary` | PASS | 1 |
| `plan510-tunnelmanager-black-box` | PASS | 3 |
| `plan511-version-ceiling-derivation` | PASS | 0 |
| `plan511-version-ceiling-boundary` | PASS | 0 |
| `plan512-destination-liveness-live` | PASS | 0 |
| `plan512-destination-liveness-boundary` | PASS | 1 |
| `plan489-peer-budget-receipt` | PASS | 0 |
| `plan513-discovery-live` | PASS | 2 |
| `plan513-discovery-boundary` | PASS | 1 |
| `plan513-discovery-unit` | PASS | 2 |
| `plan513-discovery-seam-unit` | PASS | 0 |
| `plan514-exploratory-target` | PASS | 7 |
| `plan514-exploratory-target-boundary` | PASS | 0 |
| `plan516-netdb-publication` | PASS | 2 |
| `plan515-publication` | PASS | 0 |
| `plan515-publication-boundary` | PASS | 0 |
| `plan516-confirmation` | PASS | 3 |
| `plan516-confirmation-boundary` | PASS | 1 |
| `plan517-publication-retry` | PASS | 3 |
| `plan517-publication-retry-boundary` | PASS | 0 |
| `plan518-publication-refresh` | PASS | 1 |
| `plan518-publication-refresh-boundary` | PASS | 0 |
| `plan519-publication-off-loop` | PASS | 2 |
| `plan519-publication-off-loop-boundary` | PASS | 0 |
| `plan520-publication-address` | PASS | 1 |
| `plan520-publication-address-boundary` | PASS | 0 |
| `plan520-netdb-addressed-builder` | PASS | 1 |
| `plan520-runtime-install-gate` | PASS | 1 |
| `plan514-switch-unit` | PASS | 0 |
| `cargo-deny` | PASS | 0 |

The plan's own steps are the last four: the lane, the guard, the netdb's
addressed builder and the runtime's install gate. The lane and the guard are
also run **inside** the workspace test step, so both are exercised twice.

This is the **third** run. The first, on the implementation commit, reported
four failures; they are accounted for in the section above. The second was
run on the fixed tree and passed every step, but its log lost the commit line
to a defect in the scratch script -- it wrote the header and then truncated
the log -- so the third run was made with the defect corrected, and this is
its log.

## The floor's first run, and the fixes

The routine floor was first run on the implementation commit `0687c1e4`, and it
reported **149 steps, 145 PASS, 4 FAIL**. Every failure came from this plan's
own changes and from nothing else, and each was fixed rather than tolerated:

1. **`cargo clippy`** — the plan's own guard carried an unused constant
   (`NETDB_FLOODFILL_MODULE`) and an unused helper (`call_site`). Both were
   removed. Neither was a rule, so the guard's rule count is unchanged.
2. **`check-router-readiness-contract`** — two of the checker's needles no
   longer pointed at their refusals, because this plan moved where the
   refusals are enforced. The rules were **re-anchored** at the new
   enforcement points and **extended**: the advertisement is now pinned
   twice, refused unless the SSU2 service itself runs and refused outside
   the controlled loopback profile, where the old rule pinned only the
   `if raw.advertise {` line. One rule was added and two rules' labels
   changed; **no rule was dropped**, and the checker's self-test still
   detects a mutation of every rule, including the new ones.
3. **`check-m12-floodfill-boundaries`** — the M12 checker's own exclusion of
   the test directories (`--glob '!*/tests/*'`) does not reach a nested
   `crates/*/tests/` path, because a ripgrep glob's `*` does not cross a
   directory separator, so it read this plan's own **probe fixture** — a
   string literal inside a test file that stands for a hypothetical
   violating daemon — as if it were a production recorder. The checker was
   **not** modified. The probe fixture was rewritten so that it names a
   tracker handle rather than the runtime's own field. The rule the probe
   exercises is unchanged, because the guard's own rule 10 keys on the
   `ReachabilitySignal` token the fixture still names, so the inverted
   probe still proves that the guard fires.
4. **`check-m12-floodfill-boundaries --self-test`** — the same fixture, read
   by the self-test variant, fixed by the same change.

The fixes are a **fix**, not a relaxation: no script's rule was weakened, no
rule was disabled, no exclusion was widened, and no production wire was
changed to go green. The plan of record carries the same account in its
amendment log, entry 17.7, and Plan 490's closure record — which describes
the checker's label as it stood at Plan 490's time — is closed and is not
rewritten.

## The negative controls

The guard's rules are **structural**, and every rule has an **inverted probe**
— a mutation that must be reported. A rule with no probe is a rule nobody has
shown to fire. All twelve rules and all thirteen probes are green:

| Rule | Probe |
| --- | --- |
| `the_switch_is_not_refused_at_parse_time` | `the_guard_reports_an_unconditional_parse_refusal` |
| `the_daemon_holds_no_copy_of_the_publication_rule` | `the_guard_reports_a_daemon_copy_of_the_publication_rule` |
| `the_owner_never_builds_an_address` | `the_guard_reports_an_owner_that_builds_its_own_address` |
| `the_addressed_builder_qualifies_without_a_permit` | `the_guard_reports_an_addressed_builder_that_takes_a_permit` |
| `the_floodfill_builders_still_require_the_permit` | `the_guard_reports_a_floodfill_builder_without_a_permit` |
| `the_family_capability_is_carried` | `the_guard_reports_a_material_without_the_family_capability`, `the_guard_reports_a_transport_that_always_writes_caps` |
| `the_record_is_installed_and_published` | `the_guard_reports_a_cycle_that_publishes_before_the_owner_refreshes` |
| `the_redating_preserves_the_address` | `the_guard_reports_a_redating_that_rebuilds_the_record` |
| `the_expiry_withdraws_the_address` | `the_guard_reports_an_owner_that_keeps_the_lapsed_address` |
| `the_daemon_records_no_reachability_signal` | `the_guard_reports_a_daemon_that_records_its_own_signal` |
| `the_owner_adds_no_infrastructure` | `the_guard_reports_an_owner_that_starts_a_task` |
| `the_install_gate_binds_every_address` | `the_guard_reports_an_install_gate_that_requires_an_address` |

The probes are mutations of **production source text**, not of the guard's own
logic: each probe feeds a violating source fragment to the same rule function
the rule test uses, so a rule that stopped firing would be reported by its
probe.

## Invariant review

The plan's seven invariants, each checked against the closing tree:

1. **`caps=f` stays refused** without the floodfill advertisement permit. The
   addressed builder takes no permit at all, so it cannot reach the `f`
   capability; the floodfill builders are untouched and still take
   `&FloodfillAdvertisementPermit`. **Holds.**
2. **The loopback default publishes nothing.** With the switch off the owner
   is disabled and the installed record stands byte-identical — the lane
   asserts the installed bytes are unchanged. The invariant's *parenthetical*
   ("exactly as today") was wrong and is corrected in the plan's amendment
   log: today the record carries an **unqualified** address, and the plan's
   withdrawal returns it to the **zero-address** form. **Holds, as amended.**
3. **The publication cycle is unchanged.** The period, the fan-out, the
   ladder's timing, the correlator, the freshness rule and the task's
   ownership are untouched: Plan 520 adds one call inside the cycle's
   existing branch and one field on the task's existing struct. **Holds.**
4. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   RouterInfo's `caps` string is unchanged, the support inventory's advertised
   count stays zero, and `Cargo.toml` and `Cargo.lock` are byte-identical.
   **Holds.**
5. **One decision, one owner.** The publication decision is the transport's
   `build_publication_snapshot` and nothing else; the daemon asks the runtime
   for the material and never computes a second answer. The guard proves the
   daemon's own copy of the rule does not exist. **Holds.**
6. **The netdb's qualification is the authority.** An address the netdb's own
   `is_qualified_ssu2_address` would refuse is never signed and never
   installed. **Holds.**
7. **No fabricated evidence.** The daemon records no reachability signal at
   all; the runtime owns the evidence, and a missing signal is a missing
   signal. **Holds.**

## Failure, cancellation, restart, and contention review

- **A missing signal** lowers the corroboration count, and the snapshot's own
  decision follows: the transport admits no address, the published record
  returns to its zero-address form, and the refusal is counted under its own
  reason. The router publishes no endpoint it cannot justify.
- **A contradicting signal** takes precedence over corroboration inside the
  transport's own tracker
  (`crates/i2pr-transport/src/reachability.rs:528-556`). The daemon never sees
  it and never re-implements it; the transport admits no address and the
  withdrawal follows.
- **An expired snapshot** is withheld: the address is withdrawn and the record
  returns to its zero-address form, counted under its own reason.
- **A restart** rebuilds the owner and the tracker. The evidence does not
  survive the restart, because it was never persisted, so a restarted router
  publishes no address until it has earned the evidence again. That is the
  conservative outcome, and it is the reference's: i2pd assumes reachable only
  until a peer test finds a firewall (`libi2pd/RouterContext.cpp:1212`).
- **A re-dating cycle** re-signs the record with the address intact; the
  cycle's record and the installed record stay one record.
- **Contention** — one owner, one tracker, one snapshot at a time. The owner
  runs inside the publication's cycle, which is single-flight by Plan 517's
  own rule (`begin_cycle` leaves a running cycle alone), so two refreshes can
  never overlap.
- **Cancellation** — the owner holds no task of its own, so there is nothing
  to cancel: it runs to completion inside the cycle's branch, and the cycle's
  own cancellation path is unchanged.

## Migration and compatibility review

- **No wire change.** The address form is the form the netdb already
  qualifies and the transport already emits.
- **No dependency change.** `Cargo.toml` and `Cargo.lock` are byte-identical;
  the daemon gained no `i2pr-transport-ssu2` dependency, which is why the
  owner asks the runtime for the material instead of calling the transport's
  builder.
- **The published RouterInfo's `caps` string is unchanged**, and
  `router.version` stays `0.9.62`.
- **The default configuration changes in no way**: the switch stays off, so
  the default router publishes exactly what it published before — the
  installed record stands byte-identical, and the lane asserts it.
- **The runtime's install gate is relaxed, not weakened.** The checks that
  are about *truth* are unchanged — every SSU2 address the record carries must
  name the router's own bound endpoint with the router's own static and intro
  keys, and two conflicting SSU2 addresses are still refused. The check that
  was about *presence* — that an address must be there at all — is what the
  transport's own publication policy requires to yield.

## Security review

- **No secret crosses a boundary.** The owner holds the `RouterIdentityBundle`
  by reference for the length of one call, exactly as the netdb's builder
  does; it never clones private key material and never retains the bundle.
  The transport's material carries **public** address data only.
- **No fabricated reachability.** The daemon records no signal of its own, so
  a local condition can never be written into a published claim. The
  published address is the transport's, whole.
- **No new listener, no new port, no new exposure.** The owner starts no
  task, no timer and no listener, and the guard proves it. The switch adds no
  configuration section.
- **No advertisement.** The support inventory's advertised count stays zero,
  the record's own `caps` string is unchanged, and `router.version` is
  untouched.
- **The floodfill capability stays unreachable** through the new builder: it
  takes no permit, and the netdb's ordinary options guard still refuses the
  `f B K L M N P R S U X` letters.
- **Fail closed.** Every refusal path — the unqualified address, the missing
  evidence, the closed runtime, the poisoned state, the refused install — is a
  typed error, and every one of them is counted under its own reason.

## Limitations

1. **The stock-peer half is not claimed.** No stock peer has qualified the
   published record in the live network; that needs Plan 493's authorized
   two-host topology.
2. **The direct form is not reachable in production today.** The evidence
   needs three distinct corroboration classes and no production entry point
   starts a peer test outside the controlled floodfill activation. The
   mechanism is provable in the controlled lane; the *route* to it in
   production does not exist yet. This is finding **M1**.
3. **The withdrawal is cycle-granular.** The owner refreshes inside the
   publication's cycle, whose period is 39 minutes
   (`PUBLICATION_INTERVAL_MS`), while the reachability evidence's lifetime is
   30 minutes (`DEFAULT_OBSERVATION_TTL`). A record may therefore keep
   publishing an address for up to one publication interval after the
   evidence has lapsed. The granularity is the publication's own, and the
   reference's own publication is likewise periodic.
4. **The private-LAN profile may not opt in.** The runtime's publication
   material is refused for any non-loopback bind, so the switch is refused
   there rather than silently publishing nothing.
5. **The transport's firewalled form has no home in the netdb's builders.**
   The netdb's ordinary builder produces the zero-address form, which is what
   the withdrawal installs. The firewalled form (an address with `s` and `v`
   only) would need a builder of its own.
6. **The plan's own premise was wrong**, and two of its decisions were
   unimplementable as written. The corrections live in the plan's amendment
   log, not in a silent rewrite of the registered text.

## Findings by severity

### Medium — open, recorded for a successor

**M1 — the ordinary published address cannot be earned in production.**

*What.* The direct form requires `ReachabilityState::Reachable`, which the
transport's tracker defines as **more than** the corroboration floor — three
distinct supporting classes
(`crates/i2pr-transport/src/reachability.rs:66`, `:557-561`). The runtime's
production recorders are `note_explicit_bind_for_controlled_qualification`
(the explicit bind), the `AddressObserved` session event (a peer reporting
the address it sees) and the controlled peer test's confirmed outcome. The
first is called only by the controlled activation, the second needs a peer
that observes us, and the third needs a started peer test — and there is **no
production entry point** that starts one outside the controlled floodfill
activation, which in any case installs the *floodfill* record that this
plan's stand-down rule leaves alone.

*Why it matters.* Under the default configuration the switch is off and
nothing changes, so this is not a regression. But with the switch on, the
ordinary record would publish **no** endpoint, because the evidence cannot be
earned. The mechanism is proved; the production route to it does not exist.

*Not fixed here* because inventing a peer-test entry point is a change to the
runtime's controlled role surface, and the plan's own scope keeps the
introducer and peer-test roles out of it. The successor should own a bounded
production peer-test entry point (or the authenticated-peer observation that
would supply the third class), and it must not be built on this plan's
back.

*Owner:* a successor plan in the network-participation line.

### Low — recorded, not fixed

**L1 — the daemon deep-dive's remaining module rows still carry stale line
counts.**

Plan 519's closure recorded the same drift as **L1**. Plan 520 wrote the
deep-dive's header and its own row from numbers taken **mid-authoring**,
before the last edits landed, and it left the other rows at the counts they
had before several plans' changes.

The numbers Plan 520 made stale are corrected and derived from the tree. The
header is now **77 `.rs` files, 133 725 lines**, and the rows for
`src/lib.rs` (**6 578**), `src/config.rs` (**7 068**), `src/router_i2np.rs`
(**3 874**) and `src/self_publication.rs` (**462**) match the files. The
`src/config.rs` and `src/router_i2np.rs` rows were **already** stale before
Plan 520, by 239 and 78 lines; Plan 520 added 43 and 19 more. The source has
not changed since Plan 520's implementation commit, so every one of these
numbers is now the tree's own.

The **other** rows still carry the counts they had before several plans'
changes. The numbers are derivable, and the derivation is the successor's
job: a guard that computes them (Plan 372's
`scripts/check-tooling-inventory.py` is the precedent) would keep them from
rotting again.

*Owner:* the same successor Plan 519 named.

### Informational

**I1 — the registered plan's premise was wrong.** See the amendment log.

**I2 — the transport's `caps_with_family` is not on the direct path.** The
family capability is prepended only on the introducer-only path; the direct
path uses the caller's capability set verbatim. The daemon therefore passes
**no** capability set at all and takes the address whole, which is why the
guard proves the transport writes the `caps` option only when the set is
non-empty.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md` is updated:

- the **RouterInfo** row's *Missing* cell now reads "**the published address
  is unqualified** — it omits the `caps` and `mtu` options the form carries
  and the netdb's own `is_qualified_ssu2_address` demands, so the record
  names an endpoint no peer can qualify (**Plan 520**)", replacing the
  incorrect "the published record carries no address at all";
- the **consequence** paragraph now says the router cannot be **found** by a
  peer "because the address its record publishes is one no peer can
  qualify";
- the inventory's derived counts are **112 surfaces, 107 experimental, 4
  deferred, 1 ready, 0 advertised**, matching `specs/support.toml` exactly.

Plan 520's own row moves from *registered* to *closed* in this commit.

## Unblock audit

| Blocked item | Unblocked by Plan 520? | Why |
| --- | --- | --- |
| The router's published record names an endpoint no peer can qualify | **Yes, structurally** | The netdb's own qualification accepts the published form, and the transport's snapshot carries it |
| A peer finds `i2pr` by the record `i2pr` publishes | **No** | The direct form needs three corroboration classes and no production peer-test entry point exists (**M1**); the stock-peer half is Plan 493's |
| Plan 491's **F2** (the stock-peer half) | **No** | Blocked externally on Plan 493's authorized two-host topology |
| Plan 497's **M2** and limitation 1 (the stock-peer half) | **No** | Blocked externally on Plan 493's authorized two-host topology |
| Plan 487's stock-peer half | **No** | Blocked externally on Plan 493's authorized two-host topology |
| The `caps=f` advertisement | **No** | Plan 494's, blocked on Plan 493 |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | **No** | Blocked on external environments |

## Handoff notes

- The next free plan number after this one is **521**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is patched
  or vendored.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology.
- The next thing worth doing in this line is **M1**: a bounded production
  peer-test entry point, or the authenticated-peer observation that would
  supply the third corroboration class. Until one exists, the opt-in
  publishes no endpoint in production — which is the honest outcome, and the
  reference's own ("we assume reachable until we discover firewall through
  peer tests", `libi2pd/RouterContext.cpp:1212`).
- The daemon deep-dive's **remaining module rows** still carry stale line
  counts (**L1**); the header and the four rows Plan 520 touched are correct.
