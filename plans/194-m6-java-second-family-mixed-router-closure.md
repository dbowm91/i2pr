# Plan 194 — M6 Java I2P second-family qualification and final mixed-router closure

Status: **registered, blocked by Plan 193**. This plan supersedes the execution role of historical `plans/189-m6-java-i2p-second-family-qualification-and-closure.md`. Retain Plan 189's already-landed cross-family ledger/checker/workflow scaffold; do not discard or duplicate it.

## 1. Goal

Qualify the same bounded mixed-router destination/Streaming path proven against i2pd under Plan 193 against a second independent implementation family: exact-pinned unmodified Java I2P 2.13.0.

Plan 194 is the only plan allowed to set:

```text
milestone6_interoperable = passed-via-plan194
```

The claim remains bounded to the controlled MVP path actually demonstrated. It does not imply public-network readiness, transit/floodfill support by i2pr, arbitrary peer diversity, or every Streaming/crypto option.

## 2. Prerequisites

Do not execute before:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
milestone6_i2pd_streaming_interop = passed-via-plan193
```

Retain all first-family evidence from Plans 184–193. Java failures must never invalidate passing i2pd evidence unless they expose a shared i2pr regression that can be reproduced against the i2pd lane.

The Plan 189 scaffold is retained:

```text
scripts/check-m6-mixed-router-acceptance-evidence.sh
tests/integration/m6-interop/run-m6-mixed-router.sh
.github/workflows/m6-mixed-router-external.yml
```

Plan 194 should complete those surfaces rather than create a parallel acceptance system.

## 3. Exact reference

Use unmodified:

```text
Java I2P 2.13.0
repository = i2p/i2p.i2p
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
```

Requirements:

- exact commit verified before build/run;
- clean checkout, no source patching;
- Java 17+ and Ant/official build path as required by the pin;
- fresh disposable user-writable datadir;
- loopback-only controlled services/transports where supported;
- no public reseed or public-network dependency for green evidence;
- no root/sudo/namespaces/Docker/VM/systemd requirement.

If stock Java I2P cannot satisfy the private controlled topology, stop with the exact configuration/runtime blocker and register one narrow topology corrective. Do not silently switch to the public I2P network.

## 4. Controlled topology

Use one Java router at a time as the reference family. The reference may serve three bounded roles needed by the private test topology where stock behavior permits:

- authenticated SSU2 peer;
- one-hop tunnel participant / controlled NetDB peer;
- owner of the reference destination/service exposed through public SAM/I2CP-facing APIs.

Reference-side service destinations may use documented zero-hop client tunnel options if required. i2pr counted mixed-router paths must continue to use real remote one-hop tunnel material.

The test must never inject Java private router state through internal APIs.

## 5. Qualification layers

Plan 194 is a family-independence qualification, not merely another application echo. Execute the following layers in order and stop at the first failing boundary.

### 5.1 Authenticated transport

Prove the existing Plan 184 daemon-owned SSU2 runtime establishes an authenticated session with the Java pin and exchanges an authenticated I2NP control message through the ordinary dispatcher.

Do not change SSU2 semantics unless the exact failure proves a standards-conformance gap.

### 5.2 One-hop tunnel construction and liveness

Build one real outbound and one real inbound one-hop tunnel through Java using the same production short-build coordinator used for i2pd.

Require:

- replies consumed and authenticated through production paths;
- no creator-known synthetic installation;
- installed material in the existing pool/registry;
- creator-side liveness DeliveryStatus succeeds before the idle-delete boundary;
- unequal gateway/local tunnel-ID semantics remain preserved.

If Java uses a standards-permitted reply form different from i2pd, support the alternative through a typed standards-conformant branch with focused regressions. Do not add Java fingerprint branching unless the protocol genuinely permits both forms.

### 5.3 NetDB RouterInfo lookup/publication

Through the real exploratory tunnel pair:

- RouterInfo lookup returns a validated signed record;
- publication/store path obtains protocol-derived success where the controlled topology supports it;
- direct transport is not counted as the NetDB path;
- reply path uses the real inbound gateway tuple from Plan 190.

### 5.4 Destination LeaseSet2 and raw ECIES/Garlic delivery

Prove:

- reference Standard LeaseSet2 resolves through the live NetDB path;
- local Standard LeaseSet2 publication/visibility succeeds;
- Java accepts the Plan 192 9-byte short-transport + I2CP-style Data envelope;
- raw destination application payload succeeds i2pr -> Java with digest equality;
- raw destination application payload succeeds Java -> i2pr with digest equality;
- direct transport / LocalZeroHop substitutes remain rejected for counted i2pr paths.

### 5.5 Streaming both directions

Repeat the essential Plan 193 Streaming path against Java:

- i2pr initiator -> Java responder reaches `Established`;
- Java initiator -> i2pr listener reaches `Established`;
- small payload digest each direction;
- multi-packet/large payload digest each direction;
- orderly close/half-close;
- sibling isolation;
- clean resource baseline.

A full duplicate of every Plan 193 impairment row is not required if the behavior is implementation-independent and already proven by local deterministic tests plus i2pd external evidence. The status file must justify every omitted external row.

## 6. Java reference application

Use public Java router client surfaces only. Preferred order:

1. ordinary SAM 3.x public interface;
2. shipped/public Java I2P client API if SAM cannot exercise a required service behavior.

Tiny test applications may drive those public APIs but must not implement I2P transport/tunnel/NetDB/Garlic/Streaming framing themselves.

Never call private Java router classes to insert tunnels, LeaseSets, NetDB entries, or Streaming packets.

## 7. Cross-family evidence

Complete the Plan 189 cross-family ledger. Required family-qualified rows should include at least:

```text
m6-i2pd-ssu2
m6-i2pd-tunnels
m6-i2pd-netdb
m6-i2pd-destination-bidirectional
m6-i2pd-streaming-a
m6-i2pd-streaming-b

m6-java-ssu2
m6-java-tunnels
m6-java-liveness
m6-java-netdb
m6-java-leaseset2
m6-java-destination-outbound
m6-java-destination-inbound
m6-java-streaming-a
m6-java-streaming-b
m6-java-clean-resource-baseline
```

The final unified evidence must bind to:

- exact i2pr implementation head;
- exact i2pd pin retained from Plan 193;
- exact Java pin;
- clean reference checkouts;
- controlled/private topology;
- real one-hop tunnel proof;
- NetDB proof;
- Standard LeaseSet2 proof;
- bidirectional destination delivery;
- bidirectional Streaming;
- resource cleanup.

No row may become passed solely from source inspection or a status document.

## 8. Cross-family semantic comparison

Record a sanitized comparison table for the two reference families:

- SSU2 establishment behavior;
- short-build request/reply shape;
- tunnel liveness behavior;
- DatabaseLookup/Store/SearchReply behavior;
- Standard LeaseSet2 signing/encryption-key profile;
- destination Data envelope handling;
- Streaming SYN/options/ports/ACK/close behavior;
- every standards-permitted implementation difference exercised by i2pr.

This table is evidence for compatibility reasoning, not a replacement for command-derived pass rows.

## 9. Evidence checker requirements

Extend the existing `scripts/check-m6-mixed-router-acceptance-evidence.sh` so it rejects:

- missing Java invocation while claiming two-family closure;
- Java row inferred from i2pd pass;
- patched Java source;
- public-network/reseed dependency hidden in the lane;
- synthetic tunnel material or private state injection;
- direct destination-over-transport substitution;
- self-composed i2pr peer substitution;
- missing Streaming direction A or B;
- hard-coded pass rows;
- missing exact-head/pin cleanliness evidence;
- private key or application plaintext leakage;
- missing cleanup/resource baseline.

Keep the checker in routine CI. The complete external lane may remain manual.

## 10. Hosted workflow

Complete `.github/workflows/m6-mixed-router-external.yml` so the Java-family path actually executes. It must:

1. verify exact head;
2. fetch/build exact clean Java pin;
3. execute the Java qualification layers in order;
4. execute or import retained exact-head i2pd Plan 193 evidence as required by the final ledger;
5. run the fail-closed cross-family checker;
6. upload only sanitized evidence;
7. exit nonzero on any missing family/layer row.

Before final closure, require at least one exact-head full two-family workflow pass. Prefer two complete passes on the same revision if practical.

## 11. Failure policy

If Java fails where i2pd passes:

1. stop at the first failing protocol boundary;
2. minimize the reproducer;
3. compare current official specification, Java pin, and retained i2pd behavior;
4. classify whether i2pr is wrong, Java differs within spec, or the private topology is impossible under stock Java configuration;
5. create one narrow corrective if needed;
6. keep Plan 193 i2pd evidence retained-passed;
7. do not weaken the two-family acceptance criterion.

Do not combine multiple newly discovered protocol corrections into Plan 194 merely to reach closure.

## 12. Final M6 acceptance criteria

Plan 194 passes only when:

1. Plan 193 is passed on retained exact-head evidence;
2. exact-pinned unmodified Java I2P 2.13.0 runs in the controlled environment without public-network dependency;
3. authenticated SSU2 session passes;
4. real inbound/outbound one-hop tunnel construction passes;
5. tunnel liveness passes;
6. RouterInfo NetDB lookup/publication passes through tunnels;
7. remote Standard LeaseSet2 lookup/local publication passes;
8. raw destination ECIES/Garlic delivery passes both directions using the Plan 192 envelope;
9. Streaming establishes both directions;
10. small/large data and close semantics pass;
11. no patched reference/private injection/direct shortcut is counted;
12. fail-closed cross-family ledger/checker passes;
13. full workspace/static/dependency floor passes;
14. exact-head routine CI is green;
15. exact-head external two-family workflow is green;
16. support/CONFORMANCE/architecture/status docs state the same bounded claim.

## 13. Closure transition

Only after all acceptance criteria pass:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = passed-m6-java-second-family-mixed-router-closure
milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = passed-via-plan194
milestone6_interoperable = passed-via-plan194
next_executable_plan = 195
next_product_layer = milestone10-remote-service-final-closure
```

Explicitly retain unsupported/deferred scope:

- public-network production readiness;
- arbitrary peer diversity;
- i2pr transit role;
- i2pr floodfill role;
- arbitrary NAT/IPv6 external interoperability;
- every Streaming option/crypto suite;
- SSU1/PQ transport families not already supported by their own milestones.

## 14. Handoff

Plan 195 resumes the two remote M10 application rows that Plan 181 correctly blocked on M6 mixed-router Streaming. Plan 194 closes M6 only; it does not close M10.