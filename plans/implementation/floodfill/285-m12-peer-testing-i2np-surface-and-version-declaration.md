# Plan 285 — M12 close the I2NP peer-testing gap and reconsider the version declaration

Status: **registered-m12-peer-testing-gap-is-the-remaining-floodfill-eligibility-blocker**

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
declaration is currently 0.9.58, so the controlled record loads and is then refused floodfill
eligibility.

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

`i2pr` implements every I2NP type through short tunnel-build. The single missing type is
`TunnelTestMessage` (231), which is precisely the 0.9.62-era peer-testing addition the reference
pairs with that level: `NETDB_MIN_PEER_TEST_VERSION` is 0.9.62, and the reference clears a
router's SSU2 peer-testing address caps below that level (`RouterInfo.cpp:468-473`).

So the remaining blocker is one message type, not a version-string debate.

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
