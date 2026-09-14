# Plan 198 — M6 Java public-client final closure corrective

Status: **registered executable corrective**. This plan supersedes the Plan 194 interpretation that treated the Java SAM LeaseSet2 publication limitation as sufficient for final M6 closure. It does not discard the passing evidence from Plans 193, 196, or 197; it restores the original fail-closed Plan 194 acceptance boundary and supplies a public Java client path capable of exercising the missing LeaseSet2, destination, and Streaming rows.

## 1. Goal

Fully close the retained Milestone 6 independent-router interoperability criterion without weakening its acceptance contract.

The current `main` authority labels M6 passed even though the latest exact-pinned Java lane records 26 rows `passed`, 22 rows `blocked`, and 0 rows `failed`. The blocked set contains the layers Plan 194 explicitly made mandatory for two-family closure: remote LeaseSet2 lookup/publication, bidirectional destination delivery, and bidirectional Streaming. Routine CI is green, but no exact-head full two-family external workflow has passed on the closing revision.

Plan 198 must:

1. immediately restore fail-closed M6 authority;
2. retain the valid i2pd, Java topology, authenticated SSU2, PQ-option tolerance, and Java tunnel-install evidence;
3. replace the Java **SAM-only counted service destination** with an ordinary public Java client/API service that can own a published Standard LeaseSet2 in the controlled topology;
4. execute the missing Java NetDB/LeaseSet2, raw destination, and Streaming rows over the existing real mixed-router path;
5. make the final evidence checker reject any M6 closure with mandatory rows `blocked`, `failed`, or missing;
6. require an exact-head successful two-family external workflow before the M6 authority is allowed to move to passed;
7. unblock Plan 195 only after all of the above passes.

This is a closure corrective, not a new transport architecture and not a reason to reopen the already-passed i2pd work.

## 2. Starting authority and correction

Starting implementation head when this plan was registered:

```text
i2pr main = aba33e4fe2e0f27ca0786f4a25a549aec5cc2df6
routine CI = 34794444271 (success)
workspace = 2333 passed, 9 ignored
```

Retain:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
```

Reclassify the current Plan 194 final-closure interpretation:

```text
plan_194 = retained-partial-java-qualification-stopped-at-sam-ls2-publication-boundary
plan_194_final_closure_claim = superseded-by-plan198
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
plan_195 = registered-blocked-by-plan198
next_executable_plan = 198
```

The Plan 194 implementation is still useful evidence. Its 26 passing Java rows remain valid. Its 22 blocked rows are not failures, but they are also not final-acceptance passes.

## 3. Exact independent references

Do not change pins:

```text
i2pd 2.61.0
repository = PurpleI2P/i2pd
commit     = 635b013a612ff47278ef02acf8580a28e10e26c5
role       = retained first-family full mixed-router/Streaming proof via Plan 193

Java I2P 2.13.0
repository = i2p/i2p.i2p
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
role       = second-family final qualification
```

The Java checkout/cache must remain exact-pinned and unmodified. Test helpers may compile out-of-tree against the staged public jars, as Plan 196's `ControlledRouter.java` already does.

## 4. Why the existing SAM service destination is not the final gate

The current Java lane established an important implementation-family difference: in this private controlled topology the Java SAM bridge does not make the SAM-owned destination's LeaseSet2 available in the local NetDB in the way the i2pd SAM bridge does. That prevents the normal i2pr remote lookup from obtaining the reference LeaseSet2 and therefore blocks all downstream delivery and Streaming rows.

That observation is retained as a **SAM compatibility finding**, not promoted to an M6 pass.

Plan 194 §6 already defined the correct fallback: use ordinary SAM first, then use the shipped/public Java I2P client API when SAM cannot exercise a required service behavior. Plan 194 also explicitly permits the **reference-owned** service destination to use documented local zero-hop client tunnels; the counted i2pr path must still use real remote one-hop tunnels.

Therefore Plan 198 changes only the Java reference application used by the missing rows. It must not inject LeaseSets or private router state.

## 5. Public Java client precedent already in this repository

Reuse the high-level lifecycle pattern already proven by Plan 172 in:

```text
tests/integration/i2cp/external/java/i2cp_java_session_driver.java
```

That counted driver uses public:

```text
I2PClientFactory.createClient()
I2PClient.createSession(...)
I2PSession.connect()
I2PSession.sendMessage(...)
I2PSessionListener / receiveMessage(...)
```

and requests a real non-empty zero-hop Standard LeaseSet2 through ordinary client behavior.

Its established zero-hop option profile is:

```text
inbound.length=0
outbound.length=0
inbound.quantity=1
outbound.quantity=1
inbound.backupQuantity=0
outbound.backupQuantity=0
inbound.allowZeroHop=true
outbound.allowZeroHop=true
i2cp.leaseSetType=3
i2cp.leaseSetEncType=4
i2cp.fastReceive=true
```

For Plan 198 the Java **reference service** must publish its LeaseSet2, so `i2cp.dontPublishLeaseSet=true` from the M9 local-only test must **not** be carried over. Omit that option or set it explicitly false.

Do not copy Plan 172's special destination construction unless the normal public destination creation path proves incompatible. Prefer ordinary `I2PClient.createDestination(...)` / transient public-client creation first.

For Streaming, exact-pinned Java I2P exposes public `net.i2p.client.streaming.I2PSocketManagerFactory`. Its `createDisconnectedManager(..., host, port, Properties)` API creates the public socket manager without connecting; for a server, `getSession().connect()` then builds/installs the client tunnels and LeaseSet before `I2PServerSocket` accepts connections. Use this public surface rather than implementing Streaming or I2CP framing in the helper.

Pinned upstream source reference:

```text
apps/ministreaming/java/src/net/i2p/client/streaming/I2PSocketManagerFactory.java
@ 9134f808337b401e8e53c73734c81fab04280c9d
```

## 6. Required Java reference helpers

Prefer two small, purpose-specific out-of-tree helpers under:

```text
tests/integration/m6-interop/java/
```

Suggested names:

```text
ReferenceRawDestination.java
ReferenceStreamingService.java
```

A single helper is acceptable only if it stays smaller and clearer than two independent helpers. Do not build another router/harness framework.

### 6.1 `ReferenceRawDestination.java`

Use only public I2P client APIs.

Required behavior:

1. connect to the controlled Plan 196 Java router over the harness-selected loopback I2CP endpoint;
2. create/load one destination using public client APIs;
3. request the zero-hop profile in §5 with Standard LeaseSet2 type 3 and X25519 encryption type 4;
4. publish the LeaseSet2 (`dontPublishLeaseSet` absent/false);
5. emit only sanitized coordination facts to the harness:
   - destination Base64/public hash;
   - `session.connect()` success;
   - timing/status;
   - payload lengths/digests;
6. receive a raw message from i2pr using the ordinary `I2PSessionListener` / `receiveMessage()` path and prove byte/digest equality;
7. send a raw message to the i2pr destination using ordinary `I2PSession.sendMessage()` and prove the i2pr side receives/decrypts it;
8. destroy the public client session cleanly.

A tiny localhost control channel/stdin protocol may coordinate destination strings, payload test IDs, and digests. It must not implement I2P protocol framing or expose private destination keys.

### 6.2 `ReferenceStreamingService.java`

Use only public Streaming APIs:

```text
I2PSocketManagerFactory
I2PSocketManager
I2PServerSocket
I2PSocket
```

Required behavior:

1. create a public socket manager against the selected loopback I2CP endpoint with the same published zero-hop Standard LS2 profile;
2. call the manager session's normal connect lifecycle before the server is counted ready;
3. publish the reference Destination to the harness;
4. Direction A: accept an i2pr-originated I2P stream through `I2PServerSocket`, exchange small + multi-packet payloads/replies, and prove digests;
5. Direction B: use the public manager/socket API to connect from Java to the i2pr listening destination, exchange small + multi-packet payloads/replies, and prove digests;
6. exercise orderly close and the required half-close/EOF behavior supported by the retained Plan 193 semantics;
7. exercise at least one sibling connection while the first stream closes so isolation is independently visible;
8. destroy manager/session resources and return to clean baseline.

Do not call internal Streaming implementation classes directly. Do not instantiate private router jobs or insert LeaseSets manually.

## 7. `run-java.sh` integration

Modify the existing:

```text
tests/integration/m6-interop/run-java.sh
```

Do not create a parallel Java harness.

Required flow:

1. verify exact Java pin/cache and Plan 196 cache immutability;
2. start `ControlledRouter.java` exactly as retained from Plan 196;
3. prove all Plan 196 topology invariants and authenticated SSU2 preflight;
4. compile Plan 198 Java public-client helper(s) into the ephemeral scratch directory using the exact-pinned staged jars;
5. start the raw helper and wait for public-client `session.connect()` plus published LS2 readiness;
6. pass the helper's public Destination to the existing Rust Java destination driver;
7. execute the full raw destination rows;
8. stop raw helper cleanly;
9. start the Streaming helper, wait for its public manager/session to become usable, and pass its public Destination to the existing Rust Streaming driver;
10. execute both Streaming directions and mandatory data/close/isolation rows;
11. stop helper and controlled router cleanly;
12. verify reference cache fingerprint remains unchanged;
13. emit only sanitized evidence into the existing M6 evidence tree.

The current SAM `STYLE=RAW` row may remain as a diagnostic compatibility row. It must not be the source of the mandatory LS2/delivery/Streaming pass rows.

## 8. Rust external-driver changes

Continue using:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
```

Do not duplicate the i2pr mixed-router implementation.

The counted Java reference destination should be supplied by the Plan 198 helper through an explicit environment/evidence handoff such as:

```text
JAVA_RAW_REFERENCE_DESTINATION_B64
JAVA_STREAM_REFERENCE_DESTINATION_B64
```

Use names consistent with the existing harness style.

### 8.1 Raw destination path

The Rust driver must prove, through the existing production path:

```text
i2pr destination
 -> existing DestinationRouting
 -> remote LeaseSet2 lookup through real outbound exploratory tunnel
 -> Java floodfill/local NetDB
 -> published Java public-client Standard LeaseSet2
 -> selected Java zero-hop lease
 -> existing ECIES/Garlic + Plan 192 Data envelope
 -> real outbound tunnel / Java router
 -> public Java I2PSession receive API
```

and reverse:

```text
public Java I2PSession.sendMessage(i2pr Destination)
 -> Java router normal NetDB/client routing
 -> i2pr published LS2 / real inbound one-hop tunnel
 -> i2pr Garlic/ECIES/Data dispatch
 -> destination receive
```

The helper's zero-hop lease is legal only on the **reference client side**. The i2pr creator path must continue to reject `LocalZeroHop` or direct-SSU2 substitutes as counted evidence.

### 8.2 Streaming path

The Rust driver must use the existing single `StreamingManager` implementation. No test-only Streaming codec, packet injector, or Java-specific branch may be introduced.

Both directions must traverse the exact same destination routing layer proven by §8.1.

## 9. Mandatory Java rows that must become passed

At minimum, the final Java family ledger must have command-derived `passed` evidence for all of the following. Use the repository's existing canonical labels where they already exist; do not silently rename guarded rows to escape old evidence requirements.

### Transport/tunnel prerequisites retained and revalidated

```text
external-reference-verified-java
external-session-established-java
external-tunnel-build-accepted-java
java tunnel liveness / DeliveryStatus row
```

### NetDB and LeaseSet2

```text
external-netdb-lookup-tunnel-java
remote Java Standard LeaseSet2 lookup through the real outbound tunnel
Java public-client LeaseSet2 visibly stored/resolvable in controlled NetDB
local i2pr Standard LeaseSet2 publication/visibility through the real tunnel path
```

### Raw destination delivery

```text
external-destination-ls2-resolved-java
external-destination-message-roundtrip-java
i2pr -> Java raw payload digest equality
Java -> i2pr raw payload digest equality
no direct destination-over-SSU2 shortcut
no i2pr LocalZeroHop counted substitute
sibling/destination isolation as already required by the retained destination lane
```

### Streaming

```text
external-streaming-established-java
external-streaming-multipacket-digest-java
i2pr -> Java Established
Java -> i2pr Established
small payload digest both directions
multi-packet/large payload digest both directions
orderly close/EOF
required half-close behavior
sibling stream isolation
external-clean-resource-baseline
```

Plan 193's i2pd impairment/retransmission evidence remains retained; Plan 198 need not reproduce every packet-loss experiment against Java unless a Java-specific behavior forces it. Plan 194's original rule remains: omitted second-family robustness rows must be justified by implementation-independent deterministic/i2pd evidence.

## 10. Final evidence must have zero blocked mandatory rows

The existing static evidence checks are necessary but insufficient for final closure because they can confirm that a guarded row is wired while still allowing runtime status `blocked`.

Plan 198 must add a final closure gate, either as a new:

```text
scripts/check-m6-final-closure-evidence.sh
```

or as an explicit final mode in the existing:

```text
scripts/check-m6-mixed-router-acceptance-evidence.sh
```

The final gate must consume the actual external evidence ledger, not just source text.

When `milestone6_interoperable` is claimed, it must reject:

- any mandatory i2pd or Java family row with `blocked`;
- any mandatory row with `failed`;
- any mandatory row missing entirely;
- Java LS2/destination/Streaming pass inferred from SAM creation, source inspection, local tests, or i2pd evidence;
- hard-coded `passed` rows;
- an M6 passed authority while Plan 195 is executable but final ledger is not all-pass;
- missing exact i2pr head, i2pd pin, Java pin, or clean-reference facts;
- missing controlled/private topology facts;
- private Java router-state injection;
- Java source patching;
- direct destination-over-transport shortcuts;
- self-composed i2pr peer substitution;
- secret/private-key/raw-payload leakage;
- absent cleanup/resource baselines;
- absent exact-head external workflow provenance.

Before final closure, the final gate should print a compact count such as:

```text
mandatory_i2pd: N/N passed, 0 blocked, 0 failed, 0 missing
mandatory_java: N/N passed, 0 blocked, 0 failed, 0 missing
m6_final_closure: passed
```

Anything else exits nonzero.

Keep the structural checker in routine CI. The evidence-consuming final mode may run only in the external workflow if it requires external artifacts.

## 11. Exact-head external workflow is a hard closure gate

Update and execute:

```text
.github/workflows/m6-mixed-router-external.yml
```

Final workflow requirements:

1. checkout the exact candidate closing head;
2. verify/build exact i2pd pin;
3. verify/build exact Java pin;
4. run the retained i2pd mixed-router family lane or import only evidence that the final checker can prove belongs to the same exact candidate head where appropriate;
5. run the complete Plan 198 Java public-client lane;
6. run all per-layer structural checkers;
7. run the final evidence-consuming all-pass checker;
8. upload sanitized evidence only;
9. exit nonzero on any mandatory blocked/failed/missing row;
10. record the workflow run ID and exact head in Plan 198 status.

Require **at least one successful full two-family workflow on the exact closing head**. Prefer two complete successful runs on the same revision if practical and stable, matching the precedent used for Plan 193.

Routine CI success alone cannot close Plan 198.

## 12. Stop rules

Plan 198 should be allowed to fix only defects exposed by the public-client closure path that are within the already-claimed M6 protocols.

### 12.1 Allowed narrow fixes inside Plan 198

A small standards-conformance correction may land in the same plan only when:

- the first failing boundary is unambiguous;
- the behavior is already required by the existing I2P specification/profile;
- the change does not add a new protocol family or major subsystem;
- focused deterministic regression tests are added;
- i2pd first-family evidence is rerun if the shared path changes.

### 12.2 Must stop and register a new corrective

Stop rather than stretch Plan 198 if the first failing boundary requires:

- a new cryptographic suite;
- Java private/internal router API use;
- public-network participation to obtain evidence;
- an i2pr transit/floodfill role not otherwise implemented;
- a new tunnel protocol or transport family;
- a new Streaming implementation;
- relaxing the two-family criterion;
- converting mandatory rows to optional/blocked acceptance.

Record exact stop provenance and retain all previously passed evidence.

## 13. No-go rules

Do not:

- patch the Java I2P or i2pd reference sources;
- use the public I2P network or public reseed;
- use root/namespaces/Docker/VM/systemd as a runtime requirement for the local controlled lane;
- use `i2p.vmCommSystem=true`;
- call private Java router classes to insert RouterInfos, LeaseSets, tunnels, Garlic messages, or Streaming packets;
- manually implement I2CP or Streaming wire framing in the Java helpers;
- inject Java private keys into i2pr;
- install creator-known fake tunnel material;
- substitute direct SSU2 application delivery;
- let the i2pr side use a local zero-hop path for counted mixed-router rows;
- mark `blocked` as a final M6 pass;
- unblock Plan 195 before Plan 198 final evidence passes.

## 14. Routine/local quality floor

Before external qualification, require the normal repository floor on the implementation head:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo test --locked --doc --workspace   # if still part of repository floor
cargo deny check advisories bans sources
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

Run all retained M6/SAM/I2CP/service-tunnel evidence/boundary scripts. Do not reduce their row counts to make Plan 198 green.

## 15. Documentation and authority normalization

Registration of Plan 198 immediately makes its status record the newest authority for this blocker line. Implementation must additionally normalize every central status surface that currently claims the bounded Plan 194 gap is final closure, including at least:

```text
plans/194-status.md
plans/195-status.md
plans/README.md
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
.opencode/skills/i2pr-architecture/SKILL.md
```

While Plan 198 is open, all must converge on:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = registered-executable-m6-java-public-client-final-closure-corrective
plan_195 = registered-blocked-by-plan198
milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
next_executable_plan = 198
remaining_sequence = 198 -> 195
```

Do not rewrite historical evidence; explicitly mark superseded interpretations.

## 16. Final Plan 198 acceptance criteria

Plan 198 may pass only when **all** of the following are true:

1. Plan 193's i2pd full mixed-router/Streaming evidence remains valid;
2. exact-pinned unmodified Java I2P 2.13.0 runs in the Plan 196 controlled/private topology;
3. Plan 197 PQ-option tolerance remains regression-green without enabling PQ negotiation;
4. authenticated Java SSU2 session passes;
5. real one-hop inbound and outbound tunnels through Java pass;
6. creator tunnel liveness passes;
7. RouterInfo NetDB lookup/publication required by the controlled path passes through real tunnels;
8. the Java public-client reference destination creates and publishes a Standard LeaseSet2 through ordinary public API behavior;
9. i2pr resolves that remote Standard LeaseSet2 through its real tunneled NetDB path;
10. i2pr publishes its own Standard LeaseSet2 and Java can route to it through ordinary client behavior;
11. raw destination delivery passes i2pr -> Java with digest equality;
12. raw destination delivery passes Java -> i2pr with digest equality;
13. no direct transport or i2pr local-zero-hop substitute is counted;
14. Streaming reaches `Established` i2pr -> Java;
15. Streaming reaches `Established` Java -> i2pr;
16. small and multi-packet/large payload digests pass both directions;
17. close/EOF and required half-close semantics pass;
18. sibling stream isolation passes;
19. clean resource baselines pass;
20. every mandatory Java final row is `passed` — **0 blocked, 0 failed, 0 missing**;
21. every mandatory retained i2pd final row remains passed;
22. structural evidence checkers pass;
23. the new runtime final-closure evidence checker passes;
24. the full workspace/static/dependency floor passes on the exact implementation head;
25. routine exact-head CI is green;
26. at least one exact-head `M6 mixed-router external interoperability` workflow completes successfully with both families and final checker green;
27. exact head + both reference pins + workflow run IDs are recorded in `plans/198-status.md`;
28. central docs/skills/authority records agree on the same bounded but fully demonstrated M6 claim;
29. Plan 195 remains blocked until criteria 1–28 are complete.

No criterion may be waived by relabeling the Java SAM LS2 limitation as a bounded final pass.

## 17. Closure transition

Only after §16 passes may authority become:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_198 = passed-m6-java-public-client-complete-second-family-closure

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = passed-via-plan198
milestone6_interoperable = passed-via-plan193-and-plan198

plan_195 = unblocked-m10-remote-service-interop
next_executable_plan = 195
next_product_layer = milestone10-remote-service-final-closure
```

The final M6 claim remains intentionally bounded to the controlled path actually proven. It does **not** imply public-network production readiness, arbitrary peer diversity, i2pr transit/floodfill roles, arbitrary NAT/IPv6 interop, SSU1, PQ key exchange, or every Streaming option.

## 18. Handoff order

Execute in this order:

```text
A. restore fail-closed authority / keep Plan 195 blocked
B. implement public Java raw-destination helper
C. prove published Java Standard LS2 + raw bidirectional delivery
D. implement public Java Streaming helper
E. prove Streaming both directions + data/close/isolation
F. harden final evidence checker to require zero blocked mandatory rows
G. run full local/routine quality floor
H. run exact-head two-family external workflow until one clean success exists
I. normalize all authority/docs
J. mark Plan 198 passed and only then unblock Plan 195
```

Do not begin Plan 195 in parallel.