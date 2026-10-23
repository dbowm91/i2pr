# Plan 491 — API-version floors and peer eligibility

**Status:** `passed-api-version-floors-and-peer-eligibility`
**Date closed:** 2026-10-23
**Subsystem:** network participation (phase 4 of 7)
**Implementation plan:** [`plans/implementation/network-participation/491-api-version-floors-and-peer-eligibility.md`](../../implementation/network-participation/491-api-version-floors-and-peer-eligibility.md)
**Roadmap:** [`plans/subsystems/network-participation-roadmap.md`](../../subsystems/network-participation-roadmap.md)
**Implementation commits:** `dc36e2d7` — *feat(network-participation): plan 491 API-version floors and peer eligibility*; `526104ae` — *test(network-participation): plan 491 controls, probes, and the private mesh lane*; `f566b97f` — *docs(network-participation): plan 491 architecture, conformance, and inventory*; `c195f363` and `27c20b9c` — the two defects the routine floor caught (the floor-caught fixes are committed separately, as Plan 489 and Plan 490 did, so this record can report the tree honestly; neither is part of the plan's deliverable — see §7.3)
**Predecessor:** Plan 285 (`retained-m12-peer-testing-implemented-and-declared-pending-mixed-router-evidence`), whose `CONTROLLED_ROUTER_VERSION` declaration this plan makes *binding* rather than merely descriptive, and Plan 488 (`passed-peer-profile-store`), whose profile store carries the per-peer evidence.
**Owner boundary:** `crates/i2pr-proto` (the comparator and the specification's obligation table), `crates/i2pr-netdb` (the derived floor constants, the per-peer evidence, and the selection gate), `crates/i2pr-daemon` (the two enforcement points and the receipts).

---

## 1. Scope

Plan 491 made the router **honor the minimum-peer obligations that its own
published `router.version` attaches**. The I2NP specification's API-version
table attaches two duties to the `0.9.62` row — the level this router
declares:

```text
minimum peers will build tunnels through, as of 0.9.68
minimum floodfill peers will send DSM to, as of 0.9.68
```

Delivered:

- the **typed comparator** (`crates/i2pr-proto/src/common/version.rs`):
  `ApiVersion`, the specification's numeric comparison over published
  version strings, with `PublishedVersion::Unknown` (no version published)
  and `PublishedVersion::Uncomparable` (a value that cannot be compared) as
  first-class states, and `VersionEligibility` as the typed decision a floor
  produces;
- the **specification's obligation rows as data**
  (`MINIMUM_PEER_OBLIGATIONS`), the catch-all line
  (`CATCH_ALL_VERSION`), the specification snapshot level
  (`SPECIFICATION_API_LEVEL`), and the derivation `floors_in_force`;
- the **derived floor constants** (`crates/i2pr-netdb/src/version.rs`):
  `MIN_PEER_VERSION_FOR_TUNNEL_BUILD` and `MIN_FLOODFILL_VERSION_FOR_DSM`,
  derived from the obligation rows and from the level this router declares,
  plus the typed eligibility helpers and the counted refusal vocabulary
  (`VersionRefusalCounts`);
- the **per-peer version evidence** in Plan 488's profile store
  (`crates/i2pr-netdb/src/profile.rs`, `profile_store.rs`): every profile
  carries the peer's published version and its comparability state, refreshed
  from the peer's signed RouterInfo on every insert and adoption;
- the **version gate in the pure selection policy**
  (`crates/i2pr-netdb/src/selection.rs`): `VersionGate`,
  `VersionGatedSelection`, and `select_candidates_with_report`;
- the **first enforcement point** — the tunnel-build peer selection
  (`crates/i2pr-daemon/src/destination_peers.rs`): a typed `CandidateRefusal`
  vocabulary, counted refusals in the projection summary, and a fail-closed
  terminal that names the reason;
- the **second enforcement point** — the DSM publication target selection
  (`crates/i2pr-netdb/src/publication.rs` and
  `crates/i2pr-daemon/src/netdb_tunnels.rs`): a typed refusal, counted
  refusals, and the eligible-target selection that excludes a below-floor
  floodfill;
- the **receipts** (`crates/i2pr-daemon/src/netdb_participation.rs`):
  `VersionEligibilityReceipt` and the two driver methods that make the
  decisions and record them;
- the **offline controls**: the comparator's totality property test, the
  static boundary guard `crates/i2pr-daemon/tests/version_floor_boundary.rs`
  with an inverted probe per rule, and the private mesh lane
  `crates/i2pr-daemon/tests/version_eligibility_mesh.rs` with its sanitized
  evidence artifact.

Deliberately **not** delivered, by design:

- **no advertisement of any kind.** `router.version` is unchanged at
  `0.9.62`, `caps` is unchanged, the published RouterInfo is unchanged, the
  support inventory's advertisement flags are unchanged, and the inventory
  still has **zero** advertised surfaces;
- **no new participation capability.** The plan makes the router compliant
  with the level it already claims; it adds no role, no message, no option,
  and no wire change;
- **no operator knob.** The floors are specification constants; no
  configuration key can lower them;
- **editing the declared version** — explicitly forbidden by the plan, and
  guarded statically;
- the **bandwidth-tier derivation** and the metering behind it (Plan 494's
  dependency, unresolved in the reference implementations too);
- **caps=f** and the floodfill role (Plan 494);
- **transit participation** (Plan 492);
- the **two-family public qualification** (Plan 493) and the **floodfill
  role** (Plan 494).

---

## 2. Baseline at registration

At registration the router had **no peer-version floor logic anywhere in
the tree**. The plan names `c43f3404` as its registration baseline; the
implementation ran on the Plan 490 closing tree (`522ff6c2`), and the state
described below is identical in both — Plan 490 changed nothing about
version handling:

- The only version comparisons in the repository were the SAM protocol's
  version range. `i2pr-proto` had a `ProtocolVersion` type that validated a
  `router.version` string as *text* — non-empty, digits and dots — but
  nothing ever **compared** two versions, and nothing ever read a *peer's*
  version.
- `crates/i2pr-netdb/src/local.rs` declared
  `CONTROLLED_ROUTER_VERSION = "0.9.62"` and pinned it against the
  implemented I2NP surface by a test. The declaration was **descriptive**:
  it said what the router claimed, and nothing acted on it.
- `crates/i2pr-netdb/src/profile_store.rs` classified each peer by its
  signed `caps` option and tracked its failure model. It carried **no**
  version evidence.
- `crates/i2pr-netdb/src/selection.rs` selected peers by rank, tier, state,
  participation cap and /16 diversity. It never consulted a peer's
  published version, so a peer below the agreed floor was as selectable as
  any other.
- `crates/i2pr-daemon/src/destination_peers.rs` projected validated records
  into build candidates on family, key and endpoint facts alone.
- `crates/i2pr-netdb/src/publication.rs` chose the nearest floodfills to
  publish the local RouterInfo to, and
  `crates/i2pr-daemon/src/netdb_tunnels.rs` sent the DSM to them. Neither
  consulted a floodfill's published version.

A router that joined the network would therefore have built tunnels through
peers the network has agreed to stop using, and sent DSMs to floodfills
below the agreed floor. Both are network-visible mistakes, and both are
exactly what the pinned reference implementations enforce.

The specification's table was read from the primary source during
implementation (the I2NP specification's "Protocol Versions" section) and
the two reference implementations were read at their pinned commits:

| implementation | pinned commit | the constant it applies |
| --- | --- | --- |
| Java I2P 2.13.0 | `9134f808337b401e8e53c73734c81fab04280c9d` | `net.i2p.router.tunnel.pool.TunnelPeerSelector`: `MIN_VERSION = "0.9.62"` |
| i2pd 2.61.0 | `635b013a612ff47278ef02acf8580a28e10e26c5` | `libi2pd/NetDb.hpp`: `NETDB_MIN_HIGHBANDWIDTH_VERSION = 0.9.62`, `NETDB_MIN_FLOODFILL_VERSION = 0.9.62` |

Both references publish `router.version` `0.9.70` and both apply the floor
`0.9.62` — the version of the specification row that carries the two
obligations. The derivation in this plan reproduces that value from the
specification's table and the router's declared level, so the floor is not
a transcription of the references' constants.

---

## 3. Implementation commits

| Commit | Subject | Content |
| --- | --- | --- |
| `dc36e2d7` | feat(network-participation): plan 491 API-version floors and peer eligibility | `i2pr-proto`: `common/version.rs` — the comparator, the three published-version states, the typed eligibility decision, the specification's obligation rows, and the floor derivation. `i2pr-netdb`: `version.rs` — the declared level parsed once, the two derived floor constants, the eligibility helpers, and the counted refusal vocabulary; `profile.rs`/`profile_store.rs` — the per-peer version evidence; `selection.rs` — the version gate and the counted refusals; `publication.rs` — the eligible DSM targets. `i2pr-daemon`: `destination_peers.rs` — the typed build-peer refusal and the fail-closed terminal; `netdb_tunnels.rs` — the typed DSM refusal and its counter; `netdb_participation.rs` — the version-eligibility receipts; `service_product.rs` — the live call sites now fail closed with the reason. |
| `526104ae` | test(network-participation): plan 491 controls, probes, and the private mesh lane | The comparator's totality property test, the static boundary guard `crates/i2pr-daemon/tests/version_floor_boundary.rs` with an inverted probe per rule, the private mesh lane `crates/i2pr-daemon/tests/version_eligibility_mesh.rs`, the sanitized evidence artifact, and the publication fixtures that now publish an explicit `router.version`. |
| `f566b97f` | docs(network-participation): plan 491 architecture, conformance, and inventory | The three architecture deep-dives, `specs/CONFORMANCE.md`, `specs/support.toml`, and the plan document's closing status and refined production surface. |
| `c195f363` | fix(network-participation): resolve the plan 491 floor findings | **Not part of the deliverable.** Two rustdoc links the floor's `cargo doc` step rejected: the selection policy's link to the private `crate::version` module, and the policy module's link to `i2pr_proto::common::floors_in_force` (`common` is private in `i2pr-proto`). §7.3. |
| `27c20b9c` | fix(network-participation): publish a version in the live publication fixture | **Not part of the deliverable.** The one real regression the floor caught: the live NetDB publication fixture's floodfill published no `router.version`, so the new DSM floor refused it. The fixture now publishes the declared version, exactly as the other publication fixtures do. §7.3. |

---

## 4. Requirement-to-evidence matrix

| # | Requirement | Evidence | Where |
| --- | --- | --- | --- |
| R1 | **The comparator is total and typed.** Every published version string maps to a comparable value or to an explicit `Unknown`/`Uncomparable` state; a malformed or absent version never panics and never silently compares as "newest". | `ApiVersion::parse` returns `Option` and never panics; `PublishedVersion::parse_option` maps `None` to `Unknown` and an unparseable value to `Uncomparable`. The property test drives four thousand generated strings plus the hand-picked malformed and missing cases and asserts that each maps to a value or to an explicit state, that a reported value round-trips, and that an incomparable value is never eligible. | `version.rs` — `every_version_string_maps_to_a_value_or_an_explicit_state`, `rejects_malformed_versions`, `rejects_component_overflow`, `published_version_states_are_distinct` |
| R2 | **The numeric-comparison rule is the specification's.** `0.9.13` implies support for `0.9.12`; a version at or below the catch-all line is handled by the catch-all rule, not by a lexicographic comparison. | The comparator compares the components left to right as numbers; `0.9.9` is below `0.9.10` and `0.10.0` is above `0.9.62`. `CATCH_ALL_VERSION` names the specification's `0.7.9 or lower` row and the test asserts it is below every obligation row. The ordering is a total order over the comparable subset. | `version.rs` — `compares_numerically_not_lexicographically`, `the_catch_all_line_is_below_every_obligation`, `ordering_is_a_total_order`, `the_comparison_is_a_total_order_over_the_corpus` |
| R3 | **The two floor constants are pinned to the specification's table for the level this router declares, and a test fails if either drifts.** | The obligation rows are data in `i2pr-proto`; the floors are *derived* from those rows and from `CONTROLLED_ROUTER_VERSION` by `floors_in_force`. The derived values are `0.9.62` for both purposes. A test re-derives them and compares; a static guard fails if the policy module ever restates the floor as a literal, and another fails if the declared version is edited. | `i2pr-netdb/src/version.rs` — `the_floors_are_derived_from_the_specification_table`, `the_floors_are_not_a_hand_maintained_copy`, `the_declared_level_parses_to_the_controlled_version`; `version_floor_boundary.rs` — `the_policy_module_derives_the_floors`, `the_declared_version_is_not_edited` |
| R4 | **The tunnel-build path refuses a below-floor peer and fails closed with a reason when no candidate qualifies.** | `DestinationPeerCandidate::from_validated` returns `Result<Self, CandidateRefusal>`; the version decision is taken **before** any other fact about the record. `project_validated_store` counts each refusal by reason. `select_projected_destination_path` fails closed with `NoEligibleVersionPeers` when the floor refused every scanned record, and the live call sites in `service_product.rs` use it. | `destination_peers.rs` — `a_peer_below_the_tunnel_build_floor_is_refused_and_counted`, `a_peer_without_a_comparable_version_is_refused`; `netdb_participation.rs`; the live call sites |
| R5 | **The DSM path refuses a below-floor floodfill and fails closed with a reason when no eligible floodfill remains.** | `NetDbTunnelCoordinator::begin_publication` refuses with `FloodfillRefusedByVersionFloor(reason)` and counts the refusal in `publications_refused_by_version_floor`; the publication coordinator's `nearest_eligible_floodfills` excludes the same peers and reports the same counts. The refusal happens before the `DatabaseStore` body is built. | `netdb_tunnels.rs` — `a_floodfill_below_the_dsm_floor_is_refused`; `publication.rs` — `a_floodfill_below_the_dsm_floor_is_refused_and_counted` |
| R6 | **Both decisions are counted and explainable in receipts; no secret, no address, and no key appears in them.** | `VersionEligibilityReceipt` carries the gate, the examined population, the accepted count, the refusal counts by reason, and a digest over those counts. The static guard asserts that the receipt type carries no peer identity at all. The lane asserts that the rendered receipt and the rendered evidence file contain no peer hash, no address, no key, and no version string. | `netdb_participation.rs` — `VersionEligibilityReceipt`, `version_receipt_digest`; `version_floor_boundary.rs` — `the_version_receipt_carries_no_peer_identity`; `version_eligibility_mesh.rs` — the evidence assertions |
| R7 | **The declared `router.version` is unchanged, and its pinning test still passes.** | `CONTROLLED_ROUTER_VERSION` is still `"0.9.62"` in `crates/i2pr-netdb/src/local.rs`; Plan 285's test `controlled_router_version_matches_the_implemented_i2np_surface` is untouched and passes; the static guard asserts the declaration's exact text. | `local.rs`; `version_floor_boundary.rs` — `the_declared_version_is_not_edited` |
| R8 | **No wire change, no new option, no advertisement.** | No I2NP message, RouterInfo option, or published field changed. `specs/support.toml` gains one `experimental` surface with `advertised = false`; the inventory holds 93 surfaces and **0** advertised entries. `caps` and the published RouterInfo are unchanged. | `specs/support.toml` — `netdb.api-version-floors`; the registry's surface tally |
| R9 | **No migration, and no operator knob.** The floors are specification constants. The static guard asserts that the daemon's configuration module names neither floor constant nor a floor-shaped key, so an operator cannot lower them. A peer that publishes no version is refused — the conservative, spec-compatible default. | `version_floor_boundary.rs` — `no_configuration_key_can_lower_a_floor`, `the_guard_reports_a_configuration_knob` |
| R10 | **The evidence: the per-peer version is live in the profile store and follows the record.** | `Profile::published_version` carries the three-state evidence; `classify` refreshes it from the peer's signed RouterInfo on every insert and adoption, so a peer that upgrades its declared version is re-evaluated. A peer whose stored RouterInfo is gone has no profile at all (the store removes the two together), so it cannot be used — a stricter outcome than a refusal. | `profile.rs` — `published_version`, `set_published_version`; `profile_store.rs` — `the_profile_carries_the_published_api_version`, `the_version_evidence_is_refreshed_when_the_record_is_replaced`; `version_eligibility_mesh.rs` — `a_peer_that_upgrades_its_version_is_re_evaluated`, `a_peer_whose_record_is_gone_is_not_used_after_a_restart` |
| R11 | **The private-mesh lane: the two decisions are taken over a real mesh and recorded.** | Five peers publishing five different API versions; the tunnel-build and DSM selections refuse the three that fail the floor and count each refusal by reason; the receipts carry the counts; the fail-closed case is asserted; the evidence artifact holds counts and digests only. | `crates/i2pr-daemon/tests/version_eligibility_mesh.rs`; `tests/integration/network-participation/evidence/plan491/result.json`; the floor's `plan491-mesh-lane` and `plan491-evidence-deterministic` steps |
| R12 | **The offline controls, with an inverted probe per invariant.** | The static guard scans five production sources and asserts five structural invariants. Each rule has an inverted probe — an edited declaration, a restated floor, an enforcement point without the gate, a configuration knob, and a receipt carrying an identity must all be reported — and a benign probe proves the scanner ignores comments and `#[cfg(test)]` blocks. | `crates/i2pr-daemon/tests/version_floor_boundary.rs` — all thirteen tests |

---

## 5. Work packages and outcomes

### WP1 — the comparator

`crates/i2pr-proto/src/common/version.rs` owns the comparator and nothing
else. `ApiVersion` holds up to four `u16` components; the parse is strict
(one to four dotted components, each a non-empty run of ASCII digits whose
value fits in a `u16`), so a malformed value is a *state*, never an error and
never a truncated number. The comparison is component-wise and numeric; when
one side runs out of components while the other still has one, the shorter
version is *less* — the rule the pinned Java reference implements in
`net.i2p.util.VersionComparator`.

`PublishedVersion` keeps three facts apart: `Known`, `Unknown` and
`Uncomparable`. `VersionEligibility::evaluate` is the decision: an unknown or
uncomparable version is refused whatever the floor is, because "not
comparable" is never "good enough".

**Outcome:** 17 unit tests, including a deterministic totality property test
over a generated corpus.

### WP2 — the floors

The specification's obligation rows are **data**:

| row version | in force from | tunnel-build peers | DSM floodfill targets |
| --- | --- | --- | --- |
| 0.9.62 | 0.9.68 | yes | yes |
| 0.9.59 | 0.9.63 | yes | yes |
| 0.9.58 | 0.9.62 | yes | no |
| 0.9.51 | 0.9.58 | yes | yes |
| 0.9.36 | 0.9.46 | yes | no |
| 0.9.28 | 0.9.34 | no | yes |

`floors_in_force(declared, release)` applies a row when the router's
declared version is at or above the row's version **and** the network release
named in the row's "as of" clause has been reached; the highest applicable
row wins. The release is the API level of the pinned specification snapshot
(`0.9.69`, the level `specs/protocols/02-i2np.md` describes), which is past
`0.9.68` — so the `0.9.62` row is in force and both floors are `0.9.62`.

### WP3 — the evidence

`Profile` gained one field: the peer's published version and its
comparability state. `classify` refreshes it from the peer's signed
RouterInfo on every insert, replacement and adoption. The evidence is *not*
persisted in the profile projection: the stored RouterInfo is the authority,
and a restart re-reads it — which is the plan's stated restart semantics and
avoids a projection-format change with no migration.

### WP4 — the tunnel-build enforcement point

`DestinationPeerCandidate::from_validated` changed from `Option` to
`Result<Self, CandidateRefusal>`. The refusal vocabulary is closed:
`UnknownVersion`, `UncomparableVersion`, `BelowVersionFloor`, `Unqualified`.
The version decision is taken **first**, before the family, key and endpoint
facts, so a peer that cannot be compared is refused without being inspected
further.

The projection counts each refusal by reason, and
`select_projected_destination_path` turns an empty projection into the
*reason* it is empty. The two live call sites in `service_product.rs` now
use it, so the terminal an operator sees is
`NoEligibleVersionPeers` rather than a bare `NoCandidates`.

### WP5 — the DSM enforcement point

`PublicationCoordinator::nearest_eligible_floodfills` returns the eligible
targets *and* the counted refusals; `nearest_floodfills` is now the target
view of it, so a below-floor floodfill is not a publication target even when
a caller uses the older entry point. `NetDbTunnelCoordinator::begin_publication`
refuses a below-floor floodfill with the typed error and counts the refusal
in its own counters.

### WP6 — the receipts

`VersionEligibilityReceipt` is a separate, bounded log from Plan 488's
participation receipts, because it describes a *decision* rather than the
store's observed state. The driver's `select_tunnel_build_peers` and
`select_dsm_targets` make the decisions and record them, so a refusal is
never a silent skip.

### WP7 — the controls

The static guard, its five inverted probes and its benign probe; the
comparator's property test; and the private mesh lane.

### Design decisions

1. **The floor is derived, never transcribed.** The plan's stop condition
   forbids pinning the floors "without hand-maintaining two copies of the
   truth". The obligation rows are the only copy; the floors follow from
   them and from the declared level. A static guard fails if the policy
   module ever restates a floor as a literal.
2. **The constants live in `i2pr-netdb`, not `i2pr-proto`.** The plan
   suggested `i2pr-proto`. The derivation is keyed on
   `CONTROLLED_ROUTER_VERSION`, which `i2pr-netdb` owns; `i2pr-proto` cannot
   see it without a dependency inversion or a duplicated literal. Keeping
   the derivation and the declaration in one crate is what makes decision 1
   true. **Recorded deviation** from the plan's stated file layout.
3. **The two enforcement points are the live selections, not
   `i2pr-client/src/pool.rs`.** The plan offered "the tunnel-build peer
   selection" as the alternative. `i2pr-client`'s pool manages tunnel
   *lifecycles* and consumes peers chosen upstream; the live selection is
   the daemon's `destination_peers.rs` (and the netdb selection policy
   beneath it). **Recorded deviation**, in the plan's own terms.
4. **The publication fixtures now publish a version.** Under the new floor a
   peer that publishes no version is refused. The fixtures were updated
   rather than the floor weakened: refusing a versionless peer is the
   intended fail-closed behaviour, and the update makes the fixtures
   honest about what they claim to be.
5. **The `Uncomparable` state is kept even though the signed RouterInfo
   validation already rejects most malformed versions.** The
   `ProtocolVersion` validation accepts any non-empty digits-and-dots
   string, so `"0.9.62."`, `"0.9.65536"` and a five-component version all
   pass the signed-record validation and are *not* comparable. The state is
   reachable, and the comparator must model it.
6. **The version decision precedes the other facts in the build-peer
   projection.** A peer whose version cannot be compared is refused before
   the family, key and endpoint facts are inspected, so the refusal reason
   is the version, not a downstream symptom.
7. **The receipts are a separate log.** Folding the version decisions into
   Plan 488's participation receipts would have meant changing that closed
   receipt's shape and its digest; a separate log keeps both records
   readable and both digests stable.
8. **The floor is a constant, not a configuration key.** The plan forbids an
   operator lowering it, and a static guard asserts that the daemon's
   configuration module names neither floor.

---

## 6. Invariant review

| Invariant | Review | Verdict |
| --- | --- | --- |
| The declaration is not edited | `CONTROLLED_ROUTER_VERSION` is still `"0.9.62"`; Plan 285's pinning test is untouched and passes; the static guard asserts the declaration's exact text and has an inverted probe for an edited declaration. | **Held.** |
| The comparator is total and typed | Every input maps to a value or to `Unknown`/`Uncomparable`; no input panics; the property test covers a generated corpus. | **Held.** |
| Fail closed on the floors | An unknown, uncomparable or below-floor peer is never a build peer and never a DSM target; an empty projection fails closed with `NoEligibleVersionPeers`. | **Held.** |
| The numeric-comparison rule is the specification's | Component-wise numeric comparison; the catch-all line is named and asserted below every obligation row. | **Held.** |
| No wire change, no new option, no advertisement | No I2NP message, RouterInfo option, or published field changed; the inventory holds 93 surfaces and 0 advertised. | **Held.** |
| Bounded and cheap | The comparator allocates nothing; the version evidence is one small per-peer field; the refusal counts are three `u32`s; the receipt log is bounded by the driver's existing receipt cap. No unbounded state was added. | **Held.** |
| Both enforcement points are recorded | Every refusal is counted by reason; the receipts carry the counts; the lane asserts the counts. | **Held**, with the qualification in §11 (the receipt log's owner is not yet a live consumer of the product loop). |

**Lock order and contention.** The two enforcement points read the validated
store and the profile store, both of which are single-writer and are read
under the same short, non-awaiting guards as before. No lock is held across
an `.await` that can block on another lock holder, and the recorded
**bridge → coordinator** order is unchanged. The new code adds no lock at
all.

**Cancellation.** A selection is a synchronous, bounded operation over an
already-bounded store; there is nothing to cancel inside it. A cancelled
build leaves the candidate set consistent and the budget consumed, exactly
as before the plan: the version gate only removes candidates, it never
partially consumes a budget.

**Restart.** The profile store re-reads each retained peer's version from
its stored RouterInfo when the store is adopted, so a restart re-derives
the evidence rather than trusting a persisted copy. A peer whose stored
RouterInfo is gone has no profile at all — the store removes the record and
the profile together — so it cannot be used, which is stricter than a
refusal.

---

## 7. Verification

### 7.1 Focused runs

```text
cargo test --locked -p i2pr-proto --lib common::version -- --test-threads=1
cargo test --locked -p i2pr-netdb --lib version:: -- --test-threads=1
cargo test --locked -p i2pr-netdb --lib selection:: -- --test-threads=1
cargo test --locked -p i2pr-netdb --lib publication:: -- --test-threads=1
cargo test --locked -p i2pr-netdb --lib profile_store:: -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib destination_peers:: -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib netdb_tunnels:: -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_eligibility_mesh -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

### 7.2 The AGENTS.md routine floor

The routine floor from `AGENTS.md` was run three times on the closing line.
The first two runs were **red** and each caught a real defect; the third run,
on the fixed tree, is **green** in every step.

| run | tree | outcome | what it caught |
| --- | --- | --- | --- |
| 1 | `dc36e2d7` + `526104ae` + `f566b97f` | **2 FAIL / 82** — `cargo-doc`, `cargo-test-workspace` | Two rustdoc link errors, and one workspace-test failure in `i2pcontrol_tunnels` that did not reproduce in isolation. |
| 2 | `c195f363` | **1 FAIL / 82** — `cargo-test-workspace` | With the doc step green, the workspace run got past `i2pcontrol_tunnels` and exposed the **real** regression the plan had introduced: the live NetDB publication fixture's floodfill published no `router.version`, so the new DSM floor refused it. |
| 3 | `27c20b9c` | **0 FAIL / 82** | — |

Cargo stops the workspace test run at the first failing test binary, so run 1
masked the publication regression behind the `i2pcontrol_tunnels` failure.
A separate `--no-fail-fast` sweep of the whole workspace on the fixed tree
resolved every remaining binary at once: **195 test binaries ok, 0 failed**.

### 7.3 The failures the floor caught, and what each cost

**1. Two rustdoc links (`cargo doc`, run 1).** The selection policy's
`[`crate::version`]` link pointed at a private module, and the policy
module's `[`i2pr_proto::common::floors_in_force`]` link pointed at
`i2pr-proto`'s private `common` module. `-D warnings` turned both into
errors. **Cost:** one commit (`c195f363`), two one-line link changes. The
links were documentation-only; no behaviour changed.

**2. The live publication fixture published no version (`cargo test
--workspace`, run 2).** `netdb_tunnel_live::publication_traverses_outbound_tunnel_with_protocol_ack`
failed with `FloodfillRefusedByVersionFloor("unknown-version")`. The
fixture's floodfill published `caps=f` and nothing else, so the new floor
refused it — **the plan working exactly as designed**. The *fixture* was
wrong, not the floor: a publication target that publishes no version is not
usable, and both reference implementations behave the same way. **Cost:**
one commit (`27c20b9c`), one fixture. The same correction had already been
applied to the other publication fixtures
(`netdb_tunnel_unit.rs`, `publication.rs`); this one was missed because
cargo had stopped the run before reaching it.

**3. An intermittent `i2pcontrol_tunnels` failure (run 1, not reproduced).**
`i2pcontrol_tunnels::tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets`
failed once with "runtime transition failed: manager configuration
rejected". It **passed** in the same binary in three isolated runs, in the
full `--no-fail-fast` workspace sweep, and in floor runs 2 and 3. The
attribution is evidence-backed rather than assumed:

- the failing path is the managed-service **staging** path
  (`build_service_runtime` → `staging … failed`), which is not on the
  plan's changed path;
- the plan's change to that path is **provably a no-op** for this test: the
  test's daemon holds an *empty* validated-RouterInfo store
  (`DestinationTunnelCoordinator::new` starts empty and the test's config
  declares no bootstrap), so the version gate refuses nothing, the
  projection's `version_refusals()` is zero, and the terminal is
  `NoCandidates` — byte-identical to the pre-change
  `select_destination_path_os` on an empty candidate list;
- the other sixteen tests in the same binary — several of which create
  tunnels through the same manager — passed in the very run that failed.

**Recorded as a Low finding (§12, F4)**, not fixed here: it belongs to the
I2PControl / service-tunnel line, and a plan-of-record is required before
touching that path.

### 7.4 The floor step table

| # | Step | Outcome | Seconds |
|---|---|---|---|
| 1 | `cargo-fmt` | PASS | 4 |
| 2 | `cargo-check` | PASS | 1 |
| 3 | `cargo-build-siblings` | PASS | 0 |
| 4 | `cargo-build-static-fixture` | PASS | 0 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 1 |
| 7 | `cargo-test-workspace` | PASS | 1118 |
| 8 | `cargo-clippy` | PASS | 1 |
| 9 | `cargo-doc` | PASS | 8 |
| 10 | `cargo-test-doc` | PASS | 4 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 0 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 8 |
| 16 | `planning-tests` | PASS | 7 |
| 17 | `check-runtime-boundaries` | PASS | 0 |
| 18 | `check-console-boundaries` | PASS | 5 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 0 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 0 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `check-managed-app-package-boundary` | PASS | 1 |
| 29 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 30 | `check-managed-app-policy-boundary` | PASS | 0 |
| 31 | `check-managed-app-policy-boundary-self` | PASS | 0 |
| 32 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 33 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 34 | `check-service-tunnel-boundaries` | PASS | 0 |
| 35 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 36 | `check-managed-app-manager-boundary` | PASS | 0 |
| 37 | `check-m11-per-epoch-composition` | PASS | 0 |
| 38 | `check-service-anonymity-boundaries` | PASS | 0 |
| 39 | `check-fixture-manifest` | PASS | 0 |
| 40 | `check-ntcp2-vectors` | PASS | 0 |
| 41 | `check-ssu2-vectors` | PASS | 0 |
| 42 | `check-i2cp-vectors` | PASS | 2 |
| 43 | `check-ntcp2-interoperability` | PASS | 1 |
| 44 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 45 | `check-m11-transit-boundaries` | PASS | 1 |
| 46 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 47 | `check-sam-acceptance-evidence` | PASS | 0 |
| 48 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 49 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 50 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 51 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 52 | `check-els2-live-lane-evidence` | PASS | 2 |
| 53 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 54 | `check-outproxy-request-path` | PASS | 0 |
| 55 | `check-outproxy-wire-lane-evidence` | PASS | 1 |
| 56 | `check-config-secret-hygiene` | PASS | 0 |
| 57 | `check-workflow-validity` | PASS | 0 |
| 58 | `check-floodfill-type5-serve` | PASS | 0 |
| 59 | `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| 60 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 61 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 62 | `check-destination-tunnel-evidence` | PASS | 1 |
| 63 | `check-streaming-tunnel-evidence` | PASS | 7 |
| 64 | `check-m6-mixed-router-acceptance-evidence` | PASS | 3 |
| 65 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 66 | `check-m12-floodfill-boundaries-self` | PASS | 0 |
| 67 | `ntcp2-harness-tests` | PASS | 0 |
| 68 | `plan488-mesh-lane` | PASS | 1 |
| 69 | `plan488-evidence-deterministic` | PASS | 0 |
| 70 | `plan489-mesh-lane` | PASS | 0 |
| 71 | `plan489-evidence-deterministic` | PASS | 0 |
| 72 | `plan490-mesh-lane` | PASS | 0 |
| 73 | `plan490-evidence-deterministic` | PASS | 0 |
| 74 | `plan490-ssu2-transport-suite` | PASS | 9 |
| 75 | `plan490-transport-boundary-guard` | PASS | 0 |
| 76 | `plan491-mesh-lane` | PASS | 1 |
| 77 | `plan491-evidence-deterministic` | PASS | 0 |
| 78 | `plan491-boundary-guard` | PASS | 0 |
| 79 | `plan491-proto-suite` | PASS | 0 |
| 80 | `plan491-netdb-suite` | PASS | 68 |
| 81 | `plan491-client-suite` | PASS | 30 |
| 82 | `cargo-deny` | PASS | 2 |

**82 steps, 82 PASS, 0 FAIL.**

### 7.5 The false-pass audit

Every "passed" claim in this record was checked for the way it could have
passed without proving anything:

- **The comparator is not fed hand-written versions.** The property test
  drives a generated corpus of four thousand strings through the parser and
  asserts that each maps to a value or to an explicit state and that a
  reported value round-trips. A comparator that returned a constant would
  fail the round-trip assertion.
- **The floors are not transcribed constants.** The derivation is exercised
  by re-deriving the floors from the obligation table and comparing them
  with the shipped constants; the static guard fails if the policy module
  ever restates a floor as a literal, and its inverted probe proves the
  rule has teeth. A hand-written `0.9.62` that happened to be right would
  still be a violation.
- **The refusals are not asserted on synthetic values.** The mesh lane's
  peers are **signed** RouterInfos built by the test, and the version is
  read back out of the signed options — the same path a peer's published
  version takes. A comparator that ignored the record could not produce the
  lane's refusal counts.
- **The fail-closed terminal is not the same assertion as "no candidates".**
  The lane and the daemon test assert the *specific* error
  (`NoEligibleVersionPeers`), not merely that the call failed, and the
  selection is asserted to return no candidate at all rather than a shorter
  path.
- **The receipts cannot name a peer.** The static guard asserts that the
  receipt *type* carries no peer identity, and the lane asserts that the
  rendered receipt and the rendered evidence file contain no peer hash, no
  address, no key, and no version string. The receipt's fields are counts
  and labels by construction.
- **The evidence file cannot leak or drift.** The lane asserts, before
  writing it, that the rendered evidence contains no peer hash, no address,
  no key, and no version string; the floor's `plan491-evidence-deterministic`
  step proves the artifact is byte-identical across runs, so it cannot have
  been hand-edited between runs.
- **The static guard cannot have gone blind.** Every rule has an inverted
  probe that fails if the scanner stops reporting an edited declaration, a
  restated floor, an enforcement point without the gate, a configuration
  knob, or a receipt carrying an identity. They are not the same assertion
  as the production scan. A benign probe proves the scanner ignores
  comments and `#[cfg(test)]` blocks, so the inverted probes cannot pass
  merely because the scanner reports everything.
- **The advertisement claim is executable, not only declarative.**
  `specs/support.toml` is parsed by the floor's tooling check, the new
  surface carries `advertised = false`, and the inventory's advertised count
  is asserted to be zero.
- **No `|| true`, no `continue-on-error`, no filename filtering, no fake
  peer environment, no broad exclusion, and no early return-success** was
  added to any script, and no production wire behaviour was changed to make
  a test pass. The floor script itself is a plain sequence of `run` steps
  that records each step's exit status.

---

## 8. Migration and compatibility

- **No wire change.** The version was already in the RouterInfo.
- **No `router.version` change.** The declaration stays `0.9.62`, and Plan
  285's pinning test is untouched.
- **No migration.** The profile projection's format is unchanged: the
  version evidence is re-derived from the stored RouterInfo, never
  persisted. A profile written by an earlier build decodes unchanged.
- **No new configuration key.** The floors are specification constants.
- **Behavioural consequence, stated plainly.** A router that adopts a store
  of peers whose records publish no `router.version` will refuse those peers
  for tunnel building and for DSM publication. That is the intended
  fail-closed behaviour and it matches the reference implementations, which
  also refuse a peer whose version they cannot establish.

---

## 9. Security and contention

- **Fail closed is a security property, not a convenience.** A peer whose
  version cannot be established is not used. The alternative — treating an
  unknown version as "newest" — is exactly the under-compliance the plan
  exists to prevent.
- **No secret is read, stored or reported.** The comparator reads the public
  `router.version` option. The receipts carry counts and stable reason
  labels; the static guard asserts that the receipt type carries no peer
  identity, and the lane asserts that neither the rendered receipt nor the
  rendered evidence file contains a peer hash, an address, a key, or a
  version string.
- **The evidence file is sanitized by construction.** It holds counts and
  digests of counts. The digests cover the gate, the population, the
  accepted count and the refusal counts — no peer identity, because the
  receipt holds none.
- **No unbounded state.** The version evidence is one small per-peer field;
  the refusal counts are three saturating `u32`s; the receipt log shares the
  driver's existing bounded receipt cap.
- **Contention.** No new lock; both enforcement points read the same
  single-writer stores under the guards that already existed; the
  bridge → coordinator order is unchanged.

---

## 10. Documentation and operational changes

- `docs/architecture/i2pr-proto.md` — the comparator, the three states, the
  obligation rows, the derivation, the catch-all line, and the property
  test.
- `docs/architecture/i2pr-netdb.md` — the derived floors, the typed
  eligibility decisions, the counted refusal vocabulary, and why the
  constants live in this crate.
- `docs/architecture/i2pr-daemon.md` — the two enforcement points, their
  typed refusals, the fail-closed terminal, and the receipts.
- `specs/CONFORMANCE.md` — the Plan 491 section: what is claimed, what is
  not, the totality of the comparator, the numeric rule, the derivation,
  the fail-closed behaviour, and the counted refusals.
- `specs/support.toml` — the `netdb.api-version-floors` surface
  (`experimental`, `advertised = false`) and the plan's status token. The
  inventory holds **93** surfaces and **0** advertised entries.
- `plans/implementation/network-participation/491-api-version-floors-and-peer-eligibility.md`
  — the closing status, the refined production surface, and the two
  recorded deviations from the plan's stated file layout.

No operational change: no new configuration key, no new listener, no new
file on disk, no new dependency.

---

## 11. Known limitations

- **The receipt log's owner is not yet a live consumer of the product loop.**
  Plan 488's driver is deliberately not wired into the router's tunnel
  building, and Plan 491 did not change that. The *decisions* are enforced
  live — the two enforcement points refuse at the point of use, and the
  daemon's own counters (`publications_refused_by_version_floor`,
  `CandidateProjectionSummary`) count them — but the plan's
  `VersionEligibilityReceipt` log is produced by the driver, so it is
  exercised by the private mesh lane and the offline controls rather than by
  a live tunnel build. This is the same shape as Plan 490's F1.
- **The floors have not been exercised against a pinned stock peer.** The
  reference implementations' constants were read at their pinned commits and
  the derivation reproduces their value, but i2pr's own selection has not
  been observed against a live i2pd or Java peer, because that needs the
  authorized two-host topology Plan 493 owns.
- **The peer set in the private mesh is synthetic.** The lane's peers are
  signed RouterInfos built by the test, not peers learned from the
  network. The lane proves the *policy*, not the network's composition.
- **The version evidence is per-peer, not per-record-generation.** A peer
  that republishes a *different* version keeps the latest evidence, which is
  the intended behaviour, but the store keeps no history of the versions it
  has seen.
- **The comparator accepts at most four components.** The specification's
  widest row is `0.6.1.10` (four), so the bound is not expected to bind; a
  five-component version is *uncomparable* rather than truncated.
- **One unrelated test is intermittent under load.** The floor's first run
  reported a managed-tunnel staging failure in `i2pcontrol_tunnels` that did
  not reproduce afterwards; see §7.3 and §12 F4. It is outside the plan's
  path and is recorded rather than absorbed.

---

## 12. Findings by severity

### Medium — open

**F1 — The receipt log is not produced by a live product loop.** The
`VersionEligibilityReceipt` log is owned by the participation driver, which
Plan 488 deliberately left unwired from the router's tunnel building. The
live enforcement points refuse and count in their own counters, so the
*policy* is enforced in production, but the plan's receipt shape is not yet
produced by a live build. The wiring is the consumer's change and belongs to
the phase that consumes it. **Not fixed here**: fixing it means wiring
Plan 488's driver into the product loop, which is a different plan's scope.

**F2 — No stock-peer evidence for the floors.** The floor value is derived
from the specification's table and cross-checked against the pinned
references' source constants, but i2pr has not refused a *live* pinned peer
that is below the floor, because that needs the authorized two-host topology
Plan 493 owns. **Not fixed here**: it needs external topology this plan
forbids.

### Low — open

**F3 — `check-runtime-boundaries.sh` has no section for the new
`i2pr-netdb` policy module.** The comparator and the policy module are pure
— no clock, no socket, no task, no allocation in the comparison path — but
the script has no `i2pr-netdb` section, so that purity is not policed by
the repository's own checker. The crate-local guard
(`version_floor_boundary.rs`) covers the plan's structural invariants but
not the runtime-neutrality rule. **Recorded, not fixed**: adding a section
to the script is a boundary-checker change, and the rule is to fix the
boundary rather than weaken a script — a new section here would need its
own plan-of-record, exactly as Plan 362 did for `i2pr-api`.

**F4 — An intermittent `i2pcontrol_tunnels` managed-tunnel failure, outside
the plan's path.** The floor's first run reported
`tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets` failing
with "runtime transition failed: manager configuration rejected". It did
not reproduce in three isolated runs of the same binary, in the full
`--no-fail-fast` workspace sweep, or in the two later floor runs. The plan's
change is not on the failing path and is provably a no-op there (§7.3,
failure 3). **Recorded, not fixed**: the residual risk is a load-sensitive
race in the managed-service staging path, which belongs to the I2PControl /
service-tunnel line and needs a plan-of-record before that path is touched.

**F5 — The comparator's four-component ceiling is an unstated assumption in
the specification's terms.** The specification's table's widest row is
four components, so the bound does not bind today; a future five-component
row would make such versions uncomparable rather than comparable.
**Recorded, not fixed**: widening the ceiling is a one-line change with no
caller, and the plan's scope was the current table.

### Informational

- The signed-RouterInfo validation already rejects a `router.version` that
  is not digits-and-dots, so most malformed values cannot reach the
  comparator at all. The `Uncomparable` state is nevertheless reachable —
  `"0.9.62."`, `"0.9.65536"` and a five-component version all pass the
  signed-record validation and are not comparable — and the comparator models
  it.
- The two reference implementations publish `router.version` `0.9.70` and
  apply the floor `0.9.62`. The derivation in this plan reproduces `0.9.62`
  from the specification's table and the router's *declared* level
  (`0.9.62`), so the constants are not a transcription.

### No findings

- The declared version is unchanged and its pinning test passes.
- No advertisement: 93 surfaces, 0 advertised.
- No new configuration key, no new dependency, no new I2NP message, no new
  RouterInfo option.
- No unbounded state, no new lock, no `.await` across a lock.
- No secret in a receipt, in the evidence file, or in a log.
- The reference pins are unchanged and nothing was patched or vendored.

---

## 13. Roadmap disposition

- **§6 dependency graph:** `491 API-version floors + peer eligibility` is
  marked `(passed)`.
- **§7 milestone table:** row 491 becomes `closed` /
  `passed-api-version-floors-and-peer-eligibility`, with the implementation
  commit recorded.
- **§12 status summary:** the phase is closed, and **Plan 492** (the transit
  participation posture) becomes the next actionable phase.
- The phase order is unchanged: 488 → 489 → 490 → **491** → 492 → 493 → 494.

---

## 14. Unblock audit

Registered work that lists the just-closed milestone as a dependency:

| Plan | Relationship to 491 | Other hard dependencies | Interface dependencies | Disposition |
| --- | --- | --- | --- | --- |
| 492 — transit participation posture | Its predecessor lineage names 491 (the version floors are the selection policy's peer-eligibility rule, and the posture must build on the *floors* the router now applies). | Closed: Plan 254/268 (the transit ingress owner and the passed M11 one-family qualification), Plan 488 (the selection policy), Plan 454's governor. | Stable: the transit dispatch order, the transit state machine, the selection policy's `VersionGate`. | **Remains `ready`.** Plan 492 was already `ready`; its dependency on 491 is now *closed*, so nothing is newly unblocked and nothing regresses. |
| 493 — two-family public qualification | Depends on 491 transitively (the floors decide which peers the qualification may use). | **Blocked**: the authorized two-host topology (Plan 431, stopped at exactly that boundary; Plans 472/476/480 own the runner, Plan 449 the portable LAN lane) and Plan 435's transport activation. | Stable. | **Remains `blocked`.** 491's closure satisfies one interface dependency; the blocking topology dependency is untouched. |
| 494 — floodfill role and `caps=f` | Names "Plan 491's DSM floor" as a predecessor. | **Blocked** on 493's two-family evidence. | Stable: the DSM floor is now a closed, derived constant. | **Remains `blocked`.** The interface dependency is closed; the blocking evidence dependency is untouched. |

No registered plan lists 491 as a hard dependency while having every other
hard dependency closed, so **no plan moves to `ready` in this commit**.
Plan 492 was already `ready` before this closure and stays `ready`: it is
the next actionable phase, and it needs no new plan-of-record.

---

## 15. Open items

- **Plan 492** is the next actionable phase: the transit participation
  posture. It is offline/private-mesh testable and needs no external
  topology.
- **Plan 493** remains blocked on the authorized two-host topology and Plan
  435's transport activation.
- **Plan 494** remains blocked on 493's two-family evidence; the bandwidth
  tier may still end in an honest "unknown class", because bandwidth
  metering is unresolved in `i2pr` and in both reference routers.
- **F1** (the receipt log's owner is not a live consumer) belongs to the
  phase that wires Plan 488's driver into the product loop.
- **F2** (no stock-peer evidence) belongs to Plan 493's topology.
- **F3** (no `i2pr-netdb` section in `check-runtime-boundaries.sh`) needs
  its own plan-of-record, as Plan 362 did for `i2pr-api`. The rule stays:
  fix the boundary, never weaken the script.
- **F4** (the intermittent `i2pcontrol_tunnels` managed-tunnel staging
  failure) belongs to the I2PControl / service-tunnel line.
