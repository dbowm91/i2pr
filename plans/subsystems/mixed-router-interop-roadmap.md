# M6 Mixed-Router Interop Roadmap

Status: active

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/02-i2np.md` (authenticated router-I2NP dispatch)
- `specs/SOURCES.md` + `specs/IMPLEMENTATIONS.md` (exact-pinned references: i2pd 2.61.0, Java I2P 2.13.0)

Related ADRs:

- `docs/adr/0021-minimal-java-support-topology.md`; see index for others.

## 1. Purpose and ownership boundary

Authenticated I2NP preflight, one-hop exploratory tunnels, NetDB lookup/publication, destination/garlic routing, short-build reply + NetDB reply-path + wire-format correctives, i2pd Streaming qualification (33/33), Java second-family qualification (controlled topology, pq tolerance, public-client observability, Branch G corrective framework, harness/evidence corrective).

Historic/registered plans: 183–194, 196–198, 200, 201, 205, 217–245 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No `milestone6_interoperable` claim; Java second-family stays `not-yet-passed`. No public I2P, no reference patching, no production wire change to go green.

## 4. Current state

Plan 217 closed the Java harness corrective. Plan 218 remains the trustworthy behavioral stop: the destination lane passes the established forward/publication evidence, Java admits a reverse raw-Destination send, and no matching reverse payload reaches i2pr inside the bounded window.

Plan 219's J219-B attribution is superseded; Plan 220 refuted it. Plan 222 reproduced the exact client-NetDB selector and nonce-correlated send path and narrowed the reverse failure to status 17. Plan 223 corrected the generated Destination identity / LS2 separation: new router-owned Destinations are legacy type-0/256 identity shape while Standard LS2 remains X25519/type-4/32. Status 17 disappeared and the same tracked send now emits:

```text
P223-NEXT-BOUNDARY ordered_statuses=[1, 21]
```

The open boundary is therefore OCMOSJ `NO_LEASESET (21)` after ACCEPTED.

Exact-pinned Java 2.13.0 research shows status 21 is only the lookup-failure result, not a root cause. The current harness publishes the i2pr target LS2 specifically toward Router B but does not prove that Router B's main NetDB actually holds a current `receivedAsPublished` copy that Java would answer from. If B is answerable, the remaining exact path is helper client lookup A→B, B's LS reply, A's inbound client-tunnel DSM receipt, and installation into the helper client sub-DB. Pinned `InNetMessagePool` stores a matching DSM inline before running the lookup-success job, ruling out a store-vs-success scheduling race.

Plan 224 is closed as `P224-OBSERVABILITY-GAP-LOOKUP-PATH`: Router B's target
LS2 is current and query-answerable, Router A's helper client DB remains empty,
and the exact lookup trace was not observable. Plan 225 is now closed as a
narrow diagnostic corrective with the exact terminal
`P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B`: the helper search
started and exhausted without dispatching the target lookup to Router B. No
bootstrap/floodfill/SAM/tunnel/topology corrective is authorized, and M6 Java
remains not-yet-passed.

Exact-pinned source review after Plan 225 identified a narrow controlled-topology hypothesis: `IterativeSearchJob` uses `IP_CLOSE_BYTES=3`, while A/B/C currently advertise SSU2 on the same `127.0.0.0/24`. Plan 226 closed with an exact target-job `P226-BASELINE-B-ZERO-HOP-UNKNOWN` terminal: `b_ip_close_skipped=false`, so the distinct loopback /24 correction was not admitted. `netDb.alwaysQuery` remains outside acceptance authority.

Pinned/current Java I2P also exposes `explicitPeers` as a client tunnel debugging option carried through ordinary I2CP SessionConfig. Unlike the old Plan-201 one-hop attempt, it can select a valid Router C without waiting for fresh-router fast/high-capacity tier promotion while still constructing a genuine stock-Java tunnel. Plan 227 is closed: it proved C selectable in A's main NetDB on all 3 counted attempts but stock Java built no one-hop client tunnels within the five-minute `I2PSession.connect()` ceiling (`P227-EXPLICIT-ONE-HOP-NOT-BUILT`), so no lookup rerun was interpretable. Plan 228 is closed as attribution-only with `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both`: client configs through C are created in both directions, but neither direction obtains the paired tunnel Java requires (only zero-hop exploratory tunnels available on all 4 counted attempts), so no build request is ever dispatched. Profile mutation, exploratory/client policy changes, VMComm, `netDb.alwaysQuery`, direct tunnel install, and client-NetDB RI injection remain forbidden.

Plan 229 is closed at `P229-C-NOT-EXPLORATORY-ELIGIBLE`: the bounded
corrective applied (C restored to non-floodfill transit, A-only stock
one-hop small-router exploratory profile proven live) but every counted
run stops at the transit-peer gate — the ordinary authenticated
DatabaseStore bootstrap populates NetDB entries without populating
Router A's profile-organizer tier population (`profile_count=0`), which
pinned `ExploratoryPeerSelector` selects from. Bytecode review shows
`isSelectable` is RI-fact based and tier-map independent, so the
retained selectable-C signal never implied tier membership.

Plan 230 is closed at `passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary`.
Exact-pinned source review had shown the P229 `unreachable` signal was
non-authoritative (`ProfileOrganizer.isFailing()` is deprecated and always
false) and that ordinary `heardAbout()` profile creation is capability-gated
by `R` plus bandwidth/congestion caps. Plan 230 proved that exact predicate
first (baseline `P230-A-PREDICATE-INELIGIBLE reason=compound`: missing-`R`
plus low-bandwidth-`L` on the floodfill observer), applied only the matching
stock corrections (`i2np.udp.status=ok` fixture-wide plus transit-C-only
`128/128` bandwidth, never forced-class), and required natural profile
creation through the existing authenticated RI bootstrap: 3/3 counted runs
eligible (`R` + tier `N` + comm `OK`), 2/3 bootstrapped naturally with
RI-identity match, the deepest run passing the retained P229/P228/P201 gates
through digest-matched forward delivery and stopping at the reverse
Java→i2pr payload, which reproduces the retained Plan-218 signature (send
admitted, no payload in 45 s). No new i2pr-visible wire boundary was found.

Plan 231 is closed at `passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary`.
Exact-pinned source ordering proves `STATUS_SEND_ACCEPTED` is emitted only after the inline OCMOSJ `DispatchJob` has called `TunnelDispatcher.dispatchOutbound(...)` and returned. Plan 231 correlated the single tracked reverse message through Java A's outbound gateway (enqueue proven via `client.dispatchTime` +1 with a window gateway accept under lane-quiet premises), Router C's exact one-hop outbound endpoint (receive == A send id, processed 1→5), the selected target LeaseSet inbound gateway (absent on Router B for the exact lease tunnel, twice observed), and i2pr's exact TunnelData/recovery/Garlic/Destination stages (honestly zero wire). The stop root-causes to the test-driver lease fixture (advertised gateway B vs actual inbound gateway A); a narrow lease-gateway fixture corrective is recommended with no production change. Plan-230 topology, profile policy, tunnel settings, and timing windows stayed frozen.

Plan 232 is closed at Outcome B
(`passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary`,
on `236ccb6`): all three Java-driver local leases derive gateway+tunnel
from the installed inbound route, Router B stays the publication target,
raw-Destination reverse delivery flips to digest-matched
`P232-D-REVERSE-DELIVERY-PASSED` (role B → A, exact IBGW present), and the
retained Streaming continuation proves initial route parity live with the
Direction-A SYN sent on two consecutive counted runs, both stopping
identically at `SYN-ACK never established
(syn_accepted=false established=false pump_error=0)`.

Plan 233 is superseded-before-execution. Its narrow Streaming SYN-ACK attribution remains valid, but its family-pass outcome did not reconcile the still-required Plan-200/201 client-LS2 lifecycle rows in the fail-closed Java harness.

Plan 234 is closed at Outcome B with `P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`. Plan 235 is also closed at Outcome B with `P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND`: Java's public socket surface was ready and i2pr admitted the outbound SYN, but no inbound TunnelData appeared. Plan 236 closed at `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` after source-locking the exact stock-Java response path; its helper response fields remained literal Unknown placeholders rather than measurements. Plan 237 closed at `P237-D-ROUTER-I2CP-NOT-OBSERVED` on `a1065d1`: three identical counted executions proved scheduler action (delta 2), response-packet construction via source-locked `Connection.sendPacket` (delta 1), and `I2PSession.sendMessage` return (lifetime-event delta 3, zero failures) from real stock helper-JVM observations, then stopped at the first Router-A stage, for which no probe exists yet. A counted pre-budget attempt on `7506d06` exposed and corrected two observer defects (scheduler logs via `SchedulerImpl`; `ackImmediately` never fires on fresh SYN-ACK) before the budget restarted. The retained Plan-232 raw-Destination reverse pass remains authoritative; Java-family M6 remains not-yet-passed. Plan 238 closed at the retained `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` token on `08df6ea`, but its executed evidence proves Router-A I2CP admission (`client.distributeTime` delta exactly 3, matching helper sendMessage delta 3) and zero dispatch deltas on three counted runs. The token is therefore historical classifier nomenclature for a post-admission/pre-dispatch boundary, not evidence that admission failed. Plan 239 is closed at `P239-D-TARGET-LEASESET-LOOKUP-FAILED` on `d82cf06`: three identical counted executions prove local target-LS absent pre/post, remote lookup found 0 with failed 3/4/4, and dispatch deltas 0 with rates known, stopping at the first post-admission D stage before any tunnel/dispatch attribution. Plan 240 is closed at `P240-C-B-ZERO-HOP-UNKNOWN-RI` on `37025f9`: three identical counted executions correlate the exact streaming target ISJ (2 jobs/attempt, both with Router B in `toTry`), prove B lookup-candidate eligible (RI present, floodfill-indexed, not banlisted, fresh, profile present, comm established), and prove the first B-specific boundary inside pinned `sendQuery()` — the selected outbound tunnel was zero-hop while A's main NetDB held no validated RI for B, so no target DLM was ever dispatched to B and the search exhausted. IP-close is false on all runs, so no topology correction is admitted (as in Plan 226). Plan 241 is closed at the exact A/B build-stage boundary on `33ebdc4`: the Streaming helper's forced zero-hop profile is replaced by the proven one-hop-via-C SessionConfig (proven active live), the Plan-230 bootstrap gate and installed-pair gate stop each run at the first missing stage (attempt 1: 1+1 non-zero-hop tunnels installed but not exact-via-C; attempts 2–3: profile never bootstraps within retained bounds), and the old zero-hop-unknown terminal is structurally unreachable. Live lookup continuation (§§8–11) awaited the successor.

Plan 242 subsequently closed the stock one-hop selector-semantics corrective: the C-specific profile gate is replaced by a stock-candidate-population gate, the exact-via-C pair gate is replaced by a genuine non-zero-hop pair gate, and bounded path-role facts remain diagnostic. Its routine/focused/full workspace floors passed, but its closure host lacked the Java reference cache plus i2pr daemon pair, so no live counted external attempts executed there. Plan 243 closed the hosted stock-client-build qualification on `f359baba57fc7d952ab7f5a5367d34671c590b18`: three counted same-SHA executions of the frozen Plan-242 Streaming lane; Direction A established on 2/3 attempts (`inbound_tunneldata=1`, `expected_tunneldata=1`, `recovery=1`, `garlic_payload=1`, `adapter_successes=1`, `connection_established=true`); one earlier stock-Java `I2PSession.connect()` handshake ceiling stop on attempt 2; reverse direction bounded at the retained Plan-236 response-emission gap. No production i2pr corrective; successor required for the streaming reverse direction + publication / final-closure axis.

Plan 244 closed the continuous response attribution on `154e92d8436dbad0b020a0b19d848d2d227a22af`: three counted same-SHA executions of the frozen Plan-242 Streaming lane established Direction A 3/3 with fully bound response epochs (helper DBID + target + epoch id, provenance matched), and every epoch stopped identically at the first missing live stage `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` (stock-Java scheduler acts with `scheduler_delta=1`, `Connection.sendPacket` construction absent with `ack_constructed_delta=0`, failures zero; the historical P236 token printed but never governed). No production i2pr corrective; the §20 `helper response not emitted` arm owns the narrow stock-response successor (own plan-of-record, not pre-registered).

Plan 245 closed the stock-response construction-signal attribution corrective on `c866967f114ce15012d3256e38eebdd1f7e9f6f5` (counted attempts on `685a59f` after the Plan-237-stats extract): three counted same-SHA executions of the frozen Plan-242 Streaming lane established Direction A 3/3 with fully bound response epochs, and the Stage A.0 classifier proved the scheduler reschedule-only branch on every attempt with identical deltas (`scheduler_delta=1`, `scheduler_send_branch_delta=0`, `scheduler_reschedule_branch_delta=1`, `receiver_packet_built_delta=0`, `connection_resend_timer_delta=0`, failures zero). The Plan-244 `Resend in` retransmit-timer proxy was **not** a false negative on this lane — construction genuinely did not occur because the scheduler rescheduled before the response window opened (`SchedulerReceived.eventOccurred` took the `timeTillSend > 0` branch and called `reschedule(timeTillSend, con)`, never the send-branch `timeTillSend <= 0 && getNextSendTime() > 0` arm that would invoke `con.sendAvailable()` and downstream `flushAvailable`/`writeData`/`buildPacket`/`sendPacket`). No production i2pr corrective; the Plan-245 §17 narrow timer/state attribution successor owns the bounded investigation into why `con.getNextSendTime() - _context.clock().now() > 0` holds for the frozen 45-second response window on every counted attempt.

Plan 205 remains retained/deferred.

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
183 -> 184 -> 185 -> 186 -> 187 -> 188 -> 190 -> 191 -> 192 -> 193 -> 196 -> 197 -> 194 -> 198 -> 200 -> 201 -> 217 -> 218 -> 219 -> 220 -> 221 -> 222 -> 223 -> 224 -> 225 -> 226 -> 227 -> 228 -> 229 -> 230 -> 231 -> 232 -> 233 -> 234 -> 235 -> 236 -> 237 -> 238 -> 239 -> 240 -> 241 -> 242 -> 243 -> 244 -> 245. Plan 205 is retained conditional fallback and is not part of the primary sequence.
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 183 | ready | registered-m6-mixed-router-streaming-interop-program | `plans/implementation/mixed-router-interop/183-m6-mixed-router-streaming-interop-program.md` | `plans/closure/mixed-router-interop/183-status.md` |
| 184 | closed | passed-m6-authenticated-i2np-runtime-and-reference-preflight | `plans/implementation/mixed-router-interop/184-m6-authenticated-i2np-runtime-and-reference-preflight.md` | `plans/closure/mixed-router-interop/184-status.md` |
| 185 | closed | passed-m6-live-one-hop-exploratory-tunnels-and-liveness | `plans/implementation/mixed-router-interop/185-m6-live-one-hop-exploratory-tunnels-and-liveness.md` | `plans/closure/mixed-router-interop/185-status.md` |
| 186 | closed | passed-m6-mixed-router-netdb-lookup-and-publication | `plans/implementation/mixed-router-interop/186-m6-mixed-router-netdb-lookup-and-publication.md` | `plans/closure/mixed-router-interop/186-status.md` |
| 187 | blocked | blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correct... | `plans/implementation/mixed-router-interop/187-m6-remote-leaseset2-and-destination-garlic-routing.md` | `plans/closure/mixed-router-interop/187-status.md` |
| 188 | blocked | blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-p... | `plans/implementation/mixed-router-interop/188-m6-mixed-router-streaming-with-i2pd.md`; `plans/implementation/mixed-router-interop/188-m6-short-build-reply-interop-corrective.md` | `plans/closure/mixed-router-interop/188-status.md` |
| 189 | blocked | registered-m6-java-second-family-qualification-and-closure (blocked-by-plan188-plan190-plan191-plan192) | `plans/implementation/mixed-router-interop/189-m6-java-i2p-second-family-qualification-and-closure.md` | `plans/closure/mixed-router-interop/189-status.md` |
| 190 | closed | passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed; remote lane no... | `plans/implementation/mixed-router-interop/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md` | `plans/closure/mixed-router-interop/190-status.md` |
| 191 | blocked | stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked; 2 ordering rows ... | `plans/implementation/mixed-router-interop/191-m6-inbound-destination-delivery-boundary.md` | `plans/closure/mixed-router-interop/191-status.md` |
| 192 | closed | passed-m6-i2cp-wire-format-corrective (i2pd-compatible I2CP-style Data body wire-format; inbound-delivery boundary E ... | `plans/implementation/mixed-router-interop/192-m6-i2cp-wire-format-corrective.md` | `plans/closure/mixed-router-interop/192-status.md` |
| 193 | closed | passed-m6-i2pd-mixed-router-streaming-qualification (local rows passed; 33/33 external rows twice on exact head 36871... | `plans/implementation/mixed-router-interop/193-m6-i2pd-mixed-router-streaming-qualification.md` | `plans/closure/mixed-router-interop/193-status.md`; `plans/closure/mixed-router-interop/193-streaming-status.md` |
| 194 | closed | passed-m6-java-second-family-mixed-router-closure-with-sam-ls2-gap (status-file token; re-scoped by Plans 198/200/201... | — | `plans/closure/mixed-router-interop/194-m6-java-second-family-mixed-router-closure.md`; `plans/closure/mixed-router-interop/194-status.md` |
| 196 | closed | passed-m6-java-controlled-first-run-topology-corrective (out-of-tree ControlledRouter.java test-only launcher + rewri... | `plans/implementation/mixed-router-interop/196-m6-java-controlled-first-run-topology-corrective.md` | `plans/closure/mixed-router-interop/196-status.md` |
| 197 | closed | passed-m6-pq-ssu2-option-support-corrective (parser-only tolerance of the SSU2 `pq` KEM-scheme option Java I2P 2.13.0... | `plans/implementation/mixed-router-interop/197-m6-pq-ssu2-option-support-corrective.md` | `plans/closure/mixed-router-interop/197-status.md` |
| 198 | superseded | superseded-execution-decomposed-and-closed-via-plans200-204. | — | `plans/closure/mixed-router-interop/198-m6-java-public-client-final-closure-corrective.md`; `plans/closure/mixed-router-interop/198-status.md` |
| 200 | closed | passed-m6-java-public-client-publication-observability-and-verified-bootstrap (Java helpers decoupled `leaseset=publi... | `plans/implementation/mixed-router-interop/200-m6-java-public-client-publication-observability-and-verified-bootstrap.md` | `plans/closure/mixed-router-interop/200-status.md` |
| 201 | blocked | blocked-after-plan242-corrected-gates-pending-plan243-and-publication-closure | — | `plans/closure/mixed-router-interop/201-m6-java-public-client-publication-corrective-and-second-family-closure.md`; `plans/closure/mixed-router-interop/201-status.md` |
| 205 | retained | retained-deferred-conditional-after-plan218-direct-i2cp-requalification (Plan 227 localized the active failure below client-tunnel establishment; Plan 228 closed attribution at paired-tunnel selection on the direct Java client build path below SAM) | `plans/implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md` | `plans/closure/mixed-router-interop/205-status.md` |
| 217 | closed | passed-m6-java-closure-harness-and-evidence-corrective (transfer-once invariant; positive/negative evidence split; relative Java NetDB dir; disjoint streaming build/tunnel/message-id namespace; `I2PR_M6_JAVA_DRIVER` selector; static-checker invariants) | `plans/implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md` | `plans/closure/mixed-router-interop/217-status.md` |
| 218 | stopped | stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary (behavioral stop retained; Plan 227 proved selectable C but no one-hop client tunnel; Plan 228 closed attribution at NO-PAIRED-TUNNEL) | `plans/implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md` | `plans/closure/mixed-router-interop/218-status.md` |
| 219 | superseded | retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220 | `plans/implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md` | `plans/closure/mixed-router-interop/219-status.md` |
| 220 | closed | passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required | `plans/implementation/mixed-router-interop/220-m6-java-plan219-diagnostic-attribution-corrective.md` | `plans/closure/mixed-router-interop/220-status.md` |
| 221 | superseded | superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/221-m6-java-client-netdb-ocmosj-narrowing.md` | `plans/closure/mixed-router-interop/221-status.md` |
| 222 | closed | passed-m6-java-client-netdb-ocmosj-narrowing-corrective | `plans/implementation/mixed-router-interop/222-m6-java-client-netdb-ocmosj-narrowing-corrective.md` | `plans/closure/mixed-router-interop/222-status.md` |
| 223 | closed | passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset (type-0/256 + LS2 type-4/32; status 17 gone → P223-NEXT-BOUNDARY [1,21] on 0755dc1) | `plans/implementation/mixed-router-interop/223-m6-java-destination-identity-crypto-separation-corrective.md` | `plans/closure/mixed-router-interop/223-status.md` |
| 224 | closed | passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap (Router B answerable; helper client DB empty; exact lookup trace unavailable on 895132c) | `plans/implementation/mixed-router-interop/224-m6-java-no-leaseset-lookup-path-attribution.md` | `plans/closure/mixed-router-interop/224-status.md` |
| 225 | closed | passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution (exact terminal `P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B`) | `plans/implementation/mixed-router-interop/225-m6-java-no-leaseset-lookup-path-observability-corrective.md` | `plans/closure/mixed-router-interop/225-status.md`; `plans/closure/mixed-router-interop/225-corrective-closure.md` |
| 226 | closed | passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary | `plans/implementation/mixed-router-interop/226-m6-java-loopback-peer-diversity-corrective.md` | `plans/closure/mixed-router-interop/226-status.md` |
| 227 | closed | passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary | `plans/implementation/mixed-router-interop/227-m6-java-explicit-one-hop-client-tunnel-corrective.md` | `plans/closure/mixed-router-interop/227-status.md` |
| 228 | closed | passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary | `plans/implementation/mixed-router-interop/228-m6-java-client-tunnel-build-path-attribution.md` | `plans/closure/mixed-router-interop/228-status.md` |
| 229 | closed | passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary | `plans/implementation/mixed-router-interop/229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective.md` | `plans/closure/mixed-router-interop/229-status.md` |
| 230 | closed | passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary | `plans/implementation/mixed-router-interop/230-m6-java-profile-population-path-attribution.md` | `plans/closure/mixed-router-interop/230-status.md` |
| 231 | closed | passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary | `plans/implementation/mixed-router-interop/231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective.md` | `plans/closure/mixed-router-interop/231-status.md` |
| 232 | closed | passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary | `plans/implementation/mixed-router-interop/232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure.md` | `plans/closure/mixed-router-interop/232-status.md` |
| 233 | superseded | superseded-before-execution-by-plan234-final-closure-authority-corrective | `plans/implementation/mixed-router-interop/233-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix.md` | `plans/closure/mixed-router-interop/233-status.md` |
| 234 | closed | passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary | `plans/implementation/mixed-router-interop/234-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective.md` | `plans/closure/mixed-router-interop/234-status.md` |
| 235 | closed | passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound | `plans/implementation/mixed-router-interop/235-m6-java-streaming-post-accept-response-boundary-corrective.md` | `plans/closure/mixed-router-interop/235-status.md` |
| 236 | closed | passed-m6-java-streaming-response-emission-observability-gap | `plans/implementation/mixed-router-interop/236-m6-java-streaming-response-emission-and-i2cp-send-attribution.md` | `plans/closure/mixed-router-interop/236-status.md` |
| 237 | closed | passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary | `plans/implementation/mixed-router-interop/237-m6-java-streaming-stock-response-observability-corrective.md` | `plans/closure/mixed-router-interop/237-status.md` |
| 238 | closed | passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary | `plans/implementation/mixed-router-interop/238-m6-java-streaming-router-a-admission-observer.md` | `plans/closure/mixed-router-interop/238-status.md` |
| 239 | closed | passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary | `plans/implementation/mixed-router-interop/239-m6-java-streaming-router-a-dispatch-observer.md` | `plans/closure/mixed-router-interop/239-status.md` |
| 240 | closed | passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary | `plans/implementation/mixed-router-interop/240-m6-java-streaming-target-leaseset-lookup-failure-attribution.md` | `plans/closure/mixed-router-interop/240-status.md` |
| 241 | closed | passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary | `plans/implementation/mixed-router-interop/241-m6-java-streaming-one-hop-client-tunnel-fixture-corrective.md` | `plans/closure/mixed-router-interop/241-status.md` |
| 242 | closed | passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate | `plans/implementation/mixed-router-interop/242-m6-java-streaming-stock-one-hop-selector-semantics-corrective.md` | `plans/closure/mixed-router-interop/242-status.md` |
| 243 | closed | passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established | `plans/implementation/mixed-router-interop/243-m6-java-streaming-hosted-stock-client-build-qualification.md` | `plans/closure/mixed-router-interop/243-status.md` |
| 244 | closed | passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary | `plans/implementation/mixed-router-interop/244-m6-java-streaming-reverse-direction-continuous-response-attribution.md` | `plans/closure/mixed-router-interop/244-status.md` |
| 245 | closed | passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary | `plans/implementation/mixed-router-interop/245-m6-java-streaming-stock-response-construction-signal-attribution-corrective.md` | `plans/closure/mixed-router-interop/245-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-m6-mixed-router-acceptance-evidence.sh` (fail-closed; environment-gated).

Plan 224 retained every Plan-222/223 invariant and added lookup-path attribution:

- read-only Router-B main-NetDB and Router-A helper-client-subDB target-LS snapshots;
- explicit `receivedAsPublished` / current-state proof before calling Router B query-answerable;
- scratch-only class-specific Java lookup logging with default ERROR and whitelist-only sanitized booleans/counts;
- exact distinction between selector membership, actual A→B query, B lookup receipt, B published-LS answer, A inbound-client-tunnel DSM receipt, and A client-subDB post-send presence;
- the pinned Java DSM store-before-success ordering is treated as source authority and the speculative race explanation is forbidden.

Plan 225 is closed as a diagnostic corrective. Plan 226 is closed as the bounded harness corrective: its exact target job proved a non-IP zero-hop rejection, so no pairwise-distinct loopback /24 topology was admitted. Plan 227 is closed at selectable-C-but-not-built. Plan 228 is closed at the paired-tunnel attribution: configs through C are created, no paired tunnel is available in either direction, and no dispatch ever occurs. Plan 229 closed at the transit-peer stop: roles and the A-only small-router profile proven live, tier population empty (`profile_count=0`). Plan 230 closed the reachability-capability/profile-bootstrap corrective (predicate proven, stock C1+C2 correction, natural bootstrap 2/3, forward delivery digest-matched once) at the retained reverse-delivery boundary. Plan 231 closed the exact post-`ACCEPTED` attribution (A enqueue and C OBEP processing proven, exact target IBGW on B twice-absent, i2pr wire zero) with a lease-fixture root cause. Plan 232 closed the route-derived lease-gateway fixture corrective at Outcome B: raw-Destination reverse delivery digest-matched (`P232-D-REVERSE-DELIVERY-PASSED`) while the Streaming continuation proves initial route parity live and stops identically at SYN-ACK-never-established on two consecutive runs. Plan 233 is superseded-before-execution. Plan 234 closed at `P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED` after three same-SHA attempts; Plan 235 closed at `P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND` after three same-SHA attempts. Plan 236 closed at `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` after two same-SHA attempts with source lock passed and response-stage observation incomplete. Plan 237 closed at `P237-D-ROUTER-I2CP-NOT-OBSERVED` on `a1065d1` x3 (deltas 2/1/3). Plan 238 closed at `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` on `08df6ea` x3 (distribute delta 3, dispatch deltas 0, rates known). Plan 239 closed at `P239-D-TARGET-LEASESET-LOOKUP-FAILED` on `d82cf06` x3 (local absent, found 0, failed 3/4/4, dispatch 0/0, rates known). Plan 240 closed at `P240-C-B-ZERO-HOP-UNKNOWN-RI` on `37025f9` x3 (exact streaming ISJ correlated, B eligible and in `toTry`, zero-hop-unknown skip proven, never queried; source lock byte-identical, 32 rows). Plan 241 closed at the exact A/B build-stage boundary on `33ebdc4` with no tuning: the forced zero-hop Streaming fixture is removed, attempt 1 installed 1+1 non-zero-hop tunnels with zero-hop absent but not exact-via-C, and attempts 2–3 stopped at the C-specific profile gate. Exact-pinned source review after closure proves `shouldSelectExplicit()` honors `explicitPeers` only when `random.nextInt(4) == 0`, so exact-via-C was a harness over-constraint. Plan 242 closed the corrected bootstrap and pair gates on the implementation head with no tuning: the §5 stock-candidate-population gate replaces the Plan-241 §6 C-profile gate, the §7 non-zero-hop pair gate replaces the Plan-241 §7 exact-via-C gate, the §6 extended `P242-CLIENT-TUNNELS` row records role/path facts and `contains_c` as diagnostic only, source lock grew 35→38 with eleven new TunnelPeerSelector / ClientPeerSelector needles, and the routine + focused + full serial workspace floors all pass. Live counted external attempts are unexecuted on this host (no Java cache + i2pr daemon pair available); the §20 stock-client-build successor is the next executable lane. Plan 201 remains blocked (streaming axis pending the §20 stock-client-build successor, publication axis unchanged); Plan 204 remains blocked; Plan 205 remains retained-deferred.

Raw Java logs never become evidence. The Plan-223 45-second reverse-payload acceptance window stays frozen. External attempts remain limited to three per committed implementation SHA with no tuning.

## 10. Risks and decision points

- `NO_LEASESET (21)` is not evidence that publication failed; it is the bounded OCMOSJ lookup-failure result.
- Router B's exact main-NetDB LS state is the first missing discriminator because i2pr explicitly publishes the target LS2 toward B.
- A LeaseSet in B's main DB is query-answerable only when current/valid and `receivedAsPublished` according to pinned Java.
- Selector membership does not prove an actual query was sent to B.
- B answering does not prove the encrypted DSM reached A's helper client tunnel.
- A client-tunnel DSM receipt should route to the helper client sub-DB, and pinned Java stores matching DSMs inline before lookup-success callback; do not resurrect a store-race hypothesis.
- Targeted DEBUG/INFO raw logs may contain sensitive reply key/tag material. They remain scratch-only; evidence is whitelist-sanitized typed facts only.
- Plan 226 may correct only the controlled Java SSU2 loopback topology after exact target-job IP-close attribution; it may not change production i2pr protocol behavior.
- Plan 205 SAM work remains off-path.

## 11. Completion definition

Closed: Plan 224 proved an answerable Router-B main-NetDB LS2 and persistent
empty helper client DB, but its exact lookup trace was unobservable; it emitted
`P224-OBSERVABILITY-GAP-LOOKUP-PATH`. Plan 225 made the trace observable and
identified the earliest missing stage as the absence of an actual target lookup
dispatch from Router A to Router B, emitting exactly
`P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B`.

Plan 226 closed without authorizing the controlled-topology correction. Plan 227 closed with `P227-EXPLICIT-ONE-HOP-NOT-BUILT` (selectable C, no tunnels built). Plan 228 closed with `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both` (configs through C created, paired tunnel unavailable both directions, no dispatch). Plan 229 closed with `P229-C-NOT-EXPLORATORY-ELIGIBLE` (roles + A-only small-router profile proven live, organizer tier population empty, helper never started). Plan 230 closed with the reverse-delivery boundary after proving predicate/correction/bootstrap/forward-delivery. Plan 231 closed with `P231-C-TARGET-IBGW-NOT-INSTALLED` (A enqueue and C OBEP processing proven, exact target IBGW on B twice-absent, i2pr wire zero; lease-fixture root cause). Plan 232 closed at Outcome B (`P232-D-REVERSE-DELIVERY-PASSED` raw reverse, `P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY` at SYN-ACK-never-established reproduced twice); Plan 233 is superseded-before-execution; Plan 234 closed with `P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`; Plan 235 closed with `P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND`; Plan 236 closed with `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`; Plan 237 closed with `P237-D-ROUTER-I2CP-NOT-OBSERVED` (sendMessage-return proven, Router-A unattributed; Plan 238 closed the observer); Plan 238 closed with `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` (admission proven with distribute delta 3, dispatch absence proven with dispatch deltas 0; Plan 239 closed the dispatch observer); Plan 239 closed with `P239-D-TARGET-LEASESET-LOOKUP-FAILED` (local absent, found 0, failed 3/4/4, dispatch 0/0 on `d82cf06` x3); Plan 240 closed with `P240-C-B-ZERO-HOP-UNKNOWN-RI` (exact streaming ISJ correlated with B eligible and in `toTry`, zero-hop-unknown skip proven on `37025f9` x3); Plan 241 closed with the exact A/B build-stage boundary on `33ebdc4` (forced zero-hop fixture removed, one-hop profile proven active, 1+1 non-zero-hop tunnels installed but not exact-via-C once, bootstrap gate honest on profile stochasticity twice; source lock 35 rows byte-identical); Plan 242 closed with the corrected bootstrap and pair gates on the implementation head (§5 stock-candidate-population evidence replaces the Plan-241 C-profile gate; §7 non-zero-hop pair replaces Plan-241 exact-via-C; §6 extended `P242-CLIENT-TUNNELS` row records role/path facts and `contains_c` as diagnostic only; source lock grew 35→38 with the TunnelPeerSelector / ClientPeerSelector selector-semantics needles; the §20 stock-client-build successor is the next executable lane); Plan 243 closed the hosted stock-client-build qualification on `f359baba57fc7d952ab7f5a5367d34671c590b18` (Direction A established on 2/3 attempts; reverse direction remained bounded by historical Plan-236 output). Plan 244 closed the continuous response attribution on `154e92d8436dbad0b020a0b19d848d2d227a22af` (Direction A established 3/3; fully bound response epochs; `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` 3/3). Plan 245 closed the stock-response construction-signal attribution corrective on `c866967f114ce15012d3256e38eebdd1f7e9f6f5` (counted attempts on `685a59f` after the Plan-237-stats extract; Direction A established 3/3; Stage A.0 terminal `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` 3/3 with identical deltas; the Plan-244 retransmit-timer proxy was not a false negative — construction genuinely did not occur because the scheduler rescheduled; source lock grew 38→50 rows with the Plan-245 needles, md5 `890f7e0cef20d0707d4ff348b6843387`). Plan 201 remains blocked pending the Plan-245 §17 narrow-timer/state-attribution successor plus publication/final closure; Plan 204 remains blocked pending M6 Java closure via that successor; Plan 205 remains retained-deferred.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 217 closed the harness corrective; Plan 218 retains the reverse-delivery behavioral stop; Plan 219 attribution is superseded; Plan 220 refuted J219-B; Plan 221 is superseded-before-execution; Plan 222 narrowed OCMOSJ to status 17; Plan 223 corrected identity/LS2 separation and moved the tracked send to ACCEPTED→NO_LEASESET; Plan 224 closed with an observability gap; Plan 225 closed with exact lookup-path attribution; Plan 226 closed with `P226-BASELINE-B-ZERO-HOP-UNKNOWN` and no topology correction; Plan 227 closed with `P227-EXPLICIT-ONE-HOP-NOT-BUILT`; Plan 228 closed with `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both`; Plan 229 closed with `P229-C-NOT-EXPLORATORY-ELIGIBLE`; Plan 230 closed with the reverse-delivery boundary after proving predicate/correction/bootstrap/forward-delivery; Plan 231 closed with `P231-C-TARGET-IBGW-NOT-INSTALLED` and a lease-fixture root cause; Plan 232 closed at Outcome B (raw reverse passed, Streaming SYN-ACK boundary); Plan 233 is superseded-before-execution; Plan 234 closed with `P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`; Plan 235 closed with `P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND`; Plan 236 closed with `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`; Plan 237 closed with `P237-D-ROUTER-I2CP-NOT-OBSERVED` (Plan 238 closed the Router-A observer); Plan 238 closed with `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` (Plan 239 closed the Router-A dispatch observer); Plan 239 closed with `P239-D-TARGET-LEASESET-LOOKUP-FAILED` (local absent, found 0, failed 3/4/4); Plan 240 closed with `P240-C-B-ZERO-HOP-UNKNOWN-RI` (exact streaming-epoch attribution on `37025f9` x3); Plan 241 closed with the exact A/B build-stage boundary on `33ebdc4` (one-hop fixture corrective executed, old zero-hop terminal structurally unreachable, §20 successor open); Plan 242 closed the corrected bootstrap and pair gates on the implementation head (§5 stock-candidate-population gate replaces the Plan-241 C-profile gate; §7 non-zero pair replaces Plan-241 exact-via-C; §6 extended `P242-CLIENT-TUNNELS` row records role/path facts and `contains_c` as diagnostic only; source lock grew 35→38 with the TunnelPeerSelector / ClientPeerSelector selector-semantics needles; live counted external attempts unexecuted on this host). Plan 243 closed the hosted stock-client-build qualification on `f359baba57fc7d952ab7f5a5367d34671c590b18` (three counted same-SHA executions; Direction A established on 2/3 attempts; one earlier stock-Java `I2PSession.connect()` handshake ceiling stop; reverse direction still open). Plan 244 closed the continuous response attribution on `154e92d8436dbad0b020a0b19d848d2d227a22af` (three counted same-SHA executions; Direction A established on 3/3 attempts with fully bound epochs; reverse direction bounded repeatably at `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED`; no production change). Plan 245 closed the stock-response construction-signal attribution corrective on `c866967f114ce15012d3256e38eebdd1f7e9f6f5` (counted attempts on `685a59f` after the Plan-237-stats extract; Direction A established on 3/3 attempts; Stage A.0 terminal `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` 3/3 with identical deltas; the Plan-244 retransmit-timer proxy was not a false negative — construction genuinely did not occur because the scheduler rescheduled; source lock grew 38→50 rows with the Plan-245 needles, md5 `890f7e0cef20d0707d4ff348b6843387`). Plan 201 remains blocked pending the Plan-245 §17 narrow-timer/state-attribution successor plus publication/final closure; Plan 204 remains blocked pending M6 Java closure via that successor; Plan 205 remains retained-deferred.
