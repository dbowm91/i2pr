# Plan 130 status — Milestone 6 final wire/runtime corrective closure

Status: **`superseded-by-plan131-final-local-correctness-gate`** (local
Milestone 6 closure evidence; superseded as the current authority).

- Registered: **2026-08-25**; executed from source floor
  `29fb88d36f9794202e88d4b947faed30569c1991`.
- Plan of record:
  [`plans/130-m6-final-wire-runtime-corrective-closure.md`](130-m6-final-wire-runtime-corrective-closure.md).
- Roadmap authority:
  [`plans/126-130-milestone6-final-corrective-roadmap.md`](126-130-milestone6-final-corrective-roadmap.md).
- **Superseded by Plan 131** as the current Milestone 6 authority;
  see [`plans/131-status.md`](131-status.md). The four Plan 131
  acceptance items this plan left open — production Elligator
  branch randomization, independently-proven consumed ECIES tag
  replay, connection-owned I2P ports on every established send API
  including `send_close`/`send_reset`, and side-effect-free oversized
  `send_data()` rollback — are all corrected and proven by Plan 131.

## Why the Plan 129 closure was reopened

The post-Plan-129 audit retained the integrated architecture but found
four narrow defects:

1. **Production Elligator2 representation fingerprinting:** the
   production path used a fixed representation/tweak choice instead of
   the randomized on-wire behavior expected by I2P reference
   implementations.
2. **Streaming sequence/ACK semantics:** ordinary application data
   started at sequence zero; simple ACK / `ackThrough == 0` handling
   and standalone delayed ACK behavior were incomplete.
3. **I2P destination-port routing:** the inbound adapter decoded
   `destination_port` but a separate caller-supplied listener value
   determined the actual listener backlog.
4. **Replay evidence:** the Plan 129 fixture rebuilt inbound tunnel
   role/duplicate-window state before ordinary deliveries, so the
   exact tunnel-replay claim was not proven against one persistent
   live window.

All four are corrected by this plan; the retained Plan 129 work (the
full Streaming → gzip → Data → ECIES → Garlic → destination tunnel →
OBEP → seam → inbound tunnel → dispatcher → ECIES → Data → gzip →
Streaming topology, Plan 127 sender-LS2 binding and reverse routing,
Plan 128 corrected flags/options/signatures/MTU/SYN policy, the
runtime-neutral adapter architecture, and the existing fault suites)
is unchanged except where the corrected sequence/ACK semantics require
updated expectations.

## Final classification

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-streaming-wire-local
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-line
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = superseded-by-plan130-final-gate
plan_130 = superseded-by-plan131-final-local-correctness-gate
plan_131 = passed-milestone6-final-local-correctness-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

The post-Plan-130 audit retained the integrated architecture but found
four narrow correctness defects that were still unproven or only
partially proven:

1. Production Elligator2 still fixed the deployed-reference alternative
   inverse-map branch (the two high bits were randomized; the branch
   bit was not).
2. The integrated ECIES consumed-tag replay was not independently
   proven as a third distinct rejection point.
3. Established outbound `send_close` / `send_reset` / `send_data` still
   treated caller-supplied ports as authoritative wire fields rather
   than as assertions against the connection-stored tuple.
4. `send_data()` allocated the send-window sequence number before
   checking the negotiated payload ceiling, so a rejected oversized
   write could leave state behind.

These four are corrected and proven by Plan 131.

## What was corrected

### §2.1 / Phase B — production Elligator2 randomization
(`crates/i2pr-crypto/src/ecies.rs`)

- `EciesEphemeralKeypair::generate` now draws the two normative random
  high bits from the CSPRNG (`ENCODE_ELG2`: `encodedKey[31] |=
  randomByte & 0xc0`) in addition to the seed; retries stay bounded at
  64 attempts and entropy failures return the typed
  `EciesError::RandomnessUnavailable`.
- `EciesEphemeralKeypair::from_seed_bytes` is documented as the
  deterministic test/vector constructor only (fixed tweak 0); every
  frozen Plan 126 KDF/Noise vector still reproduces byte-for-byte.
- Library mode remains `curve25519-elligator2` `RFC9380` — its decode
  masks byte 31 with `0x3f`, exactly the normative `DECODE_ELG2`; no
  hand-written Elligator arithmetic was introduced. The `Randomized`
  variant was evaluated and rejected because it changes the DH public
  key itself (documented in the reference note).
- Reference evidence and independent frozen fixtures:
  [`specs/references/elligator2-production-representation.md`](../specs/references/elligator2-production-representation.md).
  Fixtures were produced by a pure-Python implementation of the
  normative encode/decode plus an RFC 7748 ladder cross-checked against
  Python `cryptography`; they pin all four high-bit variants and both
  Java/i2pd encode branches decoding to one X25519 public key.

### §2.2–§2.3 / Phases C+D — Streaming sequence and ACK/NACK semantics
(`crates/i2pr-client/src/streaming/`)

- Sequence space: send window starts application data at sequence **1**
  (`FIRST_APPLICATION_SEQUENCE`); receive windows expect sequence **1**
  after the handshake; SYN/SYN-response/plain-ACK own sequence 0.
- Plain-ACK control form: a seq-0 non-SYN packet never enters the
  receive window, never delivers, never advances counters, and never
  schedules an acknowledgement (no ACK-of-ACK loop); a hostile seq-0
  payload is dropped without window corruption.
- ACK validity is semantic: `ackThrough == 0` is processed (it
  acknowledges the handshake slot) unless NO_ACK is set. The old
  `ack_through != 0` guard is gone.
- NACK-aware cumulative acknowledgement (`SendWindowPolicy::acknowledge`,
  reference contract from Java `Connection.ackPackets`): covered packets
  clear except explicitly NACKed ones, which stay tracked; the floor
  advances to `lowestNack - 1` under NACKs; duplicate ACKs are
  idempotent; out-of-range NACKs are ignored fail-closed.
- Receiver ACK view (`RecvWindowPolicy::ack_view`, Java
  `MessageInputStream.updateAcks`): `ackThrough` = highest received
  including out-of-order buffered packets; NACKs = missing sequences
  below it, bounded by the reorder window and the 255 wire ceiling.
- Standalone delayed ACK: per-connection O(1) pending state with the
  reference 750 ms default (`StreamingConfig::delayed_ack_ms`, ceiling
  60 s), synchronous `poll_acks(now_ms)` following the
  `poll_retransmits` model; DELAY_REQUESTED value 0 emits immediately;
  any outbound packet piggybacks the current ACK state and cancels the
  pending standalone ACK; terminated/closed connections purge pending
  state.
- CLOSE allocates a real sequence number and is tracked under it;
  RESET carries the final cumulative ACK state.

### §2.4 / Phase E — wire destination-port authority
(`crates/i2pr-client/src/streaming_adapter.rs`, streaming manager)

- `StreamingDestinationAdapter::receive` lost its caller-supplied
  listener port: the adapter decodes protocol + both wire ports and
  hands them to the manager. No runtime caller can redirect one I2P
  port into another listener.
- Listener matching follows the I2CP demultiplexer contract: exact
  destination-port listener, then the wildcard listener on port 0,
  otherwise typed `NoMatchingListener` — no `entry().or_default()`
  side-effect listener creation.
- Connections retain the established local/remote port tuple
  (`local_port`/`remote_port`/`ports_match`); established traffic with
  a wrong tuple fails closed with typed `PortTupleMismatch` without
  touching connection state; `accept_inbound_syn` refuses a redirected
  tuple.

### §2.5 / Phases F+G — persistent duplicate-window evidence
(`crates/i2pr-client/tests/plan129_trajectory.rs`, new
`tests/plan130_trajectory.rs`, `crates/i2pr-tunnel/src/roles.rs`)

- The integrated fixture creates inbound roles once; ordinary
  deliveries reuse them. Only explicit rebuilds model tunnel
  replacement.
- The tunnel roles now treat an exact cell replay as a typed
  rejection: `TunnelRoleError::DuplicateCell` (participant and OBEP).
  The duplicate window is an enforcement point, not a passive counter.
- Three duplicate layers are proven independently: exact tunnel replay
  rejected by the live window (never reaches ECIES), consumed-tag ECIES
  replay rejected at session level, and a freshly resealed identical
  Streaming sequence suppressed by Streaming dedup.

## Validation record

Exact commands (pinned toolchain 1.95.0, `--locked`):

```text
cargo +1.95.0 fmt --all --check                                   pass
cargo +1.95.0 check --locked --workspace --all-targets            pass
cargo +1.95.0 test --locked -p i2pr-crypto                        39 tests ok
cargo +1.95.0 test --locked -p i2pr-proto                         pass (incl. plan128_wire fixtures)
cargo +1.95.0 test --locked -p i2pr-client                        pass (60 lib + plan120..plan130 suites)
cargo +1.95.0 test --locked -p i2pr-tunnel                        pass
cargo +1.95.0 test --locked --workspace                           51 test binaries, all green
cargo +1.95.0 clippy --locked --workspace --all-targets
      --all-features -- -D warnings                               pass (0 warnings)
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked
      --workspace --no-deps                                       pass
bash scripts/check-dependency-direction.sh                        ok
bash scripts/check-runtime-boundaries.sh                          passed
bash scripts/check-fixture-manifest.sh                            exit 0
```

New focused evidence:

- `crates/i2pr-client/tests/plan130_trajectory.rs` — 11 deterministic
  full-stack trajectories: frozen spec-derived simple-ACK byte fixture,
  frozen reference ACK/NACK expectation table, fresh-handshake sequence
  transition (SYN 0 → response 0 → first app 1 → second app 2),
  one-way delayed standalone ACK across gzip→Data→ES→tunnels→Streaming,
  piggyback suppression, reorder+NACK convergence over the reverse
  stack, wire-port listener selection/wildcard fallback/typed
  rejections, tunnel-replay vs Streaming-duplicate separation, plain-ACK
  loop freedom, and production-Elligator establishment.
- `crates/i2pr-crypto/src/ecies.rs` Plan 130 tests: production
  representative decodes to the exact intended public key, high-bit
  randomization fingerprint sample, deterministic vector constructor
  stability, reference fixture decodes (both branches × four high-bit
  variants), failing-CSPRNG typed error.

## Explicitly not claimed

Mixed-router interoperability, destination ECIES/Streaming/tunnel
interoperability against an independent router, NTCP2 activation, and
any public-network behavior remain outside this closure. NTCP2 stays
experimental and non-advertised. External acceptance debt is retained
separately.

## Handoff

Per the plan-of-record §14: stop corrective Milestone 6 planning. Next
product work is **Milestone 7 / SAM baseline planning**. Do not reopen
external transport validation as a prerequisite for SAM.
