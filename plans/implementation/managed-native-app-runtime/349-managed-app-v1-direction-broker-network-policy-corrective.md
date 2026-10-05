# Plan 349 — managed app v1 direction, broker, and network-policy corrective

Status: **registered-managed-app-v1-contract-corrective**.

Classification: **corrective invariant + infrastructure**. This plan corrects four contract defects discovered by post-closure review of Plan 345. It does not launch applications, add a sandbox, open a socket, resolve DNS, adapt SAM/I2CP/Proposal 170, implement AppManager, or create a user-visible application capability.

Corrects:
- `plans/implementation/managed-native-app-runtime/345-native-app-runtime-foundation-and-capability-contract.md`
- `plans/closure/managed-native-app-runtime/345-status.md`

Roadmap:
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:
- Plan 345 is closed as `passed-managed-native-app-runtime-contract-foundation`.

Plan 345's closure remains immutable historical authority for the evidence it executed. This corrective does not relabel its tests as failed. It narrows one conclusion from that closure: the v1 contract is not yet ready to serve as a stable downstream gateway/AppManager API until the defects below are corrected.

## Objective

Correct the Plan-345 v1 application contract before any router gateway, AppManager owner, SDK, or clearnet broker is built against it.

The pass has exactly four outcomes:

1. make hostname allow rules usable without requiring a second public-IP/CIDR allow rule while preserving DNS-rebinding and explicit-deny protection;
2. replace the incomplete "public vs a few local scopes" classifier with a fail-closed globally-routable/non-global address policy suitable for an anonymity-sensitive broker;
3. make application and administrator control traffic directionally explicit, request-correlated, and error/reply complete;
4. remove the underspecified `brokered_tcp` openable service from v1 until the later broker milestone defines a real connect transaction, while retaining the capability vocabulary as reserved policy intent.

No other Plan-345 contract area is reopened.

## Why this corrective is ready

The defects are observable in the closed Plan-345 implementation and require no router runtime or OS-specific implementation to resolve.

### Finding A — hostname rules require an unintended second public-IP allow

Current `NetworkPolicy::evaluate_resolved_address`:

1. requires `evaluate_hostname(...)` to allow the hostname;
2. then requires `evaluate_requested_ip(...)` to allow the resolved IP.

Because `evaluate_requested_ip` is default-deny, an administrator rule such as:

```text
allow tcp irc.example.org:6697
```

does not authorize a normal public result by itself. A second IP/CIDR allow rule is required. That makes hostname policy brittle and conflicts with the intended user-facing firewall semantics.

The desired rule is:

- the hostname rule authorizes the named service;
- every resolved address is independently classified;
- a genuinely globally routable resolved address may pass the hostname authorization unless an explicit matching IP/CIDR deny blocks it;
- a non-global/special resolved address remains denied unless the administrator separately authorizes that exact address/range/scope through the explicit IP/CIDR policy;
- deny precedence always wins.

This retains rebinding protection without forcing users to track public DNS address pools.

### Finding B — several non-global/special-purpose addresses are classified as `Public`

Plan 345 explicitly tested loopback, RFC1918/ULA, link-local, multicast, unspecified, and CGNAT. The current exclusion-based classifier still permits other special-purpose/non-global blocks to fall through as `AddressScope::Public`.

The corrective must establish a frozen fail-closed definition of "globally routable for broker purposes" and test at least:
- IPv4 this-network / `0.0.0.0/8`;
- loopback;
- RFC1918;
- CGNAT;
- link-local;
- IETF protocol-assignment/special-purpose ranges as applicable;
- documentation ranges;
- benchmarking range;
- multicast;
- reserved/future-use range;
- limited broadcast;
- IPv6 unspecified/loopback;
- IPv4-mapped IPv6;
- ULA;
- link-local;
- documentation;
- multicast;
- other IANA special-purpose/non-global ranges relevant to the pinned classification.

The exact classification table must be frozen in the language-neutral reference with provenance to the authoritative address-special-purpose registries or a documented standard-library predicate whose semantics are valid at the repository MSRV. Security behavior is fail-closed: a range not proven globally routable is not treated as ordinary public egress.

Do not use a dependency merely to obtain this classification unless it passes the repository dependency review.

### Finding C — control messages lack direction and complete request/reply/error semantics

Plan 345 separated `AppMessage` and `AdminMessage` by *role*, but each enum still mixes messages that conceptually travel in different directions. For example, the application vocabulary contains both requests such as `open`/`permission_request` and host-origin state such as `capabilities`/`permission_status`.

The administrator vocabulary is more serious: install/update/uninstall/launch/stop/grant/revoke/policy requests have no `request_id`, and v1 defines no corresponding reply/error envelope. A future AppManager therefore cannot safely correlate concurrent operations without inventing semantics outside the frozen contract.

Plan 349 must make direction a protocol invariant.

At minimum freeze disjoint directional surfaces equivalent to:

```text
Application -> Host
Host -> Application
Administrator -> Host
Host -> Administrator
```

Exact type names may differ.

Requirements:
- every operation that expects completion has a nonzero bounded `request_id`;
- a response carries the same request id;
- success and typed failure are representable without parsing diagnostic text;
- errors have a finite code vocabulary plus bounded human diagnostic text where useful;
- unsolicited host events are explicitly different from replies and cannot spoof a reply;
- application-origin bytes cannot decode as host/admin authority messages;
- administrator-origin requests cannot be accepted on an application-role session;
- request-id duplicate/in-flight ceilings remain mechanically enforceable;
- no admin mutation is fire-and-forget;
- every frozen request either has a defined response or is explicitly reserved/unsupported and rejected.

Do not prematurely invent package lifecycle result objects merely to preserve Plan 345's current request names. If an operation's response semantics cannot yet be truthfully frozen without the future owner, narrow/remove/reserve that operation rather than publishing an ambiguous v1 API.

### Finding D — `brokered_tcp` is openable but no connect protocol exists

Plan 345 exposes both:
- `Capability::BrokeredTcp`;
- `AppService::BrokeredTcp`, reachable through generic `open { service, stream_id }`.

The contract does not specify a destination host, port, connect result, DNS behavior, address-selection behavior, timeout/failure result, or when the data stream becomes valid. A downstream SDK or broker would have to invent incompatible semantics.

Plan 349 must:
- keep `brokered_tcp` as a reserved/requestable capability if useful for manifest/policy planning;
- remove it from the generic v1 openable router-service vocabulary;
- state explicitly that no effective runtime grant or live broker service exists yet;
- reserve the later broker milestone to define a dedicated typed connect transaction after the trusted broker owner exists.

Do not define DNS/socket runtime behavior in this corrective.

## Invariants that must not regress

All Plan-345 invariants remain authoritative:

1. `i2pr-app-proto` stays runtime-neutral and has no router, socket, filesystem, process, DNS, Tokio, dynamic-loader, or sandbox backend owner.
2. Application, administrator, publisher, router, Destination, and app-instance identities remain separate.
3. Requested capabilities remain inert; only administrator-origin policy constructs grants/effective state.
4. Applications cannot grant/revoke permissions, edit firewall policy, select direct networking, install/update packages, or mutate another app.
5. Direct host networking remains denied by default, including loopback.
6. UI descriptors remain package-relative static resources with no remote/localhost URL.
7. Every hostile input remains bounded and strict.
8. No router protocol support/advertisement changes.
9. No user-visible application/sandbox/anonymity capability is claimed.

## Scope

### In scope

- amend the language-neutral managed-app v1 reference;
- update `i2pr-app-proto` contract types and pure validation/evaluation;
- directional message enums or equivalent mechanically disjoint direction model;
- request/reply/error correlation;
- pure network-policy correction;
- comprehensive non-global address classification;
- removal/reservation of generic `brokered_tcp` service opening;
- golden/adversarial/property/fuzz tests;
- architecture docs and static contract assertions;
- planning/closure reconciliation for Plan 349.

### Explicitly out of scope

- sockets, DNS lookups, proxy connections, or broker implementation;
- process launch or supervision;
- OS sandboxing;
- package install/update implementation;
- cryptographic package signatures;
- SAM/I2CP adapter implementation;
- Proposal-170 adapter implementation;
- console or webview;
- SDK convenience API;
- mail/IRC/torrent apps;
- rebase/merge of unrelated current-main work unless needed to execute this plan;
- changes to Plan 345's historical closure record.

## Required production changes

### 1. Re-freeze the corrected v1 contract before dependent code changes

Update `specs/references/managed-native-app-runtime-v1.md` first in the corrective implementation sequence.

The update must explicitly mark the Plan-345 forms being corrected and freeze:
- directional control surfaces;
- request/reply/error correlation;
- brokered-TCP reservation semantics;
- hostname + resolved-address policy algorithm;
- global/non-global address classification rules.

Because v1 is not deployed, this is a pre-release contract correction, not a compatibility migration. Preserve a clear provenance note explaining that Plan 349 corrected the pre-runtime v1 before downstream consumers were authorized.

### 2. Directional control surfaces

Refactor the current role-only message model so direction is represented by type/decoder rather than by prose.

The contract must not rely on callers remembering "this variant is only sent by the host."

A valid design has four enums or equivalent typed channels:
- app-to-host;
- host-to-app;
- admin-to-host;
- host-to-admin.

Shared passive value types may remain shared.

Add finite response/error types. Error text is diagnostic only; callers branch on typed code/outcome.

### 3. Request correlation

Add request IDs to every request whose completion matters.

At minimum:
- app service open;
- permission request;
- every retained administrator operation.

Responses must echo the request ID. Duplicate active request IDs fail before any future owner is invoked.

If a current admin operation cannot have a truthful response without owner semantics, remove/reserve it now and let the future AppManager plan add it with complete semantics.

### 4. Correct hostname resolution policy

Freeze and implement the two-stage algorithm:

```text
requested hostname
  -> hostname rule decision
  -> resolver returns address
  -> address safety/global classification
  -> explicit IP/CIDR deny check
  -> for non-global address: explicit IP/CIDR allow required
  -> for globally-routable address: hostname allow is sufficient
  -> allow only if every gate passes
```

An explicit deny always wins.

If multiple addresses are returned, the future broker must evaluate each selected address independently. The pure contract does not choose resolver ordering or racing behavior.

### 5. Replace `Public` fall-through with fail-closed global classification

Do not call an address globally routable merely because it did not match the handful of existing local tests.

The corrected vocabulary should make the security distinction explicit, for example:
- `Global`;
- `Loopback`;
- `Private`;
- `LinkLocal`;
- `Multicast`;
- `Documentation`;
- `Benchmark`;
- `Reserved` / `SpecialPurpose`;
- `Unspecified`;

or a simpler `Global` vs `NonGlobal(reason)` representation.

Exact representation is secondary to these properties:
- ordinary brokered hostname policy only treats the frozen globally-routable class as automatically eligible after hostname authorization;
- all special/non-global classes default deny;
- explicit administrator IP/CIDR policy may authorize a non-global target only where the contract intentionally permits that scope;
- classifications are deterministic and testable without I/O.

### 6. Reserve brokered TCP until its owner plan

Remove `BrokeredTcp` from the generic router-service open path.

The capability may remain in `Capability` / manifest requests as a reserved future permission, but:
- no current app control message opens a brokered TCP stream;
- no response implies broker availability;
- docs state that a later broker plan will define connect target, DNS/address checks, timeout/failure taxonomy, and stream-ready semantics.

Tests must reject the old generic brokered-TCP open form if it remains representable in raw JSON.

### 7. Keep static boundaries and fuzzing live

Extend `fuzz/fuzz_targets/app_contract.rs` to fuzz all new directional decoders and reply/error envelopes.

Do not weaken Plan-345 runtime/dependency guards.

## Ordered work packages

### A. Corrective specification freeze

1. Update the v1 language-neutral reference with a dated/Plan-349 corrective section.
2. Freeze directional message matrix, request/reply/error model, network algorithm, global-address definition, and broker reservation.
3. No Rust contract edits before this freeze commit.

### B. Direction/correlation implementation

1. Split/refactor app/admin message surfaces by direction.
2. Add typed response/error vocabulary.
3. Add request IDs and validate nonzero/duplicate/in-flight behavior.
4. Remove ambiguous message variants whose future owner semantics are not yet defined.

### C. Network-policy correction

1. Implement fail-closed global/non-global classification.
2. Correct hostname + post-resolution decision semantics.
3. Preserve deny precedence.
4. Add exhaustive IPv4/IPv6 special-purpose boundary fixtures.

### D. Brokered-TCP narrowing

1. Remove generic broker service opening.
2. Keep only the explicitly reserved capability/request vocabulary justified by the reference.
3. Add regression tests showing no v1 app can create a broker stream.

### E. Evidence and docs

1. Update `docs/architecture/i2pr-app-proto.md`.
2. Update roadmap/registry at closure.
3. Record exact regression tests for each finding.
4. Run the full Plan-345 floor plus fuzz smoke.

## Failure, cancellation, restart, and contention semantics

The crate still owns no runtime.

Contract-level requirements:
- malformed/wrong-direction messages fail before a future owner call;
- unsupported/reserved operations return a typed failure, never silent ignore;
- reply IDs cannot be zero and must correspond to an active request at the session/accounting layer;
- duplicate request IDs are rejected;
- diagnostic text is bounded and never used as the machine decision key;
- policy parse/validation failure returns deny;
- unknown/special IP classification returns non-global/deny, never public/global by fall-through;
- no retry, timeout, resolver, or connection loop is introduced here.

## Compatibility and migration

There is no deployed managed-app runtime or downstream compatibility promise.

Plan 349 is therefore allowed to make breaking pre-release changes to the v1 contract and Rust API.

Requirements:
- keep protocol major 1 only if the corrected reference explicitly defines Plan 345 as an unreleased draft and no consumer can observe the old form;
- otherwise bump the contract version before any runtime owner is built;
- closure must state which choice was made and why;
- old ambiguous broker-open and directionless forms must not remain accepted as hidden compatibility aliases;
- no router persistent state changes.

## Required tests

### Direction and correlation

- each directional decoder accepts only its own message family;
- every opposite-direction message is rejected;
- app session rejects admin-to-host and host-to-admin messages;
- admin session rejects app messages;
- every request requires nonzero `request_id`;
- reply/error echoes the exact request ID;
- duplicate active request ID rejected;
- max in-flight and max+1;
- unsolicited event cannot decode as a response;
- typed error code round-trip;
- bounded diagnostic max/max+1;
- unknown response/error code rejected fail-closed.

### Broker reservation

- `brokered_tcp` cannot be opened through generic service-open;
- old raw JSON spelling is rejected;
- reserved `BrokeredTcp` capability, if retained, remains requested-only/inert;
- no new runtime/network owner appears in the crate.

### Hostname policy

Given only:
```text
allow hostname=irc.example.org tcp/6697
```

prove:
- globally routable resolved address -> allow;
- matching explicit IP/CIDR deny -> deny;
- loopback -> deny;
- RFC1918/ULA -> deny;
- link-local -> deny;
- documentation/benchmark/reserved/special -> deny;
- multicast/unspecified -> deny.

Then add explicit non-global IP/CIDR allow and prove only that intended target/range becomes eligible, while deny still wins.

### Address classification

Boundary tests for every frozen special-purpose range, including first/last address and adjacent globally-routable control where meaningful.

At minimum cover the ranges named in Finding B and any additional ranges in the frozen authoritative classification.

IPv4-mapped IPv6 must inherit the mapped IPv4 classification.

### Existing Plan-345 regressions

Re-run all identifier, manifest, path, capability, frame, sandbox-attestation, and no-panic tests unchanged unless the corrected directional wire contract requires an intentional golden update.

### Fuzz

Fuzz:
- frame;
- handshake;
- every directional control decoder;
- reply/error decoder;
- manifest;
- identifiers.

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

Closure floor: run the full routine floor from `AGENTS.md`, including the existing acceptance/evidence scripts and `cargo deny check advisories bans sources`. Record local vs hosted CI truthfully.

## Documentation updates

Required:
- `specs/references/managed-native-app-runtime-v1.md`;
- `docs/architecture/i2pr-app-proto.md`;
- `plans/subsystems/managed-native-app-runtime-roadmap.md`;
- `plans/registry.md`;
- `plans/closure/managed-native-app-runtime/349-status.md` at closure.

Update ADR 0032 only if the corrective changes a durable architectural decision. Do not rewrite it merely for wire spelling changes.

Do not modify Plan 345's closure record to hide that these findings were discovered after it closed.

## Acceptance criteria

Plan 349 passes only when:

1. the corrected language-neutral contract is frozen before dependent Rust changes;
2. application/admin control messages are mechanically directional;
3. every retained state-changing/completion-bearing request has a bounded nonzero request ID and defined typed response/error;
4. there is no fire-and-forget administrator mutation in v1;
5. generic `brokered_tcp` service opening is removed/rejected and the capability is explicitly reserved/inert until the broker owner plan;
6. a hostname allow rule alone permits an otherwise permitted globally-routable resolution;
7. hostname resolution to non-global/special-purpose space remains denied without explicit administrator IP/CIDR authorization;
8. explicit IP/CIDR deny overrides hostname authorization;
9. address classification is fail-closed and covers the frozen IPv4/IPv6 special-purpose registry/table rather than treating unknown exclusions as public;
10. all old Plan-345 security invariants and static boundaries remain green;
11. fuzz smoke covers all corrected hostile decoders;
12. no process, socket, resolver, sandbox, router adapter, package owner, app, or user-visible capability lands;
13. the full routine floor is green;
14. roadmap/registry mark the router gateway and package/AppManager successors blocked on Plan 349 until this corrective closes.

## Stop conditions

Stop and register a successor/research plan instead of broadening this pass if:

- correct global/non-global classification requires a new third-party dependency that has not completed repository dependency review;
- a real DNS/resolver implementation becomes necessary to test policy semantics;
- defining truthful AppManager operation responses requires implementing package/lifecycle state;
- brokered TCP cannot be removed/reserved without implementing its runtime;
- SAM/I2CP/Proposal-170 behavior must change;
- a contract version migration is required for an already shipped consumer discovered during implementation.

## Closure evidence required

The closure record must include:
- specification-freeze commit before code;
- exact diff of old vs corrected message-direction matrix;
- requirement-to-evidence matrix for Findings A-D;
- special-purpose address classification provenance/table;
- hostname-policy regression rows;
- brokered-TCP rejection rows;
- direction/request/reply/error golden fixtures;
- fuzz-smoke result;
- focused and full routine-floor results;
- dependency review;
- compatibility/version decision;
- security review covering confused-deputy, DNS rebinding, non-global egress, request spoofing, and authority escalation;
- unblock audit.

## Handoff notes

The purpose is to make the Plan-345 contract safe to build on, not to advance the runtime.

Prefer deleting or reserving ambiguous pre-runtime surface over inventing semantics for an owner that does not exist yet. In particular, do not preserve `brokered_tcp` generic opening or fire-and-forget admin requests merely for source compatibility: there is no deployed consumer to protect.

After Plan 349 closes, the router-side SAM/I2CP gateway and package/lifecycle + authenticated AppManager owner may again be considered ready to plan against the corrected contract.
