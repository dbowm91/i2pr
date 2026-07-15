# Milestone 1 Plan D closure: I2NP evidence, fixtures, and fuzzing

## Scope and outcome

This record closes `plans/014-m1-i2np-evidence-fuzzing-closure.md` within its
non-networked codec boundary. The repository now has a bounded I2NP registry,
standard and short envelope codecs, selected typed body codecs, explicit
deferred/framing-only forms for later cryptographic/state-machine bodies,
hashed local fixtures, and an opt-in nightly fuzz workspace.

This is not a router interoperability or capability-advertisement claim. No
socket, transport session, NetDB action, tunnel state machine, garlic
decryption, duplicate filter, expiration policy, or RouterInfo I2NP version
was added.

## Reconciled message support

The pinned I2NP 0.9.69 message registry is represented for types 1, 2, 3, 10,
11, and 18–26. Unknown and reserved identifiers fail with
`CodecError::Unsupported`.

The standard 16-byte header carries type, message ID, millisecond expiration,
u16 payload length, and the first SHA-256 payload byte. The obsolete five-byte
SSU header and the nine-byte NTCP2/SSU2 header use seconds expiration and rely
on their encapsulation for payload length/checksum. Every top-level decoder is
strict and caller-bounded.

Typed structural bodies are:

- `DatabaseLookupMessage`, including bounded exclusions and the defined
  no-encryption, ElGamal/AES, and ECIES reply-key layouts;
- `DatabaseSearchReplyMessage`, capped at 16 peer hashes;
- `DeliveryStatusMessage`;
- `DatabaseStoreMessage`, including reply routing fields, compressed
  RouterInfo framing, classic LeaseSet reuse, and explicit deferred
  LeaseSet2-family payloads;
- fixed TunnelData and nested standard TunnelGateway framing; and
- bounded Data/Garlic length framing plus fixed/variable tunnel-build record
  shapes retained as `Opaque`/`Deferred` values.

Cryptographic interpretation, compression/decompression, LeaseSet2-family
semantics, tunnel-record crypto, garlic cloves, fragment reassembly, and
transport policy remain later-milestone work.

## Limits and security decisions

| Surface | Limit or behavior |
| --- | --- |
| I2NP payload | 62,708 bytes, derived from the pinned tunnel-fragmentation constraint |
| Standard payload length | Checked before body allocation and u16-representable |
| DatabaseLookup exclusions | 512 hashes |
| DatabaseSearchReply peers | 16 hashes |
| Reply tags | 1–32, with 32-byte legacy or 8-byte ECIES tags |
| Tunnel data | Nonzero ID plus exactly 1,024 bytes |
| Tunnel-build records | 1–8 records; 528-byte variable/legacy or 218-byte short records |
| Unknown types | Explicit unsupported error; no opaque fallback |
| Deferred bytes | Bounded and redacted from `Debug` output |

Expiration, duplicate, replay, queue, peer fairness, routing, and resource
budgets remain state-machine responsibilities. The codec has no runtime,
filesystem, network, or nondeterministic global-state dependency.

## Fixtures and provenance

The fixture corpus is under `tests/fixtures/i2np/`. It contains locally
authored hexadecimal standard DeliveryStatus bytes and a one-byte checksum
mutation. `manifest.tsv` records source, pinned revision, generator, expected
outcome, license note, and SHA-256 hash. `scripts/check-fixture-manifest.sh`
validates the hashes. No live capture, peer address, identity, destination,
private key, or copied implementation corpus is committed.

## Fuzz inventory

The separate `fuzz/` workspace contains 17 bounded nightly `cargo-fuzz`
targets: `date`, `date32`, `hash`, `mapping`, `certificate`,
`key_certificate`, `key_and_cert`, `router_identity`, `destination`,
`router_address`, `router_info`, `lease`, `lease_set`, three direct I2NP header
targets, and the `i2np_bodies` dispatch target. Fuzz-only dependencies are
excluded from the production workspace and MSRV checks.
`scripts/fuzz-smoke.sh` runs a short campaign for every target when
`cargo-fuzz` is installed; a missing tool is a clear local-environment
limitation rather than a production fallback.

Minimized regressions belong in the ordinary fixture/test corpus. Fuzzing is
not treated as interoperability evidence.

## Changed files

- `crates/i2pr-proto/src/i2np.rs` — registry, headers, bodies, limits, tests.
- `crates/i2pr-proto/src/lib.rs` — module/export and boundary documentation.
- `.gitignore` — ignores generated fuzz artifacts and targets.
- `tests/fixtures/i2np/` — locally authored vectors and manifest.
- `scripts/check-fixture-manifest.sh` — fixture hash validation.
- `fuzz/` and `scripts/fuzz-smoke.sh` — opt-in fuzz workspace and smoke lane.
- `README.md`, `AGENTS.md`, `CONTRIBUTING.md` — development and agent guidance.
- `docs/architecture.md`, `docs/security-model.md` — codec boundary and risk model.
- `docs/protocol-support.md`, `specs/support.toml` — exact non-advertised support.
- `specs/protocols/02-i2np.md`, `specs/SOURCES.md`, `specs/CONFORMANCE.md` — resolved subset, traceability, and fuzz-conformance guidance.
- `plans/014-closure.md` — this closure record.

No production dependency or crate boundary was added. The fuzz-only
`libfuzzer-sys` dependency is isolated from the production workspace.

## Deviations and unresolved work

- The plan's broad “initial bodies” wording is split into typed structural
  bodies and explicit deferred/framing-only records. This avoids claiming
  NetDB, tunnel, or garlic behavior before those state-machine plans.
- The combined DatabaseLookup encryption/key-derivation mode remains
  unsupported because the pinned document marks its exact format as TBD.
- No authoritative external binary or cross-router fixture was imported;
  local vectors prove deterministic codec behavior only.
- `cargo-fuzz` is an optional nightly Unix-only lane and is not part of the
  stable CI quality gate. CI does not claim fuzz campaigns on platforms where
  the toolchain is unavailable.

The initial I2NP API version, mixed-router message exchange, expiry/duplicate
policy, encrypted NetDB replies, current tunnel-build cryptography, and garlic
clove semantics remain open for their later plans.

## Quality and CI evidence

The final handoff records the exact command results below after the complete
tree is formatted and tested:

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all --check` | passed |
| `rtk cargo check --workspace` | passed |
| `rtk cargo test --workspace` | passed — 75 tests |
| `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps` | passed |
| `rtk bash scripts/check-dependency-direction.sh` | passed |
| `rtk cargo deny check advisories bans sources` | passed — existing duplicate `rand_core` 0.6/0.9 warning |
| `rtk cargo +1.85.0 check --workspace --all-targets` | passed |
| `rtk bash scripts/check-fixture-manifest.sh` | passed |
| `rtk cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets` | passed |
| `rtk bash -c 'CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh'` | passed — 17 targets × 32 runs; managed-environment leak sanitizer disabled by the script |
| GitHub Actions CI run [`29390732012`](https://github.com/dbowm91/i2pr/actions/runs/29390732012) | passed — Ubuntu/macOS quality, Ubuntu MSRV, and dependency policy jobs |

The first online fuzz-smoke attempt was blocked by crates.io DNS resolution;
the recorded smoke result used cached dependencies offline. The CI run does
not claim a fuzz campaign because the fuzz workspace is intentionally an
optional nightly lane. No public-network malformed, stress, or adversarial
testing was performed.

## Prerequisites for later milestones

Milestone 3 must add authenticated transport framing and exchange evidence.
Milestone 4 must add NetDB acceptance, validation, freshness, retry, and
publication state machines. Milestone 5 must add tunnel-record cryptography,
fragmentation/reassembly, and cleanup. Milestone 6 must resolve the selected
LeaseSet family, garlic/ECIES semantics, destination routing, and streaming.
Each must update `specs/support.toml` and `docs/protocol-support.md` from
evidence rather than from this codec's type presence.
