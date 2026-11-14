# Plan 485 — SAM session lifetime for sequential stream reuse: `passed`

**Plan:** `plans/implementation/core-router-recovery/485-sam-session-lifetime-for-sequential-stream-reuse.md` (registered `4fb9dfb0`, 2026-10-16). **Predecessor:** `plans/closure/core-router-recovery/478-status.md` (cause-3-live teardown rows, union-gauge correction). **Disposition:** passed — sequential sessions survive; no support/capability/advertisement change; 433/439 gates unchanged.

## Implementation commits (branch `work/plans-477-480-router-recovery`)

- `4fb9dfb0` — plan registration (doc + registry + roadmap rows, `registered-ready`).
- Implementation + this closure (this turn): control-socket tracking set + disposition hooks + conditional finish gate + sequential test + baselines verified unchanged.

## Requirement-to-evidence matrix

| Plan WP | Requirement | Evidence |
| --- | --- | --- |
| WP1 | Finished stream reaps session only with no live control socket | `sam/raw_stream.rs` finish gate gains `!control_socket_live` conjunct; `RawStreamFinished` variant retained and still fires for socket-less sessions |
| WP1 | Control-socket tracking (insert on SessionControl entry, release on departure/exit) | `SamServiceState.session_control_sockets` + `note/forget/live` helpers; `apply_disposition` hook (both call sites); post-loop release backstop |
| WP2 | Baselines reach zero via explicit paths, none weakened | `sam_stream_final_acceptance` 11/11 green UNCHANGED (same-socket sessions still reap: no control socket); no baseline file edited |
| WP3 | Idle retention bounded, analysis recorded | Below: `max_sessions` caps count; 60 s `command_timeout` caps control idleness; set holds ≤1 entry per live control socket (≤ `max_clients`); no new limiter needed |
| WP4 | Deterministic fail-before/pass-after sequential reuse + probes | `plan485_sequential_streams_reuse_live_session` passes; gate-removal probe fails it with session_count 0 (pre-fix signature); `note`-removal probe fails the tracking unit test; both reverted |
| WP5 | One exact-pin lane read in order | `zgcd_jsn`: forward echo completes with NO cause-3 row; reverse ACCEPT parks (ACCEPT-SYN rows fire); DATA table then applies on its merits |
| WP6 | Negative controls | Concurrent sibling streams green (unchanged baselines); duplicate CREATE still rejected by state machine (unchanged code path); control-close teardown green (lane-end cause-2-live-0 row + `sam_plan146_reference` smoke); 3.3 child exemption untouched in gate; shutdown baselines green in every acceptance test |

## Tests and guards run (local; CI truthfully not run this turn)

- `cargo test --locked -p i2pr-daemon --lib` — 794 passed, 3 ignored.
- `cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1` — 11 passed (343 s), incl. new sequential test + 6 baselines byte-identical.
- `cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` — 4 passed.
- `cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1` — 44 passed.
- `cargo test --locked -p i2pr-daemon --lib sam::` — 38 passed (tracking unit + cause codes + totals).
- `python3 -m unittest discover -s tests/integration/core-router-recovery -p 'test_*.py'` — 66 passed.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — 51 passed.
- `cargo clippy --locked -p i2pr-daemon --all-targets -- -D warnings` — clean. `cargo fmt --all --check` — clean.
- `python3 scripts/check-global-plan-number-uniqueness.py` — passed. `bash scripts/check-dependency-direction.sh` — ok. `bash scripts/check-runtime-boundaries.sh` — passed.
- Exact-pin lane `zgcd_jsn` (pinned i2pd 2.61.0, verified-fresh binary bin size 295861104, HEAD `4fb9dfb0`): builds 4/4, installs 4/4, owners 2/2; forward ESTABLISHED + echo; teardown rows `[{cause 2, driver 1, session 1}]` only (lane-end control close); ACCEPT-SYN rows `[{observed 2, accepted 0, rejected 2, backlog 0/0}]`. Snap: `target/interop/plan458-scratch-snaps/zgcd_jsn-485-sequential-survives-syn-rejected-run/` (local-only).
- NOT run: full AGENTS floor (`cargo doc`, deny, remaining evidence checkers), macOS CI, Java fleet. Labeled as not run; no claim depends on them.

## Migration / compatibility

Wire protocol unchanged (no SAM reply text altered except no new lines added at all — this plan adds zero log lines). Session lifecycle contract change: standalone sessions now persist across zero-attachment gaps until control close. Clients that relied on implicit post-stream session death (none observed; reference i2pd/i2p clients hold control sockets per SAM convention) would see sessions linger until close — bounded by existing `max_sessions` + 60 s command timeout. Same-socket (Plan 149) sessions still reap on last finish (no control socket remains). 3.3 child semantics byte-identical (exemption preserved).

## Security and contention

No new sockets, listeners, ports, or runtime. New state is one `HashSet` of session IDs behind a leaf mutex (insert/remove/contains only, never held across other locks — no lock-order change). Session IDs never log. Lock-poisoning paths fail closed (`unwrap_or(false)` / skipped insert). No secret, identity, or payload material in any new path.

## Documentation / operational

Plan doc states acceptance; this closure records outcomes. No operator action: no config keys added/changed; `command_timeout` default (60 s) intentionally untouched. Lane parsers unchanged (no new transitions). The `establish_pair` doc comment ("last close tears the session down") remains true for its same-socket sockets.

## Known limitations

- A green reverse leg is NOT demonstrated: on `zgcd_jsn` both stock SYNs were rejected at our manager (`syn_rejected=2`, backlog 0/0) and the 20 s ACCEPT expired. Rejection-cause split (validation vs signature vs listener vs backlog) is the next bounded question, owned by Plan 478's frontier — not a 485 defect.
- `98nph0lj` mid-wait session death cause remains unattributed from that run (pre-teardown-INFO); bounded hypothesis recorded (60 s command-timeout vs 60 s CONNECT-wait race) with `xeewsj_6` counterexample noted. Future reruns carry the INFO to settle it.

## Findings by severity

- High: none remaining in scope.
- Medium: reverse-direction SYNs rejected 2/2 at our streaming manager on the first post-fix lane — product-significant, but explicitly the next (478) question with a live instrument already in place (`stream_accept_syn_wait_expired` rows). Not a 485 regression (pre-fix the ACCEPT never parked at all).
- Low: none. (Duplicate session IDs are rejected at reservation time, before any teardown call, so no clobber path exists.)

## Unblock audit

No registered plan lists 485 as a hard or interface dependency (registered this turn). Plan 478's frontier now depends on 485's ownership fix being live — satisfied by this closure (lane `zgcd_jsn` proves forward-finish survival). No rows move to ready. No corrective successor required.

## Roadmap disposition

Closed (passed). Roadmap §7 row 485 → passed. Plan 478 stays active on the reverse DATA product gate.
