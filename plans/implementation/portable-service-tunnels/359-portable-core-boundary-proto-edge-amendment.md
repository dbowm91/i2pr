# Plan 359 — portable-core boundary guard: name the one permitted `i2pr-proto` edge

Status: **executed-inline-with-the-main-merge; amendment landed and mutation-tested**

Subsystem: `portable-service-tunnels` (the guard is that subsystem's rule).

Classification: **invariant** (a static-guard-backed boundary property) + **planning
amendment**. This is the plan-of-record required by `AGENTS.md` ("a plan-of-record is
required to close each one; the rule is still to fix the boundary, never to weaken a
script") for the deliberate amendment of a CI-enforced guard.

## Why this plan exists

Merging six branches into `main` surfaced a contradiction between two plans that had
never seen each other. Neither plan was wrong when it was written; they assumed
different facts about the other.

**Portable-service-tunnels/350** audited the portable policy core, found no production
use of `i2pr-proto`, removed the dead dependency, and wrote a guard to stop the package
regressing to a workspace dependency:

```text
portable_dependency_pattern='i2pr-(daemon|runtime|netdb(-persist)?|transport(-ntcp2|-ssu2)?|tunnel|testkit|proto)'
```

Its stated rationale, in the guard's own comment, was that the core "must not acquire
ownership of **router state, persistence, or runtime facilities**", and that the audit
"found no production use of i2pr-proto".

**Proposal 170/351** added exactly such a production use, on a branch that predated the
portable guard: `crates/i2pr-service-tunnels/src/destination.rs` imports
`i2pr_proto::{EncryptedServiceAddress, is_encrypted_service_address}` so that a
`.b33` encrypted-service address can be a service-tunnel remote target. Its doc comment
records that `EncryptedServiceAddress::from_text` **is** the whole validation policy —
body length, Base32 alphabet, canonical trailing bits, CRC-32, both signature types, and
both structural flags — and that the service layer adds nothing and therefore cannot
weaken it.

Neither branch could observe the conflict. Once both landed on `main`,
`check-service-tunnel-boundaries.sh` failed against correct, separately-landed code.

## The decision

The guard's *premise* went stale; its *rationale* did not.

`i2pr-proto` is the bounded wire-codec crate. It is runtime-neutral, performs no I/O,
owns no router state and no persistence, and is already a production dependency of
`i2pr-api`, `i2pr-netdb`, `i2pr-transport`, and `i2pr-client`. It is therefore not in the
class the rule exists to exclude, and Plan 351's use of it is exactly the kind of reuse
the portable line exists to enable.

So the edge is **permitted explicitly**, and the amendment is written as a named
exception with its reason in the script — not as a silent deletion of `proto` from a
pattern, which would have left the next reader unable to tell whether the omission was
deliberate.

### What changed in `scripts/check-service-tunnel-boundaries.sh`

1. `proto` removed from `portable_dependency_pattern`. Every crate that actually owns
   router state or a runtime — `daemon`, `runtime`, `netdb`, `netdb-persist`,
   `transport`, `transport-ntcp2`, `transport-ssu2`, `tunnel`, `testkit` — stays
   forbidden, unchanged.
2. The "no direct production dependency on another i2pr crate" rule now excepts exactly
   `i2pr-proto` by name rather than forbidding the whole set. Every other workspace
   crate, including `i2pr-client`, `i2pr-core`, and `i2pr-crypto`, is still rejected.
3. The positive control was rewritten to assert both directions: each forbidden class
   must still be caught, **and** the one permitted edge must *not* trip either rule.

### The rejected alternative

Keeping the guard byte-identical required removing the `i2pr-proto` edge from the
production build. That was rejected on inspection, not on preference: it would force
`i2pr-service-tunnels` either to duplicate the encrypted-service codec — creating a
second implementation to drift from the one Plan 351 exists to use — or to defer
validation to the daemon, weakening the crate's boundary-validation responsibility. Both
are worse outcomes than the defect this amendment fixes. The alternative is recorded here
so the reasoning is auditable rather than implied by the diff.

## Defect found during execution and fixed before landing

The first draft of the manifest-scan exception anchored on `^[[:space:]]*i2pr-proto`
while the scan piped through `grep -En`, so every candidate line was prefixed with its
line number and **nothing** was excepted — the guard failed against the very manifest it
was written to permit. It was caught by running the guard against the real tree, not by
inspection. The anchor is now `'^[0-9]+:[[:space:]]*i2pr-proto[[:space:]]*='`.

Recorded because it is the shape of defect a reviewer reading only the pattern would
miss: an exception filter that silently excludes nothing still *looks* like an exception.

## Evidence

`scripts/check-service-tunnel-boundaries.sh`, mutation-tested locally:

| Mutation | Expected | Result |
|---|---|---|
| add `i2pr-daemon` | fail | fail (detected) |
| add `i2pr-netdb-persist` | fail | fail (detected) |
| add `i2pr-transport-ssu2` | fail | fail (detected) |
| add `i2pr-tunnel` | fail | fail (detected) |
| add `i2pr-runtime` | fail | fail (detected) |
| add `i2pr-testkit` | fail | fail (detected) |
| add `i2pr-client` | fail | fail (detected) |
| add `i2pr-core` | fail | fail (detected) |
| add `i2pr-crypto` | fail | fail (detected) |
| unmodified tree (control) | pass | pass |
| `i2pr-proto` edge removed (control) | pass | pass |
| restored tree (control) | pass | pass |

**9 of 9 mutations detected; 3 of 3 controls pass.**

`crates/i2pr-service-tunnels/API-SNAPSHOT.txt` was regenerated for the same pass:
**18 additions, 0 removals** — the Plan 342 `TargetPolicy` surface
(`target_policy`, `*_with_policy`, `classify_client_target`, `ClientTargetClass`,
`is_via_outproxy`, `refusal`) and Plan 351's `destination::encrypted_service`. Nothing
was deleted from the public surface. `scripts/check-portable-service-tunnel-api.py`
passes at 696 declarations.

## Invariants preserved

- The portable core still owns no router state, persistence, runtime, transport, tunnel,
  or testkit dependency. Nine of nine mutations prove the rule bites.
- Rule 1 of the same script — no Tokio, sockets, listeners, tasks, or timers — is
  untouched, as is every rule below the two amended.
- `scripts/check-dependency-direction.sh` continues to require
  `i2pr-service-tunnels` -> `i2pr-proto` to be *named* in its expected map. That entry was
  lost in the merge and restored; the direction guard and the boundary guard now agree
  about exactly one workspace edge.
- No socket, no runtime ownership, and no protocol behaviour moved. `i2pr-proto` was
  already a production dependency of four other runtime-neutral crates.

## Limitations

- This plan records an amendment to a guard. It does not widen the portable core's
  *runtime* neutrality, which rule 1 enforces separately and which this change does not
  touch.
- `i2pr-proto` is permitted as a dependency; the portable line's ADR 0033 ownership
  contract is unchanged.
- Nothing here re-opens Proposal 170/351's closure or changes any status token.

## Handoff

`main` carries the amendment, the restored dependency and its direction-guard entry, the
regenerated API snapshot, and the collision-ledger reconciliation that made the merge
possible. Plan 348 stays blocked on Proposal 170/347; nothing in this plan affects that.
