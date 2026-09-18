# Plan 211 — M10 product-only remote HTTP/IRC closure and exact-head acceptance

Status: **registered / blocked on Plan 210**.

Source floor: `16f569a1b3ef57310f038d0776b95ac0d2a4ad9d` plus successful Plan 210 implementation.

Supersedes for execution: the over-promoted closure interpretation of Plan 209. Retain Plan 209's `ServiceProduct` composition helper and real curl/jaraco subprocess work where useful; remove or correct any fake/uncausal evidence.

## 1. Objective

Close Milestone 10 itself after Plan 210 proves the real generic remote product path.

Plan 211 is primarily an acceptance/evidence pass. Product changes are permitted only when the black-box HTTP/IRC run exposes a real M10 application-composition defect. Do not reopen lower protocol layers merely because an application row fails.

Required final product rows:

```text
system curl
 -> actual i2pr HTTP client proxy
 -> actual service-owned StreamingManager
 -> Plan 210 real service Destination path
 -> exact-pinned i2pd-hosted HTTP Destination
 -> ordinary HTTP fixture

exact-pinned jaraco/irc
 -> actual i2pr IRC client profile
 -> actual service-owned StreamingManager
 -> Plan 210 real service Destination path
 -> exact-pinned i2pd-hosted IRC Destination
 -> ordinary IRC fixture
```

The counted driver is black-box with respect to i2pr networking internals.

## 2. Preconditions

Do not execute the counted Plan 211 rows until Plan 210 status is `passed-m10-real-service-destination-network-material-and-inbound-streaming` and its bidirectional generic external lane is green on the same implementation lineage.

Required retained evidence:

- Plans 174–180 local product/profile implementation remains passed;
- Plan 181's 29 local/independent-client rows remain green;
- Plan 193 exact-pinned i2pd mixed-router Streaming evidence remains retained-passed;
- Plan 210 Direction A and Direction B generic product rows are green;
- no unresolved Plan 210 stop condition remains.

Plan 211 does not depend on the Java M6 second-family branch. That branch may proceed in parallel and remains relevant to Plan 204 cross-milestone convergence only.

## 3. Exact external pins

Retain the currently documented exact pins unless an explicit reproducible incompatibility forces a narrow update:

```text
i2pd repository = PurpleI2P/i2pd
version          = 2.61.0
commit           = 635b013a612ff47278ef02acf8580a28e10e26c5

jaraco/irc repository = jaraco/irc
commit                = 90e10e690da2c7bf60de21be4e36d24c9ffd7474
```

For extracting the public Destination from i2pd destination key files, prefer an exact-pinned unmodified upstream tool. The current researched candidate is:

```text
PurpleI2P/i2pd-tools
commit = c128e67b426739b6088a2e07e4645fd027c97acd
command = keyinfo -d <private-key-file>
```

`keyinfo -d` output is public Destination material. The private key file itself must never enter evidence, logs, environment output, or i2pr configuration.

If the existing i2pd process can expose the same public Destination cleanly through a public management/SAM surface, that is also acceptable. Do not write a parser for i2pd private key files inside i2pr merely for this plan.

## 4. Product-only test-driver rule

The counted Plan 211 Rust/application driver may import/use only product-level i2pr surfaces needed to:

- build `ServiceTunnelSet` and `StaticAliasTable`;
- build/start `ServiceProduct`;
- obtain product listener addresses;
- obtain sanitized snapshots/counter snapshots;
- stop the product.

It must not directly construct or drive:

- `StreamingManager`;
- `StreamingDestinationAdapter`;
- `DestinationRouting`;
- `EciesSessionManager`;
- `DestinationDispatcher`;
- `DestinationTunnelCoordinator`;
- `ExploratoryBuildCoordinator`;
- `Ssu2DaemonService`;
- `RouterDeliveryService`;
- `RouterDeliveryRequest`;
- tunnel roles/cells/Garlic/I2NP delivery objects;
- direct/private delivery helpers;
- `record_observation` / `record_remote_application_observation` success synthesis.

The shell harness may configure/start exact-pinned external i2pd and ordinary loopback fixtures. It may not patch external source.

## 5. Replace the empty-service Plan 209 shape with real M10 specs

The current Plan 209 driver must not count success from an empty `ServiceTunnelSet` or expected listener ports supplied from the environment.

Build actual enabled M10 service specs before `ServiceProduct::start`.

At minimum, run HTTP and IRC with fresh product instances to keep evidence attribution unambiguous.

### HTTP product instance

Create one enabled `HttpClient` service with:

- loopback listener port `0` (OS-selected is preferred);
- dedicated client Destination unless existing policy explicitly uses a named shared group;
- normal bounded Plan 176 options/timeouts;
- static alias table entry:

```text
alpha-test.i2p -> DestinationRef::ConfiguredDestination(<i2pd HTTP public Destination>)
```

Do not make `alpha-test.i2p` resolve by DNS or by a synthetic local peer mapping.

### IRC product instance

Create one enabled `IrcClient` service with:

- loopback listener port `0`;
- dedicated client Destination unless an explicit shared group is already the retained configuration;
- `DestinationRef::ConfiguredDestination(<i2pd IRC public Destination>)` as the fixed remote target;
- retained Plan 178 privacy/filter options;
- ordinary target Streaming port expected by the i2pd server tunnel.

No test-only peer bridge may be installed for either instance.

## 6. External i2pd service topology

Use one exact-pinned unmodified i2pd router process per counted application run unless a single router with two tunnel definitions is demonstrably simpler and preserves evidence isolation.

The HTTP run must provide:

```text
i2pd server Destination
 -> i2pd server tunnel
 -> loopback HTTP fixture
```

The IRC run must provide:

```text
i2pd server Destination
 -> i2pd server tunnel
 -> loopback IRC fixture
```

Record only:

- i2pd exact commit/version;
- router readiness facts;
- public Destination hash or digest/public string where required for config;
- server tunnel readiness;
- fixture readiness;
- sanitized counts/digests.

Never record private destination keys.

## 7. Phase A — HTTP remote row with real curl

Drive unmodified system `curl` through the actual i2pr HTTP listener.

Required positive requests:

1. GET `/hello`;
2. POST `/post` with a deterministic non-secret body and digest verification;
3. `/large` response large enough to require multiple Streaming packets;
4. optional CONNECT opaque tunnel row if the current exact-pinned i2pd server fixture can support it without broadening scope; retained local CONNECT evidence remains authoritative if remote CONNECT is not required by the original M10 remote criterion.

Required negative/policy checks in the same product revision:

- clearnet target rejected;
- IP-literal target rejected;
- unknown `.i2p` alias fails boundedly;
- no clearnet DNS resolution is attempted for `alpha-test.i2p`.

For each counted positive request snapshot the production remote counters **before and after** the command.

Require causal deltas from that exact command:

```text
remote_outbound_composed_delta > 0
remote_inbound_dispatched_delta > 0
remote_outbound_requests_delta > 0
local_coowned_deliveries_delta == 0
unknown_peer_delta == 0
```

If the first request performs the ordinary LS2 lookup, require lookup-start/success delta. Later requests may legitimately be cache hits, but the run must prove that the actual i2pd HTTP Destination was resolved through ordinary Plan 210 destination-aware lookup at least once.

HTTP row success requires all of:

- curl exit code `0`;
- expected HTTP status;
- expected body digest;
- fixture confirms expected method/path;
- POST fixture confirms expected request-body digest;
- large fixture/response digest matches;
- privacy-header/hop-by-hop facts are derived from the fixture observation, not a literal success assignment;
- required production counter deltas occur in the same command window;
- no local-coowned fallback;
- clean connection baseline after completion.

## 8. Phase B — IRC remote row with exact-pinned jaraco/irc

Use the exact-pinned unmodified library through its public API. A raw in-tree IRC protocol script is not counted evidence.

The external driver must perform, at minimum:

- TCP connect to actual i2pr IRC client listener;
- registration (`NICK`/`USER` through library behavior);
- observe welcome/registration completion;
- natural server `PING` -> client/library `PONG` exchange;
- client -> server/fixture ordinary `PRIVMSG`;
- server/fixture -> client ordinary `PRIVMSG`;
- CTCP ACTION if the library exposes it cleanly;
- prove DCC/address-bearing CTCP remains blocked by retained policy (this may reuse a focused product policy row if sending DCC through the external server is impractical, but do not claim it from a literal flag);
- orderly disconnect/QUIT.

Snapshot production remote counters before/after the IRC session. Require session-scoped deltas:

```text
remote_outbound_composed_delta > 0
remote_inbound_dispatched_delta > 0
remote_outbound_requests_delta > 0
local_coowned_deliveries_delta == 0
unknown_peer_delta == 0
```

The IRC row must not pass merely because the prior HTTP run advanced aggregate counters.

IRC success requires:

- jaraco process exits successfully;
- registration event actually observed;
- fixture sees expected sanitized registration facts;
- PING/PONG observed;
- outbound PRIVMSG fixture digest/token observed;
- inbound PRIVMSG client event observed;
- ACTION behavior observed where counted;
- privacy rewrite is fixture-derived;
- DCC policy result is command/policy-derived;
- required remote counter deltas occur in the same session window;
- clean shutdown baseline.

## 9. Phase C — make evidence causal and fail-closed

Delete or stop consuming literal rows such as:

```text
http-fixture-observed=1
http-policy-retained=1
http-clean-resource-baseline=1
irc-fixture-observed=1
irc-policy-retained=1
irc-clean-resource-baseline=1
```

when those values are assigned unconditionally.

Every mandatory row must be derived from one of:

- command exit/status;
- parsed application response;
- fixture-produced observation file/socket event;
- before/after product counter snapshot;
- product shutdown/resource snapshot;
- exact external pin verification.

A status markdown file is never evidence.

## 10. Required sanitized subfacts

The precise row names may match current Plan 209 naming, but the counted evidence must provide equivalent facts.

HTTP minimum:

```text
http-curl-version
http-i2pd-pin-ok
http-public-destination-loaded
http-product-listener-bound
http-command-exit
http-status
http-response-digest
http-fixture-method-path
http-post-request-digest
http-large-response-digest
http-ordinary-ls2-lookup-proven
http-remote-outbound-composed-delta
http-remote-inbound-dispatched-delta
http-router-delivery-delta
http-local-coowned-delta-zero
http-unknown-peer-delta-zero
http-clearnet-rejected
http-ip-literal-rejected
http-clean-resource-baseline
```

IRC minimum:

```text
irc-jaraco-pin-ok
irc-i2pd-pin-ok
irc-public-destination-loaded
irc-product-listener-bound
irc-command-exit
irc-registration-observed
irc-ping-pong-observed
irc-outbound-privmsg-observed
irc-inbound-privmsg-observed
irc-action-observed-or-retained-policy-reference
irc-dcc-blocked-derived
irc-privacy-rewrite-derived
irc-ordinary-ls2-lookup-proven-or-cache-proven-after-same-target-resolution
irc-remote-outbound-composed-delta
irc-remote-inbound-dispatched-delta
irc-router-delivery-delta
irc-local-coowned-delta-zero
irc-unknown-peer-delta-zero
irc-clean-resource-baseline
```

Aggregate authoritative rows:

```text
remote-independent-http-eepsite
remote-independent-irc-service
```

Each aggregate row is `passed` only if every mandatory subfact for that application is present and positive in the same run id/head.

## 11. Resource and lifecycle baseline

After each application product instance stops, require command-derived/snapshot-derived proof that:

- active application listeners = 0;
- active service connections = 0;
- pending Streaming connects/accepts = 0;
- pending remote LS2 resolutions = 0;
- remote destination material entries = 0 after full shutdown;
- inbound tunnel owner mappings = 0 after full shutdown;
- draining generations = 0;
- no local target connection remains;
- retained buffered bytes return to baseline;
- daemon child scope has stopped cleanly according to existing sanitized state.

Do not inspect private task internals merely to satisfy the row; reuse existing snapshots and add narrow sanitized counts if needed.

## 12. Runner changes

Update `tests/integration/service-tunnels/run-independent.sh` so the remote section is deterministic and fail-closed.

Required flow:

```text
verify exact i2pr HEAD
verify Plan 210 passed prerequisite
verify curl
fetch/verify exact jaraco pin
fetch/verify exact i2pd pin
fetch/verify exact i2pd-tools pin if used

HTTP phase:
  create fresh evidence/run directory
  start HTTP fixture
  start exact-pinned i2pd + HTTP server tunnel
  extract public HTTP Destination
  invoke counted product-only HTTP test
  aggregate subfacts -> remote-independent-http-eepsite
  stop product/reference/fixture and prove baseline

IRC phase:
  create fresh evidence/run directory or explicit phase id
  start IRC fixture
  start exact-pinned i2pd + IRC server tunnel
  extract public IRC Destination
  invoke counted product-only IRC test
  aggregate subfacts -> remote-independent-irc-service
  stop product/reference/fixture and prove baseline

merge sanitized evidence
fail if either aggregate row != passed
```

No `skip` may return success for missing curl, Python, jaraco, i2pd, public Destination extraction, listener readiness, or fixture readiness.

Retries:

- maximum two full application attempts only for transient readiness/timing issues;
- never retry deterministic parser/policy/pin failures;
- each retry must use fresh connection/session ids;
- record attempt number;
- final failed attempt fails the lane.

## 13. Static evidence-integrity checker

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` so it rejects at least:

1. empty `ServiceTunnelSet` in the counted Plan 211 driver;
2. environment-supplied fake listener ports in lieu of actual product listener discovery;
3. direct construction of lower-stack i2pr networking types in the counted driver;
4. calls to manual observation/success-increment helpers;
5. literal mandatory success assignments;
6. aggregate counter checks without before/after deltas;
7. HTTP and IRC sharing one prior counter delta as proof;
8. missing i2pd exact pin verification;
9. missing jaraco exact pin verification;
10. patched/vendored external router/client source;
11. in-tree raw HTTP or IRC client used as independent evidence;
12. private Destination key contents in evidence/logging;
13. public Destination not actually installed into the M10 service config/alias path;
14. `remote-independent-*` aggregation that can pass with missing subfacts;
15. external lane returning success on unavailable dependency/readiness failure;
16. M10 final status promotion without exact-head external run provenance.

Keep routine CI running the static checker; keep the expensive external execution manual/workflow-dispatch.

## 14. Manual hosted workflow

Update/retain `.github/workflows/service-tunnels-external.yml` as the authoritative external execution entry.

Requirements:

- `workflow_dispatch`;
- Ubuntu hosted runner;
- exact requested repository head checkout;
- exact i2pd and jaraco pins;
- exact i2pd-tools pin if used;
- static boundary/evidence checkers before external execution;
- invoke the same `run-independent.sh` used locally;
- upload sanitized evidence artifact on success and failure where safe;
- no secrets;
- fail closed on missing dependency, pin mismatch, readiness failure, command failure, missing row, or cleanup failure.

## 15. Exact-head repeatability requirement

Before closing M10, run the complete counted Plan 211 remote matrix **twice on the same implementation commit** where hosted capacity permits.

Both runs must record:

```text
i2pr_commit = exact same SHA
i2pd_commit = 635b013a612ff47278ef02acf8580a28e10e26c5
jaraco_commit = 90e10e690da2c7bf60de21be4e36d24c9ffd7474
http aggregate = passed
irc aggregate = passed
cleanup baseline = passed
```

If one run fails and one succeeds, diagnose before closure. Do not average/flakiness-vote the result into a pass.

One exact-head run may be retained temporarily during implementation, but final authority promotion requires the repeatability gate unless a documented infrastructure outage makes the second run impossible. If that exception is used, status must say `evidence-passed-single-run` and must not silently claim two-run repeatability.

## 16. Full validation floor

On the exact closure head run at least:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Retain the complete focused Plan 174–180 product suites and Plan 210 generic external qualification.

## 17. Documentation/support authority to update on success

After evidence is green, normalize M10 claims in the same closure commit or a directly following no-code authority commit:

```text
plans/closure/service-tunnels/181-status.md
plans/closure/service-tunnels/195-status.md
plans/closure/service-tunnels/208-status.md
plans/closure/service-tunnels/209-status.md
plans/closure/service-tunnels/210-status.md
plans/closure/service-tunnels/211-status.md
plans/closure/service-tunnels/204-status.md
plans/README.md
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
specs/support.toml
specs/CONFORMANCE.md
specs/protocols/11-service-tunnels.md
docs/architecture/i2pr-service-tunnels.md
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-client.md
docs/architecture/tooling.md
```

Plan 211 may close **Milestone 10 independently** of the Java M6 second-family branch. Plan 204 remains the later cross-milestone authority/convergence pass; it must not retroactively gate an already proven M10 product on unrelated Java-family validation.

Bounded M10 claim:

```text
Milestone 10 service tunnels:
  generic TCP client/server
  HTTP/1.1 .i2p proxy + CONNECT
  SOCKS5 no-auth DOMAINNAME CONNECT
  IRC client privacy profile
  IRC server authenticated-Destination hostname projection
  bounded transactional service lifecycle
  retained independent local application-client evidence
  real service-Destination tunnel material
  bidirectional generic independent-router product evidence
  independent i2pd-hosted HTTP service interoperability
  independent i2pd-hosted IRC service interoperability
```

Do not broaden this into public-network/global interoperability.

## 18. Explicit acceptance criteria

Plan 211 / M10 closes only when all are true:

1. Plan 210 is passed on the implementation lineage.
2. Plan 181's retained 29 local rows remain green.
3. Counted HTTP driver builds a real enabled `HttpClient` spec.
4. Counted IRC driver builds a real enabled `IrcClient` spec.
5. Actual product listener ports come from the running product, not environment placeholders.
6. i2pd HTTP public Destination is extracted through an unmodified public/reference tool surface.
7. i2pd IRC public Destination is extracted through an unmodified public/reference tool surface.
8. No private reference Destination material enters evidence.
9. `alpha-test.i2p` resolves through the configured static alias to the real i2pd HTTP Destination.
10. IRC fixed target is the real i2pd IRC Destination.
11. System curl HTTP GET passes end-to-end.
12. System curl HTTP POST passes with fixture request-body digest equality.
13. System curl large response passes with digest equality.
14. HTTP privacy/policy facts are fixture/command-derived.
15. HTTP command window has positive remote outbound composition/delivery delta.
16. HTTP command window has positive remote inbound dispatch delta.
17. HTTP command window has zero local-coowned and zero unknown-peer deltas.
18. Exact-pinned jaraco/irc registers successfully end-to-end.
19. IRC PING/PONG is observed.
20. IRC outbound PRIVMSG is fixture-observed.
21. IRC inbound PRIVMSG is client-observed.
22. IRC privacy rewrite is fixture-derived.
23. DCC/address-bearing policy remains fail-closed with derived evidence.
24. IRC session window has positive remote outbound composition/delivery delta.
25. IRC session window has positive remote inbound dispatch delta.
26. IRC session window has zero local-coowned and zero unknown-peer deltas.
27. Each application proves ordinary destination-aware LS2 resolution or an explicitly attributable cache hit after that same target was resolved.
28. No counted driver constructs lower-stack i2pr networking objects.
29. No mandatory success fact is a literal/manual assignment.
30. Both aggregate remote rows fail closed on any missing subfact.
31. Resource/lifecycle baseline is clean after each application instance.
32. Routine static/full validation floor is green on the exact closure head.
33. Manual external workflow is green on the exact closure head.
34. Preferably two complete external runs pass on that same SHA; otherwise the single-run exception is explicitly recorded without claiming repeatability.
35. Support/conformance/docs state the same bounded M10 capability.
36. M10 closure does not claim Java second-family M6 closure.

## 19. Stop conditions

Stop and register a narrow follow-up only if a concrete product defect remains after Plan 210, for example:

- HTTP profile cannot construct a `RemoteDestination` from the validated configured Destination/LS2 without a missing public conversion API;
- IRC profile uses a separate connection path that bypasses the Plan 210 normal service delivery driver;
- i2pd server tunnel requires a specific Streaming port behavior not represented by current M10 configuration;
- exact-pinned jaraco has a reproducible environment incompatibility;
- evidence shows valid Streaming bytes reach the remote application but Plan 176/178 application parsing/policy is wrong.

Do not respond by reintroducing a shadow router stack or weakening the remote rows.

## 20. Terminal authority transition

On success Plan 211 may record:

```text
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = passed-m10-real-service-destination-network-material-and-inbound-streaming
plan_211 = passed-m10-product-only-remote-http-irc-and-final-acceptance
plan_181 = passed-m10-independent-application-and-service-interop-via-plan211
plan_195 = passed-m10-remote-independent-service-via-plan210-and-plan211
m10_remote_transport_core = passed-via-plan210
m10_remote_application_interop = passed-via-plan211
milestone10_remote_service_interop = passed-via-plan210-and-plan211
milestone10_final_acceptance = closed-via-plan211
next_product_layer = milestone11-planning
```

Plan 204 then becomes a later convergence/normalization gate for the independent Java M6 branch plus already-closed M10 authority. It must not downgrade or block M10 after Plan 211 has command-derived exact-head closure evidence.
