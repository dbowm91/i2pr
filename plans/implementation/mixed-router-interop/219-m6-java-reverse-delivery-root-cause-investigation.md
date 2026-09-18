# Plan 219 — M6 Java reverse-delivery root-cause investigation

Status: **registered-ready-m6-java-reverse-delivery-root-cause-investigation**.

## 1. Objective

Determine the first exact stock-Java boundary that prevents the Java helper on
Router A from delivering a reply to the i2pr Destination after Plan 218 proved
the opposite direction and the i2pr-side lookup/publication path.

This is an **investigation and attribution plan**, not a corrective
implementation plan. Its job is to replace the current coarse conclusion
(`java-floodfill-candidate=0`) with a command-derived classification of
the actual failing layer.

The investigation must distinguish, in order:

1. Router B is configured as floodfill but its live RouterInfo does not
   advertise capability `f`;
2. Router B advertises `f`, but Router A stores or serves a stale/different
   RouterInfo;
3. Router A stores the correct RouterInfo but its PeerManager capability index
   does not contain Router B under `f`;
4. Router A indexes Router B as `f`, but FloodfillPeerSelector excludes it;
5. the Java client-specific NetDB has no usable lookup peer even though the
   main NetDB knows Router B;
6. the LeaseSet lookup is sent but does not return the i2pr LeaseSet;
7. the LeaseSet lookup succeeds but OutboundClientMessageOneShotJob cannot
   choose a lease/tunnel or dispatch the Garlic message;
8. Java dispatches TunnelData and the failure moves back onto the i2pr inbound
   receive/decrypt/dispatch path; or
9. the corrected reverse-delivery path succeeds, making Plan 218 eligible for
   a fresh qualification rerun.

No topology "fix" is authorized until one of those branches is proven.

## 2. Why this plan is ready

Plan 217 closed the harness/evidence defects and Plan 218 ran the corrected
destination-only lane on exact commit
`7762e132ec5602360627387639fee27510126546`.

Retained Plan 218 evidence already proves:

- authenticated i2pr↔Java SSU2 sessions;
- reciprocal Router A/B RouterInfo bootstrap;
- real one-hop outbound and inbound i2pr tunnel installation;
- Java helper LS2 lookup through the owned i2pr outbound tunnel;
- signature-valid Standard LS2 decode and destination/key match;
- i2pr LS2 publication through the real outbound tunnel;
- byte-exact i2pr → Java raw Destination delivery;
- Java helper `sendMessage(...)` accepts the reverse-send request;
- the reverse payload does not arrive at i2pr within the bounded window.

Therefore the investigation begins at Java reverse-send lookup/dispatch rather
than reopening already-passed transport, LS2 decode, or i2pr→Java delivery.

## 3. Exact-pinned Java source facts

All Java source statements in this plan refer to Java I2P 2.13.0 commit
`9134f808337b401e8e53c73734c81fab04280c9d`.

### 3.1 Floodfill discovery starts from the PeerManager capability index

`FloodfillPeerSelector.selectFloodfillParticipants(...)` obtains its
initial set from:

```java
_context.peerManager().getPeersByCapability(
    FloodfillNetworkDatabaseFacade.CAPABILITY_FLOODFILL);
```

The selector then ranks candidates good / OK / bad. A new, weak, or unprofiled
peer may be ranked badly but is not thereby absent from the initial set.
Therefore profile quality alone cannot explain an empty initial floodfill set.

### 3.2 RouterInfo ingestion normally populates that capability index

`KademliaNetworkDatabaseFacade.store(Hash, RouterInfo, ...)` reads
`routerInfo.getCapabilities()` and calls:

```java
_context.peerManager().setCapabilities(key, caps);
```

If Router A stores Router B's current RouterInfo with `f`, Plan 219 must
prove whether Router B appears in Router A's floodfill capability index.

### 3.3 Configured floodfill is not advertised floodfill

The current harness fact `java-floodfill-capable` only greps
`router.floodfillParticipant=true` from `router.config`.

Pinned Java `Router.getCapabilities()` advertises `f` according to
runtime floodfill-enabled state. `FloodfillMonitorJob`, when forced by
`router.floodfillParticipant=true`, enables floodfill after the comm
system is running, rebuilds RouterInfo on the state transition, and during low
uptime defers its own FloodfillRouterInfoFloodJob.

A first-created RouterInfo and the later floodfill-enabled RouterInfo can
therefore differ. The exact RouterInfo exchanged during bootstrap is
load-bearing evidence.

### 3.4 Empty floodfill selection does not by itself prove total lookup failure

Pinned `IterativeSearchJob.runJob()` falls back to arbitrary known routers
when floodfill selection is empty. But
`KademliaNetworkDatabaseFacade.getAllRouters()` returns an empty set for a
**client DB**.

The Java reverse-send path uses:

```java
getContext().clientNetDb(_from.calculateHash()).lookupLeaseSet(...)
```

inside `OutboundClientMessageOneShotJob`.

This creates a precise hypothesis: Router A's main NetDB may know Router B while
the helper's client-specific lookup still has no fallback peer unless Router B
is available through the global floodfill capability index.

### 3.5 Public helper send acceptance is not delivery proof

The helper's boolean `sendMessage(...)` only proves the request was
admitted into Java's client-message pipeline. Router-side processing continues
through `ClientMessagePool` and
`OutboundClientMessageOneShotJob`, which must:

1. find or remotely look up the target LeaseSet;
2. choose a usable target lease;
3. choose an outbound client tunnel;
4. construct/encrypt the Garlic message; and
5. dispatch it.

Plan 219 must observe the first missing step.

## 4. Invariants

The investigation MUST preserve:

- exact stock Java I2P 2.13.0 pin; no Java source patching;
- exact i2pd pin remains untouched;
- no public I2P participation;
- no VMComm;
- no reflection or private-state mutation;
- no NetDB/tunnel-state injection;
- no direct LS2 copying;
- no fake RouterInfo/floodfill capability;
- no `netDb.alwaysQuery` override in the authoritative baseline;
- no i2pr production wire change;
- no wait extension as a substitute for attribution;
- loopback-only/non-advertised controlled transport posture;
- sanitized evidence only.

Read-only observation through public Java Router/RouterContext APIs is allowed
if required, provided it is test-only and cannot mutate router state.

## 5. Scope

### In scope

- `tests/integration/m6-interop/run-java.sh`;
- `tests/integration/m6-interop/java/ControlledRouter.java` only for
  read-only diagnostic output;
- `crates/i2pr-daemon/tests/java_tunnel_external.rs` only for additional
  RouterInfo/correlation evidence;
- narrow M6 evidence/static-checker updates;
- test-only parsing of public RouterInfo capability/published/hash data;
- exact-pinned Java log correlation for PeerManager, floodfill selection,
  IterativeSearchJob, OutboundClientMessageOneShotJob, and tunnel dispatch;
- one-variable controlled differential rerun after baseline classification.

### Explicitly out of scope

- changing i2pr production NetDB/tunnel/Streaming/Garlic/SSU2 behavior;
- rewriting helpers to SAM;
- patching Java source;
- joining the public I2P network;
- adding a fourth router before the three-router failure is classified;
- hard-coding Router B through `netDb.alwaysQuery` as a closure mechanism;
- invoking Java internal store/lookup jobs to force success;
- Plan 204 convergence;
- M11 work.

## 6. Required investigation work

### A. Establish an immutable baseline transcript

Run destination-only mode on a fresh scratch topology before changing runtime
behavior:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

Record one correlation timeline with relative timestamps for:

- Router A/B/C process start;
- first RouterInfo file appearance;
- Router A/B floodfill-monitor enable decision;
- RouterInfo rebuild/publication;
- A↔B bootstrap DatabaseStore;
- helper READY;
- i2pr LS2 publication;
- helper SEND acceptance;
- OCMOSJ lookup start/result;
- Java outbound tunnel/lease selection;
- Java dispatch or terminal send failure.

Do not infer causal ordering from unordered grep counts.

### B. Prove live RouterInfo capability state

For Router A, B, and C, emit sanitized snapshots at:

1. first RouterInfo appearance;
2. immediately before bootstrap;
3. immediately after bootstrap;
4. immediately before reverse helper SEND;
5. after reverse-send wait expires.

Each snapshot contains only public RouterInfo facts:

```text
router_role
router_hash
routerinfo_sha256
published_timestamp
capabilities
bandwidth_tier
has_floodfill_capability=true|false
ssu2_address_count
```

For Router B, prove whether the exact RouterInfo bytes delivered to Router A
contain `f`. If RouterInfo changes during the run, record both hashes and
published timestamps.

### C. Prove Router A's view of Router B

Answer each of these separately:

```text
A main-netdb contains B RouterInfo?
A main-netdb B RouterInfo hash matches current B RouterInfo?
A main-netdb B RouterInfo capabilities include f?
A PeerManager getPeersByCapability('f') contains B?
A considers B permanently banlisted?
A selector input includes B?
A selector result includes B?
```

A configuration line MUST NOT satisfy these rows.

A test-only read-only state snapshot may use public accessors such as
`Router.getContext()`,
`context.netDb().lookupRouterInfoLocally(...)`, and
`context.peerManager().getPeersByCapability('f')`. It may emit only
sanitized booleans/counts/capabilities/hashes and MUST NOT modify state.

### D. Correlate the actual reverse-send client lookup

Capture exact-pinned log/event evidence scoped to the helper destination and
i2pr destination for:

```text
reverse-send-admitted
target-ls-local-hit | target-ls-local-miss
target-ls-remote-lookup-started
target-ls-lookup-peer-count
target-ls-lookup-peer-selected
target-ls-lookup-succeeded | target-ls-lookup-failed
target-ls-lease-count
java-client-outbound-tunnel-selected | unavailable
garlic-message-built
tunnel-dispatch-submitted
```

Prefer exact pinned strings and typed driver facts. If raw Java logs cannot
uniquely identify the relevant job, add a bounded test-only correlation
mechanism rather than relying on global counts.

The main NetDB and helper client-specific NetDB must be distinguished.

### E. Replace coarse floodfill evidence with typed Plan 219 evidence

The historical keys may remain for traceability, but Plan 219 classification
must consume typed evidence equivalent to:

```text
j219-b-live-ri-has-f
j219-a-stored-b-ri-has-f
j219-a-peermanager-b-indexed-f
j219-a-selector-input-count
j219-a-selector-result-count
j219-client-db-main-router-count
j219-client-db-lookup-started
j219-client-db-lookup-peer-selected
j219-client-db-lookup-result
j219-ocmosj-lease-selected
j219-ocmosj-outbound-tunnel-selected
j219-ocmosj-dispatch-submitted
```

The static checker must enforce that classification uses these typed facts,
not `router.config` or unscoped grep proxies.

### F. Permit one differential experiment only after baseline classification

After baseline classification, permit at most one fresh-scratch,
single-variable differential run.

If Router B's live RouterInfo gains `f` after the RouterInfo used for
bootstrap, rerun with bootstrap delayed until Router B's live RouterInfo
actually advertises `f` and exchange that newest RouterInfo through the
existing ordinary authenticated DatabaseStore bootstrap path. Change nothing
else.

If Router B advertises `f` and Router A stores that exact RouterInfo but
PeerManager lacks B, do not alter topology: capture the store /
`setCapabilities` path and stop.

If Router A indexes B as `f` but selector output is empty, capture
ignore/permanent-banlist/input/output facts and stop.

If selector returns B but the client lookup fails, trace the client-specific
IterativeSearchJob lookup send/reply path and stop.

If target LS lookup succeeds, continue only far enough to distinguish lease
selection, outbound client-tunnel selection, Garlic construction, and dispatch.

If Java dispatch is proven, stop treating the issue as Java floodfill/client
lookup; preserve dispatch metadata for an i2pr inbound-boundary follow-up.

## 7. Terminal classification

Exactly one Plan 219 terminal class must be emitted per authoritative run:

```text
J219-A-B-LIVE-RI-NOT-F
J219-B-A-STORED-B-RI-NOT-F
J219-C-A-PEERMANAGER-MISSING-B-F
J219-D-A-FLOODFILL-SELECTOR-EXCLUDES-B
J219-E-CLIENT-DB-LOOKUP-HAS-NO-PEER
J219-F-CLIENT-DB-LOOKUP-SENT-NO-LS
J219-G-LS-FOUND-BUT-NO-OUTBOUND-CLIENT-TUNNEL
J219-H-JAVA-DISPATCH-NOT-OBSERVED
J219-I-JAVA-DISPATCH-PROVEN-I2PR-INBOUND-NOT-OBSERVED
J219-J-REVERSE-DELIVERY-PASSED
```

The earliest failed boundary wins. No catch-all "passed" class may hide missing
evidence.

## 8. Failure, cancellation, restart, and contention semantics

- Baseline and differential runs use fresh scratch RouterContexts.
- State from a previous run must not be reused.
- Preserve Plan 217 cleanup and cache-fingerprint guards.
- Keep reverse-send observation bounded by existing Java/client-message and
  driver timeouts unless a pinned Java timeout specifically justifies a change.
- A missing diagnostic row is unknown/failure, never zero-by-default evidence.
- An early Java router exit is environment/runtime failure, not J219-A..J.
- No blind retries; one baseline plus one justified differential is the normal
  maximum.

## 9. Compatibility and migration

No user-facing compatibility change and no support claim change.

`milestone6_java_mixed_router_interop` remains
`not-yet-passed` and `milestone6_interoperable` remains
`not-yet-claimed`.

Any evidence-label changes are test-only and must update the relevant static
checker in the same implementation commit.

## 10. Required verification

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh

I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

The final M6 closure checker is not required to pass in Plan 219; this is an
investigation and the Java family remains open unless J219-J is reached.

## 11. Acceptance criteria

Plan 219 closes only when:

1. one immutable i2pr implementation SHA is recorded;
2. exact Java source/cache fingerprint is verified unchanged;
3. the baseline run is fresh and destination-only;
4. Router B's live RouterInfo `f` capability is command-derived;
5. the exact B RouterInfo stored by A is identified by hash/published/caps;
6. Router A's PeerManager `f` membership for B is observed;
7. floodfill selector input and output are explicitly observed;
8. client-specific NetDB behavior is distinguished from main-NetDB knowledge;
9. OCMOSJ lookup success/failure is correlated to the helper reverse send;
10. if lookup succeeds, target lease selection is observed;
11. if lease selection succeeds, outbound client-tunnel selection is observed;
12. if tunnel selection succeeds, Java Garlic/tunnel dispatch is observed;
13. exactly one J219-A..J terminal classification is emitted;
14. classification is supported by typed facts, not coarse counts;
15. at most one justified single-variable differential run is used;
16. no Java patch/public network/private-state mutation/VMComm/direct LS2 copy
    or production i2pr wire change is introduced;
17. Plan 193 i2pd first-family authority is untouched;
18. closure states the smallest next corrective scope without implementing a
    speculative fix.

## 12. Stop conditions

Stop at the earliest proven boundary:

- B bootstrap RouterInfo lacks `f` → next plan owns
  floodfill-ready RouterInfo timing/role correction.
- A stores B with `f` but PeerManager lacks it → next plan owns
  capability-index/bootstrap correction.
- PeerManager contains B but selector removes it → next plan owns the named
  selector exclusion.
- selector chooses B but client lookup fails → next plan owns client-subdb
  lookup behavior.
- lookup succeeds but no outbound client tunnel exists → next plan owns tunnel
  readiness/selection.
- Java dispatch is proven → next plan investigates i2pr inbound receive/decrypt.
- reverse delivery succeeds → stop; do not extend Plan 219 into Streaming.
  Register a fresh Java final-qualification rerun.

## 13. Closure evidence required

`plans/closure/mixed-router-interop/219-status.md` must contain:

- implementation commits;
- exact baseline SHA and Java pin;
- helper-SEND-to-dispatch source-path map;
- RouterInfo capability timeline for A/B/C;
- Router A view/index/selector evidence for B;
- client-db lookup evidence;
- OCMOSJ lease/tunnel/dispatch evidence;
- J219 terminal classification;
- baseline/differential matrix if a differential was justified;
- commands and outcomes;
- findings by severity;
- exact next-plan scope;
- unblock audit for Plans 201, 204, 205 and the mixed-router roadmap.

## 14. Handoff

```text
plan_219 = registered-ready-m6-java-reverse-delivery-root-cause-investigation
next_executable_plan = 219-m6-java-reverse-delivery-root-cause-investigation

plan_201 = blocked-pending-plan219-root-cause-classification
plan_205 = retained-deferred
plan_204 = blocked-on-m6-java-second-family-closure
```

Do not register a corrective Plan 220 until Plan 219 emits its J219-A..J
classification. Repeated speculative topology changes before that
classification are explicitly out of scope.
