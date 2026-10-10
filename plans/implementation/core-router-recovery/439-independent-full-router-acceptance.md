# Plan 439 — Independent end-to-end ordinary and full-role router acceptance

Status: registered / **blocked on Plans 433, 435, 436 and 438**. Final acceptance/evidence plan, not a second implementation of transport, NetDB, transit or floodfill.

## Objective and dependencies

Prove i2pr is a fully *functional* general-purpose I2P router in an authorized environment, supporting both NTCP2 and SSU2, automatically reseeding, constructing/maintaining genuine multihop tunnels, serving ordinary SAM/I2CP applications, admitting optional transit and advertising/serving optional floodfill only with truthful measured eligibility. Gate any broader `production-ready`, anonymity or privacy claim on separate security review and public-network operating evidence, not this acceptance alone.

Prerequisites are executed closure evidence from 430–438 as expressed in the core router roadmap; Plan 435 must have full two-family NTCP2 interoperability, Plan 438 must qualify i2pd and Java floodfill with actual normal role eligibility. Check full current git tree and known unrelated open defects before execution.

## Classification, invariants, scope

**Capability:** complete clean-install-to-client-routing and optional network roles. **Infrastructure:** one integrated evidence matrix with reproducible fixed references, resource measurements, restart/cancellation drivers. **Invariant:** no control/readiness/capability promotion by fiat; no sensitive endpoint or destination linkage in telemetry; no dependence on manipulated reference/router source. **Polish:** concise installation/profile docs and operator troubleshooting with exact limitations. Out: new application UI, arbitrary optimizations, forced hardening of unrelated ELS2/managed-app parallel lines, active attacks against public I2P.

## Ordered work packages

1. Freeze exact source SHA and reference pins for stock i2pd and stock Java I2P, topology, reference installation commands and finite run budget. Reconcile with specs/CONFORMANCE.md, guardrails and all new closure statuses. No test may quietly patch references or fall back to synthetic local-only I2NP.
2. Install i2pr from scratch with empty identity/cache; explicitly consent to HTTPS reseed; validate TLS/SU3 chain, minimum reachable peers, durable RouterInfo and SSU2 transport address; establish independent router I2NP links.
3. Exercise genuine inbound/outbound 2–3-hop exploratory/destination tunnel builds and maintenance through independent routers; publish and resolve Standard LS2; perform bidirectional application Streaming bytes via SAM and I2CP, including actual remote service operation and graceful stream close. Check separate client destination keys and absence of router↔destination identity cross-linkage in published metadata.
4. Authenticate both NTCP2 and SSU2 in two-way independent-router roles and prove automatic transport fallback on controlled loss of UDP/TCP; verify NetDB and tunnels continue functioning after failover and subsequent peer churn. Include IPv4, known IPv6 qualification boundaries, and firewalled/introducer behavior only to the extent genuinely supported.
5. Enable bounded transit and floodfill under deliberate test authorization: valid hop forward and tunnel expiry, bounded floodfill store/lookup/replication, observed caps only when healthy and eligible, withdrawal when budget or reachability fails. Default disabled roles must not open side-effecting listeners.
6. Execute deterministic replay, corrupted packet, malformed RouterInfo, handshake-exhaustion, source spoof, tunnel timeout/rebuild, large flow and resource-overload tests **only in isolated authorized testnet**; assert memory/channel/task/socket baseline restoration and no high/critical unresolved vulnerabilities.
7. Exercise normal graceful shutdown, forced shutdown, corrupted persisted cache handling, two consecutive restarts, transient reseed source failure and sustained application/data-path recovery. Produce a reproducible trace/evidence manifest with per-row pass/stop and no secret logs.
8. Audit public-facing docs and `specs/support.toml` against closure authorities, check exact capability/version claims, and record what is still not supported. Provide concise operator configuration for node-only, transit and floodfill profiles.

## Failure, restart, contention / compatibility

Repeat only a predeclared finite number of attempts on one frozen SHA and record *all* attempts; environment failure cannot become protocol success. Congestion must preserve local-client work ahead of transit/floodfill and abandon lower-priority work with a typed reason. Router identity persists, transient sessions and tunnels do not; validated NetDB may reload with freshness/clock checks. Preserve existing configs with explicit opt-in migration, refuse unknown/new unsupported fields. A degraded profile must visibly retract claims and remain shut to new unsafe admissions.

## Verification and acceptance

Execute full `AGENTS.md` floor (fmt, check, workspace serial tests, clippy, required guard self-tests), controlled exact-pinned NTCP2, SSU2, M6/M10 router-side/streaming, M11 transit, M12 floodfill and cold-start HTTPS suites; capture complete commands and exit status, CI on exact head, negative controls and sanitized ledger. Acceptance is a 100%-accounted-for matrix of required roles, with all mandatory cells genuinely passing. Any missing Java/i2pd proof, fictitious cap, absent actual multihop path, or silent skip means blocked; create a scoped corrective, never mark closure. Closure record plans/closure/core-router-recovery/439-status.md reports exact SHAs, reference signatures, product/user operations proven, security limits and forward roadmap. Distinguish `operational-core-passed`, `full-dual-transport-and-roles-passed`, and `production-anonymity-reviewed` as separate flags; the last must remain false without independent audit.
