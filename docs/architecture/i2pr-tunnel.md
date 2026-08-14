# `i2pr-tunnel` — Milestone 5 exploratory tunnel substrate

Runtime-neutral tunnel identity, exploratory tunnel pool, build-record
layout surface, build-cryptography seam, and reply-path provider for
`i2pr`. The crate is the first Milestone 5 implementation surface
(Plan 107) and lands the substrate required to flip the Plan 106
NetDB seam from `BlockedExploratoryTunnelUnavailable` to `Available`
once a real inbound tunnel is registered.

> Status: experimental. Not production-ready. See `README.md` and
> `GUARDRAILS.md`.

## Purpose

`i2pr-tunnel` owns:

- typed tunnel identity ([`identity`](src/identity.rs)) —
  `TunnelId`, `TunnelDirection`, `TunnelRole`, `TunnelLifetime`,
  `TunnelState`, `TunnelPeer`;
- bounded exploratory tunnel pool configuration
  ([`config`](src/config.rs)) — `ExploratoryPoolConfig` with hard
  ceilings on `max_inbound`, `max_outbound`, `length_hops`,
  `lifetime_seconds`, `build_concurrency`, and
  `failure_threshold`;
- the deterministic [`pool::ExploratoryPool`](src/pool.rs) with
  bounded replacement, expiry, failure accounting, and the
  `select_inbound_reply_path` selector;
- the [`build::BuildRecordLayout`](src/build.rs) surface over the
  existing `i2pr_proto::DeferredBuildRecords` codec and the typed
  `BuildRequestKind` enumeration;
- the [`build_crypto::BuildCryptography`](src/build_crypto.rs) trait
  with the default `NoBuildCryptography` implementation that always
  returns `BuildCryptographyError::Unavailable` (the live
  ECIES-X25519 primitive lands in Plan 108+);
- the [`provider::ExploratoryPoolReplyPathProvider`](src/provider.rs)
  adapter that turns an `ExploratoryPool` into a
  `i2pr_netdb::ReplyPathProvider`.

The crate deliberately remains runtime-neutral: it does not open
sockets, does not perform DNS, does not spawn tasks, and depends only
on `i2pr-proto`, `i2pr-crypto`, `i2pr-core`, and `i2pr-netdb`.

## Module layout

| Module | Purpose |
| --- | --- |
| `identity` | `TunnelId`, `TunnelDirection`, `TunnelRole`, `TunnelLifetime`, `TunnelState`, `TunnelPeer` |
| `config` | Bounded `ExploratoryPoolConfig` with hard ceilings |
| `pool` | Deterministic `ExploratoryPool` with bounded replacement, expiry, and failure accounting |
| `build` | `BuildRecordLayout` (Short/Variable) over `DeferredBuildRecords`; `BuildRequestKind` wire-type markers |
| `build_crypto` | `BuildCryptography` trait + `LayerKeys` zeroizing wrapper + `NoBuildCryptography` default |
| `provider` | `ExploratoryPoolReplyPathProvider` that turns the pool into a `ReplyPathProvider` |

## Dependency boundary

```text
i2pr-tunnel -> i2pr-proto, i2pr-crypto, i2pr-core, i2pr-netdb + thiserror, sha2, zeroize
```

Runtime-neutral: no `tokio`, no `std::net`, no `std::fs`, no sockets,
no DNS. The plan-of-record
[`plans/107-milestone-5-exploratory-tunnel-substrate.md`](../../plans/107-milestone-5-exploratory-tunnel-substrate.md)
explicitly forbids adding any of these dependencies.

## Plan 107 surfaces

- `identity::TunnelId`, `identity::TunnelDirection`,
  `identity::TunnelRole`, `identity::TunnelLifetime`,
  `identity::TunnelState`, `identity::TunnelPeer` — bounded typed
  values that describe a tunnel slot.
- `config::ExploratoryPoolConfig` with the documented hard
  ceilings and the `balanced()` constructor.
- `pool::ExploratoryPool` — deterministic pool with bounded
  replacement, expiry, failure accounting, and the
  `select_inbound_reply_path` selector.
- `pool::TunnelSlot` — monotonic slot identifier.
- `pool::TunnelRegistration` — the typed registration record.
- `pool::RegisterOutcome`, `pool::RegisterError`,
  `pool::PoolFullError`, `pool::PoolError` — the typed failure
  vocabulary.
- `build::BuildRecordLayout`, `build::BuildRequestKind`,
  `build::BuildRecordLayoutError` — the build-record layout
  surface.
- `build_crypto::BuildCryptography` trait,
  `build_crypto::BuildCryptographyError`,
  `build_crypto::LayerKeys`, `build_crypto::NoBuildCryptography` —
  the build-cryptography seam.
- `provider::ExploratoryPoolReplyPathProvider` — the
  `ReplyPathProvider` adapter.

## Key contracts

- `TunnelId::new(value)` rejects zero; the pool refuses zero
  identifiers and the lookup state machine refuses zero reply-path
  tunnel IDs.
- `ExploratoryPoolConfig::try_new` rejects any value that exceeds
  the documented hard ceilings; the configuration is bounded at
  every layer.
- `ExploratoryPool::register_inbound` returns
  `RegisterError::Full { kind, registration }` when the pool is at
  capacity; the registration is **not** silently dropped.
- `ExploratoryPool::select_inbound_reply_path(now)` returns the
  oldest valid inbound tunnel; expired and failed tunnels are never
  returned.
- `BuildCryptography::seal` and `BuildCryptography::open` always
  validate inputs first; the default `NoBuildCryptography`
  implementation rejects every call.
- `LayerKeys` is `Zeroize`-derived and has no `Debug`, `Clone`, or
  serde implementations.

## Plan 107 acceptance criteria

Plan 107 closes when:

1. `i2pr-tunnel` compiles and passes its own unit tests.
2. The pool enforces the configured `max_inbound` ceiling and
   returns typed errors when full.
3. The reply-path provider turns the pool into a
   `ReplyPathProvider`.
4. `NetDbSeam::path_status` reports `Available` when the injected
   provider has at least one valid inbound tunnel.
5. The unit tests prove pool selection is stable for the same seed
   and that expired tunnels are filtered out.
6. The cryptographic surface is a typed seam only; no live build is
   exercised, and `BuildCryptography::seal` returns `Unavailable`
   from at least one unit test.

## Distinctive design choices

1. **Tunnel IDs reject zero.** The pool and the lookup state
   machine refuse zero; the value is reserved by I2P for "no
   tunnel".
2. **Configuration is bounded at every layer.** The
   `ExploratoryPoolConfig::try_new` constructor rejects every value
   that exceeds the documented hard ceiling; the configuration
   cannot become a vector for unbounded growth.
3. **Time is injected.** `advance_time` is the only way to surface
   expiry; the pool never reads the wall clock.
4. **Reply-path provider is a trait, not a concrete type.** The
   seam consumes `Box<dyn ReplyPathProvider>`, so the production
   pool implementation can be swapped without rewrites.
5. **Build cryptography is a trait, not a concrete type.** The live
   ECIES-X25519 primitive lands in Plan 108+; the default
   implementation always returns `Unavailable` so Plan 107 cannot
   silently claim cryptographic coverage it does not have.
6. **No `tokio`, no `std::net`, no `std::fs`.** The crate is
   runtime-neutral; the live build orchestration will live in
   `i2pr-runtime` and depend on `i2pr-tunnel`, never the other way
   around.

## Out of scope (Plan 108+)

- live ECIES-X25519 build encryption primitive and consensus
  against pinned Java I2P / i2pd vectors;
- live mixed-router tunnel build execution;
- transit participation (Milestone 11);
- destination-specific tunnel pools (Milestone 6);
- LeaseSet publication from tunnel records.

## Cross-references

- [`i2pr-netdb`](i2pr-netdb.md) — owns the `ReplyPath` token and
  the `ReplyPathProvider` trait that the new adapter implements.
- [`i2pr-daemon`](i2pr-daemon.md) — composes the NetDB seam; the
  seam consumes the reply-path provider when one is injected.
- Plan-of-record: [`plans/107-milestone-5-exploratory-tunnel-substrate.md`](../../plans/107-milestone-5-exploratory-tunnel-substrate.md).
