# Plan 035 closure: supervised runtime link manager and NTCP2 addresses

Status: implemented as a bounded controlled-runtime subset. TCP ownership is
now available only through `i2pr-runtime` and local loopback/private tests;
public listeners, automatic publication, NetDB mutation, NAT discovery,
mixed-router interoperability, and capability advertisement remain excluded.
The runtime exposes exact I/O, admission, replay, backoff, listener, dial, and
joined-link owners, but does not claim a complete wire-level handshake or
authenticated data-phase driver; those composition tests are Plan 036 work.

## Service graph and ownership

```text
Ntcp2RuntimeService
  ├── BoundNtcp2Listener / ListenerHandle (owned accept child)
  ├── DialAdmission (bounded retry/backoff owner)
  ├── ReplayCache (fixed token capacity and deterministic expiry)
  └── LinkHandle
        ├── reader child
        └── writer child
```

`i2pr-runtime` owns Tokio TCP streams, listener sockets, bounded Tokio
channels, cancellation, deadlines, and child scopes. `InboundPermit` owns one
exact global/IP/subnet admission grant until handoff or drop. `LinkHandle` owns
one reader and writer child registration; dropping or closing the handle requests
cancellation, while `ChildScope` remains responsible for joining and accounting
the tasks. No raw socket or runtime channel crosses into the transport crates.

## Address and publication boundary

`i2pr-transport-ntcp2::address` parses NTCP/NTCP2 RouterAddress values without
DNS or socket calls. It validates literal IPv4/IPv6 hosts, ports 1..=65535,
exact I2P-base64 static keys and IVs, version/capability options, duplicate and
unknown options, and distinguishes `ConfiguredListenAddress` from
`ResolvedDialTarget`. Debug output redacts host/port and key/IV material.
Address and reachability values remain observation candidates; no RouterInfo or
NetDB mutation is performed.

## Admission, replay, backoff, duplicate, and deadline policy

| Surface | Bound/policy | Evidence |
| --- | --- | --- |
| Inbound pending | global, exact-IP, IPv4 `/24`, IPv6 `/64` | `InboundAdmission` tests |
| Replay | fixed capacity, expire-before-insert, full fails closed | `ReplayCache` tests |
| Dial retry | expiring bounded entries and capped exponential delay | `DialAdmission` implementation |
| Duplicate links | same direction retains existing; opposite direction uses local/remote hash ordering; loser drains/rejects | `DuplicateLinkPolicy` tests and ADR 0014 |
| I/O | exact/bounded read/write helpers with cancellation and capped deadlines | `read_exact`/`write_all_exact` |
| Link queue | bounded item and byte counters; typed queue/closed/cancel outcomes | `LinkHandle` implementation |

The duplicate rule is a deterministic local policy seam, not a Java I2P or
i2pd interoperability claim. Plan 036 must validate it against pinned
reference versions before any support or capability metadata changes.

## Runtime API inventory

- `Ntcp2RuntimeConfig`, `Ntcp2RuntimeLimits`, `Ntcp2RuntimeDeadlines`, and
  `IpPrefixPolicy` validate nonzero bounded configuration.
- `InboundAdmission` and `InboundPermit` enforce and release pre-crypto
  admission.
- `ReplayCache` and `DialAdmission` provide bounded runtime owners.
- `BoundNtcp2Listener`, `ListenerHandle`, `Ntcp2RuntimeService::listen`, and
  `Ntcp2RuntimeService::dial` own controlled socket setup.
- `LinkHandle` provides joined reader/writer child ownership and bounded sends.
- `read_exact` and `write_all_exact` map OS errors to fixed privacy-safe
  categories.
- `Ntcp2Event` and runtime snapshots contain only fixed categories, local IDs,
  coarse address families, and aggregate counters.

## Tests and cleanup evidence

The runtime unit suite covers limit validation, IPv4/IPv6 prefix accounting,
global/IP/subnet denial and permit release, replay capacity/expiry, and a
paused-time loopback listener with exact partial-I/O helper use, plus reader
and writer child joining after link close. The transport
suite covers deterministic duplicate direction policy and stale-safe manager
state. The NTCP2 suite covers literal address parsing, malformed options,
canonical base64, exact key/IV widths, IPv4/IPv6 family classification, and
redacted diagnostics. No test contacts an external address.

## Private-testnet harness status

The boundary and artifact requirements are documented in
`docs/private-testnet.md`. No Java I2P/i2pd process harness or mixed-router
artifact is committed in Plan 035. That evidence is explicitly deferred to
Plan 036.

## Support ledger

`ntcp2.runtime-link-manager` is recorded as `experimental` and
`advertised = false` in `specs/support.toml`; the same limitation is reflected
in `docs/protocol-support.md` and `specs/protocols/03-ntcp2.md`.

## Plan 036 prerequisites

Plan 036 must provide pinned mixed-router versions and isolated namespaces,
validate full initiator/responder handshake and authenticated I2NP exchange,
reconcile duplicate/padding/coalescing/address-publication behavior, and
record sanitized artifacts before any NTCP2 support or capability claim can be
advanced. Public-network traffic remains out of scope for malformed, stress,
or fault-injection testing.

## Local validation

The final handoff records the exact command results below:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-runtime --all-targets
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-testkit --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
git diff --check
```

All commands above passed on 2026-07-15. `cargo deny` reported the existing
duplicate `rand_core` lock entries as a warning; advisories, bans, and sources
still passed. The fuzz smoke command generated temporary corpus additions that
were removed before handoff.
