# Plan 200 — M6 Java public-client publication observability and verified bootstrap

Status at registration: **registered-executable**.

This is a **diagnostic/evidence corrective**, not a product-feature pass. It exists because Plan 199 proved the two-router Java topology was plausible but did not prove the intermediate state transitions required to explain why the public-client Standard LeaseSet2 remained network-invisible.

Plan 200 supersedes the diagnostic portion of Plan 199 Phase A. It does **not** supersede Plan 199's final acceptance criteria, Plan 193's retained i2pd evidence, or Plan 198's requirement for a complete Java-family destination/Streaming pass.

## 1. Goal

Produce one fail-closed external Java run that identifies the **first real failing boundary** in the exact-pinned Java I2P 2.13.0 public-client publication path.

The run must distinguish all of the following instead of collapsing them into `client-ls2-local-but-not-network-visible`:

```text
A. Router A does not actually know Router B in its main NetDB
B. Router B does not actually know Router A in its main NetDB
C. public I2CP session is connected but no current Standard LS2 is created/stored
D. LS2 exists but no eligible client tunnel pair exists for publication
E. client publication has no eligible floodfill candidate
F. DatabaseStore is emitted to Router B but no acknowledgement is observed
G. Router B acknowledges/stores the LS2 but an ordinary remote lookup still fails
H. all publication boundaries pass
```

No implementation agent may choose a corrective branch until this plan records one of those outcomes with command-derived evidence.

## 2. Frozen source and reference versions

Use the existing exact pins without substitution:

```text
Java I2P 2.13.0
commit 9134f808337b401e8e53c73734c81fab04280c9d

i2pd 2.61.0 retained first-family evidence
commit 635b013a612ff47278ef02acf8580a28e10e26c5
```

Relevant exact-pinned Java behavior already established by Plans 198/199:

- `ClientConnectionRunner` creates a client-specific `FloodfillNetworkDatabaseFacade` for the primary destination.
- client sub-NetDBs share the main NetDB peer selector / KBucket set but do not register incoming DatabaseLookup/DatabaseStore handlers.
- `ClientMessageEventListener` calls the client facade's `publish(ls)` after accepting a client `CreateLeaseSet` / `CreateLeaseSet2`.
- `KademliaNetworkDatabaseFacade.publish(LeaseSet)` stores the local LS and schedules `RepublishLeaseSetJob`.
- `RepublishLeaseSetJob` calls `_facade.sendStore(...)` only while the destination remains local/current/publishable.
- `StoreJob` chooses floodfills from the facade's routing table, requires usable reply/outbound client tunnel state for LeaseSet publication, and fails when no eligible path exists.
- the Plan 199 helper currently prints `READY` immediately after `session.connect()`; that is session readiness, not proof of network-visible LS2 publication.

## 3. Current defect in the evidence model

At head `71942008d40f58507181f286449f979c92ee7c90`:

- `bootstrap_java_router_peers` submits ordinary RouterInfo DatabaseStore messages and treats `RouterDeliveryOutcome::Accepted` as sufficient bootstrap proof.
- the bootstrap messages use `reply_token = 0`, so the lane obtains no protocol acknowledgement.
- the public Java helper prints `READY` after `I2PSession.connect()`.
- the Rust driver records `session=connected leaseset=published` before any external publication proof exists.
- a real tunneled DatabaseLookup then receives no LS2 response within the bounded window.

Those facts prove the final symptom but do not identify the failed Java publication stage.

Plan 200 must correct the semantics before attempting another topology/product fix.

## 4. Scope

Plan 200 may change only:

- Java external-test helpers under `tests/integration/m6-interop/java/`;
- the Java M6 external runner under `tests/integration/m6-interop/`;
- `crates/i2pr-daemon/tests/java_tunnel_external.rs` diagnostic/bootstrap probes;
- M6 external evidence/static checker logic needed to validate the new diagnostic facts;
- Plan/status/docs that describe this diagnostic boundary.

Plan 200 must **not**:

- change normal i2pr protocol behavior;
- change destination/Streaming wire formats;
- change SSU2/NetDB/tunnel algorithms merely to make Java green;
- patch the exact-pinned Java checkout;
- call private/package-private Java NetDB insertion APIs;
- inject a LeaseSet directly into either Java router or i2pr;
- use VMComm;
- join the public I2P network;
- silently replace Java with i2pd for the second-family row.

If a real i2pr protocol defect is discovered, record it as the Plan 200 result and leave the fix to Plan 201.

## 5. Required topology

Retain the smallest controlled topology that can answer the question:

```text
Router A = exact-pinned Java service/client router
  - public I2CP enabled on loopback
  - public Java helper owns the test Destination
  - separate disposable RouterContext/data directory

Router B = exact-pinned Java publication/floodfill router
  - floodfill participant
  - separate disposable RouterContext/data directory

Probe router = i2pr test-owned SSU2/I2NP endpoint
  - used only for ordinary protocol bootstrap/lookup/evidence
  - not a fake floodfill
```

Both Java routers remain loopback-only, reseed-disabled, non-public, real SSU2, and independently keyed.

Do **not** add Router C in Plan 200. If evidence proves a third router is necessary for normal client-tunnel eligibility, record that as the diagnostic result for Plan 201.

## 6. Phase A — correct evidence semantics

### A.1 Helper readiness

Change `ReferenceRawDestination.java` and `ReferenceStreamingService.java` so `READY` means only:

```text
public helper process alive
I2CP session established
Destination public material available
control socket ready
```

Do not emit or imply `leaseset=published` at helper readiness.

Recommended sanitized helper facts:

```text
PUBLIC_CLIENT_SESSION_CONNECTED=1
PUBLIC_CLIENT_DESTINATION_LEN=<bounded integer>
PUBLIC_CLIENT_CONTROL_READY=1
```

Never retain private destination bytes.

### A.2 Driver terminology

Replace any current evidence text that equates `session.connect()` with LeaseSet publication.

Required distinction:

```text
public-client-session-established
public-client-leaseset-created
public-client-leaseset-publication-attempted
public-client-leaseset-publication-acked
public-client-leaseset-network-visible
```

A row may only be `passed` if its exact condition is independently observed.

## 7. Phase B — prove Router A/B main-NetDB bootstrap

The Plan 199 bootstrap only proves that i2pr accepted a send request. Plan 200 must prove that the receiving **Java main NetDB** can subsequently serve the peer RouterInfo.

### B.1 Ordinary store remains mandatory

Keep ordinary signed RouterInfo DatabaseStore delivery over authenticated SSU2/I2NP.

No filesystem copying into `netDb/`, reflection, Java method invocation, or direct object insertion is allowed.

### B.2 Add positive post-store proof in both directions

After the stores, run ordinary DatabaseLookup probes:

```text
query Router A main NetDB for Router B's RouterInfo
query Router B main NetDB for Router A's RouterInfo
```

Each probe must receive a normal DatabaseStore RouterInfo response whose:

- key equals the requested RouterHash;
- RouterInfo signature validates;
- identity hash matches the expected peer;
- published SSU2 address matches the disposable peer;
- response arrived over the ordinary protocol path.

A DeliveryStatus acknowledgement for the bootstrap store may be added as an additional fact, but it does **not** replace the post-store lookup proof.

Required rows:

```text
java-main-netdb-a-knows-b
java-main-netdb-b-knows-a
```

If either fails, stop before starting the public helper and classify `routerinfo-bootstrap-not-installed`.

## 8. Phase C — observe the Java client LeaseSet lifecycle

Use stock Java behavior and disposable logging configuration. Do not modify Java source.

Enable sufficiently narrow DEBUG/INFO logging or stock stats for these exact classes/boundaries:

```text
net.i2p.router.client.ClientMessageEventListener
net.i2p.router.client.ClientConnectionRunner
net.i2p.router.networkdb.kademlia.KademliaNetworkDatabaseFacade
net.i2p.router.networkdb.kademlia.RepublishLeaseSetJob
net.i2p.router.networkdb.kademlia.StoreJob
net.i2p.router.networkdb.kademlia.FloodfillStoreJob
```

The raw Java logs remain scratch-only. Extract only sanitized facts to evidence.

### C.1 Required sanitized lifecycle facts

At minimum prove or disprove:

```text
java-client-subdb-created
java-create-leaseset2-received
java-client-leaseset-stored-current
java-client-leaseset-publish-scheduled
java-client-leaseset-republish-job-ran
```

Hashes may be represented only as stable digests/truncated public identifiers when necessary to correlate events. Never copy private keys, encrypted session keys, or full sensitive log lines.

### C.2 No false positives

A log line saying `Publishing:` is proof only that Java reached the facade publication call. It is not proof that a floodfill store completed.

A stored client-subDB LS2 is not proof that Router B has it.

## 9. Phase D — isolate tunnel and floodfill selection

The publication job must be classified at the first boundary it reaches.

Extract stock evidence for:

```text
client inbound tunnel selectable? yes/no
client outbound tunnel selectable? yes/no
floodfill candidate list non-empty? yes/no
selected candidate == Router B? yes/no
DatabaseStore queued? yes/no
DatabaseStore reply/ack observed? yes/no
store failure reason if any
```

Do not infer tunnel eligibility merely from the helper being connected.

Particular stock failures to distinguish include:

```text
No reply inbound tunnels available
No more peers left and none pending
selected candidate missing RouterInfo
store timeout / failed peer
```

If zero-hop client pools are not eligible for the store path, record that fact. Do not change the helper profile under Plan 200.

## 10. Phase E — prove network visibility independently

Only after B/C/D reach `DatabaseStore acked` may the lane claim publication success.

Run an ordinary DatabaseLookup for the helper Destination against Router B's main/floodfill NetDB using the real M6 tunneled lookup path.

Required result:

```text
DatabaseStore Standard LS2 returned
LS2 signature valid
Destination hash matches helper Destination
lease count >= 1
lease expiration current
response ingested through existing i2pr LeaseSet2 validation/store path
```

Required row:

```text
java-public-client-leaseset-network-visible
```

Plan 200 does not need to continue through the full destination/Streaming matrix after this row; Plan 201 owns final M6 Java closure.

## 11. Machine-readable result classification

Write exactly one terminal classification to sanitized evidence and `plans/200-status.md` when executed:

```text
P200-A-router-a-missing-router-b
P200-B-router-b-missing-router-a
P200-C-client-ls2-not-created-or-current
P200-D-client-tunnel-publication-path-unavailable
P200-E-no-eligible-floodfill-candidate
P200-F-store-sent-no-ack
P200-G-store-acked-remote-lookup-fails
P200-H-publication-path-passed
```

If more than one symptom appears, choose the earliest protocol/lifecycle boundary. Record later symptoms only as secondary facts.

## 12. Static/evidence checker requirements

Extend the existing M6 Java checker so it rejects:

- `READY` being described as publication proof;
- `RouterDeliveryOutcome::Accepted` being described as Java NetDB installation proof;
- a passed publication row without the post-store RouterInfo lookup proofs;
- a passed network-visible LS2 row without an ordinary Router B DatabaseLookup/DatabaseStore transcript;
- raw Java logs copied into retained evidence;
- exact-pinned source mutation;
- private Java/internal NetDB calls;
- public-network/reseed dependence.

## 13. Validation commands

At minimum on the Plan 200 implementation head:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash tests/integration/m6-interop/run-java.sh
```

The external runner may finish with blocked downstream M6 rows. That is acceptable for Plan 200 if and only if it produces one unambiguous terminal classification above.

## 14. Acceptance criteria

Plan 200 passes only when all of the following are true:

1. Exact Java I2P 2.13.0 pin is unchanged and clean.
2. Router A and B use independent RouterContexts/data directories.
3. No VMComm/private NetDB injection/public reseed is used.
4. Helper `READY` no longer claims publication.
5. Driver evidence no longer equates session connect with publication.
6. Router A serving Router B's RI is proven by ordinary post-store lookup.
7. Router B serving Router A's RI is proven by ordinary post-store lookup.
8. Client subDB creation is positively observed or explicitly classified absent.
9. CreateLeaseSet2 receipt is positively observed or explicitly classified absent.
10. Current LS2 storage is positively observed or explicitly classified absent.
11. Publish scheduling/job execution is positively observed or explicitly classified absent.
12. Client inbound/outbound publication-tunnel availability is distinguished.
13. Floodfill candidate availability is distinguished.
14. Router B selection is distinguished from mere RouterInfo presence.
15. DatabaseStore emission is distinguished from acknowledgement.
16. Router B network visibility is tested through ordinary lookup.
17. Exactly one terminal `P200-*` classification is emitted.
18. Raw secret-bearing Java logs are scratch-only.
19. No normal i2pr product protocol behavior is changed solely to make this diagnostic pass.
20. Static checkers fail closed on missing diagnostic facts.

## 15. Handoff

On Plan 200 completion:

```text
next_executable_plan = 201
plan_201_input = exact P200 terminal classification + sanitized facts
```

Plan 201 must implement only the smallest correction justified by that classification.
