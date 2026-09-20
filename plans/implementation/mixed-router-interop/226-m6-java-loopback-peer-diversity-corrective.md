# Plan 226 — M6 Java loopback peer-diversity corrective

Status: **registered-ready-m6-java-loopback-peer-diversity-corrective**

## 1. Objective

Close the exact Plan-225 boundary:

```text
P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
```

by proving whether exact-pinned Java I2P 2.13.0 skips Router B because the
controlled A/B/C SSU2 RouterInfo addresses all occupy the same IPv4 /24
(`127.0.0.0/24`), then correcting only that controlled-test topology if and
only if the skip is proven.

This is a harness/topology corrective, not an i2pr production wire change.

## 2. Current authority

Retain:

- Plan 223: Destination identity / LS2 crypto separation corrected;
- Plan 224: Router B holds a current, validated,
  `receivedAsPublished=true`, query-answerable target LS2;
- Plan 225: Router A helper search starts, exhausts, and does not dispatch the
  target lookup to B;
- Plan 222/223 selector: B is present in the exact production-equivalent
  candidate selector;
- tracked reverse send remains `ACCEPTED (1) -> NO_LEASESET (21)`;
- reverse payload remains absent in the frozen 45-second window.

M6 Java interoperability remains unclaimed.

## 3. Exact-pinned Java source findings

Java pin:

```text
2.13.0
9134f808337b401e8e53c73734c81fab04280c9d
```

Pinned `IterativeSearchJob` sets:

```java
private static final int IP_CLOSE_BYTES = 3;
```

and during `retry()`:

```java
Set<String> peerIPs = new MaskedIPSet(getContext(), h, IP_CLOSE_BYTES);
if (!_ipSet.containsAny(peerIPs)) {
    _ipSet.addAll(peerIPs);
    peer = h;
    break;
}
_log.info(getJobId() + ": Skipping query w/ router too close to others " + h);
_skippedPeers.add(h);
```

Pinned `MaskedIPSet` masks the first `mask` bytes of an IPv4 address.
With `mask=3`, Java therefore treats peers in the same IPv4 /24 as
IP-close for this search.

It also adds a `p<port>` token and optional family token. Current A/B/C
SSU2 ports are independently allocated, so the current obvious shared token
is the `127.0.0/24` address prefix.

Current harness launches all three Java SSU2 transports with:

```text
Router A = 127.0.0.1:<dynamic>
Router B = 127.0.0.1:<dynamic>
Router C = 127.0.0.1:<dynamic>
```

Therefore the Plan-225 result is consistent with Java's normal anti-Sybil
peer-diversity filter, but Plan 226 MUST prove the exact target-job skip before
changing topology.

## 4. Why not use netDb.alwaysQuery

Pinned Java also contains `netDb.alwaysQuery`, explicitly for testing/local
networks. It bypasses normal peer choice and forces one router to be queried.

Plan 226 MUST NOT use it for acceptance authority. It would prove that B can
answer when forced, but would bypass the normal `MaskedIPSet` search behavior
we are trying to validate.

No counted Plan-226 run may set `netDb.alwaysQuery`.

## 5. Invariants

1. No Java source patching.
2. No reflection/private-state mutation.
3. No Java NetDB/tunnel/key injection.
4. No public I2P.
5. No VMComm/network namespace requirement.
6. Java/i2pd pins unchanged.
7. No i2pr production protocol change.
8. No Destination/LS2 crypto change.
9. No helper session/SAM/I2CP semantic change.
10. No tunnel length/count change.
11. No search-limit/concurrency change.
12. No `netDb.alwaysQuery` in authoritative runs.
13. No publication retry or target change.
14. 45-second reverse-payload window unchanged.
15. Raw Java logs remain scratch-only.
16. Existing diagnostic/SAM/I2CP control endpoints remain on `127.0.0.1`.
17. Only Java SSU2 RouterInfo transport hosts may change after the baseline
    skip is proven.
18. A topology correction may not be applied if the exact target job shows a
    different pre-dispatch rejection reason.

## 6. Work package A — target-job skip attribution

Extend the existing bounded scratch-log sanitizer.

Correlate by exact target `IterativeSearchJob` job ID:

1. find `New ISJ for LS <target>`;
2. capture only that job ID;
3. for that same job ID record bounded typed facts:
   - `b_selected_preflight` (retained Plan-222 fact);
   - `b_ip_close_skipped`;
   - `b_old_router_rejected`;
   - `b_zero_hop_unknown_rejected`;
   - `b_encrypted_lookup_unsupported`;
   - `no_ib_client_tunnel`;
   - `no_reply_crypto`;
   - `peer_try_count`;
   - `query_to_b`;
   - `search_failed`.

The authoritative proof for the topology hypothesis is:

```text
b_selected_preflight=true
b_ip_close_skipped=true
query_to_b=false
search_failed=true
```

where `b_ip_close_skipped` requires the exact target job ID and exact Router-B
hash on Java's:

```text
Skipping query w/ router too close to others <Router-B>
```

line.

Do not infer this from shared addresses alone.

### A stop terminals

If the exact target job instead proves another reason, stop without topology
change:

```text
P226-BASELINE-B-OLD-ROUTER-REJECTED
P226-BASELINE-B-ZERO-HOP-UNKNOWN
P226-BASELINE-B-ENCRYPTED-LOOKUP-UNSUPPORTED
P226-BASELINE-NO-INBOUND-CLIENT-TUNNEL
P226-BASELINE-NO-REPLY-CRYPTO
P226-BASELINE-NOT-IP-DIVERSITY
P226-OBSERVABILITY-GAP
```

## 7. Work package B — loopback-subnet host preflight

Only after `b_ip_close_skipped=true`, test the controlled Ubuntu host for
three distinct loopback /24 SSU2 bind addresses:

```text
A = 127.0.1.1
B = 127.0.2.1
C = 127.0.3.1
```

Required checks:

- Python `ipaddress.ip_address(host).is_loopback == true`;
- UDP bind to each address succeeds without privilege;
- the first three octets are pairwise distinct;
- no configured host is `0.0.0.0` or non-loopback.

Do not use `127.0.0.2/.3/.4`: Java masks three IPv4 bytes, so those remain in
one `127.0.0/24` bucket and do not correct the boundary.

If the host cannot bind the distinct loopback /24 addresses:

```text
P226-ENVIRONMENT-DISTINCT-LOOPBACK-SUBNETS-UNAVAILABLE
```

Stop. Do not fall back to public/private LAN addresses or `alwaysQuery`.

## 8. Work package C — controlled topology correction

Modify `tests/integration/m6-interop/run-java.sh` only as needed to expose
explicit SSU2 hosts:

```text
JAVA_SSU2_HOST_A=127.0.1.1
JAVA_SSU2_HOST_B=127.0.2.1
JAVA_SSU2_HOST_C=127.0.3.1
```

Pass those hosts to the existing `ControlledRouter` `ssu2Host` argument.

Keep on `127.0.0.1`:

- SAM listener;
- I2CP listener;
- diagnostic TCP server;
- raw/streaming helper control sockets.

Do not alter `ControlledRouter` transport policy except where a test assertion
must accept any loopback SSU2 host rather than literal `127.0.0.1`.

RouterInfo evidence must prove:

```text
A_SSU2_HOST=127.0.1.1
B_SSU2_HOST=127.0.2.1
C_SSU2_HOST=127.0.3.1
pairwise_mask3_distinct=true
all_loopback=true
```

## 9. Work package D — corrected exact destination lane

After the topology-only correction, rerun:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

Retain the same target, helper, publication path, tracked nonce behavior, and
45-second reverse window.

Required stage evidence:

```text
b_ip_close_skipped=false
query_to_b=true
b_lookup_received=true
b_published_ls_answered=true
```

Then continue to observe:

```text
a_client_tunnel_ls_received
A_CLIENT_AFTER_SEND.validated_present
ordered_statuses
frozen_payload_45s
```

No stage may be inferred from a later stage.

## 10. Plan-226 terminal taxonomy

Exactly one authoritative terminal:

### Baseline/harness attribution

```text
P226-BASELINE-IP-DIVERSITY-CONFIRMED
```

is an intermediate gate only, never final closure.

### Corrected topology terminals

If B is still IP-close skipped despite pairwise /24 distinction:

```text
P226-EVIDENCE-CONTRADICTION-IP-DIVERSITY-PERSISTS
```

If B remains unqueried for a different reason:

```text
P226-NEXT-BOUNDARY-B-STILL-NOT-QUERIED
```

If A dispatches to B but B does not receive:

```text
P226-NEXT-BOUNDARY-A-TO-B-LOOKUP-DELIVERY
```

If B receives but does not answer despite retained answerability:

```text
P226-EVIDENCE-CONTRADICTION-B-ANSWERABLE-NOT-ANSWERED
```

If B answers but A does not observe the DSM:

```text
P226-NEXT-BOUNDARY-B-REPLY-TO-A-CLIENT-TUNNEL
```

If A receives the DSM but client DB remains empty:

```text
P226-EVIDENCE-CONTRADICTION-CLIENT-DSM-NOT-STORED
```

If client DB becomes usable and status 21 disappears but another terminal
appears:

```text
P226-NEXT-BOUNDARY ordered_statuses=<...>
```

If digest-matched reverse payload arrives within 45 seconds:

```text
P226-REVERSE-DELIVERY-PASSED
```

If required evidence cannot be observed:

```text
P226-OBSERVABILITY-GAP
```

## 11. Tests and static guards

Add focused tests for:

1. exact target-job-ID correlation;
2. exact Router-B hash required for IP-close fact;
3. unrelated ISJ skip cannot classify B;
4. shared `127.0.0.x` addresses classify same /24;
5. `127.0.1.1/127.0.2.1/127.0.3.1` classify pairwise distinct mask-3
   prefixes;
6. non-loopback host rejected;
7. failed bind preflight stops before launch;
8. baseline non-IP reason prevents topology correction;
9. `netDb.alwaysQuery` rejected by static evidence guard;
10. SAM/I2CP/diagnostic hosts remain `127.0.0.1`;
11. Java SSU2 RouterInfo hosts match configured distinct loopback /24s;
12. Plan-225 target/search evidence remains reproducible before correction;
13. post-correction B query requires encrypted-DLM dispatch, not selector
    membership;
14. 45-second result remains frozen;
15. exactly one final P226 terminal is emitted.

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` to reject:

- all A/B/C SSU2 hosts in one mask-3 prefix after the correction gate;
- `127.0.0.2/.3/.4` as a purported peer-diversity correction;
- non-loopback SSU2 hosts;
- `netDb.alwaysQuery`;
- Java patch/reflection/state mutation;
- search-limit/tunnel/timing changes;
- raw Java log promotion.

## 12. Verification floor

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

cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p225_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p224 -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
```

## 13. Exact-head external execution

Commit all implementation before counted execution.

Require:

```bash
git status --porcelain=v1
git rev-parse HEAD
```

with an empty status.

Then run the exact destination lane.

Maximum three attempts per implementation SHA, each with fresh scratch
RouterContexts and identical topology/configuration. No tuning between attempts.

## 14. Acceptance criteria

Plan 226 closes only if:

1. exact Plan-225 target-job search is observable;
2. baseline Router-B pre-dispatch reason is proven;
3. topology changes occur only if exact IP-close skip is proven;
4. Java mask semantics are recorded as three IPv4 bytes;
5. chosen SSU2 hosts are loopback and pairwise distinct by those three bytes;
6. host bind preflight passes unprivileged;
7. control-plane endpoints remain on `127.0.0.1`;
8. Java pins remain unchanged;
9. no `alwaysQuery` acceptance override exists;
10. corrected RouterInfos carry the intended SSU2 hosts;
11. corrected exact target search records whether B is dispatched;
12. Router-B answerability is re-proven on the same run;
13. downstream lookup/reply/store stages remain separately observed;
14. ordered tracked-send statuses are recorded;
15. frozen 45-second payload result is recorded;
16. exactly one final P226 terminal is emitted;
17. routine/focused verification passes;
18. registry/roadmap/dependency audit lands with closure.

## 15. Closure and handoff

Update:

```text
plans/closure/mixed-router-interop/226-status.md
```

with:

- implementation SHA;
- exact Java/i2pd pins;
- baseline target-job skip facts;
- loopback mask/bind preflight;
- corrected RouterInfo SSU2 hosts;
- corrected target-job trace;
- B main-LS answerability;
- A client-DB pre/post state;
- ordered send statuses;
- frozen 45-second result;
- final P226 terminal;
- full attempt history;
- verification;
- unblock audit.

If `P226-REVERSE-DELIVERY-PASSED`, do not claim M6 Java closure automatically.
Audit Plan 201 as the likely next qualification/closure owner.

If any `NEXT-BOUNDARY` terminal occurs, register a new bounded successor and
leave Plan 201/204 blocked.

## 16. Registration disposition

```text
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution
plan_226 = registered-ready-m6-java-loopback-peer-diversity-corrective

plan_201 = blocked-pending-plan226-loopback-peer-diversity-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan226
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 226-m6-java-loopback-peer-diversity-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```
