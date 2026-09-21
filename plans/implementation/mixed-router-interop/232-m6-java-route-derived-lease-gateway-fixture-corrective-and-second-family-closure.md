# Plan 232 — M6 Java route-derived lease-gateway fixture corrective and second-family closure

Status: **registered-ready-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure**

## 1. Objective

Correct the exact test-fixture defect proven by Plan 231, then immediately
re-run the retained Java second-family qualification far enough to either:

1. prove digest-matched bidirectional raw-Destination delivery and continue
   through the existing Java Streaming rows to M6 Java-family closure; or
2. stop at the first new exact post-correction boundary with bounded evidence.

Plan 232 is not another exploratory diagnostic campaign.

Plan 231 proved that the published local Standard LS2 advertises the wrong
inbound gateway. The Java external driver builds the real inbound tunnel through
the Java service router (Router A / `service_hash`), but constructs the local
lease with Router B / `java_hash`:

```rust
InboundLeaseSource::from_parts(
    registrations_in[0].slot(),
    Hash::from_bytes(*java_hash.as_bytes()),
    IBGW_RECEIVE,
    ...
)
```

The same defect exists in all three local-lease construction sites in
`crates/i2pr-daemon/tests/java_tunnel_external.rs`:

- raw-Destination local LS2;
- initial Streaming local LS2;
- refreshed/republication Streaming local LS2.

Plan 232 MUST replace that duplicated fixture assumption with a route-derived
lease contract based on the inbound tunnel that the test actually installed.

No production protocol behavior change is authorized by this plan.

## 2. Registration basis

Plan 231 closed as:

```text
passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
```

Its deepest counted run proved:

```text
tracked Java reverse send                  ACCEPTED
Java A outbound gateway enqueue            proven
Router C exact one-hop OBEP processing     proven
selected target lease gateway              Router B
selected target lease tunnel               38401 / 0x9601
Router B exact IBGW for that tunnel        absent pre and post
i2pr exact target TunnelData               zero
```

The driver independently proves the installed inbound route is:

```text
gateway_router          = service_hash  (Router A)
gateway_receive_tunnel  = IBGW_RECEIVE
local_receive_tunnel    = IBGW_NEXT
```

Therefore Java correctly forwards according to the advertised LS2, but the LS2
points to a router that does not own the advertised inbound gateway.

This is a controlled-test fixture defect, not evidence of an i2pr production
wire defect.

## 3. Core design decision

The corrective MUST NOT merely replace `java_hash` with `service_hash` at three
call sites.

Instead, derive every local LS2 lease gateway and gateway tunnel directly from
the installed inbound route in the tunnel registry.

The intended contract is conceptually:

```rust
let receive_id = <the installed local inbound receive id>;
let inbound_route = coord
    .registry()
    .inbound_gateway_route(receive_id)
    .expect("installed inbound gateway route");

let lease = InboundLeaseSource::from_parts(
    registration.slot(),
    inbound_route.gateway_router,
    inbound_route.gateway_receive_tunnel.get(),
    expires,
    published_or_threshold,
);
```

Exact implementation details may follow existing types/APIs, but the source of
truth MUST be the installed route, not a hard-coded Java router role.

The test must retain and explicitly distinguish:

```text
lease gateway               = installed inbound tunnel gateway (Router A today)
LS2 publication target      = Java floodfill/publication router (Router B today)
```

These are different protocol roles and MUST remain independently derived.

## 4. Scope

Authorized files are expected to remain test/planning/evidence surfaces,
primarily:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
tests/integration/m6-interop/run-java.sh            (only if evidence wiring requires it)
plans/closure/mixed-router-interop/232-status.md    (closure)
plans/registry.md
plans/subsystems/mixed-router-interop-roadmap.md
plans/closure/mixed-router-interop/201-status.md
plans/closure/service-tunnels/204-status.md
plans/subsystems/service-tunnels-roadmap.md
```

No production `src/` change is authorized unless the corrected route reaches a
new, independently proven i2pr-owned defect. If that occurs, stop Plan 232 at
the exact boundary and register a separate production corrective.

## 5. Invariants

1. Java I2P remains exact-pinned at
   `9134f808337b401e8e53c73734c81fab04280c9d`.
2. i2pd remains exact-pinned at
   `635b013a612ff47278ef02acf8580a28e10e26c5`.
3. Plan-230 C1+C2 controlled-topology corrections remain unchanged.
4. Plan-231 observability remains intact.
5. No topology expansion, public I2P, reseed, VMComm, `netDb.alwaysQuery`,
   direct tunnel installation, profile mutation, or Java reference patching.
6. No timeout inflation:
   - 30-second Plan-230 natural-profile poll stays frozen;
   - five-minute helper/tunnel ceiling stays frozen;
   - 45-second reverse payload window stays frozen;
   - 70-second status-only window stays frozen.
7. Router B remains the controlled NetDB/floodfill publication target where the
   retained lane requires it.
8. The LS2 lease gateway MUST come from the installed inbound route and MUST NOT
   be inferred from the publication target.
9. The LS2 lease tunnel id MUST come from the same installed inbound route and
   MUST match the actual gateway-side receive tunnel.
10. Raw logs remain scratch-only.
11. M10 product authority remains closed and is not reopened by this M6 work.
12. Plan 205 SAM/helper pivot remains off-path.

## 6. Work package A — route-derived lease helper/contract

Refactor the Java external test driver so all local lease construction uses one
small test-only helper or equivalent shared contract.

The helper MUST accept/derive enough information to prove:

```text
registration slot
local inbound receive id
installed inbound route exists
installed route gateway router hash
installed route gateway receive tunnel id
installed route local receive tunnel id
lease expiration
lease threshold/publication time inputs
```

It MUST return an `InboundLeaseSource` whose:

```text
gateway == installed_route.gateway_router
tunnel_id == installed_route.gateway_receive_tunnel
```

Do not derive either field from:

- `java_hash`;
- `service_hash` directly;
- the publication target;
- historical constants alone;
- expected role names such as A/B.

Hard-coded constants may remain only as regression assertions against the
controlled fixture, not as the source of the lease.

### Required route assertions

For the current controlled topology, retain or add assertions that:

```text
installed_route.gateway_router == service_hash
installed_route.gateway_receive_tunnel == IBGW_RECEIVE          (destination)
installed_route.local_receive_tunnel == IBGW_NEXT                (destination)
```

and the corresponding Streaming namespace:

```text
installed_route.gateway_router == service_hash
installed_route.gateway_receive_tunnel == STREAM_IBGW_RECEIVE
installed_route.local_receive_tunnel == STREAM_IBGW_NEXT
```

These assertions document the fixture; the route object remains authoritative.

## 7. Work package B — correct all three local lease sites

Current source contains exactly three
`InboundLeaseSource::from_parts(...)` local-lease constructions in the Java
external driver. Plan 232 MUST cover all three.

### B1 — raw-Destination local LS2

Replace the current Router-B gateway assumption with the route-derived lease.

Before publication, emit bounded evidence:

```text
p232-destination-lease-route
  gateway_matches_installed_route=true
  tunnel_matches_installed_route=true
  gateway_matches_service_router=true
  gateway_differs_from_publication_target=<true|false>
  gateway_tunnel=<id>
  local_receive=<id>
```

For the current topology,
`gateway_differs_from_publication_target=true` is expected and is an important
regression proof.

### B2 — initial Streaming local LS2

Apply the same route-derived contract before the initial Streaming LS2
publication.

Emit:

```text
p232-streaming-lease-route stage=initial ...
```

### B3 — refreshed/republication Streaming LS2

The later fresh/republication lease MUST also rederive or explicitly revalidate
the installed inbound route before rebuilding the LS2.

Emit:

```text
p232-streaming-lease-route stage=refresh ...
```

Do not copy the previously derived Router A hash blindly across a long-running
test. Revalidation is preferred because it proves the refreshed LS2 still
matches live installed tunnel material.

## 8. Work package C — preserve publication semantics

Plan 232 MUST prove that correcting the lease gateway does not redirect NetDB
publication.

For the raw-Destination lane and Streaming publication/republication:

```text
publication_target == java_hash
lease_gateway == installed_route.gateway_router
```

Under the current fixture this means:

```text
publication target = Router B
lease gateway      = Router A
```

Add an explicit bounded evidence row or unit assertion that prevents future
refactors from making these values aliases merely because both are Java
routers.

No change to Java floodfill/publication policy is authorized.

## 9. Work package D — raw-Destination requalification

After the fixture correction, execute the existing destination-only Java lane
without changing the Plan-230/231 topology or windows.

The lane MUST again prove the retained prerequisites:

1. Plan-230 capability predicate eligible;
2. natural Router-C profile bootstrap;
3. non-zero exploratory tunnels;
4. exact one-hop client tunnels through C;
5. target LS2 lookup;
6. digest-matched i2pr -> Java raw-Destination delivery.

Then re-run the tracked Java -> i2pr reverse send.

### Required corrected-route preflight

Before `SEND_TRACKED`, prove:

```text
published_local_ls2_gateway == installed_inbound_route.gateway_router
published_local_ls2_tunnel  == installed_inbound_route.gateway_receive_tunnel
target_router_has_exact_ibgw == true
```

The third proof must query the router identified by the actual advertised
lease, not a hard-coded role.

### Reverse-delivery success

Success requires within the frozen 45-second window:

```text
nonce-correlated ACCEPTED
exact target TunnelData observed by i2pr
tunnel recovery completes
Garlic envelope decodes
Destination dispatcher queues payload
payload length matches
payload SHA-256 matches
```

Terminal:

```text
P232-D-REVERSE-DELIVERY-PASSED
```

If reverse delivery passes, Plan 232 MUST continue directly into Work Package E
on the same corrected implementation. Do not register an intermediate plan.

### New post-correction boundary

If the corrected route has an installed target IBGW but delivery still fails,
retain the Plan-231 stage evidence and emit the earliest new exact boundary:

```text
P232-D-JAVA-FORWARDING-BOUNDARY
P232-D-I2PR-NO-EXPECTED-TUNNELDATA
P232-D-I2PR-TUNNEL-RECOVERY-FAILED
P232-D-I2PR-GARLIC-DECODE-FAILED
P232-D-I2PR-DESTINATION-DISPATCH-MISSED
P232-D-I2PR-PAYLOAD-MISMATCH
P232-D-OBSERVABILITY-GAP
```

A new i2pr-owned boundary ends Plan 232. Do not fix production code under this
fixture corrective.

## 10. Work package E — Java Streaming continuation

If raw-Destination reverse delivery passes, immediately execute the retained
Java Streaming qualification using the same route-derived lease helper.

This is required because the same incorrect `java_hash` lease-gateway
assumption exists in both the initial and refreshed Streaming LS2 today.

The Streaming lane MUST prove, using existing Plan-201/217 acceptance semantics:

- i2pr -> Java connection establishment;
- Java -> i2pr connection establishment;
- bidirectional payload delivery with digest/byte equality;
- close/EOF behavior;
- sibling isolation;
- refreshed/republication LS2 route parity;
- existing cleanup invariants.

No new Streaming architecture is authorized.

If every mandatory retained Java Streaming row passes, Plan 232 MAY close the
Java second-family branch directly and unblock Plan 204 convergence.

If Streaming exposes a genuinely new boundary after route parity is proven,
stop at that exact boundary and keep raw-Destination reverse delivery recorded
as passed.

## 11. Work package F — route-parity evidence contract

Add a small typed Plan-232 evidence/classifier surface sufficient to distinguish
fixture correctness from protocol results.

Required durable facts for each local LS2 construction:

```text
lane=destination|streaming
stage=initial|refresh
registration_slot
local_receive_tunnel
route_gateway_hash
route_gateway_tunnel
lease_gateway_hash
lease_gateway_tunnel
publication_target_hash
gateway_route_match
tunnel_route_match
publication_distinct_from_gateway
```

Hashes/tunnel ids are allowed bounded evidence. No keys, tags, private
Destination material, or payload plaintext.

A lease construction MUST fail closed before publication if either route-match
boolean is false.

## 12. Required focused tests

Add at least:

```text
p232_destination_lease_gateway_matches_installed_inbound_route
p232_destination_lease_tunnel_matches_installed_inbound_route
p232_destination_publication_target_is_not_lease_gateway_source
p232_streaming_initial_gateway_matches_installed_inbound_route
p232_streaming_initial_tunnel_matches_installed_inbound_route
p232_streaming_refresh_revalidates_installed_inbound_route
p232_streaming_refresh_gateway_matches_installed_inbound_route
p232_streaming_refresh_tunnel_matches_installed_inbound_route
p232_route_derived_helper_rejects_missing_inbound_route
p232_route_derived_helper_rejects_slot_route_mismatch
p232_java_hash_cannot_be_hardcoded_as_local_lease_gateway
p232_all_java_local_lease_sites_use_route_derived_contract
p232_corrected_target_router_must_have_exact_ibgw_before_send
p232_reverse_pass_requires_exact_tunneldata_and_digest
p232_raw_reverse_pass_continues_to_streaming
p232_streaming_pass_can_close_java_second_family
p232_timeout_windows_remain_frozen
p232_no_production_surface_change
```

Tests may be named differently if equivalent coverage is explicit.

## 13. Static evidence guards

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` to fail when:

1. any Java external-driver local lease site passes
   `Hash::from_bytes(*java_hash.as_bytes())` as its lease gateway;
2. any local lease gateway/tunnel is derived from the publication target;
3. fewer or more than the expected Java-driver local lease sites are covered
   without an explicit checker update;
4. the destination/Streaming/refresh sites bypass the route-derived helper or
   parity assertion;
5. a local LS2 is published before gateway+tunnel route parity is proven;
6. the publication target is silently changed from the retained Java floodfill
   router;
7. Plan-230 topology corrections are changed;
8. Plan-231 observability is removed;
9. 30 s / five-minute / 45 s / 70 s windows are changed;
10. a `P232` token appears in production `src/`;
11. raw Java logs become durable evidence;
12. public/reseed/VMComm/`netDb.alwaysQuery` appears in the counted lane.

## 14. Attempt discipline

Implementation MUST be committed before counted execution.

Maximum three counted external attempts per implementation SHA. No
between-attempt tuning.

Runs that fail before the Plan-230 natural-profile/bootstrap gate may record the
existing early-stop/observability terminal and consume attempts according to the
existing lane policy; do not extend timers to force natural profile creation.

The first corrected run that reaches raw-Destination reverse send must preserve
all Plan-231 observations so a new boundary is directly comparable.

If raw reverse passes, proceed to Streaming without a new plan or topology
change.

## 15. Verification floor

The closing implementation SHA MUST pass the full routine floor:

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
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'

cargo test --locked -p i2pr-daemon --test java_tunnel_external p232_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p231_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
javac <all staged Java probes/helpers against exact pinned Java jars>
```

If raw-Destination and Streaming reach final qualification, also run every
existing Plan-201 final M6 evidence command unchanged.

## 16. Acceptance criteria

Plan 232 closes successfully only when:

1. all three current Java-driver local lease construction sites use the
   route-derived contract;
2. lease gateway and tunnel id match the installed inbound route before every
   publication;
3. publication target remains independently derived and unchanged;
4. no hard-coded Router-B/`java_hash` gateway assumption remains in local lease
   construction;
5. the corrected target router exposes the exact advertised IBGW before reverse
   send;
6. raw-Destination i2pr -> Java remains digest-matched;
7. Java -> i2pr raw-Destination delivery becomes digest-matched inside the
   frozen 45-second window, OR the first new exact post-correction boundary is
   recorded;
8. if raw reverse passes, existing Java Streaming qualification is run in the
   same plan;
9. if all mandatory Java raw-Destination + Streaming rows pass, Java
   second-family M6 is marked passed and Plan 204 is unblocked for convergence;
10. no production change is made merely to close the fixture;
11. full exact-head verification passes;
12. status/registry/roadmaps/Plan-201/Plan-204 are updated together at closure.

## 17. Closure outcomes

### Outcome A — full Java second-family closure

If corrected raw-Destination reverse delivery and all retained Java Streaming
rows pass:

```text
P232-JAVA-SECOND-FAMILY-PASSED
```

Then:

```text
milestone6_java_mixed_router_interop = passed
plan_201 = closed/passed per retained final authority
plan_204 = dependency-ready for cross-milestone convergence
```

Do not create a ceremonial Java closure successor.

### Outcome B — raw reverse passes, Streaming finds a new boundary

Record:

```text
P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY <exact-stage>
```

Retain raw-Destination pass evidence and register only the narrow Streaming
corrective actually justified.

### Outcome C — corrected route exposes an i2pr-owned raw boundary

Record the exact earliest Plan-232 D-stage terminal and register a separate
production corrective. Plan 232 itself MUST NOT change production code.

### Outcome D — fixture route parity cannot be established

Record:

```text
P232-FIXTURE-ROUTE-PARITY-FAILED
```

This is a test-harness defect and does not justify production work.

## 18. Closure evidence

`plans/closure/mixed-router-interop/232-status.md` must record:

- implementation SHA(s);
- exact Java/i2pd pins;
- proof that all three local lease sites are route-derived;
- destination route/lease/publication-target parity evidence;
- Streaming initial route/lease/publication-target parity evidence;
- Streaming refresh route/lease/publication-target parity evidence;
- exact advertised target IBGW preflight;
- raw-Destination forward and reverse digests/statuses;
- retained Plan-231 stage evidence if reverse still fails;
- Streaming qualification rows if raw reverse passes;
- exact terminal per counted attempt;
- full exact-head verification;
- Plan-201 / Plan-204 unblock audit;
- confirmation that M10 product authority remained unchanged.

## 19. Registration disposition

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
plan_232 = registered-ready-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure

plan_201 = blocked-pending-plan232-route-derived-lease-gateway-fixture-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan232
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## 20. Handoff notes for smaller-model execution

This is intentionally narrow.

Do not begin with Java instrumentation or topology changes. The root cause is
already proven.

First refactor local LS2 lease construction so the gateway hash and gateway
tunnel id come from the installed inbound route. There are exactly three
current call sites in the Java external driver. Cover all of them.

Do not replace `java_hash` with `service_hash` mechanically and stop there.
That would fix today's fixture while preserving the same class of bug. Use the
registry route as source of truth and assert that it currently resolves to
`service_hash`.

Do not change the NetDB publication target. Router B can remain the floodfill
router that stores/serves the LS2 while Router A is the inbound tunnel gateway.

Once route parity is proven, run the existing destination lane. If reverse raw
delivery passes, continue directly through Streaming. The plan is allowed to
close Java second-family M6 if the retained mandatory rows all pass.
