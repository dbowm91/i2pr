# Plan 148 status — SAM 3.1 independent-client interoperability and final Milestone 7 closure

Status: **`blocked-external-client-build-failure`**.

Registered: **2026-09-01**.

Plan of record: [`plans/148-m7-sam31-independent-client-final-closure.md`](148-m7-sam31-independent-client-final-closure.md). Source audit: Plan 145 (`active-m7-sam31-corrective-roadmap`), [`plans/145-status.md`](145-status.md).

## Outcome

Plan 148 does not close. The selected external clients, i2plib and libsam3, are not available in this checkout's cache, and no build/install lane exists for them. The closure contract requires at least two independent SAM implementations to move application bytes through the real i2pr listener. No such evidence exists. `sam_independent_clients = 0-passed`; `milestone7_local_product = not-closed`.

A prior attempt added a Rust-only `SamClient` helper and treated two instances of it as independent clients. That evidence is invalid: it is one implementation, one codebase, and one test-side author. The attempt also used a wall-clock sleep, fabricated FORWARD peer metadata, and did not complete the required SILENT, multi-stream, backpressure, fault, privacy-log, and external-client matrix. It was rejected rather than retained as closure evidence.

## External-client blocker

| Candidate | Pin | Language/license | Blocker |
| --- | --- | --- | --- |
| i2plib | `6edf51cd5d21cc745aa7e23cb98c582144884fa8` (`v0.0.14`) | Python / MIT | Pinned source is not present in the local cache; no build/install wrapper is checked in; fetching is unavailable on this host. |
| libsam3 | `e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7` (`v1.0.0`) | C / mixed public-domain/MIT components | Pinned source and build artifact are not present in the local cache; no build/install wrapper is checked in. |
| txi2p | `0611b9a86172cb70d2f5e415a88eee9f230590b3` | Python/Twisted / ISC | Optional candidate remains blocked by legacy `ometa`; it is not a Plan 148 prerequisite. |

Plan 148 permits replacement with another maintained independent client only after pinning provenance. This status does not select a replacement.

## Retained internal evidence

The following Plan 146/147 suites are valid local product and regression evidence, but do not satisfy the independent-client criterion:

- `crates/i2pr-daemon/tests/sam_plan146_reference.rs` — bidirectional Java I2P private-destination evidence.
- `crates/i2pr-daemon/tests/sam_stream_raw_product.rs` — dedicated raw TCP↔Streaming byte path through the real SAM listener.
- `crates/i2pr-daemon/tests/sam_forward_naming.rs` — loopback FORWARD and local NAMING behavior.
- `crates/i2pr-daemon/tests/sam_loopback.rs` — bounded session lifecycle, parser, and resource regressions.
- `crates/i2pr-daemon/tests/sam_stream_product.rs` and `sam_stream_independent.rs` — retained Plan 143/144 local-delivery and in-process handshake regressions.
- `i2pr-client` Plan 127–134 focused tests — retained Milestone 6 correctness evidence.

The canonical raw product lane is Plan 147's lane, not a substitute for the two-independent-client gate.

## Remaining acceptance work

A passing Plan 148 must still demonstrate:

1. at least two independent external SAM implementations negotiate with the real listener;
2. both private-destination representation directions through those implementations;
3. cross-client CONNECT/ACCEPT and exact bidirectional binary bytes;
4. byte-exact SILENT true/false behavior;
5. multiple independent streams and bounded close/reset/control-session lifecycle;
6. slow-reader/slow_writer and loss/duplicate/reorder/ACK evidence through the raw path;
7. FORWARD and NAMING revalidation without fabricated metadata or DNS expansion;
8. parser/resource/privacy evidence, including captured default logs;
9. focused Plan 127–134 M6 regressions;
10. the full workspace format/check/test/clippy/doc/boundary gates.

Until these items are executed with real external clients, the attempted Rust helper tests must not be counted as independent-client evidence.

## Validation

The following local baseline remains green; these checks validate existing implementation and regression evidence, not Plan 148 closure:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

The full workspace suite has 1,255 passing tests at this disposition. `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` requires repair of a pre-existing broken intra-doc link in another crate before it can be used as a Plan 148 gate; this is recorded as a local validation defect, not hidden as a pass.

## Handoff

Do not promote `plan_148`, Milestone 7, or `sam_independent_clients`. Do not commit the invalid Rust-only "independent-client" test attempt. Resolve the external-client cache/build prerequisite, write the two actual client harnesses, and rerun the complete acceptance matrix. Plan 148 remains blocked until its closure record records those real runs and exact client provenance.
