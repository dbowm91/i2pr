# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or any security-sensitive workload. NTCP2 stays experimental and non-advertised.

## Status

The local Milestone 6 product (destinations, garlic, LeaseSet2, Streaming) remains closed under corrected local-correctness semantics via [**Plan 134**](plans/134-status.md). Independent-router interoperability is tracked as external acceptance debt.

Milestone 7 / SAM has several strong retained results:

- [**Plan 146**](plans/146-status.md) passed bidirectional SAM 3.1 private-destination reference requalification against pinned Java I2P/i2pd behavior.
- [**Plan 147**](plans/147-status.md) landed the dedicated same-socket raw STREAM owner, TCP↔Streaming byte pump, actual Streaming `Established` wait, OS-CSPRNG runtime path, and supervised ACK/retransmit driver.
- [**Plan 149**](plans/149-status.md) passed the self-composing localhost STREAM product. `SESSION CREATE` builds the destination/LeaseSet2/bridge/local-delivery/runtime-driver composition before returning success, and the canonical black-box test drives the resulting path only through SAM TCP after listener startup.
- [**Plan 150**](plans/150-status.md) retains successful external-client core evidence with exact pinned `i2psam` and qualified pinned `i2plib.sam` client surfaces. Its broad final-closure interpretation was superseded by the stricter Plan 151 evidence gate.

[**Plan 151**](plans/151-status.md) closed Milestone 7 final localhost SAM acceptance with executable sibling-stream/backpressure/fault/CLOSE-RESET/FORWARD lifecycle evidence, focused M6 regressions, and hosted external-client evidence.

[**Plan 152**](plans/152-status.md) is the retained narrow Milestone 6 robustness corrective discovered by Plan 151: receiver-retention cap with ACK gating, coalesced duplicate ACKs, and sender ECIES ratchet-key trimming, without a wire-format change.

Current classification:

```text
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
plan_167 = passed-m9-i2cp-loopback-server-runtime
plan_168 = passed-m9-i2cp-message-data-plane
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_170_external_wire_data_plane = retained-passed
plan_170_final_acceptance = superseded-by-plan172
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective
plan_172 = passed-m9-i2cp-independent-leaseset2-lifecycle-corrective
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering

milestone7_local_product = passed-via-plan149
plan150_external_core_evidence = retained-passed
milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed
milestone8_ssu2_direction_a = passed-via-plan161
milestone8_ssu2_direction_b = passed-via-plan161
milestone8_ssu2_ledger = landed-via-plan161
milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_client_owned_destination = passed-via-plan166
milestone9_i2cp_loopback_server_runtime = passed-via-plan167
milestone9_i2cp_message_data_plane = passed-via-plan168
milestone9_i2cp_self_composed_local_product = passed-via-plan169
milestone9_i2cp_invalid_preamble_close = passed-via-plan171
milestone9_i2cp_independent_wire_data_plane = passed-via-plan170
milestone9_i2cp_independent_clients = passed-via-plan170-and-plan172
milestone9_i2cp_independent_leaseset2 = passed-via-plan172
milestone9_final_acceptance = closed-via-plan172
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = not-yet-passed
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_product_layer = milestone10-service-tunnels
next_executable_plan = 179
```

Milestone 8 is **closed** via [**Plan 161**](plans/161-status.md) within its bounded direct-interop scope. Directions A and B are genuinely proven against exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) over real loopback UDP, including authenticated session establishment, small and fragmented I2NP/DatabaseStore exchange with return DeliveryStatus traffic, cached-token behavior, malformed/resource rows, and the fail-closed external evidence lane. [**Plan 162**](plans/162-status.md) passed the narrow external-test lane correction. Public-network, NetDB/tunnel/destination, IPv6-external, PQ, SSU1, and Milestone 6 mixed-router interoperability remain outside that claim.

Milestone 9 / I2CP planning is now registered via [**Plan 163**](plans/163-status.md). [**Plan 164**](plans/164-status.md) passed the I2CP source/profile/wire foundation: pinned official I2CP sources with Java I2P 2.13.0 and go-i2cp references, the explicit M9 compatibility profile, runtime-neutral `i2pr-api::i2cp` bounded framing/message codecs with committed fixtures and a routine-CI vector checker, and no sockets or sessions. [**Plan 165**](plans/165-status.md) passed the connection/session/options state machines: typed `ConnectionStateMachine` with explicit message-family transitions, canonical `SessionConfig` signature/date/ceiling verification (Ed25519/X25519 only, ±30 s skew, signed region retained), bounded option disposition table and projection into `i2pr-client::DestinationConfig` with router-wide ceilings authoritative, bounded `SessionRegistry` with reserve/commit/rollback, reconfiguration taxonomy, and typed `I2cpAction` vocabulary. [**Plan 166**](plans/166-status.md) passed the M9 client-owned destination + LeaseSet2 bridge: `DestinationOwnership::RouterOwned` / `ClientOwned`, `DestinationPublic` (non-secret public destination), `InboundDecryptionCapability` (non-`Clone`, redacted, zeroized wrapper for the client-supplied X25519 inbound decryption secret), atomic `install_client_lease_set2` (signature + lease ownership + expiry + decryption-key match), typed `LeaseRequest` (sourced from real inbound tunnels, never synthesized) with `take_client_refresh_request`, and the `I2cpAction::RequestVariableLeaseSet` action. [**Plan 167**](plans/167-status.md) passed the M9 I2CP loopback server runtime: disabled-by-default `[i2cp]` config block (loopback-only, non-loopback bind rejected, bounded per-connection read/write/session resources), supervised Tokio listener in `i2pr-daemon::i2cp` that projects the typed `I2cpAction` vocabulary into the Plan 166 client-owned destination runtime, `0x2a` preamble + incremental `FrameDecoder` + multiple-frames-per-write + EOF/reset/cancel/timeout convergence on a single `teardown_connection` cleanup path, and twelve real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. [**Plan 168**](plans/168-status.md) passed the M9 I2CP message data plane: bounded per-session `SendMessage`/`SendMessageExpires` validation against the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam (no second routing stack), bounded `MessageStatus` correlation table sized strictly below the destination runtime's per-session outbound ceiling, `MessagePayload` inbound frames delivered only to the owning session's bounded queue and drained through a `tokio::sync::Notify` (sibling-isolation guaranteed), the cross-session local loopback shortcut for two destinations owned by active I2CP sessions, `DestLookup` resolving through the local destination registry with the documented typed not-found reply, `GetBandwidthLimits` returning the config-derived client ceiling and the documented neutral router values, and the `InboundPayloadFrame::WIRE_OVERHEAD_BYTES = 14` constant used by every backpressure assertion. Eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise every Plan 168 §11 case. SAM router-owned product regressions remain green. No reconfiguration, no `HostLookup`/`HostReply` resolution, and no independent-client evidence claim. [**Plan 169**](plans/169-status.md) then closed the self-composed local product and hardening pass. [**Plan 171**](plans/171-status.md) passed the narrow invalid-preamble close corrective on the common terminal path (retained). The M9 architecture reuses the existing destination product rather than creating an I2CP-specific router stack: runtime-neutral I2CP framing/session state lives in `i2pr-api`, TCP/Tokio listener ownership remains in `i2pr-daemon`, and `i2pr-client` now carries explicit `RouterOwned` (SAM) and `ClientOwned` (I2CP) ownership modes on a single destination runtime. I2CP remains experimental, disabled by default, and loopback-only throughout M9. [**Plan 170**](plans/170-status.md) closed Milestone 9 with independent Java I2P and Go I2CP clients exchanging digest-matched cross-client application traffic through the loopback daemon under fail-closed hosted evidence.

The `[sam]` config section remains disabled by default and loopback-only when enabled. No localhost SAM or I2CP result is router-to-router interoperability evidence.

Milestone 9 final acceptance is **closed via [Plan 172](plans/172-status.md)** (experimental, loopback-only): Plan 170 wire/data-plane evidence is retained-passed; Plan 172 proves the independent LeaseSet2 lifecycle with exact-pinned Java I2P 2.13.0 high-level `I2PSession.connect()` plus public go-i2cp `ProcessIO` lifecycle, real non-empty local zero-hop `RequestVariableLeaseSet`, client-signed Standard LeaseSet2 + X25519 capability installs, usability gated on install, and digest-matched bidirectional traffic after both installs (24 fail-closed rows).

Milestone 10 planning is registered via [**Plan 173**](plans/173-status.md). [**Plan 174**](plans/174-status.md) passed the service-tunnel foundation: runtime-neutral `i2pr-service-tunnels` crate (typed kinds, destination policy, Base32/alias validation, loopback-only listener/target shapes, central ceilings), the shared daemon Streaming pump reused by SAM with no second byte pump, and the strict disabled-by-default `[service_tunnels]` surface. [**Plan 175**](plans/175-status.md) passed the first complete M10 application service product: persistent router-owned service destinations (`ServiceDestinationStore`; versioned, atomic, secret-safe), a daemon-owned `ServiceTunnelManager` that binds loopback TCP for `generic-client` and a loopback Streaming listener for `generic-server`, reuse of the Plan 149 local destination product path and the Plan 174 shared byte pump, and a typed cross-tunnel local destination lookup so client/server tunnels owned by the same router do not need an external LeaseSet lookup. [**Plan 176**](plans/176-status.md) passed the first M10 application profile: the runtime-neutral `i2pr-service-tunnels::http` module (bounded HTTP/1.1 parser with smuggling rejection, hop-by-hop `Connection` removal, conservative privacy/header rewrite, `.i2p`-only target validation, bounded error response generation), the strict disabled-by-default `http-client` configuration surface, and the daemon HTTP proxy executor (`service_tunnels_http.rs`) that owns one loopback listener, parses headers under a bounded deadline, dispatches CONNECT to a port-policy-gated 2xx tunnel or rewrites and forwards an ordinary proxy request, and reuses the Plan 174 shared byte pump + Plan 149 destination product path. [**Plan 177**](plans/177-status.md) passed the second M10 application profile: the runtime-neutral `i2pr-service-tunnels::socks5` module (RFC 1928 no-auth greeting negotiation, CONNECT request parser with strict `.i2p`/DOMAINNAME-only target policy, deterministic RFC 1928 reply generator with a neutral loopback `127.0.0.1:0` bind, and bounded typed errors), the strict disabled-by-default `socks5-client` configuration surface, and the daemon SOCKS5 proxy executor (`service_tunnels_socks5.rs`) that owns one loopback listener, runs greeting + CONNECT negotiation under bounded deadlines, dispatches to the Plan 174 shared byte pump in opaque tunnel mode after real Streaming `Established`, and reuses the Plan 149 destination product path. SOCKS5 explicitly rejects clearnet/IP literals/IPv4/IPv6/localhost/mixed-suffix/non-CONNECT commands (no clearnet outproxy, no UDP ASSOCIATE, no BIND, no username/password auth, no Tor RESOLVE); success is sent only after Streaming reaches `Established` and same-read post-request bytes are preserved as first tunnel bytes. [**Plan 178**](plans/178-status.md) passed the third M10 application profile: the runtime-neutral `i2pr-service-tunnels::irc` module (bounded IRC/IRCv3 line parser with 512-byte core / 8191-byte tag-envelope / 4094-byte tag-data ceilings, typed command classifier with per-direction allowlist, client-to-network privacy rewrites for USER/PING/QUIT/PART, CTCP/DCC policy allowing ACTION while dropping DCC and unsupported CTCP) and the daemon IRC client tunnel executor (`service_tunnels_irc_client.rs`) that owns one loopback listener per `irc-client` spec and reuses the Plan 174 shared byte pump + Plan 149 destination product path. The IRC server profile, the full local-product composition / reconcile pass, and remote-service interop remain on the Plan 179/180/181 roadmap.

For the full plan hierarchy, MVP roadmap, and what's implemented vs. not, see [**`plans/README.md`**](plans/README.md).

## Workspace

```text
crates/
  i2pr-proto/               Bounded wire codecs, typed errors, no I/O
  i2pr-crypto/              Protocol-specific cryptographic wrappers
  i2pr-storage/             Atomic persistence and migration support
  i2pr-core/                Shared contracts, lifecycle, budgets, health
  i2pr-transport/           Transport-neutral link management
  i2pr-transport-ntcp2/     NTCP2 protocol implementation (no I/O)
  i2pr-transport-ssu2/      SSU2 v2 protocol (runtime-neutral), path validation/publication, peer-test/relay/introducers
  i2pr-runtime/             Tokio-owned supervision, cancellation, transport I/O
  i2pr-netdb/               RouterInfo + LeaseSet2 validation, store, lookup, publication
  i2pr-netdb-persist/       Persistent cache + bounded SU3 reseed ingestion
  i2pr-tunnel/              Tunnel identity, exploratory pool, ECIES-X25519 short-build, runtime-neutral data plane
  i2pr-client/              Destinations, ECIES-X25519-AEAD-Ratchet session layer, routing, I2P Streaming
  i2pr-api/                 Runtime-neutral application-protocol adapters (SAM 3.1 plus the M9 I2CP wire/profile foundation; no sockets)
  i2pr-service-tunnels/     Runtime-neutral M10 service-tunnel config/policy (no sockets; Plan 175 generic client/server tunnels composition lives in i2pr-daemon)
  i2pr-daemon/              CLI, configuration, composition, supervision, application listener ownership
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
tools/
  i2pr-interop/             Non-production interop launcher (test only)
```

The dependency direction is enforced by `scripts/check-dependency-direction.sh`. Architecture deep-dives live under [`docs/architecture/`](docs/architecture/); the index is [`docs/architecture/overview.md`](docs/architecture/overview.md).

## Build, test, lint

Requires Rust 1.95.0 (pinned via `rust-toolchain.toml`); MSRV is 1.88.

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Focused seams and the constrained-host lane are documented in [`AGENTS.md`](AGENTS.md).

## OpenCode skills

Loadable skill bundles under [`.opencode/skills/`](.opencode/skills/) cover the routine development seam ([`i2pr-local-dev`](.opencode/skills/i2pr-local-dev/SKILL.md)), documentation navigation ([`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md)), the closed NTCP2 interop lane ([`i2pr-ntcp2-interop`](.opencode/skills/i2pr-ntcp2-interop/SKILL.md)), the historical rootless sandbox ([`i2pr-rootless-sandbox`](.opencode/skills/i2pr-rootless-sandbox/SKILL.md)), and the historical Multipass recovery guest ([`i2pr-multipass-recovery`](.opencode/skills/i2pr-multipass-recovery/SKILL.md)). Load the matching skill before touching its surface.

## License

No license selected yet. Do not copy code from I2P+, i2pd, Emissary, or other routers until license compatibility is reviewed. Specifications and observed behavior may be used for clean-room implementation.
