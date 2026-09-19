# Plan 222 — M6 Java client-NetDB/OCMOSJ narrowing corrective

Status: **registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective**.

This plan supersedes the unexecuted Plan 221 narrowing plan. Plan 221 is retained
as historical registration evidence but MUST NOT be executed directly.

## 1. Objective

Produce a trustworthy, exact-pinned attribution for the Java → i2pr reverse
Destination failure at or below Java's client-NetDB / OutboundClientMessageOneShotJob
(OCMOSJ) path.

Plan 222 corrects two preconditions that make Plan 221 unsafe to execute as
written:

1. the Plan 220 selector probe did not reproduce Java's actual lookup selector
   inputs; and
2. Plan 221 did not prioritize the strongest available public per-message
   observation surface: `I2PSession.sendMessage(..., SendMessageStatusListener)`.

The only allowed outcome of Plan 222 is:

> one exact-clean-head destination-only run emits a `P222-*` terminal that is
> derived from the real client lookup selector inputs and one nonce-correlated
> helper send, or emits a narrower explicit observability gap when the exact
> boundary still cannot be proven without violating the test constraints.

Plan 222 MUST NOT implement the eventual protocol/topology fix.

## 2. Research verdict

The corrective direction is supported by the exact-pinned Java I2P 2.13.0
source at:

```text
9134f808337b401e8e53c73734c81fab04280c9d
```

### 2.1 Plan 220 selector evidence was real, but not production-equivalent

Pinned `IterativeSearchJob` computes:

```java
_rkey = ctx.routingKeyGenerator().getRoutingKey(key);
```

and selects floodfills with:

```java
selectFloodfillParticipants(_rkey, _totalSearchLimit + EXTRA_PEERS, ks)
```

The pinned selector explicitly documents the argument as:

```text
the ROUTING key (NOT the original key)
```

Plan 220's `P220SelectorProbe` instead called the selector with:

- the raw destination hash;
- hard-coded fanout `N=3`;
- the main facade directly.

Therefore the Plan 220 row:

```text
SELECTOR Known(pass)
```

is not exact enough to authorize a client-NetDB root-cause claim.

### 2.2 Client DB and main DB share selector/k-buckets, but not LeaseSet storage

Pinned `KademliaNetworkDatabaseFacade.startup()` shows that a client DB:

- reuses the main DB's `KBucketSet`;
- reuses the main DB's peer selector;
- uses its own transient data store.

Pinned `getAllRouters()` explicitly returns an empty set for a client DB.

That means:

- probing the shared selector is valid only if the inputs exactly match the
  client's real lookup;
- client LeaseSet presence MUST be observed through
  `context.clientNetDb(helperHash)`, not inferred from main-NetDB state;
- if the exact selector produces no usable peer, the client DB fallback cannot
  rescue the lookup through `getAllRouters()`.

### 2.3 The exact production selector width is not N=3

Pinned `IterativeSearchJob` uses:

```text
TOTAL_SEARCH_LIMIT = 5
TOTAL_SEARCH_LIMIT_WHEN_FF = 3
EXTRA_PEERS = 1
```

with:

```text
total = 3 only when facade.floodfillEnabled() && uptime > 30 min
otherwise total = 5
total = context property netdb.searchLimit if overridden
selector_width = total + 1
```

The controlled topology normally runs below 30 minutes, so a hard-coded N=3
is not production-equivalent.

### 2.4 Public send-status API gives exact per-message correlation

Pinned `I2PSession` exposes:

```java
long sendMessage(
    Destination dest,
    byte[] payload,
    int offset,
    int size,
    int proto,
    int fromPort,
    int toPort,
    SendMessageOptions options,
    SendMessageStatusListener listener)
```

The returned long is the helper-side nonce/message identifier used in later
status callbacks.

Pinned `I2PSessionMuxedImpl`:

- allocates a nonzero nonce;
- creates a `MessageState`;
- associates the supplied `SendMessageStatusListener`;
- returns that nonce.

Pinned `ClientConnectionRunner` sends
`STATUS_SEND_ACCEPTED` for that nonce when the router admits the send.

Pinned OCMOSJ `dieFatal(status)` reports that same nonce back through the
client manager.

This is a public API and does not require:

- Java source patching;
- reflection;
- private-field inspection;
- NetDB mutation;
- tunnel mutation.

### 2.5 Status meanings are useful but must not be overclaimed

The exact-pinned status surface can safely distinguish these classes:

| Status | Numeric | What may be concluded |
|---|---:|---|
| `STATUS_SEND_ACCEPTED` | 1 | Router admitted the nonce-bearing client send. Nothing downstream is proven. |
| `STATUS_SEND_BEST_EFFORT_FAILURE` | 3 | OCMOSJ reached its post-dispatch timeout machinery; do not call this "network delivery failed at peer X". |
| `STATUS_SEND_GUARANTEED_SUCCESS` | 4 | OCMOSJ received its delivery-status ACK path. Dispatch and remote ACK path passed. |
| `STATUS_SEND_FAILURE_LOCAL_LEASESET` | 15 | Sender's local LeaseSet/tunnel prerequisites failed. |
| `STATUS_SEND_FAILURE_NO_TUNNELS` | 16 | OCMOSJ reached send preparation but had no usable outbound/reply tunnel or garlic construction returned no usable tunnel. |
| `STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION` | 17 | Destination/LeaseSet encryption is unsupported for this send. |
| `STATUS_SEND_FAILURE_DESTINATION` | 18 | Destination validation/options failed. |
| `STATUS_SEND_FAILURE_BAD_LEASESET` | 19 | A far-end LeaseSet was present but unusable. |
| `STATUS_SEND_FAILURE_EXPIRED_LEASESET` | 20 | Far-end LeaseSet was expired and could not be refreshed. |
| `STATUS_SEND_FAILURE_NO_LEASESET` | 21 | OCMOSJ could not obtain a usable far-end LeaseSet. |
| `STATUS_SEND_FAILURE_META_LEASESET` | 22 | Far-end result is a Meta LS2 and cannot be sent to directly. |
| `STATUS_SEND_FAILURE_LOOPBACK` | 23 | Send attempted to the same Destination. |

A status callback is evidence about the exact nonce-bearing send only.

Absence of a terminal callback is NOT evidence of dispatch.

### 2.6 Existing 45-second reverse-delivery acceptance window stays frozen

OCMOSJ's pinned defaults are:

```text
OVERALL_TIMEOUT_MS_DEFAULT = 60 s
LS_LOOKUP_TIMEOUT = 15 s
OVERALL_TIMEOUT_NOLS_MIN = 23 s
REPLY_TIMEOUT_MS_MIN = 55 s
RATCHET_REPLY_TIMEOUT_MS_MIN = 30 s
```

Plan 222 MUST NOT extend the existing 45-second i2pr reverse-delivery acceptance
window to make the lane pass.

A separate status-observation window may continue after the 45-second delivery
window solely to receive the router's own asynchronous terminal status.

That later status MUST NOT retroactively change whether the 45-second i2pr
payload-delivery row passed or failed.

## 3. Authority correction

The authoritative planning state becomes:

```text
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective

plan_201 = blocked-pending-plan222-client-netdb-ocmosj-narrowing
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan222-narrowing
```

Important retained Plan 220 facts:

- RouterHash cross-check passed.
- Router A holds Router B's exact current `f`-bearing RouterInfo.
- Router A PeerManager indexes B under `f`.
- Java helper reverse send is admitted.
- no reverse TunnelData/payload reaches i2pr inside the existing 45-second
  acceptance window.
- J219-B is refuted.

Not retained as final authority until Plan 222:

- exact selector pass;
- "first unresolved stage is definitely CLIENT-NETDB".

## 4. Invariants

Plan 222 MUST preserve:

- Java I2P 2.13.0 exact pin
  `9134f808337b401e8e53c73734c81fab04280c9d`;
- i2pd 2.61.0 exact pin
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- no public I2P participation;
- no Java source patch;
- no reflection;
- no private-field access;
- no mutation of Java NetDB state for diagnosis;
- no fake RouterInfo / fake capability injection;
- no `netDb.alwaysQuery`;
- no direct LS2 copy into a Java client DB;
- no tunnel-state injection;
- no VMComm;
- no SAM pivot;
- no topology expansion;
- no production i2pr wire change;
- no change to the existing 45-second reverse-delivery acceptance window;
- loopback-only control/diagnostic sockets;
- all missing or ambiguous observations remain Unknown;
- Plan 193 i2pd authority unchanged;
- Plan 215 M10 authority unchanged.

## 5. Scope

### In scope

- `tests/integration/m6-interop/java/ReferenceRawDestination.java`;
- `tests/integration/m6-interop/java/ControlledRouter.java`;
- `tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P220SelectorProbe.java`
  or a renamed `P222SelectorProbe.java`;
- `crates/i2pr-daemon/tests/java_tunnel_external.rs`;
- `tests/integration/m6-interop/run-java.sh`;
- `scripts/check-m6-mixed-router-acceptance-evidence.sh`;
- Plan 220/221/222 authority records and roadmap/registry updates.

### Out of scope

- changing Java client tunnel length/quantity;
- changing Java floodfill mode;
- changing Java RouterInfo bootstrap;
- changing LeaseSet publication behavior;
- changing i2pr NetDB / Garlic / tunnel / SSU2 / LS2 production code;
- Streaming requalification;
- final Java-family closure;
- Plan 204 convergence;
- M11.

## 6. Small-model execution contract

A smaller executor MUST follow this order exactly.

Do not skip ahead.

Do not combine work packages.

After each work package:

1. run the named focused test/static check;
2. fix only failures introduced by that package;
3. commit before continuing if the package changes implementation behavior;
4. do not run the external Java lane until all local packages are complete.

If a step requires behavior outside this plan, STOP and write it in the Plan 222
closure record. Do not improvise a topology or protocol change.

## 7. Work package A — freeze Plan 220 selector overclaim

### A1. Do not delete Plan 220 evidence

Retain the Plan 220 closure history.

Add a current-authority amendment stating:

- J219-B remains refuted;
- exact stored-RI and PeerManager observations remain valid;
- the Plan 220 selector row used raw target hash and hard-coded N=3;
- therefore selector equivalence and the earliest client-message boundary are
  pending Plan 222.

### A2. Freeze old selector row as historical only

Do not let `P220 selector_contains_b=true` directly satisfy any P222 terminal.

Add a regression test that feeding only the old P220 selector observation into
the P222 classifier yields:

```text
P222-OBSERVABILITY-GAP-SELECTOR-EQUIVALENCE
```

## 8. Work package B — exact client lookup preflight

This package repairs selector equivalence before touching send-status logic.

### B1. Use the actual helper source Destination hash

The Rust driver already parses:

```text
JAVA_RAW_REFERENCE_DESTINATION_B64
```

and derives `reference_hash`.

Convert `reference_hash` to lowercase 64-character hex and pass it to the
Java diagnostic.

This is the `fromLocalDest` / client DBID used by:

```java
ctx.clientNetDb(_from.calculateHash())
```

in OCMOSJ.

### B2. Query the actual client facade

Add a bounded diagnostic command such as:

```text
P222-CLIENT-LOOKUP-PREFLIGHT <client_dbid_hex> <target_dest_hex> <router_b_hex>
```

Implementation requirements:

1. parse all hashes as exact 32-byte lowercase hex;
2. call:
   ```java
   NetworkDatabaseFacade ndb = context().clientNetDb(clientDbid);
   ```
3. verify/cast to `KademliaNetworkDatabaseFacade`;
4. record:
   - `client_db_resolved`;
   - `client_db_is_client`;
   - `target_ls_present_before_send`;
   - `target_ls_type` if present;
   - `target_ls_current` if the public API supports a direct safe check;
5. do not modify the facade.

If `clientNetDb(clientDbid)` falls back to the main DB, record that explicitly
and classify as an observability/topology precondition failure. Do not pretend
it is the client DB.

### B3. Derive the real routing key in Java

Inside the test-only probe:

```java
Hash routingKey = ctx.routingKeyGenerator().getRoutingKey(targetHash);
```

Record both:

```text
target_hash_hex
routing_key_hex
```

They MUST normally differ.

Do not accept a Rust-side reimplementation as the primary proof; use the pinned
Java router's own routing-key generator.

### B4. Reproduce IterativeSearchJob selector width

Compute the width exactly from pinned source behavior:

```text
default_total =
    3 if facade.floodfillEnabled() && ctx.router().getUptime() > 30 minutes
    else 5

total_search_limit =
    ctx.getProperty("netdb.searchLimit", default_total)

selector_width = total_search_limit + 1
```

Record:

```text
facade_floodfill_enabled
router_uptime_ms
netdb_search_limit_effective
selector_extra_peers=1
selector_width
```

Do not hard-code N=3.

### B5. Use the production-equivalent selector overload

Call:

```java
selectFloodfillParticipants(routingKey, selectorWidth, clientFacade.getKBuckets())
```

using the overload that supplies the router self-ignore set exactly as
`IterativeSearchJob` does.

Do not pass `Collections.emptySet()` from the probe.

Record:

- selected count;
- selected peer hashes, bounded to at most 8;
- whether Router B is selected;
- whether selection is empty.

### B6. Selector interpretation rules

Allowed conclusions:

- empty exact selector + actual client DB:
  `Known(fail): CLIENT-NETDB-NO-LOOKUP-PEER`
  because the client DB's `getAllRouters()` fallback is empty in pinned source;
- nonempty selector:
  selector stage passes only as "lookup has candidates";
- B absent from a nonempty selector:
  NOT automatically a root cause;
- B present:
  proves B is one candidate, not that B was actually queried.

### B7. Focused checks after package B

Add unit/static rows for:

- raw target hash rejected as selector key when routing key is available;
- N=3 hard-code rejected;
- client DBID required;
- client facade fallback-to-main is not a pass;
- empty selector vs nonempty selector semantics;
- B-excluded-but-nonempty remains non-root-cause.

## 9. Work package C — tracked helper send using public status listener

Keep the existing `SEND` control command unchanged for backwards compatibility.

Add a new command:

```text
SEND_TRACKED <destination_b64> <payload_hex> <from_port> <to_port>
```

### C1. Public API only

Use:

```java
SendMessageOptions options = new SendMessageOptions();

long nonce = session.sendMessage(
    peer,
    body,
    0,
    body.length,
    I2PSession.PROTO_DATAGRAM_RAW,
    fromPort,
    toPort,
    options,
    listener);
```

Do not set a special expiration in the first implementation.

Do not alter reliability/tunnel/publication session options.

The only behavior difference from legacy `SEND` is requesting public
asynchronous status notifications.

### C2. Correlation state

Maintain a bounded helper-local map keyed by `nonce`.

Each entry may hold at most:

- payload length;
- payload SHA-256;
- creation monotonic timestamp;
- ordered status events;
- each event:
  - numeric status;
  - local monotonic elapsed milliseconds.

Maximum tracked messages: 32.

Maximum events per message: 16.

Evict oldest completed entries first.

Never store destination private key bytes or payload bytes in the status map.

### C3. SEND_TRACKED response

Return one bounded line:

```text
TRACKED_SENT nonce=<u64> payload_len=<n> digest=<sha256>
```

If the send throws:

```text
TRACKED_ERROR class=<simple-class-name>
```

Do not expose stack traces through the control protocol.

### C4. Status polling command

Add:

```text
SEND_STATUS <nonce>
```

Response shape:

```text
TRACKED_STATUS nonce=<n> count=<n> events=<status:elapsed_ms,...>
```

or:

```text
TRACKED_STATUS nonce=<n> count=0 events=none
```

Unknown nonce:

```text
TRACKED_STATUS_UNKNOWN nonce=<n>
```

The Rust driver will poll; the Java helper MUST NOT block waiting for a future
status event.

### C5. Listener semantics

The listener MUST append every state-changing callback in order.

Do not collapse to the last status.

A later success after a best-effort failure is legal in pinned OCMOSJ and must
remain visible.

## 10. Work package D — Rust tracked-send control surface

Extend `ReferenceControl` with:

```rust
async fn send_raw_tracked(...) -> Option<TrackedSend>
async fn send_status(nonce: u64) -> Option<Vec<TrackedStatusEvent>>
```

Use strict parsing.

Reject:

- missing nonce;
- malformed integer;
- malformed digest;
- more than 16 events;
- duplicate malformed fields.

Never turn parse failure into a protocol failure.

Parse failure = Unknown diagnostic evidence.

The legacy `send_raw()` remains for historical tests that still need it.

## 11. Work package E — preserve the 45-second delivery window

The reverse-send flow becomes:

1. collect exact selector/client-DB preflight;
2. call `SEND_TRACKED`;
3. require a valid nonce;
4. set `reverse_java_send_admitted=Known(true)` only after the tracked call
   returns successfully;
5. run the existing i2pr inbound pump for exactly the existing
   `DATAGRAM_WAIT` duration;
6. poll `SEND_STATUS nonce` during the 45-second window without delaying the
   pump;
7. freeze the legacy payload-delivery result at 45 seconds;
8. if no decisive status has arrived, continue **status-only** polling until
   70 seconds from the tracked-send start;
9. do not resume or extend the i2pr payload acceptance window after 45 seconds.

Why 70 seconds:

- Java default OCMOSJ overall timeout is 60 seconds;
- the 10-second margin is diagnostic scheduling tolerance;
- no Java timeout property is changed.

If no terminal status arrives by 70 seconds, status state is Unknown.

## 12. Work package F — status-to-fact mapping

Introduce P222 facts separate from frozen P220 facts.

Required facts:

```text
selector_equivalent
selector_nonempty
target_leaseset_present_pre_send
tracked_nonce
status_accepted
status_no_leaseset
status_bad_leaseset
status_expired_leaseset
status_unsupported_encryption
status_no_tunnels
status_best_effort_failure
status_guaranteed_success
status_other_terminal
reverse_i2pr_tunneldata_observed_45s
reverse_i2pr_payload_recovered_45s
```

### F1. Exact allowed mappings

`STATUS_SEND_ACCEPTED (1)`

```text
status_accepted = Known(true)
client_message_admitted = Known(true)
```

No other stage passes from this status.

`STATUS_SEND_FAILURE_NO_LEASESET (21)`

```text
client_netdb_usable_target_ls = Known(false)
terminal = P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-USABLE-LEASESET
```

Do not claim whether zero peers vs queried peer vs negative cache unless another
exact observation proves it.

`STATUS_SEND_FAILURE_BAD_LEASESET (19)`,
`EXPIRED_LEASESET (20)`,
`UNSUPPORTED_ENCRYPTION (17)`,
`META_LEASESET (22)`

These prove OCMOSJ obtained or evaluated destination/LeaseSet material far
enough to return that specific failure.

Emit the matching P222 boundary, preserving the exact status code.

`STATUS_SEND_FAILURE_NO_TUNNELS (16)`

Allowed conclusion:

```text
client_netdb_or_local_ls_stage_passed_far_enough_for_send_preparation
ocmosj_send_preparation_failed_no_tunnels = Known(true)
terminal = P222-CORRECTED-ATTRIBUTION OCMOSJ-NO-USABLE-TUNNEL-OR-GARLIC-PATH
```

Do not distinguish:

- outbound tunnel selection failure;
- inbound ACK tunnel absence;
- garlic constructor returning null;

unless additional exact evidence does so.

`STATUS_SEND_BEST_EFFORT_FAILURE (3)`

Pinned OCMOSJ emits this from `SendTimeoutJob`, which is installed on the
post-garlic dispatch path.

Allowed conclusion:

```text
ocmosj_post_dispatch_timeout_path_reached = Known(true)
```

If i2pr observed no TunnelData in the frozen 45-second window:

```text
terminal = P222-CORRECTED-ATTRIBUTION
           JAVA-DISPATCH-PATH-REACHED-I2PR-TUNNELDATA-NOT-OBSERVED
```

Do not claim the exact packet left the kernel or reached Router B solely from
this callback.

`STATUS_SEND_GUARANTEED_SUCCESS (4)`

Allowed conclusion:

```text
ocmosj_delivery_ack_path_passed = Known(true)
```

If the 45-second i2pr payload row still failed, stop and record an evidence
contradiction requiring a dedicated follow-up. Do not guess.

### F2. Multiple-status ordering

A tracked send may produce:

```text
ACCEPTED -> BEST_EFFORT_FAILURE -> GUARANTEED_SUCCESS
```

or another legal state progression.

Classification uses the strongest later evidence but MUST preserve the full
ordered sequence.

A later success supersedes a probable failure for delivery-path proof.

## 13. Work package G — optional fallback only if listener is insufficient

Do not implement router stats/log correlation unless this branch is reached.

Branch condition:

```text
selector exact
tracked SEND accepted
no decisive terminal status by 70 s
no i2pr payload by 45 s
```

Then, and only then, use one of:

1. read-only fresh-run Java stat deltas; or
2. exact-pinned OCMOSJ class log lines correlated by:
   - bounded send time window;
   - exact target Destination hash/base32;
   - helper session;
   - one-message fresh topology.

Coarse global greps are forbidden.

If correlation is ambiguous, emit:

```text
P222-OBSERVABILITY-GAP-OCMOSJ-POST-ACCEPT
```

and stop.

## 14. Work package H — P222 terminal taxonomy

Exactly one terminal per run:

```text
P222-OBSERVABILITY-GAP-PRE-EPOCH
P222-OBSERVABILITY-GAP-SELECTOR-EQUIVALENCE
P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-LOOKUP-PEER
P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-USABLE-LEASESET
P222-CORRECTED-ATTRIBUTION OCMOSJ-BAD-LEASESET
P222-CORRECTED-ATTRIBUTION OCMOSJ-EXPIRED-LEASESET
P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
P222-CORRECTED-ATTRIBUTION OCMOSJ-NO-USABLE-TUNNEL-OR-GARLIC-PATH
P222-CORRECTED-ATTRIBUTION JAVA-DISPATCH-PATH-REACHED-I2PR-TUNNELDATA-NOT-OBSERVED
P222-OBSERVABILITY-GAP-OCMOSJ-POST-ACCEPT
P222-EVIDENCE-CONTRADICTION-ACK-SUCCESS-WITHOUT-I2PR-PAYLOAD
P222-REVERSE-DELIVERY-PASSED
```

Do not invent additional terminal strings during execution.

If a new necessary class appears, STOP and amend/register a new plan.

## 15. Work package I — static guards

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh`.

Required guards:

### I1. Selector-equivalence guards

Reject:

- `PROBE_FANOUT = 3` as authoritative selector width;
- direct use of raw target hash in
  `selectFloodfillParticipants(...)`;
- `Collections.emptySet()` as the authoritative selector overload input;
- authoritative selector query without helper client DBID;
- selector pass derived only from Router B presence.

Require:

- `routingKeyGenerator().getRoutingKey`;
- `netdb.searchLimit`;
- `EXTRA_PEERS` equivalent value 1 documented in test-only code;
- `clientNetDb(clientDbid)`;
- exact selector width emitted in evidence.

### I2. Tracked-send guards

Require:

- `SendMessageStatusListener`;
- listener-enabled long `sendMessage`;
- `SEND_TRACKED`;
- `SEND_STATUS`;
- bounded nonce event storage;
- legacy `SEND` remains.

Reject:

- changing `i2cp.messageReliability`;
- changing helper tunnel lengths/quantities;
- changing Java timeout properties.

### I3. Timing guards

Require:

- 45-second i2pr delivery result frozen before status-only extension;
- diagnostic status deadline >= 60 s and <= 75 s;
- no reuse of the later deadline for the payload pass/fail row.

## 16. Work package J — focused local tests

Add tests for:

1. routing-key vs raw-key distinction;
2. dynamic selector width computation;
3. client DBID required;
4. selector empty → no lookup peer;
5. selector nonempty / B absent does not root-cause;
6. tracked-send parser accepts valid nonce/digest;
7. tracked-send parser rejects malformed nonce;
8. ordered status sequence retained;
9. ACCEPTED alone does not pass client-NetDB;
10. NO_LEASESET maps to client-NetDB usable-LS failure;
11. NO_TUNNELS maps only to combined OCMOSJ no-tunnel/garlic boundary;
12. BEST_EFFORT_FAILURE + no 45 s TunnelData maps to dispatch-path-reached boundary;
13. GUARANTEED_SUCCESS + no payload maps to evidence contradiction;
14. no terminal callback maps to observability gap;
15. later success overrides probable failure for strongest-evidence evaluation;
16. 70 s diagnostic deadline cannot alter 45 s payload outcome.

## 17. Work package K — exact-clean-head external run

Implementation MUST be committed before the counted run.

Record:

```bash
git rev-parse HEAD
git status --porcelain=v1
```

Porcelain MUST be empty.

Run:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

### K1. Retry budget

Plan 220 exceeded its own retry budget because of the known pre-epoch helper
publication race.

Plan 222 corrects that process defect explicitly.

Allowed:

- up to 3 total exact-head attempts;
- all attempts on the SAME committed implementation SHA;
- fresh scratch RouterContexts every attempt;
- no topology change;
- no timeout change;
- no code/config change between attempts.

If all 3 attempts stop before the authoritative epoch:

```text
P222-OBSERVABILITY-GAP-PRE-EPOCH
```

Close Plan 222 stopped. Do not keep rerunning until green.

If implementation changes after any run, that starts a new implementation SHA
and requires a new closure rationale; the earlier run cannot be the
authoritative counted result.

## 18. Verification commands

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

M6 evidence:

```bash
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
```

Focused:

```bash
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p222 -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
```

External:

```bash
test -z "$(git status --porcelain=v1)"
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

## 19. Acceptance criteria

Plan 222 closes only when ALL applicable items pass.

### Selector equivalence

1. helper source Destination hash is used as the client DBID.
2. `clientNetDb(clientDbid)` resolves to a client DB, not main fallback.
3. Java derives the routing key with its own `routingKeyGenerator`.
4. raw target hash and routing key are both recorded.
5. selector width matches pinned `IterativeSearchJob` logic.
6. authoritative selector no longer hard-codes N=3.
7. selector uses the production-equivalent self-ignore overload.
8. selected peers are bounded and recorded.
9. empty selector and nonempty selector have distinct semantics.
10. B absence from a nonempty selector is not by itself a root cause.

### Per-message send correlation

11. helper uses public listener-enabled `I2PSession.sendMessage`.
12. a unique nonce is returned and recorded.
13. payload length/digest match the intended reverse payload.
14. all listener events are retained in order.
15. ACCEPTED is treated only as admission.
16. exact failure statuses map only to documented boundaries.
17. no listener status is synthesized from logs.
18. no missing callback becomes false.
19. legacy `SEND` remains compatible.

### Timing/evidence

20. existing 45-second i2pr payload window is unchanged.
21. status-only observation is independently bounded to <=75 seconds.
22. later status polling cannot turn a 45-second payload fail into a payload pass.
23. exact implementation SHA precedes the authoritative run.
24. authoritative run starts clean.
25. no more than 3 exact-head attempts are used.
26. no tuning occurs between attempts.
27. exactly one P222 terminal is emitted per attempt.
28. one authoritative terminal is selected by the closure record.
29. Plan 193 and Plan 215 remain untouched.
30. no protocol/topology corrective leaks into this plan.

## 20. Stop conditions

Stop immediately if:

- exact selector still cannot be reproduced without reflection/private access;
- tracked listener API is unsupported by the actual helper session
  implementation;
- client DBID resolves only to main DB unexpectedly;
- status sequence conflicts with pinned source assumptions;
- all 3 exact-head runs stop pre-epoch;
- a required conclusion would need Java patching or topology mutation.

If exact selector is empty:

- stop with client-NetDB no-peer attribution;
- do not continue to OCMOSJ speculation.

If listener reports NO_LEASESET:

- stop with client-NetDB usable-LeaseSet failure;
- later protocol corrective belongs to a new plan.

If listener reports NO_TUNNELS:

- stop with combined OCMOSJ no-tunnel/garlic-path attribution;
- do not split it further without new evidence.

If listener reaches post-dispatch timeout and i2pr still saw no TunnelData:

- stop with Java-dispatch-path-reached / i2pr-TunnelData-not-observed boundary;
- next plan should inspect Java outbound tunnel delivery vs i2pr inbound tunnel
  handling.

If reverse payload arrives within the frozen 45-second window:

- stop with `P222-REVERSE-DELIVERY-PASSED`;
- register final Java-family requalification, not another diagnostic pass.

## 21. Closure evidence required

Create/update:

```text
plans/closure/mixed-router-interop/222-status.md
```

It MUST contain:

- implementation commit SHA;
- clean-tree proof;
- exact Java/i2pd pins;
- exact source facts used for selector-width/routing-key logic;
- helper client DBID;
- target hash and derived routing key;
- client facade identity / isClientDb result;
- effective search limit and selector width;
- selected peer hashes;
- target LeaseSet pre-send state;
- tracked send nonce;
- reverse payload SHA-256;
- ordered listener status sequence;
- frozen 45-second i2pr TunnelData/payload result;
- any status-only post-45-second observations;
- P222 terminal;
- retry count;
- every verification command/outcome;
- findings by severity;
- unblock audit for Plans 201, 204, 205, 218, 220, 221;
- smallest next plan implied by the evidence.

## 22. Handoff for smaller executor

Read these files first:

```text
plans/implementation/mixed-router-interop/222-m6-java-client-netdb-ocmosj-narrowing-corrective.md
plans/closure/mixed-router-interop/222-status.md
plans/closure/mixed-router-interop/220-status.md
crates/i2pr-daemon/tests/java_tunnel_external.rs
tests/integration/m6-interop/java/ReferenceRawDestination.java
tests/integration/m6-interop/java/ControlledRouter.java
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P220SelectorProbe.java
scripts/check-m6-mixed-router-acceptance-evidence.sh
```

Then execute only:

```text
B -> C -> D -> E -> F -> H -> I -> J -> verification -> K
```

Package G is conditional and MUST be skipped unless the public listener path
ends in the documented post-accept observability gap.

Do not touch production Rust router code unless a compile-only refactor is
strictly required to keep test helpers building. Any production behavioral
change means STOP.

## 23. Registration handoff

```text
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective

plan_201 = blocked-pending-plan222-client-netdb-ocmosj-narrowing
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan222-narrowing

next_executable_plan = 222-m6-java-client-netdb-ocmosj-narrowing-corrective
```

No topology/bootstrap/protocol corrective is authorized by this registration.
