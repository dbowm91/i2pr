# Plan 511 — the comparator's component ceiling, derived rather than assumed

**Subsystem:** core-router-recovery (the API-version and peer-eligibility line)
**Status:** registered
**Predecessor:** Plan 491 (API-version floors and peer eligibility),
`plans/closure/network-participation/491-status.md`, finding **F5**
**Closes:** Plan 491's **F5** — *"The comparator's four-component ceiling is an
unstated assumption in the specification's terms."*

## 1. Objective

Make the version comparator's component ceiling **derived from a fact the tree
holds**, instead of asserted in a prose comment, and **record what the two
pinned references actually do** with a version that has more components than
the specification's table has ever used.

Plan 491 recorded F5 and did not fix it, for two reasons it stated as
settled:

> *"Recorded, not fixed: widening the ceiling is a one-line change with no
> caller, and the plan's scope was the current table."*

Both reasons are wrong, and the first one is wrong in a way that matters:

1. **Widening the ceiling is not a one-line change.** `ApiVersion` holds
   `components: [u16; MAX_API_VERSION_COMPONENTS]`, and `ApiVersion::new` and
   `ApiVersion::new4` both fill the array literally
   (`crates/i2pr-proto/src/common/version.rs:54` and `:62`). Changing the
   constant is **compile-breaking at three sites**, and it multiplies the
   struct by 26 (`ApiVersion` is 10 bytes today; at a 128-component ceiling it
   is 257), which multiplies the profile store's memory by about five.
2. **The plan's scope was not the current table.** The current table is the
   *widest row*, and the widest row is exactly what F5 says is an unstated
   assumption. Fixing F5 means fixing the *assumption*, not the number.

The tree holds the specification's **minimum-peer obligation** rows as data
(`MINIMUM_PEER_OBLIGATIONS`, `version.rs:362`, six rows, every one of them
three components). It does **not** hold the specification's other rows — the
feature-support rows — and the four-component row the ceiling is derived from
(`0.6.1.10`) appears in the tree only as **prose**, in the doc comment above
`MAX_API_VERSION_COMPONENTS` (`version.rs:35-38`):

> *"The specification's table contains a four-component row (`0.6.1.10`), so
> the comparator must accept four components."*

That comment is an **assertion about a document the tree does not hold**. F5's
complaint is precisely that the ceiling rests on it.

**The plan's deliverable.** The specification's table's widest row becomes a
**held record**, and the ceiling becomes a **derivation** from that record. A
future five-component row in the specification's table then widens the
comparator by construction — the version becomes **comparable**, which is
exactly what F5 says it would not — instead of becoming uncomparable behind a
hard-coded four.

## 2. Why ready

- Plan 491's floors, obligations and the eligibility gates are **live** and
  **guarded**; Plan 503 made the DSM gate and the peer-eligibility receipt
  live; Plan 509 decided the lookup-target posture. The comparator is a
  settled, load-bearing part of the product.
- The comparator's semantics are **already pinned** by an executable artefact
  and a static guard, so a change here is measurable rather than speculative.
- The two pinned references are **available locally at their pins**, so their
  component handling can be **derived from their own source** rather than
  inferred — the same method Plan 495 used for the peer-eligibility
  predicates.
- The finding is **Low** and **forward-looking**: the bound does not bind
  today, because the specification's widest row is four components and the
  comparator accepts four. So the plan can be careful without being late.

## 3. Current implementation evidence

### 3.1 The ceiling, and where it is asserted

| Fact | Where |
| --- | --- |
| `pub const MAX_API_VERSION_COMPONENTS: usize = 4;` | `crates/i2pr-proto/src/common/version.rs:38` |
| The only justification is the doc comment quoting `0.6.1.10` | `crates/i2pr-proto/src/common/version.rs:35-37` |
| The array is sized by the constant | `version.rs:48` — `components: [u16; MAX_API_VERSION_COMPONENTS]` |
| `parse` refuses a fifth component | `version.rs:95` — `if digits == 0 || len >= MAX_API_VERSION_COMPONENTS { return None; }` |
| The stored width is asserted in prose too | `version.rs:69-72` — *"one to four dotted components"* |
| `ApiVersion::new` fills the array literally | `version.rs:54-59` — `components: [major, minor, patch, 0]` |
| `ApiVersion::new4` fills the array literally | `version.rs:62-67` — `components: [major, minor, patch, build]` |

### 3.2 The ordering, and the exactness premise

| Fact | Where |
| --- | --- |
| `cmp_to` compares component-wise; the exhausted side is **less** | `version.rs:146-175` |
| `cmp_to` must stay `const` — `floors_in_force` is a `const fn` | `version.rs:441`, `:448`, `:462` |
| `ApiVersion`'s `PartialEq`/`Eq`/`PartialOrd`/`Ord` are **all** `cmp_to` | `version.rs:188-206` |
| `PublishedVersion` derives **no** `Ord`/`PartialOrd` | `version.rs:226` — `#[derive(Clone, Copy, Debug, PartialEq, Eq)]` |
| Every floor declares **three** components | `version.rs:362-397` — `ApiVersion::new(0, 9, 62)` and five others |

**Two consequences the plan relies on.**

- Because `PublishedVersion` has no `Ord`, **two published peer versions
  cannot be ordered through the public API at all**. The only ordering the
  router performs is *peer version against a floor*, and every floor declares
  three components. So the comparator's ordering is exercised only where the
  floor's width is three.
- Because the stored array is sized by the same constant as the parser's
  bound, the ordering is **exact** for every version the parser accepts:
  there is no truncation and no lossy fallback. That is the property the plan
  must not weaken.

### 3.3 What the tree holds of the specification's table

`MINIMUM_PEER_OBLIGATIONS` (`version.rs:362`) holds the six rows that carry a
minimum-peer obligation, and its doc comment (`version.rs:341-357`) says so
explicitly: *"Only the rows that carry a minimum-peer obligation appear here;
the other rows describe feature support and impose no floor."* The
four-component row is one of the **other** rows, so it is not held as data.

### 3.4 What the two pinned references actually do — derived from their source

This evidence does not exist in the repository today. Both readings below are
from the references' **own pinned source**, not from a model of it.

**i2pd 2.61.0** (pin `635b013a612ff47278ef02acf8580a28e10e26c5`, checkout at
`/tmp/i2pd-ref`, tag `2.61.0`):

- `libi2pd/RouterInfo.cpp` parses the `router.version` property by
  **collapsing every digit into an `int`**:

  ```cpp
  m_Version = 0;
  for (auto ch: value)
  {
      if (ch >= '0' && ch <= '9')
      {
          m_Version *= 10;
          m_Version += (ch - '0');
      }
  }
  ```

  There is **no component count** in that parse: the separators are simply not
  digits and are skipped. A five-component version is therefore **not
  malformed** to i2pd.
- `libi2pd/version.h:18` defines
  `#define MAKE_VERSION_NUMBER(a,b,c) ((a*100+b)*100+c)`, so the floor
  `0.9.62` is the integer `962` (`libi2pd/NetDb.hpp:54-58` names
  `NETDB_MIN_HIGHBANDWIDTH_VERSION`, `NETDB_MIN_FLOODFILL_VERSION` and
  `NETDB_MIN_SHORT_TUNNEL_BUILD_VERSION` from it).
- Therefore `"0.9.62.1.2"` collapses to `96212`, which is **above** `962`:
  **i2pd admits a five-component version and treats it as above the floor.**

**Java I2P** (`net.i2p.util.VersionComparator`):

- The comparator walks both strings, extracts components separated by `.`,
  `-` or `_`, and parses each with `parseLong` — a **`long`**, not an `int`.
- The loop runs until **both** strings are exhausted: there is **no component
  count cap** in the implementation.
- When one side runs out of components and the other still has one, the
  exhausted side is **less**. That is the rule `i2pr`'s `cmp_to` already
  implements, and the rule `version.rs:43-45` already credits to Java.
- **Therefore Java admits a five-component version too, and orders it above a
  three-component version with the same prefix.**

**Provenance caveat, stated plainly.** The pinned Java reference is
**2.13.0** (`9134f808337b401e8e53c73734c81fab04280c9d`). The Java
installation available on this host is **2.13.1**, and `VersionComparator`
was read from that installation's `i2p.jar`. The reading **corroborates** the
behaviour the repository already records in
`scripts/plan495-peer-eligibility.py` — a dotted version split into integer
components with no count cap — but it is **not** a reading of the pinned
2.13.0 artifact. The **primary** derivation is i2pd's, whose checkout is at
the exact pin.

### 3.5 The divergence this creates

The router is the **only** one of the three that refuses a version with more
than four components. The refusal is:

- **typed** — `PublishedVersion::Uncomparable` →
  `VersionEligibility::UncomparableVersion`, a distinct refusal reason, not a
  generic failure;
- **counted** — the refusal is reported by reason, never silently skipped;
- **fail-closed** — the peer is excluded, not admitted;
- **monotone** — a version with more components than a floor can only ever be
  **above** a shorter version with the same prefix, so the refusal can only
  **exclude** a peer that both references would admit. It can never admit a
  peer below the floor.

### 3.6 What a widening would cost

Measured on this tree, not estimated:

| Quantity | Today | At a 128-component ceiling |
| --- | --- | --- |
| `size_of::<ApiVersion>()` | 10 bytes | 257 bytes |
| `size_of::<PublishedVersion>()` | 12 bytes | 260 bytes |
| `Profile` (`crates/i2pr-netdb/src/profile.rs:393-402`) | ~64 bytes | ~312 bytes |
| The profile store's ≤4096 profiles | ~262 KiB | ~1.3 MiB |

The `ProfileProjection` does **not** carry the published version
(`crates/i2pr-netdb/src/profile_store.rs:303-334`), so the projection byte
budget is unaffected either way.

## 4. Invariants that must not regress

1. **The comparator's ordering stays exact.** No truncation, no lossy
   fallback, no widened tie-break. Every version the parser accepts must
   compare exactly as the dotted-decimal order says.
2. **The parser's bound and the stored width stay the same constant.** A
   version the parser accepts must be stored in full.
3. **A version beyond the bound stays a typed refusal**, counted by reason,
   never a silent skip and never a pass.
4. **Every floor still declares three components**, so every live ordering is
   against a three-component floor.
5. **The floors themselves do not move.** `MINIMUM_PEER_OBLIGATIONS` is
   unchanged, and no floor is added, removed or re-derived.
6. **`cmp_to` stays `const`**, and `floors_in_force` stays a `const fn`.
7. **No peer is admitted below a floor.** The loosening F5 contemplates must
   be *monotone upward only*.
8. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo are unchanged; the support inventory's advertised
   count stays zero; `Cargo.toml` and `Cargo.lock` stay byte-identical.
9. **No production behaviour change.** The comparator must accept and refuse
   **exactly** the versions it accepts and refuses today.
10. **No plan of record and no closed closure record is rewritten.**

## 5. Scope

**In scope.**

- `crates/i2pr-proto/src/common/version.rs`: hold the specification's table's
  widest row as data, derive the ceiling from it, and make the derivation
  the only thing that decides the bound.
- A new executable artefact that demonstrates the derivation, today's
  behaviour, and the refusal's typed path.
- A new static guard that pins the derivation, the held row, the exactness
  premise, the typed refusal, and the floors' width.
- `specs/QUALIFICATION-GATES.md`: record what the two pinned references do
  with a version that has more components than the specification's table has
  ever used — the first time this repository records it.
- `specs/CONFORMANCE.md`, `specs/support.toml`,
  `docs/architecture/i2pr-proto.md`, `docs/architecture/i2pr-daemon.md`: the
  plan's findings.
- `plans/registry.md`, `plans/subsystems/core-router-recovery-roadmap.md`,
  and this plan's closure record.

**Out of scope.**

- **The component width.** The parser's per-component bound is `u16`
  (≤ 65535); Java parses components as `long`s and would accept a larger one.
  That is a **second, separate divergence** and it is recorded as a finding
  for a successor, not fixed here: widening the width to `u64` would multiply
  the stored array by four again, and a version with a component above 65535
  is not a version.
- **The i2pd digit-collapse.** i2pd's ordering is not a component-wise
  comparison at all — it cannot distinguish `0.9.6.2` from `0.9.62`. The two
  references therefore **disagree with each other**, which is the reason the
  router follows the specification's table rather than either oracle.
- **The floors.** Plan 491's floors and obligations are not re-derived.
- **The lookup target.** Plan 509's decision is unchanged.
- **Any other plan's guard.** Every other plan's guard file stays
  byte-identical.

## 6. Production changes

**One crate, one file, one constant, one new record.**

| Change | Where |
| --- | --- |
| Hold the specification's table's widest row as data | `crates/i2pr-proto/src/common/version.rs` — a new `pub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 6, 1, 10];` |
| Derive the ceiling from that record | the same file — `pub const MAX_API_VERSION_COMPONENTS: usize = <the record's width>;` |
| Nothing else in production | — |

The stored array's width, the parser's bound and the held record's width are
**one** number after this change, and that number is a **fact about the
specification's table** rather than a literal in the comparator.

The change is **behaviour-preserving**: the record is four components wide, so
the constant is four, so the parser and the ordering are exactly as they are
today. What changes is that the number now has a **source**.

## 7. Design decisions the plan must make explicit

### D1 — The ceiling is derived from a held record, not asserted in prose.

The specification's table's widest row becomes data. The ceiling is computed
from it. A future five-component row in the specification's table widens the
comparator by construction, and the version becomes **comparable** — which is
the outcome F5 says the current design denies.

### D2 — The derivation must be mechanical, and the guard must pin the record.

Two guards, doing two different jobs:

- The **guard** pins the *record*: if the tree's record of the specification's
  table changes, the guard **reports** it, so the change is a deliberate act
  with a re-derivation attached, not a silent edit.
- The **guard** also pins the *derivation*: the ceiling's initialiser must
  name the record, so the ceiling cannot be re-pinned to a literal.

### D3 — The record is pinned, and the pin is the specification's row.

The held record is `0.6.1.10` — the four-component row the specification's
table contains, and the row the current doc comment already quotes. The plan
does not invent a row and does not fabricate the table: it holds the **one**
row that the ceiling actually depends on, and says so.

### D4 — The divergence from both references is recorded, not closed.

The router refuses a version with more components than the specification's
table has ever used; neither pinned reference refuses one. The plan records
this, with the safety argument (typed, counted, fail-closed, monotone), and
does **not** pretend the router matches the references. The references
disagree with each other, and the specification's table is the router's own
normative source for the obligations the floors come from.

### D5 — No production behaviour change is the outcome, not a shortfall.

The plan makes the assumption *derived and held* and makes the harm
*structurally impossible*, without changing which versions the router accepts
today. The plan's value is that the next person to widen the specification's
table gets a **comparable** version rather than an **uncomparable** one, and
gets told that the table moved.

### D6 — The exactness premise is guarded.

The ordering is exact for every version the parser accepts. It is also exact
for every live decision, because every floor declares three components and
`PublishedVersion` has no `Ord`. The guard pins both: the floors' width, and
the absence of a peer-to-peer ordering path.

## 8. Work packages

- **WP1 — the derivation.** Hold the specification's table's widest row as
  data; derive `MAX_API_VERSION_COMPONENTS` from it; correct the two prose
  sites that assert "four" (`version.rs:35-38` and `version.rs:69-72`) so
  they describe the derivation rather than a number.
- **WP2 — the artefact.** `crates/i2pr-proto/tests/version_ceiling_derivation.rs`:
  the ceiling equals the held record's width; a four-component version parses
  and compares exactly; a five-component version is refused today; the
  refusal is typed and counted downstream; a **wider held row would widen the
  ceiling**, demonstrated through the derivation's own helper.
- **WP3 — the guard.** `crates/i2pr-daemon/tests/version_ceiling_boundary.rs`:
  the derivation is in place; the record is the pinned row; the parser's
  bound and the stored width are the same number; the ordering is exact and
  treats the exhausted side as less; every floor declares three components;
  no peer-to-peer ordering path exists; the refusal is typed. Every rule
  carries an **inverted probe**.
- **WP4 — the references' evidence.** `specs/QUALIFICATION-GATES.md` gains
  the section recording what i2pd 2.61.0 and Java I2P do with a
  more-than-four-component version, with the pins, the file and line, and the
  Java provenance caveat.
- **WP5 — the closure.** The support inventory, the conformance record, the
  two deep-dives, the registry and roadmap rows, the negative controls, the
  floor, and the closure record.

## 9. Failure, cancellation, restart, and contention semantics

Not applicable in the usual sense: the plan adds no task, no timer, no
listener, no socket, no channel and no I/O. The comparator is a pure,
`const`-callable function over a bounded byte string. A malformed version
fails **closed** and **typed**, exactly as today.

## 10. Compatibility and migration

None. Nothing is persisted, renamed, reinterpreted or re-versioned. The
comparator accepts and refuses exactly the versions it accepts and refuses
today. The only new public names are the held record and, if the plan adds
one, the derivation's helper.

## 11. Required tests

| Test | What it proves |
| --- | --- |
| `crates/i2pr-proto/tests/version_ceiling_derivation.rs` | the derivation, today's behaviour, the typed refusal, and that a wider held row widens the ceiling |
| `crates/i2pr-daemon/tests/version_ceiling_boundary.rs` | the guard's ten rules, each with an inverted probe |
| `crates/i2pr-daemon/tests/version_floor_boundary.rs` | Plan 503's guard is **unchanged** and still passes |
| `crates/i2pr-proto` unit tests | the existing malformed-version rows still hold |

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-proto --test version_ceiling_derivation -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_ceiling_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 -c "import tomllib; tomllib.load(open('specs/support.toml','rb'))"
```

## 13. Documentation updates

- `specs/QUALIFICATION-GATES.md` — the references' component handling, with
  the pins, the file and line, and the Java provenance caveat.
- `specs/CONFORMANCE.md` — the plan's section: what is claimed, what is not.
- `specs/support.toml` — one `experimental`, non-advertised surface.
- `docs/architecture/i2pr-proto.md` — the derivation and the divergence.
- `docs/architecture/i2pr-daemon.md` — the eligibility path's treatment of an
  uncomparable version.
- `plans/registry.md`, `plans/subsystems/core-router-recovery-roadmap.md`.

## 14. Acceptance criteria and stop conditions

**Accepted when all of the following hold.**

1. `MAX_API_VERSION_COMPONENTS` is **derived** from the held record, and the
   held record is the specification's four-component row.
2. The comparator accepts and refuses **exactly** the versions it accepts and
   refuses today — proved by the artefact and by the existing proto unit
   tests.
3. The ordering stays **exact**: no truncation, no lossy fallback.
4. Every floor still declares three components; `cmp_to` is still `const`.
5. The guard's rules hold on the landed tree, and each rule's inverted probe
   fires.
6. The negative controls are applied to the real tree, each breaks an
   **assertion** rather than the build, and each is reverted byte-for-byte.
7. `router.version` stays `0.9.62`; `caps` and the published RouterInfo are
   unchanged; the advertised count stays zero; `Cargo.toml` and `Cargo.lock`
   are byte-identical.
8. Plan 491's closure record is **not** rewritten; the discharge lives here
   and in the registry row.

**Stop conditions.**

- If the derivation turns out to need a *new* number rather than the record's
  width, stop: a new number is an **invented** bound, which the repository's
  rule forbids.
- If the exactness premise fails — if a floor ever declares more components
  than the comparator stores — stop and re-derive; do not widen the array to
  make a symptom disappear.
- If the references' evidence cannot be stated with a pin and a file and
  line, the divergence is **not** recorded and the plan does not close.

**What this plan does not close.** Plan 491's **F2** (no stock-peer evidence)
is blocked externally on Plan 493's authorized two-host topology. The
per-component width divergence (§5) is recorded for a successor.

## 15. Closure evidence required

- The requirement-to-evidence matrix.
- The exact commands and their outcomes.
- The negative controls, with the mutation, the failing-test count, and the
  byte-for-byte restoration.
- The invariant, failure, migration and security reviews.
- The limitations, the findings by severity, the roadmap disposition, and the
  unblock audit.
- The floor table.

## 16. Handoff notes

- The production diff is **one file and one constant** plus the held record.
  The comparator's behaviour does not change.
- Plan 503's `version_floor_boundary.rs` and every other plan's guard file
  stay byte-identical. If a guard fails, the answer is a change in **this**
  plan's code, never an edit to another plan's guard.
- The references' evidence is quoted from their **own pinned source**. The
  Java reading is from **2.13.1**, not the pinned **2.13.0**, and says so.
- The next free plan number is **512**.
