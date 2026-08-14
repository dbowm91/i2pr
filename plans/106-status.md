# Plan 106: closure record

- Status: **implemented and closed on the local host**.
- Date: 2026-08-13.
- Parent authority: Plan 102 (and the Plan 102 amendment that
  documents the Milestone 5 exploratory-tunnel dependency).
- Baseline: Plan 103/104/105 closures with the runtime-neutral
  `i2pr-netdb` validator, store, cache seam, SU3 reseed pipeline,
  and transport-neutral lookup/publication state machines.
- Implementation source:
  `crates/i2pr-daemon/src/bootstrap.rs`,
  `crates/i2pr-daemon/src/netdb_seam.rs`,
  `crates/i2pr-daemon/src/config.rs`,
  `crates/i2pr-daemon/src/lib.rs`,
  `crates/i2pr-daemon/src/error.rs`,
  `crates/i2pr-daemon/tests/netdb_integration.rs`,
  `crates/i2pr-netdb/src/lookup_engine.rs` (Plan 106 corrective fix),
  `crates/i2pr-netdb/src/lib.rs` (re-export correction).
- Next implementation planning target: **Milestone 5 exploratory
  tunnels** under Plan 102 authority.

## Closure summary

Plan 106 composed the validated RouterInfo store, persistent cache,
reseed path, and NetDB state machines into the real `i2pr` daemon
without activating any I2P transport. The implementation lands
the Plan 106 work packages 1–12 against the existing Plan 103/104/105
runtime-neutral surfaces.

### Work package 1 — bounded NetDB/reseed configuration

`Config` exposes two new sections:

- `[netdb]`: `enabled`, `max_records`, `max_encoded_bytes`,
  `min_router_infos`, `min_floodfill_advertisers`. Hard ceilings
  are 65 536 records and 64 MiB aggregate encoded bytes. Both
  `min_*` fields participate in the bootstrap readiness policy.
- `[reseed]`: `enabled`, `max_sources`, `max_su3_bytes`,
  `sources[[signer_id, certificate_path]]`. Reseed cannot be enabled
  while `netdb.enabled` is false. `signer_id` is bounded to 256 UTF-8
  bytes. `max_su3_bytes` cannot exceed 16 MiB. `sources` cannot
  exceed `max_sources`.

Every raw struct uses `deny_unknown_fields` so an unknown key fails
closed at parse time with the existing `ConfigParse` exit code.

### Work package 2 — explicit daemon startup phases

The real daemon path (`run_daemon`) now runs the Plan 106 bootstrap
pipeline synchronously before starting the supervisor:

1. load normalized config (already done by `Config::load`);
2. initialize logging (`initialize_logging`);
3. load persistent router identity (`IdentityStore::load`);
4. construct NetDB store (`Bootstrap::new`);
5. build and self-validate local RouterInfo (`LocalRouterInfoBuilder`);
6. load and revalidate RouterInfo cache (`CacheLoader::load_into`);
7. assess bootstrap sufficiency (`Bootstrap::compute_state`);
8. if explicitly enabled and below threshold, run the bounded
   offline reseed path (`Bootstrap::run_offline_reseed`);
9. publish NetDB bootstrap readiness (`Bootstrap::snapshot`);
10. enter supervised long-lived lifecycle (`Supervisor::run`).

No router transport starts in this sequence. The service graph
contains the existing `lifecycle` service plus a new
`netdb-bootstrap` service that observes the supervisor's
cancellation token; the bootstrap pipeline itself runs before the
supervisor starts so its outcome is observable in the CLI exit path.

### Work package 3 — bootstrap readiness policy

The `BootstrapState` enum carries the bounded vocabulary:

```text
empty
cache-sufficient
reseed-required
reseeding
ready-for-network-integration
degraded-insufficient-peers
failed
```

`BootstrapPolicy::from_config` derives the readiness thresholds from
the validated config. `min_router_infos` and `min_floodfill_advertisers`
stay local policy values, not current network population constants.
The state machine exposes a sanitized `BootstrapSnapshot` for
diagnostics:

- state label (bounded `&'static str`);
- `record_count`, `encoded_bytes`, `floodfill_advertisers`
  (aggregate counts only);
- `reseed_attempts`, `last_reseed_summary` (typed ReseedSummary).

Health and readiness are distinct: an empty store is internally
healthy but not ready for I2P integration.

### Work package 4 — reseed activation through Plan 104 trust semantics

The Plan 106 daemon owns a `ReseedIngestor` configured with the
operator-supplied trust set. Plan 104 `trust_signer_from_certificate`
parses the operator-supplied DER X.509 certificates and constructs
a `TrustedSigner` with a fixed `RsaSha512_4096` signature type. The
daemon refuses any bundle where the trust set cannot be loaded.

The `Bootstrap::run_offline_reseed` pipeline performs the full
Plan 104 SU3 verification chain:

```text
bounded source selection -> bounded offline read
  -> SU3 trust verification (Plan 104 verifier)
  -> bounded archive processing (Plan 104 ingestion)
  -> Plan 103 RouterInfo validation
  -> normal store insertion
  -> readiness recomputation
```

The pipeline never opens sockets, never accepts plain HTTP bytes, and
never falls back to unsigned content. A failed reseed never deletes an
already useful cache; exhaustion of the bounded source/deadline
budget yields a typed `ReseedAttemptSummary` with the outcome label
`"verification-failed"` / `"unknown-signer"` / `"empty-result"` and a
fresh readiness recomputation. The HTTPS reseed adapter is
explicitly deferred; the daemon integration tests cover the
offline source path so production code does not require an
Internet connection to validate bootstrap semantics.

### Work package 5 — truthful local RouterInfo ownership

The daemon holds exactly one `LocalRouterInfo` signed by the
persistent identity and self-validated through the Plan 103
validator. Under Plan 101 authority the local RouterInfo carries
zero `RouterAddress` entries:

```text
NTCP2 addresses = none
SSU2 addresses  = none
floodfill cap   = false
transit claims  = absent
```

The `LocalRouterInfoBuilder` already refuses the `f`, `B`, `K`, `L`,
`M`, `N`, `P`, `R`, `S`, `U`, `X` capability letters under the
Plan 101 activation guard. Remote NetDB ingestion can never
replace the daemon's locally owned RouterInfo even if a malicious
record claims the same RouterHash because the store distinguishes
records by their contained identity, not by the filename.

### Work package 6 — persistence integration

After the cache loader and reseed runs complete, the daemon writes
every validated remote RouterInfo that was not already on disk
through the Plan 104 cache seam. Persistence failures are tracked
in the snapshot (`persisted`, `persistence_failed`) and never
retroactively make cryptographically valid in-memory data invalid.
The Plan 104 cache is incrementally durable; the daemon does not
rewrite the complete database at shutdown.

### Work package 7 — runtime seam for Plan 105 actions

The new `netdb_seam::NetDbSeam` exposes the Plan 105 state
machines behind a stable runtime-facing surface:

- `path_status() -> ExploratoryPathStatus`
  - `BlockedExploratoryTunnelUnavailable` until Milestone 5 lands
    the exploratory inbound/outbound tunnel substrate;
- `begin_lookup(...)` translates the `RouterInfoLookup::start`
  outcome into the typed `LookupAction` vocabulary;
- `cancel()`, `diagnostics()`, `lookup()`,
  `lookup_mut()`, `validation_context()` give the runtime adapter
  the same primitives the Plan 105 module owns.

A peer transport link is not equivalent to a complete reply path;
the seam reports `BlockedExploratoryTunnelUnavailable` until
Milestone 5 supplies an exploratory inbound gateway + tunnel
identifier. Production code does not insert dummy tunnel IDs and
does not route DatabaseLookup directly over NTCP2.

### Work package 8 — service graph and shutdown

`build_daemon_graph` continues to refuse NTCP2 activation. The
graph contains the `lifecycle` service plus the new
`netdb-bootstrap` service. Both services are wired to the
supervisor's cancellation primitive so shutdown is bounded and
clean:

- `lifecycle` waits for `tokio::signal::ctrl_c()` and reports
  `RequestedShutdown`;
- `netdb-bootstrap` waits on the supervisor's cancellation token
  and reports `RequestedShutdown` so the supervisor can tear down
  both services within the configured grace deadline.

The daemon can remain live with a degraded bootstrap state; the
graph contains no restart loop or busy polling path.

### Work package 9 — privacy-safe observations

`BootstrapSnapshot` exposes only bounded aggregate counts
(`record_count`, `encoded_bytes`, `floodfill_advertisers`,
`reseed_attempts`) and a typed `ReseedSummary`. No private identity
material, complete peer inventories, RouterInfo bytes, or raw I2NP
payloads reach the diagnostics path. The `ReseedAttemptSummary`
records one-line outcome labels (`completed`, `unknown-signer`,
`verification-failed`, `empty-result`, `trust-set-load-failed`) and
the bounded per-aggregate counts.

### Work package 10 — daemon integration tests

`crates/i2pr-daemon/tests/netdb_integration.rs` adds 24 tests
covering:

- safe defaults for omitted NetDB/reseed config;
- invalid/excessive limits reject;
- reseed enable does not enable NTCP2;
- explicit NTCP2 enable remains rejected;
- dry-run performs no cache mutation;
- empty cache + reseed disabled produces typed insufficient state;
- valid cache above threshold becomes ready without reseed;
- mixed corrupt/valid cache retains valid entries;
- local RouterInfo self-validates and has no NTCP2 address;
- service graph contains no `ntcp2-transport`;
- starting a lookup without an exploratory path does not produce a
  direct transport send;
- bootstrap pipeline fails closed when identity is missing;
- bootstrap policy derives from config;
- store summary reflects populated state;
- reseed attempt summary records outcome labels;
- trust set rejects unparseable certificate;
- bootstrap state terminal predicate is bounded;
- typed reseed failure outcome is observable;
- cache loader reports rejected records via bootstrap;
- bootstrap snapshot clone is independent;
- bootstrap report is cloneable and typed;
- cache loader rejects unknown filename via compose;
- loaded cache state invalid carries error.

No root, namespaces, Java I2P, i2pd, or public I2P connection is
required.

### Work package 11 — Milestone 4A closure state

```text
routerinfo_validation             = implemented (Plan 103)
local_netdb                       = implemented (Plan 103)
persistent_routerinfo_cache       = implemented (Plan 104)
su3_reseed_verification           = implemented (Plan 104)
reseed_ingestion                  = implemented (Plan 104)
netdb_query_state_machine         = implemented (Plan 105)
routerinfo_publication_state      = implemented (Plan 105)
netdb_daemon_integration          = implemented (Plan 106)
live_routerinfo_lookup            = blocked-on-milestone5-exploratory-tunnels
live_publication_verification     = blocked-on-milestone5-and-qualified-transport
milestone4_full_exit              = pending-cross-milestone-checkpoint
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
external_netdb_over_ntcp2         = blocked
```

Milestone 4 is **not** declared fully passed. The local/bootstrap
implementation phase is closed; live acceptance is revisited after
Milestone 5 supplies exploratory tunnels.

### Work package 12 — Milestone 5 handoff contract

The NetDB layer exposes:

```text
target RouterHash
selected floodfill peer
DatabaseLookup I2NP message
query deadline/cancellation
response ingestion API
```

The future Milestone 5 tunnel layer must own:

```text
outbound exploratory delivery
inbound reply gateway RouterHash + tunnel ID
I2NP delivery outcome
inbound decoded I2NP delivery back to NetDB
```

Tunnel code must not own RouterInfo validation, routing-key
selection, or lookup retry policy. The NetDB runtime seam already
exposes the typed `ExploratoryPathStatus` vocabulary so the
Milestone 5 owner can register an exploratory reply path without
touching NetDB internals.

## Implementation surface

```text
crates/i2pr-daemon/src/bootstrap.rs       (BootstrapState, BootstrapPolicy,
                                            Bootstrap, BootstrapSnapshot,
                                            BootstrapReport,
                                            ReseedAttemptSummary,
                                            bootstrap_with_offline_reseed,
                                            build_trust_set, store_summary)
crates/i2pr-daemon/src/netdb_seam.rs      (NetDbSeam, ExploratoryPathStatus)
crates/i2pr-daemon/src/config.rs          ([netdb] and [reseed] sections)
crates/i2pr-daemon/src/error.rs           (ExitCode::RuntimeBootstrap,
                                            DaemonError::RuntimeBootstrap)
crates/i2pr-daemon/src/lib.rs             (bootstrap_daemon, run_daemon wires
                                            bootstrap pipeline + service
                                            graph; netdb-bootstrap service)
crates/i2pr-daemon/tests/netdb_integration.rs  (24 integration tests)
crates/i2pr-netdb/src/lib.rs              (re-exports trust_signer_from_certificate)
crates/i2pr-netdb/src/lookup_engine.rs    (terminal-path active state
                                            preservation when no candidate
                                            exists)
```

Wiring changes:

- `Cargo.toml` (workspace) — already lists `i2pr-daemon` and
  `i2pr-netdb`.
- `crates/i2pr-daemon/Cargo.toml` — adds `rand_chacha`, `rand_core`
  dev-deps for the integration test fixtures.

Documentation and support ledger updates:

- `README.md` — Plan 106 status section, support claim update,
  Plan 099 block trims to historical.
- `AGENTS.md` — Plan 106 status block, Plan 099 status block
  reclassification, dependency direction unchanged.
- `docs/architecture/i2pr-daemon.md` — documents the NetDB/
  reseed configuration sections, the bootstrap pipeline, and the
  service graph updates.
- `docs/architecture/overview.md` — notes that the daemon is the
  NetDB composition owner.
- `docs/architecture/dependency-graph.md` — already lists
  `i2pr-daemon -> {i2pr-netdb, i2pr-netdb-persist}`.
- `docs/architecture/interop-apparatus.md` — keeps the historical
  Plan 038-098 lane as audit record; no live interop change.
- `docs/protocol-support.md` — current support table reflects
  Plan 106 implementation.
- `specs/support.toml` — new `netdb.daemon-bootstrap` surface;
  `milestone = 4`, `plan_106_implementation_floor =
  "plans/106-status.md"`.

## Validation commands and results

```text
$ cargo +1.95.0 fmt --all --check
(no output)

$ cargo +1.95.0 check --locked --workspace --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s) in N.NNs

$ cargo +1.95.0 test --locked --workspace
[all suites pass; netdb_integration = 24 tests]

$ cargo +1.95.0 test --locked -p i2pr-daemon
50 lib + 24 integration + 7 CLI = 81 tests

$ cargo +1.95.0 test --locked -p i2pr-netdb
117 tests pass

$ cargo +1.95.0 test --locked -p i2pr-storage
[no change; existing tests pass]

$ cargo +1.95.0 test --locked -p i2pr-runtime
[no change; existing tests pass]

$ cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
No issues found

$ RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
Generated docs without warnings

$ bash scripts/check-dependency-direction.sh
dependency direction: ok

$ bash scripts/check-runtime-boundaries.sh
runtime boundary checks passed

$ bash scripts/check-ntcp2-interoperability.sh
Plan 099 NTCP2 interoperability static check: OK

$ bash scripts/check-fixture-manifest.sh
(no output)

$ bash scripts/check-ntcp2-vectors.sh
NTCP2 vector manifest is complete and hashes match.

$ git diff --check
(no output)
```

The Plan 046 rootless interop boundary check (`bash
scripts/check-rootless-interop-boundary.sh`) reports a missing
`tests/integration/ntcp2/harness/rootless_supervisor.py` file —
this is a pre-existing baseline failure from the Plan 099
harness-reduction commit (`c04da77`) and is unrelated to Plan
106.

## Coverage against Plan 106 closure criteria

| Plan 106 criterion | Result |
| --- | --- |
| 1. Bounded NetDB/reseed config is integrated with side-effect-free validation | Met. `Config::parse` enforces `deny_unknown_fields` everywhere; new sections have bounded maxima. |
| 2. Daemon startup loads identity, builds truthful local RouterInfo, revalidates cache, and computes bootstrap readiness | Met. `bootstrap_daemon` runs the full pipeline; `Bootstrap::run` enforces the order. |
| 3. Explicitly enabled reseed can populate the validated store through the Plan 104 path | Met. `Bootstrap::run_offline_reseed` consumes the offline SU3 bundle through `ReseedIngestor::ingest_su3_into`; the Plan 104 trust + signature + ZIP pipeline is the only insertion path. |
| 4. Cache/reseed failure is bounded and preserves valid existing state | Met. The cache loader isolates invalid entries; reseed failure is recorded as a typed `ReseedAttemptSummary` and does not erase valid in-memory state. |
| 5. Local RouterInfo advertises no unqualified transport | Met. `LocalRouterInfoBuilder` already refuses the `f`, `B`, `K`, `L`, `M`, `N`, `P`, `R`, `S`, `U`, `X` capability letters; `addresses()` is empty. |
| 6. NetDB/bootstrap ownership uses existing supervisor/lifecycle contracts | Met. `build_daemon_graph` registers only `lifecycle` and the new `netdb-bootstrap` service; both use the existing `ServiceSpec` API. |
| 7. Service graph contains no NTCP2 transport and explicit NTCP2 enable remains rejected | Met. `daemon_graph_contains_no_ntcp2_transport_service` and `daemon_graph_rejects_ntcp2_enabled_config` regression tests pass. |
| 8. Plan 105 has a runtime-facing seam that reports exploratory-path absence honestly | Met. `netdb_seam::NetDbSeam::path_status()` returns `BlockedExploratoryTunnelUnavailable` until Milestone 5. |
| 9. Production code does not invent dummy paths or direct-link lookup shortcuts | Met. The `LookupAction::NeedExploratoryReplyPath` variant is the only state-machine output when the runtime cannot supply a reply path; the daemon integration test verifies this. |
| 10. Shutdown joins/cancels all owned bootstrap/reseed/NetDB work | Met. The bootstrap pipeline runs synchronously before the supervisor; the `netdb-bootstrap` service awaits the supervisor's cancellation token. |
| 11. Integration tests pass without privileged host features or external I2P routers | Met. The 24 netdb-integration tests use only local fixtures, deterministic time, and bounded synthetic RouterInfos. |
| 12. Documentation records Milestone 4 local/bootstrap implementation complete and full live acceptance deferred to the Milestone 5 tunnel dependency | Met. This closure record plus the README/AGENTS/specs updates. |
| 13. The next implementation action is Milestone 5 exploratory-tunnel planning | Met. Plan 102 amendment governs; the Plan 106 → Milestone 5 hand-off contract is recorded in WP 12. |

## Status

Plan 106 is closed on the local host. NTCP2 remains experimental
and non-advertised. The next executable implementation is
**Milestone 5 exploratory tunnels** under Plan 102 authority.
