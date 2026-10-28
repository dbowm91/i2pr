# External qualification gates: the peer-eligibility layer

This document is the canonical statement of the **external qualification
gate**: what must be known, before any protocol attempt, about whether the
**stock peer** could admit and select `i2pr`, and how a lane must classify a
failure when it happens.

The operator-facing procedure lives in
[`docs/operations/ssu2-private-lan-qualification.md`](../docs/operations/ssu2-private-lan-qualification.md).
This document is the *model*; that one is the *procedure*. Where they differ,
this document governs the model.

The checker that enforces this model is
[`scripts/plan495-peer-eligibility.py`](../scripts/plan495-peer-eligibility.py).
Its `--self-test` is part of the repository's routine floor.

**This document claims no interoperability and no capability.** It says how a
lane must be shaped so that its result means something.

---

## 1. Why the peer is a second source of failure

A qualification lane runs against a **stock peer that has its own admission
and selection state**. That is an independent source of failure, and a gate
that cannot separate the two is not a gate — it is a coin flip that
occasionally lands on a false conclusion about `i2pr`.

This matters far more in an isolated topology than on the live network:

- On the live network a stock router has thousands of alternative peers. One
  bad interaction is noise; it retries elsewhere and forgets.
- In the lab the peer has **no alternatives**. Our router may be the only
  candidate, so one bad interaction *is* the whole topology. The peer's own
  state — its profile store, its selection filters, a ban, a cooldown — can
  dominate the outcome completely.
- Consequently **an isolated-topology failure is not evidence of a
  live-network failure**, and an isolated-topology success is not evidence of
  public reachability.

The asymmetry is not only practical, it is **source-visible**. The pinned
i2pd files a newly learned floodfill into its floodfill set only while it
holds fewer than 2000 floodfills, or when the peer's profile for that router
is already *real*
(`libi2pd/NetDb.cpp:333-344`, `libi2pd/NetDb.hpp:45`). In an isolated lab
i2pd holds zero floodfills, so **our router is filed immediately**; on the
live network the same router is filed only after it has actually been
talked to. A lab lane is therefore *more* permissive than the live network,
and a lab pass is not a live-network statement.

### 1.1 What the failure owner means

Every outcome names **which side owns the failure**, and ownership is a
separate axis from the outcome class:

| Axis | Values | Meaning |
| --- | --- | --- |
| Outcome class | the taxonomy in §5 | *where* the run ended |
| Cause | `i2pr-lab-profile`, `i2pr-implementation`, `peer-state`, `intentional-withholding`, `unattributed` | *whose artifact* was responsible |

Without the cause axis a lab-profile defect would be filed as a peer-side
condition — the exact misreading this gate exists to prevent. A missing
`netId` is refused by the *peer*, but the artifact at fault is *ours*.

The two `i2pr-…` causes are both ours, and the distinction matters:

- **`i2pr-lab-profile`** — the operator-facing lab RouterInfo profile the
  preflight evaluates is wrong. Fix the profile. The preflight can say this
  because it evaluates the profile *before* the attempt.
- **`i2pr-implementation`** — the daemon's own code path is at fault. This is
  the only cause that may accompany `i2pr-defect`.

A peer-side outcome class may still carry an `i2pr-…` cause: the peer
refused, and *our* artifact was why. It may never carry
`i2pr-implementation` — that would be a misfiled implementation defect.

---

## 2. How the predicates were derived

Every predicate in §3 is derived from the **pinned reference's own source**,
and each carries that source's file and line. The pins are frozen:

| Family | Reference | Pin |
| --- | --- | --- |
| `i2pd` | i2pd 2.61.0 | `635b013a612ff47278ef02acf8580a28e10e26c5` |
| `java-i2p` | Java I2P 2.13.0 | `9134f808337b401e8e53c73734c81fab04280c9d` |

The derivation is repeatable: clone the pinned source, read the cited
location, and check the predicate against it. The repository never patches,
vendors or re-tunes a reference to make a lane pass; a peer configuration
that a lane *requires* is itself a finding about the lane's design, and is
recorded as such.

The checker encodes the derived predicates. It does **not** re-derive them at
run time, and it never opens a socket: a gate that could reach the peer
could not be falsified by an offline fixture. What the checker guarantees is
that the encoded predicate set is applied faithfully, that every predicate
reports its state, and that an unknown predicate refuses a lane-ready
verdict.

---

## 3. The predicates

A predicate has one of three kinds:

- **admission** — the peer's ingest gate: will it accept and persist our
  RouterInfo at all? All admission predicates are required.
- **dial** — will the peer establish a transport session to us at all? All
  dial predicates are required.
- **explore** — will the peer's *own selection state* choose us as a target?
  These are grouped into named **routes**. A peer may reach us by more than
  one route; **at least one route must be available** for the profile to be
  lane-ready.

Each predicate's state is one of `satisfied`, `unsatisfied`,
`deliberately-unsatisfied` or `unknown`. An `unknown` predicate **refuses a
lane-ready verdict**: the checker never guesses, and it never converts a
peer's silence into a verdict about `i2pr`.

### 3.1 i2pd 2.61.0

**Admission — the ingest gates.**

| Predicate | Requirement | Source |
| --- | --- | --- |
| `i2pd-ri-signature` | The RouterInfo signature verifies. | `libi2pd/RouterInfo.cpp`, `RouterInfo::VerifySignature` |
| `i2pd-ri-netid-present` | A `netId` property is present. | `libi2pd/RouterInfo.cpp:479-486,502` (`isNetId`, `SetUnreachable`) |
| `i2pd-ri-netid-matches` | The `netId` equals the peer's network id. | `libi2pd/RouterInfo.cpp:482-487` |
| `i2pd-ri-version-present` | A `router.version` property is present and non-zero. | `libi2pd/RouterInfo.cpp:464-476,502` |
| `i2pd-ri-transport-present` | At least one transport the peer supports is published. | `libi2pd/RouterInfo.cpp:502` |
| `i2pd-ri-family-signature` | A published family signature verifies. | `libi2pd/RouterInfo.cpp:495-504` |

The `netId` gate fails **silently**: a missing `netId` marks the RouterInfo
unreachable *without logging*, and the peer's NetDB refuses to persist it
(`libi2pd/RouterInfo.cpp:502`). A silent failure here looks exactly like
"the peer ignored us", and it must never be reported as an `i2pr` defect.

**Dial — will the peer open a session to us?**

| Predicate | Requirement | Source |
| --- | --- | --- |
| `i2pd-not-unreachable` | The peer does not consider our RouterInfo unreachable. | `libi2pd/Transports.cpp:535` (`r->IsUnreachable()`) |
| `i2pd-dial-version-floor` | `router.version` clears the minimum-allowed floor (**0.9.58**), **or** we declare high bandwidth. | `libi2pd/Transports.cpp:536`, `libi2pd/NetDb.hpp:58` |
| `i2pd-reachable-from-peer` | We publish a non-introducer address on a transport the peer supports. | `libi2pd/RouterInfo.h:275` (`IsReachableFrom`), `libi2pd/Transports.cpp:535` |

A static-key mismatch on the published address makes the peer ban the whole
address; the ban list is cleaned up on a 66 s + 0–20 s cycle
(`libi2pd/Transports.h:131-132`), and a ban is consulted before a peer is
created (`libi2pd/Transports.cpp:526`). The ban is a **peer-state** condition
and is covered by §6, not by a RouterInfo predicate.

**Explore — will the peer's selection state target us?**

Route `floodfill-exploration`:

| Predicate | Requirement | Source |
| --- | --- | --- |
| `i2pd-floodfill-declared` | Our caps declare the floodfill capability (`f`). | `libi2pd/RouterInfo.cpp:451-455` (`CAPS_FLAG_FLOODFILL`) |
| `i2pd-floodfill-eligible` | `router.version` is at or above **0.9.62**, a published (or reachable-then-published) IPv4 address is offered, and the signing key is not DSA. | `libi2pd/RouterInfo.cpp` `IsEligibleFloodfill`, `libi2pd/NetDb.hpp:55` (`NETDB_MIN_FLOODFILL_VERSION = 0.9.62`), `libi2pd/NetDb.cpp:333-344` |

Route `ssu2-peer-test`:

| Predicate | Requirement | Source |
| --- | --- | --- |
| `i2pd-peer-test-cap` | We publish the SSU2 testing address cap (`B`) and `router.version` is at or above **0.9.62**. | `libi2pd/RouterInfo.cpp:468-475` (i2pd strips `eSSUTesting` below the floor), `libi2pd/NetDb.hpp:57`, `libi2pd/NetDb.cpp:1179-1188` (`GetRandomSSU2PeerTestRouter`) |

> **The floodfill cap alone is not enough.** i2pd files us as a floodfill
> only when `router.version >= 0.9.62` **and** a published IPv4 address is
> offered. A RouterInfo that advertises `f` below that floor is *never*
> filed, so an exploratory request can never target it. The peer then falls
> back to the generic dial path, which needs only the dial predicates.
> Operators must read "advertise floodfill" as "advertise floodfill **and**
> clear the floodfill floor".

### 3.2 Java I2P 2.13.0

**Admission — the ingest gates.**

| Predicate | Requirement | Source |
| --- | --- | --- |
| `java-ri-signature` | The RouterInfo signature verifies. | `router/java/src/net/i2p/data/router/RouterInfo.java` |
| `java-ri-netid` | The `netId` property is present and equals the peer's network id. | `router/java/src/net/i2p/data/router/RouterInfo.java:85` (`PROP_NETWORK_ID`) |
| `java-ri-version-present` | A `router.version` property is present. | `router/java/src/net/i2p/data/router/RouterInfo.java` |
| `java-ri-transport-present` | At least one transport the peer supports is published. | `router/java/src/net/i2p/router/transport/` |
| `java-ban-surface` | Our `(caps, version)` pair does not land on one of the peer's ban triggers. | `router/java/src/net/i2p/router/transport/ntcp/InboundEstablishState.java:797-822` |

**The ban surface is checked offline precisely because it is expensive.** The
peer's NTCP2 inbound-establish path bans a RouterInfo whose caps are exactly
`LU` at version `0.9.56`, or that carry `L` at version `0.9.57`, or that
carry `G`, `X` and `f` at version `0.9.67`. The cooldown is 30 to 120
minutes, which is long enough to make **every later attempt in the same lane
fail**. The trigger is keyed on an *exact* version string, so a RouterInfo at
`0.9.62` is unaffected.

**Explore — will the peer's selection state target us?**

Route `java-selection`:

| Predicate | Requirement | Source |
| --- | --- | --- |
| `java-profile-creation` | The peer's profile-creation predicate accepts our caps: `R` present, and either `f` present, or (no `L` — unless the local router is neither floodfill-enabled nor at 128 KiB/s of share bandwidth) with no `E` and no `G`. | `router/java/src/net/i2p/router/peermanager/ProfileManagerImpl.java:431-441` (`shouldCreate`) |
| `java-not-excluded` | The peer's exclusion rules do not exclude us: no excluded cap letter (`K`, `E`, `G`), caps that say more than reachability, a non-DSA signing key, `ECIES_X25519`, and `router.version` at or above **0.9.62**. | `router/java/src/net/i2p/router/tunnel/pool/TunnelPeerSelector.java:347-403` (`shouldExclude`), `:45-50` (exclude caps), `router/java/src/net/i2p/router/Router.java:1128-1148` (capability letters) |

`shouldCreate` is the **only route to a profile**. Without a profile the
peer never selects the router, which is the exact state recorded as
`P230-D-ELIGIBLE-BUT-NO-PROFILE` — eligible by the capability predicate,
never profiled, never selected, poll exhausted.

> **`R` is required, and it is not free.** Java's `shouldCreate` requires
> the `R` capability, so a RouterInfo without `R` never gets a profile at
> all. In the authorized lab `R` is a *true* statement — the socket really is
> bound and really is reachable at the advertised endpoint — so it is set
> under the lab profile and **only** under the lab profile. It is never
> forged, and it is never set on a public bind.

---

## 4. The lab RouterInfo profile

One profile, stated once, so that both hosts and the evidence agree. Every
field is a decision, not a default. The checker holds this table too, and
`--profile-drift` fails when the two disagree.

| Field | Value | Reason | Family |
| --- | --- | --- | --- |
| `netId` | `2` | Peer ingest gate; a missing netId marks the RouterInfo unreachable without logging and the peer's NetDB refuses to persist it. | i2pd, java-i2p |
| `router.version` | `0.9.62` | Clears the peer's dial floor (0.9.58), its floodfill-eligibility floor (0.9.62) and its peer-test floor (0.9.62). | i2pd, java-i2p |
| `caps` | `fOR` | 'f' makes us a floodfill exploration target, 'R' is required by Java's profile-creation predicate, 'O' declares high bandwidth so the dial floor is met even below 0.9.58. | i2pd, java-i2p |
| `address caps` | `46` | Publishes the IPv4 (and IPv6) SSU2 address rather than an introducer-only entry. | i2pd, java-i2p |
| `signing key` | `ed25519` | Java excludes DSA peers from selection; i2pd excludes DSA from floodfill eligibility. | i2pd, java-i2p |
| `encryption` | `ECIES_X25519` | Java's peer-exclusion rule requires ECIES_X25519. | java-i2p |
| `published address` | `the bound RFC1918 endpoint` | An honest statement about the lab: the socket really is bound and really is reachable there. | i2pd, java-i2p |

**Reachability is never inferred from configuration.** `R` is set only when
the bound socket is real and the peer has confirmed the path.

**The profile is scoped to the lab.** It never authorizes publication,
public reachability, a support-inventory claim, or a floodfill capability
claim outside the authorized subnet.

---

## 5. The claim ladder and the outcome taxonomy

### 5.1 The tier ladder

Record each tier independently. **A failure at a higher tier never retracts a
lower-tier pass.**

| Tier | What it proves | Predicate layer |
| --- | --- | --- |
| T0 | the peer admitted and persisted our signed RouterInfo | admission |
| T1 | the peer's own selection state admitted us as a dial target | explore |
| T2 | an authenticated transport session was established | dial |
| T3 | an I2NP message travelled in one direction | — |
| T4 | the peer answered a request we originated (correlated two-way reply) | — |
| T5 | the peer **initiated a fresh session to us** | — |
| T6 | our daemon's own product behaviour changed (a tunnel, a published record, a client session) | — |
| T7 | a tunnel was built | — |
| T8 | external SAM/Streaming acceptance | — |

Each tier records `pass`, `fail` or `not-attempted` **with a reason**. A run
that reaches T4 and then fails at T5 is a **T4 pass with a T5 failure**, and
the evidence must say exactly that.

### 5.2 The outcome taxonomy

Every lane run ends in exactly one of these. The taxonomy is what stops a
peer-side condition from being read as an `i2pr` verdict.

| Outcome | Meaning | Counts against the attempt budget? | Says anything about `i2pr`? |
| --- | --- | --- | --- |
| `pass` | the tier ladder completed to its target | — | yes |
| `i2pr-defect` | the failure is in code we own, attributable to a stage we can name | yes | yes — negatively |
| `peer-admission-refused` | the peer refused our RouterInfo (signature, version floor, `netId`, address) | yes | no |
| `peer-selection-refused` | the peer admitted us but its selection state declined to dial us | yes | **no** |
| `peer-state-poisoned` | ban, cooldown, or stale profile store suppressed legitimate traffic | **no** — a material change licenses a rerun | no |
| `environment-unavailable` | missing host, authorization, route, port, clock, or pinned binary | no | no |
| `timeout` | a bounded stage deadline elapsed with no attributable cause | yes | no |

`peer-selection-refused` is a **result about the peer's selection state**, not
about `i2pr`. It must never be reported as an `i2pr` failure, and it must
never be reported as a pass.

### 5.3 The evidence shape

A lane's evidence is one JSON document. The checker validates it.

```json
{
  "schema": "i2pr-plan495-lane-evidence-v1",
  "family": "i2pd",
  "target_tier": 4,
  "outcome": "pass",
  "cause": null,
  "tiers": [
    {"tier": 0, "state": "pass", "reason": "ri-persisted"},
    {"tier": 1, "state": "pass", "reason": "selected"},
    {"tier": 2, "state": "pass", "reason": "session-established"},
    {"tier": 3, "state": "pass", "reason": "one-way-i2np"},
    {"tier": 4, "state": "pass", "reason": "two-way-reply"}
  ],
  "attempts": [
    {"outcome": "peer-state-poisoned", "cause": "peer-state", "consumes_budget": false, "cooldown_seconds": 2100},
    {"outcome": "pass", "cause": null, "consumes_budget": true}
  ],
  "attempts_consumed": 1
}
```

The validator rejects evidence that:

- omits a tier below a tier that was reached;
- records two outcome classes for one run, or an outcome outside the
  taxonomy;
- reports `pass` without the tier that earned it;
- reports `i2pr-defect` without the cause `i2pr-implementation` and a named
  failing tier;
- reports a peer-side outcome with a cause outside
  `i2pr-lab-profile`, `peer-state`, `intentional-withholding` or
  `unattributed`;
- reports `peer-state-poisoned` without a cooldown, or lets it consume the
  attempt budget.

---

## 6. The peer-state hygiene protocol

The peer's local state is the most common cause of a misleading result, so
it is a **first-class precondition**, checked before and after every attempt.

**Before each attempt:**

- use a **fresh peer data directory**. Stale `netDb/r*/*.dat` files carry
  stale peer profiles, and the peer redials previously learned RouterInfos —
  a pending outbound dial to our bind port can then swallow our inbound
  handshake by endpoint routing;
- use a **fresh peer process**;
- never present a mismatched static key on the live port. A static-key
  mismatch on a published address makes the peer ban the whole address for
  **~30–39 minutes**, which breaks every later run until restart;
- ensure the advertised endpoint equals the actually bound socket.

The hygiene record is one JSON document, and the checker validates it:

```json
{
  "schema": "i2pr-plan495-peer-state-v1",
  "peer_data_dir_fresh": true,
  "peer_process_fresh": true,
  "no_stale_netdb_files": true,
  "stale_netdb_file_count": 0,
  "static_key_matches_bound_port": true,
  "advertised_endpoint_matches_bound_socket": true
}
```

Any false condition — or a non-zero stale-file count — is a
**`peer-state-poisoned`** outcome naming the conditions that failed.

**After a failure:**

- a `peer-state-poisoned` outcome names the peer's cooldown and **does not
  consume the attempt budget**. A cooldown expiring is a material change in
  the peer's state, so it licenses a controlled rerun — it is not an
  "unchanged-topology retry".
- a `peer-selection-refused` outcome records the peer's own exclusion reason,
  so the next attempt can fix it rather than re-run blind.

---

## 7. The deliberate-withholding control

Before trusting any result from a lane, run it once with the selection
predicate **deliberately withheld** — for example, a lab RouterInfo that
advertises no floodfill capability.

The lane must then report `peer-selection-refused` **with the peer's own
reason**, and must **not** report `i2pr-defect` and must **not** report a
pass.

This control is the gate's own self-test. It proves the lane can distinguish
"the peer would not choose us" from "our implementation is broken" — the
distinction the whole gate exists to preserve. If the withholding run reports
`i2pr-defect`, the lane's classification is wrong and no result from it may
be cited.

In the checker, the withholding is expressed in the lab profile itself:

```json
{"withheld": ["i2pd-floodfill-declared"]}
```

and the checker's `--withholding` mode asserts that the resulting verdict is
`peer-selection-refused` with the cause `intentional-withholding`, and
neither `i2pr-defect` nor a pass.

---

## 8. What the checker guarantees

`scripts/plan495-peer-eligibility.py` is **pure**: it opens no socket, starts
no process and touches no network. This is not a promise, it is a checked
property — `audit_purity()` walks the checker's own AST and rejects any
import of a networking or process module and any process-spawning call, and
`--self-test` carries the inverted probes that prove the audit fires.

The checker:

1. evaluates every predicate of the candidate profile's family and reports
   each one's state, its requirement and the source it was derived from;
2. refuses a lane-ready verdict when any predicate is `unknown`;
3. classifies a verdict in the gate's own vocabulary, so a lane never has to
   invent a wording for "the peer would not have picked us";
4. validates an evidence document against the tier ladder and the taxonomy;
5. validates a peer-state hygiene record;
6. runs the deliberate-withholding control;
7. compares the checked lab profile with the one documented in §4.

**No raw reference log, RouterInfo, address, key or identity reaches an
evidence file.** The checker's output is states, counts and digests.

---

## 9. What this model does not settle

- **A one-family result is progression evidence, not conformance evidence.**
  At least two independent implementation families are required for a
  router-to-router claim, and Java I2P and I2P+ count as one family.
- **A private-LAN pass does not establish public reachability.** It is
  controlled non-loopback evidence and nothing more.
- **The reference routers are never patched, vendored, or reconfigured to
  accommodate `i2pr`** beyond the recorded lab profile.
- **A predicate the source does not determine is `unknown`, not `satisfied`.**
  The gate would rather refuse a verdict than guess one.

## 10. The API-version component ceiling

This section records what the two pinned references do with a published
`router.version` that carries **more components than the specification's
API-version table has ever used**. The router's own comparator refuses such
a version, so the divergence is recorded here with the evidence behind it,
rather than left implicit.

### 10.1 What the specification's table says, and what the router holds

The specification's API-version table contains a four-component row
(`0.6.1.10`). `i2pr` holds the table's **minimum-peer obligation** rows as
data (`i2pr_proto::MINIMUM_PEER_OBLIGATIONS`, six rows, every one of them
three components); it does **not** hold the table's other rows. The
four-component row the comparator's bound depends on was therefore held only
as prose, and Plan 511 made it a **held record**
(`i2pr_proto::SPECIFICATION_WIDEST_VERSION_ROW`) with the comparator's
component ceiling **derived** from it
(`i2pr_proto::MAX_API_VERSION_COMPONENTS`).

### 10.2 i2pd 2.61.0 — no component count

Pin `635b013a612ff47278ef02acf8580a28e10e26c5` (tag `2.61.0`).

`libi2pd/RouterInfo.cpp` reads the `router.version` property by collapsing
every digit into an `int`:

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

The separators are simply not digits and are skipped, so there is **no
component count** in the parse. `libi2pd/version.h:18` defines

```cpp
#define MAKE_VERSION_NUMBER(a,b,c) ((a*100+b)*100+c)
```

so the floor `0.9.62` is the integer `962`
(`libi2pd/NetDb.hpp:54-58` names `NETDB_MIN_HIGHBANDWIDTH_VERSION`,
`NETDB_MIN_FLOODFILL_VERSION` and `NETDB_MIN_SHORT_TUNNEL_BUILD_VERSION`
from it). `"0.9.62.1.2"` therefore collapses to `96212`, which is **above**
`962`.

**i2pd admits a five-component version and treats it as above the floor.**

Note also what i2pd's model is *not*: it is **not** a component-wise
comparison. It cannot distinguish `0.9.6.2` from `0.9.62` — both collapse to
`962`. That is why the two references do not agree with each other on how a
longer version compares, and why `i2pr` follows the specification's table
rather than either oracle.

### 10.3 Java I2P — no component count either

`net.i2p.util.VersionComparator` walks both strings, extracts the components
separated by `.`, `-` or `_`, parses each with `parseLong` — a **`long`**,
not an `int` — and compares them left to right. The loop runs until **both**
strings are exhausted: the implementation has **no component count cap**.
When one side runs out of components and the other still has one, the
exhausted side is **less** — the rule `i2pr`'s comparator already implements,
and the rule the specification states.

**Java admits a five-component version and orders it above a
three-component version with the same prefix.**

**Provenance caveat, stated plainly.** The pinned Java reference is
**2.13.0** (`9134f808337b401e8e53c73734c81fab04280c9d`). The reading above
was taken from a **2.13.1** installation, because the pinned artifact was not
available on the host. It **corroborates** the behaviour this repository
already records in `scripts/plan495-peer-eligibility.py` — a dotted version
split into integer components, with no count cap — but it is **not** a
reading of the pinned 2.13.0 artifact, and it is labelled as such wherever
it is relied upon. The **primary** derivation is i2pd's, whose checkout is
at the exact pin.

### 10.4 The divergence, and why it is safe

`i2pr` is the **only** one of the three that refuses a version with more
than four components. The refusal is:

- **typed** — `PublishedVersion::Uncomparable` maps to
  `VersionEligibility::UncomparableVersion`, a distinct reason and not a
  generic failure;
- **counted** — `VersionRefusalCounts::record` keeps a separate
  `uncomparable_version` counter, and `counts_by_reason` publishes it under
  the stable label `uncomparable-version`;
- **fail-closed** — the peer is excluded, never admitted;
- **monotone** — a version with more components can only ever be **above** a
  shorter version with the same prefix, so the refusal can only **exclude** a
  peer both references would admit. It can never admit a peer below a floor.

### 10.5 The per-component width, recorded but not settled

The router's per-component bound is a `u16` (≤ 65535). Java parses components
as `long`s and would accept a larger one; i2pd's collapse is an `int`. A
version with a component above 65535 is therefore refused by `i2pr` and
admitted by Java. That is a **second, separate divergence**. It is **not**
settled here: widening the width would multiply the comparator's stored array
again, and a version with a component above 65535 is not a version. It is
recorded for a successor plan.

