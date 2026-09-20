# Plan 228 — M6 Java client-tunnel build-path attribution

Status: **registered-ready-m6-java-client-tunnel-build-path-attribution**

## 1. Objective

Close the exact Plan-227 build boundary:

```text
P227-EXPLICIT-ONE-HOP-NOT-BUILT
```

by identifying the earliest missing stage in stock Java I2P's one-hop client
tunnel construction path for the raw helper.

Plan 228 is attribution-only. It MUST NOT add a tunnel-building workaround,
mutate peer profiles, alter exploratory/client tunnel policy, patch Java, or
change i2pr production behavior.

The result must distinguish at least:

1. explicit Router-C peer selection never produces a client tunnel config;
2. a client tunnel config exists but no paired tunnel is available;
3. a build request is created but never dispatched;
4. an outbound build request is dispatched but Router C does not receive it;
5. Router C receives the request but cannot decrypt/process it;
6. Router C processes it and explicitly rejects it;
7. Router C accepts/forwards a reply but Router A does not receive it;
8. Router A receives a reply but cannot decrypt/validate it;
9. reply succeeds but local tunnel join/install fails;
10. a one-hop tunnel installs and Plan 227's five-minute NOT-BUILT result was
    caused by a different pool/direction/qualification mismatch.

The plan stops at the earliest proven missing stage.

## 2. Retained authority

Retain unchanged:

- Java I2P 2.13.0:
  `9134f808337b401e8e53c73734c81fab04280c9d`;
- i2pd 2.61.0:
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- Plan 223 Destination identity / LS2 separation;
- Plan 224 Router-B query-answerable target LS2;
- Plan 225 exact helper-search attribution;
- Plan 226 exact
  `P226-BASELINE-B-ZERO-HOP-UNKNOWN` boundary;
- Plan 227:
  - Router C present and valid in Router A main NetDB;
  - `ProfileOrganizer.isSelectable(C)=true`;
  - Router C not banlisted;
  - stock I2CP `inbound.explicitPeers` /
    `outbound.explicitPeers` scoped to the raw helper;
  - helper requests one-hop inbound/outbound tunnels;
  - zero-hop disabled;
  - three counted exact-head attempts each reached Java's own approximately
    five-minute `I2PSession.connect()` ceiling;
  - no installed one-hop client tunnel was observed;
  - terminal `P227-EXPLICIT-ONE-HOP-NOT-BUILT`.

M6 Java interoperability remains unclaimed.

## 3. Exact-pinned Java build pipeline

Pinned Java source establishes the normal path.

### 3.1 Pool configuration

`TunnelPool.configureNewTunnel()` calls the pool peer selector. For the
Plan-227 client profile, the configured peer list must include Router C and the
local router, yielding the normal Java creator config. A null/empty selection
returns no build config.

### 3.2 Build executor

`BuildExecutor` repeatedly asks pools how many tunnels they need. For
non-zero-hop builds it will not proceed unless the router has usable tunnel
infrastructure:

```text
getFreeTunnelCount() > 0
getOutboundTunnelCount() > 0
```

Otherwise it tries to kick-start exploratory tunnel construction and waits.

This stage MUST be distinguished from client peer selection.

### 3.3 Paired tunnel requirement

`BuildRequestor.request()` always uses paired tunnels for client builds.

For an inbound client tunnel, Java needs an outbound paired tunnel to carry
the build request.

For an outbound client tunnel, Java needs an inbound paired tunnel for the
build reply path.

When the client pool has no opposite-direction tunnel yet, Java falls back to
a suitable router/exploratory tunnel. If none is available:

```text
Tunnel build failed, as we couldn't find a paired tunnel for <cfg>
```

and the build completes as `OTHER_FAILURE` before Router C receives a build
request.

This is a first-class Plan-228 hypothesis because Plan 227 proved C selectable
but observed no build success/reject/timeout strings in its narrow whitelist.

### 3.4 Outbound-client build dispatch

For an outbound client tunnel, after message construction Java logs:

```text
Sending the tunnel build request directly to <first-hop>
...
via IB tunnel <pairedTunnel>
```

then sends the build request to Router C via `OutNetMessagePool`.

The send carries a `TunnelBuildFirstHopFailJob`; a first-hop delivery
failure completes the build as `OTHER_FAILURE` and increments
`tunnel.buildFailFirstHop`.

### 3.5 Inbound-client build dispatch

For an inbound client tunnel, Java sends the build request through the paired
outbound tunnel to the inbound gateway. The request may be wrapped for the
gateway as required by the stock Java path.

### 3.6 Router-C processing

`BuildHandler` / `BuildMessageProcessor` on Router C:

- receive/dequeue the build request;
- decrypt the record intended for C;
- resolve the next hop if required;
- determine the response code;
- emit the reply/forward path.

Pinned debug output contains the bounded discriminator:

```text
Read slot <n> ... accepted? <response> ...
```

The plan may sanitize only the acceptance code/result, never the full request
record.

### 3.7 Router-A reply handling

A matching reply reaches `BuildHandler.handleReply()`, which:

- matches `replyMessageId` to the pending creator config;
- decrypts via `BuildReplyHandler`;
- records each remote status;
- calls `TunnelDispatcher.joinInbound()` or `joinOutbound()`;
- completes the build as `SUCCESS`, `REJECT`, `BAD_RESPONSE`, or
  `DUP_ID`.

If no reply arrives within the build request timeout, `BuildExecutor` logs:

```text
Timed out waiting for reply asking for <cfg>
```

and records a client build expiry.

## 4. Scope

Allowed production changes: **none**.

Allowed test-only changes:

- read-only Java diagnostics under
  `tests/integration/m6-interop/java/`;
- scratch-only targeted Java logger configuration;
- `run-java.sh` bounded evidence extraction/classification;
- `java_tunnel_external.rs` focused Plan-228 classification tests;
- static acceptance-evidence guards;
- planning/closure documents.

Do not execute or modify the Streaming helper in Plan 228.

## 5. Invariants

1. No Java source patch.
2. No reflection/private-state mutation.
3. No profile score/tier mutation.
4. No RouterInfo insertion into client NetDB.
5. No direct tunnel installation.
6. No VMComm.
7. No `netDb.alwaysQuery`.
8. No public I2P.
9. No Plan-226 distinct-loopback topology.
10. Plan-227 raw-helper one-hop profile remains unchanged.
11. Router C remains the sole explicit client-tunnel peer.
12. Router B remains the target LS2 publication/floodfill role.
13. No tunnel length/quantity/variance changes.
14. No exploratory tunnel settings changes.
15. No paired-tunnel policy changes.
16. No build timeout changes.
17. No helper five-minute ceiling changes.
18. No reverse-delivery 45-second timeout changes.
19. No Java random seeding or explicit-peer probability patch.
20. Raw Java logs remain disposable scratch.
21. Durable evidence may contain only bounded booleans, counters, direction,
    hashes, message/reply IDs where safe, response/status codes, and elapsed
    timings.
22. The plan stops on the first proven missing stage and does not repair it.

## 6. Work package A — pre-build tunnel infrastructure snapshot

Before starting the Plan-227 raw helper, add a read-only diagnostic on Router A:

```text
P228-TUNNEL-INFRA
```

Return bounded facts:

```text
free_tunnel_count=<n>
inbound_tunnel_count=<n>
outbound_tunnel_count=<n>
inbound_exploratory_count=<n>
outbound_exploratory_count=<n>
inbound_exploratory_nonzero_count=<n>
outbound_exploratory_nonzero_count=<n>
```

Use public tunnel-manager/pool accessors only.

Do not expose peer paths.

Repeat the snapshot periodically only through the existing bounded harness
polling epoch and once at helper timeout. Do not cause a build.

This establishes whether `BuildExecutor` had the prerequisite router tunnel
infrastructure required to attempt non-zero-hop client builds.

## 7. Work package B — client-pool configuration/selection evidence

Extend the existing Plan-227 scratch logger for:

- `TunnelPool`;
- `TunnelPeerSelector`;
- `ClientPeerSelector`;
- `BuildExecutor`.

For the exact raw-helper destination only, sanitize:

```text
inbound_pool_requested_build=<bool>
outbound_pool_requested_build=<bool>
inbound_config_created=<bool>
outbound_config_created=<bool>
inbound_config_contains_c=<bool>
outbound_config_contains_c=<bool>
inbound_config_null=<bool>
outbound_config_null=<bool>
build_executor_attempted_inbound=<bool>
build_executor_attempted_outbound=<bool>
build_executor_blocked_no_router_tunnels=<bool>
```

Exact Router-C identity correlation is required for
`*_config_contains_c=true`.

Do not infer config creation from selector log presence alone.

## 8. Work package C — paired-tunnel attribution

Extend the scratch-only `BuildRequestor` logger and sanitizer.

For each direction, record:

```text
paired_tunnel_selected=<bool>
paired_tunnel_kind=client|exploratory|none|unknown
paired_tunnel_zero_hop=<bool>
paired_tunnel_nonzero=<bool>
build_message_created=<bool>
other_failure_no_paired_tunnel=<bool>
other_failure_message_create=<bool>
```

No paired tunnel path/peer list may enter durable evidence.

Authoritative earliest-stage terminal if either direction has a config but
cannot obtain the paired tunnel it requires:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=<inbound|outbound|both>
```

If no client config is ever created, use the earlier selection/executor
terminal instead.

## 9. Work package D — build request dispatch evidence

For each client direction, record whether a stock build request leaves Router A.

Outbound client build:

```text
outbound_build_direct_dispatch_to_c=<bool>
outbound_build_first_hop_failure=<bool>
```

Inbound client build:

```text
inbound_build_dispatch_via_paired_tunnel=<bool>
inbound_build_target_c=<bool>
```

Correlate with:

- raw-helper destination;
- Router-C hash;
- creator-config direction;
- bounded Java message/reply ID where available.

Do not classify a selector/config event as a dispatch.

If a message was created but no dispatch occurs:

```text
P228-ATTRIBUTION-BUILD-CREATED-NOT-DISPATCHED direction=<...>
```

If outbound direct send invokes the first-hop fail path:

```text
P228-ATTRIBUTION-FIRST-HOP-DELIVERY-FAILURE
```

## 10. Work package E — Router-C build request handling

Enable targeted scratch logging on Router C for:

- `BuildHandler`;
- `BuildMessageProcessor`.

Sanitize per exact A/C build epoch:

```text
c_build_request_received=<bool>
c_build_record_decrypted=<bool>
c_build_next_hop_known=<bool>
c_build_response_seen=<bool>
c_build_response_code=<integer|none>
c_build_accepted=<bool>
c_build_rejected=<bool>
c_build_forward_or_reply_seen=<bool>
```

Never retain:

- decrypted request records;
- layer/reply keys;
- tunnel IDs beyond a bounded correlation identifier;
- next-hop RouterInfo contents;
- peer path lists.

If A dispatches but C receives nothing:

```text
P228-ATTRIBUTION-A-DISPATCHED-C-NOT-RECEIVED
```

If C receives but cannot decrypt:

```text
P228-ATTRIBUTION-C-DECRYPT-FAILURE
```

If C explicitly rejects:

```text
P228-ATTRIBUTION-C-REJECTED code=<n>
```

## 11. Work package F — Router-A reply handling

Sanitize Router A's `BuildHandler` / `BuildReplyHandler` events:

```text
a_matching_reply_received=<bool>
a_reply_pending_match=<bool>
a_reply_decrypted=<bool>
a_remote_status_seen=<bool>
a_remote_status_code=<integer|none>
a_join_attempted=<bool>
a_join_success=<bool>
a_build_complete_result=SUCCESS|REJECT|TIMEOUT|BAD_RESPONSE|DUP_ID|OTHER_FAILURE|none
```

If C accepted/forwarded a reply but A never receives a matching reply:

```text
P228-ATTRIBUTION-REPLY-NOT-RETURNED
```

If A receives but cannot decrypt:

```text
P228-ATTRIBUTION-REPLY-DECRYPT-FAILURE
```

If A receives an explicit remote reject:

```text
P228-ATTRIBUTION-REMOTE-REJECT code=<n>
```

If all remote statuses agree but local join fails:

```text
P228-ATTRIBUTION-LOCAL-JOIN-FAILURE
```

## 12. Work package G — timeout/result counters

At helper timeout, collect a final read-only bounded snapshot for the exact
raw-helper client pools:

```text
client_build_success_count=<n>
client_build_reject_count=<n>
client_build_expire_count=<n>
first_hop_fail_count=<n>
inbound_in_progress=<n>
outbound_in_progress=<n>
inbound_installed=<n>
outbound_installed=<n>
```

Prefer exact pool/read-only state where available. Java global rate/stat values
may be used only as corroboration and MUST NOT override direction-specific
trace evidence.

If the only authoritative result is a pending request that expires without C
or A reply evidence:

```text
P228-ATTRIBUTION-BUILD-REPLY-TIMEOUT direction=<...>
```

## 13. Terminal taxonomy

Exactly one authoritative final Plan-228 terminal.

### Earliest pre-dispatch stages

```text
P228-ATTRIBUTION-NO-ROUTER-TUNNEL-INFRA
P228-ATTRIBUTION-NO-CLIENT-CONFIG direction=<...>
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=<...>
P228-ATTRIBUTION-BUILD-MESSAGE-CREATE-FAILURE direction=<...>
P228-ATTRIBUTION-BUILD-CREATED-NOT-DISPATCHED direction=<...>
P228-ATTRIBUTION-FIRST-HOP-DELIVERY-FAILURE
```

### Remote processing stages

```text
P228-ATTRIBUTION-A-DISPATCHED-C-NOT-RECEIVED
P228-ATTRIBUTION-C-DECRYPT-FAILURE
P228-ATTRIBUTION-C-REJECTED code=<n>
P228-ATTRIBUTION-REPLY-NOT-RETURNED
```

### Reply/install stages

```text
P228-ATTRIBUTION-BUILD-REPLY-TIMEOUT direction=<...>
P228-ATTRIBUTION-REPLY-DECRYPT-FAILURE
P228-ATTRIBUTION-REMOTE-REJECT code=<n>
P228-ATTRIBUTION-LOCAL-JOIN-FAILURE
```

### If the assumed boundary disappears

If a genuine inbound and outbound one-hop client tunnel through C is installed
during the diagnostic run:

```text
P228-NEXT-BOUNDARY-CLIENT-TUNNELS-BUILT
```

Then stop Plan 228. Do not continue into the Plan-226 lookup/reverse-delivery
lane inside this attribution plan; a successor/requalification plan owns that
behavioral continuation.

### Evidence failure

```text
P228-OBSERVABILITY-GAP-BUILD-PATH
```

## 14. Correlation rules

The sanitizer must fail closed unless it can correlate events to the Plan-227
raw helper and Router C.

Use, where available:

- raw-helper destination/client DBID;
- Router-C exact hash;
- pool direction;
- creator-config destination;
- build/reply message ID;
- attempt epoch.

A raw log line outside that correlated epoch MUST NOT set a Plan-228 fact.

If an exact build/reply ID cannot safely be derived for a stage, use the
destination + direction + Router-C tuple and mark correlation strength
explicitly.

## 15. Focused tests and static guards

Add focused tests for:

1. no router tunnel infrastructure;
2. selector activity without client config;
3. config created through C but no paired tunnel;
4. message created but not dispatched;
5. outbound first-hop failure;
6. A dispatch / C receive absence;
7. C receive / decrypt failure;
8. C explicit reject code;
9. C accept / reply absent at A;
10. A reply decrypt failure;
11. A remote rejection;
12. local join failure;
13. build reply timeout;
14. inbound-only successful install is not enough to claim both tunnels built;
15. outbound-only successful install is not enough;
16. both one-hop client tunnels installed maps only to
    `P228-NEXT-BOUNDARY-CLIENT-TUNNELS-BUILT`;
17. unrelated exploratory build logs cannot classify the client build;
18. unrelated destinations cannot classify the raw helper;
19. raw secret/key/request-record lines are rejected from durable evidence;
20. exactly one terminal emitted;
21. earliest-stage precedence is deterministic.

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` to reject:

- Java/profile/NetDB/tunnel mutation;
- exploratory/client tunnel policy changes;
- `router.usePairedTunnels` overrides;
- `netDb.alwaysQuery`;
- VMComm;
- public/non-loopback topology changes;
- build timeout changes;
- Plan-227 helper profile changes;
- Plan-226 distinct topology;
- raw Java log promotion;
- a Plan-228 terminal without required upstream stage evidence.

## 16. Execution policy

Commit implementation before counted execution.

Require:

```bash
git status --porcelain=v1
git rev-parse HEAD
```

with an empty working tree.

Run destination only:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

Maximum three counted attempts per implementation SHA.

Each counted attempt:

- fresh disposable A/B/C RouterContexts;
- same baseline loopback topology;
- same raw-helper Plan-227 SessionConfig;
- same Router-C explicit peer derivation;
- same Java/i2pd pins;
- same five-minute helper ceiling;
- no parameter tuning between attempts.

A single counted run may terminate early as soon as the earliest missing stage
is conclusively classified; it does not need to consume the full five minutes
after an exact fail-fast branch is proven.

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

cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
```

## 18. Acceptance criteria

Plan 228 closes only if:

1. reference pins remain unchanged;
2. Plan-227 raw-helper profile remains unchanged;
3. no production Rust code changes;
4. Router C selectability remains proven or any contradiction is recorded;
5. router/exploratory tunnel infrastructure is observable;
6. client pool selection/configuration is separately observable;
7. paired-tunnel availability is separately observable;
8. build-message creation is separately observable;
9. Router-A dispatch is separately observable;
10. Router-C receipt/decrypt/response is separately observable when reached;
11. Router-A reply/decrypt/status/join is separately observable when reached;
12. the earliest missing stage is classified without inferring from later
    helper timeout;
13. no workaround is implemented;
14. raw logs remain scratch-only;
15. exactly one terminal is emitted;
16. focused/routine verification passes;
17. closure/registry/roadmap/unblock audit lands.

## 19. Closure disposition

Update:

```text
plans/closure/mixed-router-interop/228-status.md
```

with:

- implementation SHA(s);
- exact Java/i2pd pins;
- attempt history;
- pre-build tunnel infrastructure;
- exact client configuration/selection facts;
- paired-tunnel facts;
- dispatch facts;
- Router-C request processing facts;
- Router-A reply/install facts;
- timeout/result counters;
- exact final terminal;
- local and CI verification;
- security/compatibility notes;
- unblock audit.

Do not rewrite Plans 226/227 to hide their prior hypotheses or stops.

If Plan 228 closes at a specific missing stage, register one narrow corrective
successor only after the evidence supports a change.

If Plan 228 emits:

```text
P228-NEXT-BOUNDARY-CLIENT-TUNNELS-BUILT
```

the successor should re-run the frozen Plan-226/227 destination lookup and
reverse-delivery qualification, rather than adding more tunnel diagnostics.

## 20. Registration disposition

```text
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
plan_228 = registered-ready-m6-java-client-tunnel-build-path-attribution

plan_201 = blocked-pending-plan228-client-tunnel-build-path-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan228
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 228-m6-java-client-tunnel-build-path-attribution
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```
