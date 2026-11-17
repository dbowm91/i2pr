# Plan 496 — NTCP2 guarded operator profile and gate-aware transport activation

## Status

`passed-ntcp2-private-lan-operator-profile-and-gate-aware-activation`

Plan 496 delivered the **source-side activation substrate** that Plan 435 is
waiting on: a guarded, operator-authorized, non-public NTCP2 private-LAN
qualification profile, and a profile-aware activation gate that replaces Plan
101's blanket loopback refusal with an equally strict per-reason refusal set.

Plan 435 is **not** closed by this record. Plan 435's acceptance includes
two-family public evidence that needs an authorized external topology this
host does not have. Plan 496's scope explicitly excluded that evidence, and
the unblock audit below records that Plan 435's remaining gap narrows to the
external evidence alone.

## Commits

**Implementation commits:**
- `eae7f657` — *docs(plans): register Plan 496, the NTCP2 guarded operator profile*;
- `a23f0734` — *feat(transport): land the Plan 496 NTCP2 operator profile and gate-aware activation*;
- `a5914f95` — *fix(boundary): scope the service-tunnel loopback-bind rule to the production config surface*.

The closure commit that adds this record is documentation-only, on the same
pattern Plan 492 and Plan 495 used.

## What changed

### `crates/i2pr-daemon/src/config.rs`

`[transport.ntcp2].qualification_profile` accepts a third named value,
`private_lan_qualification`, alongside `disabled` and `controlled_loopback`.
The private-LAN profile requires `enabled = true`; exactly one RFC1918 IPv4
`bind_address` literal; an explicit nonzero `listen_port` of at least 1024;
a network id representable by the handshake width; and **family coherence with
SSU2** — the `[ssu2]` surface must itself be in `private_lan_qualification`
mode and bind an RFC1918 IPv4 address, because one RouterInfo carries both
transports' endpoints.

Every refusal is a specific `ConfigError::Semantic` that names the offending
field and states the requirement. Loopback, the unspecified address, public
IPv4, TEST-NET and every IPv6 literal each get their own reason.

### `crates/i2pr-runtime/src/ntcp2_runtime.rs`

A new public `Ntcp2DialScope` enum with `Loopback` and `PrivateLan` and an
`admits(&self, ip: IpAddr) -> bool` method. `Loopback` reproduces the
historical clamp exactly. `PrivateLan` admits only RFC1918 IPv4 literals.
`dial_authenticated_loopback_peer` remains and now delegates to
`dial_authenticated_peer_in_scope(.., Ntcp2DialScope::Loopback, ..)`, so its
behaviour is unchanged; the address filter became
`if !scope.admits(endpoint.ip())`. `DialOutcome` gained `AddressOutOfScope`.
The scope is always the caller's and is never widened inside the runtime.

### `crates/i2pr-runtime/src/ssu2_runtime.rs`

`install_scoped_dual_router_info` is the scope-aware counterpart of
`install_controlled_dual_router_info`; the controlled one stays
loopback-only and delegates with `Ntcp2DialScope::Loopback`.

### `crates/i2pr-daemon/src/router_i2np.rs`

`generate_scoped_dual_transport_router_info` is the scope-parameterized
builder; `generate_controlled_dual_transport_router_info` (loopback) and
`generate_private_lan_dual_transport_router_info` (RFC1918) both delegate to
it. The two daemon handles gained `install_scoped_dual_router_info`.

### `crates/i2pr-daemon/src/ntcp2_activation.rs` (new)

The activation gate is a **pure** function,
`ntcp2_activation_scope(&Ntcp2Config) -> Result<Ntcp2DialScope, Ntcp2ActivationRefusal>`.
It is pure on purpose: the configuration layer already refuses every shape the
gate refuses, so an in-closure gate branch could never be reached from a
configuration file. Extracting the decision makes every branch reachable by a
unit test, which the in-closure form was not.

The five refusals are `Disabled`, `NoExplicitProfile`, `NonLoopbackBind`,
`NonPrivateLanBind` and `PrivilegedPort`. The gate is called from two places,
so there is one authority for the decision: the composition root in
`crates/i2pr-daemon/src/lib.rs` (which fails daemon startup) and the NTCP2
service closure (which degrades that service independently).

### `scripts/check-router-readiness-contract.py`

The checker gained the Plan 496 invariants — the private-LAN refusals, the
gate's five refusals, the two scope admission rules, and the new test names —
each with an inverted probe. The self-test's duplicated rule mapping was
**removed**: the live checks and the self-test mutations now read one shared
`source_controls()` function, so a rule can no longer drift out of the
self-test. That duplication is what let the moved runtime literal escape the
self-test when the scope change landed.

## Requirement-to-evidence matrix

| §5.1 requirement | Evidence |
| --- | --- |
| 5.1.1 the operator profile: default-off, RFC1918 IPv4 only, explicit nonzero port, network-id width, SSU2 family coherence | `crates/i2pr-daemon/src/config.rs` `normalize_ntcp2`; `crates/i2pr-daemon/tests/ntcp2_private_lan_profile.rs` (5 deterministic rows); `config.rs` unit tests `ntcp2_private_lan_profile_refuses_every_unauthorized_shape`, `ntcp2_private_lan_profile_refusals_are_not_vacuous`, `ntcp2_private_lan_profile_accepts_rfc1918_endpoint_with_matching_ssu2`, `ntcp2_private_lan_profile_never_falls_back_to_the_controlled_profile`, `ntcp2_defaults_stay_disabled_and_loopback_only` |
| 5.1.2 a profile-scoped NTCP2 dial admission | `crates/i2pr-runtime/src/ntcp2_runtime.rs` `Ntcp2DialScope`, `admits`, `dial_authenticated_peer_in_scope`, the delegating `dial_authenticated_loopback_peer`, `DialOutcome::AddressOutOfScope`; tests `dial_scope_admits_only_its_profile_addresses`, `dial_scope_admission_set_is_exactly_the_profile_set` |
| 5.1.3 a socket-derived dual-transport RouterInfo, held locally and never published | `crates/i2pr-daemon/src/router_i2np.rs` `generate_scoped_dual_transport_router_info` / `generate_private_lan_dual_transport_router_info`; `crates/i2pr-runtime/src/ssu2_runtime.rs` `install_scoped_dual_router_info`; `crates/i2pr-daemon/src/lib.rs` sends it only into the local `local_router_info` watch |
| 5.1.4 a profile-aware activation gate, at least as strict as the blanket refusal | `crates/i2pr-daemon/src/ntcp2_activation.rs` and its six unit tests; called from `crates/i2pr-daemon/src/lib.rs` at the composition root and in the NTCP2 service closure |
| 5.1.5 negative controls for every refusal | `ntcp2_activation.rs` `the_refusals_are_not_vacuous` and `the_gate_never_widens_a_scope`; `config.rs` `ntcp2_private_lan_profile_refusals_are_not_vacuous`; `ntcp2_runtime.rs` `dial_scope_admission_set_is_exactly_the_profile_set`; `scripts/check-router-readiness-contract.py` inverted probes for every new rule |
| 5.1.6 dual-family continuity | the two families share one delivery registry, one bandwidth governor, one authenticated I2NP dispatcher and one local-RouterInfo channel. **They do not share one link registry** — SSU2 owns a private `i2pr_transport::TransportManager`, NTCP2 owns its own admission structures. Recorded, not claimed away; unifying them is a separate architectural change with its own plan-of-record. |
| §4.1 default-off | `config.rs` `ntcp2_defaults_stay_disabled_and_loopback_only`; `omitted_ntcp2_section_means_disabled` |
| §4.2 no existing floor lowered | every configuration the Plan 101 guard rejected is still rejected: `the_controlled_profile_does_not_widen_to_a_private_lan_endpoint`; `config.rs` `ntcp2_controlled_loopback_requires_explicit_loopback_and_port` |
| §4.3 no advertisement | the profile adds no publication path; the NetDB publication path still requires an `Active` floodfill role **and** an in-memory `FloodfillAdvertisementPermit`; `[ssu2] advertise = true` is still refused at parse time (`the_private_lan_profile_never_enables_advertisement`); the environment-gated row asserts the child's output carries no `caps=f` |
| §4.4 no floor weakening | `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` is empty; `router.version`, the published RouterInfo capability set and the support inventory are unchanged |
| §4.5 the address scope is never widened | `the_gate_never_widens_a_scope`; `dial_scope_admission_set_is_exactly_the_profile_set` |
| §4.6 RFC1918 IPv4 only | `the_private_lan_profile_refuses_every_non_rfc1918_endpoint`; `ntcp2_private_lan_profile_refuses_every_unauthorized_shape` |
| §4.7 fail closed | every refusal is a typed error or a typed `DialOutcome`; none is skipped silently |

## Commands run

| Step | Outcome | Seconds |
| --- | --- | --- |
| `cargo-fmt` | PASS | 18 |
| `cargo-check` | PASS | 85 |
| `cargo-build-siblings` | PASS | 75 |
| `cargo-build-static-fixture` | PASS | 3 |
| `cargo-copy-static-fixture` | PASS | 1 |
| `cargo-rebuild-plain-fixture` | PASS | 0 |
| `cargo-test-workspace` | PASS | 2739 |
| `cargo-clippy` | PASS | 11 |
| `cargo-doc` | PASS | 8 |
| `cargo-test-doc` | PASS | 3 |
| `check-dependency-direction` | PASS | 1 |
| `check-global-plan-number-uniqueness` | PASS | 0 |
| `check-adr-number-uniqueness` | PASS | 0 |
| `check-portable-service-tunnel-api` | PASS | 0 |
| `check-portable-service-tunnel-consumer` | PASS | 6 |
| `planning-tests` | PASS | 7 |
| `check-runtime-boundaries` | PASS | 0 |
| `check-console-boundaries` | PASS | 2 |
| `check-console-browser-security` | PASS | 0 |
| `check-tooling-inventory` | PASS | 0 |
| `check-license-metadata` | PASS | 0 |
| `check-managed-app-private-client-seams` | PASS | 0 |
| `check-router-readiness-contract` | PASS | 0 |
| `check-router-readiness-contract-self` | PASS | 0 |
| `check-core-router-recovery-contract` | PASS | 0 |
| `check-core-router-recovery-contract-self` | PASS | 0 |
| `plan449-ssu2-preflight-self` | PASS | 0 |
| `plan495-peer-eligibility-self` | PASS | 0 |
| `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| `plan476-gate-binding` | PASS | 1 |
| `plan476-gate-binding-self` | PASS | 0 |
| `check-managed-app-package-boundary` | PASS | 0 |
| `check-managed-app-package-boundary-self` | PASS | 0 |
| `check-managed-app-policy-boundary` | PASS | 0 |
| `check-managed-app-policy-boundary-self` | PASS | 1 |
| `check-managed-app-secured-sandbox` | PASS | 0 |
| `check-managed-app-secured-sandbox-self` | PASS | 0 |
| `check-service-tunnel-boundaries` | PASS | 0 |
| `check-managed-app-gateway-boundary` | PASS | 0 |
| `check-managed-app-manager-boundary` | PASS | 0 |
| `check-m11-per-epoch-composition` | PASS | 0 |
| `check-service-anonymity-boundaries` | PASS | 0 |
| `check-fixture-manifest` | PASS | 0 |
| `check-ntcp2-vectors` | PASS | 0 |
| `check-ssu2-vectors` | PASS | 0 |
| `check-i2cp-vectors` | PASS | 0 |
| `check-ntcp2-interoperability` | PASS | 0 |
| `check-constrained-host-lane-boundary` | PASS | 0 |
| `check-m11-transit-boundaries` | PASS | 1 |
| `check-m11-transit-qualification-evidence` | PASS | 1 |
| `check-sam-acceptance-evidence` | PASS | 0 |
| `check-ssu2-acceptance-evidence` | PASS | 0 |
| `check-i2cp-acceptance-evidence` | PASS | 0 |
| `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| `check-els2-type11-transcript-boundary` | PASS | 0 |
| `check-els2-live-lane-evidence` | PASS | 2 |
| `check-encrypted-service-consumer-caller` | PASS | 0 |
| `check-outproxy-request-path` | PASS | 0 |
| `check-outproxy-wire-lane-evidence` | PASS | 0 |
| `check-config-secret-hygiene` | PASS | 1 |
| `check-workflow-validity` | PASS | 0 |
| `check-floodfill-type5-serve` | PASS | 0 |
| `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| `check-exploratory-tunnel-evidence` | PASS | 0 |
| `check-netdb-tunnel-evidence` | PASS | 0 |
| `check-destination-tunnel-evidence` | PASS | 0 |
| `check-streaming-tunnel-evidence` | PASS | 5 |
| `check-m6-mixed-router-acceptance-evidence` | PASS | 3 |
| `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| `check-m12-floodfill-boundaries-self` | PASS | 1 |
| `ntcp2-harness-tests` | PASS | 0 |
| `plan488-mesh-lane` | PASS | 0 |
| `plan488-evidence-deterministic` | PASS | 0 |
| `plan489-mesh-lane` | PASS | 0 |
| `plan489-evidence-deterministic` | PASS | 0 |
| `plan490-mesh-lane` | PASS | 1 |
| `plan490-evidence-deterministic` | PASS | 0 |
| `plan490-ssu2-transport-suite` | PASS | 6 |
| `plan490-transport-boundary-guard` | PASS | 0 |
| `plan491-mesh-lane` | PASS | 1 |
| `plan491-evidence-deterministic` | PASS | 0 |
| `plan491-boundary-guard` | PASS | 0 |
| `plan491-proto-suite` | PASS | 0 |
| `plan491-netdb-suite` | PASS | 88 |
| `plan491-client-suite` | PASS | 34 |
| `plan492-posture-suite` | PASS | 2 |
| `plan492-m11-lane` | PASS | 5 |
| `plan496-private-lan-profile` | PASS | 0 |
| `plan496-ntcp2-activation-gate` | PASS | 1 |
| `cargo-deny` | PASS | 2 |

**90 steps, 90 PASS, 0 FAIL.** Local run on this host; CI was not consulted for this record.

The floor ran on the implementation tree (`a5914f95`). The closure commit
that adds this record is documentation-only, and the document-level checkers
were re-run on the closing tree afterwards; the cargo results carry over
unchanged, on the same pattern Plan 492 and Plan 495 used.

The first floor attempt on this plan did **not** pass, and is recorded here
rather than discarded:

- its `cargo-fmt` step failed — a formatting slip introduced by an edit made
  after the initial format run;
- its `check-service-tunnel-boundaries` step failed — the rule's scope was
  wrong (see M2 below).

Both were corrected and the **entire floor was re-run from scratch** on the
closing tree. The table above is that second, complete run
(`/tmp/i2pr-floor-496-run2.log`, tree `a5914f95`).

Focused, before the floor:

```text
CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --test ntcp2_private_lan_profile -- --test-threads=1
# 5 passed; 0 failed; 1 ignored

CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --test ntcp2_private_lan_profile -- --ignored --exact private_lan_profile_binds_the_operator_endpoint_and_advertises_nothing --test-threads=1
# 1 passed; 0 failed  — the shipped `i2pr` binary bound the operator's real
# RFC1918 endpoint on both transports and shut down cleanly

python3 scripts/check-router-readiness-contract.py
# router-readiness contract checks passed
python3 scripts/check-router-readiness-contract.py --self-test
# router-readiness contract self-test passed

CARGO_BUILD_JOBS=4 cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# clean
```

## Invariant review

- **Default-off.** With no `[transport.ntcp2]` section the transport stays
  disabled and loopback-only. The new profile is a third *named* value, not a
  default, and it is refused without `enabled = true`.
- **No floor lowered.** The set of configurations the Plan 101 guard
  rejected is unchanged. The gate adds a third accepted shape and refuses
  every other shape with its own reason.
- **Scope is never widened.** The scope is derived from the profile at the
  gate and threaded explicitly to the runtime dial and to the RouterInfo
  install. The runtime never widens it.
- **One authority for the decision.** The composition root and the service
  closure call the same pure function, so they cannot disagree.

## Failure review

- A refused profile fails the composition root with the named reason. The
  daemon does not start. The configuration layer refuses the same shapes
  first, so the daemon-level gate is defence in depth and is exercised by
  unit tests rather than by a configuration file.
- A bind failure after the gate accepts degrades the NTCP2 service alone:
  the service is registered `StartupRequirement::Optional`, and the router
  keeps running with SSU2 unaffected.
- The environment-gated row failed closed on the first run: it caught a
  **second** Plan 101 blanket guard at the composition root
  (`crates/i2pr-daemon/src/lib.rs`) that the service-level replacement had
  missed. That guard was replaced with the same gate, and the row then
  passed. The row is not a formality — it found a real defect.

## Migration review

None. No stored format changed, no key rotated, no data-directory layout
changed, no configuration key removed or renamed. The new profile is additive
inside an existing `deny_unknown_fields` struct.

## Security review

- **No publication.** The profile adds no publication path. The NetDB
  publication path still requires an `Active` floodfill role and an in-memory
  advertisement permit; neither can be minted by this profile. `[ssu2]
  advertise = true` remains refused at parse time.
- **No reachability permit, no capability letter.** The signed record carries
  no `caps` option. The environment-gated row asserts the child's output
  contains no `caps=f`.
- **Address scope.** A peer address outside the profile's scope is refused by
  the runtime's scoped admission before any socket is opened. The controlled
  profile's clamp is unchanged.
- **Private-endpoint disclosure — stated plainly, per transport.** The
  private-LAN record carries the operator's RFC1918 endpoints. It is
  **never** published to the NetDB, but three mechanisms disclose it to
  peers on the private LAN:
  1. **When i2pr dials.** The NTCP2 initiator's SessionConfirmed carries
     i2pr's signed RouterInfo, so any peer i2pr dials learns the private
     endpoint from the record itself.
  2. **Over SSU2, when a peer dials i2pr.** The SSU2 responder's
     SessionCreated carries the responder's own RouterInfo. Because this
     profile composes **one** record for **both** transports, a peer that
     completes an SSU2 handshake with this router learns the whole record —
     the SSU2 address *and* the NTCP2 address. This is the widest of the
     three disclosures. It is a property of the SSU2 transport, not of the
     NTCP2 profile; the NTCP2 responder does **not** transmit a RouterInfo
     (its SessionCreated carries only options).
  3. **From the connection.** Any peer that opens a TCP connection to the
     NTCP2 listener sees the operator's private address as the connection's
     source address.
  The profile does not narrow any of the three. It narrows *who can reach
  the router at all*: RFC1918 addresses are not routable from the public
  network, so the audience for every disclosure is the private LAN. This is
  the same disclosure model Plan 453 accepted for the SSU2 private-LAN
  profile.
- **No secret material.** The profile reuses the existing identity-bound
  transport keys and obfuscation IV. Nothing new is persisted and no secret
  is logged or serialized.
- **Fail closed.** Every unauthorized shape is refused with a typed reason.

## Documentation and operations

- `docs/operations/ntcp2-private-lan-qualification.md` — the operator
  procedure, including the out-of-band handover of the signed record and the
  private-endpoint disclosure.
- `docs/architecture/i2pr-daemon.md` — the new profile, the gate module, and
  the profile matrix.
- `docs/architecture/i2pr-runtime.md` — the dial scope and the scoped
  install.
- `specs/CONFORMANCE.md` — the operator profile's exact claim, with its
  non-promotion stated.

## Limitations

1. **This is a qualification substrate on a private LAN, not a public
   transport activation.** Nothing is advertised and no capability is
   promoted.
2. **The environment-gated row needs a host with an RFC1918 address.** It is
   `#[ignore]`-gated; the ordinary floor compiles it and skips it. The
   explicit run **fails** when the host has no RFC1918 address rather than
   passing silently. The row passed locally on this host
   (`192.168.182.143`).
3. **The two families do not share one link registry.** SSU2 owns a private
   `i2pr_transport::TransportManager`; NTCP2 owns its own admission
   structures. They share the delivery registry, the bandwidth governor, the
   I2NP dispatcher and the local-RouterInfo channel. Unifying the link
   registries is a separate architectural change.
4. **The two families keep separate link registries even in this profile**,
   so "the peer is reachable by either family" is delivered through the
   shared delivery registry, not through one link table.
5. **The private-LAN profile composes one RouterInfo for both transports.**
   The family-coherence rule is what keeps that record coherent.
6. **No two-host live proof.** The external evidence Plan 435 needs remains
   unavailable here.

## Findings by severity

### High

- **H1 — a second Plan 101 blanket guard survived the first replacement.**
  The service-level gate was replaced but the composition-root guard
  (`crates/i2pr-daemon/src/lib.rs`) still refused any non-controlled-loopback
  profile, so the private-LAN profile could never start. **Found by the
  environment-gated row, which failed with the old message.** Fixed by
  replacing the guard with the same pure gate. Without the environment-gated
  row this would have shipped as a profile that parses but never runs.

### Medium

- **M1 — the readiness-contract checker duplicated its rule mapping** between
  the live checks and the self-test mutations. The Plan 496 runtime change
  moved a pinned literal, the live check failed, and the self-test would not
  have noticed the rule it duplicated had gone stale. Fixed by removing the
  duplication: both now read one shared `source_controls()` function.

- **M2 — `check-service-tunnel-boundaries` rule 5 scanned the whole config
  file, including the `#[cfg(test)]` module.** The rule's own words are "the
  production daemon config must not permit non-loopback client listeners or
  server TCP targets", but it grepped `crates/i2pr-daemon/src/config.rs`
  end to end, so a *test fixture* could fail a production boundary rule.
  Plan 496's private-LAN fixtures live in the test module and tripped it.
  The rule was **scoped to the production parsing surface** — everything
  before the column-zero `#[cfg(test)]` that opens the test module — which
  is a correction, not an exemption: the production surface contains no
  non-loopback `bind_address` literal at all, because the NTCP2 private-LAN
  bind address is a runtime-supplied value that `normalize_ntcp2` validates
  structurally. The indented inline `#[cfg(test)]` test helpers inside
  production code are deliberately **not** a cut point, so the whole
  production surface is still covered. Three controls make the rule
  non-vacuous and its scoping explicit, and the scoping was verified by
  injecting a real non-loopback literal into `RawSamConfig`: the checker
  failed closed on it.

### Low

- **L1 — the gate was initially inlined in the service closure**, where its
  refusal branches were unreachable because the configuration layer refuses
  the same shapes first. Fixed by extracting it into the pure
  `ntcp2_activation` module so every branch has a unit test.

No unresolved high or medium findings.

## Roadmap disposition

- `plans/subsystems/core-router-recovery-roadmap.md` — Plan 496's row moves
  from `registered (source substrate)` to closed.
- `plans/subsystems/ntcp2-transport-roadmap.md` — Plan 496's row is added and
  closed.
- `plans/registry.md` — Plan 496's row moves to recently closed with its
  status token; Plan 435's row is annotated to name what its source-side
  work packages now have and what remains.

## Unblock audit

Audited the registry's blocked work against Plan 496's closure.

| Plan | Depends on Plan 496? | Other hard dependencies | Outcome |
| --- | --- | --- | --- |
| 435 | yes — for its source-side work packages WP2 and WP6 | 433 and 434, both blocked on an authorized external topology | **Stays blocked.** Its remaining gap narrows from "WP2/WP6 source work plus external evidence" to the external evidence alone: WP3 (durable identity-bound keys) was closed by Plan 456, WP4 (dual-transport selection/fallback) by Plan 461, and WP2/WP6 by this plan. |
| 439 | no | 433, 435, 436, 438 | **Stays blocked.** |
| 493 | no — it depends on Plan 435's *acceptance*, not on this plan | the authorized two-host topology (Plans 431/472/476/480) | **Stays blocked.** Plan 496 does not move it: Plan 435's acceptance is still open. |
| 494 | no | 493 | **Stays blocked.** |

Nothing moved to `ready`. Every remaining blocker is either an external
environment or an unbuilt executor.

One registry annotation follows from this audit: Plan 435's row now says
which of its work packages are discharged, so a reader does not re-derive it.
No plan's state token changed.
