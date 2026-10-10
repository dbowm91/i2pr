# Plan 447 — Actual `i2pr run` SSU2→NetDB→tunnels→SAM/I2CP end-to-end corrective

Status: **registered / ready**. Date: 2026-10-10. Owner core-router-recovery. Corrective successor to Plan 443 (closure `plans/closure/core-router-recovery/443-status.md`), and independent controlled precursor to Plan 433. Plan 432 HTTPS SU3 local bootstrap and Plan 442 persistent SSU2 transport identity have passed their *local-component* evidence; Plan 440 evidence contract passed. Original Plan 431 public non-loopback qualification remains stopped.

## Objective / why ready

Plan 443 re-ran five exact-pinned stock-i2pd SSU2/M6 lanes, all passing authenticated bidirectional I2NP, NetDB, exploratory and destination tunnels, LeaseSet2, Garlic/ECIES, and bidirectional Streaming. But those lanes start daemon-owned components *inside a test process*, not a separately invoked production binary with actual SAM/I2CP clients. Implement a narrowly scoped **production-binary supervised smoke and then a controlled multirouter product path**, so we measure composition bugs rather than replay already-passed component tests. Emissary's single Router→TransportManager→NetDb/TunnelManager lifecycle is a behavior model, no code copied.

## Classification / invariants

**Capability:** controlled whole-process signed RouterInfo through I2P application routing. **Infrastructure:** isolated `i2pr run` launcher, one real daemon resource owner, configured SAM 3.1/I2CP external client, test-owned reference router peer topology, staged redacted readiness reports. **Invariant:** signed identity persisted, no second transport socket/NetDB owner, no public RouterInfo `R`/`f`/SSU2 advertisement from loopback, all normal SAM/I2CP/console off unless explicitly configured, no synthesized tunnel success, no fake loopback zero-hop acceptance, bounded owned children/sockets. **Polish:** operator startup example and failures by owner. Out: public SSU2 qualification, NTCP2, floodfill caps, building a duplicate app server, modifying stock peers.

## Bounded work packages

1. **Source-verified normal process:** Read `crates/i2pr-daemon/src/main.rs`, `config.rs::normalize_ssu2`, `lib.rs::run_daemon`/`register_ssu2_service`/`bootstrap_daemon` and actual SAM/I2CP service listener owner. Name exact stable CLI/config/TOML. Start compiled `i2pr run` as a separately supervised child with test-only explicit loopback bind, isolated identity/cache/datadir and ephemeral/local client listeners; health means real authenticated SSU2 session or client service readiness, **not** just "process alive."
2. **Gate A — daemon owner wiring (no stock multi-hop prerequisite):** Connect an actual SAM 3.1 or I2CP client to its listener and exercise real protocol negotiation/session/listener lifecycle. Inspect persisted Plan 442 SSU2 `s`/`i` bindings and one network-ID-correct RouterInfo; load Plan 432 validated seed/cached peers where needed. Prove startup, graceful shutdown, restart and default-disabled zero-socket for disabled listeners. **Gate A may run independently even if a separate stock-to-stock topology is ineligible**; this is a daemon composition result only, not interop qualification.
3. **Gate B — normal-process link and NetDB:** Start one or more pristine pinned i2pd 2.61.0 processes separately with independent identities, accepted controlled loopback addresses and a known-good reference bootstrap. Where Plan 440 requires testing a *stock candidate-selection claim*, validate a stock-to-stock control first; do not require an unrelated full stock-to-stock multihop network to run Gate A. Establish real signed/validated RouterInfo, normal-daemon SSU2 authentication in both directions, inbound/outbound I2NP and a genuine NetDB DatabaseLookup/DatabaseStore over the wire. No test-driver direct transport link or static router bypass.
4. **Gate C — multihop application:** Using enough healthy independent reference peers to make the actual hop count achievable, build nonzero-hop exploratory and destination pools with signed Standard LS2 publication/lookup. Drive an external SAM or I2CP request/response through the normal daemon and a real remote client/service; assert byte-exact bidirectional Streaming and graceful close, no local shortcut. Use reference-known-good bootstrap peers; do not tie Java's own exploratory startup to an ineligible i2pr floodfill. Prove recovery on loss/restart of one stock peer and of `i2pr run`, with bounded attempts and no stale tunnel reuse.
5. **Observability and posture:** Redacted per-owner stage snapshots and event counts, with session-local correlation in volatile memory; distinguish `daemon-serving`, `link-authenticated`, `netdb-live`, `tunnel-installed` and `application-bytes-complete`. Evidence tiers separately reflect Gate A, B and C. Public reachability and two-family normal-router conformance stay gated by Plans 431/433/435/439.
6. **Regression and docs:** Add negative configs for unmatched key/identity, local-address policy, wrong network ID, port collision, SAM session denial, expired RI/cache, post-shutdown orphan task and false ready projection. Preserve existing five M6 lanes as separate regressions; do not replace them with synthetic tests. Record actual config examples, cleanup script, CI environment skip behavior with `#[ignore]` explicit fail-on-missing-env policy and a controlled reference manifest.

## Failure / cancellation / restart / migration

Bound subprocess, timer, transport and pending-tunnel capacities with one cancellation owner; leaked listeners/tasks or dangling NetDB lookups are failing tests. No outside-network reseed unless opt-in/test fixture. Restart must preserve router/transport identities but expire stale sessions/tunnels. No default config migration, non-loopback bind or new unauthenticated local control exposure.

## Verification and closure

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon -- --test-threads=1
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
bash tests/integration/m6-interop/run-preflight.sh
bash tests/integration/m6-interop/run-netdb.sh
bash tests/integration/m6-interop/run-streaming.sh
python3 scripts/check-core-router-recovery-contract.py --self-test
# real i2pr-run subprocess + controlled pinned stock-reference runner,
# exact path/CLI registered during implementation; full AGENTS.md routine floor
```

**Pass requires** Gates A, B and C with independently observed authenticated I2NP, actual signed network DB and nonzero-hop application request/reply, restart/fault cleanup and explicit negative controls. Gate A or component lanes alone are partial evidence; if stock topology invalid, retain Gate A evidence but mark Plan 447 blocked at Gate B/C. Any passed controlled profile does **not** close Plan 433's external normal-router or Plan 431 non-loopback gate. `plans/closure/core-router-recovery/447-status.md` must state separate gate outcomes, exact revision/reference pins, actual commands, full checker results, security/migration caveats and Plan 433/436 readiness impact. No capability/support promotion on registration.
