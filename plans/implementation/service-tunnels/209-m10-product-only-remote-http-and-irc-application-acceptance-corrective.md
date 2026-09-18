# Plan 209 — M10 product-only remote HTTP/IRC acceptance corrective

Status: **registered / blocked on Plan 208**.

## 1. Purpose

Plan 209 corrects the remaining acceptance-harness defect after Plan 207.

Plan 207 improved the application layer substantially: it introduced real system `curl` invocations, exact-pinned jaraco/irc public-API subprocesses, fixture-derived application facts, and aggregate rows that no longer depend on `record_remote_application_observation`.

However, the counted driver still imports and directly drives a separate destination/tunnel/Streaming/router stack (`StreamingManager`, `StreamingDestinationAdapter`, `DestinationTunnelCoordinator`, exploratory-build state, router-delivery state) beside the `ServiceTunnelManager`. It also manually advances legacy remote transport counters. This can make application evidence look complete while the product listeners still do not own the remote network path.

Plan 209 removes that shadow stack. The acceptance driver becomes a black-box product client/fixture harness over the production path established by Plan 208.

## 2. Preconditions

Do not execute Plan 209 as a pass attempt until Plan 208 is passed on the candidate head.

Required authority before execution:

```text
plan_208 = passed-m10-production-delivery-driver-remote-route-integration
m10_remote_transport_core = passed-via-plan208
```

If Plan 208 is not passed, Plan 209 stays blocked.

## 3. Scope

Primary surfaces:

- `crates/i2pr-daemon/tests/service_tunnels_application_genuine_remote_qualification.rs` or its replacement
- `tests/integration/service-tunnels/run-independent.sh`
- `tests/integration/service-tunnels/clients/irc_driver.py`
- existing HTTP/IRC fixtures
- `scripts/check-service-tunnel-acceptance-evidence.sh`
- minimal production composition helper only if required to boot the same product stack in the test

No protocol implementation should move into the test harness.

## 4. Non-goals

Plan 209 does **not**:

- modify HTTP proxy semantics unless a genuine product bug is observed;
- modify IRC filtering semantics unless a genuine product bug is observed;
- add another router stack to the harness;
- add handwritten substitutes for curl or jaraco/irc;
- patch i2pd;
- use the public I2P network;
- close the Java branch;
- perform final Plan 204 authority convergence.

## 5. Product-only harness rule

The counted i2pr side must be the product path only.

The Plan 209 driver must **not** construct or directly drive:

- `StreamingManager`,
- `StreamingDestinationAdapter`,
- `DestinationRouting`,
- `EciesSessionManager`,
- `DestinationTunnelCoordinator`,
- `ExploratoryBuildCoordinator`,
- `Ssu2DaemonService`,
- `RouterDeliveryService`,
- direct `RouterDeliveryRequest`s,
- manual TunnelData/Garlic cells.

It must not define a helper equivalent to the current `send_transport_request()` that manually composes and sends the remote stream.

It must not call `record_observation` or another free-form counter API to synthesize transport success.

The only acceptable lower-stack construction is through one production daemon composition function that Plan 208 also uses in real product code. The test may hold the returned product handle so it can start/stop the controlled instance and read sanitized counters.

## 6. Required topology

Reuse the exact-pinned i2pd 2.61.0 controlled loopback lane.

Peer side:

- one ephemeral i2pd router;
- i2pd SAM bridge used to create/hold independent HTTP and IRC service Destinations;
- i2pd server tunnels point to harness-owned loopback HTTP and IRC fixtures;
- exact router pin and source revision verified clean;
- no public-network participation.

Product side:

- one normal i2pr daemon/service-tunnel composition with Plan 208 remote backend installed through production composition;
- real HTTP client listener from the configured service manager;
- real IRC client listener from the configured service manager;
- no second i2pr destination/tunnel stack in the acceptance test.

## 7. Phase A — delete the shadow-network path from the Plan 207 driver

Refactor or replace `service_tunnels_application_genuine_remote_qualification.rs`.

Remove direct imports/use of the shadow-stack types listed in §5.

Remove helpers that:

- build exploratory tunnels in the application driver;
- manually populate destination routing;
- manually construct ECIES/Streaming adapter state;
- manually call `StreamingDestinationAdapter::send`;
- manually encode outbound cells;
- manually submit `RouterDeliveryRequest`s;
- manually drain a test-owned `StreamingManager`;
- manually advance `remote_stream_established`, `remote_outbound_requests`, or `remote_inbound_payloads`.

If the existing test becomes very small after removal, that is desirable.

The driver should primarily:

1. obtain/start the production product instance;
2. obtain actual bound HTTP and IRC listener addresses;
3. run external application clients;
4. read fixture facts;
5. read operation-derived Plan 208 counters;
6. emit sanitized evidence;
7. stop the product.

## 8. Phase B — use production operation-derived backend proof

Replace the current `plan206-backend-counters` proof based on manually advanceable legacy counters.

The Plan 209 product proof must depend on Plan 208 operation-boundary facts from the actual manager delivery driver, including at minimum:

- `remote_outbound_composed > 0`,
- `remote_inbound_dispatched > 0`,
- ordinary remote LS2 resolution/cache success observed from production,
- `local_coowned_deliveries == 0` for the counted remote connections,
- no terminal reachable-peer `unknown_peer` result.

If additional typed counters are introduced by Plan 208 for ordinary lookup completion or remote send success, use them.

The acceptance driver may **read** these counters. It may not advance them.

## 9. Phase C — HTTP acceptance through real curl

Use the real system `curl` executable as a subprocess against the actual i2pr HTTP proxy listener.

Record the curl version.

Mandatory cases:

### C.1 GET

```text
curl -> i2pr HTTP listener -> I2P remote service -> i2pd server tunnel -> HTTP fixture
```

Require:

- curl exit 0;
- expected response status;
- expected body digest;
- fixture confirms the GET path;
- Plan 208 remote operation counters advance during the same run.

### C.2 POST body

Require:

- curl exit 0;
- fixture receives expected method/path/body digest;
- response digest matches expected fixture response;
- no body truncation.

### C.3 multi-packet/large response

Use a body large enough to require multiple Streaming packets.

Require complete digest equality and no truncation.

### C.4 clearnet/no-outproxy policy

Attempt a prohibited/non-`.i2p` target through the configured HTTP listener.

Require the documented local policy rejection and prove the HTTP fixture/i2pd remote path was not used for that prohibited destination.

### C.5 privacy/policy retention

Retain the existing HTTP privacy/header rules. Record only bounded facts/digests, never raw sensitive peer material.

## 10. Phase D — IRC acceptance through exact-pinned jaraco/irc

Use the exact-pinned clean jaraco/irc checkout and its public API (`irc.client`), invoked through the existing Python driver or a smaller equivalent using that package.

Do not replace it with a handwritten socket client for counted rows.

Record:

- exact jaraco/irc git revision;
- Python version;
- subprocess exit code.

Mandatory cases:

1. connection establishes through the actual i2pr IRC listener;
2. registration reaches fixture and client observes numeric welcome;
3. fixture PING causes expected PONG behavior;
4. outbound PRIVMSG reaches fixture;
5. fixture-generated inbound PRIVMSG reaches client;
6. CTCP ACTION allowed according to current policy;
7. DCC attempt blocked according to current policy;
8. hostname/privacy rewrite rules remain in force;
9. QUIT/clean shutdown completes;
10. Plan 208 remote operation counters advance during the same run.

Application semantics must come from jaraco/irc events and fixture facts, not from manager observation labels.

## 11. Phase E — evidence model

The authoritative remote application rows are:

```text
remote-independent-http-eepsite
remote-independent-irc-service
```

Each aggregate row must be derived from mandatory same-run subfacts.

### HTTP mandatory subfacts

At minimum:

- `http-client-command=system-curl`
- `http-command-exit=0`
- `http-get-status-ok=1`
- `http-body-digest=<64 hex>`
- `http-post-fixture-observed=1`
- `http-post-body-digest-match=1`
- `http-multipacket-digest=<64 hex>`
- `http-no-clearnet-fallback=1`
- `http-production-remote-outbound=1`
- `http-production-remote-inbound=1`
- `http-local-coowned-not-used=1`
- `http-clean-resource-baseline=1`

### IRC mandatory subfacts

At minimum:

- `irc-client-library=jaraco/irc`
- `irc-command-exit=0`
- `irc-registration-welcome=1`
- `irc-ping-pong-roundtrip=1`
- `irc-privmsg-outbound-observed=1`
- `irc-privmsg-inbound-observed=1`
- `irc-ctcp-action-allowed=1`
- `irc-dcc-blocked=1`
- `irc-privacy-hostname-rewrite=1`
- `irc-quit-clean=1`
- `irc-production-remote-outbound=1`
- `irc-production-remote-inbound=1`
- `irc-local-coowned-not-used=1`
- `irc-clean-resource-baseline=1`

The exact key names may be adapted to existing conventions, but the semantics must remain command/fixture/product-derived.

## 12. Phase F — runner behavior

`tests/integration/service-tunnels/run-independent.sh` must remain fail-closed.

### Local-only/routine lane

- retained local 29-row matrix runs;
- remote application rows are explicitly `blocked`/not counted as passed when exact-pinned remote prerequisites are absent;
- no synthetic fallback.

### Full external lane

- verifies/stages exact i2pd and jaraco pins;
- starts fixtures;
- starts exact-pinned i2pd controlled topology;
- starts one product i2pr service-tunnel instance using Plan 208 composition;
- runs curl and jaraco cases;
- aggregates evidence only after commands return;
- any mandatory missing/failed subfact fails the lane;
- no retry that silently changes topology or evidence semantics.

A full lane can be one authoritative exact-head execution. Repeat only if an actual nondeterminism/flakiness issue needs diagnosis; do not make repeated runs a new mandatory architecture layer.

## 13. Phase G — static anti-cheat rules

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` so the counted Plan 209 driver fails static validation if it contains direct references to the shadow network stack.

Reject counted-driver imports/calls for:

- `StreamingManager`,
- `StreamingDestinationAdapter`,
- `DestinationTunnelCoordinator`,
- `ExploratoryBuildCoordinator`,
- `Ssu2DaemonService`,
- `RouterDeliveryRequest`,
- direct `send_transport_request`-style helper,
- `record_remote_application_observation`,
- `.record_observation(`.

Allow production composition helpers that return a fully wired product instance without exposing these internals to the acceptance driver.

Also require:

- real curl subprocess invocation;
- exact-pinned jaraco/irc invocation;
- actual manager listener addresses;
- fixture fact consumption;
- Plan 208 operation-derived counter reads;
- same-run evidence directory/run id.

## 14. Phase H — clean up stale Plan 207 scaffolding

Once Plan 209 passes:

- retain Plan 207 as historical evidence that real application clients were introduced;
- remove or clearly mark the old shadow-stack application driver path as non-authoritative;
- delete dead acceptance-only helpers if nothing else uses them;
- do not leave two competing remote application drivers both claiming authority;
- correct stale implementation-head references in status files.

Do not delete reusable peer fixture code merely because it originated in Plan 207.

## 15. Acceptance criteria

Plan 209 passes only when **all** are true on one exact head:

1. Plan 208 is already passed on that head/ancestor with unchanged production semantics.
2. Counted application driver contains no parallel i2pr Streaming/router stack.
3. Counted driver contains no direct `StreamingDestinationAdapter::send`.
4. Counted driver contains no manual remote transport success counter increments.
5. HTTP uses actual system curl against actual i2pr listener.
6. HTTP GET succeeds and fixture/body facts agree.
7. HTTP POST succeeds and fixture/request-body facts agree.
8. HTTP multi-packet/large body digest agrees.
9. HTTP clearnet/outproxy fallback remains prohibited.
10. HTTP counted connection has operation-derived Plan 208 outbound evidence.
11. HTTP counted connection has operation-derived Plan 208 inbound evidence.
12. IRC uses exact-pinned clean jaraco/irc public API against actual i2pr listener.
13. IRC registration/welcome passes.
14. IRC PING/PONG passes.
15. IRC outbound and inbound PRIVMSG pass.
16. CTCP ACTION policy passes.
17. DCC remains blocked.
18. IRC privacy/hostname rewrite remains enforced.
19. IRC clean QUIT/shutdown passes.
20. IRC counted connection has operation-derived Plan 208 outbound evidence.
21. IRC counted connection has operation-derived Plan 208 inbound evidence.
22. Neither counted remote application path uses local co-owned delivery.
23. No reachable counted peer terminates as `unknown_peer`.
24. Fixtures observe the expected application traffic.
25. Full external lane has zero mandatory blocked/failed/missing rows.
26. Local retained rows remain green.
27. Exact i2pd and jaraco revisions are verified clean.
28. Routine CI/static/dependency checks remain green.
29. Status/README authority does not claim final M10 closure before Plan 204 convergence.

Only then set:

```text
plan_207 = retained-partial-real-client-harness-superseded-by-plan209
plan_209 = passed-m10-product-only-remote-http-and-irc-application-interop
plan_181 = passed-m10-independent-application-and-service-interop-via-plan209
plan_195 = evidence-passed-m10-remote-independent-service-via-plan208-and-plan209-pending-plan204
milestone10_remote_service_interop = evidence-passed-via-plan208-and-plan209-pending-plan204
```

Plan 209 does **not** itself set `milestone10_final_acceptance = closed`. Plan 204 owns final convergence.

## 16. Stop conditions

Stop and record the first real product boundary if:

- curl cannot establish because the product delivery driver still does not route remotely;
- jaraco cannot establish for the same reason;
- remote inbound traffic reaches router/tunnel code but cannot be mapped into the owning service runtime;
- product-derived counters do not line up with observed application traffic;
- a pass appears to require reintroducing the shadow stack.

If any of these occur, do not restore the Plan 207 parallel stack. Fix the production boundary or register one narrower corrective based on the evidence.

## 17. Handoff

Plan 209 is blocked on Plan 208.

Execution graph:

```text
Java: 205 -> evidence-driven successor if required
M10:  208 -> 209
Final: closed Java branch + passed 209 -> 204 convergence
```
