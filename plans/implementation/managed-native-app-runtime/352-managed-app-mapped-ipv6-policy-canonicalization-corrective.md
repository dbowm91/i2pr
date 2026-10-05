# Plan 352 — managed app mapped-IPv6 policy canonicalization corrective

Status: **registered-managed-app-mapped-ipv6-policy-corrective**.

Classification: **corrective invariant + infrastructure**. This plan corrects one post-Plan-349 policy-normalization defect in the unreleased managed-app v1 contract. It does not add DNS, sockets, a clearnet broker, process supervision, sandboxing, router adapters, package lifecycle, or a user-visible application capability.

Corrects:
- managed-runtime Plan 349:
  - `plans/implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md`
  - `plans/closure/managed-native-app-runtime/349-status.md`

Roadmap:
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:
- Plan 349 is closed as `passed-managed-app-v1-direction-broker-network-policy-corrective`.

## Objective

Make IPv4-mapped IPv6 addresses policy-equivalent to their canonical IPv4 address for every IP/CIDR authorization decision.

Plan 349 correctly made `address_scope(::ffff:a.b.c.d)` inherit the mapped IPv4 classification, but its IP/CIDR rule matcher still compares the original `IpAddr::V6` value against IPv4 selectors. This creates a representation-dependent authorization result.

The canonical defect is:

```text
allow hostname=example.org tcp/443
deny  ip=8.8.8.8 tcp/443

resolved 8.8.8.8          -> denied
resolved ::ffff:8.8.8.8   -> currently misses IPv4 deny, classifies Global, may allow
```

The same mismatch affects IPv4 CIDR allow/deny rules.

The corrected contract must make those two runtime representations authorization-equivalent.

## Why this corrective is ready

The defect is entirely inside the pure runtime-neutral policy layer:

- `address_scope` already recognizes IPv4-mapped IPv6 using `to_ipv4_mapped()`;
- `NetworkPolicy::matching_ip_rules` does not canonicalize its target before comparing exact IP or CIDR selectors;
- no resolver, socket, platform API, or router owner is required to reproduce or fix the issue;
- Plan 349 already froze the security rule that explicit IP/CIDR deny overrides hostname authorization.

This is therefore a bounded follow-up to the same contract rather than a broker/runtime milestone.

## Security invariant

Policy authorization must depend on the destination address, not on which equivalent socket-address representation a resolver/runtime supplied.

For all IPv4 address `v4`:

```text
policy(v4, port) == policy(::ffff:v4, port)
```

for:
- direct IP evaluation;
- hostname post-resolution evaluation;
- exact IPv4 selectors;
- IPv4 CIDR selectors;
- allow rules;
- deny rules;
- deny precedence.

No mapped representation may bypass an administrator rule written in canonical IPv4 form.

## Required contract decision

Freeze one canonical policy-address representation.

The preferred v1 rule is:

1. runtime target addresses are canonicalized before rule matching;
2. an IPv4-mapped IPv6 target canonicalizes to `IpAddr::V4(mapped)`;
3. ordinary IPv6 targets remain IPv6;
4. policy selectors are canonical:
   - an exact `DestinationSelector::Ip` containing IPv4-mapped IPv6 is rejected by `NetworkPolicy::validate`;
   - an `IpCidr` whose network is IPv4-mapped IPv6 is rejected by `NetworkPolicy::validate`;
   - administrators express IPv4 policy in ordinary IPv4 form;
5. `address_scope` and policy matching therefore operate over the same canonical target identity.

This deliberately avoids two textual/policy spellings for one IPv4 destination and avoids ambiguous mapped-IPv6 CIDR semantics.

If implementation discovers a stronger reason to normalize mapped selectors rather than reject them, stop and update the language-neutral contract first. Do not silently accept both representations with subtly different CIDR behavior.

## Invariants that must not regress

1. Hostname-only allow continues to authorize an otherwise permitted globally routable result.
2. A matching IP/CIDR deny always wins over hostname authorization.
3. Non-global/special-purpose destinations still require explicit IP/CIDR authorization.
4. IPv4-mapped IPv6 continues to inherit the mapped IPv4 address classification.
5. `brokered_tcp` remains reserved and non-openable.
6. Four directional control vocabularies, request correlation, and typed errors remain unchanged.
7. `i2pr-app-proto` remains runtime-neutral with no DNS/socket/process/filesystem/Tokio/platform owner.
8. No router protocol/support advertisement changes.
9. No runtime or anonymity/security capability claim is added.

## Scope

### In scope

- update the managed-app v1 language-neutral reference with canonical mapped-address semantics;
- add a small pure canonicalization helper or equivalent internal representation;
- apply canonicalization before exact IP/CIDR rule matching;
- reject mapped-IPv6 exact/CIDR policy selectors as noncanonical;
- regression/property tests;
- fuzz coverage if the relevant decoder/selector validation path is not already covered;
- architecture docs and Plan-352 closure/roadmap/registry state.

### Out of scope

- DNS resolution;
- address-selection/racing policy;
- Happy Eyeballs;
- socket addresses or connect calls;
- IPv4-compatible historical IPv6 forms other than standardized IPv4-mapped addresses;
- NAT64 synthesis semantics;
- broker connect transaction design;
- SAM/I2CP/Proposal-170 adapters;
- OS sandbox/process runtime;
- package/AppManager implementation;
- changing the Plan-349 closure record.

## Required production changes

### 1. Specification freeze before code

Amend `specs/references/managed-native-app-runtime-v1.md` before implementation.

The reference must state:
- mapped IPv6 targets are canonicalized to IPv4 before IP/CIDR matching;
- mapped IPv6 policy selectors are invalid/noncanonical in v1;
- scope classification and rule matching use the same canonical address identity;
- equivalent mapped/unmapped runtime inputs have identical policy results.

This remains a pre-runtime v1 correction; no wire version bump is required unless implementation discovers an actual external consumer.

### 2. Canonical target helper

Introduce one pure helper, conceptually:

```text
canonical_policy_ip(IpAddr) -> IpAddr
```

with exactly:
- `V6(v).to_ipv4_mapped() -> V4(mapped)`;
- otherwise unchanged.

Use one owner for this logic. Do not duplicate mapped-address handling in multiple evaluators.

### 3. Match on canonical target

Apply canonicalization before:
- `evaluate_requested_ip` matching;
- `evaluate_resolved_address` IP/CIDR matching;
- any shared `matching_ip_rules` helper.

`address_scope` may retain its existing mapped handling or call the canonical helper, but there must be one provable semantic result.

### 4. Reject noncanonical policy selectors

`NetworkPolicy::validate` must reject:
- exact mapped-IPv6 `DestinationSelector::Ip`;
- mapped-IPv6 `IpCidr.network`.

Reason: an administrator should not be able to encode two semantically equivalent selectors with different matching behavior.

No automatic mutation of serialized policy state is introduced by this contract crate.

## Work packages

### A. Freeze semantics

1. Update v1 reference.
2. Add a short Plan-352 note in `docs/architecture/i2pr-app-proto.md`.
3. Freeze rejection vs normalization behavior before Rust changes.

### B. Implement canonicalization

1. Add the pure canonical-address helper.
2. Route target IP matching through it.
3. Reject mapped selectors in policy validation.
4. Preserve all Plan-349 behavior outside this defect.

### C. Regression and property evidence

Add exact tests for:
- exact IPv4 deny vs mapped target;
- IPv4 CIDR deny vs mapped target;
- exact IPv4 allow vs mapped target;
- IPv4 CIDR allow vs mapped target;
- hostname allow + IPv4 exact deny vs mapped global target;
- hostname allow + IPv4 CIDR deny vs mapped global target;
- non-global mapped target requiring explicit IPv4 allow;
- mapped policy exact selector rejected;
- mapped policy CIDR selector rejected;
- ordinary IPv6 selector behavior unchanged.

Add a deterministic property loop over representative/random IPv4 values proving:

```text
evaluate_requested_ip(v4) == evaluate_requested_ip(mapped(v4))
evaluate_resolved_address(host, v4) == evaluate_resolved_address(host, mapped(v4))
```

for generated allow/deny cases that remain within the bounded test budget.

## Failure, cancellation, restart, and contention semantics

No runtime owner exists.

- invalid/noncanonical policy fails validation;
- invalid policy evaluates deny;
- mapped target canonicalization is deterministic and allocation-free;
- no retry, cancellation, persistence, resolver, or concurrent owner is introduced;
- arithmetic and CIDR bounds remain checked as in Plan 349.

## Compatibility and migration

The managed-app v1 contract is still unreleased and has no authorized external consumer.

Therefore:
- mapped-IPv6 policy selector rejection is a pre-release tightening;
- no compatibility alias for mapped selectors is required;
- protocol version remains 1.0 if the no-consumer assumption is confirmed at closure;
- no persistent migration exists because no runtime policy store exists.

If a real consumer or persisted policy owner is discovered, stop and register a migration/versioning corrective instead.

## Required tests

At minimum:

```text
deny 8.8.8.8/32     rejects 8.8.8.8 and ::ffff:8.8.8.8
deny 8.8.8.0/24     rejects 8.8.8.8 and ::ffff:8.8.8.8
allow 8.8.8.8/32    treats both forms identically
allow 8.8.8.0/24    treats both forms identically

hostname allow example.org:443
+ exact IPv4 deny
resolved ::ffff:same-ip
=> deny

hostname allow example.org:443
+ IPv4 CIDR deny
resolved ::ffff:member
=> deny
```

Also:
- mapped RFC1918 and mapped documentation targets inherit their IPv4 non-global classification and explicit-allow requirements;
- exact mapped selector fails `validate`;
- mapped-CIDR network fails `validate`;
- ordinary native IPv6 global and special-purpose cases retain Plan-349 outcomes.

## Exact verification commands

Focused:

```text
cargo fmt --all --check
cargo check --locked -p i2pr-app-proto --all-targets
cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1
cargo clippy --locked -p i2pr-app-proto --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-proto --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
bash scripts/fuzz-smoke.sh
```

Closure must also run the full routine floor from `AGENTS.md` and record local vs hosted CI evidence truthfully.

## Documentation updates

Required:
- `specs/references/managed-native-app-runtime-v1.md`;
- `docs/architecture/i2pr-app-proto.md`;
- `plans/subsystems/managed-native-app-runtime-roadmap.md`;
- `plans/registry.md`;
- `plans/closure/managed-native-app-runtime/352-status.md` at closure.

Do not rewrite Plan 349's closure record.

## Acceptance criteria

Plan 352 passes only when:

1. mapped IPv6 target canonicalization is frozen in the language-neutral reference before code;
2. mapped and unmapped representations of the same IPv4 target produce identical exact-IP decisions;
3. mapped and unmapped representations produce identical IPv4-CIDR decisions;
4. hostname authorization cannot bypass an IPv4 deny via a mapped IPv6 result;
5. non-global mapped results retain the mapped IPv4 scope and authorization requirements;
6. mapped IPv6 policy selectors/CIDRs are rejected as noncanonical;
7. native IPv6 behavior is unchanged except where a regression test proves an intentional correction;
8. Plan-349 directional/broker/security invariants remain green;
9. no runtime/network owner or user-visible capability lands;
10. focused and full routine floors pass;
11. fuzz smoke remains green;
12. successor managed-app gateway/AppManager planning remains blocked until this corrective closes.

## Stop conditions

Stop and register a new plan if:
- canonicalization requires OS/socket-address behavior rather than pure `IpAddr`;
- NAT64 or resolver synthesis policy must be defined;
- a deployed/persisted managed-app policy consumer is discovered;
- correcting the issue requires changing the wire major version;
- a new dependency is proposed.

## Closure evidence required

The closure record must include:
- spec-freeze commit before implementation;
- exact before/after bypass reproduction;
- exact-IP and CIDR mapped/unmapped equivalence rows;
- mapped-selector rejection evidence;
- deterministic property-test result;
- focused/full verification commands and results;
- dependency diff;
- compatibility/no-consumer confirmation;
- security review for deny precedence and representation confusion;
- unblock audit.

## Handoff notes

Keep this pass narrow. The bug is not in IPv6 routing, DNS, or broker behavior; it is a canonical identity mismatch inside pure policy matching.

After Plan 352 closes, the corrected v1 contract may again unblock bounded planning of the router app-principal gateway and package/lifecycle + AppManager owner, subject to the separate branch-integration hygiene authority.
