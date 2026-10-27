# Plan 509 — the lookup-target API-version decision

**Subsystem:** `network-participation`
**Status:** registered
**Predecessor:** Plan 503 (the version-gated floodfill selection), Plan 491 (the
API-version floors), Plan 502 (the transit owner's own structures)
**Successor:** none yet

---

## 1. Objective

Close Plan 503's recorded Medium finding — *"the lookup-target selection is not
gated: the router still asks a below-floor floodfill for a lookup"* — by making
the product decision explicit on the record, proving the decision's load-bearing
claims executably, and installing a static guard so the decision cannot drift.

The deliverable is **a decision plus its proof plus its enforcement**, not a
speculative protocol change.

**The decision this plan makes: the lookup target is NOT version-gated.** The
router asks the floodfills its own routing policy selects, and every answer is
validated by the router before it is trusted. No API-version floor is applied to
the lookup path, because no floor is derivable and the specification imposes no
lookup duty.

## 2. Why ready

- The finding is **registered and owned**: Plan 503's closure record names it as
  a Medium finding, records it in the unblock audit as *"New, open. Needs a
  plan-of-record"*, and names it as *"the next candidate on this line"*.
- The decision's inputs are **all already in the tree** — the specification's
  obligation table, the derived floors, the two reference implementations'
  documented rules, the lookup state machine, and the answer-validation path.
  Nothing needs to be fetched, pinned, or measured to decide it.
- The lookup path is **live in production**, so the decision is about a path
  that runs: `crates/i2pr-daemon/src/service_product.rs:5592` calls
  `begin_lease_lookup`, and `crates/i2pr-daemon/src/service_product.rs:8520`
  calls `begin_encrypted_lease_lookup`.
- The **structure already encodes the decision**: `i2pr_netdb::VersionGate` has
  exactly two variants, because the specification imposes exactly two duties. A
  lookup gate cannot be expressed in the tree's own vocabulary without adding
  a third duty that does not exist.

## 3. Current implementation evidence

### 3.1 The specification imposes no lookup duty

`crates/i2pr-proto/src/common/version.rs:362` holds
`MINIMUM_PEER_OBLIGATIONS`, six rows copied from the specification's
API-version table. Each row carries exactly two booleans
(`crates/i2pr-proto/src/common/version.rs:336`):

```rust
pub constrains_tunnel_peers: bool,
pub constrains_dsm_targets: bool,
```

There is no third boolean, because the specification's table has no third duty.
The table's rows are *minimum peers will build tunnels through* and *minimum
floodfill peers will send DSM to*. A `DatabaseLookup` is neither: it is not a
tunnel build and it is not a DSM.

The derived floors carry the same shape
(`crates/i2pr-proto/src/common/version.rs:419`):

```rust
pub struct VersionFloors {
    pub min_peer_for_tunnel_build: Option<ApiVersion>,
    pub min_floodfill_for_dsm: Option<ApiVersion>,
}
```

There is no `min_floodfill_for_lookup` field, and
`floors_in_force` (`crates/i2pr-proto/src/common/version.rs:442`) has no branch
that could produce one.

### 3.2 The tree's vocabulary has exactly two gates

`i2pr_netdb::VersionGate` — the enum the daemon's receipts map onto — has two
variants, `TunnelBuildPeer` and `DsmTarget`, and
`VersionDecisionKind::selection_gate()`
(`crates/i2pr-daemon/src/version_eligibility.rs:62`) maps one-to-one onto them.
A third kind would require a third gate, and a third gate would require a
third specification duty.

### 3.3 The lookup path is ungated, deliberately and visibly

The lookup's candidate set is produced by the **ungated** selector:

- `crates/i2pr-netdb/src/lookup_policy.rs:258` — `select_floodfill_candidates`
  is the nearest-N selection by XOR distance with the routing key. It filters
  validity and the floodfill capability and applies the caller's exclusions. It
  reads no API version.
- `crates/i2pr-netdb/src/lookup_policy.rs:381` —
  `select_version_gated_floodfills` is the gated selector. It returns the
  eligible targets and the counted refusals.
- `crates/i2pr-netdb/src/lookup_engine.rs:307`, `:402` and `:602` — the three
  places the lookup state machine selects candidates. All three call the
  **ungated** selector.
- `crates/i2pr-daemon/src/destination_tunnels.rs:1164` — the daemon's own
  pre-check on the same store, with the same routing key, the same target
  exclusion and the same policy ceiling. Also the **ungated** selector.

The gated selector is used in exactly two places, both DSM paths:
`crates/i2pr-daemon/src/destination_tunnels.rs:1004` (the service LeaseSet2
publication) and `crates/i2pr-daemon/src/destination_tunnels.rs:1021` (the
creator RouterInfo publication).

### 3.4 The answer is validated by the router, not by the responder's word

The lookup's answer is a `DatabaseStore`. It reaches
`crates/i2pr-daemon/src/destination_tunnels.rs:1429`, which calls
`ingest_lease_set2_response` (`crates/i2pr-daemon/src/netdb_seam.rs:494`)
against the `LeaseSet2Store`. The record is validated before it is trusted, so
a peer that publishes a low API version cannot supply the router with data the
router did not check. The same holds for the encrypted-service path
(`crates/i2pr-daemon/src/service_product.rs:8612`).

### 3.5 An unhelpful candidate is skipped, not re-asked

The lookup state machine excludes already-queried peers and **re-selects** from
the store on each attempt
(`crates/i2pr-netdb/src/lookup_engine.rs:602`), so a candidate that does not
answer costs one round trip and the lookup moves to the next. The rotation is
bounded by the policy's candidate ceiling and the peer budget, and it
terminates: exhaustion yields a terminal result, never a spin
(`crates/i2pr-daemon/src/destination_tunnels.rs:1223`).

### 3.6 The two reference implementations do not agree on a version rule for
floodfills, and neither applies one to a lookup

`specs/QUALIFICATION-GATES.md:161` records i2pd's rule: a RouterInfo that
advertises the floodfill capability below `router.version` **0.9.62** is never
filed as a floodfill (`IsEligibleFloodfill`, `NETDB_MIN_FLOODFILL_VERSION`).

`specs/QUALIFICATION-GATES.md:204` records Java's rule, and it is a **different**
rule: Java's `0.9.62` is a **peer-exclusion** floor (`shouldExclude` — whether
the peer will select *us*), and Java's floodfill-profile predicate
(`shouldCreate`) requires the `f` capability with **no version floor at all**.

So the references disagree about who may be a floodfill by version, and
**neither** applies an API-version floor to which floodfill a lookup may ask.
Both publish `router.version` **0.9.70**
(`plans/closure/network-participation/491-status.md:721`).

## 4. Invariants that must not regress

1. The router's own traffic is never claimed as transit work, and every answer
   the router trusts is validated by the router.
2. A lookup never asks a peer the router has no authority to ask, and never
   asks a peer twice for the same lookup.
3. The API-version floors remain **derived** from the specification's table.
   No floor is hand-maintained, and no floor is added for the lookup path.
4. `VersionGate` and `VersionDecisionKind` keep exactly two variants, and the
   receipts keep carrying counts and stable reason labels only — no peer hash,
   no address, no key, no version string, no payload.
5. The declared `router.version` stays `0.9.62`; `caps` and the published
   RouterInfo are unchanged; the advertised count stays zero.
6. No new configuration key, no new dependency, no new task, no new timer, no
   new listener, no new socket, and no production wire change.

## 5. Scope

### In scope

- The decision itself, stated in the tree's own documentation.
- A **feasibility artefact** that proves the decision's load-bearing claims
  executably, against the real lookup path.
- A **static guard** that enforces the decision and its premise.
- The support inventory, the conformance notes, the daemon deep-dive, the
  registry row and the roadmap row.

### Explicitly out of scope

- **Any change to the lookup's selection.** The decision is that the selection
  does not change. Plan 509 adds no gate and removes no candidate.
- **Any change to the store's notion of a floodfill advertiser.** The two
  reference implementations disagree about that, so the router must not invent
  a rule; the DSM gate already refuses to *send* to a below-floor floodfill
  (Plan 503), and the lookup's answer is validated (Plan 491's boundary).
- **A third `VersionGate` variant, a third receipt kind, or a third derived
  floor.** The decision is that the specification has no third duty.
- **The peer-profile store's read half** (Plan 497 limitation 5), which
  remains bound to Plan 433's blocked composition.
- **Plan 433's `NetDbTunnelCoordinator`**, which stays `blocked`.
- **Any stock-peer claim.** Plan 509 proves the decision over the tree's own
  deterministic surfaces and claims **no** interop evidence.

## 6. Production changes

**None.** Plan 509 changes no production behaviour.

This is deliberate and is the point of the plan: the finding is closed by a
**decision** that the current behaviour is correct, not by a change. The plan
adds evidence and enforcement, and it adds them where they belong — the
documentation, the artefact and the guard.

The plan's own production-change table therefore records **one** production
file as *read but not changed*, and the artefact and the guard as new test
files.

## 7. Design decisions the plan must make explicit

### 7.1 No floor is derivable for a lookup, and a derived floor may not be
invented

The repository's rule is that a floor is **derived** from the specification's
table and the router's declared level, never hand-maintained. The table has no
lookup row, so there is nothing to derive from. A lookup floor would be an
invented protocol requirement, and inventing one is exactly what
`specs/CONFORMANCE.md` forbids: no capability, version, RouterInfo, SAM or
I2CP behaviour beyond the tested subset.

### 7.2 The two existing floors encode peer-side duties that do not transfer

Each floor says that a peer **will not** do something: a below-floor peer will
not build tunnels through the router, and will not send the router's DSM
onward. The router's refusal is therefore exactly *do not ask a peer for
something it will not do*.

The specification imposes no such duty for a lookup. A below-floor floodfill is
not thereby incapable of answering a lookup, so there is no peer-side
behaviour to refuse against. Refusing would encode a rule that does not exist.

### 7.3 Validity, not version, is what the router's answer depends on

The answer is a `DatabaseStore` the router validates before it trusts it. The
consequence is that the responder's API version is **irrelevant** to the
router's safety: a below-floor peer that answers correctly is answered to; a
below-floor peer that answers incorrectly is refused by the validation. The
version gate would therefore buy **no** safety at all.

### 7.4 The rotation already absorbs an unhelpful candidate

A candidate that cannot answer costs one round trip; the state machine
excludes it and re-selects. The rotation is bounded and terminates. The cost
of not gating is therefore one wasted round trip in the reachable case, not a
lost lookup.

### 7.5 The references disagree, so the router must not pick a side

i2pd refuses to *file* a below-floor floodfill; Java applies its `0.9.62` to
peer exclusion and imposes no version floor on filing a floodfill at all.
Neither gates a lookup. The router cannot derive a consensus rule from two
implementations that disagree, so the router keeps its own rule — capability
plus validity — and the **validation** is the control.

### 7.6 The decision is recorded where it cannot be misread

The decision lives in the daemon deep-dive (which is where the lookup path is
described), in `specs/CONFORMANCE.md` (which is where the claim boundaries are
stated), and in the support inventory's notes (which is where a reader looks
before believing a capability). It is **not** copied into `README.md`,
`AGENTS.md` or any skill: the registry is the live index and duplicated plan
state goes stale silently.

## 8. Work packages

### WP1 — the decision, on the record

The daemon deep-dive gains a section for the lookup path's API-version
posture, stating the decision and each of §7's six reasons with a pointer to
the tree. `specs/CONFORMANCE.md` gains the matching claim boundary, and
`specs/support.toml` gains one `experimental`, `advertised = false` surface.

### WP2 — the feasibility artefact

A new test file that drives the **real** lookup path and proves:

1. **A below-floor floodfill that answers correctly completes the lookup.** The
   router accepts a valid answer from a peer that publishes a version below the
   floor — the version is irrelevant because validity is what matters.
2. **A below-floor floodfill that answers incorrectly is refused by the
   validation**, not by the version — the refusal is the router's own answer
   check, and the reason says so.
3. **A silent below-floor floodfill is skipped, not re-asked** — the rotation
   moves to the next candidate and the lookup completes.

Each claim is asserted against the real path, and each is given an **inverted
probe** so a claim that cannot fail is a defect.

### WP3 — the static guard

A new guard file asserting:

1. **The lookup path is ungated** — the daemon's lookup selection and the
   state machine's three selections call the ungated selector, and the gated
   selector appears only on the two DSM paths.
2. **The premise holds** — `MinimumPeerObligation` carries no lookup field,
   `VersionFloors` carries no lookup floor, and `VersionGate` has exactly two
   variants.
3. **The answer is validated** — the lookup's answer path goes through the
   lease-set2 validation, not through a version check.
4. **The rotation skips** — the lookup state machine's candidate selection
   applies the excluded set.

The guard is fail-closed (an absent function is a violation), strips comments,
excludes `#[cfg(test)]` blocks, and carries an inverted probe per rule.

### WP4 — the documentation and the inventory

The support inventory's surface, the conformance section, the deep-dive's
section, and the registry and roadmap rows.

### WP5 — the floor and the closure record

The routine floor on the closing tree, the closure record, and the unblock
audit that records Plan 503's finding as **discharged**.

## 9. Failure, cancellation, restart, and contention semantics

Plan 509 changes no production behaviour, so it adds no failure mode. The
artefact and the guard are test-only and have no runtime effect.

- **The artefact's controls fail closed.** A control that breaks the *build*
  proves less than one that breaks the *assertion*, and the detector must
  count both.
- **The guard is fail-closed.** A rule whose named function is absent is a
  violation, never a passing rule.
- **The decision's premise is checked, not assumed.** If a future specification
  snapshot adds a lookup duty, the guard's premise rule fires and the decision
  must be re-made on the record.

## 10. Compatibility and migration

- **No wire change.** A `DatabaseLookup` is byte-identical before and after.
- **No peer-observable change.** The router asks the same floodfills, in the
  same order, with the same messages.
- **No configuration change.** No key is added, removed, or reinterpreted.
- **No data migration.** Nothing is persisted; the artefact's state is
  in-memory and test-only.
- **No dependency change.** `Cargo.toml` and `Cargo.lock` stay byte-identical.

## 11. Required tests

| Test | File | Asserts |
| --- | --- | --- |
| A below-floor floodfill's correct answer completes the lookup | `crates/i2pr-daemon/tests/lookup_target_version_decision.rs` | the version is irrelevant to a valid answer |
| An incorrect answer is refused by validation | the same | the refusal is the validation's, not a version check |
| A silent candidate is skipped and the lookup completes | the same | the rotation moves on |
| The lookup path is ungated | `crates/i2pr-daemon/tests/lookup_target_version_boundary.rs` | the decision holds |
| The premise holds | the same | no lookup row, no lookup floor, two gates |
| The answer is validated | the same | the answer path validates |
| The rotation skips | the same | the excluded set is applied |
| The M6 destination lane is unchanged | `cargo test --locked -p i2pr-daemon --test destination_tunnel_unit` | the lookup path still works |
| The I2PControl tunnels lane is unchanged | `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels` | the service product is unchanged |

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test lookup_target_version_decision -- --test-threads=1
cargo test --locked -p i2pr-daemon --test lookup_target_version_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Plus the repository's routine floor, in full, on the closing tree.

## 13. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the lookup path's API-version posture
  section, and the module-table row for any file the plan touches.
- `specs/CONFORMANCE.md` — the Plan 509 section, stating that the lookup target
  is not version-gated, why, and that no stock-peer evidence exists.
- `specs/support.toml` — one `experimental` surface,
  `netdb.lookup-target-version-posture`, with `advertised = false`.
- `plans/registry.md` — the Plan 509 row.
- `plans/subsystems/network-participation-roadmap.md` — the Plan 509 row and
  §12.

## 14. Acceptance criteria and stop conditions

### 14.1 Acceptance

1. The decision is stated in the tree's documentation with all six reasons
   traced to the tree.
2. The artefact proves all three claims, and each claim has an inverted probe.
3. The guard holds on the real tree, and each of its rules has an inverted
   probe.
4. The controls are applied to the **real** tree, run against the **real**
   lanes, and reverted byte-for-byte, with the failing-test counts recorded.
5. The routine floor passes in full on the closing tree.
6. Nothing is advertised: `router.version` stays `0.9.62`, `caps` and the
   published RouterInfo are unchanged, the advertised count stays zero, and
   `Cargo.toml` / `Cargo.lock` stay byte-identical.
7. No production behaviour changed, and the diff proves it.

### 14.2 Stop conditions

- **The artefact reveals that the lookup's answer is NOT validated.** Then the
  decision's premise is false, the decision must not be made, and the plan
  stops with the defect recorded for a corrective plan.
- **The artefact reveals that the rotation does not rotate.** The same.
- **The guard's premise rule fires on the real tree.** The specification's table
  has changed, and the decision must be re-made.
- **Any change to the lookup's selection is proposed.** It is out of scope; it
  needs its own plan-of-record.
- **A stock-peer claim is proposed.** Plan 509 claims none.

### 14.3 What this plan does **not** close

- Plan 491 **F2** (no stock-peer evidence) — needs Plan 493's authorized
  two-host topology.
- Plan 491 **F4** (the intermittent `i2pcontrol_tunnels` staging failure) —
  belongs to the I2PControl / service-tunnel line.
- Plan 487 **F3**'s destination-inbound half — needs tunnel rekeying, which the
  tree does not implement.
- Plan 497 limitation 5's **read** half — bound to Plan 433's blocked
  composition.
- Plan 490 Medium #3 — external.
- Plan 505/506/507/508's stock-peer questions — external.

## 15. Closure evidence required

1. The commits, in order.
2. A requirement-to-evidence matrix.
3. The commands run, with their outcomes, labelled local or CI truthfully.
4. The negative controls, with the mutation, the failing-test count, and the
   byte-for-byte restoration.
5. The invariant, failure, migration and security reviews.
6. The limitations, the findings by severity, the roadmap disposition, and the
   unblock audit recording Plan 503's finding as discharged.

## 16. Handoff notes

- **The plan changes no production behaviour, and that is the outcome, not a
  shortfall.** The finding was a question ("should the lookup be gated?"), and
  the plan answers it with a sourced decision, an executable proof and an
  enforcing guard.
- **The decision's premise is checked by the guard, not assumed.** If the
  specification's table ever gains a lookup duty, the guard fires.
- **Plan 503's closure record is not rewritten.** The finding is discharged in
  this plan's closure record and in the registry row.
- **The next candidate on this line** is whatever the unblock audit names; the
  source-side candidates that remain are Plan 491's F4 (the I2PControl line),
  Plan 487 F3's destination-inbound half (a new I2NP mechanism), and Plan 497
  limitation 5's read half (bound to Plan 433).
