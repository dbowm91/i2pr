# Plan 033 closure: NTCP2 handshake state machines

Status: complete for the bounded, runtime-neutral experimental scope. This
closure records local structural evidence only; it does not claim Java I2P,
i2pd, mixed-router, public-network, anonymity, or capability-advertisement
interoperability.

## Delivered scope

Plan 033 now provides:

- strict `SessionRequest`, `SessionCreated`, and `SessionConfirmed` codecs;
- exact options and SessionConfirmed RouterInfo/options/padding block parsing;
- typed skew, replay-token, fail-closed replay-decision, and bounded reference
  cache policies;
- consuming `InitiatorState` and `ResponderState` transitions with bounded
  read/write, timestamp, padding, replay, RouterInfo, authenticated-result,
  and terminal-action boundaries;
- RouterInfo signature verification, expected RouterIdentity matching, and
  NTCP/NTCP2 version-2 static-key binding;
- cancellation, deadline, and disconnect terminal inputs;
- deterministic full transcript, key-direction, mutation, limit, policy,
  identity-binding, and partial-delivery tests;
- a fuzz target covering the three codecs, authenticated payload blocks,
  RouterInfo binding inputs, replay/skew inputs, and bounded state commands.

The implementation remains in `i2pr-transport-ntcp2` and stays Tokio-free,
filesystem-free, socket-free, and NetDB-free. `i2pr-testkit` supplies the
one-byte segmented stream test without becoming a production dependency of the
transport crate.

## Wire and policy decisions

| Area | Decision |
| --- | --- |
| SessionRequest/Created | 32-byte AES-obfuscated ephemeral, 32-byte authenticated options frame, then bounded cleartext padding; 64-byte minimum and 65535-byte maximum |
| Request/Created padding | 880/848-byte non-PQ maxima from the pinned 0.9.69 source; production distribution remains deferred |
| SessionConfirmed | 48-byte encrypted static frame plus negotiated 16..65487-byte part-two frame, with a 65535-byte total cap |
| Part-two blocks | RouterInfo with zero flags, then at most one Options block and at most one Padding block; unknown, duplicate, reordered, and trailing blocks reject |
| Clock policy | Injected timestamps, ±60 seconds, typed stale/future errors, and replay retention of 120 seconds |
| Replay | SHA-256 of the encrypted ephemeral field; deterministic bounded reference cache; replay, full, and unavailable outcomes fail closed |
| Peer binding | Signed RouterInfo, expected RouterIdentity hash, X25519 identity encryption key, and NTCP/NTCP2 version-2 `s` static option must all validate |
| Runtime boundary | Actions carry bounded owned bytes or typed/redacted values; runtime owns clocks, cancellation, deadlines, padding source, replay storage, and partial stream adaptation |

The handshake does not add data-phase frames, sockets, listener/dial policy,
duplicate-link resolution, RouterInfo publication, NetDB mutation, or
capability advertisement. The production padding distribution and later
interoperability evidence remain open decisions.

## Evidence

The following commands passed in this workspace after the implementation and
documentation updates:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
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

The workspace lane completed 172 tests; the focused handshake lane completed
16 tests and the testkit lane completed 20 tests after the final code
corrections. The fuzz smoke lane exercised every registered target for 32
deterministic runs, including `ntcp2_handshake`. The final cargo-deny rerun
used `--disable-fetch` because the managed environment exposes its cached
advisory database lock read-only; the cached database checks passed.

## Follow-up gate

Before any support metadata changes, a later plan must add an authorized
runtime adapter with exact partial-read/write adaptation, production padding
policy, clock/deadline/cancellation ownership, replay-cache integration, and
link admission. A separate interoperability lane must compare fixed outputs
and failure behavior with current I2P and i2pd implementations on an isolated
authorized test network. Data-phase frames, capability advertisement, and
public-network testing remain outside this closure.
