# Plan 435 — Qualified normal-daemon NTCP2 and SSU2 dual-transport lifecycle

Status: registered / **blocked on Plans 433 and 434**. New scoped activation successor to historical Plan 101; no earlier status token is rewritten.

## Objective and readiness

Make the normal i2pr daemon capable of **opt-in** NTCP2 listener/dialer operation, truthful published NTCP2 RouterAddress, and cooperative NTCP2+SSU2 peer selection/fallback without duplicate router ownership. The full two-family interop evidence requirements of specs/CONFORMANCE.md remain mandatory for broad transport capability claims. Plan 434 must close with bidirectional actual I2NP; Plan 433 must prove common NetDB/router pipeline with SSU2.

## Baseline and classification

Currently normalize_ntcp2 refuses enabled=true and i2pr-daemon never registers the NTCP2 transport service (Plan 101). Existing i2pr-runtime::Ntcp2RuntimeService and generic transport contracts must be reused. **Capability:** NTCP2 dial/listen + two transport families with fallback. **Infrastructure:** durable NTCP2 static keys and obfuscation IV ownership, session/link health, transport address publication, routing policy. **Invariant:** no key/log leakage, truthful address binding, exact-peer RouterInfo validation, public-facing activation fails closed until external proof. **Polish:** operator fallbacks and redacted metrics. Out: SSU1, public floodfill, new protocol dialect, new reference harness or default-on external sockets.

## Ordered work packages

1. Reconcile Plan 434 authenticated-link send/receive contract with live Plan 433 peer manager. Prove NTCP2/SSU2 compete through one shared link registry and generic transport delivery contract; no separate NetDB or tunnel implementation.
2. Introduce strict explicit operator NTCP2 profile in config, preserving disabled-by-default and the guarded controlled-lab profile. Public listen binds are config-validated and must not be reachable via accidental `[transport.ntcp2]` defaults. Record necessary operator address/NAT overrides, but never accept config as reachability evidence.
3. Load/persist NTCP2 static key and its address obfuscation IV with versioned atomic storage, ownership, rotation/backup semantics and restart revalidation. Build signed RouterInfo only from actual current transport key/address and per-profile reachability; include withdrawal on loss/failure/key changes.
4. Integrate inbound/outbound dial admission, peer selection preferring established authenticated links, protocol-family failure isolation, transport backoff and duplicate-link resolution. TCP partial I/O, stalled handshakes and task cancellation must be fully bounded; one transport's failure may not terminate the other.
5. Perform bidirectional NTCP2 with pinned stock i2pd **and** stock Java I2P, with genuine I2NP store/lookup and tunnel-building messages. Run an intentional SSU2 loss/recovery scenario proving NTCP2 fallback, and reverse fallback when TCP fails. No test-only direct transport owner shortcut.
6. Reconcile docs/config/RouterInfo support status only after evidence. Plan 101 no-activation safety guard must be **replaced by equivalent gate-aware enforcement**, not deleted or weakened; mutation-test that an unqualified bind/advertisement still fails.

## Resource, failure, cancellation, migration

Dual-transports charge shared session/queued-byte/global task budgets; separate protocol-family backoff and source-IP pre-auth caps. Shutdown drains sessions, retires transport address/claims, and joins tasks. Existing disabled configs remain accepted; migrate any old NTCP2 keys only with explicit validation and compatibility tests. Persisted records are never trusted blindly after upgrade.

## Tests / acceptance

Commands: cargo test --locked -p i2pr-transport-ntcp2 --all-targets; cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1; cargo test --locked -p i2pr-daemon -- --test-threads=1; bash scripts/check-ntcp2-vectors.sh; revised check-ntcp2-interoperability.sh and controlled i2pd+Java qualification scripts; plus full AGENTS.md floor. Require two-family real-session/authenticated I2NP in both roles, valid RouterInfo from both protocols, no advertised unreachable endpoints, repeated reconnects, resource-baseline restoration, no path loss on family failover. Keep negative no-activation controls in default config. If Java runner cannot establish a real stock-router instance or reference implementation rejects record/version, record a stop and precise successor; do not call one-family experimental proof a full pass. Closure plans/closure/ntcp2-transport/435-status.md must list exact SHAs, commands, deviations, matrices, and gating decision.
