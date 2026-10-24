# SSU2 private-LAN qualification procedure

This procedure exercises the daemon-owned SSU2 socket and its signed local
RouterInfo inside an operator-authorized private subnet, against an
exact-pinned stock peer.

It does **not** qualify public reachability, production peer selection,
RouterInfo publication, or `R`/`f` capability claims. Plan 431 remains the
authority for independent two-host interoperation; Plan 493 remains the
authority for the two-family participation matrix.

## 0. The one thing to understand before you run anything

**A qualification lane has two independent sources of failure: our
implementation, and the peer's own admission and selection state.** A lane
that cannot tell them apart is not a gate — it is a coin flip that
occasionally lands on a false conclusion about `i2pr`.

This matters far more in an isolated topology than on the live network:

- On the live network a stock router has thousands of alternative peers. One
  bad interaction is noise; it retries elsewhere and forgets.
- In the lab the peer has **no alternatives**. Our router is effectively the
  only candidate, so one bad interaction *is* the whole topology. The peer's
  own state — its profile store, its selection filters, a ban, a cooldown —
  can dominate the outcome completely.
- Consequently **an isolated-topology failure is not evidence of a
  live-network failure**, and an isolated-topology success is not evidence
  of public reachability.

This procedure therefore separates the two failure sources *before* the
protocol attempt, names the peer's own exclusion reasons when it declines
us, and treats the peer's local state as a first-class cause. Sections 2, 4
and 5 exist for that reason and are not optional paperwork.

The governing rule is already canonical: `plans/README.md`'s *Bounded
execution and causal corrective policy*, rule 2 — *"Preflight source-owned
prerequisites before process launch. Inspect pinned reference call graph and
selection filters"* — and rule 1, which requires independent claim tiers so
that *"earlier lower-tier successes remain valid evidence; later failure
cannot erase them."*

## 1. Prepare the two-host inventory

Obtain authorization from both host owners for one isolated RFC1918 subnet,
one bounded attempt, and a timeout no longer than five minutes. Assign distinct
`i2pr` and pinned-i2pd roles, each with an interface-local IPv4 address, prefix,
unused UDP port, distinct host identifier, and a new private data directory
outside the repository. Keep the inventory outside the repository with mode
`0600`; the preflight refuses repository paths and group/world-readable files.

Use the schema and exact reference revision accepted by
`python3 scripts/plan449-ssu2-preflight.py --help`. Run the preflight on each
host with the same inventory. It checks the assigned local address, interface,
route-selected source address, UDP bind availability, distinct endpoints,
data-directory properties, and the unmodified i2pd 2.61.0 pin. It sends no
datagram and does not start either router. Do not share host addresses or
identity-bearing logs in repository evidence.

## 2. Peer-eligibility preflight — before any process launches

This stage is **offline**. It reads the pinned peer's own source and state to
decide whether the peer *could* select us, before we spend an attempt. It must
not open a socket to the peer and must not start either router.

### 2.1 The admission gates our RouterInfo must clear

The pinned i2pd 2.61.0 peer enforces ingest gates on any RouterInfo it
receives. They are recorded in `crates/i2pr-runtime/tests/ssu2_independent.rs:227`:

- `router.version` digits must clear the peer's **minimum-allowed floor**.
- A `netId` property matching the peer's network (mainnet `2`) must be
  present.

The second gate fails **silently**: a missing `netId` marks the RouterInfo
unreachable *without logging*, and the peer's NetDB refuses to persist it. A
silent failure here looks exactly like "the peer ignored us", and it must
never be reported as an `i2pr` defect.

### 2.2 The eligibility predicates — why a stock peer may decline to dial us

Admission is not selection. Even with an admissible RouterInfo, a stock peer
may have no reason to dial us:

- **i2pd.** The peer learns our RouterInfo **in-band**, during the session we
  initiate to it (`crates/i2pr-runtime/tests/ssu2_independent.rs:847`), and
  then dials back only if we are a target of its exploration. In the
  controlled lane that target is produced by advertising the **floodfill**
  capability — `caps = "fO"`, documented at
  `crates/i2pr-runtime/tests/ssu2_independent.rs:41` as the *"direction-B
  trigger"*. A peer that we did not initiate to, and that we do not advertise
  as floodfill, is simply not dialled.
- **Java I2P 2.13.0.** The predicate is stricter still.
  `ProfileManagerImpl.heardAbout()` — the only route to a profile — is gated
  on the peer RouterInfo carrying `R` **plus** floodfill, bandwidth and
  congestion conditions (`plans/closure/mixed-router-interop/201-status.md:661`).
  Selection then additionally requires `selectAllPeers` /
  `getProfileNonblocking` membership, `not_failing_count > 0`, and a peer that
  is selectable, non-banned and non-floodfill
  (`plans/closure/mixed-router-interop/230-status.md:149`).

That Java path has already produced exactly the false failure this procedure
exists to prevent. Plan 230's attempt 3b recorded
`P230-D-ELIGIBLE-BUT-NO-PROFILE` — *"eligible 3/3 on this SHA; 30 s D poll
exhausted with count=1, C absent"*
(`plans/closure/mixed-router-interop/230-status.md:168`). The peer was fully
eligible by the capability predicate, never created a profile for us, never
selected us, and the poll timed out with the peer absent. Read naively, that
says "the peer does not implement this". It actually says "the peer's
selection state never admitted us".

### 2.3 What the preflight must record

For each family, record which predicates our lab RouterInfo satisfies, which
it deliberately does not, and why:

| Predicate | Where it comes from | If we fail it |
| --- | --- | --- |
| RouterInfo signature valid | the peer's ingest path | `peer-admission-refused` |
| `router.version` above the peer's floor | peer ingest gate | `peer-admission-refused` |
| `netId` present and matching | peer ingest gate (silent) | `peer-admission-refused` |
| advertised endpoint equals the bound socket | `Plan 453` signed-RI path | `peer-admission-refused` |
| selection target (floodfill cap for i2pd) | peer exploration filters | `peer-selection-refused` |
| `R` present (Java only) | Java `heardAbout` predicate | `peer-selection-refused` |
| bandwidth/congestion class accepted | Java class gate | `peer-selection-refused` |
| peer profile store is fresh | peer state, §5 | `peer-state-poisoned` |

A family that cannot satisfy a predicate is **not** a failed attempt. It is
`environment-unavailable` or `peer-selection-refused`, and it must not be
reported as an `i2pr` result in either direction.

## 3. The lab RouterInfo profile

One profile, stated once, so that both hosts and the evidence agree. Every
field is a decision, not a default.

| Field | Lab value | Why |
| --- | --- | --- |
| `netId` | `2` | peer ingest gate; missing ⇒ silently unreachable, never persisted |
| `router.version` | at or above the pinned peer's minimum-allowed floor | peer ingest gate |
| `caps` | floodfill (`f`), plus the bits the family under test requires | makes us an exploration target so the peer dials back |
| `R` | per family: **required** by Java's `heardAbout` predicate | Java selection predicate |
| bandwidth / congestion class | a class the peer accepts (Plan 230's C2) | Java selection predicate |
| published address | the **actual** bound RFC1918 endpoint and port | honest reachability inside the lab |

**`R` is never forged.** In an authorized lab where we genuinely bind and are
genuinely reachable at that address, `R` is a true statement about the lab.
The same bit on a public bind would be a forgery, which `GUARDRAILS.md` and
`specs/CONFORMANCE.md` forbid outright. The profile is therefore scoped to
the lab profile and never to a public or ordinary daemon configuration.

**Reachability is never inferred from configuration.** `R` is set only when
the bound socket is real and the peer has confirmed the path.

## 4. Run only the authorized private profile

On the i2pr host, use a temporary config with:

```toml
[ssu2]
enabled = true
qualification_profile = "private_lan_qualification"
bind_ipv4 = "<this-host RFC1918 address>"
bind_ipv6 = ""
port = <this-host's assigned UDP port>
advertise = false
introducer_service = false
```

Use the inventory's dedicated data directory. The profile rejects wildcard,
non-RFC1918, nonlocal, IPv6, missing/privileged/ephemeral-port, and disabled
binds. The daemon signs the endpoint reported by the bound socket before
readiness and preserves the router and transport identities in that data
directory. Verify the resulting local RouterInfo only through the daemon's
sanitized local inspection path; distribute its encoded bytes to the other
host only through the separately authorized lab's private provisioning method.
Do not publish it to public NetDB.

Run the exact-pinned i2pd peer with its own inventory data directory and
private endpoint. **The peer's configuration is part of the test and must be
recorded**, because it changes the peer's behaviour:

- the controlled transport lane runs the peer with `notransit = true`,
  `floodfill = false` and `bandwidth = L`
  (`tests/integration/ssu2/run-independent.sh:74-77`);
- a **transit** leg requires a transit-capable peer. `notransit = true`
  cannot satisfy it, and changing it changes the peer's profile of us and its
  ban surface. The peer configuration for a transit leg is a different test
  and gets its own attempt and its own evidence.

## 5. Peer-state hygiene — before and after every attempt

The peer's local state is the most common cause of a misleading result.

**Before each attempt:**

- use a **fresh peer data directory**. Stale `netDb/r*/*.dat` files carry
  stale peer profiles, and the peer redials previously learned RouterInfos —
  a pending outbound dial to our bind port can then swallow our inbound
  handshake by endpoint routing (`plans/closure/ssu2/161-status.md:242`);
- use a **fresh peer process**;
- never present a mismatched static key on the live port. The pinned peer
  treats a static-key mismatch on a published address as a misbehaving peer
  and **bans the whole address for ~30–39 minutes**, which breaks every later
  run until restart (`plans/closure/ssu2/161-status.md:247`);
- ensure the advertised endpoint equals the actually bound socket.

**After a failure:**

- a `peer-state-poisoned` outcome names the peer's cooldown and **does not
  consume the attempt budget**. A cooldown expiring is a material change in
  the peer's state, so it licenses a controlled rerun — it is not an
  "unchanged-topology retry". This is the canonical rule: an *identified
  cause*, including reference fixture and environment mistakes, permits a
  correction and a rerun inside the still-active plan, and no rule fixes a
  global attempt count of one (`plans/README.md`, bounded-execution rule 3).
- a `peer-selection-refused` outcome records the peer's own exclusion reason,
  so the next attempt can fix it rather than re-run blind.

## 6. Claim tiers

Record each tier independently. **A failure at a higher tier never retracts a
lower-tier pass.**

| Tier | What it proves |
| --- | --- |
| T0 | the peer admitted and persisted our signed RouterInfo |
| T1 | the peer's own selection state admitted us as a dial target |
| T2 | an authenticated transport session was established |
| T3 | an I2NP message travelled in one direction |
| T4 | the peer answered a request we originated (correlated two-way reply) |
| T5 | the peer **initiated a fresh session to us** |
| T6 | our daemon's own product behaviour changed (a tunnel, a published record, a client session) |
| T7 | a tunnel was built |
| T8 | external SAM/Streaming acceptance |

T5 is the tier that proves the peer can *choose* us, and it is the tier the
controlled SSU2 lane already demonstrates: *"The pinned peer learned our
RouterInfo in-band during direction A... wait for it to initiate a fresh
session: token/source validation, Retry, and promotion all run through the
normal responder path (no test bypass)"*
(`crates/i2pr-runtime/tests/ssu2_independent.rs:847`).

Each tier records `pass`, `fail`, or `not-attempted` with a reason. A run that
reaches T4 and then fails at T5 is a **T4 pass with a T5 failure**, and the
evidence must say exactly that.

## 7. Outcome taxonomy

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

## 8. The deliberate-withholding control

Before trusting any result from this lane, run it once with the selection
predicate **deliberately withheld** — our lab RouterInfo advertises no
floodfill capability.

The lane must then report `peer-selection-refused` **with the peer's own
reason**, and must **not** report `i2pr-defect` and must **not** report a
pass.

This control is the gate's own self-test. It proves the lane can distinguish
"the peer would not choose us" from "our implementation is broken" — the
distinction the whole procedure exists to preserve. If the withholding run
reports `i2pr-defect`, the lane's classification is wrong and no result from
it may be cited.

## 9. Collect the evidence

Collect only sanitized stage/count evidence: the tier outcomes, the counts per
tier, the outcome classification, the peer configuration used, and digests. A
successful local bind or signed RouterInfo alone is not an interoperation
result.

Never retain raw RouterInfos, addresses, keys, identities, or peer logs.
Never promote a lab result into a public-reachability, `R`, `f`, introducer,
or support-inventory claim.

## 10. Tear down

Stop both processes, confirm their owned listeners have closed, and retain only
the sanitized evidence required by the plan. Remove the temporary config and
inventory according to the host owners' retention rules. Before any ordinary
daemon run, set `qualification_profile = "controlled_loopback"` (or remove the
key), disable SSU2 unless explicitly needed for a separate controlled test,
and use the normal data directory. The private profile never authorizes
publication, reachability, public support claims, or floodfill capability.

## 11. What this procedure does not settle

- **A one-family result is progression evidence, not conformance evidence.**
  `specs/CONFORMANCE.md` item 7 requires at least two independent
  implementation families for a router-to-router claim, and Java I2P and
  I2P+ count as one. A private-LAN pass against i2pd alone satisfies none of
  the two-family claim; Plan 493 owns that.
- **A private-LAN pass does not establish public reachability.** It is
  controlled non-loopback evidence and nothing more.
- **The reference routers are never patched, vendored, or reconfigured to
  accommodate `i2pr`** beyond the lab profile recorded in §4. A peer
  configuration that is required to make the lane pass is itself a finding
  about the lane's design.
