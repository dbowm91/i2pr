# Plan 491 — API-version floors and peer eligibility: honoring the obligations of the published `router.version`

**Status:** closed as `passed-api-version-floors-and-peer-eligibility` (phase 4 of
[network participation](../../subsystems/network-participation-roadmap.md));
the closure record is
`plans/closure/network-participation/491-status.md`
**Owner:** `i2pr-proto` (the version comparator and the specification's
obligation table), `i2pr-netdb` (the derived floor constants, the per-peer
version evidence, and the selection gate), `i2pr-daemon` (the two
enforcement points and the receipts)
**Baseline:** `work/plans-477-480-router-recovery@c43f3404`
**Authoritative predecessor:** `plans/closure/netdb/285-status.md` (the
`router.version` declaration pinned to the implemented I2NP surface),
`plans/closure/floodfill/306-status.md` (the reachability caps and the
bandwidth-tier boundary)
**Research input:** the I2NP specification's API-version table (the
minimum-peer obligations attached to 0.9.62, the numeric-comparison rule,
and the "0.7.9 or lower" catch-all), the pinned i2pd 2.61.0 and Java I2P
2.13.0 peer-admission behaviour
**Predecessor lineage:** 285 (the version declaration) → 488 (peer profile
store) → 489 (routing table) → 490 (introducer role) → **491**

## Objective

Make the router **honor the minimum-peer obligations that its own published
`router.version` attaches**, and make every peer-eligibility decision
traceable to a peer's *published* version rather than to an assumption.

One bounded outcome: **the router parses and compares published API
versions, and refuses to build tunnels through peers below the floor and to
send DSMs to floodfills below the floor — with both decisions recorded.**

### Why this is a real obligation, not a nicety

The specification's version table attaches to API **0.9.62** — the level
`i2pr` declares (`CONTROLLED_ROUTER_VERSION` in
`crates/i2pr-netdb/src/local.rs`) — two duties:

- *minimum peers will build tunnels through, as of 0.9.68*;
- *minimum floodfill peers will send DSM to, as of 0.9.68*.

A router at 0.9.62 or above must therefore apply those floors. `i2pr`
currently has **no peer-version floor logic anywhere in the tree** (the
only version comparisons in the repository are the SAM protocol's version
range), so a router that joined the network would build tunnels through
peers the network has agreed to stop using, and would send DSMs to
floodfills below the agreed floor. Both are network-visible mistakes, and
both are exactly the kind of thing a reference implementation enforces.

## Why ready

- **Hard dependencies are closed.** The `router.version` declaration is
  closed and pinned to the implemented I2NP surface by a test (Plan 285);
  the reachability-caps and bandwidth-class work is closed at Plan 306's
  boundary with the governor landed by Plan 454; Plan 488 supplies the peer
  store whose entries carry the published version.
- **Interface dependencies are stable.** `RouterInfo`'s option parsing and
  `protocol_version()` are closed surfaces in `i2pr-proto`; the comparator
  is a new, small, pure type.
- **No external topology is needed.** The floors are decided from a peer's
  *published* RouterInfo, which the router already receives and stores.

## Current implementation evidence

Present today:

- `crates/i2pr-proto/src/common/mod.rs` — the RouterInfo option mapping and
  the `protocol_version()` accessor that returns the published
  `router.version` string.
- `crates/i2pr-netdb/src/local.rs:160` — `CONTROLLED_ROUTER_VERSION =
  "0.9.62"`, with the comment that the value "may only be as high as the
  message surface `i2pr` actually implements" and is pinned by a test that
  fails if the declaration outruns the surface.
- `crates/i2pr-netdb/src/store.rs` — the bounded validated-RouterInfo store
  (the version is therefore already available per peer), and its
  `f`-capability advertiser count.
- `crates/i2pr-daemon/src/bandwidth_eligibility.rs` and Plan 454's
  governor — the existing shape for a policy that is decided from a
  peer's published properties and is revalidated at the point of use.

Missing today:

- **no version comparator** — the published version is a string and is
  never compared;
- **no floor constants** — the minimum-peer floors are not represented
  anywhere;
- **no enforcement point** — tunnel-build peer selection and the DSM
  publication target selection do not consult a peer's version;
- **no per-peer version evidence in the profile store** — Plan 488's
  store must carry it.

## Invariants that must not regress

1. **The declaration is not edited.** `CONTROLLED_ROUTER_VERSION` stays
   pinned to the implemented I2NP surface; this plan adds *compliance* with
   the declared level, never a change to it. A peer that refuses the
   declaration is a finding, not a licence to edit it.
2. **The comparator is total and typed.** Every published version string
   maps to a comparable value or to an explicit "unknown/uncomparable"
   state; a malformed or absent version never panics and never silently
   compares as "newest".
3. **Fail closed on the floors.** A peer whose version is unknown,
   unparseable, or below the floor is **not** used for the gated purpose.
   Silence is not consent.
4. **The numeric-comparison rule is the specification's.** `0.9.13` implies
   support for `0.9.12`; a version at or below the catch-all line is
   handled by the catch-all rule, not by a lexicographic comparison.
5. **No wire change, no new option, no advertisement.**
6. **Bounded and cheap.** The version evidence is a small per-peer field;
   it adds no unbounded state, and the comparator allocates nothing.
7. **Both enforcement points are recorded.** Each refusal is a counted,
   explainable decision — never a silent skip.

## Scope

**In:**

- the **version comparator**: a total, typed comparison of published
  `router.version` strings, with the specification's numeric rule and its
  catch-all;
- the **floor constants**: the minimum peer version for tunnel building and
  the minimum floodfill version for DSM, pinned to the specification's
  table and to the level this router declares;
- the **per-peer version evidence** in Plan 488's profile store;
- the **first enforcement point**: tunnel-build peer selection refuses
  peers below the tunnel floor;
- the **second enforcement point**: DSM publication refuses floodfills
  below the DSM floor;
- the receipts for both decisions;
- the offline controls.

**Explicitly out:**

- **editing the declared version** (explicitly forbidden above);
- the **bandwidth-tier derivation** and the metering behind it — that is
  Plan 494's dependency and remains open in the reference implementation
  too;
- **caps=f** and the floodfill role (Plan 494);
- **transit participation** (Plan 492);
- **public-network** operation;
- a new **I2NP message** or a new RouterInfo option.

## Required production changes

- **`crates/i2pr-proto/src/common/version.rs`** (new): `ApiVersion` — the
  typed comparator over published version strings, with `Unknown` and
  `Uncomparable` as first-class states — plus the specification's
  minimum-peer obligation rows and the derivation of the floors from them.
- **`crates/i2pr-netdb/src/version.rs`** (new): the floor constants
  (`MIN_PEER_VERSION_FOR_TUNNEL_BUILD`, `MIN_FLOODFILL_VERSION_FOR_DSM`),
  derived from that table and from the level this router declares. The
  constants live here rather than in `i2pr-proto` because the derivation is
  keyed on `CONTROLLED_ROUTER_VERSION`, which `i2pr-netdb` owns; keeping the
  derivation and the declaration in one crate is what stops the floors from
  becoming a second, hand-maintained copy of the truth.
- **`crates/i2pr-netdb/src/profile.rs`** (extended from Plan 488): the
  per-peer published version and its comparability state.
- **`crates/i2pr-netdb/src/selection.rs`**: the version gate in the pure
  selection policy, with the counted refusals.
- **`crates/i2pr-daemon/src/destination_peers.rs`** (the tunnel-build peer
  selection): the first enforcement point, with a typed refusal. The plan
  offered `i2pr-client/src/pool.rs` "or the tunnel-build peer selection";
  the live selection is the daemon's, because `i2pr-client`'s pool manages
  tunnel lifecycles and consumes peers chosen upstream.
- **`crates/i2pr-netdb/src/publication.rs`** and
  **`crates/i2pr-daemon/src/netdb_tunnels.rs`** (the publication target
  selection): the second enforcement point, with a typed refusal. The plan
  offered `crates/i2pr-daemon/src/service_product.rs` "or the publication
  target selection".
- **`crates/i2pr-daemon/src/netdb_participation.rs`**: the receipts that
  record both decisions with counts and reasons.

## Ordered work packages

1. **WP1 — the comparator.** The type, its states, the specification's
   numeric rule, the catch-all rule, and the totality property (a property
   test over a corpus of version strings, including malformed and missing
   ones).
2. **WP2 — the floors.** The two constants, their derivation from the
   specification's table and from the level this router declares, and a
   test that fails if the declared level changes without the floors being
   re-derived.
3. **WP3 — the evidence.** The per-peer version in the profile store, its
   lifetime, and its behaviour when the peer's RouterInfo is refreshed.
4. **WP4 — the tunnel-build enforcement point.** The selection filter, the
   typed refusal, and the fallback when *every* candidate is below the
   floor (the tunnel build must fail closed, with a reason, not loop).
5. **WP5 — the DSM enforcement point.** The publication target filter, the
   typed refusal, and the behaviour when no eligible floodfill remains.
6. **WP6 — the receipts.** Counted decisions with reasons: peers refused
   by floor, publications refused by floor, and the version states
   encountered.
7. **WP7 — the controls.** The offline controls and the inverted probes.

## Failure, cancellation, restart, and contention semantics

- **An unknown or unparseable version** is a refusal, not a pass; it is
  counted separately from a below-floor refusal so an operator can see
  which case occurred.
- **A cancellation** during a filtered selection leaves the candidate set
  consistent and the budget consumed.
- **A restart** re-reads the peers' versions from their stored RouterInfos;
  a peer whose stored RouterInfo is gone is treated as unknown, hence
  refused.
- **Contention**: the two enforcement points read the profile store; the
  store remains single-writer, and no lock is held across an `.await` that
  can block on another lock holder. The recorded lock order (**bridge →
  coordinator**) still holds.

## Compatibility and migration

- **No wire change.** The version is already in the RouterInfo.
- **No `router.version` change.**
- **No migration.** A router that has never seen a peer's version refuses
  that peer for the gated purposes — a conservative, spec-compatible
  default.
- **No new configuration key**: the floors are specification constants, not
  operator knobs. An operator must not be able to lower them.

## Required tests

**Offline controls**:

- the comparator is total: every version string maps to a value or to an
  explicit state, and no input panics;
- the numeric rule: `0.9.13` is at least `0.9.12`; `0.9.9` is not at
  least `0.9.10`; the catch-all line is handled by the catch-all rule;
- a malformed version, an empty version, and a missing version are all
  "unknown" and are refused;
- the floor constants match the specification's table for the level this
  router declares, and the test fails if the declared level changes;
- the tunnel-build filter refuses a below-floor peer and fails closed when
  no candidate qualifies;
- the DSM filter refuses a below-floor floodfill and fails closed when no
  eligible floodfill remains;
- the receipts count each refusal with its reason;
- an inverted probe for each invariant.

## Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-proto --all-targets -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## Documentation updates

- `docs/architecture/i2pr-proto.md` — the comparator and the floors;
- `docs/architecture/i2pr-netdb.md` — the per-peer version evidence;
- `docs/architecture/i2pr-client.md` — the tunnel-build enforcement point;
- `specs/CONFORMANCE.md` — the version-obligation evidence;
- `plans/subsystems/network-participation-roadmap.md` and
  `plans/registry.md` — the phase's row.

## Acceptance criteria

1. The comparator is total and typed; a malformed, empty, or missing
   version is "unknown" and is refused.
2. The two floor constants are pinned to the specification's table for the
   level this router declares, and a test fails if either drifts.
3. The tunnel-build path refuses a below-floor peer and fails closed with a
   reason when no candidate qualifies.
4. The DSM path refuses a below-floor floodfill and fails closed with a
   reason when no eligible floodfill remains.
5. Both decisions are counted and explainable in receipts; no secret, no
   address, and no key appears in them.
6. The declared `router.version` is unchanged, and its pinning test still
   passes.
7. The offline controls pass, and each invariant has an inverted probe.
8. The routine floor is green on the closing tree, with the step table in
   the closure record.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the comparator cannot be made total and typed;
- the floors cannot be pinned to the specification's table and to the
  declared level without hand-maintaining two copies of the truth;
- either enforcement point would have to fail *open* to keep the router
  working;
- the lane would need a public bind, an external topology, or a public
  network;
- any step would require editing `router.version`, `caps`, the reference
  pins, or a boundary script to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/network-participation/491-status.md` with the full closure
  content the repository requires;
- the registry and roadmap rows advanced to closed;
- the unblock audit recorded, with any newly ready plan moved to `ready` in
  the same commit.

## Handoff notes

- **Do not edit the declared version.** This plan exists so that the router
  complies with the level it already claims; changing the claim to dodge a
  floor would invert the whole discipline.
- **Fail closed.** A peer that does not publish a parseable version is not
  usable for the gated purposes. An under-complying router is a defect, not
  a safe default.
- **Reference comparison.** i2pd 2.61.0 and Java I2P 2.13.0 both apply the
  minimum-peer floors when selecting tunnel hops and DSM targets; they are
  the behavioral oracles for the two enforcement points, and the
  specification's version table is the authority for the constants.
