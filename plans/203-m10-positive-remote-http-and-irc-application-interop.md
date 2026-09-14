# Plan 203 — M10 positive remote HTTP and IRC application interoperability

Status at registration: **registered-blocked-by-plan202**.

Plan 203 converts the two remaining Plan 181 remote application rows from deliberate blocker probes into positive independent-service evidence.

It depends on Plan 202's production remote Destination/Streaming path. It does not need to wait for the Java Plan 201 branch; application qualification may proceed against retained-passed exact-pinned i2pd while Java closes in parallel. Final M10 closure still waits for both branches in Plan 204.

## 1. Goal

Produce command-derived positive evidence for:

```text
remote-independent-http-eepsite = passed
remote-independent-irc-service = passed
```

using ordinary unmodified application clients through the production M10 service-tunnel manager and a genuinely independent I2P router/service.

## 2. Frozen references

Use existing pins:

```text
i2pd 2.61.0
635b013a612ff47278ef02acf8580a28e10e26c5

jaraco/irc
90e10e690da2c7bf60de21be4e36d24c9ffd7474

curl = system client recorded by version in evidence
```

The i2pd reference must be built from the verified cache used by existing interop lanes.

## 3. What changes from the current runner

Current `tests/integration/service-tunnels/run-independent.sh` intentionally provisions an independent destination and then expects:

```text
established = 0
unknown_peer > 0
delivered = 0
```

It records both remote rows as `blocked`.

Plan 203 must replace that final negative qualification with real independent services and positive application traffic. The negative unreachable test may remain as a regression test for an unavailable/unroutable destination, but it cannot be the final remote acceptance path.

## 4. Independent service topology

Use exact-pinned i2pd as the independent router.

Preferred service hosting mechanisms, in order:

1. stock i2pd server-tunnel configuration pointing to loopback fixtures;
2. public i2pd SAM STREAM session that forwards to loopback fixtures;
3. another documented public i2pd application surface.

Do not create an in-tree fake I2P router or shadow Streaming implementation.

Private destination keys stay in the disposable scratch directory. Retained evidence may contain only public destination material hashes/lengths, pins, command statuses, counters, and payload digests.

## 5. Remote HTTP service row

Required topology:

```text
unmodified curl
 -> i2pr HTTP client proxy listener
 -> production M10 remote routing (Plan 202)
 -> real i2pr Destination/Streaming/tunnels
 -> exact-pinned i2pd
 -> independently owned I2P HTTP service Destination
 -> ordinary loopback HTTP fixture
```

### Required HTTP sub-evidence

At minimum:

```text
HTTP GET status + body digest
HTTP POST/request-body digest
large response or request spanning multiple Streaming packets
Host/target resolved only as .i2p / configured Destination
Plan 176 hop-by-hop/privacy rewrite retained
no clearnet DNS/IP/outproxy fallback
remote Streaming established counter > 0
local-coowned delivery not used for the remote peer
clean close/resource baseline
```

The single final row `remote-independent-http-eepsite` may aggregate these facts only if every mandatory subfact passed in the same run.

Optional CONNECT coverage may be retained from local rows; it is not a substitute for the remote eepsite GET/POST/data path.

## 6. Remote IRC service row

Required topology:

```text
exact-pinned jaraco/irc public client API
 -> i2pr IRC client service tunnel
 -> production M10 remote routing (Plan 202)
 -> real i2pr Destination/Streaming/tunnels
 -> exact-pinned i2pd
 -> independently owned I2P IRC service Destination
 -> ordinary loopback IRC fixture
```

### Required IRC sub-evidence

At minimum:

```text
connection established
registration completed / welcome observed
PING/PONG round-trip
bidirectional PRIVMSG/message round-trip
ACTION/CTCP action allowed per current policy
DCC blocked per Plan 178 policy
client-supplied local hostname/address not leaked
reason/user privacy rewrites retained where applicable
remote Streaming established counter > 0
local-coowned delivery not used for the remote peer
QUIT/EOF clean
resource baseline clean
```

Use jaraco/irc as the application client; do not replace it with an in-tree handwritten IRC client for counted evidence.

## 7. Fixture rules

HTTP and IRC loopback fixtures may be small deterministic test servers because they are **application endpoints behind the independent I2P router**, not substitutes for I2P routing or application clients.

Fixtures must:

- bind loopback only;
- use deterministic bounded payloads;
- record digests/counts, not sensitive raw data;
- fail closed on malformed setup;
- shut down cleanly.

## 8. Evolve the service-tunnel evidence checker

`scripts/check-service-tunnel-acceptance-evidence.sh` currently encodes the two remote rows as intentionally blocked.

Change the final/full lane semantics to require:

```text
remote-independent-http-eepsite = passed
remote-independent-irc-service = passed
```

The local-only lane may still mark them not-run/blocked with explicit lane provenance.

The **full** lane must fail if either remote row is blocked, failed, missing, or not command-derived.

## 9. Required anti-cheat/static invariants

Reject final remote passes when any of the following is true:

- literal/unconditional `passed` record;
- no executed curl/jaraco command status;
- i2pd source pin missing/mismatched/dirty;
- jaraco pin missing/mismatched/dirty;
- remote destination is actually owned by the same i2pr manager;
- local co-owned bridge handled the remote peer;
- no Plan 202 remote lookup/establishment facts;
- clearnet DNS/IP/outproxy fallback;
- HTTP/IRC client replaced with an in-tree counted shadow client;
- private keys copied to evidence;
- public-network dependency;
- `|| true` or skipped dependency converts a mandatory row to pass.

## 10. Preserve the 29 retained local rows

The full lane must continue to execute and pass all retained local/independent-application rows from Plan 181.

Do not delete local coverage to reduce runtime.

Expected final summary after Plan 203:

```text
m10_local_rows: 29/29 passed
m10_remote_rows: 2/2 passed
m10_blocked: 0
m10_failed: 0
m10_missing: 0
```

This is application-evidence closure only; Plan 204 still owns final milestone authority/CI normalization.

## 11. Expected files

Likely changes:

```text
tests/integration/service-tunnels/run-independent.sh
scripts/check-service-tunnel-acceptance-evidence.sh
.github/workflows/service-tunnels-external.yml
crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs
crates/i2pr-daemon/tests/<positive remote application support tests>.rs
tests/integration/service-tunnels/fixtures/*   # only deterministic loopback app fixtures if needed
plans/203-status.md
```

Avoid product changes in Plan 203 unless the positive application run reveals a profile-specific bug that is reproducible after Plan 202's generic remote transport passes.

If such a bug appears, fix only the relevant HTTP/IRC service profile and preserve Plan 202's generic transport evidence.

## 12. Validation

On the candidate Plan 203 head:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash tests/integration/service-tunnels/run-independent.sh
cargo deny check advisories bans sources
```

Run the full external lane with exact-pinned i2pd and jaraco dependencies available.

If a mandatory remote row requires retry, require two complete clean full-lane passes on the same exact head.

## 13. Acceptance criteria

Plan 203 passes only when:

1. Plan 202 is passed.
2. Exact-pinned i2pd is used and verified clean.
3. Exact-pinned jaraco/irc is used and verified clean.
4. Independent HTTP service Destination is not co-owned by i2pr.
5. Independent IRC service Destination is not co-owned by i2pr.
6. curl reaches the HTTP fixture only through i2pr -> I2P remote path.
7. HTTP GET status/body digest passes.
8. HTTP POST/request-body path passes.
9. HTTP multi-packet path passes.
10. HTTP privacy/header policy remains correct.
11. No clearnet fallback occurs.
12. jaraco/irc connects only through the i2pr IRC client tunnel.
13. IRC registration/welcome passes.
14. IRC PING/PONG passes.
15. Bidirectional IRC message round-trip passes.
16. ACTION policy passes.
17. DCC remains blocked.
18. Host/address privacy policy passes.
19. Remote routes show Plan 202 lookup/Streaming establishment facts.
20. Local-coowned delivery counters do not account for the remote peer.
21. Clean QUIT/EOF/resource shutdown passes.
22. All retained 29 local rows remain passed.
23. Both remote rows are command-derived `passed`.
24. Full lane has zero blocked/failed/missing mandatory rows.
25. No private/public-network/test-shortcut violation occurs.

## 14. Handoff

On success:

```text
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_181 = passed-m10-independent-application-service-interop-evidence
plan_195 = evidence-ready-for-final-normalization
milestone10_remote_service_interop = evidence-passed-pending-plan204-normalization
```

Plan 204 may then perform exact-head cross-family + M10 final acceptance and normalize authority records.
