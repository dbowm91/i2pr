# Plan 189 — M6 Java I2P second-family qualification and mixed-router closure

Status: **blocked until Plan 188 passes**.

## 1. Goal

Requalify the mixed-router path against a second independent implementation family and close the retained Milestone 6 external interoperability debt only if the same essential tunnel/NetDB/destination/Streaming path works with exact-pinned Java I2P.

## 2. Exact reference

Use unmodified:

```text
Java I2P 2.13.0
repository = i2p/i2p.i2p
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
```

Java SDK 17+ and Ant/official build path may be used in the external lane. Verify exact commit and clean checkout before execution. A prepared dependency cache is acceptable; patching source or replacing protocol behavior is not.

I2P+ is not a second family relative to Java I2P and cannot substitute for this criterion.

## 3. Controlled Java topology

Run the Java router in a fresh unprivileged private datadir/config with:

- loopback transport/service bindings only where configurable;
- no public reseed/network dependency for green evidence;
- controlled floodfill availability sufficient for the test dataset;
- transit participation sufficient to accept i2pr's one-hop test tunnels;
- SAM enabled on loopback for the reference service destination.

The reference service may use documented zero-hop client options (`inbound.length=0 outbound.length=0`), but i2pr's counted mixed-router paths must remain real remote one-hop tunnels.

If stock Java configuration cannot simultaneously satisfy the isolated floodfill/transit/SSU2 topology without contacting the public network, capture the exact blocker and write a narrow topology corrective. Do not silently switch to public I2P.

## 4. Qualification breadth

Do not merely repeat one application echo. Prove the essential lower-layer boundaries that establish family independence:

1. authenticated SSU2 direct session under Plan 184 runtime ownership;
2. one-hop inbound and outbound Short Tunnel Build accepted by Java;
3. paired tunnel-liveness DeliveryStatus test succeeds;
4. RouterInfo NetDB lookup through exploratory tunnels succeeds;
5. controlled RouterInfo publication obtains protocol-derived success;
6. reference Standard LeaseSet2 resolves and validates;
7. i2pr destination Standard LeaseSet2 is visible to the reference path;
8. raw destination ECIES/Garlic message succeeds in both directions;
9. Streaming direction A and B both establish;
10. small and multi-packet data digests match;
11. close/resource baselines are clean.

A narrower subset is acceptable only if a row is literally implementation-independent and already proven below the reference boundary; the status record must justify every omitted repeated row.

## 5. Java reference application

Use Java router public SAM/client APIs for the reference destination/service. A tiny test application may speak SAM or use shipped public client APIs, but must not implement I2P tunnel/NetDB/Garlic/Streaming framing itself.

Prefer existing exact-pinned Java SAM sample behavior where suitable. No private Java router classes may be called to inject protocol state.

## 6. Cross-family semantic comparison

Record a sanitized comparison table for i2pd vs Java:

- SSU2 session establishment;
- short-build request/reply type and acceptance;
- tunnel lifetime/test behavior;
- NetDB lookup/store/search disposition;
- LeaseSet2 type/enc/signing algorithms;
- Streaming SYN/options/ports/close behavior;
- any reference-specific tolerated difference.

Differences must be handled through standards-conformant behavior, not implementation fingerprint branches unless the protocol explicitly permits alternatives and the branch is documented/tested.

## 7. Failure policy

If Java fails where i2pd passes:

- minimize the first failing protocol boundary;
- compare against current official specification and exact-pinned Java behavior;
- create a narrow corrective for that boundary;
- keep Plan 188 i2pd evidence retained-passed;
- do not weaken the two-family requirement.

## 8. M6 acceptance ledger

Create a fail-closed M6 mixed-router evidence checker/ledger if one does not already exist, e.g.:

```text
tests/integration/m6-interop/evidence.json
scripts/check-m6-mixed-router-acceptance-evidence.sh
.github/workflows/m6-mixed-router-external.yml
```

Mandatory evidence rows must be command-derived and bind to:

- exact i2pr head;
- exact i2pd pin;
- exact Java pin;
- clean reference checkouts;
- controlled/private topology;
- real one-hop tunnel proof;
- NetDB proof;
- destination ECIES/Garlic proof;
- Streaming both directions for each family;
- clean resource baseline.

Routine CI should statically validate the checker/ledger structure; the expensive external workflow may remain manual but must fail closed.

## 9. Acceptance criteria

Plan 189 passes only when:

1. exact-pinned unmodified Java I2P completes the controlled mixed-router path without public-network dependency;
2. real one-hop inbound/outbound i2pr tunnels through Java establish and pass liveness testing;
3. RouterInfo NetDB lookup/publication succeeds through those tunnels;
4. Standard LeaseSet2 resolution/publication and ECIES/Garlic destination delivery succeed;
5. Streaming establishes in both directions and small/large digests match;
6. Java and i2pd evidence both bind to exact clean pins and retained passing implementation heads;
7. no implementation-specific bypass or private state injection is used;
8. a fail-closed M6 mixed-router ledger/checker records the two-family evidence;
9. full workspace/static floor and exact-head routine CI pass;
10. exact-head external M6 workflow passes, preferably twice for the final implementation revision;
11. `specs/CONFORMANCE.md`, support docs, plans authority, and relevant architecture docs are updated to the **bounded** claim actually proven;
12. `plans/189-status.md` may set `milestone6_interoperable = passed-via-plan189` and advances `next_executable_plan = 190`.

## 10. Claim boundary

Closing M6 here means the project has demonstrated the retained mixed-router **destination/Streaming** MVP path against two independent router families in the controlled topology. It does not imply:

- public-network production readiness;
- broad peer diversity;
- transit participation by i2pr;
- floodfill role by i2pr;
- arbitrary NAT/IPv6 reachability;
- every Streaming option/crypto suite.

## 11. Stop conditions

Do not close M6 if either reference family relies on a patched router, self-composed substitution, direct destination-over-transport path, fake tunnel material, or missing command-derived evidence. If Java environmental orchestration is impossible, record the precise blocker and register a corrective/replacement plan; do not mark the second-family row passed from static source inspection.

## 12. Handoff

After M6 closure, execute Plan 190 to resume Plan 181's two remote M10 application rows. Plan 189 itself does not close M10.
