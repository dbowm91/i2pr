# Plan 229 — M6 Java non-zero exploratory paired-tunnel bootstrap corrective

Status: **registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective**

## 1. Objective

Close the exact Plan-228 boundary:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both
```

by making the controlled Java topology capable of producing the ordinary
non-zero-hop exploratory tunnels that stock Java I2P requires as paired
infrastructure before it can build the Plan-227 one-hop client tunnels.

This is a bounded reference-topology corrective. It MAY change only
test-harness Java router role/configuration needed to model a small private
test network faithfully. It MUST NOT change i2pr production behavior, Java
source, Java tunnel-building policy, peer profile contents, NetDB contents by
direct insertion, or the Plan-227 raw-helper client profile.

The intended progression is:

```text
ordinary authenticated RouterInfo bootstrap
        ↓
Router C exists as a normal selectable non-floodfill transit profile on A
        ↓
A builds genuine non-zero exploratory inbound + outbound tunnels
        ↓
stock BuildRequestor has usable paired tunnels
        ↓
unchanged Plan-227 client config builds through explicit Router C
        ↓
A actually dispatches client tunnel build request(s) to C
        ↓
either client tunnels install or the first new stock-Java boundary is recorded
```

Plan 229 stops before the Plan-226/227 destination lookup and reverse-delivery
qualification. If both client tunnels install, a successor/requalification
pass owns the frozen lookup/send behavior.

## 2. Registration basis

Plan 228 closed on exact head with:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both
```

on all four counted attempts.

Retained facts:

- Router C is present and valid in Router A's main NetDB;
- `ProfileOrganizer.isSelectable(C)=true`;
- the Plan-227 raw helper requests one-hop inbound/outbound client tunnels;
- both client creator configs contain Router C;
- `BuildExecutor` reaches the client build path;
- neither direction obtains a paired tunnel;
- only zero-hop exploratory tunnels are installed;
- no client build message is dispatched;
- Router C therefore never receives a client tunnel build request.

The problem is upstream of i2pr wire/protocol behavior.

## 3. Exact-pinned Java source findings

Reference authority remains Java I2P 2.13.0 at:

```text
9134f808337b401e8e53c73734c81fab04280c9d
```

### 3.1 Client builds require paired tunnels

Pinned `BuildRequestor.request()` always uses paired tunnels for client
builds.

For an inbound client tunnel Java needs an outbound paired tunnel. For an
outbound client tunnel Java needs an inbound paired tunnel/reply path.

When no opposite-direction client tunnel exists yet, Java falls back to the
router/exploratory pool. If the selected exploratory tunnel is zero-hop while
the configured exploratory pool is intended to be non-zero, Java explicitly
rejects it:

```java
if (pairedTunnel != null &&
    pairedTunnel.getLength() <= 1 &&
    mgr.getOutboundSettings().getLength() > 0 &&
    mgr.getOutboundSettings().getLength() +
        mgr.getOutboundSettings().getLengthVariance() > 0) {
    pairedTunnel = null;
}
```

and equivalently for inbound.

The resulting pre-dispatch failure is:

```text
Tunnel build failed, as we couldn't find a paired tunnel for ...
```

which Plan 228 observed.

### 3.2 Exploratory pools are already intended to be non-zero

Pinned `TunnelPoolSettings` defaults do not request zero-hop exploratory
operation. On a normal system the defaults are approximately:

```text
inbound exploratory length  = 2
inbound variance            = 1
outbound exploratory length = 3
outbound variance           = 0
```

Pinned `TunnelPoolManager` starts the exploratory pools and schedules
bootstrap fallback construction shortly after startup.

`TunnelPool.buildFallback()` deliberately installs a zero-hop fallback if
no real exploratory tunnel is yet usable. Therefore the zero-hop tunnels seen
in Plan 228 are fallback/bootstrap artifacts, not evidence that the configured
exploratory target length is zero.

### 3.3 Exploratory explicitPeers is not available

Pinned `TunnelPeerSelector.shouldSelectExplicit()` rejects exploratory
settings, and the exploratory selector's explicit branch is disabled.

Plan 229 MUST NOT attempt to force exploratory peer selection with
`explicitPeers`.

### 3.4 Exploratory selection uses the not-failing profile population

Pinned `ExploratoryPeerSelector` falls back to
`ProfileOrganizer.selectNotFailingPeers()` on small/fresh routers.

A RouterInfo merely existing in NetDB is therefore insufficient: the peer must
also have an ordinary profile and remain selectable.

Pinned `ProfileManagerImpl` documents `heardAbout()` as the primary route
for new profile creation. Ordinary RouterInfo DatabaseStore handling can call
`heardAbout(peer, capabilities)`.

The current i2pr Java harness already performs its A/B/C bootstrap through
ordinary authenticated I2NP `DatabaseStore` messages over real SSU2:

```text
java-router-peer-bootstrap-completed =
  ordinary-authenticated-i2np-databasestore-three-routers
```

This existing wire-level bootstrap is retained. No new direct Java NetDB or
profile mutation is necessary or permitted.

### 3.5 The controlled Router-C role is currently inconsistent

The existing Plan-201 bootstrap implementation states:

```text
Router C is a tunnel participant (not floodfill)
```

and the shell topology checks state:

```text
Router C is a tunnel participant — ... no floodfill required
```

but `ControlledRouter.java` currently sets:

```text
router.floodfillParticipant=true
```

unconditionally for every A/B/C process.

Thus Router C currently advertises as a floodfill despite its intended transit
role.

Pinned `TunnelPeerSelector.shouldExclude()` deliberately reduces floodfill
use for exploratory tunnels:

```java
if (isExploratory && isFF && ctx.random().nextInt(4) != 0)
    return true;
```

With a tiny controlled peer set, incorrectly making every router a floodfill
adds avoidable stochastic exclusion pressure precisely where Plan 229 needs a
normal transit peer.

Router B remains the publication/floodfill target. Router C should be restored
to the non-floodfill transit role already documented by the harness.

### 3.6 Java provides a stock small-router one-hop exploratory profile

Pinned upstream:

```text
installer/resources/small/router.config
```

contains:

```text
router.inboundPool.length=1
router.inboundPool.lengthVariance=1
router.outboundPool.length=1
router.outboundPool.lengthVariance=1
```

These are ordinary public router configuration properties consumed by
`TunnelPool.readConfig()` /
`TunnelPoolSettings.readFromProperties()`.

Using this profile for Router A's exploratory pools is therefore a stock Java
configuration mode, not a tunnel-policy patch or private-state injection.

For the constrained three-router test topology it also avoids requiring the
normal public-network 2/3-hop exploratory population before a paired tunnel
can exist.

## 4. Corrective strategy

Plan 229 applies exactly two test-topology corrections:

1. **restore role fidelity**:
   - Router A: service/public-client owner; retain its existing floodfill
     setting for compatibility with earlier controlled-topology evidence;
   - Router B: publication/floodfill target; MUST remain floodfill;
   - Router C: transit/tunnel participant; MUST be non-floodfill.

2. **use Java's stock small-router exploratory profile on Router A only**:
   - `router.inboundPool.length=1`
   - `router.inboundPool.lengthVariance=1`
   - `router.outboundPool.length=1`
   - `router.outboundPool.lengthVariance=1`

No other exploratory quantity, backup quantity, timeout, selection, or
paired-tunnel property changes are authorized.

Routers B and C keep their ordinary exploratory settings. Only A needs paired
exploratory infrastructure for the public-client helper owned by A.

## 5. Invariants

1. Java pin unchanged.
2. i2pd pin unchanged.
3. No i2pr production Rust changes.
4. No Java source patch.
5. No reflection/private-state mutation.
6. No direct `ProfileOrganizer.addProfile`, tier promotion, score mutation,
   profile file fabrication, or profile-map manipulation.
7. No direct Java NetDB store/publish call from the launcher/probe.
8. Existing ordinary authenticated I2NP RouterInfo DatabaseStore bootstrap is
   retained as the sole peer-learning mechanism.
9. No direct tunnel installation.
10. No VMComm.
11. No `netDb.alwaysQuery`.
12. No public I2P/reseed.
13. Baseline loopback topology remains selected; Plan-226 distinct-/24
    topology remains unadmitted.
14. Router B remains the publication/floodfill target.
15. Router C is the sole explicit peer for the Plan-227 raw client.
16. Plan-227 raw helper settings remain unchanged:
    one-hop inbound/outbound, zero-hop disabled, explicit C.
17. No Streaming-helper execution or change.
18. No helper five-minute ceiling increase.
19. No Java tunnel-build request/reply timeout changes.
20. Frozen reverse-delivery 45-second window remains untouched and is not
    entered in Plan 229.
21. Raw Java logs remain scratch-only.
22. Durable evidence is bounded typed facts only.
23. Plan 229 does not claim M6 Java interop merely because exploratory or
    client tunnels build.

## 6. Work package A — role-aware controlled launcher

Extend the out-of-tree `ControlledRouter` launcher with a strict optional
test role or equivalent bounded startup parameter.

Accepted counted roles:

```text
service
publication
transit
```

Fail on any unknown value.

Required role configuration:

```text
service:
  router.floodfillParticipant=true

publication:
  router.floodfillParticipant=true

transit:
  router.floodfillParticipant=false
```

The existing no-SAM/no-I2CP Router-C invocation remains valid.

The role flag exists only to drive normal public `Router(Properties)`
startup configuration. It MUST NOT branch into private Java state operations.

Add post-startup shell evidence proving:

```text
A role=service floodfill=true
B role=publication floodfill=true
C role=transit floodfill=false
```

and require C's actual RouterInfo capabilities to omit `f`.

If C still advertises floodfill after startup:

```text
P229-C-ROLE-MISMATCH
```

Stop before helper execution.

## 7. Work package B — Router-A small exploratory profile

For the `service` role only, set exactly the four pinned upstream small-router
properties:

```text
router.inboundPool.length=1
router.inboundPool.lengthVariance=1
router.outboundPool.length=1
router.outboundPool.lengthVariance=1
```

Do not set:

- quantity;
- backup quantity;
- allowZeroHop;
- explicitPeers;
- random key;
- tunnel build timeout;
- paired-tunnel policy.

Prove the effective live settings with a read-only diagnostic:

```text
P229-EXPLORATORY-SETTINGS
```

returning:

```text
inbound_length=1
inbound_variance=1
outbound_length=1
outbound_variance=1
inbound_quantity=<n>
outbound_quantity=<n>
```

Quantities are diagnostic only and must remain stock/current.

If the four effective settings do not match:

```text
P229-EXPLORATORY-SETTINGS-MISMATCH
```

## 8. Work package C — ordinary profile/bootstrap proof

After the existing authenticated three-router DatabaseStore bootstrap and
before waiting for non-zero exploratory tunnels, add a read-only Router-A
probe:

```text
P229-TRANSIT-PEER <router-c-hex>
```

Return only bounded facts:

```text
main_raw_present=<bool>
main_valid_present=<bool>
profile_present=<bool>
selectable=<bool>
banlisted=<bool>
unreachable=<bool>
caps_has_f=<bool>
profile_count=<n>
not_failing_count=<n>
```

`profile_present` may be derived from public
`ProfileOrganizer.selectAllPeers().contains(C)`.
`selectable` uses the existing public selector check.

The probe is read-only. It MUST NOT call any method that creates a profile.

Required gate:

```text
main_raw_present=true
main_valid_present=true
profile_present=true
selectable=true
banlisted=false
unreachable=false
caps_has_f=false
```

If the existing ordinary wire bootstrap fails to create/retain a usable
profile, stop:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE
```

Do not repair profile state in this plan.

The harness MAY wait a bounded interval for normal DatabaseStore processing
already in flight, but MUST NOT inject a profile or use a direct Java NetDB
store as compensation.

## 9. Work package D — prove genuine non-zero exploratory tunnels

Before starting the raw helper, poll Router A's normal exploratory pools with
a read-only snapshot.

Record:

```text
inbound_exploratory_count=<n>
outbound_exploratory_count=<n>
inbound_nonzero_count=<n>
outbound_nonzero_count=<n>
inbound_one_or_more_remote_hops=<bool>
outbound_one_or_more_remote_hops=<bool>
inbound_c_present=<bool>
outbound_c_present=<bool>
zero_hop_fallback_present=<bool>
```

Peer paths remain scratch-only. Durable evidence carries only booleans/counts.

Authoritative gate:

```text
inbound_nonzero_count >= 1
outbound_nonzero_count >= 1
```

C presence is expected and should be recorded, but the paired-tunnel property
requires a genuine non-zero exploratory tunnel, not a particular paired path.

Allow stock Java's ordinary exploratory pool builder to retry naturally.
Do not trigger builds by private method call.

Use a bounded readiness budget no larger than the existing Java helper
five-minute ceiling. Prefer terminating as soon as both non-zero directions
exist.

If Router C is eligible but no non-zero exploratory tunnel appears:

```text
P229-EXPLORATORY-NONZERO-NOT-BUILT direction=<inbound|outbound|both>
```

Record selector/build-stage facts sufficient to distinguish:

- no exploratory peer selected;
- exploratory config created;
- exploratory build dispatched;
- C/B receive/reject;
- reply timeout;
- install failure.

Do not implement a second correction inside the same counted SHA.

## 10. Work package E — rerun unchanged Plan-227 client helper

Only after the non-zero exploratory gate passes, start the exact existing
Plan-227 raw helper unchanged.

Retain:

```text
inbound.length=1
outbound.length=1
inbound.quantity=1
outbound.quantity=1
inbound.backupQuantity=0
outbound.backupQuantity=0
inbound.allowZeroHop=false
outbound.allowZeroHop=false
inbound.explicitPeers=<Router-C>
outbound.explicitPeers=<Router-C>
```

Do not alter client settings to make the result easier.

Reuse Plan-228 attribution instrumentation to prove, independently for each
direction:

```text
client_config_contains_c
paired_tunnel_selected
paired_tunnel_nonzero
build_message_created
build_dispatched
c_request_received
c_request_decrypted
c_response
a_reply_received
a_reply_decrypted
join_attempted
join_success
client_tunnel_installed
```

The first critical regression gate is:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL
```

MUST disappear once both non-zero exploratory directions are proven.

If Plan 228's same no-paired terminal still occurs despite proven non-zero
exploratory tunnels:

```text
P229-EVIDENCE-CONTRADICTION-NONZERO-EXPLORATORY-BUT-NO-PAIRED
```

## 11. Client-build continuation

Once a non-zero paired tunnel is selected, continue through the already
instrumented stock-Java build path rather than stopping merely because Plan
228's boundary moved.

Use the existing Plan-228 evidence precedence to locate the first new boundary:

```text
P229-NEXT-BOUNDARY-BUILD-MESSAGE-CREATE
P229-NEXT-BOUNDARY-A-DISPATCH
P229-NEXT-BOUNDARY-FIRST-HOP-DELIVERY
P229-NEXT-BOUNDARY-C-DECRYPT
P229-NEXT-BOUNDARY-C-REJECT code=<n>
P229-NEXT-BOUNDARY-REPLY-RETURN
P229-NEXT-BOUNDARY-REPLY-DECRYPT
P229-NEXT-BOUNDARY-REMOTE-REJECT code=<n>
P229-NEXT-BOUNDARY-LOCAL-JOIN
P229-NEXT-BOUNDARY-BUILD-REPLY-TIMEOUT
```

Use only the terminal corresponding to the earliest proven missing stage.

If both exact one-hop client tunnels through C install:

```text
P229-CLIENT-TUNNELS-BUILT
```

Stop the plan there.

Do NOT run the target LS lookup, tracked reverse send, or 45-second payload
acceptance in this plan. That avoids mixing topology repair with the
subsequent behavioral qualification.

## 12. Why this is not profile-state injection

Plan 229 relies on three normal stock mechanisms:

1. normal `Router(Properties)` startup configuration;
2. ordinary authenticated RouterInfo DatabaseStore messages already present in
   the harness;
3. Java's normal exploratory/client tunnel pool machinery.

The plan specifically forbids:

```text
ProfileOrganizer.addProfile
getOrCreateProfile* from a probe
profile tier mutation
profile file fabrication
netDb.store()/publish() from the launcher/probe
TunnelPool.addTunnel/direct install
reflection
Java source patch
```

The read-only profile preflight exists so the counted run fails instead of
silently manufacturing the prerequisite.

## 13. Upstream testing alignment

This correction follows Java I2P's own testing/configuration patterns:

- `MultiRouter` uses separate RouterContexts, loopback transport,
  `i2np.allowLocal=true`, no public reseed, and the real transport stack;
- Java's bundled small-router configuration uses one-hop exploratory pools;
- public router configuration properties are loaded through
  `TunnelPoolSettings.readFromProperties()`;
- zero-hop exploratory fallbacks remain available for startup bootstrap;
- no VMComm or private tunnel injection is required.

The only intentional deviation from the current i2pr harness is restoring the
documented A/B/C role split that the launcher currently flattens by making C a
floodfill.

## 14. Focused tests

Add focused tests covering at least:

1. unknown launcher role fails closed;
2. service role keeps floodfill true;
3. publication role keeps floodfill true;
4. transit role sets floodfill false;
5. only service role receives the four small-router exploratory properties;
6. no explicitPeers appears on exploratory pool settings;
7. no exploratory quantity/backup/timeout override appears;
8. C RouterInfo still advertising `f` maps to role mismatch;
9. C no profile maps to `C-NOT-EXPLORATORY-ELIGIBLE`;
10. C profile but not selectable maps to the same eligibility stop;
11. zero-hop-only exploratory state is insufficient;
12. inbound-only non-zero exploratory is insufficient;
13. outbound-only non-zero exploratory is insufficient;
14. both non-zero directions admit raw-helper start;
15. non-zero exploratory + no paired maps to contradiction;
16. paired selected + no create maps to create boundary;
17. create + no dispatch maps to dispatch boundary;
18. dispatch + C receive missing maps to first-hop/receive boundary;
19. C reject is retained with bounded code;
20. reply/decrypt/join failures retain earliest-stage ordering;
21. both exact one-hop client tunnels install maps only to
    `P229-CLIENT-TUNNELS-BUILT`;
22. no Plan-226 lookup/reverse-send evidence is permitted after the Plan-229
    terminal;
23. exactly one P229 terminal per counted attempt;
24. unrelated router/build logs cannot satisfy Plan-229 facts;
25. secret-bearing raw log lines cannot enter durable evidence.

## 15. Static acceptance guards

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` to require:

- Java/i2pd pins unchanged;
- explicit counted roles A=service, B=publication, C=transit;
- B floodfill true;
- C floodfill false;
- A-only exact four small-router exploratory settings;
- no exploratory `explicitPeers`;
- no profile mutation APIs;
- no direct Java NetDB store/publish from launcher/probes;
- existing ordinary wire DatabaseStore bootstrap retained;
- no direct tunnel install;
- no VMComm;
- no `netDb.alwaysQuery`;
- no public/reseed endpoints;
- no distinct Plan-226 topology;
- Plan-227 client helper settings unchanged;
- Streaming helper not executed;
- timeouts unchanged;
- raw logs scratch-only;
- exactly one terminal;
- target lookup/reverse-delivery lane not executed by Plan 229.

## 16. Execution policy

Commit all implementation before counted execution.

Require exact clean head:

```bash
git status --porcelain=v1
git rev-parse HEAD
```

Run destination only:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

Maximum three counted attempts per implementation SHA.

Each attempt uses:

- fresh disposable A/B/C RouterContexts;
- baseline loopback topology;
- A=service, B=publication, C=transit;
- exact A small-router exploratory settings;
- existing authenticated RouterInfo bootstrap;
- unchanged Plan-227 client helper;
- unchanged pins;
- unchanged timeouts;
- no between-attempt tuning.

A run may stop early once the first authoritative Plan-229 terminal is proven.

If implementation/config semantics change, commit a new SHA before another
counted attempt.

## 17. Verification floor

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh

cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
```

## 18. Acceptance criteria

Plan 229 closes only if:

1. reference pins remain frozen;
2. no production Rust behavior changes;
3. counted A/B/C roles are explicit and C is proven non-floodfill;
4. Router A effective exploratory settings match the pinned small-router
   profile;
5. existing ordinary authenticated RouterInfo bootstrap remains the only
   profile-learning path;
6. Router C is proven present, profiled, selectable, non-banned,
   reachable, and non-floodfill on A before relying on it;
7. Router A installs at least one genuine non-zero inbound exploratory tunnel;
8. Router A installs at least one genuine non-zero outbound exploratory tunnel;
9. Plan-227 client helper settings remain byte/semantic equivalent;
10. Plan-228 `NO-PAIRED-TUNNEL` either disappears or is classified as an
    explicit contradiction after the exploratory gate;
11. paired tunnel selection is proven when reached;
12. client build creation/dispatch/C handling/reply/join stages are recorded
    independently when reached;
13. exactly one Plan-229 terminal is emitted per counted run;
14. no target lookup/reverse-delivery claim is made;
15. raw Java logs remain scratch-only;
16. focused/routine verification passes;
17. closure record, registry, roadmap, and unblock audit land.

## 19. Stop conditions and successor rules

### Role/profile/bootstrap stops

```text
P229-C-ROLE-MISMATCH
P229-EXPLORATORY-SETTINGS-MISMATCH
P229-C-NOT-EXPLORATORY-ELIGIBLE
```

### Exploratory build stop

```text
P229-EXPLORATORY-NONZERO-NOT-BUILT direction=<inbound|outbound|both>
```

### Contradiction

```text
P229-EVIDENCE-CONTRADICTION-NONZERO-EXPLORATORY-BUT-NO-PAIRED
```

### New build-path boundaries

Use the §11 `P229-NEXT-BOUNDARY-*` taxonomy.

### Success for this corrective

```text
P229-CLIENT-TUNNELS-BUILT
```

On `P229-CLIENT-TUNNELS-BUILT`, register a narrow requalification successor
that restores the frozen Plan-226/227 target lookup and tracked reverse send.
Do not keep adding exploratory/tunnel diagnostics.

On any earlier terminal, register at most one successor tied to that exact
stage.

## 20. Closure evidence

Update:

```text
plans/closure/mixed-router-interop/229-status.md
```

with:

- implementation SHA(s);
- Java/i2pd pins;
- role configuration proof;
- Router-C RouterInfo capability proof;
- Router-A exploratory setting proof;
- C profile/selectability proof;
- ordinary bootstrap evidence;
- exploratory inbound/outbound installed counts;
- bounded exploratory build trace if a direction fails;
- Plan-228 paired/client build trace;
- exact terminal;
- counted attempt history;
- local verification;
- exact-head CI;
- security/compatibility notes;
- unblock audit.

## 21. Registration disposition

```text
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
plan_229 = registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective

plan_201 = blocked-pending-plan229-nonzero-exploratory-paired-tunnel-bootstrap-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan229
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```
