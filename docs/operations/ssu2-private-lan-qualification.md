# SSU2 private-LAN qualification procedure

This procedure exercises the daemon-owned SSU2 socket and its signed local
RouterInfo inside an operator-authorized private subnet, against an
exact-pinned stock peer.

It does **not** qualify public reachability, production peer selection,
RouterInfo publication, or `R`/`f` capability claims. Plan 431 remains the
authority for independent two-host interoperation; Plan 493 remains the
authority for the two-family participation matrix.

**The live two-host lane has not been executed.** The guarded per-host
executor in §9 and its offline simulator are complete and source-verified,
but no authorized two-host pair has run. This procedure is infrastructure
for a lane that is **infrastructure-complete, not capability-complete**: no
live two-host SSU2 result may be cited from it.

**The gate model is canonical in
[`specs/QUALIFICATION-GATES.md`](../../specs/QUALIFICATION-GATES.md).**
This document is the operator-facing procedure; that one is the model. Where
they differ, the model governs. The model is enforced by
`scripts/plan495-peer-eligibility.py`, whose `--self-test` runs in the
repository's routine floor.

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
us, and treats the peer's local state as a first-class cause. Sections 2, 4,
5 and 9 exist for that reason and are not optional paperwork.

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

  **Advertising floodfill is necessary but not sufficient.** Read from the
  pinned i2pd 2.61.0 source, the peer files us as a floodfill only when
  `router.version` is at or above **0.9.62**
  (`libi2pd/NetDb.hpp:55`, `NETDB_MIN_FLOODFILL_VERSION`) **and** we offer a
  published IPv4 address and do not sign with DSA
  (`libi2pd/RouterInfo.cpp`, `IsEligibleFloodfill`). Below that floor our
  RouterInfo is *never* filed as a floodfill, so an exploratory request can
  never target it, and the peer falls back to its **generic dial path**:
  `Transports::PostMessages` will open a session to any router whose
  RouterInfo is admissible, whose version clears **0.9.58** (or which
  declares high bandwidth), which publishes a non-introducer address the
  peer supports, and which is not banned
  (`libi2pd/Transports.cpp:526-543`, `libi2pd/NetDb.hpp:58`).

  Read "advertise floodfill" as **"advertise floodfill *and* clear the
  floodfill floor"**. A lab profile below 0.9.62 can still be dialled; it can
  never be *explored*, and T1 and T5 then rest on the generic dial path
  alone.
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
it deliberately does not, and why. The full, source-cited predicate set is in
[`specs/QUALIFICATION-GATES.md`](../../specs/QUALIFICATION-GATES.md) §3;
this table is the operator's summary of it.

| Predicate | Where it comes from | If we fail it |
| --- | --- | --- |
| RouterInfo signature valid | the peer's ingest path | `peer-admission-refused` |
| `router.version` present and above the peer's **dial floor** (0.9.58) | peer ingest gate and dial path | `peer-admission-refused` / `peer-selection-refused` |
| `netId` present and matching | peer ingest gate (silent) | `peer-admission-refused` |
| at least one supported transport published | peer ingest gate | `peer-admission-refused` |
| advertised endpoint equals the bound socket | `Plan 453` signed-RI path | `peer-admission-refused` |
| no ban-surface `(caps, version)` pair (Java only) | peer's NTCP2 ban path | `peer-admission-refused` |
| selection target: floodfill cap **and** `router.version >= 0.9.62` (i2pd) | peer exploration filters | `peer-selection-refused` |
| `R` present (Java only) | Java `shouldCreate` predicate | `peer-selection-refused` |
| bandwidth/congestion class accepted | Java class gate | `peer-selection-refused` |
| peer profile store is fresh | peer state, §5 | `peer-state-poisoned` |

A family that cannot satisfy a predicate is **not** a failed attempt. It is
`environment-unavailable` or `peer-selection-refused`, and it must not be
reported as an `i2pr` result in either direction.

**Run the preflight; do not eyeball it.** Write the lab RouterInfo profile as
a fixture under
`tests/integration/ssu2/plan495-eligibility-fixtures/` and run:

```text
python3 scripts/plan495-peer-eligibility.py --preflight \
  --profile tests/integration/ssu2/plan495-eligibility-fixtures/lab-profile-i2pd.json \
  --peer-config tests/integration/ssu2/plan495-eligibility-fixtures/peer-config-i2pd.json
```

The checker reports each predicate's state, its requirement and the pinned
source it was derived from, and it **refuses** a lane-ready verdict when a
predicate is `unknown`. It opens no socket, starts no process and touches no
network: the whole point is to decide this before spending an attempt.

Note the **cause** the checker reports next to the outcome class. A missing
`netId` is refused by the peer, but the artifact at fault is our profile —
the cause is `i2pr-lab-profile`, and the lane must record it that way
instead of filing a peer-side condition. `i2pr-implementation` is the only
cause that may accompany `i2pr-defect`: it means the daemon's own code path
is at fault, not the lab profile.

## 3. The lab RouterInfo profile

One profile, stated once, so that both hosts and the evidence agree. Every
field is a decision, not a default.

| Field | Lab value | Why |
| --- | --- | --- |
| `netId` | `2` | peer ingest gate; missing ⇒ silently unreachable, never persisted |
| `router.version` | `0.9.62` | clears the peer's **dial floor** (0.9.58), its **floodfill floor** (0.9.62) and its **peer-test floor** (0.9.62) |
| `caps` | floodfill (`f`) plus the bits the family under test requires | makes us an exploration target so the peer dials back — and, for Java, `R` is what creates a profile at all |
| `R` | per family: **required** by Java's `shouldCreate` predicate | Java selection predicate |
| bandwidth / congestion class | a class the peer accepts (Plan 230's C2) | Java selection predicate |
| published address | the **actual** bound RFC1918 endpoint and port | honest reachability inside the lab |

**The profile is canonical in
[`specs/QUALIFICATION-GATES.md`](../../specs/QUALIFICATION-GATES.md) §4**,
and the checker holds a copy of it: `--profile-drift` fails when the
documented profile and the checked profile disagree.

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
qualification_receipt_path = "/absolute/path/receipt.json"
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

### 4.1 The local receipt export

The daemon can write a **sanitized, count-only readout** of its own SSU2
observer epoch to an operator-designated file. It is enabled by one key:

```toml
[ssu2]
qualification_receipt_path = "/absolute/path/receipt.json"
```

The key is **absent by default**: with no key there is no export, no file,
and no observer readout at all. When it is set, the daemon refuses the
configuration unless `ssu2.enabled = true` and the value is an **absolute**
path. The **parent directory must already exist** — the daemon never creates
directories, so a qualification readout can never build a path tree on the
operator's behalf — and the file is created with mode `0600`. The daemon
rewrites it on a bounded cadence (500 ms), replacing the previous document
atomically.

The document is fixed-shape and count-only:

```json
{
  "schema": "i2pr-plan476-local-receipt-v1",
  "profile": "controlled_loopback" | "private_lan_qualification",
  "phases": [{"phase": "permit_checked", "count": 1}],
  "session_observed": true,
  "product_observed": false,
  "reply_correlated": false,
  "live_qualification": "not-run",
  "error": null
}
```

The phase labels are the collector's canonical nine-phase vocabulary:
`permit_checked`, `router_started`, `peer_session_accepted`,
`peer_session_initiated`, `peer_authenticated`, `request_sent`,
`request_received`, `product_effect_observed`, `router_stopped`. Only phase
labels and counts appear. The two session directions are counted separately,
because "the peer initiated a fresh session to us" is a claim of its own and
must not be folded into the count of sessions we dialled.

`error` is `null` or a collector refusal code — `receipt-overflow`,
`receipt-empty`, `receipt-sequence-invalid`,
`product-without-authenticated-ingress`, `permit-consent-revoked`. **A
non-null `error` is a failure**, and the consumer must fail closed on it
instead of reading the phases beneath it.

`live_qualification` is **always** `"not-run"`. A local export is not a
two-host claim; only the reconciled lane evidence of §9.3 can carry one, and
only after an authorized pair has actually run.

**This is a private local readout — not a listener, and not a publication
permit.** It opens no socket, advertises nothing, and publishes nothing. It
carries no key, no RouterInfo, no address, no endpoint, no payload, and no
transient message ID: the projection is incapable of holding them, and the
export adds nothing to it. Do not copy it into the repository, into an
evidence file, or into a public claim.

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

Record the hygiene state as a fixture and check it — a false condition is a
`peer-state-poisoned` outcome naming the conditions that failed:

```text
python3 scripts/plan495-peer-eligibility.py --hygiene \
  --hygiene tests/integration/ssu2/plan495-eligibility-fixtures/hygiene-fresh.json
```

The lane's evidence itself is validated the same way, against the tier
ladder and the outcome taxonomy:

```text
python3 scripts/plan495-peer-eligibility.py --evidence \
  --evidence <lane-evidence.json>
```

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

Express the withholding in the lab profile itself and run the control:

```text
python3 scripts/plan495-peer-eligibility.py --withholding \
  --profile tests/integration/ssu2/plan495-eligibility-fixtures/withholding-no-floodfill.json \
  --peer-config tests/integration/ssu2/plan495-eligibility-fixtures/peer-config-i2pd.json
```

The control passes only when the verdict is `peer-selection-refused` with the
cause `intentional-withholding` — never `i2pr-defect`, never a pass.

## 9. The guarded per-host executor — the live path

The Plan 476 executor is this procedure's live path. It runs one bounded
attempt per host, each in its own process, and then joins the two host
receipts into the lane evidence. It drives the daemon from outside: the
executor starts the router, observes it through the daemon's own receipt
export (§4.1), and stops it.

It is experimental, opt-in, and scoped to an operator-authorized private
subnet. It is not a general router launcher, and it is not a production
qualification harness.

Prove the executor itself before using it:

```text
python3 tests/integration/ssu2/plan476_live_executor.py --self-test
```

The self-test is offline: no socket, no router, no network. A green
self-test proves the executor's own rules; it is not a live result.

### 9.1 The ordered state machine

`run` is strict, ordered, and fail-closed:

**authorization → gate → withholding control → hygiene → spawn → collect →
emit.**

No process is launched until the first four stages have all passed. The
order is the point: each stage reports a refusal that the next stage would
otherwise have had to guess about.

- **authorization.** Both host owners have consented, the two hosts are
  distinct RFC1918 endpoints on one subnet, and the run epoch agrees.
- **gate.** `scripts/plan495-peer-eligibility.py` runs on the operator's lab
  profile and peer config, **before any process starts**. A refusal is
  classified with the model's two axes — outcome class plus cause — and the
  run is abandoned. The operator does not get to decide afterwards that a
  peer-side refusal was an `i2pr` defect.
- **withholding control.** The executor proves that the lane's own
  classifier recognizes a *deliberate* peer refusal: `peer-selection-refused`
  with the cause `intentional-withholding`. If the control misbehaves, the
  lane refuses to run.
- **hygiene.** The peer-state protocol (§5) is applied and checked. A
  poisoned peer state is refused and — per the canonical model — **does not
  consume the attempt budget**, because a cooldown expiring is a material
  change in the peer's state.

Between the hygiene stage and the spawn there is one more refusal point:

- **pre-spawn checks.** The declared local data directory must be absent or
  exactly `0700`; the local interface, address and route are checked against
  the peer host; and on the peer role the pinned i2pd 2.61.0 source revision,
  the cleanliness of its tracked tree, its reported version and the binary's
  SHA-256 are all verified. A missing, unpinned, patched or mismatched
  reference is refused before any socket is opened.

Only then does the executor spawn the router, collect the receipt, and emit
the host receipt.

- **spawn.** Only the declared local router is started, in its own process
  group. `SIGINT` or `SIGTERM` cancels the attempt: the child is terminated,
  then killed if it does not exit, and always reaped.
- **collect.** The daemon's exported receipt is read until it carries an
  authenticated session. The file merely existing is not readiness — the daemon
  writes it on its first export tick, before any peer has been contacted — so
  a receipt with no session is not an observation.
- **emit.** The host receipt is written mode `0600`, through an atomic
  replacement.

### 9.2 One command per host

`--epoch <32-hex>` is an operator nonce. It **must be identical on both
hosts**: the derived `run_epoch` is what binds the two host receipts
together, and a mismatched epoch is refused at reconciliation.

On the i2pr host:

```text
python3 tests/integration/ssu2/plan476_live_executor.py run --live --role i2pr \
    --manifest <inventory.json> \
    --profile <lab-profile.json> \
    --peer-config <peer-config.json> \
    --epoch <32-hex> \
    --out <i2pr-host-receipt.json> \
    --router-info <i2pr-router.info> \
    --peer-router-info <i2pd-router.info> \
    --peer-binary <i2pd> \
    --daemon-binary <i2pr> \
    --daemon-config <daemon-config.toml>
```

On the stock i2pd host:

```text
python3 tests/integration/ssu2/plan476_live_executor.py run --live --role i2pd \
    --manifest <inventory.json> \
    --profile <lab-profile.json> \
    --peer-config <peer-config.json> \
    --epoch <32-hex> \
    --out <i2pd-host-receipt.json> \
    --router-info <i2pd-router.info> \
    --peer-router-info <i2pr-router.info> \
    --peer-binary <i2pr> \
    --i2pd-binary <i2pd> \
    --i2pd-config <i2pd.conf> \
    --i2pd-source <i2pd-2.61.0-source> \
    --peer-netdb <i2pd-netDb>
```

**The shared inputs.** `--router-info` is this host's own signed
RouterInfo, `--peer-router-info` is the peer host's signed RouterInfo,
and `--peer-binary` is the *peer's* router binary. Both hosts need all
three: the receipts are bound to each other by digest, so the executor
must be able to compute both roles' RouterInfo and binary digests, not
just its own. `--daemon-receipt` may be given explicitly; when it is
omitted the executor reads the destination out of
`[ssu2] qualification_receipt_path` in `--daemon-config`. `--hygiene`
supplies the operator-recorded live-socket conditions that cannot be
derived offline (the peer's static key on the live port, and the
advertised endpoint matching the bound socket); the executor's own
observations override any operator value it can check itself.

**The pinned reference.** `--i2pd-source` names the pinned i2pd 2.61.0
source checkout. Before it starts anything the executor verifies that
the source is at the exact pinned revision, that its tracked tree is
clean, that the binary reports 2.61.0, and that the binary's SHA-256
matches — the same check the Plan 449 preflight performs. An unpinned,
patched or mismatched reference is refused. When the flag is omitted
the executor uses the repository's conventional checkout location for
the pinned revision.

**Role `i2pr`.** The executor starts `<daemon-binary> run --config
<daemon-config>` and reads the daemon's exported receipt — the file named by
`[ssu2] qualification_receipt_path` in that daemon config (§4.1).

**Role `i2pd`.** Stock i2pd has no structured observer, so the peer-side
observation is derived from the peer's **own local state**: whether the
peer's NetDB holds the expected signed RouterInfo, at
`<netDb>/r*/routerInfo-<base32>.dat`, where the name is i2pd's own base32 of
the SHA-256 RouterHash. The executor never reads that file's contents; it
records the fact that the peer persisted the RouterInfo, and nothing else.

**The two host receipts are exchanged manually, by the owner.** The
executor never reaches the peer host over the network: it does not SSH, does
not discover a peer address, does not fetch a reseed, and never touches the
public Internet. Use the separately authorized private provisioning method
(§4).

### 9.3 Reconciling the two host receipts

On the owner side, once both host receipts have been exchanged:

```text
python3 tests/integration/ssu2/plan476_live_executor.py reconcile \
    --receipt <i2pr-host-receipt.json> \
    --receipt <i2pd-host-receipt.json> \
    --out <lane-evidence.json>
```

`reconcile` joins the two receipts into the canonical
`i2pr-plan495-lane-evidence-v1` document: the T0..T8 tier ladder, the
outcome class, the cause, and the attempts consumed. It refuses a mismatched
run epoch, a duplicate role, or a missing peer receipt.

The tier ladder itself is not restated here — it is canonical in
[`specs/QUALIFICATION-GATES.md`](../../specs/QUALIFICATION-GATES.md) §5.
For this lane, its evidence sources are:

| Tier | Evidence source |
| --- | --- |
| T0 | the peer host's `peer_ri_persisted` |
| T1 | the i2pr host's `peer_session_initiated` count — the peer dialled us |
| T2 | the i2pr host's session counts |
| T3 | the i2pr host's `request_sent` and `request_received` counts |
| T4 | the i2pr host's `reply_correlated` |
| T5 | the i2pr host's `peer_session_initiated` count — a fresh inbound session |
| T6 | the i2pr host's `product_observed` |
| T7 | out of scope for this lane — always `not-attempted` |
| T8 | out of scope for this lane — always `not-attempted` |

T7 (a tunnel was built) and T8 (external SAM/Streaming acceptance) are not
part of this lane. Record them as `not-attempted`; never as a failure, and
never as a pass.

### 9.4 Classification discipline

**A peer-side refusal is reported as a peer-side refusal.** Every outcome the
executor can reach is a class from the canonical vocabulary of
[`specs/QUALIFICATION-GATES.md`](../../specs/QUALIFICATION-GATES.md) §1.1
and §5.2 — outcome class plus cause — and it never invents a wording for
"the peer would not have picked us". The reconciled lane evidence can be
`pass`, `peer-admission-refused`, `peer-selection-refused` or `timeout`; a
refusal before the router starts can additionally be
`environment-unavailable` or `peer-state-poisoned`.

`reconcile` **never emits `i2pr-defect`**. That outcome is a reviewed human
judgement about a stage we own, and only the cause `i2pr-implementation`
may accompany it. A peer that admits our RouterInfo and then declines to
select us is `peer-selection-refused`: the *reason* is carried by the tier
ladder, and the cause stays `unattributed` because the ladder cannot say
*why* the peer declined. Not an `i2pr` failure, and not a pass either.

Two consequences for the operator:

- a refusal recorded by the gate is the gate's verdict. It is not a
  provisional finding that a later run may re-read as an `i2pr` defect;
- the withholding control (§8) is the lane's own proof that the classifier
  works. If the control misbehaves, no result from that lane may be cited.

## 10. Collect the evidence

Collect only sanitized stage/count evidence: the tier outcomes, the counts per
tier, the outcome classification, the peer configuration used, and digests. A
successful local bind or signed RouterInfo alone is not an interoperation
result.

The reconciled lane evidence of §9.3 — the `i2pr-plan495-lane-evidence-v1`
document — is this lane's evidence: the tier outcomes, the outcome class and
cause, the attempts consumed, and digests. The per-host receipt of §4.1 is
not a substitute for it, and neither is an exchange of the two local
readouts on their own.

Never retain raw RouterInfos, addresses, keys, identities, or peer logs.
Never promote a lab result into a public-reachability, `R`, `f`, introducer,
or support-inventory claim.

## 11. Tear down

Stop both processes, confirm their owned listeners have closed, and retain only
the sanitized evidence required by the plan. Remove the temporary config and
inventory according to the host owners' retention rules. Before any ordinary
daemon run, set `qualification_profile = "controlled_loopback"` (or remove the
key), remove `qualification_receipt_path` so no readout file is written at
all, disable SSU2 unless explicitly needed for a separate controlled test,
and use the normal data directory. The private profile never authorizes
publication, reachability, public support claims, or floodfill capability.

## 12. What this procedure does not settle

- **No live two-host result exists yet.** The guarded per-host executor and
  its offline simulator are complete and source-verified, but no authorized
  two-host pair has been executed. Nothing in this procedure may be cited as
  a live two-host SSU2 result, in either direction.
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
