# Plan 199 — Milestone 10 unified final closure

Status: **registered executable unified M10 closure authority**.

This is the focal handoff document for ending Milestone 10. It supersedes the scattered **execution sequence** `Plan 198 -> Plan 195` as the thing a downstream implementation agent should follow, but it does **not** erase, relabel, or weaken the evidence owned by earlier plans. Plans 173–182 remain the implementation and local-acceptance history for the M10 product. Plans 183–198 remain the provenance for the mixed-router work discovered by Plan 181. Plan 199 incorporates the still-open acceptance requirements from Plans 198 and 195 into one bounded closure program.

Do not create another roadmap layer merely to restate this work. A new corrective plan after Plan 199 is justified only if execution identifies a genuinely new product defect that cannot be corrected within the phase that owns it. Prefer updating `plans/199-status.md` with the first failing boundary and continuing this plan.

## 1. Goal

Close Milestone 10 without reopening already-passed implementation layers and without converting missing interoperability evidence into a documentation-only pass.

The remaining work is narrow:

```text
Phase A
  close retained M6 Java-family mixed-router destination/Streaming acceptance
  required because M10 remote service rows depend on real remote Streaming

Phase B
  replace the two Plan 181 remote `blocked` rows with command-derived passes
    remote-independent-http-eepsite
    remote-independent-irc-service

Phase C
  run exact-head final validation/evidence workflows
  normalize M6/M10 authority
  close M10 and hand off to Milestone 11 planning
```

The final bounded M10 claim remains the Plan 173 service-tunnel product:

```text
generic TCP client/server tunnels
HTTP/1.1 .i2p proxy + CONNECT
SOCKS5 no-auth DOMAINNAME CONNECT
IRC client privacy profile
IRC server authenticated-Destination hostname projection
bounded transactional service lifecycle
independent ordinary local application clients
remote independent I2P HTTP service interoperability
remote independent I2P IRC service interoperability
```

Do **not** broaden this to public-network production readiness, anonymity guarantees, censorship resistance, arbitrary outproxying, transit/floodfill operation, or unsupported application profiles.

## 2. Registration floor and current authority

Plan 199 was registered from:

```text
i2pr main = ed81560ce93652576d3b8141b4361490af463254
commit     = plan198: wire public Java client closure lane
routine CI = 34808892812 (success)
```

Retain these passed results as prerequisites; do not reimplement them:

```text
M7 SAM 3.1 final acceptance                       = closed via Plan 151
M8 SSU2 v2 independent direct interop             = closed via Plan 161/162
M9 I2CP independent lifecycle/final acceptance    = closed via Plan 172

M10 foundation/shared Streaming runtime           = passed via Plan 174
M10 generic client/server tunnels                 = passed via Plan 175
M10 HTTP proxy + CONNECT                          = passed via Plan 176
M10 SOCKS5 CONNECT                                = passed via Plan 177
M10 IRC client privacy profile                    = passed via Plan 178
M10 IRC server authenticated hostname projection  = passed via Plan 179
M10 composition/reconcile/hardening               = passed via Plan 180
M10 local delivery/byte round-trip                = passed via Plan 182
M10 independent local application-client matrix   = 29 rows passed via Plan 181

M6 authenticated I2NP/reference preflight         = passed via Plan 184
M6 live one-hop exploratory tunnels/liveness      = passed via Plan 185
M6 mixed-router NetDB lookup/publication          = passed via Plan 186
M6 destination local product                      = retained passed via Plan 187
M6 build/reply and inbound reply-path corrections = retained passed via Plans 188/190
M6 inbound destination delivery wire corrective   = passed via Plan 192
M6 i2pd mixed-router Streaming family             = passed via Plan 193, 33/33 twice
M6 Java controlled topology/authenticated SSU2    = passed via Plan 196
M6 Java SSU2 `pq` option parser tolerance         = passed via Plan 197
```

Current open authority at registration:

```text
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = blocked-public-java-client-leaseset2-publication
plan_195 = registered-blocked-by-plan198

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Plan 199 becomes the handoff authority for the remaining sequence. Plans 198 and 195 remain detailed provenance and must be updated to passed/retained status when their incorporated requirements are actually satisfied.

## 3. Immutable external references and independence rules

Do not move reference pins during this closure unless a pin itself is proven unusable by a documented upstream defect. A version upgrade is not a shortcut around a failing acceptance boundary.

```text
i2pd 2.61.0
repository = PurpleI2P/i2pd
commit     = 635b013a612ff47278ef02acf8580a28e10e26c5
role       = retained first-family M6 proof and preferred M10 remote service host

Java I2P 2.13.0
repository = i2p/i2p.i2p
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
role       = mandatory second-family M6 qualification

jaraco/irc
repository = jaraco/irc
commit     = 90e10e690da2c7bf60de21be4e36d24c9ffd7474
role       = independent ordinary IRC application client
```

Rules:

- exact-pinned external source/checkouts remain unmodified;
- reference caches must be fingerprinted/clean-checked before and after counted runs;
- test helpers may compile out-of-tree against public Java jars/APIs;
- do not patch Java I2P, i2pd, curl, or jaraco/irc to obtain a pass;
- do not inject private router state, LeaseSets, RouterInfos, tunnel state, or Streaming state through internal Java/i2pd APIs;
- do not use a self-composed i2pr peer as an independent router substitute;
- do not use the public I2P network simply to make the controlled qualification pass;
- do not expose private Destination keys, session keys, raw sensitive payloads, or identifying logs in retained evidence.

Reference-side zero-hop client tunnels are allowed where already authorized by Plan 198. Counted **i2pr** paths must continue to use real remote tunnel/NetDB/destination/Streaming machinery and may not substitute `LocalZeroHop` or direct destination-over-SSU2 delivery.

## 4. Why Phase A is still required

Plan 181 intentionally left exactly two M10 remote rows blocked because M10 remote HTTP/IRC depends on the lower-layer mixed-router Destination/Streaming substrate. Plans 183–193 closed that substrate against i2pd. Plans 194/196/197/198 then qualified the second independent family, Java I2P.

Plan 198 correctly replaced Java SAM as the counted destination owner with ordinary public Java APIs:

```text
tests/integration/m6-interop/java/ReferenceRawDestination.java
  I2PClient / I2PSession

tests/integration/m6-interop/java/ReferenceStreamingService.java
  I2PSocketManagerFactory / I2PSocketManager / I2PServerSocket / I2PSocket
```

Both public client sessions connect to the exact-pinned controlled Java router, and the Rust test processes themselves run, but the real i2pr `DatabaseLookup` does not resolve the Java public-client Standard LeaseSet2. The Java lane remains fail-closed and therefore all lookup-dependent destination/Streaming rows remain blocked.

The latest Plan 198 status records this as:

```text
Java public-client tests: 2 passed, 0 failed at Rust process level
mandatory Java rows: blocked at public-client LeaseSet2 publication
```

The next work must solve that boundary before M10 remote HTTP/IRC is counted.

## 5. Source-grounded diagnosis of the Java LeaseSet2 boundary

The current best-supported diagnosis is that the **single-Java-router controlled topology is insufficient for Java 2.13.0's segmented NetDB publication model**, not that another i2pr transport/Streaming subsystem is missing.

This diagnosis comes from exact-pinned Java 2.13.0 source behavior and must be confirmed by Phase A.0 runtime evidence before the topology change is counted as the correction.

Relevant exact-pinned source paths:

```text
router/java/src/net/i2p/router/client/ClientConnectionRunner.java
router/java/src/net/i2p/router/client/ClientMessageEventListener.java
router/java/src/net/i2p/router/networkdb/kademlia/FloodfillNetworkDatabaseSegmentor.java
router/java/src/net/i2p/router/networkdb/kademlia/FloodfillNetworkDatabaseFacade.java
router/java/src/net/i2p/router/networkdb/kademlia/KademliaNetworkDatabaseFacade.java
router/java/src/net/i2p/router/networkdb/kademlia/RepublishLeaseSetJob.java
router/java/src/net/i2p/router/networkdb/kademlia/FloodfillPeerSelector.java
core/java/src/net/i2p/client/impl/RequestVariableLeaseSetMessageHandler.java
router/java/src/net/i2p/router/MultiRouter.java
```

The exact-pinned behavior relevant to the blocker is:

1. A primary I2CP client session receives its own `FloodfillNetworkDatabaseFacade` keyed by the client Destination hash.
2. Java's segmented NetDB therefore distinguishes the router/main NetDB from client sub-NetDBs.
3. When the client returns a signed LeaseSet/LeaseSet2, `ClientMessageEventListener` calls the client's floodfill NetDB facade `publish(ls)`.
4. The client-created LeaseSet is stored in the client-specific NetDB and scheduled for publication.
5. Incoming network `DatabaseLookupMessage` / `DatabaseStoreMessage` handlers are registered only for the non-client/main NetDB.
6. The local-client republish path sends the LeaseSet to selected floodfill peers.
7. Floodfill peer selection excludes the publishing router itself.
8. Plan 198 currently starts only one isolated Java router, so there is no distinct floodfill target to receive the client's LeaseSet into a network-visible main NetDB.

That topology naturally produces the observed symptom:

```text
public Java I2PSession.connect() succeeds
 -> Java owns a client session / client NetDB
 -> client LeaseSet2 is created locally
 -> publication needs a distinct floodfill peer
 -> no eligible peer exists in the one-router private topology
 -> i2pr sends a real DatabaseLookup to the Java router main NetDB
 -> no network-visible copy exists there
 -> no LeaseSet2 response before the bounded timeout
```

Setting or omitting:

```text
i2cp.dontPublishLeaseSet=false
```

cannot by itself fix this architecture. That option controls whether the LS2 is marked unpublished; it does not merge the client sub-NetDB into the main NetDB or create an eligible floodfill peer.

Do not convert this diagnosis into a pass from source inspection alone. Phase A.0 must prove the relevant lifecycle with sanitized runtime evidence.

## 6. Phase A.0 — prove the Java publication lifecycle before changing topology

Before adding a second reference router, make the current external lane distinguish these boundaries explicitly:

```text
java-public-client-session-established
java-request-variable-leaseset-observed
java-create-leaseset2-observed
java-client-subdb-leaseset-stored
java-client-leaseset-publication-queued
java-client-leaseset-publication-attempted
java-network-visible-leaseset-resolved
```

The first six may be diagnostic/internal-reference facts derived from sanitized stock Java logs/statistics. The seventh is counted only from the ordinary remote NetDB path.

Requirements:

- `ReferenceRawDestination` and `ReferenceStreamingService` remain public-API-only;
- `READY` after `session.connect()` must no longer be treated as proof of network-visible LS2 publication;
- retain only bounded/sanitized facts: event class, destination hash if already public and needed for correlation, timestamps/durations, counters, result categories;
- never retain the private Destination, private LeaseSet keys, raw payloads, or full secret-bearing I2CP/SAM lines;
- correct stale diagnostic language in `crates/i2pr-daemon/tests/java_tunnel_external.rs` that still attributes the Plan 198 stop specifically to the old SAM bridge;
- the stop record must identify the observed boundary, e.g. `client-ls2-local-but-not-network-visible`, rather than repeating a historical hypothesis;
- no Java private/internal API may be called merely to inspect or export the LS2.

Stop here and reassess only if the exact-pinned Java runtime does **not** reach client LeaseSet2 creation/local publication at all. That would invalidate the dual-router diagnosis and identify a narrower Java client/tunnel lifecycle issue. Do not add routers until the first failing transition is known.

## 7. Phase A.1 — controlled two-Java-router publication topology

If Phase A.0 confirms the expected client-subDB publication boundary, extend the existing Plan 196/198 harness to a minimal two-router topology.

Required roles:

```text
Java Router A — service router
  exact-pinned Java I2P 2.13.0
  owns public I2CP reference client(s)
  owns the zero-hop client LeaseSet2 used by RAW/Streaming tests
  real SSU2 on loopback

Java Router B — publication/floodfill router
  exact-pinned Java I2P 2.13.0
  separate RouterContext/process and data directory
  floodfill participant
  receives Router A client LeaseSet2 through ordinary NetDB publication
  answers the counted i2pr DatabaseLookup from its main NetDB
```

Both routers must:

- use the same verified exact-pinned Java cache without mutating it;
- have distinct disposable router/data/profile/netDb/key directories;
- have distinct SSU2 ports and any I2CP/SAM ports actually needed;
- bind transport/client surfaces to loopback only;
- disable public reseeding and update/news behavior as in the retained controlled topology;
- disable the upstream loopback bogon blocklist only within the private harness where required;
- never enable `i2p.vmCommSystem=true`;
- never use public-network peers;
- use ordinary UDP SSU2 for counted router-to-router behavior.

Prefer extending `ControlledRouter.java` in a generic way that can launch either controlled instance, or invoking the retained launcher twice. Do **not** build a specialized Java router shim that directly manipulates NetDB state.

The exact-pinned upstream `MultiRouter` utility is architectural precedent for multiple `Router(Properties)` instances and explicitly notes that VMComm bypasses UDP/TCP and is not appropriate for transport testing. Its direct internal reseed helper is **not** acceptable counted evidence here because Plan 198 forbids private state injection.

## 8. Phase A.2 — bootstrap RouterInfo knowledge through ordinary protocol paths

The two Java routers must know enough about one another to perform normal floodfill publication without using private Java state insertion.

Counted bootstrap rules:

- RouterInfo material must be ordinary signed RouterInfo emitted by the exact-pinned router;
- install/learning must happen through an ordinary I2NP/NetDB/transport path;
- no harness call to `context.netDb().publish(...)`, `store(...)`, reflection, package-private test hooks, or on-disk NetDB file fabrication;
- no source patch to bypass peer selection;
- no public reseed service.

Preferred implementation:

1. start both controlled Java routers and wait for signed RouterInfos;
2. establish/qualify ordinary SSU2 reachability as needed;
3. deliver each required RouterInfo through the existing production-compatible DatabaseStore/I2NP path already exercised by the M6 harness;
4. prove Router A's client/floodfill selection can see Router B as an eligible floodfill target;
5. prove Router B main NetDB can identify Router A where required for delivery/publication.

If only Router A needs Router B for the publication flow, do not manufacture unnecessary symmetric topology. Add only the minimum ordinary RouterInfo knowledge required by the real protocol path.

A small harness coordinator may move **public signed RouterInfo bytes** between process boundaries for delivery through the ordinary protocol implementation. It may not install them directly into private Java structures.

## 9. Phase A.3 — prove Java client LeaseSet2 publication to Router B

Only after Router A knows an eligible Router B floodfill should the counted public Java destination be considered publication-capable.

The harness must prove this sequence from command/runtime evidence:

```text
public Java client on A connects
 -> A requests leases / client creates Standard LeaseSet2
 -> A client sub-NetDB stores current LS2
 -> A schedules/attempts normal floodfill Store
 -> ordinary transport/NetDB path sends LS2 toward B
 -> B main NetDB receives and validates the LS2
 -> B can answer an ordinary DatabaseLookup for the Destination hash
```

Required properties of the reference LS2 remain:

```text
LeaseSet type      = Standard LS2 / type 3
Encryption type    = X25519 / type 4
reference inbound  = documented zero-hop client lease allowed by Plan 198
publish flag        = ordinary published behavior (`dontPublishLeaseSet` absent/false)
```

The reference-side zero-hop lease is not a shortcut for the i2pr path. It is simply the Java client's inbound lease. The counted i2pr request must still traverse the real mixed-router NetDB/tunnel/destination stack.

The harness should not wait a hard-coded long sleep if an observable publication-ready condition is available. Poll bounded readiness with a clear deadline and record how readiness was proven.

## 10. Phase A.4 — counted i2pr lookup must target the publication router

Once Router B holds the Java client LS2 in its network-visible main NetDB, perform the real i2pr lookup against Router B.

Expected counted route:

```text
i2pr DestinationRouting
 -> real installed one-hop outbound tunnel
 -> ordinary DatabaseLookup
 -> Java Router B main/floodfill NetDB
 -> DatabaseStore/LeaseSet2 reply through ordinary return path
 -> existing i2pr LeaseSet2 validation/store
```

The returned LS2's lease gateway is expected to identify **Router A**, because Router A owns the Java service Destination. Subsequent destination/Streaming traffic therefore traverses:

```text
i2pr
 -> existing ECIES/Garlic + Streaming implementation
 -> real i2pr outbound tunnel
 -> Java Router A
 -> Router A client zero-hop lease
 -> public Java I2PSession / I2PSocket API
```

and reverse traffic must traverse the ordinary Java client/router path into the real i2pr inbound tunnel.

This separation is intentional and protocol-realistic:

```text
B = NetDB/floodfill publication/lookup role
A = service Destination lease gateway
```

Do not force Router A's main NetDB to answer its own client's external lookup merely to preserve the old one-router test shape.

## 11. Phase A.5 — complete all Plan 198 Java destination/Streaming rows

After `lease-lookup-completed` becomes genuinely command-derived, continue through the existing production implementations. Avoid a new Java-specific routing or Streaming path.

At minimum preserve/obtain all mandatory Plan 198 evidence currently enforced by:

```text
scripts/check-m6-final-closure-evidence.sh
```

This includes, by category:

### Topology / transport / tunnel prerequisites

```text
daemon-strict-profile
reference-routerinfo-verified
session-established
outbound-installed
inbound-installed
liveness-first-test
direct-rejected
```

### NetDB / LeaseSet2

```text
lease-lookup-completed
ls2-publication-tunnel
```

### Bidirectional raw destination traffic

```text
destination-outbound-delivered
reference-received
destination-inbound-received
```

with exact length/digest equality and no direct transport substitute.

### Bidirectional Streaming

```text
streaming-syn-sent
streaming-syn-accepted
streaming-established
streaming-data-digest
streaming-multipacket-digest
streaming-reverse-data-digest
streaming-reverse-multipacket-digest
streaming-sibling-established
streaming-sibling-data-digest
streaming-close
streaming-sibling-isolated
streaming-b-established
streaming-b-data-digest
streaming-b-reverse-data-digest
streaming-b-close
manager-cleanup
```

### Cleanup

```text
shutdown-baseline
```

The checker is the machine-readable source of truth; if its mandatory set changes for a legitimate implementation reason, update the checker and this plan/status together. Do not rename rows simply to escape an existing requirement.

Plan 193's i2pd robustness/retransmission evidence remains retained. Java need not duplicate every impairment experiment unless a Java-specific failure makes it necessary.

## 12. Phase A.6 — final two-family M6 closure gate

Phase A is complete only when **both** exact-pinned implementation families are all-pass on the candidate closing head.

Run the existing external lane and final evidence gate:

```text
bash tests/integration/m6-interop/run-m6-mixed-router.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh
```

and the manual workflow:

```text
.github/workflows/m6-mixed-router-external.yml
```

Hard Phase A closure condition:

```text
mandatory_i2pd = all passed, 0 blocked, 0 failed, 0 missing
mandatory_java = all passed, 0 blocked, 0 failed, 0 missing
m6_final_closure = passed
exact i2pr head = closing candidate head
exact i2pd pin = verified
exact Java pin = verified
reference caches = clean/unmodified
external workflow provenance = present
```

Only then normalize:

```text
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = passed-m6-java-public-client-complete-second-family-closure
milestone6_java_mixed_router_interop = passed-via-plan198
milestone6_interoperable = passed-via-plan193-and-plan198
```

Phase B must not count a remote M10 service pass before this condition exists. Development of a fixture may be prepared in parallel locally, but final M10 evidence may not inherit an unclosed M6 substrate.

## 13. Phase B overview — convert the existing M10 lane from blocker qualification to final acceptance

Do not create a parallel M10 test framework. Extend the existing Plan 181 assets:

```text
tests/integration/service-tunnels/run-independent.sh
scripts/check-service-tunnel-acceptance-evidence.sh
.github/workflows/service-tunnels-external.yml
tests/integration/service-tunnels/fixtures/
tests/integration/service-tunnels/clients/
```

The local Plan 181 matrix is retained and must still run. The critical structural change is that the current checker/runner intentionally permit **only**:

```text
record_blocked "remote-independent-http-eepsite" ...
record_blocked "remote-independent-irc-service" ...
```

and reject any pass-capable path for those rows. That behavior was correct while M6 was unavailable. It is now a mandatory closure modification.

Phase B must:

1. remove the static rule that remote rows are blocked-only;
2. add guarded command-derived pass paths for the two remote rows;
3. preserve fail-closed behavior when references, pins, services, or evidence are unavailable;
4. reject hard-coded remote passes;
5. retain the 29 local command-derived rows and their existing integrity checks;
6. preserve the no-patch, no-clearnet-fallback, no-secret-evidence, and cleanup gates.

Do not delete `record_blocked` globally if it remains useful for a diagnostic mode. Final/full M10 closure mode must treat any blocked remote row as non-closure.

## 14. Phase B reference-family choice

Once M6 has proved both router families, the application layer does **not** need to prove both again. Plan 195 explicitly allows the simplest already-qualified family.

Default to exact-pinned **i2pd 2.61.0** for the remote HTTP/IRC application service host because:

- its full mixed-router destination/Streaming path is already retained-passed via Plan 193;
- the Plan 181 external harness already provisions exact-pinned i2pd;
- using it avoids making application-profile evidence depend on the more complex two-Java publication fixture;
- M6 Phase A has already supplied the second-family independence requirement.

If a concrete application-specific i2pd issue prevents either row, try the now-qualified Java family before changing i2pr product code. Do not reopen M6 merely because an application fixture or one router family's service API behaves differently.

## 15. Phase B.1 — remote independent HTTP eepsite row

Required path:

```text
ordinary unmodified curl
 -> i2pr HTTP .i2p proxy
 -> existing i2pr destination/Streaming path
 -> exact-pinned independent router
 -> independently hosted I2P HTTP service
 -> ordinary loopback HTTP fixture behind the reference service
```

The reference router/service side may use a public documented SAM/I2CP/tunnel API to host a small ordinary HTTP fixture. The fixture may implement HTTP application behavior; it must not implement I2P protocols.

`remote-independent-http-eepsite` becomes `passed` only if one counted command-derived attempt proves all of:

```text
reference router exact pin + clean state
reference service Destination created through public API
service LS2/Streaming reachable through the already-qualified remote substrate
curl is unmodified and version-recorded
curl target is .i2p / configured I2P Destination, not an IP fallback
no clearnet DNS lookup of the .i2p target
no clearnet HTTP outproxy fallback
HTTP GET succeeds with expected status + exact response-body digest
HTTP POST or equivalently meaningful request-body path has exact digest equality
at least one larger response/request path exercises multi-packet Streaming
retained Plan 176 privacy policy is observed (stable User-Agent policy; Referer/From absent)
connection uses the real remote Streaming path, not local-delivery fabric
service and client resources return to baseline
```

Retain only status codes, lengths, digests, policy booleans, and bounded sanitized facts. Do not retain raw sensitive HTTP headers/bodies.

If a configured full Destination is used instead of human-readable naming, that is acceptable; M10 does not require a general address-book implementation. The test must still prove `.i2p`/I2P routing and no clearnet fallback.

## 16. Phase B.2 — remote independent IRC service row

Required path:

```text
exact-pinned unmodified jaraco/irc public API
 -> i2pr IRC client profile
 -> existing i2pr destination/Streaming path
 -> exact-pinned independent router
 -> independently hosted I2P IRC service
 -> ordinary loopback IRC fixture behind the reference service
```

The reference I2P side may expose the IRC fixture through a public documented service/tunnel API. The fixture may implement IRC behavior but must not implement I2P protocols.

`remote-independent-irc-service` becomes `passed` only if command-derived evidence proves all of:

```text
jaraco/irc exact commit verified and checkout clean
client installed/used through public `irc.client` API
reference router exact pin + clean state
reference service Destination created/hosted through public API
registration succeeds and welcome is received
PING/PONG proceeds normally
bidirectional PRIVMSG/message evidence succeeds
payload/token digest or exact bounded text-token evidence matches
ACTION remains allowed where retained profile requires it
DCC remains blocked by the i2pr privacy profile
client-supplied local hostname/address does not leak through profile
connection uses real remote mixed-router Streaming, not local-delivery fabric
clean QUIT/disconnect and resource baseline
```

Do not replace jaraco/irc with an in-tree raw IRC socket client for counted evidence.

## 17. Phase B.3 — evolve the Plan 181 checker instead of bypassing it

Update:

```text
scripts/check-service-tunnel-acceptance-evidence.sh
```

The final form must continue to statically reject:

- literal/unconditional pass assignments for any required row;
- a remote row passed without an executed command/exit-status gate;
- patched/vendored external router/client source;
- missing exact jaraco/i2pd pin verification;
- counted HTTP/IRC implemented by an in-tree shadow protocol client;
- clearnet DNS/IP/outproxy fallback;
- self-composed i2pr service mislabeled independent;
- remote rows missing/blocked while `milestone10_final_acceptance` is claimed;
- public-network dependence hidden as a localhost lane;
- private Destination keys, session keys, or raw sensitive payloads in retained evidence;
- cleanup/resource baseline omissions;
- `|| true`, skipped dependency, or optional-probe behavior that converts a required row into success.

The final checker should report a compact closure summary, e.g.:

```text
m10_local_rows: 29/29 passed
m10_remote_rows: 2/2 passed
m10_blocked: 0
m10_failed: 0
m10_missing: 0
m10_final_closure: passed
```

The exact local row count may change only if the existing ledger legitimately gains a new required command-derived row; do not drop prior rows to make the count easier.

## 18. Phase B.4 — external runner and workflow requirements

Update the existing runner rather than create another one:

```text
tests/integration/service-tunnels/run-independent.sh
```

A full final run must:

1. verify repository/head identity;
2. verify retained prerequisite status and M6 final closure evidence;
3. verify/fetch exact external references;
4. record curl/Python/nc/tool versions;
5. run all retained local M10 focused suites;
6. run the 29 retained independent-local rows;
7. provision the qualified independent I2P service host;
8. run remote HTTP and IRC application rows;
9. gather only sanitized evidence;
10. shut down all listeners, fixtures, reference routers, helpers, and child processes;
11. verify ports/resources return to baseline;
12. emit deterministic `evidence.json`, `evidence.md`, and row-oriented results;
13. exit nonzero if any required row is blocked, failed, or missing.

Update the existing workflow:

```text
.github/workflows/service-tunnels-external.yml
```

The final full lane must remain manual/hosted and fail closed. Preserve local-only mode as a cheaper diagnostic/regression lane if useful, but **local-only can never produce M10 final closure evidence**.

The full workflow must include an explicit M6 final-closure prerequisite check before the two remote service rows are accepted.

## 19. Resource/lifecycle closure requirements

The final M10 external run must demonstrate cleanup at the level already established by Plans 180/181 and the newly added remote service resources.

After shutdown require at least:

```text
active service listeners = 0
active service connections = 0
draining generations = 0
pending Streaming connects = 0
pending Streaming accepts = 0
pending local target connects = 0
protocol retained bytes = 0
reference router processes = 0
reference application/service helpers = 0
ephemeral loopback listener ports = closed
persistent server identity files = unchanged where expected
no leaked supervised child tasks in sanitized counters
```

A timeout/failed application row must take the same bounded cleanup path. Do not leave cleanup as an `if success` step.

## 20. Evidence and provenance contract

Final M10 evidence must be attributable to one exact candidate head.

At minimum retain:

```text
i2pr commit SHA
workflow run ID / execution ID
hosted runner/OS context
Rust toolchain
exact i2pd revision
exact Java revision for retained M6 prerequisite evidence
exact jaraco/irc revision
curl version
row label
row classification
command-derived status
bounded sanitized detail
resource-baseline result
```

Row classifications remain distinct:

```text
local-product
external-application-client
external-independent-i2p-service
```

Never infer a remote service row from local product evidence or infer Java M6 rows from i2pd evidence.

If final M6 and final M10 artifacts are generated by separate manual workflows, both must identify the same exact candidate commit. Plan 199 closure may not combine stale M6 evidence from one commit with M10 evidence from another.

## 21. Retry/repeatability policy

Deterministic protocol/auth/parser/policy failures are not retried until corrected.

A genuinely timing-sensitive external attempt may have a small named whole-attempt retry budget. Every attempt must use fresh ephemeral process/connection identifiers and be recorded. Never retry only the failing assertion until it happens to pass.

For final closure:

- at least one complete exact-head M6 two-family external workflow is mandatory;
- at least one complete exact-head M10 full service-tunnel external workflow is mandatory;
- if either final external lane shows any transient retry/flakiness on the candidate head, require **two complete clean passes on that same head** before closure.

Two clean runs are preferred even without observed flakiness but are not mandatory when both exact-head lanes are deterministic and pass first attempt.

## 22. Full validation floor on the final closing head

Run at least:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh

cargo deny check advisories bans sources
```

Plus the focused external/product lanes:

```text
bash tests/integration/m6-interop/run-m6-mixed-router.sh
bash tests/integration/service-tunnels/run-independent.sh --full
```

and the hosted manual workflows on the exact closing head:

```text
.github/workflows/m6-mixed-router-external.yml
.github/workflows/service-tunnels-external.yml
```

Routine CI on the exact closing head must pass. A green routine CI run is necessary but does not substitute for either external workflow.

## 23. Documentation and authority normalization

Do not mark M10 closed until source, evidence, status, and support documentation agree.

Update at least:

```text
plans/198-status.md
plans/195-status.md
plans/181-status.md
plans/199-status.md
plans/README.md
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
specs/support.toml
specs/CONFORMANCE.md
specs/protocols/11-service-tunnels.md
docs/architecture/i2pr-service-tunnels.md
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-client.md
docs/architecture/tooling.md
```

Normalize obsolete text that still says:

```text
Plan 181 remote rows are blocked
Plan 195 is blocked by Plan 198
Plan 198 is blocked at Java publication
next executable = 198
next product layer = M6 corrective
```

only after the underlying evidence has actually passed.

Do not rewrite historical plan narratives to pretend they never encountered blockers. Status/current-authority documents should point to the final evidence while historical files remain useful provenance.

## 24. Failure ownership and stop conditions

Downstream agents should stop at the first concrete boundary and fix only the owning layer.

### A. Java public client never creates/stores an LS2

Classification:

```text
java-client-leaseset-lifecycle-defect-or-harness-defect
```

Action: inspect public-client/tunnel lifecycle and exact Java logs. Do not add a floodfill topology until this is understood.

### B. Router A has a client LS2 but cannot publish it to Router B

Classification:

```text
java-reference-publication-topology/bootstrap-boundary
```

Action: correct ordinary RouterInfo/floodfill reachability/publication in the controlled reference topology. Do not patch Java or i2pr product code without evidence that i2pr is the failing protocol participant.

### C. Router B has the LS2 in its main NetDB but i2pr cannot resolve it

Classification:

```text
m6-netdb-interop-defect
```

Action: diagnose DatabaseLookup/DatabaseStore encoding, routing, return tunnel, validation, or store behavior in the existing M6 path.

### D. LS2 resolves but RAW destination bytes fail

Classification:

```text
m6-destination-garlic-delivery-defect
```

Action: stop before Streaming; correct destination/Garlic/ECIES/Data routing only.

### E. RAW bidirectional destination passes but Streaming fails

Classification:

```text
m6-streaming-interop-defect
```

Action: correct the single existing Streaming implementation; do not add a Java-specific codec/stack.

### F. Full M6 passes, HTTP remote row fails

Classification:

```text
m10-http-application-profile-or-fixture-defect
```

Action: inspect HTTP proxy/service composition. Do not reopen M6 unless the retained M6 lane independently reproduces the lower-layer failure.

### G. Full M6 passes, IRC remote row fails

Classification:

```text
m10-irc-application-profile-or-fixture-defect
```

Action: inspect IRC filter/profile/service composition and independent client behavior. Do not reopen M6 without a lower-layer reproduction.

### H. A pass would require any prohibited shortcut

Stop Plan 199 rather than weakening closure if success would require:

```text
patching exact-pinned reference routers/clients
private Java/i2pd NetDB or tunnel-state injection
copying a reference LS2 directly into i2pr
hard-coded evidence rows
self-composed peer mislabeled independent
clearnet DNS/outproxy fallback
public-network participation merely to make the test green
LocalZeroHop/direct SSU2 counted on the i2pr side
secret/private-key/raw-sensitive-payload retention
skipping an unavailable mandatory dependency while returning success
```

## 25. Explicit final acceptance criteria

Plan 199 passes, and Milestone 10 closes, **only** when every item below is true on one exact candidate head.

1. Plans 174–180 and 182 retained local product evidence remains green; no local M10 implementation layer is reopened or silently weakened.
2. All 29 retained Plan 181 independent-local application rows remain command-derived `passed` (or an explicitly documented superset if new mandatory rows were added without dropping old coverage).
3. Exact-pinned i2pd 2.61.0 first-family M6 evidence remains all-pass.
4. Exact-pinned Java I2P 2.13.0 second-family topology uses public client APIs and unmodified source/cache.
5. The Java client LeaseSet lifecycle is observed through sanitized evidence; helper readiness is not treated as publication proof.
6. The Java publication topology provides a legitimate distinct network-visible floodfill/main-NetDB target without private NetDB injection or VMComm bypass.
7. RouterInfo/bootstrap knowledge required by that topology is established through ordinary protocol paths, not direct Java state mutation.
8. The Java public-client Standard LS2 becomes network-visible in the publication router main NetDB through ordinary publication behavior.
9. i2pr resolves that LS2 through its real remote `DatabaseLookup`/return-tunnel path.
10. i2pr -> Java and Java -> i2pr raw Destination payloads pass with exact length/digest equality through real counted tunnels.
11. Both Streaming directions against Java pass the mandatory establishment, small/multi-packet data, reverse data, sibling-isolation, close/EOF, and cleanup rows enforced by the Plan 198 final checker.
12. No counted i2pr Java row uses direct destination-over-SSU2 or `LocalZeroHop` substitution.
13. `scripts/check-m6-final-closure-evidence.sh` reports both families all-pass with zero mandatory blocked/failed/missing rows.
14. An exact-head `M6 mixed-router external interoperability` workflow run succeeds with exact reference pins and clean-cache provenance.
15. `remote-independent-http-eepsite` is command-derived `passed` using ordinary unmodified curl through the i2pr HTTP proxy to an independently hosted I2P HTTP service, including exact body digest evidence and no clearnet fallback.
16. The remote HTTP row also proves one meaningful request-body/large-response path and retains the Plan 176 privacy policy.
17. `remote-independent-irc-service` is command-derived `passed` using exact-pinned unmodified jaraco/irc through the i2pr IRC client profile to an independently hosted I2P IRC service.
18. The remote IRC row proves registration/welcome, PING/PONG, bidirectional messaging, ACTION policy, DCC rejection, and clean disconnect.
19. The final service-tunnel acceptance checker treats both remote rows as mandatory passes and rejects blocked/failed/missing rows or literal passes.
20. The full Plan 181 local matrix and both remote rows coexist in one deterministic final M10 evidence ledger attributable to the exact closing head.
21. All service listeners/connections, Streaming pending work, draining generations, local target connects, reference processes/helpers, and ephemeral ports return to the required clean baseline.
22. No exact-pinned external source is patched; reference source/cache cleanliness is proven.
23. No evidence artifact contains private Destination material, session keys, or raw sensitive application payloads.
24. The full workspace/static/dependency validation floor in §22 passes on the exact closing head.
25. Routine CI passes on the exact closing head.
26. The exact-head manual service-tunnel external workflow passes in full mode.
27. If either final external workflow required a transient retry, two complete clean runs of that workflow pass on the same head.
28. `plans/198-status.md` records the actual Java M6 all-pass closure rather than the old publication blocker.
29. `plans/181-status.md` and `plans/195-status.md` record the two remote M10 rows as passed from command-derived evidence.
30. `plans/199-status.md`, `plans/README.md`, README/support/conformance/architecture documentation all agree on the bounded final M10 claim.
31. Unsupported/deferred behavior remains explicitly unsupported; closure does not claim public-network readiness or unsupported application/router roles.
32. There are zero known mandatory `blocked`, `failed`, or `missing` M10 final-acceptance rows.

Any failure of items 1–32 means Plan 199 is not closed.

## 26. Closure transition

Only after §25 is satisfied, update authority to:

```text
plan_193 = retained-passed-m6-i2pd-mixed-router-streaming
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = passed-m6-java-public-client-complete-second-family-closure

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = passed-via-plan198
milestone6_interoperable = passed-via-plan193-and-plan198

plan_181 = passed-m10-independent-application-service-interop-final-closure-evidence
plan_195 = passed-m10-remote-independent-service-final-closure
plan_199 = passed-m10-unified-final-closure

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = passed
milestone10_remote_service_interop = passed-via-plan195-under-plan199
milestone10_final_acceptance = closed-via-plan199

next_executable_plan = none-at-m10-layer
next_product_layer = milestone11-planning
```

Plan 199 should be the single final M10 closure reference in current-authority documentation. Earlier plans remain linked as detailed provenance for the individual implementation layers and corrective discoveries.

## 27. Handoff rule

Execute **Plan 199** as the remaining milestone program.

Do not ask downstream agents to independently reconstruct or serially “execute Plans 198 then 195” from historical prose. Their still-valid requirements are incorporated here:

```text
Plan 198 details -> Phase A / final two-family M6 prerequisite
Plan 195 details -> Phase B / two remote M10 application rows
Plan 181 details -> retained local independent-client matrix + evidence framework
Plan 173 details -> bounded M10 product/non-goals
```

Keep changes narrowly owned by the first failing boundary. The desired result is not more planning artifacts; it is a single exact-head evidence set that closes the remaining Java-family substrate, proves the two remote application paths, and ends Milestone 10.
