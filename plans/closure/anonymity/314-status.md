# Plan 314 closure — Plan 310 multi-hop build contract and deterministic proof

Status: passed-plan310-multihop-build-contract-and-deterministic-three-hop-proof

Implementation commit: `2490c82` (`feat(anonymity): add exact-three destination build path`).

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Separate Destination request over the shared attempt owner | `DestinationBuildRequest` carries the ordered path and enforces exactly three peers in `ExploratoryBuildCoordinator::submit_destination`; it enters the same pending-attempt table, `ShortBuildPath`, state machine, reply routing, cancellation, and established-material extraction as the one-peer exploratory request. |
| Bounded validated-NetDB projection | `DestinationTunnelCoordinator::destination_peer_candidates` projects at most 256 records from its authoritative validated RouterInfo store. Candidates retain only RouterHash, the 32-byte build key, lowercased family text, and normalized address bucket/port facts. Invalid/missing family, key, or advertised endpoint data makes a record unqualified. |
| Java-derived exact-three selection | The selector uses the retained Plan 305 Java I2P 2.13.0 profile: distinct RouterHash, distinct family text, and distinct masked endpoint keys (IPv4 /16 or IPv6 /32 plus advertised port). It uses OS-seeded ChaCha8 in production, accepts injected CSPRNGs for deterministic tests, caps candidate enumeration at 256 and search at 4096 nodes, and returns typed scarcity/entropy/limit terminals without a shorter path. Family text is not certificate-verified, matching the Java source's documented verification TODO. Cross-pool endpoint-role exclusions are not claimed; the matrix assigns those to pool ownership, which Plan 315 addresses. |
| Selector-to-submission continuity | The daemon test selects from validated synthetic RouterInfos, preserves selected order in `DestinationBuildRequest`, and exercises the same ordered conversion to `ShortBuildPath` used by coordinator submission. `HopSpec::router_hash` enables the privacy-safe topology assertion. |
| Exact three-hop cryptographic trajectories | New inbound and outbound tunnel tests drive each of three hops through the production `MessageHopProcessor`, accepted responses, and ordinary reply postprocessor, then require `Established` material with exact direction, ordered RouterHashes, roles, and first-hop routing metadata. |
| Typed scarcity and no short-path fallback | Selector tests cover empty and insufficient-diversity stores, duplicates, family and address-mask exclusions, and missing metadata. Production provisions only after both outbound and inbound selections succeed; either typed selection error propagates before the first coordinator submission. No shorter request is constructed. |
| Exploratory one-peer compatibility | Existing exploratory one-hop install tests pass; its `BuildRequest` remains a one-peer request and uses the shared path core. |
| Documentation and capability scope | `docs/architecture/i2pr-daemon.md` documents the production request/selector path and states that deterministic local proof is not external three-router interoperability. No public-network anonymity claim was added. |

## Verification

Local commands and outcomes:

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` — 176 passed.
- `cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1` — 399 passed; the focused strict three-hop trajectory filter separately passed 2 tests.
- `cargo test --locked -p i2pr-client --all-targets -- --test-threads=1` — 207 passed.
- `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` — passed, including 311 library tests and existing exploratory one-hop tests; the full daemon target run completed without a reported failure.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — 3,296 passed, 34 ignored, 110 suites; 640.63 seconds.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed (0 doctests across 16 suites).
- `bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `bash scripts/check-m11-transit-boundaries.sh` — passed.
- `git diff --check` — passed.

Focused Plan 314 tests:

- `cargo test --locked -p i2pr-daemon --lib destination_peers -- --test-threads=1` — 7 passed.
- `cargo test --locked -p i2pr-daemon --lib selected_destination_order -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-daemon --lib destination_build_request_debug -- --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-tunnel --lib three_hop_trajectory_reaches_established -- --test-threads=1` — 2 passed.

No external i2pd test was required or run. The environment-gated external lanes remain outside this plan's acceptance.

## Compatibility, security, and limitations

- No dependency, wire format, persistent format, or service configuration changed.
- Candidate/request Debug output redacts RouterHashes and build keys. Selector diagnostics expose aggregate counts only.
- Selection state is ephemeral and reconstructed from validated NetDB records after restart. The shared pending-build bound remains authoritative.
- The current reference bootstrap may leave fewer than three qualified records; this returns typed scarcity before any build request. The qualified path never falls back to the reference peer or a shorter path.
- Java family labels are treated as signed RouterInfo metadata and are not certificate-verified. This matches the selected Java client behavior and does not establish verified family ownership or deployed-path diversity.
- The 256-record prefix is deliberately bounded. Its outcomes establish the selected local profile over the projected candidates, not global network diversity.

## Findings

- Critical: none.
- High: none.
- Medium: external three-router interoperability and Java-family certificate verification remain unproven/out of scope; no anonymity claim follows.
- Low: none.

## Unblock audit and roadmap disposition

Plan 315 listed Plan 314 as its only open dependency. Plan 309 and ADR 0030 are passed/accepted, and Plan 314 now supplies the stable exact-three request and established-material contract. Plan 315 is therefore moved to ready for the next sequential implementation step. Plans 311 and 312 remain blocked on Plan 315; Plan 313 remains blocked on Plan 312. Plan 308 remains independently blocked on ordinary-HTTP topology evidence. Plan 310 remains an immutable blocked record; this closure passes only the corrective build-contract/selector half.

Disposition: Plan 314 passed. Plan 315 is dependency-ready; the group-owned pool and Destination-operation work remains open under Plan 315.
