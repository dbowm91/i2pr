# Plan 201 — M6 Java public-client publication corrective and second-family closure

Status at registration: **registered-blocked-by-plan200**.

Plan 201 is the bounded corrective/closure pass for the Java I2P 2.13.0 second-family M6 lane. It may execute only after Plan 200 records exactly one terminal `P200-*` classification.

This plan replaces the open-ended “make Java publication work” portion of Plans 198/199 with a branch-controlled corrective. It must not re-diagnose the problem from scratch and must not implement multiple speculative fixes at once.

## 1. Goal

Starting from Plan 200's proven first failing boundary:

1. apply the smallest standards-compatible correction;
2. prove the public Java client Standard LeaseSet2 becomes network-visible through ordinary NetDB behavior;
3. rerun the existing Java destination + Streaming matrix;
4. close the Java second implementation family without weakening the retained i2pd family;
5. promote M6 interoperability only when both families have complete mandatory rows.

## 2. Preconditions

Required before implementation:

```text
plan_200 = passed
exactly one P200 terminal classification present
exact Java pin = 9134f808337b401e8e53c73734c81fab04280c9d
Plan 193 i2pd first-family evidence remains retained-passed
Plan 197 pq parser tolerance remains retained-passed
```

If Plan 200 is ambiguous, stop and correct Plan 200. Do not guess.

## 3. Correction matrix

Only the branch corresponding to the Plan 200 classification may be implemented initially.

### Branch A — `P200-A-router-a-missing-router-b` or `P200-B-router-b-missing-router-a`

Problem class: ordinary RouterInfo bootstrap was admitted by i2pr but not installed/servable by the Java main NetDB.

Required correction:

- fix the ordinary I2NP bootstrap exchange, ordering, acknowledgement, expiration, or return path;
- require the two post-store RouterInfo lookup proofs before public helpers start;
- retain signed stock RouterInfo bytes and normal DatabaseStore/DatabaseLookup semantics;
- do not copy NetDB files or call Java NetDB methods.

Do not change i2pr production NetDB code unless the captured transcript proves i2pr encoded an invalid I2NP message.

### Branch B — `P200-C-client-ls2-not-created-or-current`

Problem class: the public Java client session connects but the router/client lifecycle never produces a current Standard LS2.

Required correction:

- inspect the exact RequestVariableLeaseSet/CreateLeaseSet2 lifecycle and client tunnel readiness;
- correct helper options/profile only where the exact Java public API requires it;
- keep LS2 type 3 and X25519 encryption type 4;
- keep `i2cp.dontPublishLeaseSet=false`;
- retain an ordinary public `I2PClient`/`I2PSession` or `I2PSocketManager` client.

Forbidden:

- constructing the LeaseSet in the harness and inserting it into Java;
- private client/router callbacks;
- replacing the counted helper with SAM solely because SAM is easier.

### Branch C — `P200-D-client-tunnel-publication-path-unavailable`

Problem class: the LS2 exists, but Java's publication `StoreJob` has no usable client inbound/outbound tunnel path.

This branch must explicitly test whether the existing zero-hop public-client profile is the cause.

Correction order:

1. keep two Java routers and change only the counted helper to the **shortest normal client tunnel profile** accepted by Java (`length=1`, quantity=1, bounded backups), while leaving i2pr's counted side on real tunnels;
2. prove Router A can build the required client tunnel(s) through eligible Router B material;
3. only if exact logs/source prove two routers cannot simultaneously satisfy client-tunnel and floodfill roles, add one disposable Router C solely as the independent client-tunnel participant.

If Router C is required, freeze roles:

```text
A = service/public-client router
B = floodfill/publication target
C = client tunnel participant
```

Do not expand beyond three Java routers.

### Branch D — `P200-E-no-eligible-floodfill-candidate`

Problem class: Router B is present in A's main NetDB but Java does not select it as a floodfill store target.

Required correction:

- prove B's signed RouterInfo advertises the effective floodfill capability;
- prove A's main KBucket/peer selector includes B;
- allow normal Java transport/profile establishment needed to make B eligible;
- wait for normal profile state if the exact selector requires it;
- use ordinary Java-to-Java SSU2 where necessary.

Forbidden:

- fabricating peer profiles;
- changing B's signed RouterInfo after startup;
- patching Java's `FloodfillPeerSelector`.

### Branch E — `P200-F-store-sent-no-ack`

Problem class: Java selects B and emits the LS2 DatabaseStore but the normal acknowledgement does not complete.

Required correction:

- capture the ordinary store/reply path;
- verify client outbound tunnel selection, reply gateway/tunnel, DeliveryStatus routing, message expiration, and B's validation outcome;
- fix only the layer demonstrated defective.

If B rejects a malformed/unsupported LS2 produced by stock Java, re-check the topology/evidence before modifying i2pr; i2pr is not in that Java-to-Java LS2 creation path.

### Branch F — `P200-G-store-acked-remote-lookup-fails`

Problem class: Java B has acknowledged the store, but i2pr's ordinary lookup cannot retrieve/ingest it.

This is the first branch where an i2pr M6 product correction is likely justified.

Inspect in order:

```text
DatabaseLookup key/routing key
lookup target/floodfill selection
reply gateway + remote receive tunnel id
DatabaseStore LS2 decode/type
LS2 key match
signature/expiration validation
inbound tunnel reassembly
ingest into LeaseSet2Store
```

Reuse Plan 190's typed reply-path adapter and Plan 192's corrected destination delivery format. Do not create a Java-special lookup parser.

### Branch G — `P200-H-publication-path-passed`

No publication corrective is required. Proceed directly to the full Java destination/Streaming rerun and fix only any subsequently proven M6 protocol defect.

## 4. Common topology rules

Across all branches:

- exact-pinned stock Java source/build;
- disposable loopback RouterContexts;
- no public network;
- no VMComm;
- no private NetDB/tunnel state injection;
- no direct LS2 copying;
- no `LocalZeroHop` on the counted i2pr side;
- public Java client API owns the counted remote Destination;
- Java SAM remains diagnostic only unless a historical retained row explicitly requires it.

## 5. Required publication proof after correction

Before running destination payload rows, prove this exact sequence:

```text
A main NetDB serves B RouterInfo
B main NetDB serves A RouterInfo
public Java client session established
current Standard LS2 created and stored in client subDB
publication job executed
eligible client tunnel path exists
B selected as publication target
DatabaseStore emitted
store acknowledgement observed
ordinary i2pr DatabaseLookup to B returns the LS2
LS2 validates and installs in i2pr LeaseSet2Store
```

Every stage must have a sanitized fact. No stage may be inferred from a later timeout.

## 6. Resume the existing M6 Java matrix

Once network-visible LS2 publication passes, run the existing Java destination/Streaming drivers rather than creating a second framework.

Mandatory categories remain:

```text
authenticated SSU2 topology
real outbound + inbound tunnel install
remote Standard LS2 lookup
local Standard LS2 publication
bidirectional raw destination delivery
small Streaming payload
multi-packet Streaming payload
reverse-direction payload
sibling isolation
close/EOF behavior
manager/resource cleanup
no direct transport counted
```

Use `scripts/check-m6-final-closure-evidence.sh` as the machine-readable final row authority and evolve it only where Plan 200 introduced new mandatory proof rows.

## 7. Failure ownership after publication succeeds

If the next failure appears after network-visible LS2, classify it narrowly:

```text
J201-A = tunnel-build/install defect
J201-B = NetDB lookup/validation defect
J201-C = outbound destination/Garlic defect
J201-D = inbound destination delivery defect
J201-E = Streaming handshake defect
J201-F = Streaming data/reliability defect
J201-G = cleanup/resource defect
```

Do not reopen Java publication after it is positively proven unless a rerun regresses the proof.

## 8. Required validation

On the candidate closure head:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh
bash tests/integration/m6-interop/run-java.sh
bash tests/integration/m6-interop/run-m6-mixed-router.sh
cargo deny check advisories bans sources
```

If the external Java lane is flaky or requires retry, require two complete all-pass runs on the same exact head. A deterministic first-pass success may close with one complete exact-head run, though two are preferred.

## 9. Acceptance criteria

Plan 201 passes only when:

1. Plan 200 has one unambiguous terminal classification.
2. Only the justified corrective branch was implemented initially.
3. Exact Java pin remains clean/unmodified.
4. Public Java helper owns the counted destination through public APIs.
5. Router A/B bootstrap post-lookup proofs pass.
6. Current Standard LS2 creation is proven.
7. Client publication-tunnel eligibility is proven.
8. Floodfill target selection is proven.
9. LS2 DatabaseStore emission is proven.
10. Publication acknowledgement is proven.
11. Router B ordinary lookup serves the LS2.
12. i2pr validates/installs the remote LS2.
13. Real outbound and inbound i2pr tunnel rows pass.
14. Bidirectional raw destination delivery passes.
15. Direction A Streaming passes.
16. Direction B Streaming passes.
17. Small + multi-packet + reverse payload digests match.
18. Sibling isolation passes.
19. Close/cleanup/resource baselines pass.
20. No mandatory Java row is blocked, failed, or missing.
21. Retained i2pd first-family rows remain all-pass.
22. Cross-family M6 checker reports zero mandatory blocked/failed/missing rows.
23. No forbidden private-state/public-network shortcut was introduced.
24. Authority is not promoted until exact-head evidence exists.

## 10. Authority transition on success

Only after all criteria pass:

```text
plan_198 = passed-m6-java-public-client-complete-second-family-closure
plan_200 = passed-java-publication-boundary-diagnosis
plan_201 = passed-m6-java-public-client-publication-corrective-and-second-family-closure
milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = passed-via-plan201
milestone6_interoperable = passed-via-plan193-and-plan201
```

Plan 204 owns final cross-document normalization; Plan 201 may update its own status and immediately relevant M6 status files but should avoid broad M10 documentation churn.
