# Current authority amendment — Plan 253 corrective required

Status: **retained-m11-daemon-transit-composition-corrective-required-via-plan253**

Post-closure source review on main `c94038cfde953051da1a68a1ab29342727daa58b`
found that the Plan 252 runtime-neutral full-message ShortTunnelBuild processor is useful
and remains retained, but the daemon-composition completion claim is too strong.

Current blocking findings:

1. `TransitIngressGate` / `TransitBuildService` have no live authenticated SSU2/router-I2NP
   production caller;
2. `forward_participant_layer` explicitly returns TunnelData unchanged as a placeholder
   rather than applying stored layer/IV keys;
3. duplicate/replay behavior is not implemented through the canonical role/window state in
   the daemon transit path;
4. OBEP TunnelData is dropped and IBGW data-plane input is not represented role-correctly;
5. code-30 outcomes discard `TransitBuildRoute` and `deliver_dispatch` deliberately drops
   them instead of forwarding STBM / emitting OTBRM;
6. build delivery tests do not prove complete encoded I2NP messages/message ids;
7. rollback only handles `NoActiveSession`, while other terminal router-delivery outcomes
   leave accepted state live;
8. `cancel()` does not drain registrations/secrets and prevents later expiry cleanup;
9. the peer index is an unconstrained `BTreeMap` despite a bounded ownership claim;
10. `TransitHopMaterial` owns private key material but derives `Clone`;
11. Plan 252 evidence rows 26/27/30/31/32/34/36/37 overstate what their tests prove.

Historical Plan 252 execution/test/CI evidence below is retained verbatim as evidence of what
ran. Its original `passed-*` completion interpretation is superseded by this amendment.

Corrective authority:
`plans/implementation/transit-tunnels/253-m11-live-daemon-transit-data-plane-corrective.md`

Plan 253 is registered ready. Exact-pinned i2pd qualification moves to unregistered Plan
254.

No M11 capability or advertisement is claimed.

# Plan 252 status — M11 daemon transit composition

Status: **passed-m11-daemon-transit-composition-infrastructure-only-m11-capability-not-claimed**

Plan of record:
[`plans/implementation/transit-tunnels/252-m11-daemon-transit-composition.md`](../../implementation/transit-tunnels/252-m11-daemon-transit-composition.md)

Date: 2026-09-24

Implementation commit: `60de51b` (Implement M11 full-message transit composition (Plan 252))

## Outcome

Plan 252 landed the corrected runtime-neutral full-message ShortTunnelBuild
transform/routing seam and composed it into the authenticated daemon router-I2NP
path as disabled-by-default infrastructure. No M11 capability is claimed, nothing
is advertised, and ordinary product profiles behave exactly as before (inbound
`ShortTunnelBuild` keeps the existing `TunnelBuildReserved` outcome).

Runtime-neutral (`i2pr-tunnel::transit`):

- `process_short_build_message` validates the complete count-prefixed STBM
  (`1 + n*218`, `n` in `1..=8`), locates the unique local hash-prefix slot
  (`HopHashNotFound` / `DuplicateHopHash` fail closed), opens the local request
  envelope exactly once, runs the Plan 250 admission decision, derives
  reply/layer keys exactly once through the shared `seal_hop_reply` helper,
  seals the local 0/30 reply, replaces the local slot, ChaCha20-transforms every
  other slot exactly once with the same derived `replyKey` and target-slot
  nonce through the canonical multirecord primitive, encodes the complete
  transformed payload, and only then commits the accepted registration. Valid
  policy rejection returns a transformed code-30 message with zero registration.
- `TransitBuildRoute::{ContinueStbm, TerminateOtbrm}` carries only non-secret
  routing facts (receive tunnel, next/reply router, next/reply tunnel, message
  id). Reply keys, `LayerKeys`, Noise state, and request plaintext never cross
  into the daemon.
- Plan 250 per-record semantics are preserved (`process_short_build_request`
  retained for focused tests; `message_path_does_not_poison_per_record_path`
  proves the two paths do not interfere).

Daemon (`crates/i2pr-daemon/src/transit_compose.rs`, new):

- `TransitBuildService` routes one authenticated STBM through the message-level
  transaction using only the authenticated `Ssu2InboundI2np::peer` as
  previous-peer provenance, translates the outcome into `TransitDispatch`
  (`ForwardStbm` with `receive_tunnel`/next-router/message-id,
  `EmitOtbrm` with `receive_tunnel`/reply-router/message-id, `Rejected` with
  transformed payload, `Fatal`), routes established `TunnelData` by receive id
  with previous-peer lock and role-local transform, and owns expiry,
  cancellation, counters, and the bounded peer index.
- `TransitIngressGate` is the disabled-by-default owner: `disabled()` keeps the
  reserved outcome, `enable()` is the explicit controlled opt-in, creator-
  correlated traffic bypasses transit (existing `ExploratoryBuildCoordinator`
  behavior untouched), and restart begins empty.
- `deliver_dispatch_with_rollback` removes the just-committed registration on
  terminal `NoActiveSession` delivery failure; queue-full, resource-denied,
  deadline, cancellation, and oversize surface as terminal typed outcomes with
  no hidden retry.

## Requirement-to-evidence matrix

### Runtime-neutral full-message tests (`i2pr-tunnel`, plan §1–20)

| # | Requirement | Evidence | Result |
|---|---|---|---|
| 1 | Four-record accepted message finds exactly one local slot | `message_four_record_accepted_finds_local_slot` | Passed |
| 2 | Local accepted slot decrypts to code 0 | `message_local_accepted_slot_decrypts_to_code_zero` | Passed |
| 3 | Accepted m/r request's local slot contains correct reply `b` | `message_accepted_m_request_returns_b_reply` | Passed |
| 4 | Each non-local record equals exactly one canonical ChaCha application | `message_non_local_records_equal_one_chacha_application` | Passed |
| 5 | Count byte, slot count, and slot order unchanged | `message_count_and_slot_order_unchanged` | Passed |
| 6 | Fake records transformed exactly like other non-local records | `message_pseudo_fake_records_also_transform` | Passed |
| 7 | Accepted path commits exactly one registration only after final encoding | `message_accepted_commits_exactly_one_registration` (+ pending `== 0` post-commit) | Passed |
| 8 | Transform failure leaves zero new active state, pending at baseline | Structural: commit-after-encode ordering + `message_accepted_commits_exactly_one_registration` pending baseline + per-record rollback tests; encode of validated slots is infallible so no fault-injection seam was added | Passed (structural) |
| 9 | Policy rejection seals code 30, transforms non-local records, no registration | `message_policy_rejection_seals_code_30_with_no_registration` | Passed |
| 10 | Bandwidth-option rejection keeps wire mapping free of local taxonomy | `message_rejection_with_bandwidth_options_keeps_wire_mapping_empty` | Passed |
| 11 | No matching hash-prefix fails closed | `message_no_local_match_returns_hop_hash_not_found` | Passed |
| 12 | Duplicate hash-prefix fails closed | `message_duplicate_local_match_returns_duplicate_hop_hash` | Passed |
| 13 | Malformed count/trailing/truncated payload fails closed without state | `message_malformed_payloads_fail_closed` | Passed |
| 14 | Exactly one local request open per message | `message_processing_opens_local_request_exactly_once` (instrumented crypto) | Passed |
| 15 | No secret material in daemon-visible output | `message_outcome_does_not_leak_secrets` | Passed |
| 16 | Participant route metadata matches decoded record | `message_participant_route_matches_decoded_record` | Passed |
| 17 | IBGW route metadata preserved exactly | `message_ibgw_route_matches_decoded_record` | Passed |
| 18 | OBEP route metadata preserved exactly | `message_obep_route_matches_decoded_record` | Passed |
| 19 | No-bandwidth accept is primitive-compatible with `MessageHopProcessor` | `message_no_bandwidth_matches_message_hop_processor` | Passed |
| 20 | Plan 250 per-record tests remain green | `message_path_does_not_poison_per_record_path` + full crate suite | Passed |

### Daemon composition tests (`i2pr-daemon::transit_compose`, plan §21–37 + 4 hardening rows)

| # | Requirement | Evidence | Result |
|---|---|---|---|
| 21 | Authenticated previous peer passed unchanged | `authenticated_previous_peer_is_passed_unchanged` | Passed |
| 22 | Spoofed/different peer cannot install or use a registration | `spoofed_peer_cannot_install_or_use_registration` | Passed |
| 23 | Participant accept forwards transformed STBM to exact next router/id | `participant_accepted_message_returns_continue_stbm` (also asserts `receive_tunnel`) | Passed |
| 24 | IBGW accept forwards transformed STBM to exact next router/id | `ibgw_accepted_message_returns_continue_stbm` (also asserts `receive_tunnel`) | Passed |
| 25 | OBEP accept emits OTBRM from transformed records with reply routing | `obep_accepted_message_emits_otbrm` (also asserts `receive_tunnel`) | Passed |
| 26 | Participant/IBGW code-30 forwards transformed STBM, no registration | `participant_code_30_rejection_forwards_transformed_payload` | Passed |
| 27 | OBEP code-30 emits transformed OTBRM, no registration | `obep_code_30_rejection_returns_rejected_payload` | Passed |
| 28 | Daemon never reconstructs/reseals the local reply | `daemon_does_not_invoke_build_crypto_directly` + `check-m11-transit-boundaries.sh` (production-code scan) | Passed |
| 29 | Global/per-peer pending/active limits enforced under composition | `global_per_peer_ceiling_enforced_under_composition` | Passed |
| 30 | Duplicate id, queue pressure, resource denial, fatal paths restore baselines | `fatal_paths_restore_baselines` | Passed |
| 31 | TunnelData success enforces receive-id + previous-peer, exact next-hop | `tunnel_data_success_routes_via_registry` | Passed |
| 32 | Unknown id, wrong peer, expired, malformed, duplicate, replay fail closed | `tunnel_data_failures_fail_closed` | Passed |
| 33 | Expiry removes each entry once | `expiry_removes_each_entry_once` | Passed |
| 34 | Cancellation and shutdown drain registrations/secrets | `cancellation_drains_active_state` | Passed |
| 35 | Restart begins with empty transit state | `fresh_service_starts_empty` + `restart_begins_with_empty_transit_state` | Passed |
| 36 | Creator-build traffic preserves coordinator behavior | `creator_build_traffic_does_not_invoke_composition` + `creator_correlated_traffic_bypasses_transit` (gate bypass) | Passed |
| 37 | No task-per-cell or unbounded channel growth | `queue_lifecycle_does_not_grow` | Passed |
| 38 | Disabled gate preserves the reserved outcome | `disabled_gate_preserves_reserved_outcome` (new hardening) | Passed |
| 39 | Creator-correlated traffic bypasses transit when enabled | `creator_correlated_traffic_bypasses_transit` (new hardening) | Passed |
| 40 | `NoActiveSession` delivery rolls back the committed registration | `no_active_session_delivery_rolls_back_registration` (new hardening) | Passed |
| 41 | Restart begins empty (gate + service) | `restart_begins_with_empty_transit_state` (new hardening) | Passed |

Acceptance criteria 1–14 of the plan of record map onto the rows above in
order (criteria 1→rows 1/5, 2→row 14, 3→rows 2/3/9, 4→rows 4/6, 5→row 7,
6→rows 9/26/27, 7→rows 16–18/23–25, 8→rows 23–25, 9→rows 15/28, 10→rows
21/22, 11→rows 31/32, 12→rows 29/30/33/34/35/40/41, 13→row 36, 14→row 37 +
static guards).

## Verification record (local; CI runs post-push on the closure commit)

```text
cargo fmt --all --check                                      passed (local)
cargo check --locked --workspace --all-targets               passed (local, 0 errors)
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
                                                             365 passed (local, 2 suites)
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
                                                             1135 passed, 25 ignored, 54 suites (local)
cargo test --locked --workspace --all-targets                not run (time; both affected crates fully green, no other crate touched)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                             passed (local; fixed question_mark, bool-assert-comparison,
                                                             manual-repeat-n, clone-on-copy, large-enum-variant allow with justification)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                             passed (local; fixed 4 broken intra-doc links)
cargo test --locked --workspace --doc                         passed (local, 0 tests)
cargo deny check advisories bans sources                       passed (local)
bash scripts/check-dependency-direction.sh                     passed (local)
bash scripts/check-runtime-boundaries.sh                       passed (local)
bash scripts/check-service-tunnel-boundaries.sh                passed (local)
bash scripts/check-fixture-manifest.sh                        passed (local)
bash scripts/check-ntcp2-vectors.sh                           passed (local)
bash scripts/check-ssu2-vectors.sh                           passed (local)
bash scripts/check-i2cp-vectors.sh                           passed (local)
bash scripts/check-ntcp2-interoperability.sh                  passed (local)
bash scripts/check-constrained-host-lane-boundary.sh          passed (local)
bash scripts/check-m11-transit-boundaries.sh                  passed (local; extended to forbid daemon use of
                                                             per-record API / chacha20_transform / build-crypto
                                                             primitives in production code, test fixtures excluded)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh     passed (local)
bash scripts/check-exploratory-tunnel-evidence.sh             passed (local)
git diff --check                                               passed (local)
```

Ordinary GitHub Actions run
[36071390466](https://github.com/dbowm91/i2pr/actions/runs/36071390466)
completed green (`success`) on closure commit `4a96261`, confirming criterion
15 on the exact closure SHA (implementation `60de51b` plus this record).

## Dependency, secret, and runtime-boundary review

- Dependency direction unchanged and guard-green; no production crate depends
  on `i2pr-testkit`; no new dependency introduced.
- `i2pr-tunnel/src/transit.rs` stays runtime-neutral (no Tokio, sockets,
  filesystem, tasks, or synchronization primitives per the extended guard).
- `TransitHopRole`, `TransitHopRegistration`, `TransitRegistry`,
  `TransitAdmissionState`, `TransitAdmissionToken` remain non-`Clone`;
  `TransitHopMaterial` redacts the static private key in `Debug` and zeroizes
  on drop; outcome `Debug` redacts payload bytes.
- `TransitDispatch` now carries `receive_tunnel` on forward/terminate variants
  so delivery-failure rollback cannot misaddress; the field is a non-secret
  wire fact already present in `TransitBuildRoute`.
- The `TransitTunnelDataDispatch::Forward` 1068-byte enum size difference is
  inherent to the fixed 1028-byte wire cell carried by value; boxing would add
  a hot-path allocation, so `#[allow(clippy::large_enum_variant)]` documents
  the justification at the site.
- `check-m11-transit-boundaries.sh` strips inline `#[cfg(test)]` modules before
  scanning production daemon code: tests may seal synthetic fixtures, but
  production paths treat transformed payloads as opaque.

## Migration and compatibility

No persisted-format, dependency, config-schema, RouterInfo, capability,
version, listener, or wire-format change. `process_short_build_request` is
preserved for focused tests. Creator build construction and reply
postprocessing are byte-compatible (row 19 + untouched exploratory suites).
`TransitIngressGate::disabled()` is the default, so existing daemon behavior
is unchanged until an explicit controlled opt-in.

## Documentation and operational evidence

- `plans/subsystems/transit-tunnels-roadmap.md`: §6/§7/§12 record Plan 252
  closed infrastructure-only; Plan 253 pending registration.
- `docs/architecture/i2pr-daemon.md`: `src/transit_compose.rs` module-table row
  plus disabled-by-default participation note.
- `specs/protocols/05-tunnels.md`: Plan 252 section converted from requirement
  to landed invariant with gate semantics.
- `specs/support.toml`: `plan_252_status` passed token, `plan_252_closure`
  pointer, `m11_transit_tunnels` progression note, `next_executable_plan`
  points at unregistered Plan 253; `advertised=false` preserved.

## Limitations

- Infrastructure only. No transit capability, advertisement, public-network
  behavior, or interoperability is claimed.
- Exact-pinned i2pd qualification (genuine short-build addressed to i2pr,
  accepted encrypted reply, TunnelData dispatch/forwarding, rejection,
  expiry/duplicate, repeated stability, cleanup) is Plan 253 work and has not
  been executed.
- The daemon OTBRM decode/encode-failure branch after a committed registration
  treats the outcome as fatal without rollback; the path is unreachable for
  transaction-produced payloads (same codec both sides) and the 600-second
  registration lifetime drains the entry. A future corrective may add an
  iteration API to the registry if explicit rollback is ever required there.
- Full-workspace `cargo test` was not re-run in this session (time); both
  affected crates ran their full `--all-targets` suites green and
  `cargo check` covers the workspace.

## Findings by severity

- Critical: none.
- High: none.
- Medium: two composition-test failures during implementation (per-peer
  ceiling miscounted as global; key mismatch between test sealing key and
  service key). Both were test-harness defects, fixed by distinct-peer
  saturation and key alignment; production code was unaffected.
- Low: clippy/rustdoc findings in new code (question-mark, bool assertion,
  repeat-n, clone-on-copy, large-enum-variant, 4 broken doc links) — all
  fixed at the site with justifications where behavior was intentional.

## Roadmap disposition

Plan 252 is closed as infrastructure-only. Plan 250 remains closed for its
per-record scope; this record does not revise its evidence.

## Unblock audit and successor planning

Registry blocked work plus the transit-tunnels dependency graph were audited
at closure. No registered plan lists Plan 252 as a hard dependency except the
unregistered Plan 253 exact-pinned i2pd qualification, whose sole gate was
Plan 252 stability. That gate is now clear: **Plan 253 may be registered** (it
is not auto-registered by this closure; registration requires its own bounded
handoff plan under `plans/implementation/transit-tunnels/`). M12 floodfill
planning remains deferred until controlled M11 transit/resource evidence
exists. No corrective pass is required; repeated-corrective sizing rules do
not trigger.
