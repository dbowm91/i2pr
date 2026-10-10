# Plan 432 — HTTPS SU3 reseed acquisition and unattended cold-start recovery

Status: **active**. Plan 430 passed at `fd6b41ead53fdfad86f230d105e0e2977f92830a`; exact-head bootstrap/trust source census is underway. Roadmap: `plans/subsystems/core-router-recovery-roadmap.md`.

## Objective and readiness

Enable safe opt-in **online HTTPS SU3 reseed** using i2pr's existing signed archive verifier, persistent RouterInfo cache, and NetDB bootstrap state machine. Cold start should no longer require a separately supplied local `i2pseeds.su3`. Existing `i2pr-netdb::reseed`, `i2pr-netdb-persist::ReseedIngestor`, and `crates/i2pr-daemon/src/bootstrap.rs` validate local material; `bootstrap_daemon` intentionally does not contact HTTPS. Plan 430 must define NetworkReady vs merely ProcessServing.

## Classification and invariants

**Capability:** configured consented HTTPS bootstrap/recovery from empty or stale NetDB. **Infrastructure:** bounded fetcher, independently pinned signer trust, source selection/failover, bootstrap integration and durability. **Invariant:** TLS server authentication **and** SU3 signer verification independently mandatory; never accept HTTP downgrade or unsigned peer bytes; normalize/validate `?netid=2`, network partition/proxy policy, decompression and entry count limits. **Polish:** actionable but redacted source/status errors. Out: reseed server, remote updates, arbitrary downloaded archives, changing SU3 crypto, blind retry loops, default-on external network disclosure.

## Ordered work packages

1. Freeze official reseed format and trust semantics (`https://www.i2p.net/en/docs/misc/reseed/` and `specs/protocols/04-reseed-netdb.md`). Ensure signer CN/ID/date and ZIP entry metadata validation are correct; recheck current signed-bundle provenance and `netid` semantics. Certificate trust must be explicit, versioned and reviewable (no insecure acceptance of arbitrary TLS or new signing key).
2. Introduce one transport-acquisition adapter below the daemon runtime/network owner; use an existing reviewed HTTP client if it satisfies strict TLS, redirects, response-byte limit, deadlines, no credentials and proxy policy. Do not copy an Emissary fetcher; inspect its behavior only. Restrict schemes, redirect targets, DNS policy, decompression bombs, oversized bodies, source count, retries and per-host timers. No cloud source list embedded without provenance.
3. Integrate with `Bootstrap::run` state transitions without holding an async mutex over network fetches. Cache revalidation and current count/floodfill threshold choose no reseed / reseed / degraded. Fetch one bounded batch from consented sources; verify SU3 **before** persisting RouterInfos. Each RouterInfo revalidated for identity/signature/network/freshness as currently implemented.
4. Define clean recovery after transient source timeout, invalid signer, stale batch and process restart. Atomic cache writes, single-flight reseed, bounded source failover/backoff; maintain previously valid peers when a new reseed fails. The process may serve local control in degraded state but **must not report network-ready** when peer/transport/tunnel prerequisites are absent.
5. Add HTTPS test fixture server with trusted test certificate and deterministic SU3 (generated independently/pinned origin). Positive valid source, invalid TLS, valid TLS+invalid SU3, wrong netid, stale/duplicate RouterInfo, truncated/oversized ZIP, redirect off-allowlist, cancellation, no-consent/no-network and restart tests. No actual public fetch in routine CI.

## Failure/cancellation/restart/contention

Per-fetch connect/read/total deadlines, bounded decoded bytes and decompressed aggregate, at most one in-flight reseed per identity, bounded retries with cool-down, cancellation releases socket and temp files. On failed verification no partial commit of untrusted RouterInfo. Source failure cannot escalate to a fictitious cache-sufficient state.

## Compatibility, verification, acceptance, stop

Old offline reseed path and disabled-by-default config remain valid. New online fields explicitly distinguish network consent and optional proxy; never log DNS/server identities as unbounded labels. Verify:
```sh
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-netdb-persist --all-targets
cargo test --locked -p i2pr-daemon --test netdb_integration -- --test-threads=1
cargo fmt --all --check
cargo check --locked --workspace --all-targets
# new isolated HTTPS positive/negative/restart integration suite + full AGENTS.md floor
```
Pass only if an **empty state directory** with an authorized local HTTPS fixture boots into a validated usable RouterInfo inventory without offline fixture injection, remains empty on failed trust, reuses cache on restart, and has no uncontrolled clearnet access when disabled. Do not claim actual peer connectivity from a successful download; Plan 433 owns that. Stop on absent vetted TLS backend or unclear signer trust migration; register a bounded corrective with evidence. Closure `plans/closure/core-router-recovery/432-status.md` gives commits, actual commands, trust model, tests not run and Plan 433 readiness.
