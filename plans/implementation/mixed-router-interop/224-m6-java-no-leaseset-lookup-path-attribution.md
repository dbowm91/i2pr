# Plan 224 — M6 Java NO_LEASESET lookup-path attribution

Status: **registered-ready-m6-java-no-leaseset-lookup-path-attribution**

## 1. Bounded objective

Attribute the exact first failing stage behind the Plan-223 reverse-send
sequence:

```text
STATUS_SEND_ACCEPTED (1)
STATUS_SEND_FAILURE_NO_LEASESET (21)
```

without changing production i2pr behavior, Java topology, NetDB publication
policy, tunnel lengths, helper transport settings, or Java source.

Plan 224 is an **attribution pass only**.

It exists because Java I2P 2.13.0 status 21 is emitted only after the remote
LeaseSet lookup path has failed to leave a usable target LeaseSet in the
helper's client NetDB, but that status alone does not identify which of these
stages failed:

1. i2pr's freshly published Standard LS2 never became stored/answerable in
   Router B's main NetDB;
2. Router A's helper client lookup never sent a query to an answerable
   floodfill;
3. Router B received the lookup but did not answer with the published LS2;
4. Router B answered, but the encrypted DSM reply did not arrive down Router
   A's helper inbound client tunnel;
5. Router A received the DSM on the helper client tunnel but did not install
   it into the helper client sub-DB.

The plan must prove the earliest failing stage from one internally consistent
run and stop. The later corrective belongs to a new Plan 225.

## 2. Current authority

Plan 223 closed as:

```text
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
p223_terminal = P223-NEXT-BOUNDARY ordered_statuses=[1, 21]
implementation_sha = 0755dc12cad94fae33fcb76a0d470e45175539eb
```

Retained facts:

- generated i2pr Destination is Java-compatible type 0 / 256-byte legacy
  identity shape;
- Java and Rust agree on the exact post-fix Destination hash/type/length;
- Standard LS2 remains type 3 with X25519/type 4 / 32-byte encryption key;
- source Java helper LeaseSetKeys are present and support X25519;
- Plan-222 client-NetDB selector preflight remains exact and nonempty;
- Router B is present in the exact selector;
- target LS is absent in Router A's helper client DB before the tracked send;
- tracked helper send is nonce-correlated;
- status 17 is gone;
- tracked send emits ACCEPTED then NO_LEASESET;
- no matching reverse TunnelData/payload reaches i2pr inside the frozen
  45-second window;
- J219-B remains refuted.

Plan 224 MUST NOT reinterpret Plan 223's target-client-DB absence as proof that
publication failed. The current harness publishes the i2pr LS2 specifically
toward Router B, but Plan 223 never proved whether Router B's main NetDB
actually stored an answerable copy.

## 3. Exact-pinned Java source findings

Java I2P 2.13.0 pin:

```text
9134f808337b401e8e53c73734c81fab04280c9d
```

### 3.1 OCMOSJ status 21 is a lookup-failure result, not a root cause

Pinned `OutboundClientMessageOneShotJob.runJob()` uses:

```java
KademliaNetworkDatabaseFacade kndf =
    (KademliaNetworkDatabaseFacade)
    getContext().clientNetDb(_from.calculateHash());

if (_leaseSet == null) {
    _leaseSetLookupBegin = getContext().clock().now();
    LookupLeaseSetFailedJob failed =
        new LookupLeaseSetFailedJob(getContext());
    kndf.lookupLeaseSet(
        _to.calculateHash(),
        success,
        failed,
        LS_LOOKUP_TIMEOUT,
        _from.calculateHash());
}
```

Pinned timeout:

```text
LS_LOOKUP_TIMEOUT = 15 seconds
```

`LookupLeaseSetFailedJob` maps ordinary lookup failure to:

```text
STATUS_SEND_FAILURE_NO_LEASESET = 21
```

unless the target is negative-cached forever for unsupported crypto.

Therefore status 21 proves:

> no usable target LeaseSet was available to the OCMOSJ send path after the
> bounded client-NetDB lookup.

It does NOT, by itself, distinguish publication, query dispatch, reply
delivery, or client-subDB installation.

### 3.2 Client lookup must use client tunnels

Pinned `KademliaNetworkDatabaseFacade.lookupLeaseSet()` performs:

```text
local client-subDB lookup
-> negative-cache check
-> IterativeSearchJob(..., isLease=true, fromLocalDest=helperHash)
```

Pinned `FloodfillNetworkDatabaseFacade.search()` rejects exploratory-only
searches from a client sub-DB.

Pinned `IterativeSearchJob` for a client lookup:

- selects an outbound tunnel for the helper client;
- selects an inbound reply tunnel for the helper client;
- derives ratchet/ElGamal reply capability from the helper's LeaseSetKeys;
- fails if no usable inbound client reply tunnel exists;
- builds a LeaseSet DatabaseLookupMessage;
- sends it through the helper's outbound client tunnel;
- requests an encrypted reply when the floodfill RI supports it.

The source emits target/peer-correlated diagnostics such as:

```text
ISJ try ... for LS <target> to <peer> ... reply via client tunnel? true
ISJ ... failed, no IB client tunnel to receive reply
ISJ ... skipped, no ratchet/elg support
ISJ for <target> successful ...
ISJ for <target> failed ...
```

These are useful evidence if enabled in the disposable test router.

### 3.3 A floodfill only answers an LS query from an answerable main-NetDB entry

Pinned `HandleDatabaseLookupMessageJob.runJob()` checks the main NetDB:

```java
DatabaseEntry dbe = getContext().netDb().lookupLocally(searchKey);
```

For a LeaseSet query it sends the LeaseSet only if:

```text
entry is LeaseSet
AND query type is LS/ANY
AND ls.getReceivedAsPublished() == true
```

When that is true, pinned source records:

```text
We have the published LS <target>, answering query
```

and calls `sendData()`.

Therefore Router B's target LS state must be observed as two separate facts:

1. present/current;
2. received-as-published / query-answerable.

A raw stored LS that is not answerable is not sufficient.

### 3.4 A successful client-tunnel DSM reply is explicitly routed into that client sub-DB

Pinned `InboundMessageDistributor` handling of a LeaseSet DSM arriving down
a client tunnel:

```java
dbe.setReceivedBy(_client);
```

and the garlic-local path records:

```text
Storing garlic LS down tunnel for: <target> sent to: <client>
```

Pinned `FloodfillDatabaseStoreMessageHandler.createJob()` then selects:

```java
if (entry.getReceivedBy() != null)
    netdb = context.clientNetDb(entry.getReceivedBy());
else
    netdb = mainFacade;
```

Thus a DSM received on the helper's inbound client tunnel is routed to the
helper client sub-DB.

### 3.5 Store-before-success race is ruled out by pinned source

Pinned `InNetMessagePool` has an explicit DSM special case:

```java
// If a DSM has a reply job, run the DSM inline
// so the entry is stored in the netdb before the reply job runs.
...
dsmjob.runJob();
...
job.setMessage(messageBody);
jobQueue.addJob(job);
```

This ordering exists specifically so the matching NetDB entry is stored before
the lookup success job executes.

Plan 224 MUST therefore reject this speculative root cause:

```text
"OCMOSJ lookup success fired before the matching DSM was stored"
```

If the exact target DSM is proven received on the helper client tunnel but a
post-receive client-subDB snapshot still lacks the LS, that is an evidence
contradiction / Java store-path boundary, not an assumed scheduling race.

## 4. Current i2pr publication path

The current destination driver publishes the freshly generated local LS2
specifically toward Router B:

```text
local LS2
-> DatabaseStoreMessage(reply_token=0)
-> begin_ls2_publication(... target=Router B)
-> compose_ls2_publication_via_tunnel(... target=Router B)
-> RouterDeliveryService::deliver(...)
```

Current evidence:

```text
ls2-publication-tunnel cells=<n>
```

proves i2pr composed and submitted the publication through the controlled
tunnel path.

It does NOT prove:

- Router B received the DSM;
- Router B accepted/stored it;
- Router B marks it received-as-published;
- Router B would answer a later DLM for it.

The first Plan-224 discriminator is therefore Router B's exact main-NetDB
state after publication and before the reverse tracked send.

## 5. Non-negotiable invariants

1. Java I2P pin stays exactly
   `9134f808337b401e8e53c73734c81fab04280c9d`.
2. i2pd pin stays exactly
   `635b013a612ff47278ef02acf8580a28e10e26c5`.
3. No Java source patching.
4. No reflection/private-field access.
5. No Java NetDB, KeyManager, tunnel, or LeaseSet mutation through diagnostics.
6. No direct LS2 copy into Router B or Router A client DB.
7. No fake DatabaseStore injection.
8. No public I2P.
9. No VMComm.
10. No topology expansion.
11. No floodfill-role change.
12. No RouterInfo/bootstrap correction.
13. No SAM pivot.
14. No tunnel-length or tunnel-count tuning.
15. No LS2 lifetime, publication target, or crypto change.
16. No production i2pr wire change.
17. Plan-223 Destination/LS2 separation remains frozen.
18. Plan-222 exact selector semantics remain frozen.
19. The reverse payload acceptance window remains exactly 45 seconds.
20. Missing observations remain Unknown.
21. Raw Java logs remain scratch-only and MUST NOT be copied into evidence.
22. Any log line containing session keys, tags, private material, or raw
    payloads MUST NOT be emitted into sanitized evidence.
23. Plan 224 implements attribution only. Any actual correction is Plan 225.

## 6. Scope

### In scope

- read-only main-NetDB LS snapshot on Router B;
- read-only helper client-subDB LS snapshot on Router A;
- exact target hash/type/current/publication flags;
- class-specific scratch logging for exact lookup-path correlation;
- safe sanitization of those scratch logs into booleans/counts only;
- exact tracked-send status sequence;
- exact target query-to-peer observation;
- Router B lookup-receipt / published-LS answer observation;
- Router A helper-client-tunnel LS DSM observation;
- post-failure client-subDB snapshot;
- one bounded P224 classifier;
- exact-head destination-only Java lane;
- closure/authority update.

### Out of scope

- fixing LS2 publication;
- retrying publication to another peer;
- changing floodfill selection;
- forcing Router B into a client selector result;
- directly invoking Java lookup APIs to pre-prime the client DB;
- manually storing the LS into Java;
- changing reply crypto;
- changing tunnel pools;
- changing helper session options;
- changing target LS2 encoding;
- changing destination identity;
- Streaming requalification;
- final M6 Java closure;
- M11.

## 7. Diagnostic surface

Reuse the existing loopback-only `ControlledRouter` diagnostic server.

Do not add a second server.

### 7.1 P224 main-NetDB LS snapshot

Add:

```text
P224-MAIN-LS <target-hex>
```

Response:

```text
P224-EV kind=main-ls
target_hash_hex=<64-lower-hex>
raw_present=<true|false>
validated_present=<true|false>
entry_type=<int|-1>
received_as_published=<true|false|unknown>
received_as_reply=<true|false|unknown>
received_by_hex=<64-hex|none|unknown>
ls2_unpublished=<true|false|na|unknown>
lease_count=<bounded-int|-1>
key_count=<bounded-int|-1>
key_types=<comma-codes|none|unknown>
latest_lease_ms=<u64|0>
current=<true|false|unknown>
```

Use only read-only/public or package-visible same-package test helper APIs.

Required distinctions:

- `raw_present`: `lookupLocallyWithoutValidation(target)`;
- `validated_present`: `lookupLeaseSetLocally(target)`;
- `received_as_published`: exact flag on the stored LeaseSet;
- `current`: current at snapshot time;
- for LS2, expose only key type codes and counts, never key bytes.

### 7.2 P224 client-subDB LS snapshot

Add:

```text
P224-CLIENT-LS <client-dbid-hex> <target-hex>
```

Resolve:

```java
context.clientNetDb(clientDbid)
```

Record:

```text
client_db_resolved
client_db_is_client
raw_present
validated_present
entry_type
received_as_published
received_as_reply
received_by_hex
ls2_unpublished
lease_count
key_count
key_types
latest_lease_ms
current
```

If the requested DBID falls back to main, the snapshot is Unknown/invalid for
Plan-224 client authority.

### 7.3 Snapshot timing

Collect these exact snapshots:

```text
B_MAIN_AFTER_PUBLICATION_BEFORE_SEND
A_CLIENT_BEFORE_SEND
A_CLIENT_AFTER_NO_LEASESET
B_MAIN_AFTER_NO_LEASESET
```

The target hash MUST be the exact local i2pr destination hash used by the
tracked reverse send.

Do not reuse snapshots across attempts.

## 8. Scratch-only targeted Java logging

Pinned Java's default log level is ERROR, so the useful exact-target INFO/DEBUG
lookup lines are not guaranteed without a diagnostic logger configuration.

Plan 224 may create a disposable `logger.config` inside each scratch Java
router data directory before startup.

This is diagnostic configuration only; it is not a protocol/topology change.

Use:

```properties
logger.defaultLevel=ERROR
logger.minimumOnScreenLevel=CRIT
logger.flushInterval=1
logger.record.net.i2p.router.networkdb.kademlia.IterativeSearchJob=INFO
logger.record.net.i2p.router.networkdb.HandleDatabaseLookupMessageJob=DEBUG
logger.record.net.i2p.router.tunnel.InboundMessageDistributor=INFO
```

The exact-pinned `LogManager` contract uses:

```text
logger.record.<scope>=<level>
```

and defaults to `logger.config` in the router config directory.

### 8.1 Security rule

`HandleDatabaseLookupMessageJob` may emit an INFO line containing ephemeral
reply key/tag material.

Therefore:

- raw router log files MUST remain under scratch;
- raw lines MUST NOT be copied to `target/interop/...evidence`;
- sanitization is whitelist-only;
- no generic `grep ... >> evidence`;
- the sanitizer may write only typed booleans/counts and target/peer hashes;
- never write captured session key/tag substrings;
- never write the complete source line.

## 9. Sanitized lookup trace facts

Create one sanitized fact set for the exact target/send epoch.

Suggested evidence row:

```text
p224-lookup-trace:
  observable=<bool>
  target_hash_hex=<...>
  query_started=<bool>
  query_to_b=<bool>
  query_via_client_reply_tunnel=<bool>
  no_ib_client_tunnel=<bool>
  no_ratchet_or_elg_support=<bool>
  search_success=<bool>
  search_failed=<bool>
  b_lookup_received=<bool>
  b_published_ls_answered=<bool>
  a_client_tunnel_ls_received=<bool>
  reply_encryption_error_seen=<bool>
```

Only exact-target-correlated facts may be authoritative.

### 9.1 Router A query facts

From `IterativeSearchJob` scratch log, sanitize only exact target lines.

Required patterns:

```text
ISJ try ... for LS <target> to <peer> ... reply via client tunnel? true
ISJ ... failed, no IB client tunnel to receive reply
ISJ ... skipped, no ratchet/elg support
ISJ for <target> successful ...
ISJ for <target> failed ...
```

For `query_to_b`, require the exact Router B hash in the same line.

Do not infer that selector membership means actual query.

### 9.2 Router B receive/answer facts

Whitelist exact-target patterns from
`HandleDatabaseLookupMessageJob`:

```text
Handling database lookup message for <target> ...
We have the published LS <target>, answering query
```

Sanitize to booleans only.

Do not persist any later reply-key/tag line.

### 9.3 Router A inbound-client-tunnel fact

Whitelist exact-target line:

```text
Storing garlic LS down tunnel for: <target> sent to: <helper>
```

Sanitize to:

```text
a_client_tunnel_ls_received=true
```

No raw log line in evidence.

### 9.4 Log observability failure

If the targeted logger configuration is installed but the expected class log
file cannot be read or target parsing fails, set:

```text
observable=false
```

and use an observability-gap terminal where required.

Do not use absence from a missing/unreadable log as a protocol fact.

## 10. Work package A — freeze Plan-223 behavior

Before adding P224 evidence:

1. keep Destination type 0 / 256 behavior unchanged;
2. keep Standard LS2 type 4 / X25519 behavior unchanged;
3. keep Plan-222 selector width/routing-key logic unchanged;
4. keep `SEND_TRACKED` / `SEND_STATUS`;
5. keep 45-second payload window;
6. keep Router B as the existing publication target;
7. add static guards rejecting production changes in the above areas.

Run focused P222/P223 tests before continuing.

## 11. Work package B — prove Router B publication state

Immediately after:

```text
ls2-publication-tunnel
```

and after a short bounded settle that does not exceed the existing publication
timing assumptions, query Router B:

```text
P224-MAIN-LS <target>
```

Do not retry/re-publish based on the result.

Classify the Router B state:

### B1. Target raw LS absent

```text
P224-ATTRIBUTION-B-MAIN-LS-ABSENT
```

Meaning:

> the controlled i2pr publication submission did not leave the exact target
> LS present in Router B's main NetDB at the authoritative pre-send epoch.

Stop. Plan 225 owns the publication/store corrective.

### B2. Raw present, validated/current absent

```text
P224-ATTRIBUTION-B-MAIN-LS-INVALID-OR-STALE
```

Record:

- entry type;
- latest lease time;
- current flag.

Stop. Plan 225 owns validity/freshness investigation.

### B3. Valid LS present but not received-as-published

```text
P224-ATTRIBUTION-B-MAIN-LS-NOT-QUERY-ANSWERABLE
```

Pinned Java will not answer an LS DLM from this entry.

Stop. Plan 225 owns publication-state semantics.

### B4. Valid/current + received-as-published

Record:

```text
b_main_ls_answerable=true
```

Continue.

This gate is mandatory before any search-path attribution.

## 12. Work package C — exact tracked-send lookup trace

Reuse the same one tracked reverse message.

Order:

1. snapshot Router B main LS;
2. snapshot Router A helper client DB;
3. capture target hash + Router B hash + helper DBID;
4. call `SEND_TRACKED`;
5. collect ordered status events;
6. run the existing 45-second i2pr inbound payload pump unchanged;
7. collect sanitized lookup trace;
8. snapshot Router A helper client DB again;
9. snapshot Router B main LS again.

Do not initiate any standalone Java lookup probe.

A diagnostic lookup would prime the same client DB and destroy the causal
meaning of the tracked OCMOSJ send.

## 13. Work package D — terminal classifier

Exactly one P224 terminal per authoritative attempt.

Priority order is earliest proven failing stage.

### D1. Publication/store terminals

Use B1-B3 first:

```text
P224-ATTRIBUTION-B-MAIN-LS-ABSENT
P224-ATTRIBUTION-B-MAIN-LS-INVALID-OR-STALE
P224-ATTRIBUTION-B-MAIN-LS-NOT-QUERY-ANSWERABLE
```

### D2. Search cannot form a client lookup

If B is answerable but exact Router-A trace proves:

```text
no_ib_client_tunnel=true
```

emit:

```text
P224-ATTRIBUTION-A-NO-INBOUND-CLIENT-REPLY-TUNNEL
```

If exact trace proves:

```text
no_ratchet_or_elg_support=true
```

emit:

```text
P224-ATTRIBUTION-A-LOOKUP-REPLY-CRYPTO-UNAVAILABLE
```

Do not change helper keys or tunnels inside Plan 224.

### D3. B answerable but A never queries B

If:

```text
b_main_ls_answerable=true
query_started=true
query_to_b=false
search_failed=true
status 21 observed
```

emit:

```text
P224-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
```

This proves the controlled answerable publication target was not queried.

Do not call it a selector defect: selector membership and actual query
scheduling are distinct.

### D4. A dispatches query to B but B never observes it

If:

```text
query_to_b=true
b_lookup_received=false
search_failed=true
```

and trace observability is proven, emit:

```text
P224-ATTRIBUTION-A-TO-B-LOOKUP-NOT-RECEIVED
```

This is a client-outbound-tunnel/query-delivery boundary.

Do not change tunnel construction in Plan 224.

### D5. B receives query but does not answer despite answerable state

If:

```text
b_lookup_received=true
b_main_ls_answerable=true
b_published_ls_answered=false
```

take the post-send Router-B snapshot.

If it remains answerable, emit:

```text
P224-EVIDENCE-CONTRADICTION-B-ANSWERABLE-BUT-NOT-ANSWERED
```

If its state changed, classify from the post-send state and record the
transition rather than inventing a handler bug.

### D6. B answers but A never observes target LS on client tunnel

If:

```text
b_published_ls_answered=true
a_client_tunnel_ls_received=false
status 21 observed
```

emit:

```text
P224-ATTRIBUTION-B-REPLY-NOT-OBSERVED-ON-A-CLIENT-TUNNEL
```

If the scratch log contains the exact non-secret error token:

```text
DLM reply encryption error
```

record only:

```text
reply_encryption_error_seen=true
```

as supporting evidence. It is not target-specific enough to be the sole
terminal.

### D7. A observes target LS on client tunnel but client DB remains absent

If:

```text
a_client_tunnel_ls_received=true
A_CLIENT_AFTER_NO_LEASESET.validated_present=false
```

emit:

```text
P224-EVIDENCE-CONTRADICTION-CLIENT-TUNNEL-DSM-NOT-IN-CLIENT-DB
```

Pinned source says the DSM is tagged with `receivedBy=helper`, routed to that
client DB, and stored inline before lookup reply job execution.

Do not dismiss this as a race.

### D8. Client DB contains target but OCMOSJ still emits 21

If:

```text
A_CLIENT_AFTER_NO_LEASESET.validated_present=true
status 21 observed
```

emit:

```text
P224-EVIDENCE-CONTRADICTION-NO-LEASESET-WITH-USABLE-CLIENT-LS
```

Record exact LS type/current/key types.

Stop.

### D9. Status moves beyond 21

If status 21 disappears and another terminal appears:

```text
P224-NEXT-BOUNDARY ordered_statuses=<...>
```

Do not implement the next fix.

### D10. Reverse delivery passes

If the digest-matched payload arrives within the frozen 45-second window:

```text
P224-REVERSE-DELIVERY-PASSED
```

A later final requalification plan owns Java-family closure.

### D11. Incomplete evidence

If none of the above can be proven because targeted trace or required
snapshots are unavailable:

```text
P224-OBSERVABILITY-GAP-LOOKUP-PATH
```

Record which facts are Unknown.

## 14. Work package E — static evidence guards

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh`.

Require:

- `P224-MAIN-LS`;
- `P224-CLIENT-LS`;
- read-only main/client NetDB access;
- exact target hash in snapshots;
- `received_as_published` observation;
- targeted logger config with default ERROR;
- exact class-specific log overrides;
- whitelist-only sanitizer;
- single P224 classifier;
- 45-second payload freeze retained.

Reject:

- Java `store(...)` or `registerKeys(...)` in P224 diagnostic code;
- reflection/`setAccessible`;
- standalone diagnostic `lookupLeaseSet(...)` before the tracked send;
- direct LS2 copy into a Java facade;
- changes to i2pr LS2 publication target;
- additional publication retry;
- Java/i2pd pin changes;
- tunnel quantity/length changes;
- SAM pivot;
- raw Java log copying into evidence;
- sanitizer copying `Sending AEAD reply` source lines;
- session key/tag fields in evidence.

## 15. Work package F — focused tests

Add tests for at least:

1. main-LS snapshot distinguishes raw absent from validated absent;
2. main-LS snapshot records received-as-published;
3. client-LS snapshot rejects main-DB fallback;
4. LS2 snapshot exposes type/counts only, not key bytes;
5. B absent maps to `B-MAIN-LS-ABSENT`;
6. B invalid/stale maps correctly;
7. B present but RAP=false maps to not-query-answerable;
8. B answerable + no inbound client tunnel maps correctly;
9. B answerable + no reply crypto maps correctly;
10. selector-contains-B alone does not satisfy `query_to_b`;
11. query-to-B true + B receive false maps to lookup-not-received;
12. B answer true + A inbound false maps to reply-not-observed;
13. A inbound true + client DB absent maps to contradiction;
14. client DB present + status21 maps to contradiction;
15. status change maps to NEXT-BOUNDARY;
16. payload delivery maps to REVERSE-DELIVERY-PASSED;
17. missing log file yields observability gap, not false facts;
18. sanitizer rejects/censors session-key/tag-bearing lines;
19. sanitizer emits only bounded typed facts;
20. Plan-223 status-17 regression remains gone for new destinations;
21. P222 selector-equivalence tests remain green;
22. 45-second result cannot be changed by later trace collection.

## 16. Exact-head run contract

All implementation must be committed before counted execution.

Record:

```bash
git rev-parse HEAD
git status --porcelain=v1
```

Tree must be clean.

Run:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

### Retry budget

- maximum three attempts on one implementation SHA;
- fresh scratch RouterContexts every attempt;
- logger configuration identical every attempt;
- no code/config/timing/topology tuning between attempts;
- pre-authoritative startup/lease-publication infrastructure stops are recorded;
- if no attempt reaches the P224 authoritative epoch, close stopped with an
  observability/environment result.

A code/config change requires a new implementation commit and resets the
count only with explicit closure rationale.

## 17. Verification commands

Routine floor:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
```

Boundaries/evidence:

```bash
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
```

Focused:

```bash
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p224 -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p223 -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p222 -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
```

External:

```bash
test -z "$(git status --porcelain=v1)"
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

## 18. Acceptance criteria

Plan 224 closes only when all applicable criteria are recorded.

1. Plan-223 identity/LS2 separation is unchanged.
2. Plan-222 selector semantics are unchanged.
3. Target hash is exact and consistent across all P224 snapshots.
4. Router B main-NetDB raw target presence is observed.
5. Router B validated/current target presence is observed.
6. Router B received-as-published flag is observed.
7. Router B answerability is explicitly derived from pinned Java rules.
8. Router A helper client DB pre-send state is observed.
9. One nonce-correlated tracked send is used.
10. Ordered status events are retained.
11. 45-second payload result remains frozen.
12. Targeted Java lookup logging is configured before router startup.
13. Raw logs stay scratch-only.
14. Sanitized log facts contain no session key/tag/private material.
15. Actual query-to-B is distinguished from selector membership.
16. B lookup receipt is distinguished from A query dispatch.
17. B answer is distinguished from B lookup receipt.
18. A client-tunnel DSM receipt is distinguished from B answer.
19. A client-subDB post-send state is separately observed.
20. The source-proven DSM store-before-success ordering is documented.
21. Exactly one P224 terminal is emitted.
22. Terminal identifies the earliest proven failing stage or observability gap.
23. No production corrective is implemented.
24. No Java mutation/reflection/patch occurs.
25. No topology/tunnel/SAM/bootstrap/floodfill change occurs.
26. Routine/focused test floor is green.
27. Exact implementation SHA precedes external evidence.
28. Counted run begins clean.
29. Retry budget is respected.
30. Registry/roadmap/dependencies are audited after closure.
31. M6 Java interoperability remains unclaimed unless a later qualification
    closes it.

## 19. Stop conditions

Stop immediately if:

- Router B lacks an answerable LS;
- exact-target log correlation cannot be made reliable without patching Java;
- the lookup trace requires reflection/private state;
- observation would require a standalone lookup that primes the helper DB;
- raw sensitive Java logs would need to be retained as evidence;
- a proposed change modifies publication, tunnels, selector, or reply crypto;
- an evidence contradiction appears.

Do not fix the discovered stage in Plan 224.

Register Plan 225 with exactly the boundary Plan 224 proves.

## 20. Closure evidence

Create/update:

```text
plans/closure/mixed-router-interop/224-status.md
```

It MUST include:

- implementation SHA;
- exact Java/i2pd pins;
- clean-tree proof;
- target destination hash;
- Router B main-LS pre-send snapshot;
- Router A client-LS pre-send snapshot;
- tracked nonce + payload digest;
- ordered status sequence;
- sanitized lookup trace facts;
- Router A client-LS post-send snapshot;
- Router B main-LS post-send snapshot;
- frozen 45-second TunnelData/payload result;
- P224 terminal;
- source citation/provenance for Java lookup/store ordering;
- attempt history;
- routine/focused verification results;
- security/log-sanitization review;
- findings by severity;
- unblock audit;
- exact Plan-225 handoff implied by the result.

## 21. Small-model execution order

Execute in this order only:

```text
A. Freeze P222/P223 invariants and add static guards.
B. Add P224-MAIN-LS and P224-CLIENT-LS read-only snapshots.
C. Add unit tests for snapshot semantics.
D. Add scratch-only targeted logger.config generation.
E. Add whitelist-only lookup-trace sanitizer and security tests.
F. Add P224 classifier unit tests.
G. Integrate pre-send B-main and A-client snapshots.
H. Reuse SEND_TRACKED; do not add a second lookup.
I. Preserve the existing 45-second payload pump.
J. Collect sanitized exact-target trace.
K. Collect post-send A-client and B-main snapshots.
L. Emit exactly one P224 terminal.
M. Run focused/routine verification.
N. Commit implementation.
O. Confirm clean tree.
P. Run the exact destination-only Java lane.
Q. Write 224-status.md.
R. Audit Plan 201 / 204 / 205 / 218 / 222 / 223 and register only the
   resulting Plan-225 dependency state.
```

Do not merge work packages.

Do not implement the Plan-225 corrective.

## 22. Registration disposition

At registration:

```text
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
plan_224 = registered-ready-m6-java-no-leaseset-lookup-path-attribution

plan_201 = blocked-pending-plan224-no-leaseset-lookup-path-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan224-attribution

next_executable_plan = 224-m6-java-no-leaseset-lookup-path-attribution
```

No production corrective is authorized by this registration.
