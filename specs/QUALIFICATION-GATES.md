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


## 11. When one of the router's own tunnels stops answering

Plan 512 gives the router's **destination** inbound pool the failure detection
it did not have. Plan 487 recorded the gap: *a dead transit hop is invisible
to the router until the tunnel expires*, so a destination whose tunnels die
with a hop is not replenished until the lease expires. This section records
what the two pinned references do when a tunnel stops answering, because they
are the design's source.

### 11.1 What the router had, and what it lacked

The router already had a **pair-level** liveness scheduler (Plan 504). A probe
leaves through one tunnel and must come back through the other; the answer is
correlated on the **acknowledged** message id; and a pair that misses two
consecutive responses is reported unhealthy. That scheduler was installed and
driven, but its **population** was the exploratory role's alone — the
destination role's tunnels were never tracked.

The consequences of a failure were already live: failing a tunnel removes it
from the destination's pool, the daemon's tick notices that the installed
lease set no longer matches the pool's usable set and marks the destination
for republication, the replenishment deficit grows, and the existing submit
path builds the replacement. Only the **detection** and the **routing of the
failure** were missing.

### 11.2 i2pd 2.61.0 — the pair test

Pin `635b013a612ff47278ef02acf8580a28e10e26c5` (tag `2.61.0`).

`libi2pd/TunnelPool.cpp` runs `TestTunnels` on the pool's manage tick
(`libi2pd/TunnelPool.h:32`, `TUNNEL_POOL_MANAGE_INTERVAL = 10` seconds,
dispatched from `libi2pd/TunnelPool.cpp:462-475`).

**The pairing** (`libi2pd/TunnelPool.cpp:400-430`) shuffles the established
**outbound** list and the established **inbound** list and pairs them **one to
one**. There is **no shared-hop requirement**.

**The probe** (`libi2pd/TunnelPool.cpp:428-458`) is a garlic-wrapped
tunnel-test message, sent **out** through the **outbound** tunnel and
**addressed to the inbound tunnel's gateway**:

```cpp
outbound->SendTunnelDataMsgTo (it.second->GetNextIdentHash (),
    it.second->GetNextTunnelID (), msg);
```

The gateway forwards the message into the inbound tunnel and it comes back to
the local destination. **No remote cooperation is required** — the gateway
simply forwards, exactly as it forwards any tunnel's traffic.

**The miss** (`libi2pd/TunnelPool.cpp:342-345`) is a pending test that has not
answered when the next tick swaps the table. The **first** miss sets
`eTunnelStateTestFailed` (`libi2pd/TunnelPool.cpp:356`, `:369`;
`libi2pd/Tunnel.h:63`) and is **not** fatal. The **second** consecutive miss
sets `eTunnelStateFailed` (`libi2pd/Tunnel.h:64`) and the tunnel is **erased
from the pool** (`libi2pd/TunnelPool.cpp:357-366`, `:374-386`) — unless it is
the **last** one, in which case a replacement is created immediately
(`libi2pd/TunnelPool.cpp:360`, `:377`: *"don't fail last tunnel"*).

**The republication** is explicit: when the **inbound** tunnel fails,
`m_LocalDestination->SetLeaseSetUpdated(true)` is called
(`libi2pd/TunnelPool.cpp:389-393`, `:394`), which republishes the lease set.

**The reply** (`libi2pd/TunnelPool.cpp:493-537`, `ProcessTunnelTest`) matches
the answer and restores `eTunnelStateEstablished` from
`eTunnelStateTestFailed`.

**The local drop** (`libi2pd/TunnelPool.cpp:439-451`) blames the **outbound**
tunnel: a probe dropped locally never reached the gateway.

**What the reference does not do.** It does not attribute the failure to a
**hop**. The fault is a property of the **pair**; the router learns that the
path did not answer, not which router on it stopped.

### 11.3 Java I2P — the same shape

**Provenance caveat, stated plainly.** The pinned Java reference is **2.13.0**
(`9134f808337b401e8e53c73734c81fab04280c9d`). The reading below was taken
from a **2.13.1** installation, because the pinned artifact was not available
on the host, and the shipped jar carries **no `.java` sources** — so the
reading is **class and method from `javap`**, not `file:line`. It is labelled
as such wherever it is relied upon. The **primary** derivation is i2pd's,
whose checkout is at the exact pin.

`net.i2p.router.tunnel.pool.TestJob` is the analogue of `TestTunnels`;
`TunnelPool.tunnelFailed(TunnelInfo)` calls `fail(TunnelInfo)`; and for an
**inbound, non-exploratory** pool, `fail` calls `locked_buildNewLeaseSet()`
and then `requestLeaseSet(LeaseSet)` — the direct analogue of i2pd's
`SetLeaseSetUpdated(true)`.

### 11.4 What `i2pr` takes from them, and what it does not

`i2pr` takes the **pair-level** granularity, the **two consecutive misses**
before anything is failed, the **gateway addressing**, and the **"don't fail
last tunnel"** rule.

It does **not** take:

- **per-hop attribution.** The tree has no per-hop delivery-success signal,
  and the tunnel layer's IV and key state are end-to-end. Both references
  treat the fault as pair-level.
- **i2pd's both-sides failure.** i2pd fails the outbound and the inbound of
  the pair. `i2pr` fails the **inbound** only: the recorded gap is about the
  inbound pool and its advertised lease, the inbound's failure already
  produces both a replacement and a republication, and the outbound is the
  router's path to the remote service, whose failure is a **different** gap.
  The cost — a dead outbound produces repeated inbound rebuilds rather than
  one outbound rebuild — is recorded in the closure record.
- **any new wire mechanism.** The probe is the router's existing
  `DeliveryStatus` with the existing correlation id.

### 11.5 The safety property that is inherited, not relaxed

The router's scheduler refuses to tear down a pair that has **never**
answered: only an **authenticated** response proves a pair, and only a proven
pair may be failed. This is the reference's two-miss rule's own safety
margin, and it is the property that bounds the blast radius of the
gateway-addressed probe: a destination probe path that is systematically
broken proves nothing, and therefore **fails nothing**. The rule is
inherited by the destination population unchanged, and a static guard pins
that there is exactly one such gate in the scheduler's production code.

## 12. Finding a peer: the exploration and the RouterInfo lookup

Plan 513 gives the router a live DHT discovery spine. This section records
what the two pinned references do, because they are the design's source, and
it records where the tree's own authority differs from them.

### 12.1 What the router runs today, and what is missing

The live router runs **one** kind of NetDB lookup: `LeaseSet2`
(`crates/i2pr-daemon/src/netdb_seam.rs:362`). The **exploratory** lookup
(wire type `3`) and the **RouterInfo** lookup (wire type `2`) exist in the
source and have **no production caller**. The routing table is populated and
persisted in a live daemon, and **no live lookup consults it**
(`plans/closure/network-participation/497-status.md:342-348`, `:394-398`).

A router that cannot find a peer can only talk to the peers it was handed:
the seed file's floodfills plus unsolicited `DatabaseStore`s
(`plans/closure/network-participation/506-status.md:92-97`).

### 12.2 The dependency between the two kinds

An exploratory lookup's answer is a `DatabaseSearchReply`, and that message
carries peer **hashes**, not records:

```
pub struct DatabaseSearchReplyMessage {
    pub key: Hash,
    pub peer_hashes: Vec<Hash>,
    pub from: Hash,
}
```

(`crates/i2pr-proto/src/i2np/netdb.rs:223-230`.) A peer hash the router
cannot resolve is a **name**, not a peer: the routing table's driver says so
itself with `RoutingLookupOutcome::NeedsPeerResolution` —
*"a peer without an address cannot be asked"*
(`crates/i2pr-daemon/src/netdb_participation.rs:1689-1691`).

So the **RouterInfo-kind resolution is the enabling half**, and it is also
the half the support inventory names as deferred:
`netdb.live-routerinfo-lookup`, structure *"live-standards-conformant-routerinfo-lookup-over-exploratory-tunnels"*
(`specs/support.toml:1372-1379`).

### 12.3 The tree's own authority: Plan 102's amendment

`plans/implementation/netdb/102-amendment-exploratory-tunnel-dependency.md`
is a **closed authority amendment**:

> Current I2P RouterInfo `DatabaseLookup` operation uses an outbound
> exploratory tunnel and requests the response through an inbound
> exploratory tunnel.

> A direct `DatabaseLookup` over NTCP2 is not accepted as a substitute for
> the standard exploratory-tunnel path.

This is binding on `i2pr`, and it is why the RouterInfo-kind lookup is
composed on the exploratory pool's **outbound** role for the request and
its **inbound** role for the reply. The reply-path seam exists:
`ExploratoryPoolReplyPathProvider::provide_reply_path`
(`crates/i2pr-tunnel/src/provider.rs:41`) asks the exploratory pool for its
oldest valid **inbound** tunnel and translates it into a `ReplyPath`.

### 12.4 i2pd 2.61.0 — the exploration

Pin `635b013a612ff47278ef02acf8580a28e10e26c5` (tag `2.61.0`).

`libi2pd/NetDbRequests.cpp:459-507` — `NetDbRequests::Explore`:

- The exploratory pool's **next outbound** and **next inbound** tunnels are
  taken, and `throughTunnels` is true when both exist (`:462-465`).
- For each destination a **random 32-byte hash** is generated (`:472`) and
  the request is created as an **exploratory** request (`:473`).
- The floodfill is the **closest** one to that random hash
  (`netdb.GetClosestFloodfill(randomHash, excluded)`, `:479`).
- If the floodfill is **directly connected**, `throughTunnels` is set to
  false (`:482-483`) and the request is sent **directly**
  (`i2p::transport::transports.SendMessage(floodfill->GetIdentHash(), ...)`,
  `:499-500`). Otherwise the request is sent **through the outbound
  exploratory tunnel** (`:484-497`, delivered by
  `outbound->SendTunnelDataMsgs(msgs)` at `:505-506`), and the floodfill is
  told about the router with a `DatabaseStore` in the same tunnel (`:486-491`).
- The exploration is **scheduled**, not run on demand
  (`ScheduleExploratory`, `:509`; `HandleExploratoryTimer`, `:516`): the
  interval grows with the number of known routers (`:521-522`), and the number of
  destinations per round is bounded to between 1 and 9 (`:528-531`).

`libi2pd/NetDbRequests.cpp:255-311` — `SendNextRequest` is the **non**
exploratory path, and it applies the same rule: direct when the floodfill is
reachable or connected (`:267-269`: *"floodfill can't be reached directly"*
sets `direct = false`), sends directly (`:287`), or through the exploratory
pool's **outbound** tunnel with the reply path on its **inbound** tunnel
(`:291-303`); with no tunnels it refuses the request (`:305-310`).

**The reference's rule is uniform: direct when the floodfill is reachable,
through the exploratory tunnel otherwise.**

### 12.5 The divergence the tree records

`i2pr` sends the **exploratory** lookup **directly**. The reference's
tunnel fallback exists for **reachability** — a floodfill the router cannot
reach directly — and `i2pr`'s floodfills are reached over its established
transport seam. The exploratory lookup is therefore always direct here, and
the reference's fallback is **not** implemented.

This is recorded as a divergence with its reason. It is **not** the same
question as §12.3: Plan 102's amendment binds the **RouterInfo** kind, and
that kind is composed on the exploratory tunnels, exactly as the amendment
requires. The exploratory kind has no such authority in this repository.

### 12.6 Provenance caveat, stated plainly

The pinned Java reference is **2.13.0**
(`9134f808337b401e8e53c73734c81fab04280c9d`). The Java **router** classes
are **not installed** on this host — the local installation's jars carry no
`router/netdb` package at all — so there is **no** Java reading of the
exploration or of the RouterInfo request path to record. The **primary**
derivation is i2pd's, whose checkout is at the exact pin, and the tree's own
authority for the RouterInfo kind is Plan 102's closed amendment.

## 13. Which floodfill an exploratory request asks

### 13.1 The question

An exploratory lookup asks a floodfill for the peers closest to a **random**
search key. The random key changes every time, so the answer the router wants
changes every time too — and the *choice of floodfill* is part of the
question, not a detail of it.

Before Plan 514, `i2pr` answered that question with the flat store's
**first** floodfill advertiser. The store is a `BTreeMap` keyed by the peer's
hash, so that is the **lowest-hash** floodfill, and the answer did not depend
on the search key at all: every exploration asked the same peer.

### 13.2 i2pd 2.61.0 — the closest floodfill

At the exact pin (`635b013a612ff47278ef02acf8580a28e10e26c5`):

- `libi2pd/NetDbRequests.cpp:472` — the exploration draws a **fresh random
  hash** per destination: `RAND_bytes (randomHash, 32);`
- `libi2pd/NetDbRequests.cpp:479` — and asks the **closest** floodfill for
  that hash: `auto floodfill = netdb.GetClosestFloodfill (randomHash,
  dest->GetExcludedPeers ());`
- `libi2pd/NetDb.cpp:1308` — `GetClosestFloodfills` is the underlying
  question, over the floodfill bucket set.
- `libi2pd/NetDb.cpp:1312` — the target's **routing key** is what the search
  runs on: `IdentHash destKey = CreateRoutingKey (destination);`
- `libi2pd/NetDb.cpp:1316` — and the bucket set's closest-first search runs
  on it: `v = m_Floodfills.FindClosest (destKey, num, ...)`.

So the reference's rule is: **derive the random search key's routing key, and
ask the floodfill whose own position is closest to it.** A different random key
gives a different closest floodfill, and the reference excludes the peers it
has already asked — within that one request (`dest->GetExcludedPeers()`).

### 13.3 The reference ends the request rather than asking a substitute

`libi2pd/NetDbRequests.cpp:502-503` — when the bucket set names no
floodfill, the reference **completes the request with nothing**:

```cpp
else
    RequestComplete (randomHash, nullptr);
```

It does **not** fall back to an arbitrary peer it happens to know. `i2pr`
behaves the same way: a table that names no floodfill ends the request, the
refusal is counted under its own reason, and the request is never redirected.

### 13.4 The divergence the tree records, and the one it does not

**Not a divergence — the selection.** `i2pr` now derives the search key's
routing key with the same public `daily_routing_key` the reference's
`CreateRoutingKey` corresponds to, and asks the floodfill whose own daily
routing key is the smallest XOR distance from it. The tree's routing table
already ranked its peers that way; the request simply consults it now.

**Not a divergence — one peer per exploratory request.** The reference's
excluded set is **per request** (`libi2pd/NetDbRequests.h:50`, `:76`): it
holds the router itself when it is a floodfill
(`libi2pd/NetDbRequests.cpp:29`), the routers that answered (`:48`) and the
floodfill that was asked (`:59`), and it is cleared on reset (`:73`). The
advance to the **next closest** floodfill is gated on a destination that is
**not** exploratory (`libi2pd/NetDbRequests.cpp:261`:
`if (!dest->IsExploratory () && count < MAX_NUM_REQUEST_ATTEMPTS)`). So the
reference's **exploratory** request asks exactly **one** floodfill — the
closest for its random key — and does not advance. `i2pr`, after Plan 514,
does exactly that too.

**A divergence — the rate.** The reference asks `1..9` exploratory requests
per `27..55` seconds (`libi2pd/NetDbRequests.cpp:521-531`:
`numRouters = 800/numRouters`, clamped to `9`, with
`EXPLORATORY_REQUEST_INTERVAL = 55` at `libi2pd/NetDbRequests.h:32`).
`i2pr` asks **one** per participation tick, and the tick is
`netdb.participation_maintenance_period_ms` — `60` seconds by default. The
structure is the same; the rate is not. This is recorded as a finding for a
successor, not as a claim of conformance.

### 13.5 Provenance caveat, stated plainly

The same caveat as §12.6, and for the same reason: the pinned Java reference
is **2.13.0** (`9134f808337b401e8e53c73734c81fab04280c9d`), the Java
**router** classes are **not installed** on this host, and the local jars carry
no `router/netdb` package at all. There is therefore **no** Java reading of
how Java chooses the floodfill for an exploratory request, and none is
claimed. The **primary** derivation is i2pd's, at the exact pin.

The pinned Java reference is **2.13.0**
(`9134f808337b401e8e53c73734c81fab04280c9d`). The Java **router** classes
are **not installed** on this host — the local installation's jars carry no
`router/netdb` package at all — so there is **no** Java reading of the
exploration or of the RouterInfo request path to record. The **primary**
derivation is i2pd's, whose checkout is at the exact pin, and the tree's own
authority for the RouterInfo kind is Plan 102's closed amendment.

## 14. How often the router republishes its own RouterInfo

### 14.1 The question

A `RouterInfo` is a record with a lifetime. The network keeps it for a
while and then it stops being served, and a router whose record has aged
out cannot be found by a peer that wants to reach it — however long the
router itself has been running.

So the question is not *whether* the router publishes its record, but *how
often* it publishes it again.

### 14.2 i2pd 2.61.0 — the publication timer

At the exact pin (`635b013a612ff47278ef02acf8580a28e10e26c5`):

- `libi2pd/RouterContext.cpp:1456` — `SchedulePublish()`; the timer expires
  after `ROUTER_INFO_PUBLISH_INTERVAL + rng % ROUTER_INFO_PUBLISH_INTERVAL_VARIANCE`
  seconds (`:1461-1462`).
- `libi2pd/RouterContext.h:34` — `ROUTER_INFO_PUBLISH_INTERVAL = 39 * 60`
  seconds; `:36` — `ROUTER_INFO_PUBLISH_INTERVAL_VARIANCE = 105` seconds.
- `libi2pd/RouterContext.cpp:1470` — `HandlePublishTimer()`; it clears the
  per-cycle exclusion set (`:1477`), updates the record's timestamp, and
  publishes.
- `libi2pd/RouterContext.cpp:1492` — `Publish()`; it asks the **closest**
  floodfill that has not already been asked this cycle
  (`:1502`, derived by `libi2pd/NetDb.cpp:1296`), sends a `DatabaseStore`
  with a fresh reply token — directly when the peer is connected or
  reachable, otherwise through an exploratory tunnel — and adds the peer to
  the exclusion set.
- `libi2pd/RouterContext.cpp:1547` — `SchedulePublishResend()`;
  `libi2pd/RouterContext.h:37` — `ROUTER_INFO_CONFIRMATION_TIMEOUT = 1600`
  milliseconds; `:38` —
  `ROUTER_INFO_MAX_PUBLISH_EXCLUDED_FLOODFILLS = 15`.

### 14.3 What this tree does

`i2pr` published its own record **once**, from the service-tunnel
provisioning path, and never again. Plan 515 changes that:

- a **runtime-neutral schedule** owns the period, the due test and the
  counted outcomes. It takes `now_ms` from its caller and never reads a
  clock, which is what makes the cadence provable without a wall clock, a
  socket, or a sleep;
- a **router-level refresh** republishes the record on that period. It is
  the router's own, so it does not depend on a service tunnel being
  provisioned;
- the refresh and the one-shot publication select through the **same
  version-gated function** — the coordinator over its own store, where it
  leaves a DSM receipt, and the refresh over the router-wide bootstrap's
  store — so Plan 503's API-version floor holds for every publication. They
  share **one** store construction and **one** delivery path;
- the fan-out is bounded at the closest three floodfills, and every outcome
  is counted by reason.

### 14.4 The divergence this tree records

The reference's resend burst is **not** adopted. The reference asks **one**
floodfill and then, every `1600` milliseconds, the next closest ones — up to
fifteen. This tree asks the closest three in **one** pass, once per period.
One pass is not weaker in reach — it asks three peers immediately where the
reference asks one and then waits — but it does not re-ask on failure, and
it asks three where the reference may reach fifteen. The reference's
`105` seconds of variance is also not adopted: a deterministic period is
testable, and the variance exists to de-synchronise a floodfill with the
network, which a router that is not a floodfill has no need for.

### 14.5 Provenance caveat, stated plainly

The pinned i2pd checkout is at the exact pin and the derivation above is
from that checkout. The pinned Java reference is **2.13.0**
(`9134f808337b401e8e53c73734c81fab04280c9d`); the Java **router** classes are
**not installed** on this host — the local installation's jars carry no
`router/netdb` package at all — so there is **no** Java reading of the
publication timer to record. The **primary** derivation is i2pd's.

## 15. The publication's acknowledgement (Plan 516)

### 15.1 The correlation token must reach the wire

A `DatabaseStore` whose reply token is zero asks the peer for **no**
acknowledgement. The correlator's token must therefore be the token the
store message carries, and it must never be zero.

*Gate:* `crates/i2pr-netdb/src/publication.rs`, `begin_attempt`, carries the
minted token; `crates/i2pr-daemon/tests/router_info_publication_confirmation.rs`,
`the_publication_carries_the_correlators_minted_token`.

### 15.2 The reply form must be the reference's direct reply

A **zero** reply tunnel id means "send the `DeliveryStatus` straight back
to me" (`libi2pd/I2NPProtocol.cpp:285`), and the reply gateway is the local
hash. A tunnelled reply would need an inbound tunnel the router may not
hold.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_confirmation.rs`,
`the_store_uses_the_reference_s_direct_reply_form`.

### 15.3 The key, the gateway and the payload must be one record

A `DatabaseStore` whose key is the hash of one record and whose payload is
another stores a record under the wrong key.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_confirmation.rs`,
`the_store_keys_our_own_hash_and_carries_our_own_record`.

### 15.4 There must be one store-message builder

The daemon composes **no** store message of its own. The one-shot
publication and the router-level refresh share the correlator's builder and
its attempt book.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_confirmation_boundary.rs`,
`the_daemon_composes_no_store_message` and
`the_one_shot_goes_through_the_correlator`.

### 15.5 The inbound owner must join the publication

The acknowledgement arrives on the SSU2 service's inbound path. It must be
routed to the correlator, and an answer that matches no live attempt must
be **refused** and counted under its own reason.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_confirmation_boundary.rs`,
`the_inbound_owner_joins_the_publication` and
`an_unmatched_acknowledgement_is_refused`.

### 15.6 A delivery is not an acknowledgement

The two are counted separately, and a delivery admission alone never yields
a confirmation.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_confirmation.rs`,
`a_delivery_admission_is_not_an_acknowledgement` and
`the_acknowledgement_is_counted_separately_from_the_publication`.

### 15.7 What this section does **not** gate

The **stock-peer** half. That a pinned stock floodfill answers a
publication is still blocked on Plan 493's authorized two-host topology, and
no live claim is made. Plan 515's finding **F1** — the reference's resend
burst — is still open and is a successor's subject.
## 16. How the router retries a publication the network did not acknowledge (Plan 517)

### 16.1 The retry must ride the loop's existing tick

The confirmation deadline is the reference's `ROUTER_INFO_CONFIRMATION_TIMEOUT`
(`libi2pd/RouterContext.h:37`). It is observed on the SSU2 service's existing
`100` millisecond tick, so the retry must be driven from that tick and must not
arm a timer of its own.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`,
`the_retry_rides_the_existing_tick`.

### 16.2 The deadline must be the reference's constant

The deadline is `1600` milliseconds, measured from the moment the batch went
out. A different timeout would be a different protocol.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`,
`the_deadline_is_the_reference_constant`.

### 16.3 The exclusion set must reach the selection

A target the cycle has already asked must never be asked twice, so the
exclusion set has to reach the **selection** -- and the selection has to keep
going through the version gate, so Plan 503's API-version floor holds for
every target of every batch.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`,
`the_exclusion_set_reaches_the_selection`.

### 16.4 The cycle must be bounded

The cycle's bound is the reference's
`ROUTER_INFO_MAX_PUBLISH_EXCLUDED_FLOODFILLS` (`libi2pd/RouterContext.h:38`),
and the fan-out stays bounded at the tree's three.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`,
`the_bound_is_the_reference_constant` and `the_fan_out_stays_bounded`.

### 16.5 A confirmation must end the cycle

The reference clears its exclusion set and schedules the next publish at the
full interval (`libi2pd/RouterContext.cpp:1321-1328`). A confirmed publication
is not retried.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`,
`a_confirmation_ends_the_cycle`.

### 16.6 The cycle must not be able to get stuck

A cycle that is running with nothing to await would never advance, and it
would hold the next period's cycle out. The publication must therefore
**halt** the cycle whenever it cannot proceed, and **abandon** a refused
batch at once.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`,
`the_cycle_cannot_get_stuck`.

### 16.7 The cycle must never read a clock

The cycle takes the reading from its caller, so the whole ladder is provable
without a wall clock, a socket or a sleep.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`,
`the_cycle_never_reads_a_clock`.

### 16.8 What this section does **not** gate

The **stock-peer** half. That a pinned stock floodfill answers a publication,
and that the retry therefore reaches one, is still blocked on Plan 493's
authorized two-host topology, and no live claim is made.
## 17. How the router's own record stays current (Plan 518)

### 17.1 The re-dating must happen once per cycle

The reference dates the record in `HandlePublishTimer`
(`libi2pd/RouterContext.cpp:1474`), once per cycle, and its resend rungs publish
that same record. The re-dating must therefore happen at the cycle's start and
**not** per rung.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_re_dating_happens_once_per_cycle`.

### 17.2 The publication must publish the re-dated record

The publication tick must not read the installed record directly: it must
publish the record the cycle re-dated, or the freshness the cycle established
is lost at the last step.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_publication_uses_the_re_dated_record`.

### 17.3 The staleness rule must use the netdb's own window

The window is the netdb's `DEFAULT_MAX_AGE`, not a second constant: the refusal
must fire exactly when a peer would refuse the record, so the two may not drift
apart.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_staleness_rule_uses_the_netdb_window`.

### 17.4 A stale record must be refused, never published

A record the network would refuse wastes a delivery and hides the fault, so it
is refused and counted under its own reason.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`a_stale_record_is_refused`.

### 17.5 The re-dating must not log a secret

The re-dating's diagnostics carry a reason only. No key, no signature, and not
the record's bytes.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_re_dating_logs_no_secret`.

### 17.6 The proto's re-encode must preserve the content

The signed region is re-encoded from the record's own parts, so the addresses,
the peers and the options cannot drift.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_proto_preserves_the_content`.

### 17.7 The crypto's re-sign must verify its own result

A caller must never publish a record the crypto layer could not prove.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_crypto_verifies_its_result`.

### 17.8 The publication module must stay runtime-neutral

The staleness rule takes the reading from its caller and never reads a clock,
and the module holds no key.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_module_stays_runtime_neutral`.

### 17.9 The rungs must publish the cycle's record

A rung must not read a copy of the record back out of the runtime. The
cycle's record lives in the owner, so the record is decided once, at the
cycle's start, and the acknowledgement the correlator matches names the record
the rung published.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`,
`the_rungs_publish_the_cycle_record` and `the_cycle_record_is_the_owners`.

### 17.10 What this section does **not** gate

The **stock-peer** half. That a pinned stock floodfill accepts the re-dated
record in the live network is still blocked on Plan 493's authorized two-host
topology, and no live claim is made.

## 18. How the publication leaves the transport's loop (Plan 519)

The router's own RouterInfo publication used to run inside the SSU2 service's
`select!` loop, where its delivery awaited a session establishment. While that
await was pending the loop could not drain its relay queue or correlate an
inbound `DeliveryStatus`. Neither pinned reference blocks its transport's loop
on a handshake: i2pd 2.61.0 posts the send and ignores the returned future
(`libi2pd/Transports.cpp:483-496`), and Java I2P 2.13.0 runs the publication as
a job on its own pool (`PublishLocalRouterInfoJob`, `JobQueue.java:163`).

### 18.1 The transport's service loop never awaits the publication

The publication's work -- the refresh, the retry ladder and the acknowledgement
correlation -- is confined to the publication's own task. Nothing else in the
composition root may name it.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`,
`the_loop_never_awaits_the_publication`.

### 18.2 The publication's state is not owned by the loop

The loop binds no publication owner, and the owner is constructed **once** and
handed to the task: there is exactly one owner of the cycle.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`,
`the_publications_state_is_not_owned_by_the_loop`.

### 18.3 The task is owned and cancellable

The task is spawned under the service's own child scope -- never on an
ownerless task -- in a moved closure that owns the publication's state, and it
stops when its cancellation fires.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`,
`the_task_is_spawned_with_ownership_and_cancellation`.

### 18.4 The acknowledgement handoff is bounded

The channel is bounded, its capacity is bounded, and the hand-over never waits
for room: the transport's service loop is never blocked by the publication. A
full channel counts the acknowledgement as **unmatched** rather than dropping
it silently.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`,
`the_acknowledgement_handoff_is_bounded`, and
`crates/i2pr-daemon/tests/router_info_publication_off_loop.rs`,
`a_full_handoff_counts_the_acknowledgement_as_unmatched` and
`the_handoff_never_blocks_the_loop`.

### 18.5 The selection prefers an established session

i2pd's `IsConnected` and Java's `isEstablished` both prefer a peer the router
is already connected to, because that is the peer whose send needs no
handshake. The preference is applied **on top of** the version-gated closeness
selection, and the targets it skips for want of one are counted under their
own reason.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`,
`the_selection_prefers_an_established_session`, and
`crates/i2pr-daemon/tests/router_info_publication_off_loop.rs`,
`the_selection_prefers_an_established_session` and
`the_skipped_targets_are_counted_under_their_own_reason`.

### 18.6 The delivery timeout is bounded and the publication's own

The reference bounds one store attempt at 90 seconds
(`FloodfillNetworkDatabaseFacade.java:174`). A delivery that did not finish
inside the publication's own window is refused under **its own reason** and
counted, so a hung handshake holds neither the transport's loop nor the
publication's task.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`,
`the_delivery_timeout_is_bounded_and_owned`, and
`crates/i2pr-daemon/tests/router_info_publication_off_loop.rs`,
`the_delivery_timeout_is_bounded_and_is_the_references_own` and
`a_delivery_that_timed_out_is_refused_under_its_own_reason`.

### 18.7 The transport's loop carries no publication timer

The publication's period tick and the ladder's tick belong to the publication's
task, so the transport's loop gains no timer.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`,
`the_loop_carries_no_publication_timer`.

### 18.8 What this section does **not** gate

The **stock-peer** half, and the **live-dial** half. That a pinned stock
floodfill accepts the router's record in the live network is still blocked on
Plan 493's authorized two-host topology, and the lane drives the publication's
public behaviour without a socket, so a *live* dial inside the transport's loop
is not re-measured here. The structural gates above are what stand between the
publication and the transport's loop, and they read the production files.

## 20. The router's own peer test (Plan 521)

The router publishes its own RouterInfo, and Plan 520 made the SSU2 address
in that record **qualified** — published only while the router's own
reachability evidence admits the direct form. That evidence needs **three**
distinct corroboration classes, and Plan 520 closed with the finding that
the ordinary router could earn only two: the confirmed peer test was
recorded in production only by the controlled floodfill activation.

Plan 521 closes that gap at its source. The mechanism is the I2NP peer test,
message **231**, which the daemon already modelled and the dispatcher
already classified; the transport's separate out-of-session three-party
peer test is untouched and stays controlled-only.

### 20.1 The opt-in is real, and it is off by default

`ssu2.peer_test` is parsed, defaults to `false`, and is refused unless the
SSU2 service itself runs. With the switch off the router sends no probe and
records nothing.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_opt_in_defaults_off_and_needs_the_service`, and
`crates/i2pr-daemon/tests/peer_test_role.rs`, `the_opt_in_is_off_by_default`
and `the_opt_in_requires_the_ssu2_service`.

### 20.2 The probe is recorded before the send

The probe is recorded in the outstanding table **before** it is returned to
the caller, so an answer that arrives quickly is never dropped.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_probe_is_recorded_before_the_send`.

### 20.3 The recorder has one production call site

The runtime's production recorder is called from exactly **one** place in
the daemon: the confirmed-match path. The controlled recorder is a separate
method and is untouched.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_recorder_has_one_production_call_site` and
`the_controlled_recorder_is_untouched`.

### 20.4 The daemon asserts no family and no reachability state

The runtime derives the address family from the session the exchange
travelled on. The daemon never names a family, never names a reachability
signal, and never owns a tracker.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_daemon_asserts_no_family` and
`the_daemon_records_no_reachability_signal`.

### 20.5 The responder derives the answer from the authenticated delivery

The answer carries the authenticated peer and the exact link the test
arrived on, and the responder runs **no** second decoder: the live inbound
path decodes the message once and classifies it once, and the peer-test
seam reads that same classified outcome.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_responder_uses_the_authenticated_delivery` and
`the_inbound_path_decodes_once`.

### 20.6 The owner adds no task, no timer and no listener

The owner is driven by the product's **existing** tick and fed by the live
inbound path. It starts nothing and binds nothing.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_owner_adds_no_infrastructure`.

### 20.7 The reply traffic is bounded by the router

A bounded table of answered keys means one key is answered at most once per
age window, and the table refuses rather than evicting.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_answered_key_table_is_bounded`, and
`crates/i2pr-daemon/tests/peer_test_role.rs`,
`a_repeated_key_is_answered_at_most_once`.

### 20.8 The transport's policy is untouched, and the publication too

The daemon names no corroboration threshold and no class, and the peer test
touches no builder and no permit: `caps=f` stays permit-gated.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`,
`the_corroboration_policy_is_untouched` and `the_publication_is_untouched`.

### 20.9 The confirmed exchange is the third class

The confirmed peer test supplies the class the router could not earn. With
the configured bind and the peer's external-address observation already
present, the transport's tracker reaches `Reachable` only once the peer test
is recorded.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role.rs`,
`the_transport_s_tracker_reaches_reachable_with_the_confirmed_peer_test` and
`a_confirmed_exchange_records_the_router_s_own_reachability`.

### 20.10 The stock-peer half is not claimed

No stock router implements an I2NP 231 responder at the router level. The
stock half stays blocked on Plan 493's authorized two-host topology, and
the lane drives the router-level responder against this router's own peer
over a **real** SSU2 session on loopback.

*Gate:* `crates/i2pr-daemon/tests/peer_test_role.rs`,
`the_responder_answers_with_the_probed_fields_unchanged`.

## 19. The router's own published SSU2 address (Plan 520)

The router publishes its own RouterInfo, and that record has carried one
SSU2 address since the tree began. Plan 520 qualifies it: the published
address carries the family capability and the transport's MTU, so the netdb's
own `is_qualified_ssu2_address` accepts it, and it is published **only**
while the router's own reachability evidence admits the direct form.

### 19.1 The opt-in is real, and it is off by default

`ssu2.advertise` is parsed, defaults to `false`, is refused unless the SSU2
service itself runs, and is refused outside the controlled loopback profile.
With the switch off the installed record stands byte-identical.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_switch_is_not_refused_at_parse_time`, and
`crates/i2pr-daemon/tests/router_info_publication_address.rs`,
`the_opt_in_is_off_by_default` and
`the_router_publishes_no_address_when_the_opt_in_is_off`.

### 19.2 The decision has one owner

The daemon holds no copy of the direct-versus-firewalled rule: the owner asks
the runtime for the material, never names a reachability state, and never
names the transport's builder.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_daemon_holds_no_copy_of_the_publication_rule` and
`the_owner_never_builds_an_address`.

### 19.3 The netdb's own rule qualifies the published address

The netdb's addressed builder validates the address with the netdb's **own**
qualification rule and takes **no** floodfill permit, so it cannot mint
`caps=f`. The floodfill builders still require the permit.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_addressed_builder_qualifies_without_a_permit` and
`the_floodfill_builders_still_require_the_permit`, and
`crates/i2pr-daemon/tests/router_info_publication_address.rs`,
`the_direct_form_is_published_when_the_evidence_admits_it` and
`the_published_record_carries_no_f_capability`.

### 19.4 The family capability is the family letter

The runtime's material carries the family capability, and the transport
writes the `caps` option only when the capability set is non-empty — an
empty set would produce an address with **no** `caps` option, which the
netdb's own qualification refuses.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_family_capability_is_carried`.

### 19.5 The published record is the installed record

The owner installs the record, and the publication's cycle re-dates and
publishes it **after** the owner has refreshed it, so every rung of the cycle
publishes the same bytes. The re-dating preserves the address.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_record_is_installed_and_published` and
`the_redating_preserves_the_address`, and
`crates/i2pr-daemon/tests/router_info_publication_address.rs`,
`the_redated_record_still_carries_the_address`.

### 19.6 The expiry withdraws the address

When the transport admits no address, the published record returns to the
netdb's ordinary zero-address form. The runtime's install gate binds every
SSU2 address the record carries to the router's own endpoint and keys, and
refuses two conflicting ones.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_expiry_withdraws_the_address` and `the_install_gate_binds_every_address`,
and `crates/i2pr-daemon/tests/router_info_publication_address.rs`,
`the_address_is_withdrawn_when_the_evidence_lapses`.

### 19.7 The daemon records no evidence of its own

The runtime owns the reachability evidence; the daemon owns the opt-in and
the install. A signal the daemon does not genuinely hold is never recorded.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_daemon_records_no_reachability_signal`.

### 19.8 The owner adds no infrastructure

The owner starts no task, no timer and no listener, and the configuration
gains no new key beside `advertise`.

*Gate:* `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs`,
`the_owner_adds_no_infrastructure`.

### 19.9 What this gate does **not** claim

It does not claim that a **stock** peer qualifies the published record in the
live network: that half needs Plan 493's authorized two-host topology and
remains blocked on it. The direct form needs three distinct corroboration
classes, and there is **no production entry point** that starts a peer test
outside the controlled floodfill activation, so the ordinary record's address
cannot be earned in production today. Nothing is advertised: the inventory's
advertised count stays zero, `router.version` stays `0.9.62`, and the
published record's `caps` string is unchanged.

## 21. The outbound peer-session pool and the outbound keepalive (Plan 522)

The router used to dial per need and hold nothing between needs, so the
publication's ordinary path was the handshake path. Plan 522 gives the
router a **bounded pool** of outbound peer sessions, and the transport an
**outbound-only** keepalive that holds a pooled session open.

### 21.1 The opt-in is real, and it is off by default

`ssu2.peer_pool` is parsed, defaults to `false`, is refused unless the SSU2
service itself runs, and is refused when the session's idle timeout cannot
hold the keepalive's unanswered deadline.

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_an_opt_in_that_defaults_on`, and
`crates/i2pr-daemon/tests/peer_pool.rs`,
`the_opt_in_is_off_by_default`, `the_opt_in_requires_the_ssu2_service` and
`the_opt_in_is_refused_when_the_idle_timeout_cannot_hold_the_keepalive`.

### 21.2 The owner adds no infrastructure

The pool's owner holds no session, no socket, no task and no timer. The dial
is awaited on the pool's own task, and the bootstrap's lock is released
before any dial.

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_pool_that_spawns_a_task` and
`the_guard_reports_a_pool_that_binds_a_socket`.

### 21.3 There is one dial path

The pool dials through the one existing narrow seam and names no other dial
entry point, and the publication's delivery path is unchanged.

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_pool_that_dials_the_transport_directly` and
`the_guard_reports_a_publication_that_names_the_pool`.

### 21.4 The pool writes no peer profile

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_pool_that_writes_a_peer_profile`.

### 21.5 The ceilings refuse rather than evict

The population is bounded by one publication cycle's worth of peers and the
per-period dial budget is two. A candidate beyond either bound is refused and
counted under its own reason, and a peer is never evicted.

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_pool_that_evicts_a_peer` and
`the_guard_reports_a_pool_with_no_dial_budget`, and
`crates/i2pr-daemon/tests/peer_pool.rs`,
`the_pool_refuses_a_candidate_beyond_its_ceiling` and
`the_pool_never_exceeds_its_ceiling`.

### 21.6 The keepalive carries no I2NP message and holds one at a time

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_keepalive_that_carries_an_i2np_message` and
`the_guard_reports_a_session_that_keeps_two_keepalives`.

### 21.7 The keepalive respects the congestion gate and the payload budget

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_keepalive_that_bypasses_the_congestion_gate`.

### 21.8 The keepalive is outbound only

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_keepalive_on_an_inbound_session`.

### 21.9 The keepalive is off unless the pool is on

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_keepalive_that_is_on_without_the_pool`, and
`crates/i2pr-daemon/tests/peer_pool.rs`,
`the_keepalive_reaches_the_runtime_only_when_the_pool_is_on`.

### 21.10 The pool's task runs only under the opt-in

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_pool_task_that_runs_without_the_opt_in`.

### 21.11 A rule whose named item is absent fails closed

*Gate:* `crates/i2pr-daemon/tests/peer_pool_boundary.rs`,
`the_guard_reports_a_missing_named_item`.

### 21.12 The pool establishes a real session

The lane drives two daemon-owned SSU2 services on loopback and shows the pool
establishing a **real** session to a peer it knows.

*Gate:* `crates/i2pr-daemon/tests/peer_pool.rs`,
`the_pool_establishes_a_session_to_a_peer_it_knows`.

### 21.13 Every refusal is counted by reason

*Gate:* `crates/i2pr-daemon/tests/peer_pool.rs`,
`the_pool_counts_every_dial_outcome_under_its_own_reason`,
`the_pool_counts_a_peer_with_no_dialable_address`,
`the_pool_refuses_a_candidate_the_version_gate_refuses` and
`a_cancelled_dial_releases_the_peer`.

**What this gate does not open.** The **stock-peer** half stays blocked on
Plan 493's authorized two-host topology, and the **dial scope** limit — the
transport's own dial target accepts a loopback endpoint or an RFC1918 IPv4
address, so a public deployment cannot use the pool today — is a transport
boundary with its own plan-of-record. Nothing is advertised.

## 22. The SSU2 introducer address form (Plan 523)

Plan 523 corrects the **form** of the SSU2 introducer set — the peers that
will introduce this router to others — to the specification's own. The
gates below are the questions a reviewer asks of the form.

| Gate | Question | How it is answered |
|---|---|---|
| G22.1 | Does an SSU2 address carry the specification's introducer group? | The transport's parser reads `ih[N]` (the introducer's router hash, I2P base64, 32 bytes), `iexp[N]` (the expiry, optional) and `itag[N]` (the nonzero relay tag). |
| G22.2 | Are the SSU 1 introducer options refused? | `ihost[N]`, `iport[N]` and `ikey[N]` are **refused** as unknown options in an SSU2 address, never ignored. The reference says they "are for SSU only and are not used for SSU2". |
| G22.3 | Is an endpoint refused alongside introducers? | The parser returns the typed `HostPortWithIntroducers`, and the netdb's introducer rule refuses a `host` or a `port`. The specification forbids publishing them. |
| G22.4 | Is the netdb's rule exclusive with the direct rule? | `is_qualified_ssu2_address` still demands a host and a port; `is_qualified_ssu2_introducer_address` forbids them. Neither rule is weakened by the other. |
| G22.5 | Can the introducer builder advertise the floodfill role? | No: it takes no floodfill permit, so it cannot produce `caps=f`. |
| G22.6 | Does the plan add wire vocabulary? | No: the proto crate carries no introducer option name, and the guard proves it. |

**What the gates do not cover.** The **composition** — asking a peer for a
relay tag, recording an introducer, publishing the router's own introducer
set, and advertising the `C` capability — is a successor's subject. The
**stock-peer** half needs Plan 493's authorized two-host topology, and a
published introducer set that does not exist yet.
