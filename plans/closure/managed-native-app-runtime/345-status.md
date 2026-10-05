# Plan 345 closure — managed native app runtime foundation

Status: **passed-managed-native-app-runtime-contract-foundation**.

Classification: **invariant + infrastructure**. This closure establishes an
architecture boundary and runtime-neutral contract. It establishes no
user-visible application capability, process supervision, OS containment,
network broker, or anonymity/privacy property.

## Commits

- `3bcce31` — froze ADR 0032 and the language-neutral v1 contract before code.
- `f3c6604` — added `i2pr-app-proto`, boundary enforcement, fuzz target, and
  architecture/plan integration.
- `b6e0810` — made app-owned resource handles carry their principal.

The closure/registry/roadmap status transition is in the commit that adds this
record.

## Requirement-to-evidence matrix

| Acceptance | Evidence |
|---|---|
| 1. Durable trust/process/admin/UI boundary | [ADR 0032](../../../docs/adr/0032-managed-native-app-process-and-capability-boundary.md) decides separate processes, daemon/runtime ownership, scoped router capability access, brokered networking, untrusted UI, and the explicit non-anonymity guarantee. |
| 2. Contract frozen before implementation | [v1 reference](../../../specs/references/managed-native-app-runtime-v1.md) and ADR were committed as `3bcce31`, before crate implementation commit `f3c6604`. |
| 3. Leaf workspace crate | `i2pr-app-proto` is a workspace member with no production `i2pr-*` dependencies; the dependency allowlist enforces an empty set. |
| 4. No runtime/OS ownership | `scripts/check-runtime-boundaries.sh` scans source and dependencies for socket, process, filesystem, DNS, Tokio, dynamic-loader, and sandbox primitives; its positive control detected all forbidden categories. |
| 5. App/admin protocol separation | `AppMessage` and `AdminMessage` are separate strict tagged enums selected only after the one-role handshake. Tests reject admin mutation literals on the application surface, reject app decoding on an administrator handshake, and reject unknown fields/duplicate keys. |
| 6. Requested/granted/effective capabilities differ | Manifests and app permission requests carry `RequestedCapability`; `GrantedCapability` is not deserializable and requires an `AdministratorPrincipal` input; `EffectiveCapabilities` is non-deserializable and can only be projected from grants. Focused tests exercise the constructor and projection boundary. |
| 7. Default-deny and DNS post-resolution semantics | `NetworkPolicy::default()` has no allow rules. Pure evaluation denies by default, requires both hostname and resolved-address checks, classifies IPv4/IPv6 loopback/private/link-local/multicast/unspecified scopes, rejects UDP as unsupported, and applies deny precedence. |
| 8. No app firewall/direct-network mutation | No such `AppMessage` variant exists. Launch profile is a distinct administrator-side `Secured`/`UnsafeDirect` value. |
| 9. Strict bounded manifest | `Manifest::decode` enforces the 65,536-byte ceiling, schema version, strict/duplicate-free fields, bounded lists and values, requested-only permissions, and no unknown executable hooks or grant fields. Tests cover valid round-trip, duplicate/unknown fields, max+1 collection/value cases, and oversize. |
| 10. Package-relative UI and bounded bridge | `PackagePath` rejects absolute/traversal, URL/host/port, encoded, query/fragment, backslash, control, and non-ASCII forms. UI bridge JSON is bounded and must contain one valid JSON value. |
| 11. Sandbox vocabulary only | `SandboxAttestation::validate_secured` requires every frozen property, rejects duplicate/missing properties, and bounds metadata. The API has no platform backend. |
| 12. Strict bounded framing/control | Handshake and frame golden bytes, every incomplete header length, payload max/max+1, flags/version/kind/stream constraints, strict JSON duplicates/unknowns, request/stream duplicate and capacity+1 cases, and deterministic arbitrary-byte no-panic coverage pass. `fuzz/app_contract` covers the hostile decoders. |
| 13. Routine verification floor | Exact local commands and outcomes are recorded below. The serial workspace floor passed with 4,066 tests and 35 ignored. |
| 14. No runtime capability or advertisement | No socket/listener/process/filesystem/DNS owner, sandbox mechanism, router advertisement, app implementation, storage migration, support inventory update, or anonymity claim was added. |

## Verification

All evidence below is from the local worktree on implementation head
`b6e0810`; no hosted CI result is claimed.

| Command | Result |
|---|---|
| `rtk cargo fmt --all --check` | PASS |
| `rtk cargo check --locked --workspace --all-targets` | PASS |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS — 4,066 passed, 35 ignored, 148 suites |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `rtk proxy env RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` | PASS |
| `rtk cargo test --locked --workspace --doc` | PASS — 20 suites, 0 doctests |
| `rtk cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1` | PASS — 10 tests |
| `rtk cargo clippy --locked -p i2pr-app-proto --all-targets --all-features -- -D warnings` | PASS |
| `rtk proxy env RUSTDOCFLAGS='-D warnings' cargo doc --locked -p i2pr-app-proto --no-deps` | PASS |
| `rtk bash scripts/check-dependency-direction.sh` | PASS |
| `rtk bash scripts/check-runtime-boundaries.sh` | PASS — includes app-proto positive-control pass |
| `rtk bash scripts/check-service-tunnel-boundaries.sh` | PASS |
| `rtk python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 6 tests |
| `rtk bash scripts/check-fixture-manifest.sh` | PASS |
| `rtk bash scripts/check-ntcp2-vectors.sh` | PASS |
| `rtk bash scripts/check-ssu2-vectors.sh` | PASS |
| `rtk bash scripts/check-i2cp-vectors.sh` | PASS — 15 vector tests |
| `rtk bash scripts/check-ntcp2-interoperability.sh` | PASS |
| `rtk bash scripts/check-constrained-host-lane-boundary.sh` | PASS |
| `rtk bash scripts/check-m11-transit-boundaries.sh` | PASS |
| `rtk bash scripts/check-m11-transit-qualification-evidence.sh` | PASS |
| `rtk bash scripts/check-sam-acceptance-evidence.sh` | PASS |
| `rtk bash scripts/check-ssu2-acceptance-evidence.sh` | PASS |
| `rtk bash scripts/check-i2cp-acceptance-evidence.sh` | PASS |
| `rtk bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS |
| `rtk bash scripts/check-service-tunnel-acceptance-evidence.sh` | PASS |
| `rtk bash scripts/check-exploratory-tunnel-evidence.sh` | PASS |
| `rtk bash scripts/check-netdb-tunnel-evidence.sh` | PASS |
| `rtk bash scripts/check-destination-tunnel-evidence.sh` | PASS |
| `rtk bash scripts/check-streaming-tunnel-evidence.sh` | PASS |
| `rtk bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS — existing unbound historical-label warnings remain diagnostic only |
| `rtk bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS — expected negative self-test probes plus final integrity pass |
| `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS — 18 tests |
| `rtk cargo deny check advisories bans sources` | PASS — advisories/bans/sources; existing duplicate-version warnings for `block-buffer` and `windows-sys` only |
| `rtk bash scripts/fuzz-smoke.sh` | PASS — all 25 targets, including `app-contract`, completed 32 deterministic runs each under nightly; no crash/regression corpus was retained |

The plan's earlier `bash scripts/check-global-plan-number-uniqueness.py`
spelling was corrected in `AGENTS.md`, `README.md`, and this plan: the file is
Python and CI already invokes it with `python3`.

## Dependency and compatibility review

No new external dependency was added. The crate reuses already centralized
workspace `serde`, `serde_json`, and `thiserror` dependencies with the
workspace's existing feature policy. `Cargo.lock` adds only the new local
package entry. The standalone fuzz lock/workspace adds the local crate edge;
`libfuzzer-sys` remains confined to the pre-existing test-only fuzz workspace.

There is no deployed wire, package, persistence, or migration obligation. The
language-neutral v1 reference freezes the 9-byte role/version handshake,
12-byte frame header, JSON control vocabulary, strict schema rules, and
ceilings. Rust serde forms are not the ABI. Future package signatures must
retain the exact authenticated manifest bytes (or a separately frozen
canonical encoding) without reinterpreting manifest semantics.

## Security, lifecycle, and operational review

- Authority escalation: requested data cannot construct grants/effective
  capabilities. A future adapter must authenticate `AdministratorPrincipal`
  before constructing policy grants; the type is not itself an auth token.
- Confused deputy: app messages have no install/grant/revoke/firewall/direct-
  network mutation variants. Proposal 170 is only a scoped service label here;
  its credential/dispatch adapter remains future work.
- Network scope: no rule is allowed by default. A future hostname broker must
  pass both hostname and post-resolution address evaluation. Loopback, private,
  link-local, multicast, and unspecified scopes require explicit matching
  administrator policy.
- UI origin: only package-relative static resources are described; UI payloads
  are bounded JSON and have no URL/localhost field.
- Identity correlation: app, publisher, version, instance, app principal,
  resource ID, router identity, and Destination are separate contract domains.
  `PrincipalOwnedResource` pairs a resource ID with its owner.
- Lifecycle/contention: this crate owns no tasks, queues, locks, or persistent
  mutable state. `SessionLimits` is a caller-owned deterministic counter set;
  a future session owner must serialize updates and perform cancellation/close
  cleanup.
- No router identity/storage format or protocol-support advertisement changed.

No unresolved critical/high/medium/low implementation finding remains within
Plan 345 scope. The explicit unresolved limitations are future work: no OS
sandbox exists, no process is supervised, no socket/DNS/broker is implemented,
and allowed traffic cannot prevent malicious application code from encoding
identifying information.

## Unblock audit and roadmap disposition

`plans/registry.md` contains no registered successor with Plan 345 as a hard or
interface dependency; the managed-app roadmap previously contained only
unnumbered successor classes. Therefore no existing blocked plan status could
be flipped. The roadmap now records these readiness results:

| Successor class | Status after Plan 345 | Remaining dependency |
|---|---|---|
| Router-side app-principal gateway for SAM/I2CP | Ready to plan | Must adapt existing SAM/I2CP owners and preserve their current scope |
| Package/lifecycle manager + AppManager admin owner | Ready to plan | Frozen admin vocabulary; must provide a separately authenticated admin owner |
| OS sandbox/process/resource backend | Sequenced; not ready to implement as an app capability | Package/lifecycle owner and platform qualification |
| Scoped Proposal 170 adapter | Blocked | Canonical Proposal 170 completion; current Plan 348 is blocked on 342 + 347 |
| Brokered DNS/TCP, SDK, and UI host | Sequenced | Sandbox/runtime and gateway owners; no direct-network exception is implied |

Plan 345 is closed as infrastructure only. It does not make any native
application safe to execute or usable in the router.
