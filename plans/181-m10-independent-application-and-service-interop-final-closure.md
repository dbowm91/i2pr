# Plan 181 — Milestone 10 independent application/service interoperability and final closure

Status: **blocked until Plan 180 passes; final M10 acceptance gate**.

## 1. Goal

Prove the complete M10 application-facing tunnel set through ordinary independent clients and, where required by the MVP roadmap, independently implemented I2P service infrastructure. Close Milestone 10 only on command-derived exact-head evidence.

The final ledger must keep three scopes distinct:

```text
m10_local_application_product
m10_independent_local_application_clients
m10_remote_independent_i2p_service_interop
```

A self-composed i2pr client->i2pr server product is strong local evidence but is not mixed-router interoperability.

Expected closure:

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
plan_180 = passed-m10-service-tunnel-composition-reconcile-and-hardening
plan_181 = passed-m10-independent-application-service-interop-final-closure
milestone10_final_acceptance = closed-via-plan181
next_product_layer = milestone11-planning
```

## 2. Environment and safety contract

Default final lane remains unprivileged and controlled:

```text
root/sudo                   = no
Linux namespaces            = no
Docker                       = no
VM/Multipass                 = no
systemd                      = no
M10 client listeners        = loopback only
M10 server TCP targets      = loopback only
external application clients = unmodified
external I2P implementation  = exact-pinned/unmodified
```

Do not depend on the public I2P network merely to obtain a green result. Prefer a private localhost/reference-router composition. If the only feasible path to the roadmap's independent I2P service criterion would require public-network participation, stop and register a narrow corrective/qualification plan rather than silently broadening the environment contract.

## 3. Retained source floor

Before external work, require explicit passed status records for Plans 174–180 and keep all retained M6/M7/M8/M9 evidence green.

Plan 180 is the authoritative local-product evidence. Plan 181 must not reimplement product logic in test drivers.

## 4. Independent ordinary application clients

### 4.1 HTTP and SOCKS — curl

Use ordinary `curl` as the primary independent HTTP/SOCKS application client.

The runner must record:

```text
curl --version
host OS/image context
```

Do not patch curl or wrap its protocol with an in-tree HTTP/SOCKS parser.

Counted HTTP invocations should include ordinary proxy mode and CONNECT/TLS-like opaque tunnel behavior where practical. Counted SOCKS invocation must use hostname-at-proxy semantics (`--socks5-hostname` or equivalent) so the application does not resolve `.i2p` through clearnet DNS.

### 4.2 IRC — exact-pinned independent library

Use the unmodified `jaraco/irc` project through its public client API:

```text
repository = jaraco/irc
commit     = 90e10e690da2c7bf60de21be4e36d24c9ffd7474
role       = independent ordinary IRC client implementation
```

Fetch exact commit into an ephemeral directory, verify HEAD, install/build into an isolated user/venv/test directory without patching source, and drive registration/join/message behavior through `irc.client` or `irc.client_aio` public APIs.

If this exact revision has a concrete environment/build incompatibility, record it and qualify another mature unmodified IRC client through a narrow update to this plan/status before counting it. Do not replace it with an in-tree raw IRC socket script and call that independent evidence.

### 4.3 Generic TCP

Generic tunnel evidence may use a minimal OS/TCP client because generic tunnels intentionally have no application protocol. Prefer a standard installed tool (`nc`, `socat`, or equivalent) if available and version-record it. A tiny standard-library socket client may be used only for generic byte-stream verification and must not be reused as counted HTTP/SOCKS/IRC protocol evidence.

## 5. Independent local application-client matrix

Run one i2pr daemon/service generation containing the needed local services and ordinary loopback fixtures.

### 5.1 Generic tunnel

Through a generic client -> generic server -> local fixture:

- small payload exact digest;
- >=32 KiB or otherwise multi-segment payload exact digest;
- reverse bytes;
- half-close/EOF;
- two sibling connections;
- stable server Destination after restart/reconcile.

### 5.2 HTTP proxy with curl

Count at least:

- GET through `.i2p` proxy to ordinary HTTP fixture;
- POST body digest;
- larger response digest;
- CONNECT followed by opaque bidirectional bytes (may use a small local TLS/opaque fixture; no TLS interception);
- clearnet/IP target rejection;
- unknown `.i2p` bounded failure.

Record response code/body digest/target-observed privacy-header facts, not raw headers containing user data.

### 5.3 SOCKS5 with curl

Count at least:

- `curl --socks5-hostname` to a configured `.i2p` HTTP/generic service;
- body/digest equality;
- unresolved `.i2p` failure;
- prove application performs no local DNS resolution for target;
- product log/snapshot shows a SOCKS DOMAINNAME CONNECT, never IPv4/IPv6 fallback.

### 5.4 IRC client profile with jaraco/irc

Through i2pr IRC client -> i2pr IRC server -> ordinary loopback IRC fixture, use the external library to:

- connect/register;
- negotiate CAP where library exposes it;
- join a channel or equivalent scripted target;
- send/receive one ordinary PRIVMSG;
- send an ACTION where supported;
- exercise PING/PONG naturally;
- disconnect cleanly.

The fixture must prove the USER hostname presented on the IRC-server side is the authenticated remote Destination projection, not the local client's supplied hostname.

Do not require DCC/WEBIRC/TLS for closure.

## 6. Remote independent I2P service interoperability gate

The MVP roadmap requires browser HTTP traffic to reach an eepsite through i2pr and IRC client/server use cases to interoperate with selected I2P IRC infrastructure. Plan 173 correctly notes that this can intersect the still-unclaimed Milestone 6 mixed-router destination/Streaming debt.

### 6.1 Mandatory qualification attempt

Use the simplest controlled exact-pinned independent implementation available, preferring the existing reference:

```text
Java I2P 2.13.0
repository = i2p/i2p.i2p
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
```

A qualified i2pd revision may be used instead/additionally if it materially simplifies the lane, but it must be exact-pinned/unmodified and the choice documented before counted evidence.

Do not build a large historical rootless/namespace/VM harness. The qualification question is narrow:

```text
Can an i2pr M10 client service establish ordinary destination/Streaming traffic
with a service hosted by an independently implemented I2P router under the
available controlled environment?
```

### 6.2 Required remote service rows

For full M10 closure require, at minimum:

1. HTTP: `curl -> i2pr HTTP proxy -> independent I2P-hosted HTTP service`, with response digest equality.
2. IRC: external IRC client -> i2pr IRC client profile -> independently hosted I2P IRC service, with registration plus bidirectional message evidence.

A generic TCP or SOCKS remote row is useful but not required if HTTP and IRC already prove the two roadmap-specific service classes and Plan 175/177 local independent evidence is green.

### 6.3 Milestone 6 debt stop condition

If the independent-router attempt fails below M10 because the retained M6 destination/Streaming/tunnel implementation cannot interoperate with the independent router:

- preserve all passed local M10 and independent-application-client evidence;
- classify the failure precisely as `m6-mixed-router-streaming-blocker` with command/log provenance;
- do not fake the remote rows through SAM/I2CP or private injection;
- do not weaken the M10 exit criterion;
- stop Plan 181 and register one narrow follow-up corrective plan focused only on the minimum mixed-router destination/Streaming path required by HTTP/IRC service interop.

Do not reopen unrelated NTCP2/SSU2 harness history unless the concrete failure proves transport is the blocker.

## 7. External acceptance runner

Create:

```text
tests/integration/service-tunnels/run-independent.sh
```

Responsibilities:

1. verify repository/head context;
2. verify Plans 174–180 prerequisite status/checkers;
3. fetch/verify exact external pins;
4. record system curl/tool versions;
5. build/start one i2pr daemon with an explicit test-only service-tunnel config on loopback;
6. run independent local application-client matrix;
7. run the controlled independent-I2P qualification/remote rows;
8. collect sanitized command/result facts only;
9. shut down fixtures/clients/router and verify resource baselines;
10. emit deterministic `evidence.json`, `evidence.md`, and a row-oriented results file;
11. exit nonzero if any mandatory row is missing or failed.

Use shell plus tiny external-language drivers only where needed to call a public client API. Do not grow a second protocol harness.

## 8. Evidence rows

At minimum derive separate rows for:

```text
m10-foundation-boundary-checks
m10-local-final-product
generic-small-independent
generic-large-independent
generic-half-close-independent
server-identity-restart-stable
curl-http-get
curl-http-post
curl-http-large
curl-http-connect
http-clearnet-rejected
curl-socks5-domainname
socks-clearnet-ip-rejected
irc-independent-register
irc-independent-message-roundtrip
irc-user-hostname-authenticated-destination
irc-ctcp-policy-local
reconcile-rollback-local
cross-service-resource-bounds
remote-independent-http-eepsite
remote-independent-irc-service
external-clean-resource-baseline
unsupported-profile-ledger
```

Rows may be renamed for consistency, but each final criterion must map to executed evidence.

Every row must carry a classification:

```text
local-product
external-application-client
external-independent-i2p-service
```

No row may become `passed` solely because a status document says it passed.

## 9. Static evidence-integrity checker

Create:

```text
scripts/check-service-tunnel-acceptance-evidence.sh
```

It must reject at least:

- literal/unconditional success assignments;
- missing command-exit gating;
- missing jaraco/irc pin verification;
- patched/vendored external client or router source;
- counted HTTP/SOCKS/IRC rows implemented with in-tree raw protocol drivers;
- HTTP/SOCKS target using clearnet DNS/IP fallback;
- missing remote-independent rows while claiming M10 closed;
- self-composed i2pr service mislabeled as independent-I2P interop;
- public-network participation hidden as an ordinary localhost lane;
- private destination keys/raw application payloads in evidence;
- missing cleanup/baseline gating;
- external lane that skips unavailable dependencies and returns success.

Wire this checker into routine Linux CI after it exists. Keep expensive external execution manual.

## 10. Manual hosted workflow

Create:

```text
.github/workflows/service-tunnels-external.yml
```

Use `workflow_dispatch` initially.

Requirements:

- Ubuntu hosted runner;
- exact repository head checkout;
- Rust toolchain per repository policy;
- Python/tool environment required by the exact-pinned IRC client;
- external independent I2P implementation exact pin verified;
- run M10 static boundary/evidence checkers before external work;
- invoke `tests/integration/service-tunnels/run-independent.sh`;
- upload sanitized evidence artifact on success/failure where safe;
- fail closed on pin/build/readiness/client/service/evidence failure;
- no secrets required.

## 11. Repeatability

Before closure, run the complete counted external matrix at least twice on the same implementation revision if practical.

Any retry must:

- have a small named maximum;
- retry a complete client/service attempt with fresh ephemeral connection IDs;
- never retry deterministic parser/auth/policy failures until green;
- record attempt number;
- still fail after the final attempt.

## 12. Resource/lifecycle evidence

After every counted matrix and final shutdown assert sanitized baselines:

- active service listeners = 0 after manager shutdown;
- active service connections = 0;
- draining generations = 0;
- pending Streaming connects/accepts = 0;
- pending local target connects = 0;
- protocol retained bytes = 0;
- active service destinations return expected baseline;
- no leaked child tasks visible through available sanitized counters;
- persistent identity files remain present and unchanged where expected.

## 13. Final support/docs update

After all mandatory rows pass, normalize in one closure pass:

```text
plans/181-status.md
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

Exact bounded claim:

```text
Milestone 10 service tunnels:
  generic TCP client/server
  HTTP/1.1 .i2p proxy + CONNECT
  SOCKS5 no-auth DOMAINNAME CONNECT
  IRC client privacy profile
  IRC server authenticated-Destination hostname projection
  bounded transactional service lifecycle
  independent application-client evidence
  independent I2P service interoperability for required HTTP/IRC rows
```

Explicitly retain as unsupported/deferred:

- clearnet outproxy;
- SOCKS UDP/BIND/SOCKS4/auth;
- transparent proxying;
- HTTP/2/HTTP/3 proxy termination;
- TLS interception;
- DCC;
- WEBIRC/cloak extensions;
- arbitrary remote admin exposure;
- general address-book/subscription manager;
- broad public-network interoperability beyond exactly demonstrated service rows;
- transit/floodfill roles (M11/M12).

## 14. Full validation floor

On the final implementation revision run at least:

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

Plus every focused Plans 175–180 product suite.

## 15. Final acceptance criteria

Plan 181 / Milestone 10 closes only when:

1. Plans 174–180 have explicit passed status records.
2. Generic client/server independent local byte-stream rows pass.
3. Persistent server Destination is restart-stable.
4. Unmodified curl proves ordinary HTTP proxy GET/POST/large/CONNECT behavior.
5. Unmodified curl proves SOCKS5 hostname-at-proxy `.i2p` CONNECT behavior.
6. Exact-pinned unmodified independent IRC client completes ordinary registration and bidirectional messaging.
7. IRC server presents authenticated remote Destination-derived hostname to the local IRC service.
8. Privacy/unsupported profiles remain explicitly fail-closed.
9. Reconcile rollback/drain/resource bounds remain green under the final composition.
10. Independent HTTP service interoperability passes against an independently implemented I2P service/router.
11. Independent IRC service interoperability passes against selected independently implemented I2P infrastructure.
12. Self-composed rows are never substituted for criteria 10–11.
13. No external client/router source is patched.
14. No root/namespaces/container/VM/systemd requirement is introduced.
15. Client listeners/server targets remain loopback-only per M10 policy.
16. Every mandatory evidence row is command-derived and classified.
17. Evidence contains no private keys/raw application payloads.
18. Static M10 evidence-integrity checker passes and is routine-CI enforced.
19. Routine CI is green on the exact final implementation/closure head.
20. Manual external workflow is green on the exact final implementation/closure head.
21. Preferably two complete external passes succeed on the same revision, or any variance is diagnosed before closure.
22. Support/CONFORMANCE/docs all state the same bounded profile.
23. No M11 transit/floodfill or unrelated router-role work lands in this plan.
24. `plans/181-status.md` records exact closing SHA, workflow run IDs, external client/router pins/versions, artifact IDs/digests, and final scope classification.

## 16. Stop conditions

Do not close M10 if:

- only in-tree scripted HTTP/SOCKS/IRC clients pass;
- remote independent HTTP/IRC service rows are absent;
- the remote rows require a patched reference router;
- a mixed-router destination/Streaming failure is relabeled as an M10 application issue;
- a public-network-only workaround is introduced without an explicit new authority decision;
- cross-client traffic uses private injection after service activation;
- evidence is reconstructed from logs without command provenance;
- routine or external exact-head CI is red.

If the only blocker is the retained M6 mixed-router destination/Streaming debt, create the single narrow corrective described in §6.3 and resume Plan 181 only after it passes.

## 17. Handoff

On success:

```text
plan_181 = passed-m10-independent-application-service-interop-final-closure
milestone10_local_product = passed-via-plan180
milestone10_independent_application_clients = passed-via-plan181
milestone10_remote_service_interop = passed-via-plan181
milestone10_final_acceptance = closed-via-plan181
next_executable_plan = none (milestone11-planning next)
next_product_layer = milestone11-planning
```

Do not begin M11 transit participation within Plan 181.
