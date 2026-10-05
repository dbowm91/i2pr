# Plan 326 — Encrypted LeaseSet2 and client authorization disposition

Status: **blocked-prop170-encrypted-leaseset-awaiting-external-type11-transcript**

> **Token corrected 2026-10-05 by Plan 344.** The previous token named
> Plan 325's Red25519 provider qualification as the gate. That question was
> superseded for forward architecture by Plan 331, which passed. The true gate
> is Plan 335's external type-11 transcript divergence. The original token and
> all text below are preserved; see the dated correction at the end of this file.

Implementation commits: none. Plans 323 and 324 are passed; Plan 325's provider qualification closed blocked.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Plan 323 canonical non-deep TunnelManager owner is closed. | `plans/closure/i2pcontrol-proposal-170/323-status.md` | Passed. |
| Plan 324 supplies typed SigType/EncType identity policy. | `plans/closure/i2pcontrol-proposal-170/324-status.md` | Passed for Ed25519 type 7 and active LeaseSet2 X25519 type 4. |
| I2P-compatible Red25519 provider supports required signing, verification, and key blinding operations. | `plans/closure/i2pcontrol-proposal-170/325-status.md` | Blocked: no qualified maintained provider was found. |
| Type-5 encrypted/blinded LeaseSet2, all ten encryption modes, and per-client PSK/DH authorization have real publication and lookup owners. | Plan 326 implementation requirements; current NetDB/client publication owners | Not implemented; cryptographic prerequisite is unsatisfied. |

## Verification, compatibility, and security

This is a dependency closure only; no implementation or dependencies were added, and no Plan 326 tests were run. The branch's current local workspace floor is green but does not provide encrypted LeaseSet evidence. Ordinary Ed25519 is not treated as a substitute for Red25519. No secrets, identity formats, or network behavior changed.

Findings by severity: critical 0; high 0; medium 0; low 0. No encrypted LeaseSet or client-authorization support is claimed.

## Roadmap disposition

Plan 326 is closed as blocked solely on Plan 325. Plans 323 and 324 no longer gate it. Reopen after a separately reviewed I2P-compatible Red25519 provider passes vectors, malformed-input, and secret-handling qualification. Plan 328 remains blocked on 322, 326, and 327.

## Correction, 2026-10-05 (dated; the original text above is preserved)

The disposition above is **stale in its named dependency**, though the status token
is not changed here.

Plan 325's provider survey was superseded by the clean-room path: Plan 329
froze the provenance and normative boundary, Plan 330 landed the independent
Red25519 implementation, Plan 332 landed the type-5 ELS2 foundation with daily
blinding, lookup secret, B33, opaque NetDB store/serve and client
publish/resolve, and Plan 333 landed PSK and DH client authorization. So
requirement 4's stated cause — "the cryptographic prerequisite is unsatisfied" —
is no longer accurate as written: the cryptographic work exists, and it exists
because the forward architecture this record was blocked on was deliberately
replaced.

What remains genuinely open is narrower and is **not** closed by this correction:
Plan 325's *provider* question, the "all ten encryption modes" breadth in
requirement 4 against what 332/333 actually cover, and the control-plane
publication evidence. Re-audit Plan 326 against Plans 330/332/333 rather than
reopening it on Plan 325. Plan 335's measured type-11 transcript incompatibility
is a separate defect on the interoperability axis and is not a Plan 326
dependency.


## Correction, 2026-10-05 — the recorded blocker was superseded, and the true gate is the external transcript

Plan 344 performed the re-audit this record recommended, against the acceptance
criteria in this plan's own plan of record rather than against the three
sub-items listed above. See
[`344-status.md`](344-status.md).

**The status token above was wrong.** `blocked-prop170-encrypted-leaseset-awaiting-qualified-red25519-provider`
named Plan 325 as the gate. Plan 325's own successor note defers to Plan 331,
which **passed**; the provider question is answered by i2pr's own qualified
implementation. A reader following the token would have chased a closed question.
**The token is corrected to name the real gate**, and the original text above is
preserved rather than rewritten.

**All three sub-items listed above are now closed:**

| Sub-item | Closed by |
|---|---|
| Plan 325 provider question | Plan 331 (passed) — superseded for forward architecture |
| "all ten encryption modes" breadth | Plan 344, after finding and fixing a real gap |
| Control-plane publication evidence | Plans 337 and 338, and re-verified green in Plan 344 |

**And one real gap was found.** Of the ten canonical `EncryptLeaseSet`
spellings, `encrypted with per-user key (psk)` had **only ever passed at the
parser** — it had no ELS2 material row and no type-5 record row. This plan says
outright, *"No mode may pass from parser acceptance or inert storage"*, and the
frozen mapping's argument that the spelling is covered by its behavioural twin is
exactly the reasoning this plan forbids: the mapping is the design, not the
evidence. The spelling is now exercised in both publication-path rows, with three
client authorizations, and an inversion that breaks its mapping fails exactly
those two rows and no others.

**Plan 326 is still blocked**, and the reason is now stated precisely. This
plan's acceptance criteria require that "the encrypted-LS2 path works end-to-end
through real publication and lookup" and that there be "external
lookup/publication against at least one independent implementation before
capability claim". The type-11 outer signature is verified by the floodfill
that accepts a publication and produced by the publisher, so the external
requirement fails in **both** directions: the pinned Java I2P and i2pd reject a
type-5 record signed with the transcript the Red25519 specification mandates, and
they cannot produce one i2pr will accept.

**True remaining gate: Plan 335's external type-11 transcript divergence.** It
is a measured result, not an untested guess, and no local work removes it.

**Consequence worth stating plainly:** Plan 328, the full-conformance gate,
remains blocked on 326 and 327. Because 326's remainder is external, **Plan 328
cannot be unblocked by any purely local plan.** The gate is reachable, but not
from inside this repository alone. Plan 342 is the local half and remains
unblocked and unimplemented.
