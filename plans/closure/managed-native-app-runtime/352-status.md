# Plan 352 status — managed-app mapped-IPv6 policy canonicalization

Status: **passed-managed-app-mapped-ipv6-policy-canonicalization**.

Classification: corrective invariant + infrastructure. No user-visible runtime
capability, resolver, socket, process, broker, router adapter, or anonymity claim
was added.

## Implementation commits

- `5bbbf19` — activated Plan 352.
- `3e6bffc` — froze mapped-address policy identity in the v1 reference before
  implementation.
- `cf54055` — canonicalized policy targets and rejected mapped policy selectors;
  this commit also removes the tracked consumer build output required by the
  concurrent Plan 353 integration work.
- `d1be21e` — made the pre-fix representation mismatch explicit in the
  regression test.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Freeze policy identity before code | `3e6bffc`, `specs/references/managed-native-app-runtime-v1.md` | Mapped targets canonicalize to IPv4; exact mapped selectors and mapped-network CIDRs are invalid; native IPv6 retains its identity. The spec commit precedes implementation commit `cf54055`. |
| Reproduce the old bypass and prove correction | `mapped_ipv6_targets_share_ipv4_policy_identity` | The test proves raw `IpAddr` values differ, matching the former exact/CIDR comparison gap, then proves IPv4 exact and CIDR deny rules reject both the IPv4 and mapped forms. Hostname allow cannot bypass either deny. |
| Match allow and deny consistently | Same regression test | Exact and /24 CIDR allow/deny cases return equal decisions for `8.8.8.8` and `::ffff:8.8.8.8`; the deterministic /24 loop checks requested-IP and hostname post-resolution decisions across 256 addresses. |
| Preserve scoped-address policy | Same regression test and existing `default_deny_scopes_and_post_resolution_check` | Mapped private and documentation addresses inherit their IPv4 scope, remain denied without an explicit IPv4 allow, and pass with an explicit IPv4 CIDR allow. |
| Reject noncanonical selectors | Same regression test | Mapped exact-IP and mapped-network CIDR selectors return `ContractError::InvalidPolicy`; evaluation fails closed. |
| Preserve native IPv6 and Plan-349 behavior | Same regression test; `cargo test -p i2pr-app-proto` | Native IPv6 exact selectors continue to validate and authorize. Existing hostname, scope, direction, broker reservation, correlation, and typed-error rows pass. |
| Keep implementation pure and dependency-free | package manifest, dependency/runtime checks | No dependency changes; one allocation-free helper owns target canonicalization. No DNS, socket, process, filesystem, Tokio, or platform behavior. |
| Compatibility and consumer audit | repository search, workspace dependency graph, Plan 345/349 records | The v1 contract has no in-tree runtime consumer or persisted policy owner. The pre-release no-consumer assumption holds in this tree; protocol version remains 1.0. No migration is required. |

## Verification

All commands below ran locally on the rebased Linux worktree. No hosted CI
result is claimed.

Focused Plan 352 checks:

```text
cargo fmt --all --check                                      passed
cargo check --locked -p i2pr-app-proto --all-targets          passed
cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1
  15 passed
cargo clippy --locked -p i2pr-app-proto --all-targets --all-features -- -D warnings
  passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-proto --no-deps
  passed
bash scripts/fuzz-smoke.sh                                   passed
```

The final Plan 352 implementation tree also passed the full repository floor
recorded for Plan 353 below: workspace check; 4,071 workspace tests passed and
35 ignored across 148 suites; clippy; rustdoc; doc tests; all routine boundary,
vector, acceptance-evidence, planning, fuzz, and dependency-policy checks.

## Security, compatibility, and limitations

The old bypass was representation-dependent: `address_scope` classified a
mapped result through its embedded IPv4 address, but the matcher compared the
unmapped IPv4 selector directly with the original IPv6 `IpAddr`. That missed an
IPv4 deny and allowed a global hostname result to pass. Target canonicalization
now precedes both scope classification and exact/CIDR matching. Explicit deny
precedence is unchanged. Mapped IPv6 selectors are rejected rather than
normalized, so administrators have one canonical IPv4 spelling for IPv4 rules.

The change is pre-release and wire-neutral. No new dependency or persisted
configuration exists. The standard fuzz smoke passed; no separate policy
decoder fuzz target is needed because network-policy selectors have no wire
decoder in this contract. No reference-router or runtime lane applies to this
pure policy correction.

Findings by severity: critical none; high none; medium none; low none.

## Unblock audit

The registry and managed-app roadmap contain no separately numbered registered
implementation plan with a hard dependency on Plan 352. The future router
app-principal gateway and package/lifecycle + AppManager work were explicitly
blocked on this interface correction; both may now proceed to bounded plan
drafting. Their implementation is still subject to separate plans and this
branch's Plan 353 integration gate. OS sandbox qualification, router adapters,
Proposal-170 integration, and all runtime capability claims remain gated by
their own owners and evidence. No other subsystem plan is unblocked.

