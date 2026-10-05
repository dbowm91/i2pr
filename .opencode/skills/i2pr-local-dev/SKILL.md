---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — M6 destinations/garlic/LeaseSet2/Streaming, M7 SAM 3.1, M8 SSU2, M9 I2CP, M10 service tunnels (12 kinds), M11 transit tunnels, M12 floodfill, Proposal 170 I2PControl, the addressbook and outproxy seams, and ELS2/Red25519 encrypted LeaseSet2. Load before touching product/SSU2/SAM/I2CP/I2PControl/tunnel/transit/floodfill code. Current plan authority lives in plans/registry.md and plans/closure/, not in this skill.
---

# I2PR Local Development

Use this skill for the local product/SAM/SSU2/I2CP/I2PControl execution side of
the router. Historical mixed-router NTCP2 work remains separate acceptance debt.
For registering or closing out an implementation plan, load `i2pr-planning`.
For doc/ADR navigation, load `i2pr-architecture`.

**Status authority is not in this skill.** The plan ledger this file used to
carry is gone on purpose — it went ~90 plans stale while the closure records
moved. Read `plans/registry.md` for current work and
`plans/closure/<subsystem>/<newest>-status.md` for the authoritative token. The
short summary below is orientation only.

Plan 236 is closed at
`P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`. Its source-lock and
test-only classifier live in the mixed-router harness; do not treat the
returned Java socket as response emission, and do not touch production
Streaming/tunnel/NetDB code unless a new plan proves exact inbound TunnelData.
M6 Java full-router compatibility is **retained nonblocking debt** under
ADR 0026 / Plan 247 — it is not a gate on anything.

## Current authority

Read the live index, not this file. In order:

1. [`plans/registry.md`](../../../plans/registry.md) — active roadmaps, current
   milestone authorities, ready plans, blockers. Maintained with the code.
2. `plans/subsystems/<subsystem>-roadmap.md` — the workstream and its
   milestone table. 16 roadmaps: workspace-foundation, ntcp2-transport, netdb,
   exploratory-tunnels, destination-streaming, sam, ssu2, i2cp,
   service-tunnels, mixed-router-interop, transit-tunnels, floodfill,
   i2pcontrol-proposal-170, red25519-encrypted-leaseset, anonymity,
   managed-native-app-runtime.
3. `plans/closure/<subsystem>/<newest>-status.md` — **authoritative**. The token
   there beats this skill, the registry prose, and `specs/support.toml`.
4. `specs/support.toml` + `specs/CONFORMANCE.md` before claiming any support.

There is no single "active plan"; authority is milestone-keyed with parallel
lanes. Orientation only, as of this pass:

```text
active_plan                        = plan284   (M12 floodfill, registry)
ready (Proposal 170 outproxy lane) = plan342   (option surface + request paths + wire lane)
ready (ELS2 transcript)            = plan346   (ELS2-only deployed-compatibility profile)
ready (app runtime, parallel)      = plan345   (contract foundation only, no launcher/sandbox)
M11 transit                        = one-family experimental qualification passed
                                     (Plan 268); transit stays non-advertised,
                                     TransitParticipation::Disabled is enforced
M12 floodfill                      = Plans 270-276 passed; 277/278/279/306 stopped
                                     with retained work; 283/284/285 passed;
                                     broad caps advertisement still forbidden
Proposal 170 / I2PControl          = 319-321, 322(passed 2026-10-05), 323-324,
                                     329-333, 337-341, 343-344 passed; 325/327/328
                                     blocked; 326/335 blocked historical with
                                     successors 346 -> 347 registered; 348 blocked
Anonymity lane                     = 311-316, 318 passed; 308/310/317 blocked
                                     history; 313 ready; parallel, does not gate M12
```

Do not reintroduce these wrong values. They are the ones most often
misremembered or quoted from a superseded copy, and each was asserted by an
earlier ledger in this file that had gone stale:

- `plan_194` is `passed-m6-java-second-family-mixed-router-closure-with-sam-ls2-gap`
  (via Plans 196/197) — **passed**, not "retained-partial", and it did not stop at
  a first-run topology blocker.
- `plan_201` is `retained-deferred-nonblocking-java-router-compatibility-debt-via-plan248`.
  M6 Java is nonblocking retained debt (ADR 0026), not a live gate.
- `plan_204` is `retained-convergence-record-superseded-by-plan248-policy-reconciliation`
  — convergence bookkeeping, not a blocker. Earlier tokens in that append-only
  record are superseded history.
- `plan_195` is `blocked-m10-remote-independent-service-pending-plan213-and-plan214`
  — not "reactivated", and it is historical.
- `milestone10_final_acceptance` is `closed-via-plan215`.
- There is no `plan_208` ambiguity to resolve.
- **`plan_322` is `passed-canonical-routerinfo-sources-with-the-transit-participation-posture-unchanged`**
  (amended 2026-10-05; Plans 339/340 closed its five per-family and three transit
  selectors, leaving the honest product baseline at `0`/`0`/`0.0`). It is not a
  blocker. The three transit selectors stay at zero because enabling transit is a
  production posture change, not a missing snapshot.
- **`plan_335` is `blocked-measured-type-11-transcript-incompatible-with-both-named-references`,
  but its *interpretation* is superseded by Plan 346.** The measurement stands;
  "the references have a defect" does not. It is a specification/deployment split.

Closed-era milestones, for orientation only: M6 local Plan 134 (Plan 152
retained corrective); M7 SAM Plan 151; M8 SSU2 Plans 161+162; M9 I2CP Plan 172;
M10 service tunnels Plan 215; M5 Plans 107-117; M4 Plans 102-106; M3 interop
Plans 038-100 historical at `protocol-defect-localized` / `noise_authenticated`.

## Retain these working pieces

Do not rebuild them without a concrete defect:

- Plan 137 bounded loopback listener/session lifecycle;
- Plan 142 I2P Base64 correction;
- Plan 146 Java I2P/i2pd private-destination reference compatibility;
- `DestinationIdentity::from_imported` semantics;
- strict SAM parser/resource ceilings and secret hygiene;
- Plan 139 loopback-only FORWARD/NAMING implementation;
- `StreamingManager` and `StreamingDestinationAdapter` as the authoritative stream implementation;
- Plan 129 local destination/ECIES/Garlic/Streaming product path;
- Plan 147 owned raw `TcpStream` handoff, same-read preservation, actual `Established` wait, OS CSPRNG runtime path, byte pump, and supervised ACK/retransmit driver;
- Plan 149 transactional self-composed `SESSION CREATE`, one shared `Arc<DestinationIdentity>`, `SamLocalProductFabric`, local peer LeaseSet2 resolution, automatic destination driver, byte-exact SILENT/peer metadata, and typed delivery counters;
- Plan 150 external core evidence: pinned i2psam + qualified i2plib SAM surface, exact two-direction 2 MiB transfers, private destinations, SILENT, NAMING, negative matrix, and positive FORWARD;
- Plans 155–160 SSU2 local protocol/runtime/path/peer-test/relay architecture;
- Plan 161 direction-A handshake transcript corrections and regenerated vectors. Independent i2pd comparison exposed those defects; do not revert them to match older i2pr↔i2pr assumptions.
- Plan 164 I2CP framing/message codecs, the M9 compatibility profile, and the committed `tests/fixtures/i2cp/` vectors. Do not extend structural codecs into behavior/session/listener claims; those belong to Plans 165–170.
- Plan 165 I2CP `ConnectionStateMachine`/SessionConfig verification/option projection/session registry/typed `I2cpAction` vocabulary. Do not extend into a listener, destination activation, or interoperability claim; those belong to Plans 166–170.
- Plan 166 client-owned destination capability surface (`DestinationOwnership`, `DestinationPublic`, `InboundDecryptionCapability`, `install_client_lease_set2`, `LeaseRequest`, `take_client_refresh_request`) and the `I2cpAction::RequestVariableLeaseSet` action. Do not extend into a listener, socket ownership, or interoperability claim; those belong to Plans 167–170.

## Why Plan 151 exists

Plan 150's implementation/external-client work is useful, but its final
acceptance ledger overclaimed several deferred cases. The clearest example was
an unconditional `multiple-stream-lifecycle = passed` row referring to a Plan
149 sibling-stream test that did not exist.

Plan 151 made the deferred sibling/backpressure/fault/CLOSE-RESET/FORWARD and
focused M6 regression items executable through the real listener and required
every final `passed` row to derive from a command/test that actually ran.

That pass exposed one narrow M6 robustness defect family, closed by Plan 152
without a wire change: bounded receiver retention/ACK gating, coalesced
duplicate ACK behavior, and sender ECIES ratchet-key trimming.

## Evidence-integrity rule

No required final row may be marked passed merely because another plan/status
says it passed. `tests/integration/sam/run-independent.sh` derives required SAM
rows from executed commands/tests.

Plan 151 added:

```text
scripts/check-sam-acceptance-evidence.sh
```

The checker is enforced in routine Linux CI and the manual SAM external
workflow. Do not weaken it to make CI pass.

The same principle applies to current SSU2 work: Plan 161 final evidence must
come from explicitly executed local/external commands. An external test that
is skipped because no peer exists is **not** an external-interoperability pass.

## Plan 161 independent SSU2 provenance

Retain exact pins:

```text
i2pd
  version: 2.61.0
  repo: PurpleI2P/i2pd
  pin: 635b013a612ff47278ef02acf8580a28e10e26c5
  role: mandatory independent Plan 161 SSU2 reference

Java I2P
  version: 2.13.0
  repo: i2p/i2p.i2p
  pin: 9134f808337b401e8e53c73734c81fab04280c9d
  role: preferred secondary; nonblocking if narrow unprivileged orchestration is disproportionate
```

Do not patch or vendor external routers.

Direction A retained evidence:

```text
i2pr initiator -> i2pd responder
real loopback UDP
tokenless TokenRequest -> Retry -> SessionRequest -> SessionCreated -> SessionConfirmed
mutual authentication
small DatabaseStore i2pr -> i2pd
fragmented DatabaseStore i2pr -> i2pd
DeliveryStatus return for both stores
graceful session/resource teardown
```

Direction B is proven symmetrically (i2pd initiator -> i2pr responder
promotion through the normal token/Retry path, same small + fragmented
proof shape). The direction-B baseline predates the inter-direction
settle sleep so a redial landing inside the settle still counts;
see `plans/closure/ssu2/161-status.md`. Java I2P is recorded nonblocking
narrow-orchestration debt in every ledger artifact, not a silent gap.

## Plan 162 closure rule/result

Current routine CI run `33915994884` on head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both Ubuntu and macOS
quality jobs because ordinary workspace execution automatically ran:

```text
crates/i2pr-runtime/tests/ssu2_independent.rs
```

without an external i2pd environment. Dependency policy and MSRV passed; the
observed error was `missing required env I2PD_ROUTER_INFO`.

Plan 162 implemented this shape:

```text
ordinary workspace test
  -> external test is compiled/discovered
  -> external test is ignored
  -> ordinary command exits 0

dedicated external invocation
  -> explicitly selects ignored test with --ignored --exact
  -> missing external environment still fails hard
  -> exact-pinned i2pd environment executes the real trajectory
```

Preferred mechanism: a descriptive Rust `#[ignore = "..."]` attribute on only
the environment-dependent external test.

Forbidden fixes:

- missing-env early return/success;
- CI executable-name filtering;
- `|| true`;
- `continue-on-error`;
- fake `I2PD_*` values;
- broad crate/integration-test exclusion;
- production SSU2 changes merely to make CI green.

Plan 162 re-ran direction A after gating and required routine Ubuntu/macOS CI
green on its exact closing commit. The implementation closing commit was
`624e8cce177040674376163160cfbda47e6a60fe`, verified by hosted CI run
`33941941145`; `next_executable_plan = 161` is restored.

## External SAM provenance

Retain exact pins:

```text
i2psam
  repo: https://github.com/i2p/i2psam
  pin: b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
  role: counted external client

i2plib
  repo: https://github.com/l-n-s/i2plib
  pin: 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  role: counted qualified SAM-surface substitute

libsam3
  repo: https://github.com/i2p/libsam3
  pin: 7d6e658798baec31394c5685f9583343cc00900b
  role: built/probed, not counted
```

Do not patch or vendor external clients.

## Environment contract

```text
root/sudo                         = no
Linux namespaces                  = no
Docker                            = no
VM/Multipass                      = no
systemd                           = no
public I2P network                = no
localhost TCP                     = yes
localhost UDP                     = yes
exact-pinned external i2pd process= yes, Plan 161 dedicated lane only
routine CI external peer          = no
manual GitHub external lane       = yes
```

M6 Java full-router compatibility (the Plan 236/247 lane) is the exception
that is *not* runnable here: it needs an exact-pinned Java 2.13.0 cache and the
dedicated M6 interop lane, and it is retained nonblocking debt. Do not try to
reconstruct it from a local checkout, and do not relabel its result.

## Development commands

**The routine floor is `AGENTS.md` and only `AGENTS.md`.** Do not maintain a
copy here — an earlier copy of this section silently lost six entries,
including two boundary/evidence checkers. Read the floor there and run it
verbatim. Use `--test-threads=1` locally for the `i2pr-daemon`/`i2pr-runtime`
loopback suites.

Two reminders that the floor makes easy to miss:

- `python3 scripts/check-global-plan-number-uniqueness.py` — it is a `.py`
  script. Running it with `bash` garbles it (it even shells out to ImageMagick)
  and exits 2, which reads like a failure rather than a misuse.
- A green floor is not full coverage. See `AGENTS.md` → "Known checker gaps":
  `i2pr-tunnel` is unpoliced by the direction script, `check-runtime-boundaries.sh`
  has no `i2pr-api` section, `tools/i2pr-interop` is unpoliced, and
  `check-m12-floodfill-boundaries.sh` currently exits 1.

The floors below are **subsets scoped to a milestone**, not the routine floor.

Focused SAM floor:

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

Focused SSU2 floor:

```text
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --lib
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

Focused I2CP floor:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-api --test i2cp_vectors
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
bash scripts/check-i2cp-acceptance-evidence.sh
```

Focused M10 service-tunnel floor:

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
cargo test --locked -p i2pr-daemon --test service_tunnels_application_remote_qualification -- --test-threads=1
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
```

Plan 203 positive remote application interop driver (fail-closed
without the exact-pinned i2pd cache; flips to `passed` once the
dedicated M6 interop lane provisions the SSU2 endpoint + bind
tuple and the driver emits the documented `manager-routing-decision`,
`http-streaming-established=true`, `http-remote-application-established`,
and `irc-remote-application-established` evidence keys):

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_application_remote_qualification \
  m10_positive_remote_http_and_irc_application_interop -- --ignored --exact --test-threads=1
```

Plan 162 ordinary no-peer regression:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1
# expected after Plan 162 implementation: test ignored, exit 0
```

Plan 162 / Plan 161 explicit external invocation:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

With required environment absent, that explicit command must fail for missing
external configuration. With the exact-pinned i2pd lane provisioned, it must
execute and pass the full matrix (directions A+B, cached-token,
malformed/resource rows).

The full Plan 161 lane (local suites + matrix + gates, 15 command-derived
rows) is:

```text
bash tests/integration/ssu2/run-independent.sh
bash scripts/check-ssu2-acceptance-evidence.sh
```

Focused M11 transit / M12 floodfill / Proposal 170 floor:

```text
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m11-per-epoch-composition.sh
bash scripts/check-m11-transit-qualification-evidence.sh
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
bash scripts/check-i2pcontrol-acceptance-evidence.sh
bash scripts/check-service-anonymity-boundaries.sh
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-addressbook --all-targets
cargo test --locked -p i2pr-i2pcontrol --all-targets
cargo test --locked -p i2pr-su3 --all-targets
```

Note `i2pr-tunnel` and `i2pr-addressbook` have **no CI boundary script**, so
their floor above is a test-only safety net, not a boundary proof.

## Coding rules

- No new unbounded channels/queues.
- Runtime/socket ownership stays in daemon/runtime layers.
- The SSU2 central scheduler replaces a handshake's resend deadline with each new arm batch; never min-merge with a stale past value (Plan 158 regression).
- SSU2 path migration must requeue unacked fragments through the bounded loss policy; never just clear sent provenance (Plan 159 regression).
- SSU2 peer-test correlation is by nonce plus role/state, never by source; unsigned out-of-session corroboration never confirms direct reachability (Plan 160).
- SSU2 relay success proves firewalled, never direct; verify HolePunch against nonce-derived connection IDs before touching request state (Plan 160).
- SSU2 path challenges/responses are single-shot minimum-MTU control datagrams; never migrate on source change alone.
- OS CSPRNG for runtime material; deterministic randomness is test-only.
- Never log private destination material, SSU2 static/session keys, tokens, or raw payloads.
- No second private identity copy for SAM bridge ownership.
- Do not weaken M6 Streaming semantics for SAM tests.
- Do not weaken SSU2 authentication/RouterInfo/token/replay semantics for external interop.
- Do not modify SSU2 production wire behavior to repair Plan 162 CI selection.
- I2CP framing/session state stays in `i2pr-api` with no sockets, Tokio, timers, or task ownership; TCP/Tokio ownership stays in `i2pr-daemon`; destination behavior stays in `i2pr-client` (which must never depend on `i2pr-api`).
- I2CP decryption material stays non-`Clone`, redacted, and zeroized; never log private keys, session secrets, tokens, or raw payloads.
- `i2pr-api` has no `i2pr-api` section in `check-runtime-boundaries.sh`; the rule that TCP/Tokio ownership lives in `i2pr-daemon` is a discipline, not an enforced check. Do not add sockets or Tokio there on the strength of a green floor.
- Transit: `TransitParticipation::Disabled` is the enforced production state. Never enable transit participation or advertise transit to make a test pass.
- Floodfill: never add a broad `caps=f` advertisement or a capability/version field to satisfy a peer's admission gate.
- ELS2/Red25519: type-5 LeaseSet storage and PSK/DH client authorization are in-repo and pq-free. Never add ML-KEM, never advertise `common.leaseset2-family`, and never imply i2pd or Java can verify it — Plan 335 measured both rejecting i2pr.
- Outproxy (Proposal 170): the provider policy and the daemon route owner may only route through an I2P Streaming connection. Never add a direct clearnet socket, resolver, TLS client, plugin load, or process spawn. `check-service-tunnel-boundaries.sh` rules 9–11 enforce this, and `std::net::IpAddr` is deliberately allowed so the grammar can reject IP literals — do not "fix" that allowance.
- Addressbook: `i2pr-addressbook` is a no-I/O naming owner (books, precedence, subscriptions, versioned generations). Do not give it sockets, fs, or Tokio, and do not fork a second naming precedence implementation elsewhere.
- `i2pr-i2pcontrol` and `i2pr-addressbook` are runtime-neutral like `i2pr-api`: wire/domain only, no I/O.
- Do not claim I2CP behavior, sessions, listeners, or client interop from structural codecs alone.

## Final claim rules

Durable claim constraints. The per-plan changelog that used to live here is
gone; each claim below is traceable to a closure record, and the closure
records are the authority for which plans passed.

- SAM, I2CP, and service tunnels stay **disabled by default, loopback-only,
  experimental, and non-advertised**. I2CP has no `HostLookup`/`HostReply`.
- NTCP2 stays experimental and non-advertised; the daemon's NTCP2 is disabled
  (Plan 101). The lane result is `protocol-defect-localized` at
  `noise_authenticated` — never a pass.
- SSU2 public advertisement, public-network participation, broad router
  interoperability, IPv6 external interop, PQ v3/v4, and SSU1 remain
  unclaimed/deferred. Sessions stay classical X25519; publication stays
  pq-free; no ML-KEM.
- Transit stays non-advertised and `TransitParticipation::Disabled` is the
  enforced production state. M11 is a **one-family experimental
  qualification**, not general transit capability.
- No broad floodfill advertisement. M12 has no second-family qualification and
  no `caps=f` claim; no capability or version claim may be added to satisfy a
  peer's admission gate.
- M6 Java full-router compatibility is **retained nonblocking debt** (ADR 0026,
  Plan 247). No Java result is relabeled. `milestone6_interoperable` is
  `not-yet-claimed`; full two-family router conformance is not claimed.
- The Proposal 170 outproxy has **policy and a route owner but no reachable
  request path** (`open_via_outproxy` has zero callers). It is infrastructure,
  not capability, and there is no direct clearnet fallback.
- ELS2/Red25519 type-5 LeaseSets are implemented in-repo. The measured result
  stands: i2pd and Java I2P verify **each other's** type-11 signatures and both
  reject i2pr's former strict-only form, with blinded public keys identical
  across all three (Plan 335). **But do not call that a reference defect** —
  the interpretation is superseded by Plan 346: Proposal 146/standalone Red25519
  specifies domain- and length-framed HStar, while the Encrypted-LS2 spec and
  deployed Java/i2pd use randomized RedDSA without those additions. It is a
  specification/deployment split. Plan 346 (ready) will add an ELS2-only
  deployed-compatibility profile while leaving strict Proposal-146 Red25519
  unchanged; Plan 347 owns live cross-router proof. Until then no
  interoperability is claimed and `common.leaseset2-family` stays
  `advertised = false`.
- Self-composed rows are never substituted for interop. Raw reference logs are
  never evidence; only sanitized counts/hashes reach evidence files.
- Do not advance `advertised = true` without `specs/CONFORMANCE.md` evidence.

## Cross-references

- `AGENTS.md` — routine floor, hard boundaries, known checker gaps.
- `docs/architecture/overview.md` — crate index, data flow, capability snapshot.
- `docs/architecture/i2pr-<crate>.md` — per-crate deep dive.
- `i2pr-architecture` skill — ADR/plan navigation and doc-vs-source audits.
- `i2pr-planning` skill — registry/roadmap/closure mechanics.
- `plans/registry.md`, `plans/subsystems/`, `plans/closure/`.
