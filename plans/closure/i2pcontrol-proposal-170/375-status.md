# Plan 375 — Stock Java I2P bidirectional ELS2 qualification: status

Status: **blocked-reference-freeze-partial-live-els2-driver-not-written**

Plan of record:
[`375-java-live-els2-qualification.md`](../../implementation/i2pcontrol-proposal-170/375-java-live-els2-qualification.md).

Classification: external interoperability + capability evidence.

This plan **did not pass**. No ELS2 direction was executed against Java I2P.
Its freeze was partially completed; its source proof is explicitly incomplete.

## What was completed

| Freeze item (plan §Reference freeze) | Status | Result |
|---|---|---|
| the pin builds unmodified | pass | `:core:jar` + `:router:jar` at `9134f808…`; `BUILD SUCCESSFUL`, 7 tasks |
| record version and dependency versions | partial | 2.13.0; Gradle 8.5; `sourceCompatibility`/`targetCompatibility` = 17; JDK 21 required |
| re-read the exact ELS2/type-5 consumer/publisher/config surfaces | **incomplete** | see below |
| record the current i2pr SHA and Cargo.lock hash | pass | `75fd691cfe0e0473ce79f89fa16d66a5e7453649`; `Cargo.lock` SHA-256 `ab1963730134…b9004a` |
| ADR 0032 (Proposal 170) transcript policy unchanged | pass | untouched |
| controlled topology with no public-network dependency | verified available | the Plan 303/306 mesh passes on this host (see Plan 374's status) |

Durable record: [`tests/integration/els2/reference-freeze.md`](../../../tests/integration/els2/reference-freeze.md),
shared with Plan 374.

Build notes worth keeping: **JDK 21 is mandatory** — the host default JDK 25
makes Gradle 8.5 fail with `Unsupported class file major version 69`. `--offline`
also fails, because the `me.champeau.jmh` plugin must be resolved from the
plugin portal.

## What exists at the pin, and what does not

Present in `core/build/libs/i2p.jar`:

- `net/i2p/data/EncryptedLeaseSet.class`
- `net/i2p/crypto/eddsa/RedDSAEngine.class`
- the ordinary `LeaseSet2` / `MetaLeaseSet` / `LeaseSet` surfaces

**Not yet read, and this is the gap:** the ELS2 **authorization-mode vocabulary**
at this pin, and the **b33 consumer path**. Plan 375's capability matrix cannot
be sized until those are read, because the matrix size depends on how many modes
Java implements — exactly the question Plan 374's freeze answered for i2pd and
which materially changed that plan's scope.

Recording this as incomplete rather than inferring it from the class list is
deliberate. i2pd turned out to implement **all three** modes despite the
evidence available before reading the source; assuming the same for Java without
reading it would repeat the mistake in the other direction.

## The blocker

Same named gap as Plan 374, and it is the same one:

The repository has no ELS2 live driver. It has the Plan 303/306 controlled
floodfill mesh (type 1/3/7 only), Plan 350/351's i2pr-internal type-5 path, and
Plan 346's crypto-boundary transcript check. None is a live ELS2 row, and the
plan forbids satisfying one by inserting a decoded LeaseSet into a consumer, by
sharing an in-process NetDB, by invoking the resolver with test bytes, or by
modifying Java I2P.

The Java lane additionally needs its own source-level handling of Java's
tunnel-peering gate — `TunnelPeerSelector.shouldExclude` caps arity and
`allowAsIBGW` imposes an `R` requirement. Plan 373 corrected Plan 347's
diagnosis on this point: neither Java row requires Java to peer with i2pr,
because the queried-floodfill topology of Plans 303/306 already reaches i2pr as a
floodfill. ADR 0030 stands untouched and no tier letter is needed. That
correction removes what looked like a policy blocker, but not the driver work.

## Findings by severity

- **critical / high: none.** No product defect was found; no product code was
  changed.
- **medium, planning**: Plan 375's matrix size is still unsized, because its
  source proof is incomplete. Sizing it before the proof would repeat Plan 374's
  initial assumption and then have to be redone.
- **low (recorded, not fixed here)**: `scripts/interop/fetch-ssu2-reference.sh`
  cannot fetch the Java reference at its pin — it clones a default branch and
  fails its own pin check (`expected 9134f808…`, `got a629ec7c…`), so a
  correctly pinned override is required. The i2pd path supports an override and
  works. Interop-tooling change, outside this plan's scope.

## What is explicitly **not** claimed

- No ELS2 interoperability with Java I2P, in either direction.
- No live type-5 `DatabaseStore`, blinded-key `DatabaseLookup`, decrypt, inner
  LS2 validation, or application payload across routers.
- No bandwidth-tier claim, and no ADR 0030 change.
- Type 5 remains `advertised = false`. `specs/support.toml` is unchanged.

Plan 346's crypto-boundary cross-verification against executed Java signature
output remains valid and remains **not** interoperability.

## Roadmap disposition and unblock audit

- **Roadmap disposition: blocked**, on a named work gap plus an incomplete
  source proof.
- **Unblock audit, executed per `plans/README.md`:**

| Plan | Other hard dependencies | All closed? | Disposition |
|---|---|---|---|
| 377 | Plan 374 **not passed**, Plan 375 **not passed** | no | stays blocked |
| 378 | Plan 376 passed; Plan 377 not passed | no | stays blocked on 377 alone |

No corrective pass is registered: no defect was found.

## Limitations

- The presence of `EncryptedLeaseSet` and `RedDSAEngine` is a class-listing
  fact, not a behavioural one. It says the types exist at the pin; it says
  nothing about which authorization modes are reachable.
- No claim is made about Java behaviour under load, at scale, or against the
  public network.