# Plan 285 — M12 close the I2NP peer-testing gap and reconsider the version declaration

Status: **retained-m12-peer-testing-implemented-and-declared-pending-mixed-router-evidence**

Classification: capability correction. This is the only work that stands between the controlled
profile and reference-side floodfill eligibility. It owns a version-declaration decision, so it is
gated on the `specs/CONFORMANCE.md` capability-advertisement checklist.

Hard dependencies: Plan 278 (stopped at the reference gate) and Plan 284 (localized both gates).

## Objective

Close the second, non-mechanical gate that Plan 284 isolated. Plan 284 proved that the pinned
reference will only use a peer as a floodfill if that peer declares an I2NP version at or above
0.9.62, and that no injection path avoids the requirement. This plan determines whether
`i2pr` can honestly reach that level, and either closes the gap or records why it cannot.

## The gate, in source terms

`RouterInfo::IsEligibleFloodfill` (`libi2pd/RouterInfo.cpp:1022-1029`) requires
`m_Version >= NETDB_MIN_FLOODFILL_VERSION` (962, i.e. 0.9.62) with no high-bandwidth
alternative, and every peer-side `m_Floodfills.Insert` consults it. `i2pr`'s controlled
declaration is now 0.9.62, so the controlled record clears both gates. That value is substantiated
by the implemented I2NP surface, and the mixed-router evidence that would validate it end to end is
named as the remaining gap in the checklist below.

## The gap is now concrete, not a judgement call

`specs/protocols/02-i2np.md` defines `router.version` as an I2NP feature/API version, and
`specs/CONFORMANCE.md` requires advertising "the lowest truthful current feature level
compatible with its implemented subset". So the question is a finite, checkable set of I2NP
message types, not an open-ended conformance argument.

Comparing `crates/i2pr-proto/src/i2np/header.rs` against the reference's complete type
enumeration (`libi2pd/I2NPProtocol.h:111-125`):

| wire code | reference type | `i2pr` |
| --- | --- | --- |
| 1 | DatabaseStore | yes |
| 2 | DatabaseLookup | yes |
| 3 | DatabaseSearchReply | yes |
| 10 | DeliveryStatus | yes |
| 11 | Garlic | yes |
| 18 | TunnelData | yes |
| 19 | TunnelGateway | yes |
| 20 | Data | yes |
| 21 | TunnelBuild | yes |
| 22 | TunnelBuildReply | yes |
| 23 | VariableTunnelBuild | yes |
| 24 | VariableTunnelBuildReply | yes |
| 25 | ShortTunnelBuild | yes |
| 26 | ShortTunnelBuildReply | yes |
| 231 | TunnelTest | **no** |

`i2pr` implemented every I2NP type through short tunnel-build. The single missing type was
`TunnelTestMessage` (231), which is precisely the 0.9.62-era peer-testing addition the reference
pairs with that level: `NETDB_MIN_PEER_TEST_VERSION` is 0.9.62, and the reference clears a
router's SSU2 peer-testing address caps below that level (`RouterInfo.cpp:468-473`).

So the blocker was one message type, not a version-string debate, and it is now implemented.

## Delivered

- `MessageType::TunnelTest` at wire code 231, pinned by test; the identifier is not adjacent to the
  build family, so it is spelled out rather than range-grouped.
- `TunnelTestMessage { msg_id, timestamp }`, an exactly 12-byte body decoded with exact consumption
  and covered by a reference-derived golden vector whose checksum byte is independently verified as
  `SHA-256(body)[0]`.
- `I2npBody::TunnelTest` with a typed `RouterI2npOutcome::PeerTest` dispatcher arm carrying the
  authenticated peer, the exact link, and the probed fields.
- `i2pr-daemon::peer_test`: `PeerTestEcho` (the bounded, honest answer owed to an inbound probe,
  built from the authenticated delivery and echoed with the probed fields unchanged) and
  `PeerTestTracker` (capacity-bounded, age-bounded outstanding-probe table that refuses rather than
  evicts, reports a late answer as `TimedOut` rather than healthy, and exposes `expire` so
  retention is observable).
- The transit owner ignores a peer test rather than routing it into the tunnel tables.
- `CONTROLLED_ROUTER_VERSION` raised to 0.9.62 with the consistency test that fails if the
  declaration outruns the implemented surface.

## `specs/CONFORMANCE.md` checklist result

The plan required the capability-advertisement checklist to be satisfied before raising the
declaration. Its five steps, answered honestly:

1. **Feature implications.** Done. `router.version` is an I2NP feature/API version, and 0.9.62's
   paired I2NP type is `TunnelTest` (231). The type table above is the identification.
2. **Verify implied mandatory behavior.** Done. A peer that sends a test receives an answer derived
   from the authenticated delivery; an originator's probe is either matched or reported as timed
   out, never silently dropped. Pinned by nine daemon tests and four codec tests.
3. **Mixed-router tests for the changed claim.** **Not satisfied locally, and named as the
   remaining gap.** This is the circularity the plan anticipated: the mixed-router validation needs
   a reference that admits the record, and admission needed this declaration. The gap is now one
   bounded external attempt in the Plan 278 lane, not an open-ended conformance argument.
4. **Downgrade/unsupported peers.** Done for the decode boundary: an unknown type keeps its bounded
   unsupported disposition and a malformed body fails closed, so a peer below this level or sending
   a truncated probe is handled without a false claim.
5. **Update the dossier and support matrix.** Done: the I2NP dossier already defines the semantics,
   and `specs/support.toml` records the declaration and this plan.

Because step 3 is unmet, this plan closes **retained**, not passed. The local floor is green and the
declaration is substantiated by the implemented surface, but no external row may be claimed from
this plan.

## Required work

- Implement bounded I2NP `TunnelTest` (231) decode and dispatch on the runtime-neutral path, with
  a typed unsupported outcome where the downstream service is absent, and exact-consumption
  decoding with caller-visible allocation caps. Do not add a `MessageType` variant by widening
  the unknown-type handling; add the real variant and its wire code.
- Decide the peer-testing feature itself, not only the codec: an inbound test that a peer
  expects an answer to must have a bounded, honest answer, and an unanswered test must not be
  indistinguishable from a broken one.
- Keep the declaration in `i2pr_netdb::CONTROLLED_ROUTER_VERSION` as the single source of truth.
  Raise it only after the type exists and the checklist below is satisfied. It must never be
  edited to satisfy a peer's admission gate.
- Satisfy the `specs/CONFORMANCE.md` capability-advertisement checklist before raising the
  declaration above 0.9.58: identify the implied features, verify mandatory behavior, add
  mixed-router tests, test downgrade/unsupported peers, and update the dossier and support
  matrix.
- Record the circularity explicitly if it cannot be broken: the checklist's mixed-router
  validation for a version declaration needs a reference that accepts the record, while
  acceptance needs the declaration. If the checklist cannot be completed inside this plan, close
  it `blocked-*` naming the exact missing evidence rather than claiming the checklist passed.
- Land regression tests: the new type decodes and dispatches, the declaration stays consistent
  with the implemented surface, and a record at the declared level is not accepted while the
  type is absent.

## Out of scope

- `O` or any other capability letter. `O` is a high-bandwidth claim and stays false for an
  experimental loopback router.
- Broadening network access, patching or vendoring the reference, raising the Plan 278 attempt
  budget, or relaxing any guard, script, or test-selection rule.
- Public-network or second-router claims. Plan 279 remains a separate gate.

## Evidence

- Local: the full routine floor, plus a codec/dispatch test per new type and a declaration
  consistency test.
- External: the Plan 278 exact-pinned lane, one bounded attempt against i2pd 2.61.0 at the frozen
  pin, to observe whether the reference now admits the record and treats it as a floodfill.
  Sanitized counts only.

## Stop conditions

Stop and record the boundary if closing the gap would still require an unreviewed capability or
version claim, a patched reference, a raised budget, or broadened network access. Stop on any
reproducible defect outside the I2NP codec and peer-testing surface and register a further narrow
corrective.

## Closure evidence

Requirement-to-evidence matrix, exact commands with local and CI outcomes labelled truthfully,
the checklist result with its named gaps, security and invariant review, and the updated dossier
and support matrix.
