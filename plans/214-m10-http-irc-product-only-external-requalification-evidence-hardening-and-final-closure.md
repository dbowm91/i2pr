# Plan 214 — M10 HTTP/IRC product-only external requalification, evidence hardening, and final closure

Status at registration: **registered / blocked by Plan 213**.

Source floor: `232be0f87469175a1f01152a7488ecf026b27eeb` (Plan 212 source closure).

Depends on:

- Plan 213 passed generic router-backed Direction A + Direction B exact-head qualification;
- retained Plan 211 product-only HTTP/IRC harness;
- exact-pinned unmodified i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`;
- exact-pinned jaraco/irc at `90e10e690da2c7bf60de21be4e36d24c9ffd7474`;
- retained Plan 181 local 29-row matrix.

Blocks:

- `remote-independent-http-eepsite`;
- `remote-independent-irc-service`;
- `m10_remote_application_interop`;
- `milestone10_remote_service_interop`;
- `milestone10_final_acceptance`.

## 1. Purpose

After Plan 213 proves the corrected generic router-backed M10 product in both directions, Plan 214 hardens the retained Plan 211 HTTP/IRC acceptance harness so every mandatory row is genuinely command- and fixture-derived, then executes the real application-profile lane twice on one exact source SHA.

This is the terminal Milestone 10 qualification plan if no application/profile defect is found.

Do not reopen the lower router/tunnel/Streaming architecture after Plan 213 passes unless Plan 214 captures evidence that specifically points below the application profile boundary.

## 2. Why Plan 211 cannot simply be rerun unchanged

The Plan 211 harness contains valuable real clients and service specs, but the post-Plan-212 audit found evidence-integrity and execution defects that prevent its current positive rows from being final authority.

At the Plan 212 source head:

1. the counted driver imports and constructs a `_manager_placeholder = ServiceTunnelManager::new(...)`, despite the black-box rule that the counted application driver should consume only `ServiceProduct` + bounded public configuration/diagnostic surfaces;
2. `http-i2pd-pin-ok`, `irc-i2pd-pin-ok`, and `irc-jaraco-pin-ok` are stamped as literal `1` by the Rust driver instead of being tied to command verification;
3. `irc-privacy-rewrite-derived` is written as literal `1` rather than proved from the IRC fixture's observed rewritten line;
4. the DCC row is currently derived from `DCC_SENT` absence rather than proving that a deliberate DCC attempt was blocked before reaching the target;
5. the HTTP `fixture-method-path-observed` fact is inferred from response status/body digest instead of independently reading the target fixture's observed request;
6. the HTTP POST request digest is emitted by the client side but not necessarily matched against the target fixture's received-body digest;
7. the HTTP large response row records a digest/length but the current aggregate guard mostly validates digest shape, not equality to an independently known target-side payload;
8. HTTP status is currently approximated from curl exit + body length rather than captured from curl's actual HTTP status code;
9. `PLAN211_HTTP_TARGET_PORT` and `PLAN211_IRC_TARGET_PORT` are read by the driver but not used to establish causal target-side observation;
10. synchronous `std::process::Command::output()` calls may block while the driver is not continuously calling `ServiceProduct::poll_inbound()`, risking an artificial deadlock/stall in a product whose external harness is responsible for driving the inbound pump;
11. current application counter windows can be polluted by multiple HTTP cases or by unrelated work unless baselines are made operation-scoped;
12. the final aggregate rows need exact-head hosted repeatability, not merely a source-side harness commit plus routine CI.

Plan 214 corrects these issues without replacing the actual M10 HTTP/IRC product code.

## 3. Architecture locks

1. The counted application driver must consume the production `ServiceProduct` boundary.
2. Remove `_manager_placeholder` and direct `ServiceTunnelManager` construction from the counted driver.
3. Do not construct lower router/tunnel/Streaming objects in the counted driver.
4. Do not call manual observation/counter injection helpers.
5. System curl must remain the HTTP client.
6. Exact-pinned jaraco/irc must remain the IRC client.
7. i2pd must remain unmodified and exact-pinned.
8. HTTP and IRC remote destinations must remain independently owned by i2pd.
9. No public I2P participation/reseed requirement.
10. No new production dependency solely for the qualification lane.
11. No private Destination keys or raw application payloads in uploaded evidence.
12. Plan 213 generic proof must be green first; if it is not, do not use Plan 214 to diagnose the router layer.

## 4. Files to inspect before editing

```text
crates/i2pr-daemon/tests/service_tunnels_application_product_only_remote_qualification.rs
crates/i2pr-daemon/src/service_product.rs
crates/i2pr-daemon/src/service_tunnels.rs
tests/integration/service-tunnels/run-independent.sh
tests/integration/service-tunnels/fixtures/http_fixture.py
tests/integration/service-tunnels/fixtures/irc_fixture.py
tests/integration/service-tunnels/clients/irc_driver.py
tests/integration/service-tunnels/clients/parse_i2pd_destination.py
scripts/interop/fetch-service-tunnel-clients.sh
scripts/interop/fetch-ssu2-reference.sh
scripts/check-service-tunnel-acceptance-evidence.sh
.github/workflows/service-tunnels-external.yml
plans/211-status.md
plans/213-status.md
```

## 5. Phase A — remove non-black-box construction from the counted driver

Delete the `_manager_placeholder` construction and any direct `ServiceTunnelManager` / `ServiceTunnelManagerConfig` imports that exist only for it.

The driver should build:

```text
ServiceTunnelSet
StaticAliasTable
ServiceProductSpec
ServiceProduct::start(...)
```

and then use only public product/application surfaces.

If spec validation needs an explicit preflight, call the runtime-neutral configuration validator, not a second daemon manager.

Static checker rule: counted Plan 214/211 driver must not contain `ServiceTunnelManager::new`.

## 6. Phase B — pin facts must be runner-derived

Move exact reference/client pin authority out of literal driver rows.

### B1. i2pd

Before any network process starts, the runner must prove:

```text
source-revision.txt == 635b013a612ff47278ef02acf8580a28e10e26c5
i2pd --version contains 2.61.0
cached checkout HEAD == pin (if checkout retained)
tracked checkout modifications == none
```

### B2. jaraco/irc

Before execution prove:

```text
source-revision.txt == 90e10e690da2c7bf60de21be4e36d24c9ffd7474
git -C <source> rev-parse HEAD == pin
git -C <source> status --porcelain --untracked-files=no == empty
```

### B3. Evidence

Runner-owned evidence should record the exact SHA/version strings, e.g.:

```text
plan214-i2pd-pin-sha
plan214-i2pd-version
plan214-i2pd-cache-clean
plan214-jaraco-pin-sha
plan214-jaraco-cache-clean
plan214-curl-version
plan214-python-version
```

The Rust driver may consume a runner-verified boolean only as a precondition if needed, but final aggregation must bind to the runner's command output, not a literal driver success line.

Remove or de-authorize literal:

```text
http-i2pd-pin-ok = 1
irc-i2pd-pin-ok = 1
irc-jaraco-pin-ok = 1
```

as standalone proof.

## 7. Phase C — continuously pump inbound while external clients are running

The current synchronous subprocess pattern can prevent remote progress while curl/jaraco waits.

Use `tokio::process::Command` or a bounded equivalent so `ServiceProduct::poll_inbound()` remains active during every external application operation.

Recommended helper pattern:

```text
spawn child
while child not exited and before deadline:
  select/poll:
    product.poll_inbound()
    child try_wait / output readiness
    short timer
on timeout:
  terminate child
  record exact phase
```

Requirements:

- no busy loop;
- bounded deadline per case;
- terminate child group on timeout;
- preserve stdout/stderr for sanitized parsing;
- poll inbound after child exit briefly to drain final ACK/close packets;
- do not run a second product just to get a background pump.

If changing `ServiceProduct::poll_inbound()` ownership makes this awkward, add the smallest product-owned pump helper that preserves one router stack and one canonical Streaming manager. Do not expose lower internals to the driver.

## 8. Phase D — HTTP acceptance must be target-observed, not inferred

### D1. GET status/body

Run system curl against the actual M10 HTTP client listener.

Capture actual HTTP status code using curl itself, for example with `--write-out` and a separate body file/stdout framing that cannot confuse body bytes with status metadata.

Require:

```text
curl exit == 0
actual HTTP status == 200
body length == fixture expected length
body sha256 == fixture expected sha256
```

Do not derive status from body length.

### D2. Target-side GET observation

The HTTP fixture must emit a fresh per-request fact with at least:

```text
method
path
request sequence/session id
timestamp or monotonically increasing sequence
```

Before curl starts, record the fixture log offset/sequence. After the request, require a **new** record after that baseline for:

```text
GET /hello
```

Do not infer target observation from response body equality.

### D3. POST

Use deterministic request body bytes and compute their digest on the client side.

The fixture must independently record:

```text
method = POST
path = /post
body length
body sha256
```

Require fixture digest/length to equal the sent body.

Also verify the HTTP response according to fixture contract rather than merely recording its digest.

### D4. Multi-packet response

The fixture must expose the deterministic expected large-response length/digest, either as a constant documented by the fixture or as a target-side generated fact.

Require:

```text
curl exit == 0
large response length == expected
large response sha256 == expected
```

A syntactically valid 64-char digest is insufficient.

### D5. Policy rejection

Retain clearnet and IP-literal rejection cases, but ensure they cannot reach the HTTP fixture.

For each rejection case:

- take fixture sequence baseline before command;
- run curl;
- verify expected local rejection status/error;
- verify no new target fixture record.

This proves policy enforcement rather than merely client-side failure.

## 9. Phase E — HTTP operation-scoped counter evidence

Take counter snapshots around each positive remote HTTP command or a clearly bounded HTTP session window.

At minimum prove for one GET or equivalent positive case:

```text
remote_outbound_composed delta > 0
remote_outbound_requests delta > 0
remote_inbound_dispatched delta > 0
local_coowned_deliveries delta == 0
unknown_peer delta == 0
inbound_orphan_receives delta == 0
```

Do not reuse pre-HTTP counters for IRC.

If LS2 was already cached during `ServiceProduct::start`, do not manufacture an "ordinary lookup" event inside the command window. Instead distinguish:

```text
lookup performed during startup for exact HTTP Destination
or
validated exact-target LS2 already installed from startup resolution
```

Use product/state-derived evidence for the exact target hash.

## 10. Phase F — IRC acceptance must be fixture-derived

Keep exact-pinned jaraco/irc as the client API.

### F1. Registration

Require target fixture observation of:

```text
NICK <expected>
USER <rewritten/private-safe fields>
```

and client observation of the expected welcome numeric.

A `WELCOME=1` client line alone is not enough for privacy behavior.

### F2. PING/PONG

Require both:

- fixture sends PING or logs its challenge;
- client/fixture confirms matching PONG.

### F3. PRIVMSG bidirectional

Require:

- outbound PRIVMSG observed by fixture after the session baseline;
- fixture echo/reply observed by client;
- deterministic message token/digest matches expected session token.

Do not pass from `PRIVMSG_SENT=1` alone.

### F4. ACTION / CTCP

If retained as mandatory, require the fixture to observe the allowed ACTION payload and the client to observe the expected response where applicable.

If ACTION is not part of the M10 final minimum, keep it as a retained policy row but do not let an unrelated optional ACTION behavior block the mandatory IRC service row. Document the choice explicitly.

### F5. DCC blocking

The current logic `DCC_SENT absent -> blocked` is insufficient.

Make the jaraco driver deliberately attempt a deterministic address-bearing DCC/CTCP message through the M10 IRC client profile.

Evidence must prove:

```text
client attempted DCC policy case
M10 profile rejected/removed it according to expected behavior
fixture did NOT observe the forbidden DCC payload after baseline
session remained otherwise usable (unless policy intentionally closes it)
```

Required subfacts should distinguish:

```text
irc-dcc-attempted
irc-dcc-policy-result
irc-dcc-target-observed = 0
```

### F6. Privacy hostname rewrite

Remove literal `irc-privacy-rewrite-derived = 1`.

The fixture must record the actual sanitized USER/hostname fields it receives. Aggregate pass only if those fields match the expected rewritten/private-safe form and do not contain the local machine hostname/IP.

No raw personally identifying hostnames need be uploaded; the fixture can emit booleans or salted/test-token comparisons.

## 11. Phase G — IRC operation-scoped counters

Take `irc_before` only after HTTP work is complete and drained.

After the IRC session require:

```text
remote_outbound_composed delta > 0
remote_outbound_requests delta > 0
remote_inbound_dispatched delta > 0
local_coowned_deliveries delta == 0
unknown_peer delta == 0
inbound_orphan_receives delta == 0
```

Do not attribute HTTP deltas to IRC.

If the HTTP and IRC destinations are distinct, prove the two destination hashes differ and each service spec resolves to its own exact target.

## 12. Phase H — destination/public-key extraction integrity

Retain `parse_i2pd_destination.py` only if its public/private boundary remains correct.

Qualification must prove:

1. the helper reads only the public Destination prefix from each i2pd key file;
2. the computed SHA-256 hash matches the b32 address;
3. HTTP and IRC destination hashes are nonzero and distinct;
4. full private `.dat` contents are never copied into evidence;
5. only b32/hash and, where necessary for configuration, public destination base64 are passed to the Rust driver;
6. evidence artifacts never contain private suffix bytes.

Add a static or unit test that feeds a synthetic key file shape and proves output stops exactly at the public Destination boundary.

## 13. Phase I — fix target-port/facts plumbing

`PLAN211_HTTP_TARGET_PORT` and `PLAN211_IRC_TARGET_PORT` must no longer be read and discarded.

The runner should pass explicit fixture-facts paths/sequence baselines to the aggregation logic, e.g.:

```text
PLAN214_HTTP_FIXTURE_FACTS
PLAN214_IRC_FIXTURE_FACTS
```

It is acceptable for target ports to remain runner-only facts if the driver never needs them directly; remove unused driver env reads rather than preserving misleading inputs.

The pass decision must link:

```text
external client command
  + product listener port
  + exact remote destination
  + target fixture observation
  + product remote counter deltas
```

for the same bounded operation/session.

## 14. Phase J — runner structure

Prefer to keep the retained local matrix in `run-independent.sh` and move final remote application qualification into a narrow runner if this materially reduces complexity.

Suggested path:

```text
tests/integration/service-tunnels/run-plan214-applications.sh
```

Recommended flow:

```text
1. require Plan 213 exact-head status/evidence prerequisite
2. verify exact source SHA
3. verify i2pd and jaraco pins from commands
4. fresh evidence dir; remove stale result files
5. start HTTP/IRC loopback fixtures with fresh fact logs
6. start exact-pinned i2pd controlled router
7. provision independent i2pd HTTP + IRC server tunnels
8. wait for public destination files
9. parse only public destination material
10. invoke corrected Plan 214/211 product-only driver
11. continuously pump ServiceProduct inbound while curl/jaraco run
12. independently evaluate fixture facts
13. independently evaluate production counter deltas
14. write HTTP aggregate row
15. write IRC aggregate row
16. verify cleanup/resource baseline
17. exit nonzero unless both remote rows pass
```

If keeping all remote logic in `run-independent.sh` is clearly less risky, refactor the remote block into shell functions rather than adding more nested branches. Do not duplicate the same qualification in two runners.

## 15. Phase K — static evidence checker updates

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` so it rejects at least:

1. `ServiceTunnelManager::new` in the counted Plan 214/211 driver;
2. direct lower-stack construction;
3. manual observation helper calls;
4. literal mandatory pin success rows;
5. literal privacy-rewrite success rows;
6. DCC pass derived only from absence of `DCC_SENT`;
7. HTTP fixture observation inferred solely from response body/status;
8. HTTP status inferred from body length;
9. unused mandatory target/facts env variables;
10. lack of concurrent inbound pumping while external clients are active;
11. lack of per-application before/after counter windows;
12. same counter window reused for HTTP and IRC;
13. aggregate remote row passing without target fixture evidence;
14. aggregate remote row passing when command exit is nonzero;
15. aggregate remote row passing without exact destination hash proof;
16. aggregate remote row passing without Plan 213 prerequisite authority;
17. stale evidence reuse;
18. private `.dat` file copied to evidence.

## 16. Required HTTP evidence

Minimum HTTP facts:

```text
plan214-source-head
plan214-i2pd-pin-sha
plan214-i2pd-version
plan214-curl-version
http-public-destination-hash
http-product-listener-bound
http-get-command-exit
http-get-status
http-get-response-len
http-get-response-sha256
http-get-fixture-method
http-get-fixture-path
http-post-command-exit
http-post-request-len
http-post-request-sha256
http-post-fixture-body-len
http-post-fixture-body-sha256
http-large-command-exit
http-large-response-len
http-large-response-sha256
http-large-expected-len
http-large-expected-sha256
http-clearnet-rejected
http-clearnet-target-observation-delta-zero
http-ip-literal-rejected
http-ip-target-observation-delta-zero
http-remote-outbound-composed-delta
http-router-delivery-delta
http-remote-inbound-dispatched-delta
http-local-coowned-delta-zero
http-unknown-peer-delta-zero
http-orphan-receive-delta-zero
http-clean-resource-baseline
```

The aggregate `remote-independent-http-eepsite` row may pass only when every mandatory HTTP fact satisfies its expected predicate.

## 17. Required IRC evidence

Minimum IRC facts:

```text
plan214-jaraco-pin-sha
irc-public-destination-hash
irc-product-listener-bound
irc-command-exit
irc-registration-client-welcome
irc-registration-target-nick-observed
irc-registration-target-user-observed
irc-privacy-rewrite-derived-from-target
irc-ping-issued-by-target
irc-pong-observed-by-target
irc-outbound-privmsg-target-observed
irc-inbound-privmsg-client-observed
irc-privmsg-token-match
irc-dcc-attempted
irc-dcc-policy-result
irc-dcc-target-observation-delta-zero
irc-action-result (if retained mandatory)
irc-remote-outbound-composed-delta
irc-router-delivery-delta
irc-remote-inbound-dispatched-delta
irc-local-coowned-delta-zero
irc-unknown-peer-delta-zero
irc-orphan-receive-delta-zero
irc-clean-resource-baseline
```

The aggregate `remote-independent-irc-service` row may pass only when every mandatory IRC fact satisfies its expected predicate.

## 18. Failure classification

Classify exactly one terminal application qualification result per run:

```text
P214-A-prerequisite-plan213-not-green
P214-B-reference-startup-or-pin
P214-C-public-destination-extraction
P214-D-service-product-start
P214-E-http-target-resolution
P214-F-http-request-outbound
P214-G-http-return-path
P214-H-http-policy-or-fixture-integrity
P214-I-irc-target-resolution
P214-J-irc-registration
P214-K-irc-return-path
P214-L-irc-privacy-policy
P214-M-resource-or-evidence-integrity
P214-N-passed
```

If Plan 213 is green and failure is clearly HTTP/IRC profile-specific, fix only the profile/harness boundary first.

Do not reopen SSU2/NetDB/tunnel/Streaming without evidence that the same generic transport property failed again in the Plan 214 run.

## 19. Focused tests required

Add/retain tests proving:

1. Plan 214 driver contains no direct manager construction;
2. pin mismatch blocks before external execution;
3. curl status parser reads actual status independently from body;
4. GET fixture baseline rejects stale records;
5. POST fixture digest must equal sent digest;
6. large expected digest/length mismatch fails;
7. clearnet case fails if target fixture sees a request;
8. IP-literal case fails if target fixture sees a request;
9. HTTP counter window rejects zero outbound delta;
10. HTTP counter window rejects zero inbound delta;
11. IRC registration requires target + client facts;
12. privacy rewrite fails without target-side proof;
13. DCC case requires explicit attempt;
14. DCC case fails if forbidden payload reaches target;
15. IRC PRIVMSG requires target and client facts;
16. IRC counter window is independent from HTTP window;
17. duplicate/conflicting evidence keys fail aggregation;
18. stale evidence is cleared at runner start;
19. private destination key suffix is never emitted by extraction helper;
20. cleanup kills child/reference processes even when a case times out.

## 20. Exact-head validation sequence

On the candidate closure SHA:

```text
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --no-deps
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash tests/integration/service-tunnels/run-independent.sh --local-only
bash tests/integration/service-tunnels/run-plan213-generic.sh
bash tests/integration/service-tunnels/run-plan214-applications.sh
```

If Plan 213 runner is folded into the hosted workflow and not intended as a standalone local command, preserve equivalent explicit commands and artifact provenance.

## 21. Hosted exact-head gate

Run `.github/workflows/service-tunnels-external.yml` in `full` mode twice on the same exact source SHA.

Each run must execute in this order:

```text
Plan 213 generic qualification
  -> must pass
Plan 214 HTTP qualification
  -> must pass
Plan 214 IRC qualification
  -> must pass
final evidence-integrity check
  -> must pass
```

Record:

- source SHA;
- workflow run ID;
- artifact ID/name;
- exact i2pd pin/version;
- exact jaraco pin;
- HTTP aggregate row;
- IRC aggregate row;
- terminal `P214-N-passed` classification;
- cleanup/resource baseline.

Two successful runs must be consecutive for closure. If the second fails, M10 remains open.

## 22. Documentation/authority normalization after success only

After both hosted runs pass, update together:

```text
plans/214-status.md
plans/213-status.md
plans/212-status.md
plans/211-status.md
plans/181-status.md
plans/195-status.md
plans/204-status.md
plans/README.md
README.md
AGENTS.md
docs/architecture/i2pr-service-tunnels.md
```

and any support/conformance ledger that currently carries M10 final-acceptance authority.

Do not alter retained Plan 181 local row provenance beyond linking the remote closure.

Do not claim Java M6 closure.

## 23. Stop conditions

Stop and keep Plan 214 open if:

1. Plan 213 is not green on the candidate head;
2. positive HTTP/IRC rows require manual counter injection;
3. target-side evidence cannot distinguish fresh requests from stale fixture data;
4. privacy/DCC rows cannot be proved without synthetic labels;
5. remote application traffic falls back to local/co-owned delivery;
6. HTTP/IRC qualification requires a shadow lower stack;
7. i2pd or jaraco must be patched;
8. public I2P access becomes mandatory;
9. the 29 retained local M10 rows regress;
10. the two hosted full runs on one SHA are not both green.

## 24. Explicit acceptance criteria

Plan 214 passes only if all are true:

1. Plan 213 is passed on the candidate head or an ancestor with no lower-stack changes after it.
2. The counted application driver does not construct `ServiceTunnelManager` directly.
3. The counted application driver constructs no forbidden lower-stack type.
4. Exact i2pd pin verification is command-derived.
5. Exact jaraco pin verification is command-derived.
6. System curl version is recorded from the invoked binary.
7. HTTP and IRC i2pd Destination hashes are independently extracted and distinct.
8. Private i2pd destination key material never enters evidence.
9. HTTP client listener is the real product listener.
10. curl GET exits 0.
11. actual GET HTTP status is 200.
12. GET body digest/length matches fixture contract.
13. target fixture independently observes fresh `GET /hello`.
14. POST sent digest/length matches target-observed digest/length.
15. large HTTP response digest/length matches independently known expected values.
16. clearnet request is rejected locally and not observed by target.
17. IP-literal request is rejected locally and not observed by target.
18. HTTP has positive remote outbound composition/delivery delta.
19. HTTP has positive remote inbound accepted delta.
20. HTTP has zero local/co-owned, unknown-peer, and orphan deltas.
21. Exact-pinned jaraco client exits 0.
22. IRC client receives welcome.
23. Target observes expected NICK/USER registration.
24. Privacy rewrite is derived from target-observed USER/hostname values, not a literal success.
25. PING/PONG is proved from target + client observations.
26. Outbound PRIVMSG is observed by target.
27. Inbound reply/echo is observed by client with matching token.
28. DCC policy case is explicitly attempted.
29. Forbidden DCC payload is not observed by target.
30. IRC has positive remote outbound composition/delivery delta.
31. IRC has positive remote inbound accepted delta.
32. IRC has zero local/co-owned, unknown-peer, and orphan deltas.
33. HTTP and IRC use independent counter windows.
34. Inbound pumping remains active while each external client operation is in flight.
35. Cleanup/resource baseline is green.
36. Retained local M10 29-row matrix remains green.
37. Routine CI/static floors are green on exact closure SHA.
38. Hosted full service-tunnels workflow passes once on exact SHA.
39. Hosted full service-tunnels workflow passes a second consecutive time on the same SHA.
40. Both runs produce `P214-N-passed`.
41. `remote-independent-http-eepsite` is passed from command + target + counter evidence.
42. `remote-independent-irc-service` is passed from command + target + counter evidence.
43. `m10_remote_application_interop` is passed only after those two aggregate rows.
44. M10 authority/docs are updated atomically after evidence is green.
45. No file claims Java M6 second-family closure as a consequence of M10 success.
46. `milestone10_final_acceptance` becomes closed only after all above criteria pass.

## 25. Recommended commit sequence for a smaller model

```text
Commit A:
  remove placeholder manager
  convert external subprocess handling to concurrent inbound-pump pattern
  pin evidence moved to runner

Commit B:
  HTTP target-side facts + actual status + POST/large/rejection integrity
  HTTP focused tests

Commit C:
  IRC target-side registration/privacy/DCC/PRIVMSG integrity
  IRC focused tests

Commit D:
  runner aggregation + per-app counter windows + static checker
  workflow full-lane ordering Plan 213 -> Plan 214

Commit E:
  local exact-head validation
  hosted full run #1

Commit F:
  hosted full run #2 on same SHA
  authority normalization only if both hosted runs pass
```

Do not combine evidence-claim documentation with unexecuted harness code.

## 26. Success authority transition

Only after Plan 214 terminal acceptance:

```text
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_211 = retained-harness-superseded-by-plan214-final-evidence
plan_214 = passed-m10-http-irc-product-only-external-requalification-and-final-closure

m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = passed-via-plan214
milestone10_remote_service_interop = passed-via-plan213-and-plan214
milestone10_final_acceptance = closed-via-plan214
next_product_layer = milestone11-planning
```

The Java M6 second-family branch remains independently open until its own evidence closes it. Plan 204 later performs cross-milestone normalization when both branches are independently complete.
