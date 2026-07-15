# Dependency Graph — Detail

The crate-layer dependency direction is enforced by
`scripts/check-dependency-direction.sh`. This document records the
allowlist and the rules so future reviewers can reason about new
edges.

## Allowlist

Production edges (reads as "may depend on"):

| Crate | May depend on |
| --- | --- |
| `i2pr-proto` | (no production crate) + `sha2`, `zeroize` |
| `i2pr-crypto` | `i2pr-proto` + `ed25519-dalek`, `x25519-dalek`, `sha2`, `subtle`, `zeroize`, `rand_core`, `thiserror` |
| `i2pr-storage` | `i2pr-crypto` + `rand_core`, `thiserror`, `zeroize` |
| `i2pr-core` | (zero deps) |
| `i2pr-transport` | `i2pr-core`, `i2pr-proto` |
| `i2pr-transport-ntcp2` | `i2pr-proto`, `i2pr-crypto`, `i2pr-transport` + `aes`, `chacha20poly1305`, `hmac`, `sha2`, `siphasher`, `thiserror`, `zeroize` |
| `i2pr-runtime` | `i2pr-core`, `i2pr-transport` + `tokio`, `tokio-util`, `futures-util`, `tracing` |
| `i2pr-daemon` | (top of graph; `i2pr-crypto`, `i2pr-storage` today; `i2pr-core`, `i2pr-proto`, `i2pr-runtime`, `i2pr-transport` declared but unused) + `clap`, `serde`, `toml`, `thiserror`, `tracing`, `tracing-subscriber` |
| `i2pr-testkit` (test-only) | every transport-and-runtime crate + `rand_chacha`, `rand_core`, `sha2`, `tokio` |

Reverse edges (i.e. "may NOT depend on"):

- `i2pr-proto` may not depend on any `i2pr-*` crate.
- `i2pr-crypto` may not depend on `i2pr-storage` (or above).
- `i2pr-core` may not depend on anything `i2pr-*`.
- `i2pr-transport` may not depend on `i2pr-transport-ntcp2`,
  `i2pr-runtime`, `i2pr-daemon`, `i2pr-testkit`, `i2pr-netdb`,
  `i2pr-tunnel`, `i2pr-client`.
- `i2pr-transport-ntcp2` may not depend on `i2pr-runtime`,
  `i2pr-daemon`, `i2pr-testkit`.
- `i2pr-runtime` may not depend on `i2pr-daemon`,
  `i2pr-transport-ntcp2` is allowed transitively only through
  runtime integration.
- **No production crate may depend on `i2pr-testkit`.**

## ASCII graph

```text
i2pr-proto  <- i2pr-crypto <- i2pr-storage
     ^             ^              ^
     |             |              |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon (composition root)
     ^             ^              ^
     |             |              |
     +-------------+   i2pr-transport-ntcp2
                          ^
                          |
                i2pr-proto + i2pr-crypto

i2pr-testkit (test/simulation only; may depend on transport crates;
              no production crate may depend on it)
```

## Runtime boundaries (orthogonal enforcement)

From `scripts/check-runtime-boundaries.sh`:

- No `unbounded_channel` / `UnboundedSender` / `UnboundedReceiver`
  in `i2pr-runtime`, `i2pr-testkit`, `i2pr-transport`,
  `i2pr-transport-ntcp2`.
- No `tokio::*`, `std::net`, `std::fs`, `TcpStream`, `TcpListener`,
  etc. in `i2pr-transport` / `i2pr-transport-ntcp2`.
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
