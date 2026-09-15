# Plan 207 — M10 genuine remote HTTP and IRC application interoperability corrective

Status at registration: **registered-blocked-by-plan206**.

Plan 207 corrects the over-promotion of Plan 203. The current Plan 203 driver proves lower-layer remote Streaming and manager-level routing classification, but it does not derive its HTTP/IRC pass facts from actual unmodified application clients traversing the production M10 listeners. Several evidence labels are advanced manually, and the terminal `http-remote-application-established` / `irc-remote-application-established` facts are emitted after the generic lower-stack exercise.

Plan 207 replaces that synthetic acceptance with actual command-derived application evidence.

## 1. Dependency

Plan 207 starts only after Plan 206 has passed on an exact head.

```text
plan206 -> plan207
plan205 may execute independently
plan204 convergence waits for plan205 + plan207
```

The Plan 207 counted application rows may not use the pre-Plan206 capability/classification-only path.

## 2. Goal

Produce positive command-derived evidence for:

```text
remote-independent-http-eepsite = passed
remote-independent-irc-service = passed
```

through the actual product topology:

```text
system curl / exact-pinned jaraco/irc
 -> real i2pr HTTP or IRC client listener
 -> ServiceTunnelManager profile executor
 -> Plan 206 production remote backend
 -> real i2pr Destination/Streaming/tunnels
 -> exact-pinned i2pd independent service Destination
 -> deterministic loopback application fixture
```

No handwritten replacement client may substitute for curl or jaraco/irc in counted evidence.

## 3. Frozen references

Use the existing verified pins:

```text
i2pd 2.61.0
commit 635b013a612ff47278ef02acf8580a28e10e26c5

jaraco/irc
commit 90e10e690da2c7bf60de21be4e36d24c9ffd7474

curl
system binary; record `curl --version` in sanitized evidence
```

Reference checkouts must be clean. Private Destination material remains in disposable scratch directories only.

## 4. Exact defect to correct

At registration head `c67dc5b594fc32e48bcaee4bcbe8004d0d94d04b`:

- the Plan 203 Rust driver imports and constructs the lower destination/tunnel/Streaming stack itself;
- it installs `ServiceDestinationDelivery` into the manager primarily to assert `RoutingDecision::RemoteRouter`;
- it does not make the actual HTTP/IRC acceptance claim originate from external curl/jaraco commands through the manager listeners;
- it calls `record_remote_application_observation()` over the documented label list rather than deriving each label from observed application traffic;
- target HTTP/IRC ports are present in setup/evidence but the terminal application pass is not bound to observed fixture exchanges;
- terminal `http-remote-application-established=true` and `irc-remote-application-established=true` facts can therefore exist without satisfying the original Plan 203 contract.

Plan 207 must make that impossible.

## 5. Application topology

Provision exact-pinned i2pd as the independent router. Host two independent Destinations using documented public facilities:

1. HTTP service Destination -> deterministic loopback HTTP fixture;
2. IRC service Destination -> deterministic loopback IRC fixture.

Preferred hosting order:

1. stock i2pd server-tunnel configuration;
2. public SAM STREAM server/session if stock server tunnels are impractical;
3. another public documented i2pd application surface.

No in-tree fake router or shadow Streaming implementation is allowed.

## 6. Phase A — launch the real M10 product surface

The external lane must launch the actual service-tunnel manager path used by the product/example harness with Plan 206's real router backend installed.

It must expose real loopback listeners for:

```text
http-client
irc-client
```

The runner must retain machine-readable startup metadata containing only:

- listener addresses/ports;
- public Destination hashes/lengths or approved public material needed by the client config;
- exact pins/versions;
- process identifiers for cleanup.

Do not expose private Destination keys in retained artifacts.

## 7. Phase B — real HTTP evidence using system curl

Invoke the system `curl` binary as a subprocess/command. Do not use a Rust or Python "curl-equivalent" client for the counted HTTP rows.

Required HTTP cases in the same full external run:

### B.1 GET

```text
curl -> i2pr HTTP listener -> remote i2pd eepsite -> fixture
```

Require:

- curl exit status 0;
- HTTP status 200;
- expected body digest;
- fixture observed the request;
- fixture target process is reachable only through the i2pd service Destination in this counted command.

### B.2 POST/request body

Require:

- deterministic request body;
- fixture-observed request-body digest equals source digest;
- expected response status/digest;
- no direct fixture connection from curl.

### B.3 multi-packet path

Use request or response data large enough to cross multiple Streaming packets. Require exact digest equality and byte count.

### B.4 HTTP policy/privacy

Re-prove the applicable Plan 176 properties on the remote path:

- hop-by-hop headers handled correctly;
- Host/target rewritten/resolved according to I2P policy;
- privacy headers remain bounded to documented behavior;
- no local DNS resolution of the `.i2p` target;
- no clearnet IP/DNS/outproxy fallback.

### B.5 close/resource baseline

Require clean EOF/close and no leaked connection/task/backend registrations after the HTTP cases.

## 8. Phase C — real IRC evidence using exact-pinned jaraco/irc

The counted IRC driver may be a small Python program, but it must import and use the installed exact-pinned `irc.client` public API. It must not hand-code the IRC protocol over `socket` for counted rows.

Required lifecycle:

1. connect to the real i2pr IRC client listener;
2. complete registration and receive welcome (`001` or fixture-equivalent accepted welcome);
3. PING/PONG round trip;
4. outbound PRIVMSG observed by the remote fixture;
5. inbound PRIVMSG/echo observed by jaraco/irc callback;
6. CTCP ACTION allowed according to current Plan 178 policy;
7. DCC request blocked/not forwarded according to policy;
8. client-provided local hostname/address does not reach the remote fixture where the policy requires rewrite;
9. QUIT/EOF completes cleanly;
10. resources return to baseline.

Record jaraco source pin and installed package metadata in evidence.

## 9. Phase D — derive evidence from observations, never label injection

For final Plan 207 acceptance, the following are forbidden as sole/primary proof:

```text
manager.record_remote_application_observation("http-get-status")
manager.record_remote_application_observation("irc-ping-pong-roundtrip")
append_evidence(..., "http-remote-application-established", "true")
append_evidence(..., "irc-remote-application-established", "true")
```

unless the value is emitted only after parsing/validating the corresponding command output and fixture fact from the same run.

The evidence pipeline must bind every aggregate row to command-derived subfacts.

Recommended row schema:

```text
http-command-exit
http-status
http-body-digest
http-request-digest
http-multipacket-digest
http-fixture-observed
http-no-clearnet-fallback
http-policy-retained
http-remote-stream-established
http-local-coowned-not-used
http-clean-resource-baseline

irc-driver-exit
irc-registration-welcome
irc-ping-pong-roundtrip
irc-privmsg-outbound-observed
irc-privmsg-inbound-observed
irc-ctcp-action-allowed
irc-dcc-blocked
irc-privacy-hostname-rewrite
irc-remote-stream-established
irc-local-coowned-not-used
irc-clean-resource-baseline
```

The aggregate remote row passes only when every mandatory subfact is passed in the same evidence directory/run id.

## 10. Phase E — checker hardening

Update `scripts/check-service-tunnel-acceptance-evidence.sh` so the full lane rejects:

- literal/unconditional passed rows;
- manual observation-label loops standing in for application commands;
- `curl-equivalent` counted clients;
- handwritten socket IRC clients for counted rows;
- absence of exact jaraco pin verification;
- HTTP/IRC aggregate pass without matching command exit codes;
- aggregate pass without fixture-observed facts;
- remote pass without Plan 206 backend counters/route proof;
- local co-owned delivery for the independent Destination;
- direct fixture connection from the external application client;
- clearnet DNS/IP/outproxy fallback;
- private keys in retained evidence;
- `|| true`, ignored failures, or skipped dependencies converting a mandatory row to pass.

The local-only/routine CI lane may record remote rows as blocked/not-run with explicit provenance. The full external lane must require positive rows.

## 11. Phase F — runner semantics

`tests/integration/service-tunnels/run-independent.sh` must distinguish:

```text
local/routine lane:
  retained 29 local rows pass
  remote rows blocked/not-run with explicit environment provenance

full external lane:
  retained 29 local rows pass
  Plan 206 generic remote rows pass
  remote-independent-http-eepsite passes
  remote-independent-irc-service passes
  zero mandatory blocked/failed/missing rows
```

Do not copy evidence from the M6 driver into the M10 ledger unless that evidence came from the same product process/run and satisfies the Plan 206/207 topology.

## 12. Workflow

Update `.github/workflows/service-tunnels-external.yml` so the full job:

1. fetches/verifies exact i2pd and jaraco pins;
2. provisions the Plan 206 router backend environment;
3. launches deterministic remote HTTP/IRC fixtures;
4. launches exact-pinned i2pd with independent service Destinations;
5. launches the i2pr M10 service product;
6. runs actual system curl cases;
7. runs exact-pinned jaraco/irc driver;
8. runs static/evidence checkers;
9. uploads only sanitized evidence;
10. fails if either aggregate remote row is blocked/failed/missing.

## 13. Expected files

Likely changes include:

```text
tests/integration/service-tunnels/run-independent.sh
scripts/check-service-tunnel-acceptance-evidence.sh
.github/workflows/service-tunnels-external.yml
crates/i2pr-daemon/tests/service_tunnels_application_remote_qualification.rs
examples/service_tunnels_loopback_listener.rs or the existing external product launcher

tests/integration/service-tunnels/fixtures/http_remote_eepsite.py
tests/integration/service-tunnels/fixtures/irc_remote_eepsite.py
tests/integration/service-tunnels/clients/irc_remote_driver.py
```

A helper script for curl command orchestration is fine, but the counted HTTP client must remain the real curl binary.

## 14. Required validation

On the exact Plan 207 candidate head:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc

bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash tests/integration/service-tunnels/run-independent.sh
cargo deny check advisories bans sources
```

Run the full external lane twice on the same exact head before promotion if any network/timing retry was needed on the first pass.

## 15. Acceptance criteria

Plan 207 passes only when all are true:

1. Plan 206 is passed on the same code line.
2. Exact i2pd pin verified clean.
3. Exact jaraco/irc pin verified clean.
4. System curl version recorded.
5. HTTP Destination is independently owned by i2pd, not i2pr.
6. IRC Destination is independently owned by i2pd, not i2pr.
7. curl actually connects to the real i2pr HTTP listener.
8. HTTP GET status 200 observed from command output.
9. HTTP GET body digest matches fixture expectation.
10. HTTP POST/request digest matches at the remote fixture.
11. HTTP multi-packet digest/byte count matches.
12. HTTP privacy/header policy passes on the remote path.
13. No clearnet/DNS/outproxy fallback occurs.
14. jaraco/irc public API actually connects to the real i2pr IRC listener.
15. IRC registration/welcome passes.
16. IRC PING/PONG passes.
17. Outbound PRIVMSG reaches the remote fixture.
18. Inbound PRIVMSG reaches the jaraco client callback.
19. ACTION policy passes.
20. DCC remains blocked.
21. Host/address privacy rewrite passes.
22. Plan 206 remote Streaming/backend counters show real remote establishment for both profiles.
23. Local-coowned delivery is not used for either remote peer.
24. HTTP clean close/resource baseline passes.
25. IRC QUIT/EOF/resource baseline passes.
26. All retained 29 local rows remain passed.
27. Full external lane has zero mandatory blocked/failed/missing rows.
28. Aggregate HTTP/IRC rows are derived from command/fixture facts, not manually injected observation labels.
29. No private-key or raw secret evidence is retained.
30. No public I2P, test fake router, direct fixture bypass, or protocol shortcut is used.

## 16. Authority transition

Before Plan 207 passes:

```text
plan_203 = partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207
plan_181 = passed-local-matrix-remote-rows-blocked-pending-plan207
plan_195 = blocked-m10-remote-independent-service-pending-plan206-and-plan207
milestone10_remote_service_interop = not-yet-passed
```

On exact-head success:

```text
plan_207 = passed-m10-genuine-remote-http-and-irc-application-interop
plan_203 = retained-partial-superseded-by-plan207
plan_181 = passed-m10-independent-application-and-service-interop-via-plan207
plan_195 = evidence-ready-for-final-normalization-via-plan207
milestone10_remote_service_interop = evidence-passed-via-plan206-and-plan207-pending-plan204
```

Final milestone closure remains Plan 204's responsibility and still waits for the independent Java Plan 205 branch.

## 17. Handoff

After Plan 207 passes, Plan 204 may perform final authority/documentation normalization only if Plan 205 has also closed the mandatory Java M6 branch. Plan 204 must not weaken either lane to converge them.
