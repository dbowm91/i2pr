# Plan 296 closure — service-boundary implementation neutrality

Status: passed-anonymity-service-boundary-implementation-neutrality-and-leak-regression

Implementation commit: `3937a49` (`fix(anonymity): remove service boundary branding leaks`).
Reference authorities: Java I2P 2.13.0 at `9134f808337b401e8e53c73734c81fab04280c9d`; i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| Neutral default User-Agent | `DEFAULT_USER_AGENT_VALUE` is `MYOB/6.66 (AN/ON)`. At the pinned Java commit, `I2PTunnelHTTPClient.java` defines `UA_I2P` with this value. At the pinned i2pd commit, `HTTPProxy.cpp::SanitizeHTTPRequest` inserts the same value when `senduseragent` is false. |
| Resolved Destination Host | `target_for_remote_destination` derives the lowercase B32 label from `RemoteDestination.destination_hash`, preserves the requested port, and feeds that authority to the HTTP rewrite. Unit regression proves a local alias is absent and non-default port remains. Both pinned references have source paths converting resolved names to `.b32.i2p`. |
| CONNECT response branding | Daemon emits only `HTTP/1.1 200 Connection Established` plus the empty header terminator. A byte-level test rejects `i2pr`. |
| IRC stable reason branding | Explicit `ReplaceStable` removes the optional QUIT/PART reason; default Keep remains unchanged. The regression asserts the exact emitted `QUIT\r\n`. |
| Leak regression gate | `scripts/check-service-anonymity-boundaries.sh` scans application boundary sources and daemon tests for router-brand, version/build, local hostname/IP, path, and alias sentinels. Its self-check seeds each violation class and confirms detection. |
| Opaque forwarding + status language | Security model records that generic/SOCKS/CONNECT preserve application and TLS fingerprints and explicitly retains the global anonymity/privacy non-claim. No TLS inspection or broad claim was added. |

Sanitized transcript comparison (no destinations or payloads retained):

```text
before: User-Agent=i2pr/0.1; Host=<local-alias>; CONNECT Proxy-Agent=i2pr; IRC reason=i2pr
after:  User-Agent=MYOB/6.66 (AN/ON); Host=<resolved-hash>.b32.i2p; CONNECT has no Proxy-Agent; rewritten IRC reason omitted
```

## Verification

All commands ran locally on the implementation checkout. Pinned Rust 1.95.0 was used for Clippy/docs. The workspace test command required a per-process soft descriptor limit of 1024 because the host default is 256 and an existing SAM test deliberately holds 400 loopback connections.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed. |
| `cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1` | Passed: 222. |
| `cargo test --locked -p i2pr-daemon --lib service_tunnels_http::tests -- --test-threads=1` | Passed: 3. |
| `cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1` | Passed: 9. |
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed. |
| `bash scripts/check-service-anonymity-boundaries.sh` | Passed, including seeded negative self-check. |
| `cargo check --locked --workspace --all-targets` | Passed. |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed: 3,234; 31 ignored (109 suites), with `ulimit -n 1024` for this process. The same command at host default 256 failed in the existing 400-connection SAM timeout test; that exact test passed at 1024. |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed with Rust 1.95.0. RTK's default Cargo PATH selected Homebrew Rust 1.98.1 and its newer workspace-wide lints; rerunning with the pinned toolchain bin directory passed. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | Passed with Rust 1.95.0. |
| `cargo test --locked --workspace --doc` | Passed with Rust 1.95.0. |

## Compatibility, security, and limitations

- HTTP now uses resolved Destination identity in `Host`; local address-book aliases stay local.
- Explicit User-Agent Keep/Strip controls remain available. CONNECT remains opaque after establishment.
- IRC reason replacement retains its API surface but emits no replacement reason text.
- No dependency, wire-protocol, listener, destination-identity, or migration format changed.
- This plan does not establish HTTP profile equivalence, Streaming convergence, path diversity, browser anonymity, or production anonymity.
- Findings remaining: critical none; high none; medium none; low none.

## Unblock audit and roadmap disposition

Plan 296's closure satisfies the sole hard dependency of Plans 297, 298, and 300. Their other listed authorities remain closed: Plan 215 M10 service-tunnel product, existing M6 local Streaming authority, Plan 193 i2pd Streaming progression, and the Destination/NetDB/tunnel contracts they consume. Plans 297, 298, and 300 therefore move to ready. Plan 299 remains blocked on Plan 298's executed three-family evidence. Plan 301 remains blocked on Plans 297, 299, and 300. No other registry or roadmap dependency names Plan 296 as a hard/interface dependency.

Roadmap row: Plan 296 closed; Plans 297, 298, and 300 ready; Plans 299 and 301 remain dependency-blocked. No mainline or M12 status changes.
