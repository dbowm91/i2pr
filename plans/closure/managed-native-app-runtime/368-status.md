# Plan 368 closure — trusted AppManager bridge and manager protocol foundation

Status: **passed-trusted-appmanager-bridge-and-manager-protocol-foundation**.

Classification: **infrastructure**. This closure proves the architectural
contract: a future trusted application manager can project an already
authenticated principal into the router's private SAM/I2CP capability gateway
over an injected, non-discoverable transport, with an authority ceiling strictly
below Proposal 170. It ships **no** launcher, **no** `i2pr-appd`, **no** app
runtime, **no** sandbox, and **no** production caller. It must not be read as a
managed-application capability.

## Commits

Implementation and this closure record land together; see `git log --oneline`
for the implementation SHA recorded below at commit time.

## The hole this closes, and the four wrong answers

Plan 355 froze the router side of the capability boundary — but
`crates/i2pr-daemon/src/app_gateway.rs` is `pub(crate)` and, as of Plan 355, had
**no production runtime caller**. The router could decide whether a principal may
use SAM or I2CP, yet nothing outside the daemon could ask it.

Four obvious implementations are all wrong:

| Wrong answer | Why it fails |
| --- | --- |
| Expose the gateway on a loopback listener | Loopback is not a trust boundary. A managed application reaches it via any local proxy or helper, so a loopback credential is not scoped to the application it was granted to. |
| Hand the manager a Proposal 170 credential | Proposal 170 is router **administrator** authority. This converts a scoped, principal-bound capability into unrestricted router control — the exact failure ADR 0032 exists to prevent. |
| Link router internals into the manager | Collapses the trust boundary and shares router secrets and address space. |
| Trust application-supplied bytes | An application's `hello`, requested capabilities, or manifest are attacker-controlled. If any can construct authorization, the Plan 355 capability boundary is decorative. |

ADR 0035 therefore fixes two things: the transport is an anonymous **inherited**
capability, and the authority ceiling is **strictly below Proposal 170**.

## What shipped

- **ADR 0035** — `docs/adr/0035-private-manager-protocol-and-inherited-authority.md`.
- **Normative reference** — `specs/references/managed-app-manager-protocol-v1.md`
  (language-neutral; no Rust representation is a wire ABI).
- **`i2pr-app-manager-proto`** — new runtime-neutral leaf crate. Only production
  dependency is `i2pr-app-proto`. Owns handshake, bounded frames, strict
  directional control vocabulary, opaque daemon-assigned handles, and pure
  bounded accounting. No transport, sockets, processes, filesystem, DNS, timers,
  or sandbox backend.
- **`crates/i2pr-daemon/src/app_manager_bridge.rs`** — the daemon-side consumer,
  implemented over an **injected** `AsyncRead + AsyncWrite` stream. Plan 369 owns
  the concrete anonymous inherited transport.
- **`scripts/check-managed-app-manager-boundary.py`** — 8 static rule groups.
- **`ChildScope::child_of`** — a production-named constructor in
  `i2pr-runtime::context`, because the bridge gives each gateway session its own
  cancellation domain and `ChildScope::for_test` was the only public constructor.

## Requirement-to-evidence matrix

| Plan 368 requirement | Evidence |
| --- | --- |
| Fixed handshake magic/version/role | `handshake_golden_bytes_are_frozen_and_version_independent`; `handshake_rejects_wrong_magic_version_role_length_and_reserved_bytes`; `wrong_manager_magic_and_version_are_rejected_before_any_session` |
| Strict directional control vocabulary | `control_vocabulary_is_strictly_directional`; `handshake_direction_guards_reject_the_wrong_role` |
| Unknown/duplicate field rejection | `unknown_and_duplicate_fields_are_rejected`; checker rule 4 |
| Bounded frame and control payloads | `frame_truncation_oversize_and_reserved_bytes_are_rejected`; `oversized_control_payloads_are_rejected_before_parsing`; `oversize_declared_frame_length_is_refused_before_allocation`; checker rule 7 |
| Exact request/reply correlation | `reply_correlation_is_exact_and_notifications_are_uncorrelated`; `health_and_shutdown_are_bounded_and_correlated` |
| Opaque daemon-assigned handles, never reused | `handles_reject_zero_and_malformed_values`; `cross_session_and_stale_handles_fail_deterministically`; checker rule 8 |
| Service handles scoped to one session | `cross_session_and_stale_handles_fail_deterministically` |
| Session create/close | `trusted_session_create_is_accepted_and_reserved_grants_are_refused`; `repeated_create_open_close_returns_counts_to_baseline` |
| SAM/I2CP open/close/reset | `sam_service_stream_forwards_exact_octets_through_the_gateway`; `i2cp_service_stream_carries_exact_protocol_bytes` |
| Exact ordered service data bytes | both service tests above; `one_backend_eof_does_not_close_a_sibling_stream` |
| Backend close/reset notification | `one_backend_eof_does_not_close_a_sibling_stream` (asserts `service_ended` names the closed stream and its session) |
| Bounded health and shutdown | `health_and_shutdown_are_bounded_and_correlated` |
| Unknown/duplicate/stale handle rejection is deterministic | `cross_session_and_stale_and_stale_handles` → `cross_session_and_stale_handles_fail_deterministically`; `scope_accounting_is_bounded_at_capacity_one_exact_and_max_plus_one` |
| Grants re-derived through the administrator path | `trusted_session_create_is_accepted_and_reserved_grants_are_refused`; checker rule 6 |
| `control_scoped` denied before side effects | `capability_denial_and_control_scoped_allocate_nothing`; `control_scoped_is_unrepresentable_and_typed_rejected`; checker rule 5 |
| No `hello`/requested-capability authority | `application_declarations_cannot_construct_manager_authorization`; checker rule 6 |
| Two sessions, identical app identifiers, isolated | `two_sessions_with_identical_app_identifiers_stay_isolated` |
| One backend EOF does not close a sibling | `one_backend_eof_does_not_close_a_sibling_stream` |
| Manager EOF tears down all gateway sessions | `manager_transport_eof_tears_down_every_gateway_session` |
| Bounded backpressure | `stream_admission_fails_closed_once_the_child_scope_is_exhausted`; `try_send`/`tokio::io::duplex` ceilings; checker rule 7 |
| max+1 sessions/streams/requests fail | `max_plus_one_session_admission_is_rejected_and_allocates_nothing`; `stream_admission_fails_closed_once_the_child_scope_is_exhausted`; `scope_accounting_is_bounded_at_capacity_one_exact_and_max_plus_one` |
| Repeated create/open/close returns to baseline | `repeated_create_open_close_returns_counts_to_baseline` |
| Deterministic no-panic fuzz smoke | `fuzz_smoke_never_panics_and_always_fails_closed` (20 000 rounds × 4 decoders) |

## Commands run and outcomes

All run locally on this host; **nothing was run on CI**. `cargo` was invoked
with `--offline` because the only lockfile change is the new workspace member
(no new external dependency), so no registry access was needed.

```text
cargo fmt --all --check                                        PASS
cargo check --locked --workspace --all-targets                 PASS
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                                PASS (0 warnings)
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                                PASS (0 failures)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                                PASS
cargo test --locked --workspace --doc                          PASS
bash scripts/check-dependency-direction.sh                     PASS
bash scripts/check-runtime-boundaries.sh                        PASS
bash scripts/check-console-boundaries.sh                        PASS
bash scripts/check-console-browser-security.sh                 PASS
bash scripts/check-config-secret-hygiene.sh                     PASS
bash scripts/check-service-tunnel-boundaries.sh                PASS
bash scripts/check-m11-transit-boundaries.sh                    PASS
bash scripts/check-fixture-manifest.sh                          PASS
python3 scripts/check-managed-app-gateway-boundary.py          PASS
python3 scripts/check-managed-app-manager-boundary.py          PASS
python3 scripts/check-managed-app-private-client-seams.py      PASS
python3 scripts/check-workflow-validity.py                     PASS
python3 scripts/check-global-plan-number-uniqueness.py         PASS
python3 scripts/check-adr-number-uniqueness.py                  PASS
python3 -m unittest discover -s tests/planning -p 'test_*.py' PASS
```

Targeted results:

```text
cargo test -p i2pr-app-manager-proto --all-targets             PASS (18 tests)
cargo test -p i2pr-daemon --lib app_manager_bridge -- --test-threads=1
                                                                PASS (16 tests)
```

Two floor items were **not** run and are **not** claimed: `cargo deny check
advisories bans sources` (no dependency was added, but the tool was not
exercised) and the full interop/evidence matrix, which is unchanged by this
plan. On macOS, `check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh` and `check-streaming-tunnel-evidence.sh` need bash 4+;
no fixture bytes were changed by this plan.

### Checker negative evidence

`check-managed-app-manager-boundary.py` was negative-tested with 12 independent
injections, each confirmed to fail closed:

1. grouped `use std::{fs, net, sync::Arc};` → rule 3
2. sibling-group evasion `use std::{sync::Arc}::{net};` → rule 3
3. aliased socket `use std::net::TcpStream as T;` → rule 3
4. loopback literal in the bridge → rule 3
5. admin message added to the control vocabulary → rule 4
6. `ControlScoped` added to the service enum → rule 5
7. application `hello` referenced as authorization input → rule 6
8. `RequestedCapability` in the production path → rule 6
9. `EffectiveCapabilities::from_requests` instead of `from_grants` → rule 6
10. local `BTreeSet<usize>` limit counter in the bridge → rule 7
11. runtime dependency added to the contract crate → rule 1
12. a second crate consuming the protocol → rule 1

The `use`-tree normaliser was itself found to be **fail-open** during this work
and fixed: the brace-depth scan missed sibling groups, so `use std::{fs}::{net};`
dropped the forbidden `std::net` leaf entirely. The normaliser is now validated
over 9 shapes including nested groups, sibling groups, `self`, aliases, and
commented-out imports. Identifier rules scan comment-stripped source, so the
bridge's own documentation may honestly name the constructs it forbids.

## Defects found by this plan's own verification

Three real defects were found during implementation. All are fixed and
regression-covered; none reached a release because nothing here is wired to a
production caller.

### D1 — `serde_json` cannot deserialize `u128`; `create_session` never decoded (medium, fixed)

`AppInstanceId` in the closed Plan 345 contract uses `#[serde(try_from = "u128")]`.
`serde_json` **cannot** read a `u128` back from a JSON number, so a
`create_session` payload carrying a real `AppPrincipal` was undecodable: every
session create failed `InvalidControl` and the bridge tore down the transport.

Fix: the manager protocol carries its own `ManagerPrincipal` with
`ManagerInstanceId` — bounded canonical decimal digits — and converts to
`AppPrincipal` only at the gateway binding. This leaves the closed Plan 345
contract and its golden vectors untouched, and decimal digits are the
language-neutral encoding a 128-bit opaque id should have on the wire.

Regression: `manager_principal_round_trips_including_the_widest_instance_id`
(1, 7, `u64::MAX`, `u128::MAX`) and
`manager_principal_rejects_non_canonical_and_oversize_instance_ids` (empty,
`0`, `007`, `-1`, `+1`, ` 1`, `1 `, `1.0`, `abc`, over-length, and a value one
past `u128::MAX` which must be refused rather than truncated).

### D2 — `deny_unknown_fields` does not reject duplicate JSON keys (medium, fixed)

`serde_json` keeps the **last** occurrence of a repeated member and decodes
successfully. So `{"request_id":1,"request_id":2}` and `{"request_id":2}` are
indistinguishable on the wire — a protocol ambiguity, not a cosmetic issue, and
one that matters because the daemon must correlate a reply to the manager's own
request id.

Fix: `reject_duplicate_object_keys`, a bounded, allocation-free JSON scanner
that fails closed on any repeated member name at any nesting depth and refuses
any shape it cannot classify. It runs before `serde_json`.

Regression: `unknown_and_duplicate_fields_are_rejected` asserts a top-level
duplicate, a **nested** duplicate, and that the unambiguous equivalent still
decodes (so the check is not simply rejecting the whole shape).

### D3 — the use-tree normaliser in the new checker was fail-open (medium, fixed)

Recorded above under checker negative evidence. Worth calling out because a
green checker that cannot fail is worse than no checker.

## Invariant review

- **Authority ceiling.** The vocabulary contains no package, grant, policy,
  configuration, or process operation, and no administrator variant. Checker rule
  4 enforces the vocabulary shape.
- **Application declarations cannot promote.** There is no decoder from
  `AppToHostMessage`, `RequestedCapability`, or manifest bytes into
  `AppGatewayAuthorization` or `EffectiveGrant`. Manager grants are re-derived
  through `GrantedCapability::from_administrator_policy` +
  `EffectiveCapabilities::from_grants`. Checker rule 6.
- **`control_scoped` unrepresentable.** Not merely denied: `ManagerService` has no
  such variant, so no decode path can name it; the spelling is still recognised
  and refused by type.
- **Capability check precedes allocation.** `guard.gateway.authorize(...)` runs
  before the duplex, the backend open, the permit, and both child tasks. A
  refusal returns without allocating.
- **One session ↔ one gateway session ↔ one `AppInstanceId`.** Enforced by
  construction in `create_session`.
- **Bounded everything.** Contract ceilings (32 sessions, 128 streams/session, 64
  in-flight requests, 16 KiB control, 64 KiB data, 128 connections) plus local
  bounds (64 queued outbound frames, 64 queued inbound chunks per stream, a
  64 KiB in-memory duplex per stream). Declared frame lengths are validated
  against the ceiling *before* allocation, and the header addition is checked.
- **Deterministic teardown.** Manager EOF and cancellation both clear inbound
  senders, cancel every per-stream root, and shut the gateway down.

## Failure, cancellation, and restart semantics

| Event | Behaviour | Evidence |
| --- | --- | --- |
| malformed/oversize/truncated frame | transport ends; no allocation from the bad length | `oversize_declared_frame_length_is_refused_before_allocation` |
| wrong handshake magic or major | transport ends before any session exists | `wrong_manager_magic_and_version_are_rejected_before_any_session` |
| unauthorised service open | typed `PermissionDenied`, side-effect free | `capability_denial_and_control_scoped_allocate_nothing` |
| stale / cross-session handle | typed `NotFound`, no teardown of the real owner | `cross_session_and_stale_handles_fail_deterministically` |
| one backend EOF | closes only that stream, emits `service_ended` for it | `one_backend_eof_does_not_close_a_sibling_stream` |
| manager transport EOF | every session and backend connection torn down | `manager_transport_eof_tears_down_every_gateway_session` |
| child scope exhausted | typed `ResourceLimit`, all side effects undone | `stream_admission_fails_closed_once_the_child_scope_is_exhausted` |
| bridge cancellation root | descendant state torn down | `manager_cancel_root_tears_down_descendant_state` |
| daemon restart | all manager sessions lost; no persistence (out of scope by design) | ADR 0035 §7 |

There is no unbounded queue and no retry loop anywhere in the bridge.

## Security review

- **No discoverable endpoint.** The bridge contains no socket, listener, connect,
  or loopback literal; the transport is injected. Checker rule 3.
- **Transport possession is manager-process authentication only.** It does not
  exempt the manager from message validation or the authority ceiling. ADR 0035 §2.
- **No security containment is claimed.** ADR 0035 §8: this boundary restricts
  no direct networking, filesystem, process tree, or grandchildren. The roadmap
  non-guarantee is explicit that blocking unauthorized direct egress would still
  not stop an application encoding identifying data into traffic it may send.
- **Handles leak nothing.** Opaque, non-zero, daemon-assigned, never reused; no
  Rust address, pointer, connection id, or router state crosses the wire.
- **Secrets.** No secret type gained a `Debug`/`Display`/serialization. No
  `DestinationIdentity: Clone` was added and no second private identity exists.

## Compatibility and migration

None. No SAM/I2CP wire version, no `specs/support.toml` entry, and no RouterInfo
advertisement changed. The manager protocol is private, unreleased infrastructure
and is explicitly **not** an external SDK or API compatibility promise.

`specs/support.toml` and `specs/CONFORMANCE.md` were reviewed and need no change:
Plan 368 introduces no externally observable protocol behaviour.

## Docs

Added: `docs/adr/0035-private-manager-protocol-and-inherited-authority.md`,
`specs/references/managed-app-manager-protocol-v1.md`,
`docs/architecture/i2pr-app-manager-proto.md`.
Updated: `docs/architecture/overview.md`, `dependency-graph.md`,
`i2pr-daemon.md` (new "Trusted AppManager bridge" section), `tooling.md`,
`AGENTS.md` floor, `.github/workflows/ci.yml`, `scripts/check-dependency-direction.sh`
(23 keys for 23 members), `scripts/check-runtime-boundaries.sh`.

## Limitations and non-claims

- **No production caller.** `app_manager_bridge` is consumed only by its own
  module tests until Plan 369. Its `#![allow(dead_code)]` is scoped to the
  module and states that reason.
- **The protocol ceiling is not the implementation ceiling.** The protocol
  allows 128 live service streams per session, but each stream costs two child
  tasks (one bidirectional pump plus the gateway's backend driver) and
  `i2pr-runtime::MAX_CHILD_TASKS` is 64, so the effective per-session limit is
  lower. Admission **fails closed** with a typed `ResourceLimit` and allocates
  nothing once either ceiling is reached, which is the tested behaviour. Closing
  the gap is either a per-stream task-count reduction (one multiplexed task per
  session) or a lower protocol constant; that needs a plan-of-record because it
  changes either a frozen protocol fact or the runtime's task ceiling.
- **Transport intent is asserted, not proven.** The contract crate cannot show
  that the eventual binding is anonymous. That is asserted at the Plan 369
  supervisor and frozen in ADR 0035.
- **No app-process authentication.** `hello` is declaration matching, not
  authentication (ADR 0035 §6). Any plan treating it as an auth factor is wrong.
- **No sandbox, no package/grant/launch/persistence authority, no I2PControl
  dispatch, no restart recovery.**

## Unblock audit

`plans/registry.md` lists **Plan 369** as the only blocked work depending on
Plan 368 (`crates/i2pr-app-manager-proto`, `AppManagerBridge`,
`AppGatewaySession`, `AppGatewayAuthorization`, `AppGatewayComposition`). It is
now moved to `ready` in the same commit as this record: its only hard dependency
is Plan 368, and the interface dependencies it needs — the manager protocol
contract and the daemon bridge API — are now frozen in an ADR, a normative
reference, an executable contract suite, and executable bridge tests. The
documented transport-implementation ceiling above is a known limitation of the
current implementation, not an unmet interface dependency.

No other registered plan lists Plan 368 as a dependency.

## Roadmap disposition

`plans/subsystems/managed-native-app-runtime-roadmap.md` §7: the Plan 368 row
becomes `passed-trusted-appmanager-bridge-and-manager-protocol-foundation`; the
Plan 369 row becomes `ready`. The managed-application runtime milestone remains
**open** — this closure delivers the router-facing contract only, and the
outbound-proxy/launcher/sandbox work is untouched.