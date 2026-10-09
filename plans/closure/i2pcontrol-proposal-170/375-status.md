# Plan 375 — Stock Java I2P bidirectional ELS2 qualification: status

Status: **active-java-source-proof-complete-live-els2-driver-not-written**

Plan of record:
[`375-java-live-els2-qualification.md`](../../implementation/i2pcontrol-proposal-170/375-java-live-els2-qualification.md).

Classification: external interoperability + capability evidence.

This plan has not passed. No ELS2 direction has been executed against Java I2P.
The 2026-10-09 continuation completed the pinned-source audit for publisher,
consumer, and authorization modes and rebuilt the unmodified Java pin with JDK
21. The live Java driver and matrix remain outstanding. This open status does
not close Plan 375.

## What was completed

| Freeze item (plan §Reference freeze) | Status | Result |
|---|---|---|
| the pin builds unmodified | pass | `:core:jar` + `:router:jar` at `9134f808…`; `BUILD SUCCESSFUL`, 7 tasks |
| record version and dependency versions | partial | 2.13.0; Gradle 8.5; `sourceCompatibility`/`targetCompatibility` = 17; JDK 21 required |
| re-read the exact ELS2/type-5 consumer/publisher/config surfaces | pass (source only) | NONE/DH/PSK and blinded lookup confirmed at exact pin; see freeze §3.5 |
| record the current i2pr SHA and Cargo.lock hash | pass | `d616f0868db4e56bd311d74095347209d69f5aa0`; SHA-256 `5389da3fa5dcd74e13d4c6421c3a2079c98b4e9ee117a8b575ca9c80e4a92bce` |
| ADR 0032 (Proposal 170) transcript policy unchanged | pass | untouched |
| controlled topology with no public-network dependency | verified available | the Plan 303/306 mesh passes on this host (see Plan 374's status) |

Durable record: [`tests/integration/els2/reference-freeze.md`](../../../tests/integration/els2/reference-freeze.md),
shared with Plan 374.

Build notes worth keeping: **JDK 21 is mandatory** — the host default JDK 25
makes Gradle 8.5 fail with `Unsupported class file major version 69`. `--offline`
also fails, because the `me.champeau.jmh` plugin must be resolved from the
plugin portal.

## Pinned-source result (2026-10-09)

Present in `core/build/libs/i2p.jar`:

- `net/i2p/data/EncryptedLeaseSet.class`
- `net/i2p/crypto/eddsa/RedDSAEngine.class`
- the ordinary `LeaseSet2` / `MetaLeaseSet` / `LeaseSet` surfaces

`BlindData` defines `AUTH_NONE=0`, `AUTH_DH=1`, and `AUTH_PSK=3`. The stock
I2PTunnel surface maps selectors 0, 1, and 2 to none, DH, and PSK. `LookupDestJob`
decodes extended Base32 B33 bodies and derives the blinded lookup key. Stock
SAM preserves I2CP type-5/auth options. These are source-feasibility facts only;
they size the live matrix as NONE, PSK, DH, plus separate lookup-secret and
authorized/wrong/missing-credential cases. See the pinned-source citations in
`tests/integration/els2/reference-freeze.md` §3.5.

## The blocker

The Java-specific gap is the missing live driver and its evidence artifact:

The repository has Plan 279's controlled Java floodfill topology, an i2pr ELS2
consumer driver for i2pd, Plan 350/351's i2pr type-5 path, and Plan 346's
crypto-boundary transcript check. None executes either Java ELS2 direction.
The Plan-279 topology is only a substrate; it has no Java ELS2 publisher/client
driver or Java-to-i2pr application proof. Rows cannot be satisfied by inserting
a decoded LeaseSet, sharing an in-process NetDB, invoking the resolver with
test bytes, or modifying Java I2P.

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
- **medium, implementation**: the Java source matrix is now sized, but the live
  driver has not been implemented or executed. No Java result can be imported
  into Plan 377.
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

- **Roadmap disposition: active**, source proof complete; live driver and rows
  outstanding.
- **Unblock audit, executed per `plans/README.md`:**

| Plan | Other hard dependencies | All closed? | Disposition |
|---|---|---|---|
| 377 | Plan 374 delivered by Plan 406; Plan 375 **not passed** | no | stays blocked on Plan 375 |
| 378 | Plan 376 passed; Plan 377 not passed | no | stays blocked on 377 alone |

No corrective pass is registered: no defect was found.

## Limitations

- Source branches for the modes and lookup path do not prove their runtime
  interoperability or application behavior.
- No claim is made about Java behaviour under load, at scale, or against the
  public network.
