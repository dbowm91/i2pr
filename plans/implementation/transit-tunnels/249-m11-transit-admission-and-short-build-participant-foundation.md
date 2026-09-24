# Plan 249 — M11 transit admission and short-build participant foundation

Status at registration:
**registered-ready-m11-transit-admission-and-short-build-participant-foundation**

## Objective

Implement one runtime-neutral vertical slice in i2pr-tunnel:

1. interpret current ECIES tunnel-build bandwidth options;
2. make a deterministic bounded transit admission decision;
3. turn an authenticated/decrypted short-build request into an accepted or
   bandwidth-rejected encrypted reply;
4. atomically commit accepted hop-local state to a dedicated bounded transit registry;
5. prove rollback, expiry, and resource contention locally.

Stop before daemon socket/task composition. This is infrastructure, not a completed
transit-router capability.

## Why ready

Existing stable seams provide current short-build codecs/crypto, fixed 600-second request
lifetime, participant/IBGW/OBEP role primitives, replay/previous-peer checks, and
authenticated router-I2NP dispatch. ADR 0026 removes unrelated Java Streaming closure from
the dependency graph.

## Current implementation evidence

Reuse:

- crates/i2pr-tunnel/src/short_record.rs — ShortRequestRecord, BuildOptions,
  ShortReplyRecord, response codes 0/30, randomized reply encoder.
- crates/i2pr-tunnel/src/build_crypto.rs — request open, Noise state, layer-key derivation,
  reply sealing.
- crates/i2pr-tunnel/src/responder.rs — existing open/derive/seal sequence; test/simulation
  helper, not the production admission owner.
- crates/i2pr-tunnel/src/roles.rs — participant/gateway/endpoint transforms,
  previous-peer lock, duplicate rejection.
- crates/i2pr-tunnel/src/data_plane_registry.rs — bounded-registry design precedent only;
  ownership remains creator/local-pool, so do not mix remote transit state into it.

First daemon gap, out of scope: router_i2np.rs classifies ShortTunnelBuild as
TunnelBuildReserved. Plan 250 will own runtime composition.

## Protocol research authority

Normative:

- https://www.i2p.net/en/docs/specs/tunnel-creation-ecies/
- https://i2p.net/en/docs/specs/i2np/
- https://www.i2p.net/en/docs/specs/tunnel-implementation/
- https://i2p.net/en/proposals/168-tunnel-bandwidth/

Current rules relevant to this plan:

- ShortTunnelBuild records are 218 bytes; request plaintext 154 bytes; reply plaintext
  202 bytes.
- current request lifetime is 600 seconds.
- API 0.9.65 defines request keys m/r/l and accepted-reply key b.
- values are positive integer KBps strings.
- m <= r <= l when fields coexist.
- l is only for IBGW.
- if m cannot be met, reject with code 30.
- accepted reply should include b when m or r was requested and b >= m.
- bandwidth is best-effort; participating traffic remains lower priority.

Behavioral cross-check only: exact-pinned i2pd 2.61.0 TransitTunnel and Java I2P 2.13.0
TunnelDispatcher/TunnelParticipant. Do not copy implementation architecture.

## Invariants

- i2pr-tunnel stays runtime-neutral: no Tokio, sockets, sleeps, filesystem, processes.
- no new crypto primitive, unsafe, or secret Clone/Debug leakage.
- malformed Mapping remains rejected by canonical codec.
- production reply padding uses caller CSPRNG.
- zero or duplicate live receive tunnel ids fail closed.
- existing previous-peer/replay protections remain green.
- no daemon/config/RouterInfo/router.version/reference-pin changes.
- no public-network lane.

## Scope A — Typed bandwidth interpretation

Add a narrow typed consumer, preferably transit.rs, layered over BuildOptions.

Intent:

~~~text
TransitBandwidthRequest {
  minimum_kbps: Option<u32>,
  requested_kbps: Option<u32>,
  limit_kbps: Option<u32>,
}

TransitBandwidthReply {
  available_kbps: Option<u32>,
}
~~~

Exact names may differ.

Rules:

- absent -> None;
- ASCII decimal positive integer only;
- zero, sign, whitespace, non-decimal, overflow reject;
- Mapping retains responsibility for duplicate/canonical structure;
- unknown keys follow the canonical forward-compatibility policy only; do not invent
  semantics;
- enforce m<=r, r<=l, m<=l where relevant;
- l legal only for HopRole::InboundGateway.

Do not parse raw string keys in i2pr-daemon.

## Scope B — Admission policy

Add deterministic caller-configured runtime-neutral limits for:

- enabled/disabled;
- accepting vs degraded/shutdown;
- max active transit;
- max pending admissions;
- max active per previous peer;
- max pending per previous peer;
- available shared bandwidth KBps;
- optional max per-tunnel allocation.

Keep detailed local rejection reasons internally, but preserve the current ECIES wire
fingerprint: the reply byte is only Accepted (0) or BandwidthRejected (30). Any
well-formed/authenticated request rejected by local admission policy emits code 30; do not
expose capacity, shutdown, per-peer, or overload reasons as distinct wire codes.
Malformed, unauthenticated, or undecodable request records fail closed without creating
transit state; their reply behavior must follow the existing short-build parser/crypto
contract rather than inventing an observable policy response.

## Scope C — Transactional short-build postprocessing

Create a production-intended runtime-neutral transaction:

~~~text
open/decrypt own record
-> strict ShortRequestRecord decode
-> validate role/options/time/next-hop
-> reserve admission
-> derive hop-local keys
-> construct role-specific registration
-> construct accepted/rejected ShortReplyRecord
-> seal reply
-> atomically commit only accepted registration
-> release reservation on every non-commit path
~~~

Reuse existing multirecord preprocessing for other slots. Do not add a second incompatible
record transformation algorithm.

Validate request time against caller-supplied now with a narrow documented skew policy; the
wire lifetime remains 600 seconds.

## Scope D — Dedicated TransitRegistry

Do not overload DataPlaneRegistry.

Required:

- keyed by receive tunnel id;
- explicit global capacity;
- per-peer active count;
- duplicate-id rejection;
- deterministic remove and expire(now);
- mutable role lookup for future TunnelData dispatch;
- no unbounded history or silent eviction of unexpired state;
- zero live state after reject or failed reply construction/sealing;
- role secrets dropped/zeroized to the extent supported by existing key types.

A role enum may wrap existing participant/IBGW/OBEP primitives; do not duplicate
transforms.

## Mandatory tests

Bandwidth/options:

1. empty;
2. m only;
3. r only;
4. l only on IBGW;
5. m+r;
6. r+l on IBGW;
7. m+r+l on IBGW;
8. m>r reject;
9. r>l reject;
10. m>l reject;
11. zero reject;
12. plus/minus reject;
13. whitespace reject;
14. non-decimal reject;
15. u32 overflow reject;
16. l on participant reject;
17. l on OBEP reject.

Admission/transaction:

18. disabled -> code 30 + no state;
19. degraded/shutdown -> code 30 + no state;
20. global active full -> code 30 + unchanged registry;
21. global pending full -> code 30 + unchanged registry;
22. per-peer active full -> code 30 + unchanged registry;
23. per-peer pending full -> code 30 + unchanged registry;
24. m above available -> code 30 + no registration;
25. accepted m/r -> b present and b>=m;
26. accepted no m/r may omit b;
27. duplicate receive id cannot replace;
28. timestamp outside skew rejects before commit;
29. participant registration owns exact receive/next tuple;
30. IBGW role registers correctly;
31. OBEP role registers correctly;
32. RNG failure -> rollback;
33. reply-seal failure -> rollback;
34. expiry at lifetime removes;
35. early expire does not remove;
36. previous-peer mismatch regression green;
37. exact duplicate TunnelData regression green;
38. Debug output does not expose key bytes.

Keep error-injection seams narrow and runtime-neutral.

## Explicitly out of scope

router_i2np behavior changes; socket ownership; next-hop lookup/delivery; queue workers;
config schema; metrics exporter; RouterInfo caps/router.version; public transit; floodfill;
creator scoring; long-build expansion; Java external execution.

## Failure / cancellation / restart / contention

No async tasks exist in this plan. Before registry commit every error must drop temporary
secrets and release reservation; rejected requests consume no active slot. Duplicate-id
contention is a deterministic registry conflict. Synchronization belongs to Plan 250.

Transit state is ephemeral and not persisted; restart starts empty.

## Compatibility / migration

No disk migration or dependency change. BuildOptions remains wire owner; transit typing is
a consumer. DataPlaneRegistry ownership remains unchanged. Export only the transit types a
later runtime owner needs.

## Exact verification commands

~~~text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
~~~

Any dedicated M11 checker added during implementation must be included before closure.

## Documentation on implementation

Update specs/support.toml with infrastructure-only evidence and advertised=false,
specs/protocols/05-tunnels.md with exact Plan 249 state, the Plan 249 status, transit
roadmap, and registry. Register the next plan only after the unblock audit.

## Acceptance criteria

Plan 249 passes only when bandwidth parsing/role semantics match the current spec,
admission is explicit/bounded, accepted requests produce canonical encrypted replies,
all well-formed admission-policy rejections yield code 30 without leaking the internal reason, insufficient m yields code 30 with zero state, accepted state commits atomically to a
dedicated bounded registry, all pre-commit failures roll back, virtual-time expiry works,
replay/previous-peer protections remain green, no daemon/config/exposure changes land, and
the complete focused + routine floor is green.

Closure remains infrastructure-only; M11 capability must not be marked passed.

## Stop conditions

Stop and create a follow-up rather than widening scope if current exact-pinned peers
require a mandatory unsupported legacy build path, safe integration requires runtime
ownership, typed interpretation would weaken Mapping semantics, an existing short-build
wire defect is found, or a new dependency is proposed for already-available functionality.
Do not expand the ECIES reply-code vocabulary beyond the current authoritative 0/30 set.

## Closure evidence

Record implementation commits, requirement-to-test matrix, focused/full test counts,
fmt/check/clippy/doc/deny/boundary outcomes, no-daemon/no-config/no-advertisement proof,
secret/logging and rollback/resource review, remaining runtime work, and unblock audit.

## Handoff notes for smaller models

Start and finish in i2pr-tunnel. Do not begin in router_i2np.rs.

Reuse ShortRequestRecord, BuildOptions, BuildCryptography, LayerKeys, and role constructors.
Do not redesign them. Treat admission as a transaction: nothing live before acceptance and
successful reply construction.

Do not infer semantics for unknown Mapping keys. Only m/r/l/b gain M11 meaning.

When the local foundation is green, stop. Daemon wiring is the next ownership/concurrency
plan.
