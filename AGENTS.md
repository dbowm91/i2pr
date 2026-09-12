# Repository Guidelines

`i2pr` is an experimental Rust I2P router. **Not production-ready.** Do not
use it for anonymity, privacy, censorship resistance, or any security-sensitive
workload. NTCP2 remains experimental and non-advertised; the production daemon
does not activate NTCP2. SSU2 v2 has a localhost UDP runtime and Plan 161 has
proven both direct authenticated IPv4 directions against exact-pinned i2pd 2.61.0;
Plan 186 has proven daemon-owned NetDB lookup/publication over exploratory
tunnels against the same pin with no LeaseSet2/Streaming claim; no public
advertisement, public-network participation, broad router interoperability,
or Milestone 6 interoperability is claimed. Plan 187 landed the local
destination message plane (27 unit + 9 live two-role rows) with 7
remote rows blocked on the build-reply interop gap pending Plan 188.
Plan 190 isolates and corrects the inbound NetDB reply-path metadata
defect that left 5/7 destination rows blocked after the Plan 188
installs; the corrected reply path is the next external lane run.
Plan 191 ran the inbound-delivery layer and stopped at boundary E
(i2pd-compatible I2CP-style Data body wire-format defect); the
narrower follow-up is Plan 192 (next executable).

## Read first

Always read these before changing code or answering questions about state:

1. `README.md` — current product/plan status.
2. `GUARDRAILS.md` — non-negotiable engineering/security/interoperability constraints.
3. `CONTRIBUTING.md` — local quality/runtime/test conventions.
4. [`plans/README.md`](plans/README.md) and the newest relevant status files.
5. `specs/support.toml` plus `specs/CONFORMANCE.md` for support/evidence claims.

Current authority:

```text
Plan 146 = passed private-destination reference evidence
Plan 147 = raw-driver implementation retained
Plan 149 = passed self-composing localhost SAM product
Plan 150 = external-client core evidence retained-passed
Plan 150 final acceptance = superseded-by-plan151
Plan 151 = passed final acceptance evidence correction
Plan 152 = passed narrow M6 streaming corrective
Plan 153 = passed post-M7 authority/CI hygiene
Milestone 7 SAM localhost = closed (experimental, loopback-only)
Milestone 8 roadmap = Plan 154
Plan 155 = passed SSU2 v2 protocol foundation
Plan 156 = passed SSU2 v2 handshake/token/RouterInfo establishment
Plan 157 = passed SSU2 v2 data-phase reliability/fragmentation
Plan 158 = passed SSU2 v2 UDP runtime and local session product
Plan 159 = passed SSU2 v2 path validation/publication/transport selection
Plan 160 = passed SSU2 v2 peer test and relay reachability
Plan 161 = passed M8 SSU2 independent IPv4 interop and final closure
Plan 162 = passed external-test lane isolation / routine-CI corrective
Plan 163 = registered M9 I2CP roadmap
Plan 164 = passed M9 I2CP protocol and wire foundation
Plan 165 = passed M9 I2CP connection/session/options
Plan 166 = passed M9 I2CP client-owned destination + LeaseSet2 bridge
Plan 167 = passed M9 I2CP loopback server runtime
Plan 168 = passed M9 I2CP message data plane
Plan 169 = passed M9 I2CP self-composed local product and hardening
Plan 170 external wire/data-plane = retained-passed
Plan 170 final acceptance = superseded-by-plan172
Plan 171 = passed M9 I2CP invalid-preamble close and CI corrective (retained)
Plan 172 = passed M9 I2CP independent LeaseSet2 lifecycle corrective
Milestone 9 I2CP final acceptance = closed-via-plan172 (experimental, loopback-only)
Plan 173 = registered M10 service-tunnels roadmap
Plan 174 = passed M10 service-tunnel foundation and shared stream runtime
Plan 175 = passed M10 generic client/server service tunnels and persistent server destinations
Plan 176 = passed M10 HTTP `.i2p` proxy and CONNECT
Plan 177 = passed M10 SOCKS5 `.i2p` CONNECT proxy
Plan 178 = passed M10 IRC `.i2p` client profile and privacy filtering
Plan 179 = passed M10 IRC `.i2p` server profile and authenticated peer hostname
Plan 180 = passed M10 service-tunnel composition, reconcile, and hardening
Plan 181 = blocked-by-m6-mixed-router-streaming-blocker (local rows passed; remote gate pending plan183)
Plan 182 = passed M10 local-delivery corrective
Plan 183 = registered M6 mixed-router streaming interop program
Plan 184 = passed M6 authenticated I2NP runtime and reference preflight
Plan 185 = passed M6 live one-hop exploratory tunnels and liveness
Plan 186 = passed M6 mixed-router NetDB lookup and publication
Plan 187 = blocked-by-m6-build-reply-interop-gap (local rows passed; 5/7 flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)
Plan 188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190; 5/7 destination rows flipped; 2 inbound-delivery rows blocked on plan192; 2 ordering rows passed)
Plan 189 = registered M6 Java second-family qualification and mixed-router closure (blocked-by-plan188-plan190-plan191-plan192; §8 cross-family ledger/checker/workflow landed, no second-family Java row yet)
Plan 190 = passed M6 inbound NetDB reply-path tunnel-ID corrective (local rows passed; 3 destination rows flipped blocked -> passed in fresh external run)
Plan 191 = stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked; 2 ordering rows flipped passed; narrower follow-up registered)
Plan 192 = registered M6 i2pd-compatible I2CP-style Data body wire-format corrective (next executable; 9-byte short-transport inner envelope + I2CP-style Data body + STYLE=RAW / DATAGRAM VERSION=3 SAM session)
Milestone 10 foundation = passed-via-plan174 (no listener yet)
Milestone 10 generic tunnels = passed-via-plan175 (profile; byte round-trip proven-via-plan182)
Milestone 10 HTTP proxy = passed-via-plan176 (profile; byte round-trip proven-via-plan182)
Milestone 10 SOCKS5 = passed-via-plan177 (profile; byte round-trip proven-via-plan182)
Milestone 10 IRC client = passed-via-plan178 (profile; byte round-trip proven-via-plan182)
Milestone 10 IRC server = passed-via-plan179 (profile; byte round-trip proven-via-plan182)
Milestone 10 local product = passed-via-plan180-and-plan182 (reconcile model + local-delivery driver)
Milestone 10 local round-trip = passed-via-plan182 (generic/HTTP/SOCKS/IRC success paths)
Milestone 10 independent application clients = local-rows-passed-plan181-not-closed
Milestone 10 remote service interop = not-yet-passed
Milestone 10 final acceptance = not-yet-closed
M6 authenticated I2NP preflight = passed-via-plan184 (no tunnel/NetDB/Streaming claim)
M6 exploratory one-hop tunnels = passed-via-plan185 (no multi-hop / LeaseSet2 / Streaming claim)
M6 NetDB lookup/publication = passed-via-plan186 (no LeaseSet2 / Streaming claim)
M6 destination local product = passed-via-plan187 (27 unit + 9 live rows; no remote claim)
M6 destination remote interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-blocked-plan191-then-plan192
M6 inbound NetDB reply-path correction = passed-via-plan190 (typed route + adapter; 3 destination rows flipped blocked -> passed in fresh external run)
M6 inbound destination delivery boundary E = stopped-pending-plan192 (i2pd-compatible I2CP-style Data body wire-format)
M6 inbound destination delivery = blocked-pending-plan192
M6 mixed-router cross-family ledger = landed-via-plan189 (i2pd-only-runs; java-second-family-deferred-until-plan188-plan192-closes)
next_executable_plan = 192 (resolve inbound-delivery boundary E wire-format)
next product layer = m6-mixed-router-leaseset2
```

For current SSU2 interop work, read in this order:

1. [`plans/162-status.md`](plans/162-status.md)
2. [`plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`](plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md)
3. [`plans/161-status.md`](plans/161-status.md)
4. [`plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`](plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md)
5. [`plans/160-status.md`](plans/160-status.md) through [`plans/155-status.md`](plans/155-status.md)
6. [`plans/154-status.md`](plans/154-status.md) for the M8 roadmap authority.

Read in this order for SAM work:

1. [`plans/151-status.md`](plans/151-status.md)
2. [`plans/151-m7-sam31-final-acceptance-evidence-correction.md`](plans/151-m7-sam31-final-acceptance-evidence-correction.md)
3. [`plans/150-status.md`](plans/150-status.md) — retained external-client core evidence, not final closure
4. [`plans/149-status.md`](plans/149-status.md) — passed product-composition authority
5. Plans 146–148 for historical/reference context.

Read in this order for Milestone 9 I2CP work:

1. [`plans/172-status.md`](plans/172-status.md) — closed final acceptance
2. [`plans/170-status.md`](plans/170-status.md)
3. [`plans/170-m9-i2cp-independent-clients-and-final-closure.md`](plans/170-m9-i2cp-independent-clients-and-final-closure.md)
4. [`plans/171-status.md`](plans/171-status.md)
5. [`plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md`](plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md)
6. [`plans/169-status.md`](plans/169-status.md)
7. [`plans/169-m9-i2cp-self-composed-local-product-and-hardening.md`](plans/169-m9-i2cp-self-composed-local-product-and-hardening.md)
8. [`plans/168-status.md`](plans/168-status.md)
9. [`plans/168-m9-i2cp-message-data-plane.md`](plans/168-m9-i2cp-message-data-plane.md)
10. [`plans/167-status.md`](plans/167-status.md)
11. [`plans/167-m9-i2cp-loopback-server-runtime.md`](plans/167-m9-i2cp-loopback-server-runtime.md)
12. [`plans/166-status.md`](plans/166-status.md)
13. [`plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md`](plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md)
14. [`plans/165-status.md`](plans/165-status.md)
15. [`plans/165-m9-i2cp-connection-session-and-options.md`](plans/165-m9-i2cp-connection-session-and-options.md)
16. [`plans/164-status.md`](plans/164-status.md)
17. [`plans/164-m9-i2cp-protocol-and-wire-foundation.md`](plans/164-m9-i2cp-protocol-and-wire-foundation.md)
18. [`plans/163-m9-i2cp-roadmap.md`](plans/163-m9-i2cp-roadmap.md) — planning authority
19. Milestone 9 is closed via Plan 172.

Read in this order for Milestone 10 service-tunnel work:

1. [`plans/181-status.md`](plans/181-status.md) — blocked M10 independent acceptance (current authority: local rows green, remote gate pending Plan 183)
2. [`plans/181-m10-independent-application-and-service-interop-final-closure.md`](plans/181-m10-independent-application-and-service-interop-final-closure.md)
3. [`plans/182-status.md`](plans/182-status.md) — passed M10 local-delivery corrective
4. [`plans/182-m10-local-delivery-corrective.md`](plans/182-m10-local-delivery-corrective.md)
5. [`plans/183-status.md`](plans/183-status.md) — registered M6 mixed-router program (next)
6. [`plans/183-m6-mixed-router-streaming-interop-program.md`](plans/183-m6-mixed-router-streaming-interop-program.md)
7. [`plans/180-status.md`](plans/180-status.md) — passed M10 composition, reconcile, and hardening
8. [`plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md`](plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md)
9. [`plans/179-status.md`](plans/179-status.md) — passed IRC `.i2p` server profile and authenticated peer hostname
10. [`plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md`](plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md)
11. [`plans/178-status.md`](plans/178-status.md) — passed IRC `.i2p` client profile and privacy filtering
12. [`plans/178-m10-irc-client-profile-and-privacy-filtering.md`](plans/178-m10-irc-client-profile-and-privacy-filtering.md)
13. [`plans/177-status.md`](plans/177-status.md) — passed SOCKS5 `.i2p` CONNECT
14. [`plans/177-m10-socks5-i2p-connect-proxy.md`](plans/177-m10-socks5-i2p-connect-proxy.md)
15. [`plans/176-status.md`](plans/176-status.md) — passed HTTP `.i2p` proxy + CONNECT
16. [`plans/176-m10-http-i2p-proxy-and-connect.md`](plans/176-m10-http-i2p-proxy-and-connect.md)
17. [`plans/175-status.md`](plans/175-status.md) — passed generic client/server tunnels
18. [`plans/175-m10-generic-client-server-service-tunnels.md`](plans/175-m10-generic-client-server-service-tunnels.md)
19. [`plans/174-status.md`](plans/174-status.md) — passed foundation
20. [`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
21. [`plans/173-status.md`](plans/173-status.md) — roadmap authority
22. [`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md)
23. Do not claim M10 final closure or independent router interop:
    Plan 181 is blocked by the retained M6 mixed-router Streaming
    debt (`m6-mixed-router-streaming-blocker`); Plan 183 owns the
    corrective program and Plan 181 resumes only after it produces
    passing remote rows.

Plan 171 corrective (retained): every terminal pre-session I2CP
rejection terminates TCP explicitly on the common per-connection
path (`handle_connection` calls `stream.shutdown()` before
`teardown_connection` + `drop_connection`; shutdown failure never
blocks cleanup; no frame is written for an invalid first byte).
`wrong_protocol_byte_is_closed` stays strict — timeout is failure —
and proves a 24-iteration rejection trajectory with zeroed
baselines plus a subsequent valid client; the paused test waits
via a bounded yield-pump/`try_read` drain (no virtual-time
timeout, which raced server polling intermittently on macOS)
and the non-paused
`wrong_protocol_byte_is_closed_real_time` companion separates
product-close evidence from paused-clock timer behavior. Never
revert to drop-timing dependence to make a test convenient.

Do **not** trust prose that disagrees with executable tests/scripts. The newest
explicit superseding status wins when historical records conflict.

## Workspace layout

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-crypto` — protocol cryptographic wrappers.
- `i2pr-storage` — identity/key persistence.
- `i2pr-core` — shared runtime-neutral contracts.
- `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` — runtime-neutral transport/link codecs.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo/LeaseSet2 validation and local storage.
- `i2pr-runtime` — production owner of Tokio, sockets, timers, channels, cancellation; also contains non-production external transport test drivers under `tests/`.
- `i2pr-daemon` — CLI/composition root and SAM runtime/socket ownership.
- `i2pr-tunnel` — runtime-neutral exploratory/tunnel substrate.
- `i2pr-client` — destination lifecycle, LeaseSet2, ECIES session/routing, Streaming.
- `i2pr-api` — runtime-neutral SAM 3.1 parsing/state/registry/FORWARD/NAMING plus the M9 I2CP wire/profile foundation (no sockets).
- `i2pr-service-tunnels` — runtime-neutral M10 service-tunnel config/policy (no sockets; foundation only, no listener yet).
- `i2pr-testkit` — deterministic simulation/fault fixtures; no production crate may depend on it.
- `tools/i2pr-interop` — non-production test launcher.

Architecture details live under `docs/architecture/`; ADRs live under
`docs/adr/`.

## Current SAM architecture

Retain the working Plan 149 product structure unless a newer executable test
exposes a concrete defect:

- `SESSION CREATE` transactionally builds the supported localhost product;
- one `Arc<DestinationIdentity>` allocation is shared by destination runtime and SAM bridge;
- `SamLocalProductFabric` creates the localhost LeaseSet2/outbound/inbound-delivery material with OS CSPRNG;
- local peer LeaseSet2 is resolved/validated through the SAM-owned directory;
- one supervised per-destination runtime driver is started automatically;
- raw CONNECT/ACCEPT permanently transfers socket ownership out of the line parser;
- same-read command+raw bytes are preserved;
- `SILENT` behavior and non-silent ACCEPT peer Destination metadata are byte-exact;
- `DeliverySweepCounters` surface typed bounded delivery failure accounting.

The canonical product-composition test is
`crates/i2pr-daemon/tests/sam_stream_self_composed.rs`. After listener startup,
it drives behavior only through TCP/SAM and must not invoke private bridge,
LeaseSet2, tunnel-factory, driver, delivery, or byte-moving setup APIs.

## Plan 151 scope (retained)

Plan 151 was an acceptance/evidence correction, not a SAM rewrite. It added
executable proof for the items Plan 150 claimed but did not fully run:

- no synthetic/unconditional `passed` evidence rows;
- two simultaneous sibling streams and close-one/keep-one isolation;
- slow-reader and slow-writer boundedness;
- deterministic DATA-drop, ACK-drop, duplicate, reorder, corruption, and retransmission-ceiling behavior beneath real SAM sockets;
- CLOSE/RESET/control-session cleanup;
- complete FORWARD lifecycle/negative matrix;
- explicit focused Plan 127–134 regression commands;
- current-head routine CI plus manual external-client workflow.

If a new test exposes an M6 Streaming protocol defect, write a narrow protocol
corrective rather than weakening the test. That stop fired once as Plan 152
(passed narrow M6 corrective, no wire change).

## Plan 153 scope (closed)

Plan 153 was documentation and CI hygiene only: it normalized the
authoritative `plans/152-status.md`, removed stale Plan 151/152 prose,
added the Plan 152 closure pointer to the support ledger, and enforced
the Plan 151 SAM evidence-integrity checker in routine Linux CI and
the manual SAM external workflow. No `crates/` or `Cargo.lock` changes
were made.

## Plan 161 interop evidence (passed, retained)

Plan 161 has proven both independent direct SSU2 v2 directions against
exact-pinned i2pd (see `plans/161-status.md` for the full matrix and
the 24-criterion checklist):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
direction = i2pr initiator -> i2pd responder
transport = real loopback UDP
```

The passed trajectory includes tokenless Retry establishment, mutual
authentication, one small and one fragmented DatabaseStore from i2pr to i2pd,
DeliveryStatus traffic back over the authenticated session, and graceful
resource cleanup. Independent comparison exposed three real handshake
transcript divergences that were corrected in Plan 161; do not revert them to
make loopback tests match older fixtures.

Direction B, the cached-token/malformed rows, and the final
ledger/checker/workflow lane have passed locally and hosted (see
`plans/161-status.md`); Java I2P is recorded nonblocking secondary
debt. Direction A+B evidence
does not imply public I2P or broad router interoperability. Milestone 8
is closed within this bounded scope; the next layer is milestone9-planning.

## Plan 162 scope (closed)

Plan 162 was a narrow test-lane/CI corrective. Routine CI run
`33915994884` on Plan 161 direction-A head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both Ubuntu and macOS
quality jobs because ordinary workspace execution ran
`crates/i2pr-runtime/tests/ssu2_independent.rs` without an external i2pd
environment. Dependency policy and MSRV passed.

Required correction:

- keep the Plan 161 external test compiled by all-target checks;
- mark the environment-dependent test explicitly ignored for ordinary libtest execution;
- run it only with explicit `--ignored --exact` in the external lane;
- keep missing external environment a hard failure after explicit selection;
- do not add filename filtering, `|| true`, `continue-on-error`, fake peer values, or broad workspace exclusions;
- re-run direction A against the exact same pinned i2pd after gating;
- require ordinary Ubuntu + macOS CI green on the Plan 162 closing head;
- then return directly to Plan 161. These conditions passed on implementation
  commit `624e8cce177040674376163160cfbda47e6a60fe`, hosted CI run
  `33941941145`.

Do not change SSU2 production source or wire semantics inside Plan 162. If the
explicit external re-run exposes a real protocol defect, stop and create a
separate narrow protocol corrective.

## Hard boundaries

These remain non-negotiable and are CI-enforced where applicable:

- preserve workspace dependency direction; no production crate depends on `i2pr-testkit`;
- no unbounded channels/queues introduced for convenience;
- Tokio/socket/timer/task ownership stays in runtime/daemon layers;
- every spawned task has explicit ownership/cancellation;
- SAM remains loopback-only and disabled by default;
- no root/sudo, privileged container, network namespace, VM, systemd, or public-I2P requirement for current acceptance work;
- no external-client/reference patching or vendoring;
- no private SAM `PRIV`, signing seed, SSU2 static/session secret, token, or raw private application payload in logs/evidence;
- do not make `DestinationIdentity: Clone` or reconstruct a second private identity for the SAM bridge;
- an external interoperability test may be ignored in routine CI only when its dedicated lane explicitly opts in and remains fail-closed if required external configuration is absent.

Static boundary scripts are the source of truth. Fix violations; do not weaken
the scripts.

## Build/test floor

Run from repository root before handoff:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
 bash scripts/check-dependency-direction.sh
 bash scripts/check-runtime-boundaries.sh
 bash scripts/check-service-tunnel-boundaries.sh
 bash scripts/check-fixture-manifest.sh
 bash scripts/check-ntcp2-vectors.sh
 bash scripts/check-ssu2-vectors.sh
 bash scripts/check-i2cp-vectors.sh
 bash scripts/check-ntcp2-interoperability.sh
 bash scripts/check-constrained-host-lane-boundary.sh
 bash scripts/check-sam-acceptance-evidence.sh
 bash scripts/check-ssu2-acceptance-evidence.sh
 bash scripts/check-i2cp-acceptance-evidence.sh
 bash scripts/check-service-tunnel-acceptance-evidence.sh
 bash scripts/check-m6-mixed-router-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Focused SAM seams currently include:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
```

Plan 151 added its narrowly named final-acceptance suite
(`crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs`) rather than bloating
the existing self-composed file. The Plan 151 evidence-integrity checker
(`scripts/check-sam-acceptance-evidence.sh`) is enforced in routine Linux CI
and the manual SAM external workflow; do not weaken it to make CI pass.

 Plan 176 added the runtime-neutral `i2pr-service-tunnels::http` module
 (bounded HTTP/1.1 parser with smuggling rejection, hop-by-hop +
 privacy rewrite, `.i2p`-only target validation, bounded error
 response generation) and the daemon HTTP proxy executor
 (`crates/i2pr-daemon/src/service_tunnels_http.rs`) that owns one
 loopback listener per `http-client` spec. It reuses the Plan 174
 shared byte pump + Plan 149 destination product path; no new
 Garlic/I2NP/Streaming implementation exists. Plan 180 closed the
 M10 local product layer by adding the generation/draining
 reconcile surface; the per-service byte round-trip is now proven
 through the Plan 175/176/177/178/179 product suites that the manager
 composes under one supervisor.
runtime driver loop is exercised, while the byte round-trip remains
an explicit Plan 180 deliverable.

Plan 177 added the runtime-neutral `i2pr-service-tunnels::socks5`
module (RFC 1928 no-auth greeting negotiation, CONNECT request
parser with strict `.i2p`/DOMAINNAME-only target policy,
deterministic RFC 1928 reply generator with a neutral loopback
`127.0.0.1:0` bind, bounded typed errors) and the daemon SOCKS5
proxy executor (`crates/i2pr-daemon/src/service_tunnels_socks5.rs`)
that owns one loopback listener per `socks5-client` spec. It
reuses the Plan 174 shared byte pump + Plan 149 destination product
path; no new Garlic/I2NP/Streaming implementation exists. Success
is sent only after Streaming reaches `Established`; same-read
 post-request bytes are preserved as first tunnel bytes; BIND, UDP
 ASSOCIATE, IPv4/IPv6, clearnet/IP literal/localhost/mixed-suffix
 targets, and username/password auth are rejected with the typed
 RFC 1928 reply codes. Plan 180 closed the M10 local product layer
 by adding the generation/draining reconcile surface that the
 per-service product suites all reuse.

Plan 178 added the runtime-neutral `i2pr-service-tunnels::irc`
 module (bounded IRC/IRCv3 line parser with 512-byte core / 8191-byte
 tag-envelope / 4094-byte tag-data ceilings, typed command classifier
 with per-direction allowlist, client-to-network privacy rewrites for
 USER/PING/QUIT/PART, CTCP/DCC policy allowing ACTION while dropping
 DCC and unsupported CTCP, bounded typed errors) and the daemon IRC
 client tunnel executor
 (`crates/i2pr-daemon/src/service_tunnels_irc_client.rs`) that owns one
 loopback listener per `irc-client` spec. It reuses the Plan 174 shared
 byte pump + Plan 149 destination product path; no new Garlic/I2NP/Streaming
 implementation exists. Unknown/unclassified commands are dropped, never
 passed; overlong lines are dropped without truncation. Plan 180
 closed the M10 local product layer by adding the
 generation/draining reconcile surface that the IRC client product
 suite reuses under one supervisor.

Plan 179 added the runtime-neutral
`i2pr-service-tunnels::irc::server` registration interceptor
(bounded pre-registration line / byte ceilings with a typed default
of 10 lines / 8192 bytes, cross-protocol rejection of HTTP and
BitTorrent first lines via a small fixed list, an authenticated peer
Destination hash projection to `<52-char base32>.b32.i2p` that
replaces the USER hostname and is bound to the streaming peer
identity, RFC 2812 four-arg USER with the servername preserved and
legacy RFC 1459 USER with the mode parameter rejected, IRCv3 tagged
USER rewrite with envelope preserved, PASS / CAP / AUTHENTICATE /
NICK passthrough, same-read post-USER bytes preserved as first
raw-pump bytes, an optional `SERVER` server-to-server IRC handoff,
typed `RegistrationOutcome::{Incomplete, Ready, Rejected, Eof}`,
and bounded typed errors) plus the daemon IRC server tunnel
executor (`crates/i2pr-daemon/src/service_tunnels_irc_server.rs`)
that owns one Streaming accept loop per `irc-server` spec, waits for
the Streaming connection to reach `Established`, captures the peer
Destination hash from authenticated Streaming metadata (the only
acceptable source for the projected hostname), runs the bounded
registration interceptor under a 30 s total deadline (with a 20 ms
poll cadence), connects to the loopback target under a 10 s deadline,
writes the rewritten prefix + leftover exactly once, and switches
to the shared Plan 174 byte pump in opaque mode for the
post-registration stream. The Plan 175 persistent server destination
storage owns the IRC server destination identity so restart
preserves both the public service Destination and the projected
hostname algorithm. No WEBIRC, no cloaked hostnames, no DCC, no
TLS termination, no IRC daemon implementation, and no
post-registration server-side filter claim; the post-handoff
stream is byte-transparent. Plan 180 closed the M10 local product
 layer by adding the generation/draining reconcile surface that
 the IRC server product suite reuses under one supervisor.

Focused SSU2 seams currently include:

```text
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --lib
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

During Plan 162, ordinary no-peer invocation of the external driver must be:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1
# expected: external test discovered as ignored; command exits 0
```

The explicit external invocation after Plan 162 gating is:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

Without the required external environment, that explicit command must fail
closed. With exact-pinned i2pd provisioned, it must execute and pass the
full matrix (directions A+B, cached-token, malformed/resource rows).

The full Plan 161 lane (local suites + matrix + gates, 15 command-derived
rows) is:

```text
bash tests/integration/ssu2/run-independent.sh
bash scripts/check-ssu2-acceptance-evidence.sh
```

Plan 155 added the SSU2 fixture corpus (`tests/fixtures/ssu2/`) and its
checker (`scripts/check-ssu2-vectors.sh`), enforced in routine Linux CI;
do not weaken it to make CI pass.

Focused I2CP seams currently include:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-api --test i2cp_vectors
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
```

Focused M10 service-tunnel foundation seams currently include:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --lib destination_streaming
cargo test --locked -p i2pr-daemon --lib config
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_server_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_independent_application_clients -- --test-threads=1
bash scripts/check-service-tunnel-acceptance-evidence.sh
```

The full Plan 181 lane (local suites + matrix + gates, 31
command-derived rows with 2 remote rows recorded blocked) is:

```text
bash tests/integration/service-tunnels/run-independent.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
```

Plan 182 added the M10 local-delivery corrective (per-destination
delivery drivers, wildcard Streaming port 0, SAM-parity accept
paths, direction-branched pump sends, completed IRC client
executor, orderly pump half-close) plus the `jaraco/irc`
fetch script (`scripts/interop/fetch-service-tunnel-clients.sh`)
and the manual `.github/workflows/service-tunnels-external.yml`
lane; do not weaken the evidence checker to make CI pass.

Focused M6 preflight seams currently include:

```text
cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib config -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
# expected: 5 passed, 1 ignored (external preflight gated)
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight \
  ssu2_daemon_preflight_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env: passes
bash tests/integration/m6-interop/run-preflight.sh
```

Focused M6 exploratory tunnel seams currently include:

```text
cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_tunnel_external \
  exploratory_tunnels_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: ignored; with lane env: passes (12-row external lane)
bash tests/integration/m6-interop/run-tunnels.sh
bash scripts/check-exploratory-tunnel-evidence.sh
```

Focused M6 NetDB seams currently include:

```text
cargo test --locked -p i2pr-daemon --test netdb_tunnel_unit -- --test-threads=1
# expected: 22 passed
cargo test --locked -p i2pr-daemon --test netdb_tunnel_live -- --test-threads=1
# expected: 9 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_tunnel_external \
  netdb_tunnels_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env: passes
bash tests/integration/m6-interop/run-netdb.sh
bash scripts/check-netdb-tunnel-evidence.sh
```

Focused M6 destination seams currently include:

```text
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
# expected: 31 passed (Plan 187: 27; Plan 190: 4 inbound reply-path rows)
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
# expected: 9 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_external \
  destination_message_plane_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env); with lane env: the
# lane now advertises the corrected (gateway router, gateway receive id)
# reply path (Plan 190); the lease-lookup row flips from blocked to passed
# only when a real tunneled lookup response arrives within the bounded
# wait; the inbound-delivery rows flip from blocked to passed only after
# Plan 192 lands the i2pd-compatible I2CP-style Data body + 9-byte
# short-transport inner envelope + STYLE=RAW / DATAGRAM VERSION=3 SAM
# session (Plan 191 §6 stop provenance; see plans/191-status.md)
bash tests/integration/m6-interop/run-destination.sh
bash scripts/check-destination-tunnel-evidence.sh
```

Focused M6 mixed-router cross-family seams (Plan 189 §8
ledger/checker/workflow scaffold; i2pd-only rows run today,
Java second-family rows blocked until Plan 188 first-family
destination + Streaming gates are green):

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# static structural check: per-layer harnesses + checker both
# pin i2pd 2.61.0 + Java I2P 2.13.0; cross-family aggregator
# wires both pins; cross_family_row helper keeps the rc gate

bash tests/integration/m6-interop/run-m6-mixed-router.sh
# cross-family aggregator: reuses the four per-layer harnesses
# and binds every Plan 189 §8 guarded row to a family + the
# actual per-layer exit code; Java rows recorded `failed` with
# stop provenance until a follow-up plan lands the second-family
# Java harness. i2pd-only-runs is the current shape; the Java
# rows stay bound to the same evidence.json.
```

Plan 184 owns the daemon-owned authenticated I2NP spine
(`crates/i2pr-daemon/src/router_i2np.rs`: central dispatcher,
narrow `RouterDeliveryService` over existing `send_i2np`, strict
loopback/non-advertised `[ssu2]` activation, daemon-owned
`Ssu2DaemonService` under `ssu2-router` supervision). Plan 185
adds the daemon-owned exploratory build coordinator
(`crates/i2pr-daemon/src/exploratory_build.rs`: bounded pending
table, monotonic attempt / creator tunnel ids, single central
scheduler, strict OTBRM extraction, `register_*_with_material`
installs through the existing `ExploratoryPool` then activates
once into `DataPlaneRegistry`) and the bounded creator-side
tunnel liveness scheduler
(`crates/i2pr-daemon/src/tunnel_liveness.rs`: one central scheduler,
no per-tunnel task or per-tunnel timer, first-test / repeat /
response-timeout / failure-threshold policy well below the
two-minute idle deletion boundary). The exploratory tunnel
external lane is ignored in routine CI and explicitly selected
in its dedicated lane; do not add filename filtering, `|| true`,
fake peer values, or production wire changes to make it green.
The external driver derives the build encryption key from
`RouterInfo.router_identity().public_key()` (NOT the SSU2 `s`
option) so the daemon-owned runtime key and the build key share
the same X25519 keypair. i2pd is provisioned with
`notransit = false` so the reference accepts one-hop exploratory
builds; the lane stays loopback + unpublished and no public I2P
claim is made. Plan 186 adds the daemon-owned NetDB-over-tunnels
coordinator (`crates/i2pr-daemon/src/netdb_tunnels.rs`: authoritative
bounded RouterInfo store, ordinary-path reference bootstrap, floodfill
verification, tunnel-path proofs, bounded lookup/publication/search
matrices, typed tunnel-loss without direct fallback). The NetDB external
lane is ignored in routine CI and explicitly selected in its dedicated
lane; i2pd is provisioned with `notransit = false, floodfill = true`
so the reference accepts builds and acts as the controlled floodfill.
Plan 187 adds the daemon-owned destination-over-tunnels coordinator
(`crates/i2pr-daemon/src/destination_tunnels.rs`: authoritative
bounded RouterInfo store, store-parameter LeaseSet2 lookup through
the existing seam, tunnel-path proofs, authoritative LeaseSet2
cache, real-material proofs rejecting `LocalZeroHop`, bounded
local-LS2 publication with protocol-derived ack, registry-backed
Garlic recovery, typed tunnel-loss without direct fallback) plus
narrow additive seams (`begin_lease_set2_lookup_with_store` /
`ingest_lease_set2_search_reply` on the seam, `GarlicComplete` on
inbound dispatch, `deliver_outbound_cells` for client-composed
Garlic cells, registry/role accessors, `DestinationOutboundRole::from_role`
by move). The destination external lane is ignored in routine CI
and explicitly selected in its dedicated lane; i2pd is provisioned
with `notransit = false, floodfill = true` plus loopback SAM so the
reference DATAGRAM destination is created through its public
client surface. The lane stops fail-closed at the §11
build-reply gate (reference accepts builds per its transit log
but emits no consumable reply; 7 install-dependent rows recorded
blocked with multi-run diagnosis); Plan 188 owns the narrow
corrective; Plan 190 isolates and corrects the inbound NetDB
reply-path metadata defect that left 5/7 destination rows
blocked after the Plan 188 installs. The raw i2pd log is never
evidence (SAM session lines); only sanitized counts reach
evidence.

Plan 164 added the I2CP fixture corpus (`tests/fixtures/i2cp/`) and its
checker (`scripts/check-i2cp-vectors.sh`), enforced in routine Linux CI;
do not weaken it to make CI pass. Plan 165 added the connection state
machine, SessionConfig verification, option disposition table, and
session registry under `crates/i2pr-api/src/i2cp/`; no I2CP
listener, destination activation, or client-interoperability claim
exists yet. Plan 166 added the client-owned destination runtime +
Standard LeaseSet2 validation path under `crates/i2pr-client/`
(`DestinationOwnership`, `DestinationPublic`,
`InboundDecryptionCapability`, `install_client_lease_set2`,
`LeaseRequest`, `take_client_refresh_request`) and the
`I2cpAction::RequestVariableLeaseSet` typed action in
`crates/i2pr-api/src/i2cp/actions.rs`; the listener, socket
ownership, and client-interoperability claim remain Plans 167–170.
Plan 167 added the supervised loopback I2CP v0.9.67 server runtime
in `crates/i2pr-daemon/src/i2cp.rs` plus the real-TCP acceptance
test in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. Plan 168 added
the message data plane under `crates/i2pr-api/src/i2cp/data_plane.rs`
(`I2cpMessageOutcome`, `PendingStatusTable`, `InboundPayloadQueue`,
bounded per-session ceilings, `InboundPayloadFrame::WIRE_OVERHEAD_BYTES`)
plus the per-session `I2cpSessionState` in
`crates/i2pr-daemon/src/i2cp.rs` and the eighteen-test black-box
acceptance suite in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs`;
the daemon validates `SendMessage`/`SendMessageExpires`, routes
payloads through the existing `i2pr_client::DestinationRuntime::enqueue_outbound`,
drains `MessagePayload` inbound frames through a `tokio::sync::Notify`,
resolves `DestLookup` against the local destination registry, and
returns the config-derived `BandwidthLimits` reply. Plan 169 added
the reconfigure transaction handler (`handle_reconfigure_session` +
`apply_reconfigure` + `ReconfigurationOutcome`) and the per-session
reconfigure baseline (`I2cpSessionState::last_options`), the
synchronous `handle_destroy_session` data-plane drain, and three
new narrowly named acceptance suites:
`crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (the canonical
self-composed trajectory plus a bounded repeated-lifecycle soak and
a destroy-one-session/keep-sibling usable proof),
`crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (the Plan 169
§5 adversarial protocol matrix), and
`crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (the Plan 169 §6
concurrency/resource matrix). Independent Java/Go client evidence
remains in Plan 170.

## Testing conventions

- Prefer paused Tokio/manual clocks for deterministic runtime tests where compatible with socket behavior.
- Runtime-owned socket tests use loopback only; no DNS/public traffic.
- Use explicit bounded deadlines, not indefinite waits.
- Queue/resource tests cover exact capacity/max+1 and verify release after failure/closure.
- Secret-bearing values stay redacted/zeroized/non-Clone where practical.
- Channel/socket closure is a lifecycle event, not blindly retried.
- A required evidence result must be derived from an executed command/test. Never mark an acceptance row passed merely because a historical plan says it passed.
- External tests that require a separately provisioned process must never silently pass when that process/environment is absent.

## External SAM evidence

Provenance retained from Plan 150:

```text
Java I2P Plan146 reference:
  2800040deee9bb376567b671ef2e9c34cf3e30b6

i2pd Plan146 reference:
  f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e

i2psam counted Plan150 client:
  b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac

i2plib counted substitute SAM surface:
  6edf51cd5d21cc745aa7e23cb98c582144884fa8

libsam3 built/probed but not counted:
  7d6e658798baec31394c5685f9583343cc00900b
```

The manual `.github/workflows/sam-external.yml` lane is unprivileged and
localhost-only. Plan 151 reran it on the exact closing head after all new
acceptance tests were integrated; Plan 153 made its evidence checker a
permanent invariant.

## External SSU2 evidence

Current mandatory reference:

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
role = mandatory Plan 161 independent direct SSU2 reference
```

Preferred secondary reference:

```text
Java I2P 2.13.0
commit = 9134f808337b401e8e53c73734c81fab04280c9d
role = preferred secondary; recorded nonblocking narrow-orchestration debt (see plans/161-status.md)
```

Plan 161 directions A+B plus the ledger/checker/workflow lane have
passed locally. Plan 162 corrected how that external-process
test is selected by routine versus dedicated lanes; the corrective is now
closed.

## Coding conventions

- No unsafe in protocol/client/API/service crates unless separately reviewed.
- Treat all SAM/network bytes as hostile and bounded.
- Use typed errors; do not swallow codec/protocol results.
- Runtime cryptography/local-product ephemeral material uses OS CSPRNG.
- New dependencies require explicit review.
- Avoid global mutable state/service locators; pass narrow capabilities.
- Do not modify M6 wire semantics to make a SAM test convenient.
- Do not modify SSU2 wire semantics merely to make a CI lane green.

## Protocol claims

- Milestone 6 local product is closed via Plan 134; router interoperability is not claimed.
- Plan 149 closed the self-composing localhost SAM product.
- Plan 150 retains at-least-two independent-client core evidence, but its final acceptance label is superseded.
- Plan 151 is the current final Milestone 7 acceptance authority.
- Plan 152 is the passed narrow M6 robustness corrective retained underneath Plan 151.
- Plan 153 is the passed docs/CI hygiene pass.
- Plans 155–160 are passed Milestone 8 SSU2 v2 local protocol/runtime/reachability stages.
- Plan 161 is passed: directions A+B (+ cached-token/malformed rows) against exact-pinned i2pd 2.61.0 are proven over real loopback UDP with authenticated bidirectional evidence, and the fail-closed ledger/checker/workflow lane passes locally and hosted (routine CI runs `34050058216`/`34053041778`, external runs `34051298144`/`34053042857`). Milestone 8 is closed within that bounded scope.
- Plan 162 passed the narrow external-test lane/CI corrective.
- Plan 163 registered the Milestone 9 I2CP roadmap (planning authority only).
- Plan 164 passed the M9 I2CP protocol and wire foundation (structural codecs, fixtures, profile; no behavior claim).
- Plan 165 passed the M9 I2CP connection/session/options state machines (typed connection state, SessionConfig signature/date/ceiling verification with injected clock, option disposition table, bounded session registry, reconfiguration taxonomy, typed `I2cpAction` vocabulary; no listener, destination activation, or interoperability claim).
- Plan 166 passed the M9 I2CP client-owned destination + LeaseSet2 bridge: `DestinationOwnership::RouterOwned` / `ClientOwned`, `DestinationPublic`, `InboundDecryptionCapability`, atomic `install_client_lease_set2` (signature + lease ownership + expiry + decryption-key match), typed `LeaseRequest` for refresh from real inbound tunnels, and the `I2cpAction::RequestVariableLeaseSet` action. SAM router-owned product regressions remain green; no listener, socket ownership, or interoperability claim.
- Plan 167 passed the M9 I2CP loopback server runtime in `crates/i2pr-daemon/src/i2cp.rs`: disabled-by-default `[i2cp]` block, supervised Tokio listener, per-connection `ChildScope`, typed `I2cpAction` dispatch, single `teardown_connection` cleanup path on EOF/reset/timeout/cancel, and twelve real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. No application-message direction, no lookup, no reconfiguration, no independent-client interop claim.
- Plan 168 passed the M9 I2CP message data plane: bounded per-session `SendMessage`/`SendMessageExpires` validation against the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam, bounded `MessageStatus` correlation table, `MessagePayload` inbound frames delivered only to the owning session's bounded queue (sibling-isolation guaranteed), cross-session local loopback shortcut, `DestLookup` resolving through the local destination registry, and `GetBandwidthLimits` returning the config-derived client ceiling and the documented neutral router values. Eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise every Plan 168 §11 case. SAM router-owned product regressions remain green. No reconfiguration, no `HostLookup`/`HostReply` resolution, and no independent-client interop claim.
- Plan 169 passed the M9 I2CP self-composed local product and hardening: the `ReconfigureSession` transaction handler (`handle_reconfigure_session` + `apply_reconfigure` + `ReconfigurationOutcome`) parses/verifies the full new SessionConfig, classifies each diff entry using the Plan 165 `reconfiguration_class` table, and commits the new baseline atomically through `I2cpSessionState::last_options`; immutable and unsupported keys reject the whole transaction without state mutation. `handle_destroy_session` now drains the per-session Plan 168 data-plane bookkeeping synchronously so repeated DestroySession/CreateSession cycles retain zero inbound queue, status correlation, or outbound slot. The Plan 169 acceptance suites are `crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (5 tests), `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (19 tests at Plan 169 close; 20 after the Plan 171 companion), and `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (6 tests); every test binds the listener to `127.0.0.1:0` and drives behavior only through TCP/I2CP inputs. SAM router-owned product regressions, the Plan 167 listener regression in `i2cp_loopback.rs`, and the Plan 168 data-plane suite in `i2cp_message_data_plane.rs` remain green. No `HostLookup`/`HostReply` resolution and no independent Java/Go client evidence yet; those belong to Plan 170.
- Plan 171 passed the M9 I2CP invalid-preamble close and CI corrective (retained): the common per-connection terminal path in `crates/i2pr-daemon/src/i2cp.rs` explicitly shuts the TCP stream down before bookkeeping release (no wire change, no independent-client claim). The adversarial matrix is now 20 tests (the strict 24-iteration `wrong_protocol_byte_is_closed` plus its non-paused `wrong_protocol_byte_is_closed_real_time` companion).
- Plan 170 external wire/data-plane evidence is retained-passed: exact-pinned Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) and go-i2cp (`b529ee1c10a6011558b4d69fc9436a4afc489eac`) exchange digest-matched 25 B/32 KiB payloads in both directions through the loopback daemon (`tests/integration/i2cp/run-independent.sh`, 9 fail-closed rows, `scripts/check-i2cp-acceptance-evidence.sh` in routine CI, manual `.github/workflows/i2cp-external.yml`). Its final-acceptance interpretation is superseded by Plan 172: the counted Java driver bypassed `I2PSession.connect()` and no external session installed a LeaseSet2. Do not claim independent LeaseSet2 lifecycle from Plan 170 rows alone.
 - Plan 172 passed the M9 I2CP independent LeaseSet2 lifecycle corrective (see `plans/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md` and `plans/172-status.md`): explicit local zero-hop tunnel kind, 44-byte Lease-compatible non-empty real `RequestVariableLeaseSet` from the destination pool, existing Plan 166 atomic install (ElGamal-slot skip, unpublished accepted, u8 LS2 key count), high-level Java `I2PSession.connect()` plus public go-i2cp async `ProcessIO` lifecycle proof, post-LS2 digest-matched cross-client traffic both directions, fail-closed 24-row evidence. Milestone 9 final acceptance is closed-via-plan172.
 - Plan 173 registered the Milestone 10 service-tunnels roadmap.
 - Plan 174 passed the M10 service-tunnel foundation: runtime-neutral `i2pr-service-tunnels` crate, strict disabled-by-default loopback-only `[service_tunnels]` surface, shared daemon Streaming pump reused by SAM, no listener yet.
 - Plan 175 passed the first complete M10 application service product (generic client/server tunnels + persistent server destinations).
 - Plan 176 passed the M10 HTTP `.i2p` proxy + CONNECT.
 - Plan 177 passed the M10 SOCKS5 `.i2p` CONNECT proxy.
 - Plan 178 passed the M10 IRC `.i2p` client profile + privacy filter.
 - Plan 179 passed the M10 IRC `.i2p` server profile + authenticated peer hostname projection.
 - Plan 180 passed the M10 service-tunnel composition, reconcile, and hardening (see `plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md` and `plans/180-status.md`): the runtime-neutral `i2pr_service_tunnels::generation::DiffClass` typed classification (`Unchanged`, `MutableInPlace`, `ReplaceListener`, `ReplaceDestination`, `Remove`, `Add`); the daemon-owned `ServiceTunnelGeneration`/`DrainingGeneration` committed-generation model with `GenerationCounters { active_current_generation, active_draining_generation, forced_drain_closes_total }`; the `ServiceTunnelManager::reconcile(candidate, drain_deadline)` transactional algorithm that validates, diffs, stages Add/Replace*, then publishes the new generation atomically and pushes only replaced/removed old runtimes onto the draining list under a hard deadline; `reap_expired_drains` for forced-drain close handling; `generation_snapshot` for the Plan 180 §9 unified cross-service resource accounting matrix; the static `scripts/check-service-tunnel-boundaries.sh` checker enforcing the runtime-neutral constraint, no Garlic/I2NP construction in service-tunnels, the single shared `run_stream_pump` invariant, no unbounded Tokio channels, and exactly one `register_service_tunnel_manager` entry point. Two new narrowly named suites (`crates/i2pr-daemon/tests/service_tunnels_final_acceptance.rs` — 15 tests covering the Plan 180 §12 reconcile matrix, `crates/i2pr-daemon/tests/service_tunnels_adversarial_matrix.rs` — 12 tests covering the Plan 180 §13 cross-service adversarial matrix) bind the manager to a temp data directory and drive behavior only through the public API. Every Plan 174/175/176/177/178/179 product test remains green. Plan 180 closes the M10 local product layer; Plan 181 owns the M10 independent acceptance gate. M10 service tunnels stay experimental, loopback-only, disabled by default, and non-advertised; no independent router interop claim.
 - Plan 181 is blocked by the retained M6 mixed-router Streaming debt (see `plans/181-m10-independent-application-and-service-interop-final-closure.md` and `plans/181-status.md`): 29 local independent-application-client rows pass (unmodified curl HTTP/SOCKS, nc, stdlib generic driver, exact-pinned jaraco/irc through the real manager; restart stability; resource baselines; unsupported-profile ledger), and the two remote rows are recorded `blocked` with command/log provenance from a genuine qualification attempt (exact-pinned i2pd 2.61.0 SAM `DEST GENERATE` public destination; `unknown_peer>0`, `delivered=0`, no establishment, bounded timeout). Self-composed rows are never substituted for interop. Milestone 10 final acceptance stays open.
 - Plan 182 passed the M10 local-delivery corrective (see `plans/182-m10-local-delivery-corrective.md` and `plans/182-status.md`): per-destination delivery drivers reusing the Plan 129 `bridge_to_peer` seam, inbound-factory install, wildcard Streaming port 0 (SAM convention), SAM-parity accept paths with queued SYN responses, direction-branched pump sends with typed backpressure matching, orderly pump half-close (default no-op keeps SAM byte-identical), completed line-filtering IRC client executor, permit-for-task-lifetime capture, and active-slot release on every exit path. Nine round-trip tests in `service_tunnels_local_roundtrip.rs` plus six wire-surface tests prove the local byte round-trip the Plan 174–179 profiles assumed. No wire change.
  - Plan 183 registered the M6 mixed-router destination/Streaming interop program Plan 181 §6.3 requires (see `plans/183-m6-mixed-router-streaming-interop-program.md` and `plans/183-status.md`): registration only, no implementation, no M10 closure claim.
 - Plan 184 passed the M6 authenticated I2NP preflight (see `plans/184-m6-authenticated-i2np-runtime-and-reference-preflight.md` and `plans/184-status.md`): strict loopback/non-advertised `[ssu2]` activation, daemon-owned `Ssu2DaemonService` under `ssu2-router` supervision, central `router_i2np` dispatcher with authenticated peer preservation, narrow `RouterDeliveryService` over existing `send_i2np`, exact-pinned i2pd 2.61.0 bidirectional control with fail-closed 10-row lane. No tunnel/NetDB/Streaming/M10-remote claim.
- Plan 185 passed the M6 live one-hop exploratory tunnels and liveness lane (see `plans/185-m6-live-one-hop-exploratory-tunnels-and-liveness.md` and `plans/185-status.md`): daemon-owned `ExploratoryBuildCoordinator` + `TunnelLivenessScheduler` drive the existing `i2pr-tunnel::short::ShortBuildStateMachine` / `i2pr-tunnel::pool::ExploratoryPool` / `i2pr-tunnel::data_plane_registry::DataPlaneRegistry` seams end-to-end through the Plan 184 central `router_i2np` dispatcher; one real outbound and one real inbound one-hop exploratory build accepted by the exact-pinned i2pd 2.61.0 reference with `notransit=false`; bounded first-test / repeat / response-timeout / failure-threshold liveness policy; 12-row external lane + `scripts/check-exploratory-tunnel-evidence.sh` static evidence check. No multi-hop, no destination LeaseSet2 / Streaming claim.
- Plan 186 passed the M6 mixed-router NetDB lookup and publication lane (see `plans/186-m6-mixed-router-netdb-lookup-and-publication.md` and `plans/186-status.md`): daemon-owned `NetDbTunnelCoordinator` drives the existing lookup/publication state machines over the Plan 185 pair through the authoritative bounded store (ordinary-path reference bootstrap, floodfill verification, tunnel-path proofs, bounded matrices, typed tunnel-loss); exact-pinned i2pd 2.61.0 with `notransit=false,floodfill=true`; 22 unit + 9 live + 12-row external lane + `scripts/check-netdb-tunnel-evidence.sh`. No multi-hop, no destination LeaseSet2 / Streaming claim; Plan 187 landed the destination program (local rows passed, remote gate pending Plan 188).
- Plan 187 is blocked by the `m6-build-reply-interop-gap` (see `plans/187-m6-remote-leaseset2-and-destination-garlic-routing.md` and `plans/187-status.md`): the daemon-owned `DestinationTunnelCoordinator` with the full local destination message plane is landed (27 unit + 9 live two-role rows including the bidirectional ECIES/Garlic round-trip with sibling isolation over real TunnelData cells; narrow additive seams on the NetDB seam/inbound-dispatch/outbound-lookup/registry, no wire change), and the 21-row external lane proves session, reference build acceptance both directions, SAM DATAGRAM destination, reference LS2 publication, direct rejection, and liveness — but exact-pinned i2pd 2.61.0 emits no consumable ShortTunnelBuildReply (multi-run diagnosis: lossless session, `kind_reply=0`, reference transit acceptance logged), so no tunnel material installs and 7 install-dependent rows are recorded `blocked` with stop provenance. Creator-known keys are never installed without a consumed reply. Plan 188 owns the narrow build-reply corrective; no LeaseSet2/Streaming interop is claimed.
- Plan 188 is the short-build-reply corrective (see `plans/188-m6-short-build-reply-interop-corrective.md` and `plans/188-status.md`): outbound garlic-unwrap (TunnelGateway + Garlic with OBEP `RGarlicKeyAndTag`) plus inbound forwarded-ShortTunnelBuild consumption in `ExploratoryBuildCoordinator` land consumed-reference installs both directions (`installed_ob=1 installed_ib=1`); 5/7 destination rows flipped to passed and 2/4 destination-message-bound rows + 2 ordering rows were blocked on the inbound delivery layer that Plan 191 owns; no synthesis, no wire change. The deferred `plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming pass stays blocked until all seven destination rows plus the inbound-delivery rows pass.
- Plan 190 is the inbound NetDB reply-path tunnel-ID corrective (see `plans/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md` and `plans/190-status.md`): typed public `InboundGatewayRoute` (`gateway_router`, `gateway_receive_tunnel`, `local_receive_tunnel`) retained by `i2pr_tunnel::data_plane_registry::DataPlaneRegistry`; daemon-owned `reply_path_for_inbound_route` adapter derives `i2pr_netdb::ReplyPath` only from `(gateway_router, gateway_receive_tunnel)` so the local creator endpoint receive tunnel id is impossible to copy into the encoded `DatabaseLookup.reply_tunnelId`; `i2pr-netdb::ReplyPath`/`build_databaselookup` semantics unchanged. Regression rows in `crates/i2pr-tunnel/src/data_plane_registry.rs` and `crates/i2pr-daemon/tests/destination_tunnel_unit.rs` prove unequal IDs (`0x9601` vs `0x9602`) round-trip through the I2NP codec with the gateway tuple on the wire, and that lifecycle removal cleans the typed route atomically. Local Plan 187/188 suites remain green (`destination_tunnel_unit` 31 passed, `destination_tunnel_live` 9 passed, `exploratory_build_live` 11 passed). A fresh exact-pinned i2pd 2.61.0 external `run-destination.sh` proves 3 destination rows flip `blocked` → `passed`; the corrected lane stops at Plan 190 §6 boundary E (inbound delivery). No M6 wire change; no `milestone6_interoperable = passed-via-plan190` claim.
- Plan 191 is the inbound destination delivery boundary (see `plans/191-m6-inbound-destination-delivery-boundary.md` and `plans/191-status.md`): Plan 191 ran the inbound-delivery layer and stopped at boundary E per §6. The corrected `run-destination.sh` reached `destination-outbound-delivered cells=1 payload_len=27` then the i2pd SAM bridge never observed a `DATAGRAM RECEIVED` line because i2pd's `ClientDestination::HandleDataMessage` parses an I2CP-style Data header + gzip-wrapped datagram payload but i2pr emits a raw 16-byte-standard I2NP Data body whose first four bytes are misread as the length field and overflow the available buffer. The test driver no longer panics; `read_line`/`wait_for_datagram` return `Option<...>` and record distinct evidence keys (`reference-received-timeout`, `destination-inbound-send-failed`, `inbound-delivery-boundary-E-stop`). The 2 ordering rows `external-direct-rejected` / `external-liveness-first-test` flip to `passed`. The 2 inbound-delivery rows `external-reference-received` / `external-destination-inbound` stay `blocked` with stop provenance. No `milestone6_interoperable = passed-via-plan191` claim; the narrower follow-up is Plan 192.
- Plan 192 is the M6 i2pd-compatible I2CP-style Data body wire-format corrective (see `plans/192-m6-i2cp-wire-format-corrective.md` and `plans/192-status.md`): registered follow-up that owns the Plan 191 §6 stop. The fix is narrow: switch the inner I2NP envelope inside the ECIES-X25519 Garlic clove from the 16-byte standard form to the 9-byte short-transport form i2pd parses (`Garlic.cpp:1023-1028`); wrap the application payload in the I2CP-style Data body i2pd expects (`length[4 BE] + fromPort[2 BE] + toPort[2 BE] + padding[1] + protocol[1] + gzip-no-compression-wrapped payload`; `Destination.cpp:1192-1236`); and switch the test SAM session from `STYLE=DATAGRAM` (which needs a 384-byte ElGamal/DSA `from` Identity our ECIES-only i2pr does not have) to either `STYLE=RAW` or `STYLE=DATAGRAM VERSION=3` (which uses the inbound destination hash instead of an ElGamal/DSA identity; `SAM.cpp:377-379`). No M6 wire change beyond the destination message-plane seam; no `LocalZeroHop` substitution; no authentication weakening; no fake LeaseSet.
- Plan 189 is registered as the M6 Java I2P second-family qualification and mixed-router closure plan (see `plans/189-m6-java-i2p-second-family-qualification-and-closure.md` and `plans/189-status.md`): blocked until Plan 188 first-family destination + Plan 192 inbound-delivery + the deferred 188-streaming pass all close. Plan 189 §8 lands the fail-closed M6 mixed-router cross-family ledger/checker/workflow scaffold: `scripts/check-m6-mixed-router-acceptance-evidence.sh` (structural checker that verifies both pins are referenced by every per-layer static checker), `tests/integration/m6-interop/run-m6-mixed-router.sh` (cross-family aggregator that reuses the four per-layer harnesses), and `.github/workflows/m6-mixed-router-external.yml` (manual `workflow_dispatch` lane that fetches i2pd + Java deps and runs the structural checker + per-layer checkers + cross-family aggregator). The cross-family aggregator binds each guarded row to a family (`-i2pd` / `-java`) plus an executed per-layer command exit code; the Java rows are recorded `failed` with stop provenance until a follow-up plan lands the second-family Java qualification harness under `tests/integration/m6-interop/run-java.sh`. No M6 wire change; no claim that the i2pd first family has passed (the Plan 188 lookup gap closed via Plan 190; the inbound-delivery gap closes only via Plan 192).
- SAM stays experimental, loopback-only, disabled by default, and non-advertised.
- SSU2 public advertisement/public-network participation is not claimed.
- No Plan 161 direction-A evidence implies Milestone 6 destination/Streaming/tunnel interoperability or broad router interoperability.
- Do not advance `advertised = true` without `specs/CONFORMANCE.md` evidence.

## OpenCode skills

Use `i2pr-local-dev` for current local product/SSU2 execution guidance and
`i2pr-architecture` for architecture/ADR/plan navigation. Historical NTCP2,
rootless, and Multipass skills remain separate lanes.

## Commits and handoff

Use focused commits. Do not change git config, skip hooks, force-push, or amend
someone else's commit. Closure records must include exact commands/results and
current-head workflow evidence.

Current handoff: **Plan 191 stopped at the inbound-delivery
boundary E (see `plans/191-status.md`): the corrected
`run-destination.sh` reached `destination-outbound-delivered
cells=1 payload_len=27`, then the i2pd SAM bridge never observed
a `DATAGRAM RECEIVED` line because i2pd's
`ClientDestination::HandleDataMessage` parses an I2CP-style Data
header + gzip-wrapped datagram payload, but i2pr emits a raw
16-byte-standard I2NP Data body whose first four bytes are
misread as the length field and overflow the available buffer.
The test driver no longer panics (`SamClient::read_line` and
`wait_for_datagram` return `Option<...>`); distinct evidence
keys (`reference-received-timeout`, `destination-inbound-send-failed`,
`inbound-delivery-boundary-E-stop`) record the boundary E stop
provenance, and `run-destination.sh` records
`external-reference-received` and `external-destination-inbound`
as `blocked` rather than `failed`. The 2 ordering rows
`external-direct-rejected` and `external-liveness-first-test`
flip to `passed` (no longer occluded by the panic). Local
Plan 187/188/190 suites remain green (`destination_tunnel_unit`
31 passed, `destination_tunnel_live` 9 passed, `tunnel_liveness`
7 passed, `exploratory_build_live` 11 passed). Plan 192 is the
narrower registered follow-up that owns the 9-byte short-transport
inner envelope, the I2CP-style Data body + gzip-no-compression
wrapper, and the `STYLE=RAW` / `STYLE=DATAGRAM VERSION=3`
SAM session switch. Plan 188 keeps the
`installed_ob/installed_ib` retention-passed status and the
corrected reply-path rows retained-passed via Plan 190; its
remaining destination rows are now blocked on Plan 192.
Plan 189 stays blocked until Plan 188 + Plan 192 + the deferred
Streaming pass all close. M10 final acceptance stays open.**
