# Plan 106: daemon NetDB/bootstrap integration and Milestone 5 handoff

## Status and authority

- Status: **planned; blocked until Plan 105 closes**.
- Date: 2026-08-13.
- Parent authority: Plan 102.
- Prerequisites: Plans 103-105.
- Final executable plan in the current Milestone 4 foundation sequence.

## Objective

Compose the validated RouterInfo store, persistent cache, reseed path, and NetDB state machines into the real `i2pr` daemon. Establish bounded startup/readiness/shutdown semantics and a clean runtime seam for future NetDB traffic.

Plan 106 also records a necessary roadmap correction: standards-conformant RouterInfo lookups use exploratory tunnels, while exploratory tunnels are Milestone 5 work. Therefore Plan 106 closes the **Milestone 4 local/bootstrap implementation phase**, then hands execution to Milestone 5. Full Milestone 4 live lookup/publication acceptance is revisited after the required tunnel substrate exists.

Do not invent a direct-link lookup mode to satisfy the old milestone ordering.

## Authority preserved through this plan

```text
ntcp2                     = experimental-non-advertised
normal_daemon_ntcp2       = disabled-and-unenableable
external_netdb_over_ntcp2 = blocked
plan_079                  = deferred
local RouterInfo          = allowed
local NetDB/cache         = allowed
verified HTTPS reseed     = allowed under explicit configuration
public I2P peer operation = not activated by this plan
```

Reseed HTTPS is bootstrap acquisition, not I2P transport interoperability evidence.

## Expected implementation surfaces

```text
crates/i2pr-daemon/src/config.rs
crates/i2pr-daemon/src/lib.rs
crates/i2pr-daemon/src/error.rs          only if needed
crates/i2pr-daemon/tests/...
crates/i2pr-runtime/src/...              narrow effect/service adapter only
crates/i2pr-storage/...                   Plan 104 integration corrections only
crates/i2pr-netdb/...                     Plan 105 integration corrections only
Cargo.toml / Cargo.lock
scripts/check-dependency-direction.sh
scripts/check-runtime-boundaries.sh
README.md / AGENTS.md / specs support docs as actual support changes
```

Do not modify NTCP2 handshake/frame behavior, enable NTCP2, add SSU2, implement tunnels, or revive Plan 099 harness machinery.

## Work package 1 — bounded NetDB/reseed configuration

Add the smallest strict daemon configuration needed for composition. Suggested semantic groups:

```toml
[netdb]
max_router_infos = ...
max_encoded_bytes = ...
cache_enabled = true

[reseed]
enabled = false
```

Exact names follow existing config conventions.

Requirements:

- strict unknown-field handling remains;
- all counts/bytes/timeouts/source-list sizes have explicit maxima;
- network ID remains owned by the existing network config;
- reseed is conservatively disabled by default while the router is experimental unless current project authority intentionally changes that default;
- enabling reseed does not enable any I2P transport;
- `check-config` and `run --dry-run` remain side-effect free.

## Work package 2 — explicit daemon startup phases

The real daemon startup path should become:

```text
load normalized config
 -> initialize logging
 -> load persistent router identity
 -> construct NetDB policy/store
 -> build and self-validate local RouterInfo
 -> load and revalidate RouterInfo cache
 -> assess bootstrap sufficiency
 -> if explicitly enabled and needed, run bounded reseed acquisition/verification/ingestion
 -> persist accepted RouterInfos
 -> publish NetDB bootstrap readiness
 -> enter supervised long-lived lifecycle
```

No router transport starts in this sequence.

Use the existing supervisor/lifecycle framework. Add a real NetDB/bootstrap service only if long-lived ownership requires it; otherwise keep one-shot initialization explicit and small. Do not build another service framework.

## Work package 3 — bootstrap readiness policy

Define bounded local policy for whether the validated store is useful enough for later network integration, including at least:

```text
minimum valid RouterInfos
minimum valid floodfill-advertising RouterInfos
```

These are local policy values, not current network population constants.

Represent bootstrap state explicitly, for example:

```text
empty
cache-sufficient
reseed-required
reseeding
ready-for-network-integration
degraded-insufficient-peers
failed
```

Health and readiness are distinct: an empty/insufficient NetDB can be internally healthy but not ready for I2P integration.

## Work package 4 — activate reseed only through Plan 104 trust semantics

When reseed is enabled and the cache is below threshold:

```text
bounded source selection
 -> bounded HTTPS fetch or configured offline source
 -> SU3 verification
 -> bounded archive processing
 -> Plan 103 RouterInfo validation
 -> normal store insertion
 -> persistence
 -> readiness recomputation
```

No unsigned or plain-HTTP fallback and no unbounded retry loop.

A failed reseed must not delete an already useful cache. Exhaustion of the bounded source/deadline budget yields a typed degraded/failure outcome.

Keep an offline/test source path so daemon integration tests require no Internet access.

## Work package 5 — truthful local RouterInfo ownership

The daemon holds one current local `ValidatedRouterInfo` signed by its persistent identity.

Until a later transport-activation plan changes authority:

```text
NTCP2 addresses = none
SSU2 addresses  = none
floodfill cap   = false
transit claims  = absent
```

Do not create loopback/private/fake addresses. The local record must pass the same validator as remote RouterInfos.

Remote NetDB ingestion must never replace the daemon's locally owned RouterInfo even if a malicious record claims the same key.

## Work package 6 — persistence integration

New/replaced valid remote RouterInfos should request persistence after in-memory acceptance. Persistence failure affects durability/health but does not retroactively make cryptographically valid in-memory data invalid.

Do not rewrite the complete database at shutdown. The Plan 104 cache should already be incrementally durable.

## Work package 7 — runtime seam for Plan 105 actions

Create only the runtime-facing seam needed to execute Plan 105 later. It must distinguish:

```text
peer transport availability
exploratory reply-path availability
I2NP delivery submission/outcome
inbound NetDB message
deadline/cancellation
```

A peer transport link is not an exploratory reply path.

Before Milestone 5, a standard RouterInfo lookup must remain in a typed state equivalent to:

```text
blocked_exploratory_tunnel_unavailable
```

Do not insert dummy tunnel IDs or route DatabaseLookup directly over NTCP2 in production code.

## Work package 8 — service graph and shutdown

Plan 101 currently preserves a transport-neutral daemon lifecycle. Plan 106 may add meaningful NetDB/bootstrap ownership, but the graph must still contain no `ntcp2-transport`.

Requirements:

- daemon may remain live with a degraded bootstrap state;
- cache/reseed success may reach `ready-for-network-integration`;
- lack of tunnels is a capability limitation, not a restart loop;
- shutdown cancels in-progress reseed and pending NetDB work;
- all runtime-owned tasks are joined within bounded deadlines;
- no busy polling when a network path is unavailable.

## Work package 9 — privacy-safe observations

Expose only bounded aggregate state needed for development, such as:

```text
validated RouterInfo count
encoded-byte use
floodfill-advertising candidate count
cache accepted/rejected counts
reseed phase + typed outcome
active lookup count
exploratory NetDB path available/unavailable
```

Do not emit private identity material, complete peer inventories, RouterInfo bytes, or raw I2NP payloads in normal diagnostics.

## Work package 10 — daemon integration tests

Required tests:

1. safe defaults for omitted NetDB/reseed config;
2. invalid/excessive limits reject;
3. reseed enable does not enable NTCP2;
4. explicit NTCP2 enable remains rejected;
5. dry-run performs no cache mutation/network request;
6. empty cache + reseed disabled produces typed insufficient state;
7. valid cache above threshold becomes ready without reseed;
8. mixed corrupt/valid cache retains valid entries;
9. empty cache + deterministic signed test reseed reaches threshold;
10. reseed failure does not erase a useful cache;
11. local RouterInfo self-validates and has no NTCP2 address;
12. service graph contains no `ntcp2-transport`;
13. starting a lookup without an exploratory path does not produce a direct transport send;
14. shutdown during bootstrap/reseed/query work is bounded and clean.

Use local fixtures and deterministic time. No root, namespaces, Java I2P, i2pd, or public I2P connection is required.

A manual opt-in HTTPS reseed smoke is allowed if the environment permits it. Retain only aggregate verification/count outcomes. It is bootstrap evidence, not I2P protocol evidence.

## Work package 11 — Milestone 4A closure state

At Plan 106 completion record:

```text
routerinfo_validation             = implemented
local_netdb                       = implemented
persistent_routerinfo_cache       = implemented
su3_reseed_verification           = implemented
reseed_ingestion                  = implemented
netdb_query_state_machine         = implemented
routerinfo_publication_state      = implemented
netdb_daemon_integration          = implemented
live_routerinfo_lookup            = blocked-on-milestone5-exploratory-tunnels
live_publication_verification     = blocked-on-milestone5-and-qualified-transport
milestone4_full_exit              = pending-cross-milestone-checkpoint
normal_daemon_ntcp2               = disabled
```

Do not call Milestone 4 fully passed yet.

## Work package 12 — Milestone 5 handoff contract

After Plan 106, the next implementation planning target is Milestone 5 exploratory tunnels.

NetDB supplies:

```text
target RouterHash
selected floodfill peer
DatabaseLookup I2NP message
query deadline/cancellation
response ingestion API
```

The future tunnel layer supplies:

```text
outbound exploratory delivery
inbound reply gateway RouterHash + tunnel ID
I2NP delivery outcome
inbound decoded I2NP delivery back to NetDB
```

Tunnel code must not own RouterInfo validation, routing-key selection, or lookup retry policy.

## Deferred full Milestone 4 acceptance checkpoint

After Milestone 5 supplies exploratory inbound/outbound paths, return to Milestone 4 acceptance before claiming live NetDB support.

The checkpoint requires:

1. Plans 103-106 remain green;
2. exploratory tunnel pairs exist in controlled mixed-router testing;
3. at least one router transport is deliberately qualified for the required peer/tunnel traffic;
4. deferred Plan 079 intent is reconciled before normal NTCP2 activation/public I2P use;
5. any transport defect is handled by one narrow defect-driven correction, not another generic harness program.

Full external evidence then requires a real RouterInfo lookup through an outbound exploratory tunnel, a valid matching response through the inbound path, validated insertion/persistence, and local RouterInfo publication with independent verification.

A direct NTCP2 DatabaseLookup is not a substitute.

## Validation

Run at minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p i2pr-daemon
cargo test --locked -p i2pr-netdb
cargo test --locked -p i2pr-storage
cargo test --locked -p i2pr-runtime
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

No Plan 099 workflow dispatch or external router build is part of local Plan 106 closure.

## Non-goals

Plan 106 does not implement exploratory tunnels, correct/requalify NTCP2, add SSU2, perform live I2P peer dialing, advertise transport addresses, implement LeaseSet client behavior, floodfill service behavior, transit tunnels, SAM/I2CP, or long-duration public-network operation.

## Closure criteria

Plan 106 closes only when:

1. bounded NetDB/reseed config is integrated with side-effect-free validation;
2. daemon startup loads identity, builds truthful local RouterInfo, revalidates cache, and computes bootstrap readiness;
3. explicitly enabled reseed can populate the same validated store through the Plan 104 path;
4. cache/reseed failure is bounded and preserves valid existing state;
5. local RouterInfo advertises no unqualified transport;
6. NetDB/bootstrap ownership uses existing supervisor/lifecycle contracts;
7. service graph contains no NTCP2 transport and explicit NTCP2 enable remains rejected;
8. Plan 105 has a runtime-facing seam that reports exploratory-path absence honestly;
9. production code does not invent dummy paths or direct-link lookup shortcuts;
10. shutdown joins/cancels all owned bootstrap/reseed/NetDB work;
11. integration tests pass without privileged host features or external I2P routers;
12. documentation records Milestone 4 local/bootstrap implementation complete and full live acceptance deferred to the Milestone 5 tunnel dependency;
13. the next implementation action is Milestone 5 exploratory-tunnel planning, not further Milestone 3 harness work.

## Expected handoff state

```text
daemon                     = real supervised process
identity                   = persistent
local RouterInfo           = signed/validated, no false addresses
local NetDB                = bounded/validated
persistent cache           = active/revalidated
reseed trust/ingestion     = implemented
bootstrap readiness        = active
NetDB query/publication    = pure state machines integrated
exploratory NetDB path     = blocked on Milestone 5
normal NTCP2               = disabled/unenableable
next implementation        = Milestone 5 exploratory tunnels
```
