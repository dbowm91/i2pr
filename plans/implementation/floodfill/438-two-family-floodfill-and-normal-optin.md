# Plan 438 — Two-family floodfill qualification and normal-router opt-in role

Status: registered / **blocked on Plan 437**. Roadmap: plans/subsystems/floodfill-roadmap.md and core-router-recovery-roadmap.md.

## Objective and readiness

Complete M12 from controlled one-family experimental floodfill to **two independent implementation families qualified and genuinely opt-in normal-daemon floodfill**, subject to truthful reachability, bandwidth class, health and resource ceilings. Plan 437 must close the Java candidate-selection/capacity issue. Existing Plan 303 i2pd matrix and Plan 306 controlled reachability are not full two-family or public qualification.

## Implementation evidence and classification

i2pr-netdb includes type 0/1/3/5/7 validated stores/lookup, provenance/namespace, replication, rollover, persistence/maintenance; type-5 support/consumer was closed on Proposal 170 line. i2pr-daemon floodfill adapter and role owner implement controlled activation/withdrawal, but normal `caps=f` advertisement is forbidden. **Capability:** bounded normal-router opt-in floodfill serving and replication with independent peers. **Infrastructure:** stable role health/eligibility, authenticated routing and persistence, dual-family acceptance lane. **Invariant:** no unauthorized record disclosure, invalid/unsolicited store amplification, stale routing key, or unmeasured public capability; per-record and global budgets. **Polish:** operator enable/disable documentation and redacted service metrics.

## Ordered work packages

1. Reconcile all floodfill statuses Plans 270–306, 302/303, Proposal 170 Plans 350/351, current type-5 ELS2 support and strict source locks. Freeze support matrix and two-family exact pins: stock i2pd 2.61.0 and stock Java I2P 2.13.0 (or explicitly registered same-family equivalent). Do not mix Java I2P and I2P+ as separate families.
2. Wire runtime-owned floodfill server into normal public transport ingress and NetDB routing with strict authentication/validated provenance; keep transport manager transport-neutral. Use the same normalized signer/router-info and role-health gate as Plan 430/431; set `caps=f` only on opt-in eligible confirmed service.
3. Prove all supported canonical DatabaseStore/DatabaseLookup/DatabaseSearchReply behavior including encryption/reply-tunnel policy and type-5 blinded key. Test daily/next-day routing-key replication with zero-token loop prevention, namespace separation, bounded persistence/restart and record revalidation.
4. Execute complete dual-family tests: stock i2pd and Java as independent clients, each publishing RouterInfo/LeaseSet2 (plus type-5 where qualified), querying miss/hit, verifying replies, replication, and withdrawal. Include retries on independently owned tunnels, not direct test-caller shortcut. Both directions and real sender roles required; no patched reference router.
5. Enable strictly opt-in `[floodfill]` normal daemon role with actual bandwidth/reachability/health gating and bounded priority relative to local application and transit. Test live loss of eligibility, cap withdrawal, in-flight drain, config restart, IPv4/IPv6 declarations only when qualified.
6. Add negative DoS/resource/persistence tests and mutation-tested no-false-advertisement assertions. Update `specs/support.toml` and router operator docs **only for the evidence that truly passed**; keep production anonymity claims unchanged.

## Failure, contention, restart and migration

All stores verified before serving/replicating. Quota leases charged before costly work, replies rate limited and no peer-chosen amplification, memory/IO budget shared with application/transit and dropped under congestion. Persisted records revalidated after restart. On role health degradation withdraw signed capability before accepting new work, retain ordinary router connectivity. Existing disabled configuration and controlled fixture profiles continue to work.

## Tests and closure gate

Commands: cargo test --locked -p i2pr-netdb --all-targets; cargo test --locked -p i2pr-netdb-persist --all-targets; cargo test --locked -p i2pr-daemon --test floodfill_normal_optin -- --test-threads=1; bash scripts/check-m12-floodfill-boundaries.sh; bash tests/integration/floodfill/run-i2pd.sh; bash tests/integration/floodfill/run-java-floodfill.sh; full AGENTS.md floor plus dual-family, negative and restart suites. Require TWO independent accepted stock-router evidence ledgers with real request/response, rotation, replica flow and withdrawal, no fabricated caps or skipped necessary tests. If blocked by reference selection, actual resources or transport topology, record stop with precise next action and keep `caps=f` disabled. Closure plans/closure/floodfill/438-status.md must show exact SHA/reference pins, capability tier and observed health transitions. Pass may promote normal opt-in floodfill but **not** production anonymity readiness.
