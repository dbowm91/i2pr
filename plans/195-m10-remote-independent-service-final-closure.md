# Plan 195 — M10 remote independent service interoperability and final closure

Status: **registered, blocked by Plan 194**. This plan resumes only the two remote service rows that Plan 181 left blocked on the retained M6 mixed-router Streaming debt. It does not reopen the already-passed M10 local product or independent local application-client matrix.

## 1. Goal

After Milestone 6 mixed-router interoperability closes via Plan 194, complete Milestone 10 by proving the two roadmap-specific remote application paths through ordinary independent clients and independently implemented I2P service infrastructure:

```text
remote-independent-http-eepsite
remote-independent-irc-service
```

Then rerun the final evidence/checker/CI gates and close M10.

## 2. Prerequisites

Do not execute until:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = passed-m6-java-second-family-mixed-router-closure
milestone6_interoperable = passed-via-plan194
```

Retain without reimplementation:

```text
plan_174 = passed service-tunnel foundation
plan_175 = passed generic client/server tunnels
plan_176 = passed HTTP proxy + CONNECT
plan_177 = passed SOCKS5 .i2p CONNECT
plan_178 = passed IRC client privacy profile
plan_179 = passed IRC server profile
plan_180 = passed transactional composition/hardening
plan_182 = passed local-delivery corrective
plan_181 local independent-client matrix = retained-passed (29 rows)
```

The Plan 181 local matrix remains regression evidence. Do not rewrite it simply because the remote substrate is now available.

## 3. Remote HTTP row

Prove:

```text
ordinary curl
 -> i2pr HTTP .i2p proxy
 -> i2pr Streaming client path
 -> mixed-router I2P destination/Streaming substrate proven by M6
 -> independently hosted I2P HTTP service
```

Use a controlled service hosted through one exact-pinned independent router family. Prefer reusing the exact-pinned i2pd or Java fixture from Plans 193/194 rather than introducing a third router implementation.

Required facts:

- ordinary unmodified `curl` invocation and version recorded;
- `.i2p` destination resolved by the I2P path, never clearnet DNS;
- HTTP GET succeeds with exact body digest equality;
- at least one POST or larger response path proves request/response body forwarding;
- HTTP privacy/header policy retained from Plan 176;
- no clearnet outproxy fallback;
- the underlying connection is the real remote Streaming path, not local fabric/SAM substitution;
- bounded cleanup/resource baseline after request completion.

The mandatory Plan 181 row `remote-independent-http-eepsite` becomes `passed` only from command-derived execution.

## 4. Remote IRC row

Prove:

```text
exact-pinned jaraco/irc public API
 -> i2pr IRC client profile
 -> i2pr Streaming client path
 -> independently hosted I2P IRC service
```

Retain the independent IRC client pin from Plan 181:

```text
jaraco/irc
commit = 90e10e690da2c7bf60de21be4e36d24c9ffd7474
```

Use an independently hosted IRC service behind an exact-pinned reference router through public service APIs. A small loopback IRC fixture behind that independent router is acceptable; the fixture may implement IRC application behavior but must not implement I2P protocols.

Required facts:

- exact clean jaraco/irc pin verified and installed without patching;
- registration/welcome succeeds;
- PING/PONG proceeds normally;
- bidirectional PRIVMSG digest/text-token evidence succeeds;
- ACTION remains permitted where already supported;
- DCC remains blocked by the i2pr profile;
- connection travels through remote mixed-router Streaming, not local i2pr fabric;
- clean disconnect/resource baseline.

The mandatory Plan 181 row `remote-independent-irc-service` becomes `passed` only from command-derived execution.

## 5. Router-family choice

Plan 195 does **not** need to prove both router families again at the application layer because Plan 194 already closes the two-family M6 substrate. Choose the simplest retained exact-pinned family for the HTTP/IRC service rows and record why.

If the chosen family exposes an application-specific incompatibility, test the other already-qualified family before changing product code. Do not infer that an application-profile defect is a lower-layer M6 regression without evidence.

## 6. Existing Plan 181 runner

Extend the existing:

```text
tests/integration/service-tunnels/run-independent.sh
scripts/check-service-tunnel-acceptance-evidence.sh
.github/workflows/service-tunnels-external.yml
```

Do not create a parallel M10 acceptance framework.

The runner already owns the 29 local passed rows and two remote blocked rows. Plan 195 should replace the old blocker qualification section with the now-working mixed-router composition while preserving all local command-derived rows.

## 7. Evidence integrity

Update the existing M10 evidence checker so it requires both remote rows as passed before any M10 closure claim. It must continue rejecting:

- self-composed i2pr service mislabeled as independent I2P service;
- raw in-tree HTTP/IRC clients replacing curl/jaraco;
- patched external clients/routers;
- clearnet DNS or outproxy fallback;
- missing exact pin/head cleanliness evidence;
- private destination keys or raw sensitive payload evidence;
- literal/unconditional pass rows;
- missing cleanup/resource baseline;
- skipped external dependency treated as success.

The checker remains routine-CI enforced.

## 8. Final external matrix

On the final implementation revision run:

1. retained Plan 181 local independent-client matrix;
2. remote independent HTTP row;
3. remote independent IRC row;
4. service-tunnel reconcile/resource regressions;
5. M6 mixed-router evidence checker as a prerequisite sanity gate;
6. external clean-resource baseline.

Prefer two complete external passes on the same revision before final closure. Any retry must be bounded, whole-attempt, and recorded.

## 9. Resource/lifecycle requirements

After final shutdown assert:

- active service listeners = 0;
- active service connections = 0;
- draining generations = 0;
- pending Streaming connects/accepts = 0;
- pending local target connects = 0;
- protocol retained bytes = 0;
- reference processes stopped;
- ephemeral loopback ports closed;
- persistent service identity files unchanged where expected;
- no leaked child tasks visible through sanitized counters.

## 10. Full validation floor

On the exact final implementation/closure head require at least:

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
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
cargo deny check advisories bans sources
```

Plus the focused Plans 175–182 product suites and exact external runner.

## 11. Documentation/authority closure

Once both remote rows pass, normalize the authority documents in the same closure commit series:

```text
plans/195-status.md
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

The bounded M10 claim is:

```text
generic TCP client/server
HTTP/1.1 .i2p proxy + CONNECT
SOCKS5 no-auth DOMAINNAME CONNECT
IRC client privacy profile
IRC server authenticated-Destination hostname projection
bounded transactional service lifecycle
independent local application clients
remote independent I2P HTTP service interoperability
remote independent I2P IRC service interoperability
```

Do not broaden the claim to public-network production readiness or unsupported proxy/application profiles.

## 12. Acceptance criteria

Plan 195 passes only when:

1. Plan 194 has closed M6 mixed-router interoperability;
2. all retained local M10 rows remain green;
3. unmodified curl proves `remote-independent-http-eepsite` through the real mixed-router Streaming path;
4. exact-pinned unmodified jaraco/irc proves `remote-independent-irc-service` through the real mixed-router Streaming path;
5. no self-composed/local-fabric/direct-transport substitute is counted;
6. no public-network dependency is introduced;
7. all evidence rows are command-derived and privacy-safe;
8. cleanup/resource baselines pass;
9. static M10 and M6 evidence checkers pass;
10. full workspace/static/dependency floor passes;
11. exact-head routine CI passes;
12. exact-head manual service-tunnel external workflow passes;
13. support/CONFORMANCE/architecture/status documents agree on the bounded claim.

## 13. Closure transition

Only after all criteria pass:

```text
plan_195 = passed-m10-remote-independent-service-final-closure
plan_181 = superseded-by-plan195-final-remote-closure
milestone10_independent_application_clients = passed
milestone10_remote_service_interop = passed-via-plan195
milestone10_final_acceptance = closed-via-plan195
next_executable_plan = none
next_product_layer = milestone11-planning
```

## 14. Stop conditions

If either application row fails after M6 is closed, stop at the first application-layer boundary and create a narrow M10 corrective. Do not reopen M6 unless the same lower-layer failure reproduces in the retained M6 qualification lane.

Do not weaken the remote HTTP/IRC requirement, replace independent clients with raw in-tree drivers, or use the public I2P network merely to obtain a green result.