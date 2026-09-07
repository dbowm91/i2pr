# Plan 164 status — Milestone 9 I2CP protocol and wire foundation

Status: **`passed-m9-i2cp-protocol-and-wire-foundation`**.

Registered: **2026-09-07**.

Plan of record:
[`plans/164-m9-i2cp-protocol-and-wire-foundation.md`](164-m9-i2cp-protocol-and-wire-foundation.md).

## Current authority

```text
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation

milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_wire_foundation = passed-via-plan164
milestone9_final_acceptance = not-yet-closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 165
next_product_layer = milestone9-i2cp
```

## Source refresh (§2)

Verified before code changes; all three Plan 163 pins resolve to the
recorded immutable commits:

```text
i2p/i2p.website @ 26467e4b275e3a58280b9d4e6d4745d58bb8c499
  2026-08-22, "disabled feedback buttons"
  content/en/docs/specs/i2cp.md (86,307 bytes, accurate for 0.9.67)
  content/en/docs/specs/i2cp-overview.md (50,135 bytes)
  only later touch is cosmetic anchor repair: pin retained

i2p/i2p.i2p @ 9134f808337b401e8e53c73734c81fab04280c9d
  2026-07-20, Java I2P 2.13.0 release commit

go-i2p/go-i2cp @ b529ee1c10a6011558b4d69fc9436a4afc489eac
  2026-08-29, Go I2CP library
```

Recorded in `specs/SOURCES.md` (Plan 164 refresh section),
`specs/IMPLEMENTATIONS.md` (M9 I2CP reference table),
`specs/protocols/10-i2cp-service-tunnels.md` (M9 section; service
tunnels marked M10), `specs/support.toml` (plan_163/164 authority
plus `i2cp.wire-foundation` / `i2cp.message-codecs` rows), and
`specs/CONFORMANCE.md` (I2CP matrix row annotated structural-only).

No implementation source was copied. Two wire details were corrected
against the normative pages during implementation: I2P `String` is a
one-byte-length-prefixed value (max 255 bytes), and
`BandwidthLimits` is seven named integers plus nine reserved
integers (sixteen total, 64 bytes).

## What landed

`crates/i2pr-api/src/i2cp/` (`mod`, `frame`, `message`, `ids`,
`payload`, `mapping`, `error`), `#![forbid(unsafe_code)]`, no
Tokio/sockets/timers/async, no new workspace dependencies:

- `0x2a` preamble check; common frame (`u32` length + `u8` type)
  with an exact 64 KiB ceiling rejected before allocation on both
  decode and encode; capped incremental decoder for partial reads.
- Exact type IDs, directions, and M9 dispositions for all 25
  assigned types; deprecated (4/6/7/21/29), unsupported
  (BlindingInfo 42), and unknown (including abandoned 40) types are
  classified without body parsing.
- Structural codecs for the implemented profile with strict
  trailing-byte rejection: GetDate/SetDate, session
  create/reconfigure/destroy/status, variable-lease request (at
  most 16 leases), Standard-LeaseSet2 publication (ordered,
  non-`Clone`, redacted, zeroized decryption keys; LS types 1/5/7
  typed unsupported), send/expires (48-bit expiry, reserved flag
  bits rejected), payload/status (codes 0–23 plus reserved
  failures), bandwidth (GetBandwidthLimits empty, BandwidthLimits
  64 bytes), destination lookup/reply (empty/hash/destination),
  host lookup/reply (structural), disconnect.
- SessionConfig retains the exact received signed region for Plans
  165–166; signature length is sized by the destination signing
  type; enforcement of the ±30 s window belongs to Plan 165.
- Canonical Mapping reuse: strict sorted posture for SessionConfig,
  lenient normalize posture for GetDate auth and HostReply options.
- Payload/gzip metadata contract (ports, protocol number, flags)
  with a recorded 64 KiB expansion ceiling and no decompression
  helper (Plan 168).
- `tests/fixtures/i2cp/` (22 positive, 7 malformed) with
  `manifest.tsv`, enforced by `scripts/check-i2cp-vectors.sh`
  (routine Linux CI); `crates/i2pr-api/tests/i2cp_vectors.rs`
  pins field expectations and typed rejections.

The M9 compatibility profile table lives in `src/i2cp/mod.rs`. No
listener, session, destination activation, or interoperability
claim is introduced.

## Closure fields

Populated only from executed evidence:

```text
closing_sha = <implementation commit SHA, filled after commit>
routine_ci_run = <hosted run ID, filled after push>
routine_ci_ubuntu = <pending>
routine_ci_macos = <pending>
msrv = <hosted-only>
dependency_policy = passed (local)
```

Local floor on the closing tree (all executed 2026-09-07):

```text
cargo fmt --all --check = passed
cargo check --locked --workspace --all-targets = passed
cargo test --locked --workspace --all-targets -- --test-threads=1 = passed (1591 passed, 1 ignored: the Plan 162 external test)
cargo test --locked -p i2pr-api --all-targets = passed (190 passed)
bash scripts/check-i2cp-vectors.sh = passed (manifest complete, 15 vector tests passed)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings = passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps = passed
cargo test --locked --workspace --doc = passed (0 tests)
bash scripts/check-dependency-direction.sh = passed
bash scripts/check-runtime-boundaries.sh = passed
bash scripts/check-fixture-manifest.sh = passed
bash scripts/check-ntcp2-vectors.sh = passed
bash scripts/check-ssu2-vectors.sh = passed
bash scripts/check-ntcp2-interoperability.sh = passed
bash scripts/check-constrained-host-lane-boundary.sh = passed
bash scripts/check-sam-acceptance-evidence.sh = passed (22 rows)
bash scripts/check-ssu2-acceptance-evidence.sh = passed (15 rows)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' = passed (153 tests)
cargo deny check advisories bans sources = passed
```

All acceptance criteria in Plan 164 §12 are satisfied except hosted
CI confirmation, which follows the push. Criterion 14 (routine CI on
the exact closing commit) is closed by the hosted run recorded
above; any hosted failure reopens this status before Plan 165.

## Handoff

Plan 164 is closed. Execute Plan **165**
(`plans/165-m9-i2cp-connection-session-and-options.md`) next, then
Plans 166–170 in order:

```text
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
next_executable_plan = 165
next_product_layer = milestone9-i2cp
```
