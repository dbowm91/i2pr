# Plan 433 — Live NetDB, exploratory tunnels, and client routing under one daemon

Status: registered / **blocked on Plans 431 and 432**. Baseline: closures of 430–432 on a fresh exact head. Roadmap: plans/subsystems/core-router-recovery-roadmap.md.

## Objective and readiness

Prove an ordinarily configured i2pr daemon can use qualified SSU2 and an online-bootstrapped peer database to build multihop exploratory and client tunnels and carry complete application payloads over SAM/I2CP without manual test-driver injection. Plans 431 and 432 must have qualified and documented the public-capable SSU2 and HTTPS SU3 acquisition contracts. Existing M4 NetDB, M6 local streaming, Plan 193 i2pd one-family mixed-router progression, Plan 318 normal-daemon single-owner group product, and Plan 360 serving daemon are foundations, not substitutes for this test.

## Current implementation and boundary

Review crates/i2pr-daemon/src/{router_i2np.rs,netdb_seam.rs,exploratory_build.rs,destination_streaming.rs,lib.rs}, crates/i2pr-netdb/src/{publication.rs,lookup_engine.rs}, and i2pr-tunnel/i2pr-client pools; source-file names must be rechecked against HEAD. Do not spawn a second SSU2 owner for an integration test or replace stock-router tunnel construction with local zero-hop. The production service graph must own one socket/transport manager, one NetDB owner, and bounded tunnel/client pools.

## Classification / invariants

**Capability:** RouterInfo and LeaseSet publication/lookup, true multihop routing and application exchange against an independent I2P router. **Infrastructure:** NetDB transport-delivery bridge, peer candidate refresh/selection, tunnel liveness, stream dispatcher, readiness stages. **Invariant:** no transport link alone counts as an exploratory reply path, no client destination secret in transport/NetDB logs, no public capability from a controlled profile, no unbounded outstanding builds/lookups or retries. **Polish:** operator progress counters and reasoned degraded state. Out: enabling unqualified NTCP2; public transit or floodfill advertisement; proxy/UI redesign; forced Java two-family full-router completion in this milestone.

## Ordered work packages

1. Execute one cold-start baseline (bootstrapped RouterInfos, authenticated SSU2 sessions, no fake listeners). Localize the first missing normal-product edge with typed event snapshots; name exact file/call path.
2. Compose peer discovery, candidate filtering, authenticated dial, RouterInfo renewal/publication, iterative DatabaseLookup and response routing over the **same** transport manager. Verify short-transport outer I2NP semantics (the M12 Plan 302 fix) apply to every network dispatch. Validate all remote database objects before they enter routing eligibility.
3. Connect live exploratory inbound/outbound pools with actual authenticated next hops; track build replies, peer expiration, path material, shared budgets, rebuild throttles, and restart invalidation. Readiness requires at least one real qualifying inbound and outbound path; peers in the cache alone are insufficient.
4. Compose owned destination tunnel pools, signed Standard LeaseSet2 publication/lookup, ECIES delivery and bidirectional Streaming. Bind an actual SAM 3.1/3.3 or I2CP client to the normal process (not a second test router) and send exact bytes through multi-hop, independently verifiable path endpoints.
5. Exercise peer loss, late reply, dropped/reordered data, reconnection and cold restart. Separate protocol semantic defects from harness environment failures; add bounded retry with backoff, fairness and deterministic cancellation cleanup.
6. Publish precise readiness state and operational instructions: ProcessServing, BootstrapReady, LinkReady, TunnelReady, ClientReady, Degraded. Never rewrite specs/support.toml to claim full interoperability from one family.

## Failure, concurrency, restart

No unbounded per-peer dial, outstanding lookup, session admission or tunnel builds. Inflight metadata retained only until deterministic expiry, cancel, or generation change. A vanished peer must not strand a session or cause a tight dial loop. On restart previous tunnel keys/build state are not silently usable; verified cache remains reusable.

## Compatibility, verification and acceptance

Existing SAM/I2CP loopback clients remain local and off by default. Existing user service identities preserved; do not tie long-lived destination identity to ephemeral transport session lifetime. Verify at least:
- normal-daemon fresh-cache i2pd one-family two-way end-to-end client exchange over **nonzero multihop paths**, with network I2NP traces identifying actual independent routers and actual route ownership;
- a second cold start using validated cached peers, and a mixed-router restart/recovery attempt;
- malicious or stale RouterInfo/LeaseSet and typed NetDB/tunnel rejection;
- idle and overloaded resource baselines and absence of parallel socket owners.

Commands: cargo fmt --all --check; cargo check --locked --workspace --all-targets; cargo test --locked -p i2pr-daemon -- --test-threads=1; cargo test --locked -p i2pr-tunnel --all-targets; cargo test --locked -p i2pr-client --all-targets; the registered exact-pinned M6/M10 i2pd external runner; and full AGENTS.md floor. Name exact resolved runner commands and evidence directory in closure.

Stop when the first valid mixed-router path is blocked by a reproducible protocol or architecture defect; register a **narrowly located corrective** rather than a series of ad-hoc observability plans. Preserve failed artifacts. Closure plans/closure/core-router-recovery/433-status.md carries commit SHA, actual hop count/provenance and bytes, required commands/CI, missing coverage and unblock dispositions for 435/436/437.
