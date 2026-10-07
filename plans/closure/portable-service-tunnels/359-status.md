# Plan 359 status — portable-core boundary guard: the one permitted `i2pr-proto` edge

Status: **`passed-portable-core-boundary-one-permitted-proto-edge-amendment`**.

Plan of record: [`359-portable-core-boundary-proto-edge-amendment.md`](../../implementation/portable-service-tunnels/359-portable-core-boundary-proto-edge-amendment.md).

## Execution context — read this before the status token

Plan 359 was **not** executed as a dedicated implementation pass. It was authored and
applied inline inside the six-branch merge that produced `2416c30b`, and it has been
`main`'s behaviour since. What was missing was the conventional record, so this file
exists to make Plan 359's already-landed state citable the same way as every other
closed milestone.

**Plan 379 wrote this record; Plan 379 did not execute Plan 359.** Nothing below claims
new implementation work. The only genuinely new thing here is §"Verification", which
*re-ran* Plan 359's mutation evidence against the tree as it stands today so the claim
is reproduced rather than inherited. The plan document and the registry row describing
Plan 359 as an inline amendment are unchanged.

The token is `passed-*` because every requirement below is satisfied by code already on
`main` and re-verified here. The inline provenance is stated above and in the roadmap
row rather than folded out of the token.

## Implementation

- `2416c30b` — *"plans+guard: reconcile six branches onto main, and fix what the merge
  exposed."* This commit, and no other, landed Plan 359: it removed `proto` from
  `portable_dependency_pattern`, re-wrote the manifest rule to except exactly
  `i2pr-proto` by name, restored the `i2pr-proto = { path = "../i2pr-proto" }` edge in
  `crates/i2pr-service-tunnels/Cargo.toml`, restored that edge's entry in
  `scripts/check-dependency-direction.sh`, regenerated
  `crates/i2pr-service-tunnels/API-SNAPSHOT.txt`, and rewrote the guard's positive
  control to assert both directions.

## What the amendment was

Plan 350 removed the `i2pr-proto` edge as dead code and wrote a guard forbidding it,
justified by an audit *result* rather than by a claim about the crate. Proposal 170/351
independently added a production use of `i2pr_proto`
(`EncryptedServiceAddress`, `is_encrypted_service_address` in `destination.rs`). Both
landed on `main`; neither branch could see the other; the guard then failed against
code that was correct on each side.

`i2pr-proto` is the bounded wire-codec crate: runtime-neutral, no I/O, no router state,
no persistence, and already a production dependency of `i2pr-api`, `i2pr-netdb`,
`i2pr-transport`, and `i2pr-client`. It is not in the class the rule excludes. The
amendment permits the edge **by name**, with its reason written into the script, rather
than by deleting an alternative from a regex — a silent omission would leave the next
reader unable to tell a decision from an oversight.

The rejected alternative — keeping the guard byte-identical and dropping the
production use — is recorded in the plan document. It would have forced either a second
encrypted-service codec implementation or a deferral of validation to the daemon. Both
are worse than the defect the amendment fixes.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| The permitted edge is named, not omitted | `scripts/check-service-tunnel-boundaries.sh` lines 33–65: the amendment rationale is a ~16-line comment, `proto` is removed from `portable_dependency_pattern`, and the manifest rule excepts `'^[0-9]+:[[:space:]]*i2pr-proto[[:space:]]*='` by name | Pass |
| Every router-state/runtime owner stays forbidden | Same file, `portable_dependency_pattern='i2pr-(daemon\|runtime\|netdb(-persist)?\|transport(-ntcp2\|-ssu2)?\|tunnel\|testkit)'` — unchanged for all nine classes | Pass |
| The positive control asserts both directions | Lines 67–80: each forbidden class must match **and** the one permitted edge must not trip either rule | Pass |
| The direction guard agrees about exactly one workspace edge | `scripts/check-dependency-direction.sh` maps `"i2pr-service-tunnels": {"i2pr-client", "i2pr-proto"}`; the crate declares only `i2pr-proto` | Pass |
| The public surface is snapshotted | `crates/i2pr-service-tunnels/API-SNAPSHOT.txt`, 696 declarations; `python3 scripts/check-portable-service-tunnel-api.py` → `passed (696 declarations)` | Pass |
| The core is still runtime-neutral | Rule 1 of the same script (no Tokio/socket/listener/task/timer) is untouched by the amendment, as is rule 1's source-level counterpart at lines 85–98 | Pass |
| No protocol/runtime behaviour change | The amendment touches one guard script, one manifest dependency edge, one generated snapshot, and planning prose. No `.rs` production file changed. | Pass |
| No status token of a predecessor rewritten | Plans 349/350/351 closures are byte-identical to their pre-Plan-379 state | Pass |

## Verification

Run from the repository root on Plan 379's WP D head. **These are local runs, not CI.**

### Reproduced mutation evidence

Plan 359 recorded a 9-mutation table. It was re-run here rather than quoted, against a
throwaway copy of `crates/i2pr-service-tunnels/{Cargo.toml,src/}`,
`crates/i2pr-daemon/src/`, and the guard — so no mutation was ever applied to the real
tree:

| Mutation | Expected | Result |
|---|---|---|
| add `i2pr-daemon` | fail | fail |
| add `i2pr-runtime` | fail | fail |
| add `i2pr-netdb` | fail | fail |
| add `i2pr-netdb-persist` | fail | fail |
| add `i2pr-transport` | fail | fail |
| add `i2pr-transport-ntcp2` | fail | fail |
| add `i2pr-transport-ssu2` | fail | fail |
| add `i2pr-tunnel` | fail | fail |
| add `i2pr-testkit` | fail | fail |
| add `i2pr-client` | fail | fail |
| add `i2pr-core` | fail | fail |
| add `i2pr-crypto` | fail | fail |
| unmodified tree (control) | pass | pass |
| `i2pr-proto` edge removed (control) | pass | pass |
| restored tree (control) | pass | pass |

**12 of 12 forbidden dependency classes rejected; 3 of 3 controls pass.**

Plan 359's table listed 9 mutations, which is exactly the set the guard's
`portable_dependency_pattern` alternation names. Three more were run here for
completeness — `i2pr-netdb` and `i2pr-transport` (the bare forms of classes the pattern
already covers in their `-persist` / `-ntcp2` variants) and `i2pr-crypto`. Note the
mechanism differs: `i2pr-crypto` is **not** in `portable_dependency_pattern` at all. It
is rejected by the second rule — "no direct production dependency on any other i2pr
crate" — which is exactly the point of that rule existing. All three were caught, so the
extra rows widen an already-passing result; they do not change any conclusion.

### Commands

| Command | Result |
|---|---|
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed: `service-tunnel boundary checks passed` |
| `python3 scripts/check-portable-service-tunnel-api.py` | Passed: 696 declarations |
| `bash scripts/check-dependency-direction.sh` | Passed |
| `cargo tree -p i2pr-service-tunnels --edges normal` | Resolves `i2pr-proto v0.1.0` as a path dependency; no other `i2pr-*` package in the tree |
| Plan 379 routine floor | See `plans/closure/portable-service-tunnels/379-status.md` |

## Invariants, compatibility, and security

The portable core still owns no router state, persistence, runtime, transport, tunnel,
or testkit dependency; the mutation table is the evidence and it re-passes. Rule 1
(no Tokio, sockets, listeners, tasks, timers) and the source-level rule below it are
untouched.

No public API declaration was removed. Plan 359's snapshot regeneration was **18
additions, 0 removals** — the Plan 342 `TargetPolicy` surface
(`target_policy`, `*_with_policy`, `classify_client_target`, `ClientTargetClass`,
`is_via_outproxy`, `refusal`) and Proposal 170/351's
`destination::encrypted_service`.

No wire format, config key, storage format, or user-visible behaviour changed. No
dependency was added to the workspace graph by this plan — `i2pr-proto` was already a
production dependency of four other crates, and this plan only permitted an edge that
had independently become real.

Secret handling was not touched. No new `Debug`/`Display`/`Clone` was introduced on any
type, and no secret crosses the added edge: `EncryptedServiceAddress` is a parsed public
address form, not key material.

## Documentation and operations

- `scripts/check-service-tunnel-boundaries.sh` — amendment rationale and both rules,
  inline.
- `scripts/check-dependency-direction.sh` — the restored map entry.
- `crates/i2pr-service-tunnels/Cargo.toml` — the restored edge.
- `crates/i2pr-service-tunnels/API-SNAPSHOT.txt` — regenerated.
- `plans/global-number-collision-ledger.md` — untouched by Plan 359; Plan 359 introduces
  no number collision (`grep -n 359` returns nothing, which is correct: Plan 359 had a
  single owner).
- Operational impact: none. No listener, default, support claim, or runtime behaviour
  depends on this amendment.

## Limitations

- This record documents an amendment to a **guard**, not a feature. It does not widen
  the portable core's runtime neutrality, which rule 1 enforces separately and which
  this change does not touch.
- ADR 0033's ownership contract is unchanged. `i2pr-proto` is permitted as a
  dependency; it does not become part of the reusable policy surface.
- The `i2pr-client` entry in the direction guard's `i2pr-service-tunnels` allowlist is
  an unused future edge carried from Plan 350. Plan 359 did not add it and Plan 379 did
  not remove it; it is out of both plans' scope.
- Nothing here re-opens Proposal 170/351's closure or changes its status token.

## Findings by severity

- **critical**: none.
- **high**: none.
- **medium**: none.
- **low**: Plan 359's original mutation table named 9 forbidden classes while the
  guard's pattern matches 12. The table was a subset, not an error — the omitted three
  were never claimed to be tested. Corrected here by running all twelve.

## Unblock audit and disposition

Plan 379 was the only registered plan listing Plan 359 as a dependency or interface
contract; its single hard dependency was the landed amendment, which is satisfied.
Plan 379's remaining work packages were unaffected by this record's existence.

Roadmap disposition: **closed**. Plan 379 normalized this record; the roadmap row and
registry row now cite it. Plan 359 opened no successor.