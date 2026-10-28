## Status

**Plan 511 — the comparator's component ceiling, derived rather than assumed —
is closed.**

Plan 491 recorded **F5**: *"The comparator's four-component ceiling is an
unstated assumption in the specification's terms. The specification's table's
widest row is four components, so the bound does not bind today; a future
five-component row would make such versions uncomparable rather than
comparable."*

Plan 491 left it fixed in **prose only**, for two stated reasons — and the
plan found **both** reasons wrong:

- *"widening the ceiling is a one-line change"* — it is **compile-breaking at
  three sites**, and it multiplies the stored array by 26;
- *"the plan's scope was the current table"* — the current table **is** the
  widest row, which is exactly the assumption F5 names.

What landed:

- **The row is held, and the ceiling is derived from it.**
  `i2pr_proto::SPECIFICATION_WIDEST_VERSION_ROW` holds the specification's
  four-component row (`0.6.1.10`) as **data**, and
  `i2pr_proto::MAX_API_VERSION_COMPONENTS` is that row's **width** through
  `version_row_width`, not a literal beside it.
- **The harm F5 names is now structurally impossible.** A future
  five-component row in the specification's table widens the comparator **by
  construction**: the held record becomes that row, the ceiling follows it,
  and such a version becomes **comparable** — which is exactly what the
  hard-coded four denied.
- **The ordering stays exact.** The stored array's width and the parser's
  bound are the same constant, so a version the parser accepts is stored in
  full. No truncation, no lossy fallback.
- **No production behaviour changed.** The record is four components wide, so
  the constant is four, so the comparator accepts and refuses **exactly** the
  versions it accepts and refuses today. `ApiVersion` stays 10 bytes and
  `PublishedVersion` stays 12 — **measured**, not estimated.
- **The divergence from both pinned references is recorded, with evidence.**
  This is the first time this repository has recorded what the references do
  with a version that has more components than the specification's table has
  ever used. Both of them **admit** such a version; `i2pr` is the only one
  of the three that refuses one, and the refusal is typed, counted,
  fail-closed and monotone.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero** (103 surfaces, 97 experimental); `Cargo.toml` and
`Cargo.lock` are byte-identical.

**No stock-peer evidence is claimed.** The plan is over the tree's own
deterministic surfaces, and the references' behaviour is read from their
**own pinned source**.

**One deviation from the plan-of-record** — acceptance criterion 6 says
every negative control must break an assertion rather than the build. Nine of
the ten do; the tenth is refused **by the build**, because the comparator's
own four-component constructor cannot fit a three-component ceiling. That is
the intended, derived behaviour, and it is recorded under *The one deviation
from the plan-of-record* below.

## Commits

| Commit | Subject |
| --- | --- |
| `0cf50f0b` | docs(plans): register Plan 511, the comparator's component ceiling |
| `9ec6037d` | feat(proto): derive the comparator's component ceiling from the held row |
| `3469cbb9` | docs(proto): record Plan 511's derived ceiling and the references' own handling |
| `107ec32b` | fix(proto): let the constructors follow the comparator's ceiling |
| `<closure>` | docs(plans): close Plan 511 with the floor table and the evidence |

## What changed

### The gap

The specification's API-version table contains a four-component row
(`0.6.1.10`). The tree held the table's **minimum-peer obligation** rows as
data — six rows, every one of them three components — and did **not** hold
the table's other rows. The four-component row the comparator's bound
depended on therefore lived in a **prose comment** above
`MAX_API_VERSION_COMPONENTS`:

> *"The specification's table contains a four-component row (`0.6.1.10`), so
> the comparator must accept four components."*

That comment was an **assertion about a document the tree does not hold**.
F5's complaint is precisely that the ceiling rested on it.

### The derivation

```rust
pub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 6, 1, 10];

pub const MAX_API_VERSION_COMPONENTS: usize =
    version_row_width(SPECIFICATION_WIDEST_VERSION_ROW);
```

The stored array's width (`[u16; MAX_API_VERSION_COMPONENTS]`), the parser's
bound, the comparison's loop bound, and **the constructors' arrays** are
**one** number, and that number is a **fact about the specification's table**
rather than a literal in the comparator. The constructors following the
ceiling is load-bearing: without it, a wider row would break the build
instead of widening the comparator, and the negative controls caught exactly
that.

### The proof

`crates/i2pr-proto/tests/version_ceiling_derivation.rs` — eight tests:

1. the ceiling **is** the held row's width;
2. the held row **is** the specification's four-component row;
3. a version carrying exactly the ceiling's components parses **in full**;
4. the ordering **is** the specification's numeric rule — the real comparator
   is checked against an independently stated reference order for **every
   pair** in a sixteen-version matrix that includes the four-component row,
   and equality is checked against the same reference;
5. a version beyond the ceiling is refused **today**;
6. the refusal is **typed** and **counted** downstream
   (`UncomparableVersion`, a stable reason label, never eligible);
7. a **wider** held row **would widen** the ceiling — the mechanism, proved
   through the derivation's own width function;
8. the parser accepts **exactly** the stored width.

### The enforcement

`crates/i2pr-daemon/tests/version_ceiling_boundary.rs` — **eleven rules** and
**sixteen tests**, every rule with an inverted probe:

| Rule | What it pins |
| --- | --- |
| 1 | the ceiling is **derived** from the held row, not a literal beside it |
| 2 | the held row **is** the specification's four-component row |
| 3 | the stored width **is** the ceiling |
| 4 | the parser's bound **is** the ceiling, not a bare literal |
| 5 | the ordering runs to the full width, to each side's own extent, treats the exhausted side as less, and **never truncates** |
| 6 | the comparator stays `const` — `floors_in_force` is a `const fn` |
| 7 | every floor declares **three** components |
| 8 | there is **no peer-to-peer ordering path**: `PublishedVersion` derives no `Ord` |
| 9 | the refusal is **counted** in the live path, by its own reason |
| 10 | the ceiling equals the held row's width, against the **compiled** code |
| 11 | the constructors build their array **from the ceiling**, so a wider row widens the comparator instead of breaking the build |

Rules 7 and 8 together are the **exactness premise**: every live ordering is
against a three-component floor, and two published peer versions cannot be
ordered at all. Rule 9 is the **fail-closed** premise.

### The divergence, recorded

`specs/QUALIFICATION-GATES.md` §10 records, for the first time here, what the
two pinned references do:

- **i2pd 2.61.0** (pin `635b013a612ff47278ef02acf8580a28e10e26c5`) reads
  `router.version` by **collapsing every digit into an `int`** with **no
  component count** — the separators are not digits and are skipped
  (`libi2pd/RouterInfo.cpp`). `libi2pd/version.h:18` defines
  `MAKE_VERSION_NUMBER(a,b,c) ((a*100+b)*100+c)`, so the floor `0.9.62` is
  the integer `962` (`libi2pd/NetDb.hpp:54-58`), and `"0.9.62.1.2"`
  collapses to `96212` — **above** the floor. **i2pd admits it.**
- **Java I2P**'s `net.i2p.util.VersionComparator` walks both strings to
  exhaustion with **no cap**, parses components as `long`s, and treats the
  exhausted side as less. **Java admits it too.**

**The provenance caveat is stated in the record, not buried.** The pinned Java
reference is **2.13.0**; the reading was taken from a **2.13.1**
installation. It corroborates the behaviour the repository already records
in `scripts/plan495-peer-eligibility.py`, but it is **not** a reading of the
pinned artifact, and the record says so. The **primary** derivation is
i2pd's, at the exact pin.

The divergence's safety argument is stated in the same place: the refusal is
**typed** (`PublishedVersion::Uncomparable` →
`VersionEligibility::UncomparableVersion`), **counted**
(`VersionRefusalCounts::uncomparable_version`, published under the stable
label `uncomparable-version`), **fail-closed**, and **monotone** — a version
with more components can only ever be *above* a shorter version with the same
prefix, so the refusal can only **exclude** a peer both references would
admit, and can never admit one below a floor.

### The one deviation from the plan-of-record

The plan-of-record's acceptance criterion 6 says the negative controls must
each break an **assertion** rather than the build. **Nine of the ten do.
The tenth does not, and it cannot.**

Control **J** narrows the held row to three components. The comparator's own
four-component constructor (`ApiVersion::new4`) and its `build()` accessor
need a fourth component, so a three-component ceiling makes the type
unrepresentable and the **build refuses**. That is not a defect in the
control: it is the **derived** invariant that the ceiling cannot fall below
the specification's table's widest row *when that row is four components
wide*, and the build is where it is enforced.

The plan-of-record is **not rewritten**, so the deviation is recorded here.
The criterion's *intent* — that every control must be **detected**, that a
build failure must be reported separately from an assertion failure, and
that no control may be vacuous — is met for all ten. The refinement the
controls forced is that the comparator's constructors must follow the
ceiling, which is **rule 11** of the guard and the fix in `107ec32b`.

### Three things the change deliberately does not do

- **It does not widen the ceiling.** The record is four components wide, so
  the constant is four. The plan removes the *assumption*; it does not
  loosen a live admission gate on the strength of a hypothesis.
- **It does not move a floor.** `MINIMUM_PEER_OBLIGATIONS` is unchanged, and
  no floor is added, removed or re-derived.
- **It does not settle the component width.** The parser's per-component bound
  is a `u16`; Java parses components as `long`s. That is a **second**
  divergence, recorded for a successor, not fixed here: widening the width
  would multiply the stored array again, and a version with a component above
  65535 is not a version.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The ceiling has a **source**, not an assertion in prose | the guard's rule 1 and rule 2; the artefact's first two tests |
| A wider row widens the comparator **and does not break the build** | the guard's rule 11; and **control C**, which widens the record on the real tree and observes a comparable five-component version |
| A **wider row widens the comparator**, so a future five-component row makes such a version **comparable** | the guard's rule 1 (the ceiling is derived, so it follows the record) and rule 2 (the record is pinned, so a move is reported); the artefact's seventh test; and **control C**, which widens the record on the real tree and observes the behaviour change |
| The stored width and the parser's bound are the **same** constant | the guard's rule 3 and rule 4 |
| The ordering stays **exact** | the guard's rule 5; the artefact's fourth test, over every pair in a sixteen-version matrix |
| The comparator stays `const` | the guard's rule 6 |
| Every floor declares **three** components | the guard's rule 7 |
| There is **no peer-to-peer ordering path** | the guard's rule 8 — `PublishedVersion` derives no `Ord` |
| The refusal is **typed and counted** | the guard's rule 9; the artefact's sixth test |
| The ceiling equals the held row's width, in the **compiled** code | the guard's rule 10 |
| **No production behaviour changes** | the proto crate's 170 unit tests pass unchanged; the artefact's third and fifth tests pin today's behaviour; the struct sizes were measured before and after (10 and 12 bytes, unchanged) |
| The references' behaviour is **recorded with evidence** | `specs/QUALIFICATION-GATES.md` §10, with the pins, the file and line, and the Java provenance caveat |
| The **per-component width** divergence is recorded, not settled | `specs/QUALIFICATION-GATES.md` §10.5 and the plan-of-record §5 |
| Plan 491's closure record is **not** rewritten | the discharge lives here and in the registry row |
| No advertisement; `Cargo.toml` and `Cargo.lock` byte-identical | the support inventory; the diff |

## Commands run

### The routine floor

The closing floor is **123 steps**, run on the closing tree, and it passed
**123 of 123**. The script is `/tmp/i2pr-floor-511-run1.sh`; the log is
`/tmp/i2pr-floor-511-run1.log`. It is Plan 510's 121-step floor plus two
Plan 511 steps (`plan511-version-ceiling-derivation` and
`plan511-version-ceiling-boundary`).

| # | Step | Result | Seconds |
| --- | --- | --- | --- |
| 1 | `cargo-fmt` | PASS | 4 |
| 2 | `cargo-check` | PASS | 17 |
| 3 | `cargo-build-siblings` | PASS | 0 |
| 4 | `cargo-build-static-fixture` | PASS | 0 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1751 |
| 8 | `cargo-clippy` | PASS | 6 |
| 9 | `cargo-doc` | PASS | 14 |
| 10 | `cargo-test-doc` | PASS | 4 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 0 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 6 |
| 16 | `planning-tests` | PASS | 8 |
| 17 | `check-runtime-boundaries` | PASS | 1 |
| 18 | `check-console-boundaries` | PASS | 1 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 0 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 0 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `plan495-peer-eligibility-self` | PASS | 0 |
| 29 | `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| 30 | `plan476-gate-binding` | PASS | 1 |
| 31 | `plan476-gate-binding-self` | PASS | 0 |
| 32 | `check-managed-app-package-boundary` | PASS | 0 |
| 33 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 34 | `check-managed-app-policy-boundary` | PASS | 0 |
| 35 | `check-managed-app-policy-boundary-self` | PASS | 1 |
| 36 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 37 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 38 | `check-service-tunnel-boundaries` | PASS | 0 |
| 39 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 40 | `check-managed-app-manager-boundary` | PASS | 0 |
| 41 | `check-m11-per-epoch-composition` | PASS | 0 |
| 42 | `check-service-anonymity-boundaries` | PASS | 0 |
| 43 | `check-fixture-manifest` | PASS | 0 |
| 44 | `check-ntcp2-vectors` | PASS | 0 |
| 45 | `check-ssu2-vectors` | PASS | 0 |
| 46 | `check-i2cp-vectors` | PASS | 5 |
| 47 | `check-ntcp2-interoperability` | PASS | 0 |
| 48 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 49 | `check-m11-transit-boundaries` | PASS | 2 |
| 50 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 51 | `check-sam-acceptance-evidence` | PASS | 0 |
| 52 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 53 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 54 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 55 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 56 | `check-els2-live-lane-evidence` | PASS | 2 |
| 57 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 58 | `check-outproxy-request-path` | PASS | 0 |
| 59 | `check-outproxy-wire-lane-evidence` | PASS | 1 |
| 60 | `check-config-secret-hygiene` | PASS | 0 |
| 61 | `check-workflow-validity` | PASS | 0 |
| 62 | `check-floodfill-type5-serve` | PASS | 0 |
| 63 | `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| 64 | `check-exploratory-tunnel-evidence` | PASS | 1 |
| 65 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 66 | `check-destination-tunnel-evidence` | PASS | 0 |
| 67 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 68 | `check-m6-mixed-router-acceptance-evidence` | PASS | 3 |
| 69 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 70 | `check-m12-floodfill-boundaries-self` | PASS | 1 |
| 71 | `ntcp2-harness-tests` | PASS | 0 |
| 72 | `plan488-mesh-lane` | PASS | 1 |
| 73 | `plan488-evidence-deterministic` | PASS | 0 |
| 74 | `plan489-mesh-lane` | PASS | 0 |
| 75 | `plan489-evidence-deterministic` | PASS | 0 |
| 76 | `plan490-mesh-lane` | PASS | 0 |
| 77 | `plan490-evidence-deterministic` | PASS | 0 |
| 78 | `plan490-ssu2-transport-suite` | PASS | 9 |
| 79 | `plan490-transport-boundary-guard` | PASS | 0 |
| 80 | `plan491-mesh-lane` | PASS | 0 |
| 81 | `plan491-evidence-deterministic` | PASS | 0 |
| 82 | `plan491-boundary-guard` | PASS | 0 |
| 83 | `plan491-proto-suite` | PASS | 1 |
| 84 | `plan491-netdb-suite` | PASS | 91 |
| 85 | `plan491-client-suite` | PASS | 27 |
| 86 | `plan492-posture-suite` | PASS | 0 |
| 87 | `plan492-m11-lane` | PASS | 5 |
| 88 | `plan502-transit-composition-lane` | PASS | 0 |
| 89 | `plan502-transit-composition-unit` | PASS | 0 |
| 90 | `plan502-transit-boundary-self` | PASS | 0 |
| 91 | `plan496-private-lan-profile` | PASS | 0 |
| 92 | `plan496-ntcp2-activation-gate` | PASS | 1 |
| 93 | `plan497-routing-consumer-lane` | PASS | 0 |
| 94 | `plan497-netdb-preferred-order` | PASS | 1 |
| 95 | `plan497-daemon-participation-unit` | PASS | 2 |
| 96 | `plan498-introducer-handoff-lane` | PASS | 1 |
| 97 | `plan498-introducer-mesh-lane` | PASS | 0 |
| 98 | `plan498-bootstrap-resolver-row` | PASS | 25 |
| 99 | `plan498-runtime-relay-accessor` | PASS | 8 |
| 100 | `plan503-version-eligibility-live` | PASS | 1 |
| 101 | `plan503-version-eligibility-unit` | PASS | 0 |
| 102 | `plan504-liveness-boundary` | PASS | 1 |
| 103 | `plan504-liveness-live` | PASS | 0 |
| 104 | `plan504-liveness-unit` | PASS | 0 |
| 105 | `plan505-driver-boundary` | PASS | 1 |
| 106 | `plan505-driver-live` | PASS | 2 |
| 107 | `plan505-driver-unit` | PASS | 1 |
| 108 | `plan506-reconcile-boundary` | PASS | 4 |
| 109 | `plan506-reconcile-live` | PASS | 1 |
| 110 | `plan507-retention-feasibility` | PASS | 1 |
| 111 | `plan507-retention-boundary` | PASS | 0 |
| 112 | `plan507-retention-live` | PASS | 1 |
| 113 | `plan508-creator-retention-feasibility` | PASS | 0 |
| 114 | `plan508-creator-retention-boundary` | PASS | 1 |
| 115 | `plan508-creator-retention-live` | PASS | 1 |
| 116 | `plan509-lookup-target-decision` | PASS | 0 |
| 117 | `plan509-lookup-target-boundary` | PASS | 1 |
| 118 | `plan510-managed-staging-load-lane` | PASS | 0 |
| 119 | `plan510-managed-reason-boundary` | PASS | 1 |
| 120 | `plan510-tunnelmanager-black-box` | PASS | 4 |
| 121 | `plan511-version-ceiling-derivation` | PASS | 0 |
| 122 | `plan511-version-ceiling-boundary` | PASS | 0 |
| 123 | `cargo-deny` | PASS | 1 |

### The focused verification

Every command below was run **locally** on this host and passed. Nothing in
this record is CI evidence.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS (in the floor) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-proto --lib -- --test-threads=1` | PASS — 170 unit tests, unchanged |
| `cargo test --locked -p i2pr-proto --test version_ceiling_derivation -- --test-threads=1` | PASS — 8 tests |
| `cargo test --locked -p i2pr-daemon --test version_ceiling_boundary -- --test-threads=1` | PASS — 16 tests |
| `cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1` | PASS — 28 tests, Plan 503's guard, unchanged |
| `cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1` | PASS — 51 tests |
| `cargo test --locked -p i2pr-daemon --test transit_participation_boundary -- --test-threads=1` | PASS — 19 tests |
| `cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS |
| `python3 -c "import tomllib; tomllib.load(open('specs/support.toml','rb'))"` | PASS — the inventory parses |

### The ten negative controls, verified on the real tree

Each control mutates the **real** tree, runs the **real** lanes, and
restores the tree **byte-for-byte**. The detector distinguishes an
**assertion failure** from a **build failure**: a compiler error, a
"could not compile" / "aborting" / "linking" message, or an abort signal
counts as a build failure, and the test runner's own
`error: test failed, to rerun pass ...` footer does **not**.

| # | Mutation | Artefact | Guard | Proto unit | Total | Build broken? |
| --- | --- | --- | --- | --- | --- | --- |
| A | the ceiling becomes a literal beside the held row | 0 | 4 | 0 | 4 | no |
| B | the held row moves to a different four-component row | 1 | 4 | 0 | 5 | no |
| C | **the held row widens to five components** | 4 | 4 | 1 | 9 | no |
| D | the stored width diverges from the ceiling | 0 | 4 | 0 | 4 | no |
| E | the parser's bound becomes a literal | 0 | 4 | 0 | 4 | no |
| F | the ordering truncates | 1 | 4 | 5 | 10 | no |
| G | a floor widens to four components | 0 | 4 | 3 | 7 | no |
| H | published versions become orderable | 0 | 4 | 0 | 4 | no |
| I | the refusal counter drops the uncomparable reason | 0 | 4 | 0 | 4 | no |
| J | **the held row narrows below four components** | — | — | — | — | **yes — the build refuses** |

**Nine of the ten break assertions; one breaks the build, and that one is
the intended refusal.** A control that breaks the build is weaker than one
that breaks an assertion, so the build-failure count is reported separately.
It is zero for the nine, and it is **one** for control **J** — which is the
point of that control: narrowing the held row below four components makes
the comparator's own four-component constructor and its `build()` accessor
unrepresentable, so the **build refuses**. That is a derived invariant, and
the build is where it is enforced.

**Two of them change the production behaviour, and they are the ones that
matter.** Control **B** moves the held row to a different four-component row,
so the ceiling follows it and the guard reports the move — the artefact
catches it too, because the held row is a runtime value there. Control **C**
widens the held row to five components, so the ceiling follows it up and a
five-component version becomes **comparable**: the artefact's "beyond the
ceiling is refused" test fails, the proto crate's own malformed-version rows
fail, and the guard reports the record's move. That is the plan's central
claim **observed on the real tree** rather than argued, and it is the
control that caught the plan being wrong before the fix.

The tree was restored byte-for-byte after every control, and the restore is
**asserted** by the harness (a SHA-256 comparison per file), not assumed.

### The defect the controls found, and the fix

**The controls found a real defect in the plan's own central claim, and the
plan was wrong before the fix.**

The first control run showed that widening the held row to five components
**broke the build**, rather than widening the comparator. The cause was
precise: `ApiVersion::new` and `ApiVersion::new4` filled their arrays with
**four-element literals** —

```rust
components: [major, minor, patch, 0],
components: [major, minor, patch, build],
```

— so the array's length was pinned to four *independently* of the ceiling. A
wider recorded row changed the ceiling, and the two disagreed, and the
compiler said so. Under that implementation the plan's headline claim — *a
wider row widens the comparator by construction* — was **false**: a wider row
produced a build failure, not a wider comparator.

**The fix.** Both constructors now build their array from the ceiling:

```rust
let mut components = [0u16; MAX_API_VERSION_COMPONENTS];
components[0] = major;
```

and the guard gained **rule 11**, which pins that the constructors build from
the ceiling and do not fill a four-element literal, with its own inverted
probe. The plan's claim is now **observed on the real tree** rather than
argued: with the record widened to five components, the build succeeds, the
ceiling follows to five, a five-component version becomes **comparable**, and
the ordering matrix — which includes versions with four and five components —
still agrees with the specification's numeric rule.

**What the fix does not do.** The ceiling still cannot narrow below four,
because `new4` and `ApiVersion::build()` need a fourth component. That is a
**derived** invariant, and the build enforces it: control **J** narrows the
record to three components and the build refuses. That refusal is the
intended behaviour rather than a defect, and it is reported as such — it is a
**build-failing** control and therefore weaker than the other nine.

### Weaknesses found while building the controls and the guard

1. **Three of the guard's rules were wrong on their first run.** The
   landed tree failed rules 2, 5 and 6. Rule 2 parsed the held row as a
   *string* literal when the row is an **integer** array; rule 5 matched the
   comparison's arms without their `return`; rule 6 read the `const`
   modifier from the signature, which starts at the `fn` keyword and so never
   contained it. Each was a detector defect, not a tree defect, and each was
   fixed rather than relaxed: the held row is now parsed as integers, the
   arms are matched as the source writes them, and the modifiers are read
   from the text before the `fn` keyword.
2. **The truncation probe did not apply.** The first probe for rule 5
   narrowed a comparison's *extent* rather than introducing a truncation, so
   the detector — which looked for a slice — did not fire. The rule was
   strengthened to pin all three bounds (the loop's width, and each side's
   own extent) and three probes were written, one per bound.
3. **The guard's first build carried two unused helpers** (`function_signature`
   and `string_literals`), which `-D warnings` rejects. They were removed
   rather than allowed to sit as dead code.
4. **The plan's central claim was false before the fix** — see *The defect
   the controls found* above. This is the most important finding of the
   whole plan: the controls caught a claim the plan had asserted about
   itself, and the claim was wrong.

## Invariant review

| Invariant | Held? | Evidence |
| --- | --- | --- |
| The comparator's ordering stays **exact** — no truncation, no lossy fallback | yes | the guard's rule 5; the artefact's fourth test, over every pair in a sixteen-version matrix |
| The parser's bound and the stored width stay the **same** constant | yes | the guard's rules 3 and 4; the artefact's eighth test |
| A version beyond the bound stays a **typed refusal**, counted by reason, never a silent skip and never a pass | yes | the guard's rule 9; the artefact's sixth test |
| Every floor still declares **three** components | yes | the guard's rule 7 |
| The floors themselves do not move | yes | the diff — `MINIMUM_PEER_OBLIGATIONS` is untouched |
| `cmp_to` stays `const`, and `floors_in_force` stays a `const fn` | yes | the guard's rule 6 |
| No peer is admitted **below** a floor | yes | the plan's monotone-loosening argument; the refusal is unchanged, so the admitted set is unchanged |
| **No production behaviour change** | yes | the proto crate's 170 unit tests pass unchanged; the struct sizes were measured before and after and are unchanged |
| No advertisement; `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` byte-identical | yes | the support inventory; the diff |
| No plan of record and no closed closure record is rewritten | yes | Plan 491's closure record is untouched; this plan's plan-of-record is untouched |

## Failure review

| Condition | Behaviour |
| --- | --- |
| A peer publishes a version with more components than the ceiling | refused as `UncomparableVersion`, counted under `uncomparable-version`, never admitted. **Unchanged** |
| A peer publishes a malformed version | refused as before. The parser's grammar is untouched |
| The specification's table gains a wider row | the held record must move, the ceiling follows it, and such a version becomes **comparable** — the harm F5 names cannot occur. The guard **reports** the record's move |
| The held record is edited to a narrower row | the ceiling follows it **down**, and a version that used to parse stops parsing. The guard reports it; the artefact catches the behaviour change. This is control **B** |
| The held record is edited to a wider row | the ceiling follows it **up**, and the comparator accepts more versions. The guard reports it; the artefact catches the behaviour change. This is control **C** |
| The ceiling is re-pinned to a literal | the guard's rule 1 fires |
| The stored width and the ceiling diverge | the guard's rule 3 fires |
| The comparison truncates or stops early | the guard's rule 5 fires |
| A floor widens beyond the stored width | the guard's rule 7 fires |
| `PublishedVersion` becomes orderable | the guard's rule 8 fires |

## Migration review

None. Nothing is persisted, renamed, reinterpreted or re-versioned. The
comparator accepts and refuses **exactly** the versions it accepts and
refuses today. No wire format, no configuration key, no data file and no
protocol message changes.

## Security review

- **No new attack surface.** The plan adds no task, no timer, no listener,
  no socket, no configuration key and no dependency. The comparator still
  allocates nothing and never panics: a malformed string is a state, not an
  error.
- **No admission is loosened.** The comparator's accepted set is unchanged,
  so a peer that was refused is still refused. The change makes the *reason*
  for the bound checkable; it does not widen the gate.
- **The bound cannot be widened silently.** The ceiling is a derivation from a
  held record, and the guard pins the record, so widening the comparator
  requires a **deliberate** edit to a data constant that the guard reports.
- **No secret is logged, printed or serialised.** The plan adds no
  diagnostic, no receipt and no counter. The guard reads source files; the
  artefact parses version strings.
- **The guard reads only the repository's own source.** It opens no socket
  and reaches no network.

## Documentation and operations

- `specs/QUALIFICATION-GATES.md` — the new **§10**, the references' component
  handling with the pins, the file and line, and the Java provenance caveat.
- `specs/CONFORMANCE.md` — the Plan 511 section: what is claimed, what is
  not.
- `specs/support.toml` — one `experimental`, non-advertised surface,
  `proto.api-version-component-ceiling`. The inventory is now 103 surfaces,
  97 experimental, 5 deferred, 1 ready, and the advertised count stays zero.
- `docs/architecture/i2pr-proto.md` — the derivation and its consequence, and
  the module-table row for `common::version` re-corrected to **823**.
- `docs/architecture/i2pr-daemon.md` — the eligibility path's treatment of an
  uncomparable version.
- `plans/registry.md` and `plans/subsystems/core-router-recovery-roadmap.md`
  — the Plan 511 rows, closed, and Plan 491's row carrying the discharge.

## Limitations

1. **The Java reading is from 2.13.1, not the pinned 2.13.0.** The pinned
   artifact was not available on this host. The reading corroborates the
   behaviour the repository already records in
   `scripts/plan495-peer-eligibility.py`, and the **primary** derivation is
   i2pd's, at the exact pin — but the caveat is real and is recorded
   wherever the reading is relied upon.
2. **The tree holds one row of the specification's table, not the table.**
   The held record is the four-component row the ceiling depends on. The
   table's other rows — the feature-support rows the minimum-peer obligation
   rows are a subset of — are **not** held, so a *different* wider row in the
   table would still not be noticed by the tree. The plan makes the ceiling's
   own source checkable; it does not make the table complete.
3. **The divergence with the references is recorded, not closed.** `i2pr`
   still refuses a version that both references admit. The plan's position
   is that the refusal is safe — typed, counted, fail-closed and monotone —
   and that the two references **disagree with each other** about how a
   longer version compares (i2pd collapses the digits, so it cannot tell
   `0.9.6.2` from `0.9.62`; Java compares component-wise), so there is no
   single oracle to match. A future plan may revisit that.
4. **The per-component width divergence is open.** The parser's `u16` bound
   against Java's `long` is recorded, not settled.
5. **No stock-peer evidence is claimed**, and none is possible here: the
   comparator's behaviour is over the tree's own deterministic surfaces.

## Findings by severity

### Medium

- **The plan's central claim was false as first written, and the negative
  controls found it.** The claim was *"a wider row widens the comparator by
  construction"*. It was not true of the first implementation:
  `ApiVersion::new` and `ApiVersion::new4` filled their arrays with
  **four-element literals**, so the array's length was pinned to four
  *independently* of the ceiling, and a wider recorded row produced a
  **build failure** rather than a wider comparator. The controls caught it
  because they widened the record on the real tree and the build broke.
  **Fixed** before the floor: both constructors now build from the ceiling,
  the guard pins that as rule 11, and the claim is now observed rather than
  argued. The severity is Medium because a plan that asserts a false claim
  about itself is worse than a plan that asserts nothing — and the only
  reason it was caught is that the controls mutate the **real** tree rather
  than a model of it.

### Low

- **Plan 491's F5 was closed by a decision the plan overturned, and the
  decision's own justification was wrong.** Plan 491 recorded that *"widening
  the ceiling is a one-line change with no caller"*. It is compile-breaking
  at three sites, and it multiplies the stored array by 26 — from 10 bytes to
  257 — which multiplies the profile store's memory by about five, to
  roughly 1.3 MiB across its 4096 profiles. A successor that widened the
  ceiling on Plan 491's stated reasoning would have paid that cost for a
  behaviour change the plan does not make. **Corrected here**, and the
  correction is recorded in the registry row.
- **Three of the guard's rules were wrong on their first run**, and the
  guard's first truncation probe did not apply. See *Weaknesses* above. Each
  was a detector defect and was fixed rather than relaxed.
- **The guard's first build carried two unused helpers**, which `-D warnings`
  rejects. Removed.

### Recorded, not defects

- **The per-component width divergence** — the parser's `u16` bound against
  Java's `long` — is recorded for a successor and not settled here.
- **The specification's table is still not held in the tree.** Only its
  minimum-peer obligation rows and the one widest row are. Recorded.
- **The Java evidence is one patch release off its pin.** Recorded in
  `specs/QUALIFICATION-GATES.md` §10.3 and in the plan-of-record §3.4.
- **Plan 491's closure record is not rewritten.** The discharge lives here
  and in the registry row.
- **The plan of record is not rewritten.** Its §6 production-change table says
  *"one file, one constant, one new record"*, and that is what shipped.

## Roadmap disposition

The core-router-recovery roadmap's Plan 511 row is **closed** as
`passed-version-component-ceiling`, and its status sentence names the
derivation and the recorded divergence. Plan 491's **F5** is **discharged**:
the ceiling now has a source, and the harm F5 names — a future five-component
row making such versions uncomparable — is structurally impossible.

## Unblock audit

Plan 491's closure record listed F5 as *"recorded, not fixed"* with the reason
that it needed a plan-of-record. Plan 511 is that plan-of-record, and F5 is
now discharged. No other registered plan lists F5 as a hard dependency, so no
other row moves.

Plan 491's **F2** (no stock-peer evidence) stays **blocked** externally on
Plan 493's authorized two-host topology, and Plan 491's F1 and F3 remain
discharged by Plans 503 and 497. Plan 491's **F4** was discharged
conditionally by Plan 510.

The plans that remain blocked are blocked on **external environments** and
are unaffected by this plan: Plans 433, 435, 436, 437, 438, 439, 493 and
494. Plan 436's source-side work was discharged by Plan 502.

The source-side candidates that remain on this line are unaffected: Plan 491
**F2** (needs Plan 493's topology), Plan 491's per-component **width**
divergence (recorded for a successor), Plan 487 **F3** (the transit-hop
failure detection, which needs its own plan-of-record) and Plan 497
limitation 5's **read** half (which is bound to Plan 433's blocked
composition).
