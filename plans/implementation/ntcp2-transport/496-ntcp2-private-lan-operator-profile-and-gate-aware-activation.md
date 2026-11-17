# Plan 496 — NTCP2 guarded operator profile and gate-aware transport activation

| Field | Value |
| --- | --- |
| Plan | 496 |
| Subsystem | `ntcp2-transport` |
| State | `registered-source-substrate` |
| Classification | capability (source substrate) + invariant (gate enforcement) |
| Predecessor | Plan 435 (`plans/implementation/ntcp2-transport/435-ntcp2-daemon-activation-and-dual-transport.md`, registered `blocked` on Plans 433 and 434) |
| Precedent | Plan 453 (SSU2 guarded non-loopback owner and its `private_lan_qualification` profile), Plan 456 (controlled normal-daemon NTCP2 owner), Plan 461 (dual-transport normal-process I2NP continuity), Plan 475 (Java two-role authenticated NTCP2 product/reply), Plan 495 (external qualification gate) |
| Registry row | `plans/registry.md` |

## 1. Objective

Give the NTCP2 transport the same **guarded, operator-authorized, non-public**
qualification profile that Plan 453 gave SSU2 (`private_lan_qualification`), and
replace Plan 101's blanket "NTCP2 is loopback-only" activation refusal with a
**profile-aware gate that is at least as strict**.

One bounded outcome: an authorized two-host lab can bring up an `i2pr` router whose
NTCP2 transport listens on an operator-supplied RFC1918 IPv4 endpoint, signs that
endpoint into a locally-held dual-transport RouterInfo, and dials an operator-supplied
RFC1918 NTCP2 peer — with **no** NetDB publication, **no** reachability permit, **no**
capability advertisement, and **no** change to any existing floor.

This plan does **not** attempt Plan 435's acceptance. Plan 435 requires two-family
public evidence that needs an authorized external topology; Plan 496 delivers the
source-side activation substrate Plan 435 is waiting on and leaves Plan 435's
external evidence exactly where it is.

## 2. Why ready

- Plan 456 closed the controlled normal-daemon NTCP2 owner: persisted
  identity-bound transport keys and obfuscation IV, listener, initiator, and the
  common I2NP dispatcher.
- Plan 461 closed dual-transport normal-process I2NP continuity, including **both**
  single-family route-loss directions.
- Plan 475 closed the Java two-role authenticated NTCP2 product/reply qualification.
- Plan 453 closed the SSU2 guarded non-loopback owner: the `private_lan_qualification`
  profile, the RFC1918-only `Ssu2DialTarget::new_private_lan` constructor with typed
  refusals, and the socket-derived signed-RouterInfo owner. Plan 496 mirrors that
  precedent for NTCP2 rather than inventing a new shape.
- Plan 495 closed the external qualification gate that decides *who may be dialed*.
  Plan 496 does not weaken it.
- Plan 435 is registered `blocked` on Plans 433 and 434, both of which need an
  authorized external topology that does not exist in this environment. This is the
  same "source implementation boundary" split Plan 453 used against blocked Plan 431.

## 3. Current implementation evidence

- `crates/i2pr-daemon/src/config.rs`: `normalize_ntcp2` accepts only `disabled` and
  `controlled_loopback`; the controlled branch requires a loopback `bind_address` and
  `listen_port >= 1024`. `normalize_ssu2` already carries the `private_lan_qualification`
  branch with its RFC1918 IPv4 rule.
- `crates/i2pr-daemon/src/lib.rs`: the NTCP2 service closure hard-refuses any
  profile other than `ControlledLoopback` (Plan 101's blanket guard) and refuses a
  non-loopback `bind_address`.
- `crates/i2pr-runtime/src/ntcp2_runtime.rs`: `dial_authenticated_loopback_peer`
  filters peer addresses with a hard `!endpoint.ip().is_loopback()` clamp, documented
  as intentional for the controlled profile.
- `crates/i2pr-runtime/src/ssu2_runtime.rs`: `Ssu2DialTarget::new_private_lan` is the
  accepted precedent for a scope-scoped, RFC1918-only dial target with typed refusals
  (`ZeroPort`, `UnspecifiedIp`, `NotPrivateLanIpv4`, `PeerHashMismatch`).
- `crates/i2pr-daemon/src/router_i2np.rs`:
  `generate_controlled_dual_transport_router_info` builds one signed RouterInfo
  carrying both transports' addresses and rejects any endpoint that is not a matching
  fixed loopback pair; `generate_private_lan_identity_with_keys` is the SSU2-only
  private-LAN counterpart.

## 4. Invariants that must not regress

1. **Default-off.** With no configuration, NTCP2 stays disabled and loopback-only.
2. **No new configuration key may lower an existing floor.** The new profile adds a
   *narrower* allowance under a stricter conjunction; every existing refusal stays a
   refusal.
3. **No advertisement.** A private-LAN endpoint is never published to the NetDB, never
   carries `caps=f`, never yields a reachability permit, and never appears in an
   externally distributable RouterInfo.
4. **No floor weakening.** `specs/support.toml` and `Cargo.toml` stay byte-identical.
   `router.version`, the published RouterInfo capability set, and the support
   inventory do not change.
5. **Address scope is never widened silently.** The runtime dial clamp stays as narrow
   as the caller's profile permits.
6. **RFC1918 IPv4 only.** Loopback is not private-LAN, public addresses are not
   private-LAN, IPv6 is not private-LAN, `0.0.0.0` is not private-LAN, and port 0 is
   not a port.
7. **Fail closed.** Every unauthorized (profile × bind × port × family × SSU2 profile)
   combination is refused with a typed reason; none is skipped silently.

## 5. Scope

### 5.1 In scope

1. **`[transport.ntcp2].qualification_profile = "private_lan_qualification"`** — an
   explicit operator opt-in that requires: `enabled = true`; exactly one RFC1918 IPv4
   `bind_address` literal (not unspecified, not public, not IPv6); an explicit
   `listen_port >= 1024`; a network id that fits the handshake width; and an SSU2
   profile that is itself `private_lan_qualification` on an IPv4 endpoint, so the
   composed RouterInfo is family-coherent. This mirrors the existing
   `controlled_loopback` requirement that both transports agree.
2. **A profile-scoped NTCP2 dial admission in the runtime.** The hard loopback clamp
   in `dial_authenticated_loopback_peer` becomes scope-driven: `Loopback` keeps today's
   behavior exactly; `PrivateLan` accepts only RFC1918 IPv4 literals. Typed refusals.
3. **A socket-derived dual-transport private-LAN RouterInfo** in the daemon: the
   live bound NTCP2 endpoint and the configured SSU2 endpoint are signed into one
   RouterInfo that is held locally and exposed on the local inspection plane, under
   the same non-publication boundary the SSU2 private-LAN profile already has.
4. **Gate-aware activation enforcement.** The blanket refusal is replaced by a profile
   matrix that refuses every combination outside §5.1 with a typed reason. The
   replacement must be **at least as strict** as the blanket refusal for every
   configuration that the blanket refusal rejected.
5. **Negative controls** for every refusal reason, including inverted probes that
   prove each check can fail.
6. **Dual-family continuity evidence.** The two families do **not** share one link
   registry: SSU2 owns a private `i2pr_transport::TransportManager` and NTCP2 owns its
   own parallel admission structures (`InboundAdmission`, `ActiveLinkAdmission`,
   `DialAdmission`, `ReplayCache`). What they *do* share is one delivery registry
   (`Ntcp2RouterDeliveryRegistry`), one bandwidth governor, one authenticated I2NP
   dispatcher, and one local-RouterInfo channel. Plan 496 asserts that the operator
   profile preserves that sharing and that one family's failure neither terminates nor
   evicts the other, so the operator profile cannot silently regress Plan 461's
   dual-transport continuity. This plan does **not** attempt to unify the two link
   registries; that is a separate architectural change with its own plan-of-record.

### 5.2 Explicitly out of scope

- Any public/Internet-facing NTCP2 listener, advertisement, or reachability permit.
- Plan 435's two-family public evidence, Plan 433's live composition evidence, and
  Plan 434's external proof. All three stay blocked on the environment.
- Plan 493/494's two-family qualification and the `caps=f` advertisement.
- IPv6 support in the private-LAN profile (the RFC1918 IPv4 rule stays narrow).
- Any change to peer selection, scoring, NetDB storage, `specs/support.toml`, the
  published RouterInfo capability set, or `router.version`.
- Any new I2NP message, RouterInfo option, or wire change.

## 6. Required production changes

| Crate | Change |
| --- | --- |
| `i2pr-daemon` (`config.rs`) | New `Ntcp2QualificationProfile::PrivateLanQualification`; the `private_lan_qualification` normalization branch; typed refusals. |
| `i2pr-daemon` (`router_i2np.rs`) | A scope-aware dual-transport RouterInfo builder that accepts a private-LAN (SSU2, NTCP2) endpoint pair and keeps the loopback pair working. |
| `i2pr-daemon` (`lib.rs`) | The profile-aware activation gate replacing the blanket refusal; the private-LAN dual-transport registration path. |
| `i2pr-runtime` (`ntcp2_runtime.rs`) | `Ntcp2DialScope` and the scope-driven address admission inside the authenticated dial, with typed refusals. |

No new workspace dependency. `Cargo.toml` and `Cargo.lock` stay byte-identical.

## 7. Work packages

1. **WP1 — Config surface.** Add the `private_lan_qualification` NTCP2 profile and its
   normalization. Typed, specific refusals: profile not enabled, bind address not an
   RFC1918 IPv4 literal, bind address unspecified, bind address public, bind address
   IPv6, port below 1024, port zero, network id too wide, SSU2 profile not
   `private_lan_qualification`, SSU2 family mismatch.
2. **WP2 — Runtime dial scope.** Introduce `Ntcp2DialScope` with `Loopback` and
   `PrivateLan`; make the authenticated dial's address admission scope-driven; keep
   `dial_authenticated_loopback_peer` behaviorally identical under `Loopback`.
3. **WP3 — Daemon activation gate.** Replace the blanket refusal with the profile
   matrix; wire the private-LAN dual-transport RouterInfo; keep the non-publication
   boundary.
4. **WP4 — Tests.** Config negative controls (one per refusal reason, plus inverted
   probes); runtime scope tests (loopback clamp still rejects a private address under
   `Loopback`; `PrivateLan` rejects public/IPv6/unspecified); a daemon lifecycle test
   that brings the operator profile up on `127.0.0.1`-free RFC1918 semantics or, where
   the host cannot bind RFC1918, proves the refusal boundary deterministically.
5. **WP5 — Documentation.** `docs/architecture/i2pr-daemon.md`,
   `docs/architecture/i2pr-runtime.md`, `specs/CONFORMANCE.md`,
   `docs/operations/` (the operator procedure for the private-LAN profile), and the
   tooling inventory if a checker is added.
6. **WP6 — Closure.** The closure record, the registry row, the roadmap row, and the
   unblock audit.

## 8. Failure, cancellation, restart, and contention semantics

- **Failure.** A refused profile fails the NTCP2 service at startup with a typed
  reason. The daemon continues to run; NTCP2 is reported absent rather than silently
  degraded. The SSU2 transport is unaffected.
- **Cancellation.** The NTCP2 service's listener and child scope are already owned by
  the runtime `ChildScope`; the new profile adds no ownerless task and no new
  unbounded queue.
- **Restart.** The private-LAN endpoint is re-derived from the live socket on every
  start, exactly as the controlled profile does. The persisted identity-bound keys and
  obfuscation IV are unchanged by this plan, so a restart reuses the same transport
  identity and the signed RouterInfo remains reproducible.
- **Contention.** The two families keep **separate** link registries (SSU2 owns a
  private `i2pr_transport::TransportManager`; NTCP2 owns its own admission
  structures) and share one delivery registry, one bandwidth governor, and one I2NP
  dispatcher. An NTCP2 failure must not evict or disable the peer's SSU2 link, and
  vice versa. This is asserted, not assumed.

## 9. Compatibility and migration

- **No migration.** No stored format changes, no key rotation, no data-directory
  layout change.
- **Backward compatible.** Every configuration that is accepted today is accepted
  tomorrow with the same meaning. Every configuration rejected today is still
  rejected.
- **New configuration** is additive and default-off: one new enum value in an
  existing `deny_unknown_fields` struct.

## 10. Required tests

- Config unit tests: one per typed refusal, each with an inverted probe that proves
  the check fires (a mutation test, not a snapshot).
- Runtime unit tests: the dial scope admits exactly the addresses its profile allows
  and rejects the rest.
- Daemon integration test: the operator profile brings the NTCP2 owner up, the signed
  RouterInfo carries the socket-derived NTCP2 endpoint, and nothing is published.
- Negative controls: a public bind, an unspecified bind, an IPv6 bind, a privileged
  port, a missing SSU2 private-LAN profile, and a mismatched family are each refused.
- The routine floor in `AGENTS.md` passes in full.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-console-boundaries.sh
bash scripts/check-console-browser-security.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 scripts/check-license-metadata.py
python3 scripts/check-router-readiness-contract.py
python3 scripts/check-router-readiness-contract.py --self-test
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Focused, before the floor:

```text
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1
```

## 12. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the new profile, the activation gate, and the
  module rows for the changed files.
- `docs/architecture/i2pr-runtime.md` — the dial scope.
- `specs/CONFORMANCE.md` — the operator profile's exact claim: a guarded,
  non-advertised, RFC1918-only qualification path; no capability promotion.
- `docs/operations/` — the operator procedure, including the explicit statement that
  the profile is not a public activation and does not publish anything.
- `AGENTS.md` — only if the floor gains a step.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

- The operator profile brings the NTCP2 owner up on an operator-supplied RFC1918 IPv4
  endpoint with an explicit nonzero port, and signs the **socket-derived** endpoint
  into a locally-held RouterInfo.
- Nothing about that RouterInfo is published, advertised, or permitted.
- Every unauthorized combination is refused with a typed reason, and each refusal
  has a negative control.
- The runtime's loopback clamp is unchanged for every existing profile.
- The dual-family continuity assertion holds: one family's failure does not terminate
  or evict the other, and both families' deliveries route through the one shared
  delivery registry.
- The full routine floor passes.
- `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` is empty.

### 13.2 Stop conditions

- Stop and record a finding if any change would weaken an existing refusal.
- Stop and record a finding if the private-LAN endpoint can reach the NetDB
  publication path, a reachability permit, or a `caps=f` advertisement.
- Stop and record a finding if the operator profile can be activated without an
  explicit operator configuration.
- Stop and record a finding if the runtime's `Loopback` scope admits a non-loopback
  address.
- Stop and record a finding if the floor cannot be run to completion.

## 14. Closure evidence required

- Implementation commits.
- A requirement-to-evidence matrix mapping every §5.1 requirement to the exact test
  or command that proves it.
- Every command from §11 with its outcome, labelled local or CI truthfully.
- Invariant, failure, migration, and security reviews, including the non-publication
  review and the address-scope review.
- Documentation evidence.
- Known limitations, stated plainly: this is a **qualification** substrate on a
  private LAN, not a public transport activation.
- Unresolved findings by severity.
- The roadmap disposition and the **unblock audit**.

## 15. Handoff notes

- **Do not** modify `specs/support.toml`, `Cargo.toml`, or `Cargo.lock`.
- **Do not** weaken `scripts/check-*` to make a check pass.
- **Do not** promote the support inventory, the published RouterInfo, or
  `router.version`.
- The Plan 495 gate remains the authority for who may be dialed. The operator profile
  selects *addresses*, not *peers*.
- Plan 435's registration stays `blocked`. This plan does not move it; the unblock
  audit records that Plan 435's remaining gap is now purely the external two-family
  evidence.
