# Plan 453 status: passed — guarded private-LAN SSU2 owner (local scope)

Closure token: `passed-guarded-private-lan-ssu2-owner-local-scope`

Plan: [`453-ssu2-nonloopback-owner-without-external-claim.md`](../../implementation/core-router-recovery/453-ssu2-nonloopback-owner-without-external-claim.md)

## Outcome

The normal daemon now has an explicit `private_lan_qualification` SSU2 profile.
It accepts only an enabled RFC1918 IPv4 bind, no IPv6 bind, and an explicit
non-privileged port. Address publication and introducer service remain refused.
After the runtime binds, the daemon signs and installs a RouterInfo for the
socket-reported endpoint before readiness and before handing the owner to the
destination-group product. Persistent router and transport identities are
retained. The default profile remains `controlled_loopback`.

This closes Plan 453's source implementation and local-owner scope only. It
does not demonstrate an independent peer, authenticated I2NP, public
reachability, RouterInfo publication, `R`, or `f`. Plan 431 remains stopped at
its independent two-host qualification gate, and Plan 433 remains blocked.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Named fail-closed opt-in | `Ssu2QualificationProfile` and strict raw config parsing accept only `controlled_loopback` or `private_lan_qualification`. Private profile requires enabled SSU2, one RFC1918 IPv4 address, no IPv6, and a fixed port. Config tests cover wrong family, public/wildcard/nonlocal literals, missing port, and unauthorized profile. |
| No generic address-parser bypass | `parse_ssu2_bind` takes an explicit private-profile boolean; only RFC1918 IPv4 passes outside loopback. The socket bind remains the OS check for local address ownership. `check-router-readiness-contract.py` now mutation-checks the named-profile/RFC1918 guard, and its self-test passes. |
| Actual endpoint owns the local RouterInfo | `register_ssu2_service` waits for `Ssu2ServiceHandle`'s OS-reported local endpoint, signs the RouterInfo from that address and port using the persisted router and transport identities, then calls `install_local_router_info` before readiness. Runtime verification checks signature, freshness, local router identity, and equality with the actual bound socket. |
| Local private endpoint can be bound and recorded | Focused ignored test `router_i2np::tests::private_lan_profile_binds_only_the_inventory_endpoint` was explicitly invoked with an owner-assigned local RFC1918 address and passed. This was a local socket test with no peer traffic. |
| Controlled loopback behavior and vectors remain | Config, router_i2np, daemon restart, SSU2 preflight, runtime SSU2 local, and SSU2 vector tests passed; the existing loopback/default-off behavior remains covered. |
| Operator inventory, independent evidence, teardown | [`ssu2-private-lan-qualification.md`](../../../docs/operations/ssu2-private-lan-qualification.md) documents Plan 449 inventory use, the explicit profile, out-of-band provisioning only inside an authorized private lab, required Plan 431 evidence, and teardown/reset. |

## Commands and outcomes

Executed locally on Linux; no hosted-CI result is claimed.

```text
rtk cargo fmt --all --check
  passed
rtk cargo check --locked --workspace --all-targets
  passed
rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
  passed (required siblings current)
rtk cargo test --locked --workspace --all-targets -- --test-threads=1
  passed (4,780 passed, 39 ignored; 186 suites)
rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
  passed
RUSTDOCFLAGS="-D warnings" rtk cargo doc --locked --workspace --no-deps
  passed
rtk cargo test --locked --workspace --doc
  passed (29 suites)
rtk python3 scripts/check-router-readiness-contract.py
rtk python3 scripts/check-router-readiness-contract.py --self-test
  both passed after updating the stale blanket non-loopback assertion to enforce the named RFC1918 exception
rtk python3 scripts/check-core-router-recovery-contract.py
rtk python3 scripts/check-core-router-recovery-contract.py --self-test
rtk python3 scripts/plan449-ssu2-preflight.py --self-test
  all passed
rtk bash scripts/check-dependency-direction.sh
rtk python3 scripts/check-global-plan-number-uniqueness.py
rtk python3 scripts/check-adr-number-uniqueness.py
rtk python3 scripts/check-portable-service-tunnel-api.py
rtk bash scripts/check-portable-service-tunnel-consumer.sh
rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'
rtk bash scripts/check-runtime-boundaries.sh
rtk bash scripts/check-console-boundaries.sh
rtk bash scripts/check-console-browser-security.sh
rtk python3 scripts/check-tooling-inventory.py
rtk python3 scripts/check-license-metadata.py
rtk python3 scripts/check-managed-app-private-client-seams.py
rtk python3 scripts/check-managed-app-package-boundary.py
rtk python3 scripts/check-managed-app-package-boundary.py --self-test
rtk python3 scripts/check-managed-app-policy-boundary.py
rtk python3 scripts/check-managed-app-policy-boundary.py --self-test
rtk python3 scripts/check-managed-app-secured-sandbox.py
rtk python3 scripts/check-managed-app-secured-sandbox.py --self-test
rtk bash scripts/check-service-tunnel-boundaries.sh
rtk python3 scripts/check-managed-app-gateway-boundary.py
rtk python3 scripts/check-managed-app-manager-boundary.py
rtk bash scripts/check-m11-per-epoch-composition.sh
rtk bash scripts/check-service-anonymity-boundaries.sh
rtk bash scripts/check-fixture-manifest.sh
rtk bash scripts/check-ntcp2-vectors.sh
rtk bash scripts/check-ssu2-vectors.sh
rtk bash scripts/check-i2cp-vectors.sh
rtk bash scripts/check-ntcp2-interoperability.sh
rtk bash scripts/check-constrained-host-lane-boundary.sh
rtk bash scripts/check-m11-transit-boundaries.sh
rtk bash scripts/check-m11-transit-qualification-evidence.sh
rtk bash scripts/check-sam-acceptance-evidence.sh
rtk bash scripts/check-ssu2-acceptance-evidence.sh
rtk bash scripts/check-i2cp-acceptance-evidence.sh
rtk bash scripts/check-i2pcontrol-acceptance-evidence.sh
rtk bash scripts/check-els2-type11-transcript-boundary.sh
rtk bash scripts/check-els2-live-lane-evidence.sh
rtk bash scripts/check-encrypted-service-consumer-caller.sh
rtk bash scripts/check-outproxy-request-path.sh
rtk bash scripts/check-outproxy-wire-lane-evidence.sh
rtk bash scripts/check-config-secret-hygiene.sh
rtk python3 scripts/check-workflow-validity.py
rtk bash scripts/check-floodfill-type5-serve.sh
rtk bash scripts/check-service-tunnel-acceptance-evidence.sh
rtk bash scripts/check-exploratory-tunnel-evidence.sh
rtk bash scripts/check-netdb-tunnel-evidence.sh
rtk bash scripts/check-destination-tunnel-evidence.sh
rtk bash scripts/check-streaming-tunnel-evidence.sh
rtk bash scripts/check-m6-mixed-router-acceptance-evidence.sh
rtk bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
rtk bash scripts/check-m12-floodfill-boundaries.sh --self-test
rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
rtk cargo deny check advisories bans sources
  all passed; cargo deny emitted existing duplicate-version warnings for block-buffer and windows-sys
```

The first floor pass exposed the stale router-readiness assertion. The checker
was corrected to require both the explicit private profile and RFC1918 IPv4;
the focused check and its mutation self-test passed afterward. Remaining floor
commands then passed. Several negative-control evidence scripts print their
expected rejection diagnostics while their self-tests pass.

Implementation commits: `69af55c` (private-LAN owner) and `c757fe5`
(readiness guard and operator procedure). No new dependencies were added.

## Compatibility, security, and limitations

The new profile is opt-in and does not relax address policy for controlled
loopback, normal peer selection, public publication, or floodfill. It rejects
wildcard/public/CGNAT/IPv6/missing-port configurations. Binding a configured
address that is not owned by the host fails at the OS socket bind. Router and
transport signing keys remain in their existing persistent stores and the
signed endpoint uses those same keys; the test does not emit identity or key
material. The profile does not infer external reachability or mint a publication
permit. The local test was performed without any peer traffic.

Findings: critical — none; high — none; medium — independent two-host Plan 431
qualification remains open; low — no hosted CI run for this closure.

## Unblock audit and disposition

Plan 431 is not unblocked: it still requires independently addressed,
owner-authorized two-host authenticated SSU2 and bidirectional I2NP evidence.
Plan 433 and downstream external-router composition remain gated on Plan 431
and Plan 451. Plan 453's local source work did not satisfy those gates. Plans
451, 452, and 454 remain independently executable; no other blocked plan lists
the local Plan 453 implementation as its only hard dependency. Public support,
RouterInfo, `R`, `f`, and conformance claims are unchanged.
