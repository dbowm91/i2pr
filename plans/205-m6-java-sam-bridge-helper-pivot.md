# Plan 205 — Java I2P SAM-bridge helper pivot for Plan 201 final closure

Status: **registered-blocked-on-plan198-publication-boundary**.

Plan 205 is the documented next executable plan after Plan 201. Plan
201 attempted Branches A through F against the exact-pinned Java I2P
2.13.0 cache and proved bounded by Java-side ProfileOrganizer fast-peer
scoring under controlled loopback topology plus the SAM-bridge LS2
publication gap. Plan 205 picks the smallest standards-compatible
pivot that does not break Plan 198 §4 / Plan 201 §4 constraints (no
public I2P, no Java patching, no i2pr production wire change) — the
Java SAM bridge helper path.

## 1. Background

Plan 194 retained the bounded SAM-bridge LS2-publication finding:
Java I2P's SAM bridge does NOT auto-publish the SAM-destination
LeaseSet2 to the local NetDB in a controlled private topology where
the reference has no peer tunnels to build a client tunnel for the
lease. i2pd's SAM bridge does publish immediately. Plan 200 closed the
diagnostic side. Plan 201 Branch C/D attempted the zero-hop and 1-hop
helper profile changes plus the three-router Router C topology and
both proved blocked at Java's ProfileOrganizer
`_thresholdSpeedValue` (line 161) + `_fastPeers` promotion (line 913)
which does not promote loopback peers into the fast tier within the
5-minute `I2PSession.connect()` timeout (`I2PSessionImpl.java:805`).

The remaining work after Plan 201 is to pivot the Java second-family
helpers from direct I2CP to the SAM bridge path that i2pd uses
successfully in Plan 202/203. Plan 205 owns that pivot.

## 2. Goal

On one exact repository head:

1. Rewrite `tests/integration/m6-interop/java/ReferenceRawDestination.java`
   and `tests/integration/m6-interop/java/ReferenceStreamingService.java`
   to use the public Java SAM 3.1 bridge instead of direct I2CP, so
   the destination's LeaseSet2 is published through Java's own SAM
   bridge lifecycle (matching what i2pd's SAM bridge does for the
   Plan 202/203 reference destinations);
2. re-run the controlled Java topology + authenticated SSU2
   preflight against the exact-pinned Java I2P 2.13.0 cache
   (`9134f808337b401e8e53c73734c81fab04280c9d`);
3. record a fresh terminal `P200-{A..H}` classification against the
   new SAM-bridge helpers;
4. flip the seven Plan 201 §11 stop rows `blocked → passed` if the
   terminal classification's corresponding branch lands a corrective;
5. close the Java second-family qualification if the corrected lane
   produces all-pass mandatory rows.

## 3. Topology

```text
Router A (Java I2P 2.13.0) — service/public-client router (SAM bridge + I2CP listener)
Router B (Java I2P 2.13.0) — floodfill/publication target
Router C (Java I2P 2.13.0) — tunnel participant (no client apps, only SSU2 endpoint)
i2pr daemon — controlled loopback profile, no public network
```

Three disposable Java routers, each in a fresh data directory under
`/tmp/i2pr-m6-plan205.*/`. Router A exposes the SAM bridge on an
ephemeral loopback port and the I2CP listener on a second ephemeral
port. Router B is configured as `notransit = false, floodfill = true`
just like the Plan 187 destination lane. Router C binds an SSU2
endpoint only (no SAM bridge, no I2CP listener — the Plan 201 Branch
C/D three-router topology retained).

## 4. Corrective matrix (after Plan 201)

Plan 205 attempts the SAM-bridge pivot and lets the `P200-*`
classification choose the corrective:

| P200 code | corrective |
| --- | --- |
| `P200-A-router-a-missing-router-b` | SAM-bridge RouterInfo publication order fix |
| `P200-B-router-b-missing-router-a` | SAM-bridge inbound reply-path fix |
| `P200-C-client-ls2-not-created-or-current` | SAM-bridge SAM SESSION CREATE profile fix |
| `P200-D-client-tunnel-publication-path-unavailable` | SAM-bridge named lease publication |
| `P200-E-no-eligible-floodfill-candidate` | SAM-bridge floodfill pinning fix |
| `P200-F-store-sent-no-ack` | SAM-bridge DeliveryStatus routing fix |
| `P200-G-store-acked-remote-lookup-fails` | re-use Plan 201 Branch G framework |
| `P200-H-publication-path-passed` | no corrective; flip rows from blocked → passed |

## 5. Common rules

Across all corrective branches:

- exact-pinned stock Java source/build (`9134f808337b401e8e53c73734c81fab04280c9d`);
- disposable loopback RouterContexts;
- no public network;
- no VMComm;
- no private NetDB/tunnel state injection;
- no direct LS2 copying;
- no `LocalZeroHop` substitution on the i2pr side;
- public SAM bridge API owns the counted remote Destination;
- Java SAM remains the only controlled i2pd-side entry point.

## 6. Required validation

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc

bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh
bash tests/integration/m6-interop/run-java.sh
bash tests/integration/m6-interop/run-m6-mixed-router.sh

cargo deny check advisories bans sources
```

## 7. Acceptance criteria

Plan 205 passes only when:

1. SAM-bridge helpers (`ReferenceRawDestination.java` +
   `ReferenceStreamingService.java`) are rewritten to use the public
   SAM bridge API exclusively (no direct I2CP connection);
2. exact Java pin remains clean/unmodified;
3. public Java SAM session owns the counted remote Destination
   through public APIs;
4. Router A/B bootstrap post-lookup proofs pass;
5. current Standard LS2 creation is proven;
6. client publication-tunnel eligibility is proven;
7. floodfill target selection is proven;
8. LS2 DatabaseStore emission is proven;
9. publication acknowledgement is proven;
10. Router B ordinary lookup serves the LS2;
11. i2pr validates/installs the remote LS2;
12. real outbound and inbound i2pr tunnel rows pass;
13. bidirectional raw destination delivery passes;
14. Direction A Streaming passes;
15. Direction B Streaming passes;
16. small + multi-packet + reverse payload digests match;
17. sibling isolation passes;
18. close/cleanup/resource baselines pass;
19. no mandatory Java row is blocked, failed, or missing;
20. retained i2pd first-family rows remain all-pass;
21. cross-family M6 checker reports zero mandatory blocked/failed/missing rows;
22. no forbidden private-state/public-network shortcut was introduced;
23. authority is not promoted until exact-head evidence exists.

## 8. Authority transition on success

```text
plan_205 = passed-m6-java-sam-bridge-helper-pivot
plan_201 = passed-m6-java-public-client-publication-corrective-and-second-family-closure
plan_204 = passed-m10-final-closure-evidence-authority-and-documentation-normalization
milestone6_java_mixed_router_interop = passed-via-plan201-and-plan205
milestone6_interoperable = passed-via-plan193-and-plan201
milestone10_final_acceptance = closed-via-plan204
```

Plan 205 may update Plan 201 / Plan 204 status and immediately relevant
M6 status files but should avoid broad M10 documentation churn; Plan
204 owns the cross-document normalization pass.

## 9. Handoff rule

Plan 205 must not claim final closure until:

1. Plan 200 has one unambiguous terminal `P200-*` classification
   (re-recorded against the SAM-bridge helpers);
2. only the justified corrective branch was implemented initially;
3. all 23 acceptance criteria in §7 are satisfied;
4. the M6 cross-family checker
   (`scripts/check-m6-mixed-router-acceptance-evidence.sh`) is green
   on the closing exact head;
5. the M6 final closure ledger
   (`scripts/check-m6-final-closure-evidence.sh`) is green on the
   same exact head;
6. the manual M6 mixed-router workflow
   (`.github/workflows/m6-mixed-router-external.yml`) succeeds and
   the produced evidence.json reports
   `m6_mixed_router = passed-via-i2pd-2.61.0-and-java-2.13.0`;
7. Plan 204's §7/§12 authority transitions are recorded in the same
   commit.

## 10. Status narrative

Plan 205 inherits Plan 201's Branch A decode fix (commit `2dc926f`),
Branch G observation framework (commit `9bce8a7`), Branch C/D
three-router topology scaffolding (commit `d0fe596`), and the M10
remote-row wiring (commit `06769fa`). It does not require any
Plan 198 / Plan 201 §4 constraint relaxation.
