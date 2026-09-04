# Dependency Graph — Detail

The crate-layer dependency direction is enforced by
`scripts/check-dependency-direction.sh`. This document records the
allowlist and the rules so future reviewers can reason about new
edges.

## Allowlist

Production edges (reads as "may depend on"). Dev-dependencies are excluded
from this production graph; they are allowed to support crate-local tests.

| Crate | May depend on |
| --- | --- |
| `i2pr-proto` | (no production crate) + `sha2`, `zeroize`, `flate2` |
| `i2pr-crypto` | `i2pr-proto` + `ed25519-dalek`, `x25519-dalek`, `sha2`, `subtle`, `zeroize`, `rand_core`, `thiserror`, `chacha20poly1305`, `hmac`, `elligator2` (replaces the retired `curve25519-elligator2 0.1.0-alpha.2`; Plan 131) |
| `i2pr-storage` | `i2pr-crypto` + `rand_core`, `thiserror`, `zeroize` |
| `i2pr-core` | (zero deps) |
| `i2pr-netdb` | `i2pr-crypto`, `i2pr-proto` + `thiserror`, `base64ct`, `flate2`, `sha2`, `x509-parser`, `zip`, `sad-rsa` (renamed from `rsa`) |
| `i2pr-netdb-persist` | `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, `i2pr-storage` + `thiserror` |
| `i2pr-transport` | `i2pr-core`, `i2pr-proto` |
| `i2pr-transport-ntcp2` | `i2pr-proto`, `i2pr-crypto`, `i2pr-transport` + `aes`, `chacha20poly1305`, `hmac`, `sha2`, `siphasher`, `thiserror`, `zeroize` |
| `i2pr-transport-ssu2` (Plans 155–157) | `i2pr-proto`, `i2pr-crypto`, `i2pr-transport` + `chacha20`, `chacha20poly1305`, `hmac`, `rand_core`, `sha2`, `thiserror`, `zeroize` |
| `i2pr-tunnel` | `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto` + `aes`, `cbc`, `chacha20`, `chacha20poly1305`, `rand_core`, `sha2`, `thiserror`, `x25519-dalek`, `zeroize` |
| `i2pr-runtime` | `i2pr-core`, `i2pr-transport`, `i2pr-transport-ntcp2` + `tokio`, `tokio-util`, `futures-util`, `tracing` |
| `i2pr-daemon` | `i2pr-crypto`, `i2pr-core`, `i2pr-proto`, `i2pr-runtime`, `i2pr-storage`, `i2pr-netdb`, `i2pr-netdb-persist`, `i2pr-transport`, `i2pr-tunnel` + `clap`, `serde`, `toml`, `thiserror`, `tracing`, `tracing-subscriber`, `rand_core`, `tokio` |
| `i2pr-testkit` (test-only) | every transport-and-runtime crate + `rand_chacha`, `rand_core`, `sha2`, `tokio` |
| `i2pr-client` (Plan 120 / Plan 121) | `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, `i2pr-tunnel` + `rand_chacha`, `rand_core`, `thiserror`, `zeroize` |
| `i2pr-api` (Plan 136) | `i2pr-client`, `i2pr-crypto`, `i2pr-proto` |
| `tools/i2pr-interop` (non-production) | `i2pr-crypto`, `i2pr-proto`, `i2pr-runtime`, `i2pr-storage`, `i2pr-transport`, `i2pr-transport-ntcp2` |

Reverse edges (i.e. "may NOT depend on"):

- `i2pr-proto` may not depend on any `i2pr-*` crate.
- `i2pr-crypto` may not depend on `i2pr-storage` (or above).
- `i2pr-core` may not depend on anything `i2pr-*`.
- `i2pr-netdb` may not depend on `i2pr-storage`, `i2pr-transport`,
  `i2pr-transport-ntcp2`, `i2pr-runtime`, `i2pr-daemon`, or
  `i2pr-testkit` (Plan 103; cache seam goes in `i2pr-storage`,
  composition goes in `i2pr-netdb-persist`).
- `i2pr-tunnel` may not depend on `i2pr-client` (Plan 120 composition
  flows from `i2pr-tunnel` upward only; the client reuses
  `BoundedTunnelPool` but does not live inside `i2pr-tunnel`).
- `i2pr-netdb` may not depend on `i2pr-client` (the LeaseSet2 validation
  path is consumed by the client; the client does not flow back into
  NetDB).
- `i2pr-client` may not depend on `i2pr-daemon`; the daemon is the
  future composition root, not a client library.
- `i2pr-api` may not depend on `i2pr-daemon`, `i2pr-runtime`,
  `i2pr-tunnel`, `i2pr-netdb`, `i2pr-storage`, or `i2pr-testkit`
  (Plan 136; sits between `i2pr-client` and `i2pr-daemon`).
- `i2pr-netdb-persist` may not depend on `i2pr-transport`,
  `i2pr-transport-ntcp2`, `i2pr-runtime`, `i2pr-daemon`, or
  `i2pr-testkit` (Plan 104).
- `i2pr-transport` may not depend on `i2pr-transport-ntcp2`,
  `i2pr-runtime`, `i2pr-daemon`, `i2pr-testkit`, `i2pr-netdb`,
  `i2pr-tunnel`, `i2pr-client`.
- `i2pr-transport-ntcp2` may not depend on `i2pr-runtime`,
  `i2pr-daemon`, `i2pr-testkit`.
- `i2pr-transport-ssu2` may not depend on `i2pr-runtime`,
  `i2pr-daemon`, `i2pr-testkit`, `i2pr-transport-ntcp2`,
  `i2pr-netdb`, `i2pr-tunnel`, or `i2pr-client` (Plans 155–157;
  runtime-neutral protocol/establishment/data phase only). Its approved
  `i2pr-crypto` edge (Plan 156) reuses checked X25519/HKDF/signature
  verification; transcript policy stays local.
- `i2pr-runtime` may not depend on `i2pr-daemon`; its direct
  `i2pr-transport-ntcp2` edge is the approved Plan 042 runtime composition
  boundary.
- **No production crate may depend on `i2pr-testkit`.**

## ASCII graph

```text
                              +--> i2pr-proto
                              |
                       i2pr-crypto
                              |
            +-----------------+------------------+
            |                 |                  |
        i2pr-storage     i2pr-netdb          i2pr-core
            |                 |
            v                 v
   i2pr-netdb-persist  (SU3/reseed validation)
        (cache + reseed
         composition;               +--> i2pr-tunnel (Milestone 5 substrate)
         Plan 104)                  |
                                    v
                               i2pr-transport
                                     |
                    +----------------+------------------+
                    |                                   |
             i2pr-transport-ntcp2               i2pr-transport-ssu2
                                                     (Plans 155-157: foundation
                                                      + establishment + data phase)
                                     |
                               i2pr-runtime
                                    |
                              i2pr-daemon (composition root)
                                    ^     |
                                    |     v
                              i2pr-client  i2pr-api (SAM 3.1)
                              (Plan 120)     (Plan 136)

i2pr-testkit (test/simulation only; may depend on transport crates;
              no production crate may depend on it)
```

Reading from the arrows: each upper layer is built on top of the lower
ones. `i2pr-netdb-persist` consumes `i2pr-storage` and `i2pr-netdb`; the
direction was previously drawn backwards in this document and was
corrected against `crates/i2pr-netdb-persist/Cargo.toml` during the
2026-08-27 architecture audit.

## Runtime boundaries (orthogonal enforcement)

From `scripts/check-runtime-boundaries.sh`:

- No `unbounded_channel` / `UnboundedSender` / `UnboundedReceiver`
  in `i2pr-runtime`, `i2pr-testkit`, `i2pr-transport`,
  `i2pr-transport-ntcp2`, `i2pr-transport-ssu2`.
- No `tokio::*`, `std::net`, `std::fs`, `TcpStream`, `TcpListener`,
  `UdpSocket`, etc. in `i2pr-transport` / `i2pr-transport-ntcp2` /
  `i2pr-transport-ssu2`.
- No `async fn`, `async_trait`, `i2pr-netdb`, `i2pr-tunnel`,
  `i2pr-client` in transport contracts (they stay synchronous).
- Only `i2pr-runtime` and `i2pr-testkit` may list `tokio` /
  `tokio-util` deps.
- `tokio::spawn` calls must keep an explicit owner (bound to
  `let`, `push(`, or `JoinSet`).

## Cross-references

- [Overview](overview.md)
- [`scripts/check-dependency-direction.sh`](../../scripts/check-dependency-direction.sh)
- [`scripts/check-runtime-boundaries.sh`](../../scripts/check-runtime-boundaries.sh)
- [`AGENTS.md`](../../AGENTS.md)
