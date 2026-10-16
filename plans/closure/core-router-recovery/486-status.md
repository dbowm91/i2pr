# Plan 486 — SAM inbound-SYN routing to the parked ACCEPT instance: `passed`

**Plan:** `plans/implementation/core-router-recovery/486-sam-inbound-syn-routing-to-parked-accept.md` (registered `241f198`, 2026-10-16). **Predecessor:** `plans/closure/core-router-recovery/478-status.md` (lane `m8v2xk5q` instance-mismatch chain: `syn_observed=2, rejected=2, no_listener=2`, backlog `0→0`, stock 10× SYN retry then terminate). **Disposition:** passed — parked-ACCEPT SYNs route to the receiver mirror; no support/capability/advertisement change; 433/439 gates unchanged.

## Implementation commits (branch `work/plans-477-480-router-recovery`)

- `241f198` — plan registration (doc + registry + roadmap rows, `registered-ready`).
- `d255a1f` — implementation: waiter query (api) + eviction (client) + seam routing + receipt split + parser + WP1/WP3/WP5 tests.
- Implementation + this closure (this turn): expiry-message extraction + key-agreement test; lane `q3w7m9z2`; registry/roadmap to passed.

## Requirement-to-evidence matrix

| Plan WP | Requirement | Evidence |
| --- | --- | --- |
| WP1 | Fail-before lane-shape test + forward guard | `parked_client_syn_reaches_mirror_backlog` (mirror backlog 1, accepted 1, canonical untouched); `unparked_client_syn_stays_canonical_no_listener` (locks the pre-fix diagnosis); `parked_client_synack_stays_canonical` (outbound CONNECT state still answers on canonical while parked); `server_profile_ignores_parked_flag`; `corrupt_envelope_fails_closed_to_profile_routing`; `classifier_matrix_matches_routing_contract` |
| WP2 | Waiter-scoped SYN routing; non-SYN keeps profile routing; lock order documented | `classify_inbound_streaming_target` (server→mirror; client→mirror iff parked AND SYN-shape via the manager's own peek rule; all decode failures→profile); `receive_dequeued_payload` extraction (drain loop only attributes); `attach_sam_stream_registry` composition edge with leaf-lock discipline documented at the setter and the wrapper |
| WP3 | Stale-backlog age eviction, bounded, counted | `max_listener_backlog_entry_age_ms` (default 30 s > 20 s waiter window; ceiling 300 s; zero/over refused); `accept(port, now_ms)` purges stale front entries; `evict_stale_backlog_entries` sweep + `backlog_evicted` counter; expiry-path sweep before the backlog snapshot; 5 eviction unit tests (stale evicted, live survive incl. exact-TTL boundary, stale-front skip, cross-port sweep + capacity freed, config bounds) |
| WP4 | Per-instance receipt split, parser requires both | `syn_ingress_per_instance` (mirror, canonical) replaces the summed 8-tuple; WARN + TIMEOUT carry 18 keys (`syn_*` mirror + `canon_syn_*` canonical + backlog pair); parser gates each instance independently and refuses the summed legacy shape; `accept_syn_wait_expiry_message` extraction + key-agreement unit test; 10 offline parser tests incl. the `m8v2xk5q` instance-miss shape |
| WP5 | Silent-drop invariant (negative) | `rejected_syns_queue_no_outbound_packets`: all five reject paths (decode/validation/signature/no-listener/backlog-full) fail closed with `drain_outbound()` empty |
| WP6 | Regression matrix + inversion probes reverted | Dispatch matrix (profile × parked × SYN/SYN-ACK/garbage) in WP1 tests; waiter lifecycle in api tests (claim/release across sessions, outbound/forward ignored); `destination_tunnel_unit` 44, `sam_stream_self_composed` 4, `sam_stream_final_acceptance` 11/11 green; probes: forced-canonical routing fails the parked test; dropped parser key fails 5 receipt tests with `KeyError`; both reverted (zero `PROBE` markers) |
| WP7 | One exact-pin lane read in order | `q3w7m9z2` below: forward echo 4/4; reverse ACCEPT establishes (pump started, zero expiries); DATA stall recorded for 478 on its merits — no second lane |

## Tests and guards run (local; CI truthfully not run this turn)

- `cargo test --locked -p i2pr-client --all-targets` — 289 passed (lib 137 incl. 6 eviction/silent-drop tests; trajectories incl. 7 accept-clock suites).
- `cargo test --locked -p i2pr-api --all-targets` — 278 passed (lib 259 incl. 2 waiter-query tests).
- `cargo test --locked -p i2pr-daemon --lib` — 801 passed (794 baseline + 6 seam tests + 1 expiry-message test), 3 ignored.
- `cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1` — 44 passed.
- `cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` — 4 passed.
- `cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1` — 11 passed.
- `python3 -m unittest discover -s tests/integration/core-router-recovery -p 'test_*.py'` — 71 passed.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — 51 passed.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — exit 0 (a doc-comment blank line introduced with the new config field was caught and removed before close). `cargo fmt --all --check` — clean. `cargo check --locked --workspace --all-targets` — clean. `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-client -p i2pr-api --no-deps` — clean.
- `python3 scripts/check-global-plan-number-uniqueness.py` — passed. `bash scripts/check-dependency-direction.sh` — ok. `bash scripts/check-runtime-boundaries.sh` — passed.
- Exact-pin lane `q3w7m9z2` (pinned i2pd 2.61.0, verified-fresh binary bin size 296222640 mtime 1792187135, HEAD `d255a1f`, dirty 0): forward `install_committed` 4/4, `response_matched` 4/4, `ssu2:4`, mesh quiet; reverse `stream_accept_syn_wait_expired` count **0** (was 1 with `no_listener=2` on `m8v2xk5q`), `accept_pump_started` 1 (21:51:03Z); driver leg `i2pd-to-i2pr` fails at `short stream read (0/2328)` after 90.3 s; stock log shows **no SYN retry-exhaustion** — the two `packet was not ACKed after 10 attempts, terminate` rows carry established-phase stream ids (DATA-phase stall); teardown `[{cause 2, driver 1, session 1}]`; data-phase close is the all-zero torn-down shape (`bridge_present=0`). Snap: `target/interop/plan458-scratch-snaps/q3w7m9z2-parked-accept-run/` (local-only).
- NOT run: full AGENTS floor (`cargo deny`, remaining evidence checkers), macOS CI, Java fleet. Labeled as not run; no claim depends on them.

## Migration / compatibility

Wire protocol unchanged (SAM reply vocabulary untouched; the TIMEOUT text gains `canon_syn_*` pairs — counts only, same redaction posture as the existing keys). Listener/backlog ceilings unchanged (`max_sessions`, `max_listener_backlog`, `max_pending_accepts_per_session` all intact). New behavior: while an ACCEPT waiter is parked, inbound SYNs for that destination backlog on the mirror instead of dying `no_listener` on canonical; entries older than the backlog-entry age are evicted rather than handed out. Clients that never park ACCEPT observe byte-identical routing (parked flag false → profile path). FORWARD routing is intentionally unchanged (forward attachments are never `pending_accepts` members) — SAM FORWARD on client-profile destinations still exhibits the pre-486 canonical trap; recorded as a known adjacent instance for a follow-up, not a regression (no forward path was modified except the uniform `accept()` age purge).

## Security and contention

No new sockets, listeners, ports, or runtime. The registry query runs without holding any manager or bridge lock and the registry lock stays a leaf (documented at both ends); routing decides per dequeued payload from already-authenticated bytes (peek decodes are pure and bounded; the adapter still performs its own full decode). Classification failures fail closed to profile routing. Eviction is bounded by the backlog ceilings and counted. The silent-drop negative test holds on all five reject paths. No secret, identity, or payload material in any new path.

## Documentation / operational

Plan doc states acceptance; this closure records outcomes. No operator action: no config keys added/changed (TTL default rides the balanced/interactive presets). Lane parsers updated in the same commit as the emission change (no window where the lane emits unparsable lines).

## Known limitations

- A green reverse DATA leg is NOT demonstrated: the handshake now completes (SYN accepted, SYN-ACK answered, pump started) but no DATA flows within the driver's 90 s window. The DATA stall is owned by Plan 478's frontier — not a 486 defect (pre-fix the ACCEPT never established at all).
- The 18-key expiry emission never fired on the lane (no timeout occurred): its daemon↔parser agreement rests on the key-agreement unit test + offline parser tests, not a live line. A future expiry line will confirm it in situ.
- `listener_backlog()` still counts stale entries until an `accept()`/sweep touches them; gauges read at expiry run after the sweep, so the receipt is unaffected.

## Findings by severity

- High: none remaining in scope.
- Medium: reverse DATA stall (stock DATA unacked ×10 then terminate) — product-significant, owned by 478 with the handshake now proven live.
- Low: SAM FORWARD on client-profile destinations retains the canonical-trap shape (deliberately out of scope; same one-predicate fix when a plan owns it).

## Unblock audit

No registered plan lists 486 as a hard or interface dependency (registered this turn). Plan 478's reverse-handshake blocker is satisfied by this closure (lane `q3w7m9z2` proves SYN→mirror→Established live); 478 stays active on the reverse DATA product gate. No rows move to ready. No corrective successor required.

## Roadmap disposition

Closed (passed). Roadmap §7 row 486 → passed. Plan 478 stays active on reverse DATA.
