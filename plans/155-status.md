# Plan 155 status — Milestone 8 SSU2 v2 protocol foundation and addresses

Status: **`passed-m8-ssu2-v2-protocol-foundation-and-addresses`**.

Registered: **2026-09-03**. Closed: **2026-09-03**.

Plan of record:
[`plans/155-m8-ssu2-v2-protocol-foundation-and-addresses.md`](155-m8-ssu2-v2-protocol-foundation-and-addresses.md).

## Current authority

```text
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses

milestone8_protocol = ssu2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_implementation = plan155-foundation-landed

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 156
next_product_layer = milestone8-ssu2-v2
```

## What this pass did

1. Refreshed the Milestone 8 SSU2 source authority (plan §2):
   `specs/SOURCES.md` (Milestone 8 refresh: SSU2 v2 spec snapshot
   `88596022920bdf99f27db27688faf4f204792fcd`, accurate for 0.9.69,
   live re-verified 2026-09-03; Proposals 159/165 as
   historical/design context; PQ-hybrid v3/v4 deferred
   compatibility-watch; Java I2P 2.13.0
   `9134f808337b401e8e53c73734c81fab04280c9d` secondary reference;
   i2pd 2.61.0 `635b013a612ff47278ef02acf8580a28e10e26c5`
   mandatory interop reference; clean-room restriction),
   `specs/IMPLEMENTATIONS.md` (M8 reference pins),
   `specs/protocols/09-ssu2.md` (source snapshot + Plan 155
   foundation-scope section), `specs/support.toml` (Plan 153/154/155
   pointers plus the `ssu2.v2-protocol-foundation` surface,
   experimental, `advertised = false`), and the hand-maintained
   `docs/protocol-support.md` SSU2 row (experimental v2
   foundation, non-advertised; no generator exists in the repo).
   No surface was advanced to advertised/production.
2. Created the runtime-neutral workspace member
   `crates/i2pr-transport-ssu2` (plan §3):
   `#![forbid(unsafe_code)]`, no Tokio, no sockets/filesystem/async,
   depends only on `i2pr-proto`, `i2pr-transport`, `thiserror`; no
   dependency on `i2pr-runtime`, `i2pr-daemon`, or `i2pr-testkit`.
   Modules: `constants.rs`, `address.rs`, `header.rs`, `block.rs`,
   `packet.rs`. No handshake/data-session state machines were added.
3. Extended the generic transport kind (plan §4):
   `TransportKind::Ssu2` in `i2pr-transport/src/types.rs`. No
   SSU2-specific `TransportManager` was introduced; the crate
   carries no exhaustive `TransportKind` matches outside its
   definition site, so existing NTCP2 manager semantics are
   unchanged (all `i2pr-transport` contract tests pass unmodified).
4. Implemented the strict SSU2 v2 RouterAddress model (plan §5):
   style `SSU2`; `v=2` (any other version, including PQ-hybrid
   v3/v4, is the distinct `UnsupportedVersion` error); 32-byte
   I2P-base64 static (`s`) and intro (`i`) keys with all-zero
   rejection; numeric-IP `host` (hostnames refused) with paired
   canonical UDP `port`; MTU floor 1280 with a conservative 9000
   advisory ceiling; bounded `caps` (`4`/`6` families, `B`
   peer-test, `C` relay; duplicates rejected; other graphic
   characters ignored); up to 3 dense introducer groups
   (`ihostN`/`iportN`/`ikeyN`/`itagN`, nonzero tags); unknown
   options rejected; type-level `Direct` /
   `DirectWithIntroducers` / `IntroducerOnly` /
   `UnpublishedStatic` classification plus distinct
   `ConfiguredListenAddress` / `ResolvedDialTarget` types;
   redacted `Debug` throughout. Parsing never implies
   reachability/publication approval.
5. Implemented spec-traced constants and structural header
   primitives (plan §6): version 2, network ID 2, connection-ID 8,
   token 8, long 32 / short 16 layouts, message types
   0/1/2/6/7/9/10/11, datagram bounds 40/1452/1472, block IDs
   0–21 with 224–253 experimental / 254 padding / 255 reserved,
   64-block and 1024-unknown-byte ceilings, RouterInfo block cap,
   fragment/ACK/termination/path/congestion bounds, and the
   recorded (not yet executed) Noise protocol name for later
   plans. Header decoders require exact sizes, classify long vs
   short from the spec-defined type byte, reject bad
   version/network/flags/connection-ID/fragment shapes with typed
   errors, and perform no header protection (Plan 156).
6. Implemented the bounded payload block codec (plan §7): all 20
   assigned v2 block families round-trip (DateTime, Options,
   RouterInfo structural, I2NP, FirstFragment, FollowOnFragment,
   Termination with reason codes 0–22, RelayRequest,
   RelayResponse accept/Bob-reject/Charlie-reject shapes,
   RelayIntro, PeerTest messages 1–7, ACK structural, Address,
   RelayTagRequest, RelayTag nonzero, NewToken, PathChallenge,
   PathResponse, FirstPacketNumber, Congestion, Padding).
   NextNonce (spec TODO) is the distinct `UnsupportedBlock`
   error; reserved 14/255 and experimental 224–253 skip under
   the unknown budget. Padding-once-last and
   Termination-last-non-padding ordering enforced. ACK
   interpretation belongs to Plan 157; relay/peer-test signature
   verification belongs to Plan 160; RouterInfo
   verification/fragmentation belongs to Plan 156.
7. Added the SSU2 fixture namespace (plan §8):
   `tests/fixtures/ssu2/` (5 spec-derived constructed vectors +
   `manifest.tsv` + `README.md`) and the narrow checker
   `scripts/check-ssu2-vectors.sh` (manifest shape, hash pins,
   path containment, no unlisted files, 5 required IDs),
   wired into routine Linux CI. No private keys/tokens are
   committed; inline deterministic test keys are marked
   test-only. Required coverage is green: direct IPv4/IPv6,
   introducer/firewalled form, malformed keys/base64,
   duplicates/conflicts, v2 accepted, v3/v4 + unknown versions
   distinctly rejected, long/short header fixtures, every block
   round-trip, per-byte truncation, unknown byte/count ceiling,
   over-limit RouterInfo/fragment rejection.
8. Documented (plan §9): new deep-dive
   `docs/architecture/i2pr-transport-ssu2.md`; updated
   `docs/architecture/overview.md` (graph, crate index, script
   table, M8 paragraph), `dependency-graph.md` (allowlist row,
   reverse edges, ASCII graph, runtime-boundary list),
   `tooling.md` (corpus section, script row, CI step row),
   `docs/protocol-support.md` (SSU2 row),
   `specs/protocols/09-ssu2.md`,
   `specs/SOURCES.md`/`IMPLEMENTATIONS.md`/`support.toml`,
   `README.md`, `AGENTS.md`, `plans/README.md`, both skill copies
   (`.agents`/`...` and `.opencode/...` are hardlinked;
   `i2pr-local-dev` authority/handoff/floor,
   `i2pr-architecture` plan index/crate table/script table),
   and `.github/workflows/ci.yml` (Linux `Check SSU2 vectors`
   step).
9. Extended the static boundary scripts (plan §11) so the new
   crate is mechanically covered rather than conventional:
   `check-dependency-direction.sh` (exact
   `i2pr-proto`+`i2pr-transport` allowlist) and
   `check-runtime-boundaries.sh` (Tokio/socket/`async`/client
   prohibitions span `i2pr-transport-ssu2`).

## Non-goals kept (plan §10)

No Noise handshake transcript, no TokenRequest/Retry state
machine, no AEAD/header protection, no replay window, no ACK/loss
controller, no I2NP reassembly state machine, no UDP sockets or
runtime service, no peer-test/relay roles, no transport selection,
no public address publication, no independent interoperability.
`TransportKind::Ssu2` exists in the generic manager only; no UDP
socket is opened anywhere by the new crate.

## Stop conditions (none fired, plan §13)

The live SSU2 specification page resolved every wire field used
here (header layouts, message/block type tables, per-block field
shapes); no shared common-structure serialization change was
needed (`RouterAddress`/`Mapping`/`EncodedI2npMessage` reused
as-is); no new crypto dependency was introduced; the crate is
runtime-neutral and boundary-enforced.

## Validation record

Starting SHA: `81f9126331d0c3ad49872599177b534f3ccc4da0`.

Local validation on the closing tree (all green):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
  (1315 passed, 53 suites)
cargo test --locked -p i2pr-transport --all-targets (17 passed, 2 suites)
cargo test --locked -p i2pr-transport-ssu2 --all-targets (27 passed, 1 suite)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh (5 rows pinned, hashes match)
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh (22 rows command-derived)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' (153 tests, OK)
cargo deny check advisories bans sources (ok)
```

Existing NTCP2 transport-manager tests pass unmodified
(`i2pr-transport`: 17 passed), satisfying the plan §4
acceptance that NTCP2 semantics did not change.

Hosted lanes on the exact closing commit `479ef98`:

- Routine CI run `33818955817` (push, `main`,
  `479ef98d7b8f5bfa3d7c90aa37646821e3829286`): conclusion
  `success`. The Linux quality job executed the newly wired
  `Check SSU2 vectors` step green alongside the other boundary
  checks; Quality ubuntu, Quality macos, MSRV, and
  dependency-policy all green.

## Acceptance criteria (all true, plan §12)

1. Refreshed SSU2 source authority is recorded (`SOURCES.md`
   Milestone 8 refresh, `09-ssu2.md` snapshot section,
   `IMPLEMENTATIONS.md` M8 pins, `support.toml` pointers).
2. New crate builds under the current toolchain and MSRV-shaped
   lints (`cargo check`, `cargo clippy -D warnings`).
3. Dependency direction is correct and mechanically enforced
   (checker allowlist row + green run).
4. `TransportKind::Ssu2` is integrated without changing NTCP2
   semantics (no new manager; transport tests unmodified and green).
5. v2 direct and introducer RouterAddress forms are strictly modeled
   (tests: direct IPv4/IPv6, direct-with-introducers,
   introducer-only, unpublished-static).
6. v3/v4 PQ versions are distinctly classified
   unsupported/deferred (`UnsupportedVersion`, incl. v5/empty/padded).
7. Malformed/duplicate/conflicting address options fail with typed
   errors (tests for each category).
8. Packet/header structural codecs are bounded and fixture-backed
   (exact-size decoders; 3 header fixtures consumed in tests).
9. Required v2 plaintext blocks implemented in this foundation
   round-trip correctly (21-block round-trip test).
10. Malformed/truncated/oversized block cases are deterministic and
    panic-free (per-byte truncation sweep; malformed matrix).
11. Unknown-block handling is bounded and spec-correct (1024-byte
    ceiling test; reserved/experimental skip test).
12. Fixture manifest/vector checker is green (5 rows pinned).
13. Protocol/runtime boundary checker covers the new crate (extended
    greps + green run; no Tokio/socket/`async`/testkit edge).
14. No UDP socket is opened anywhere by the new crate (boundary
    grep covers `UdpSocket`/`std::net` I/O; `IpAddr`/`SocketAddr`
    are pure data carriers as in `i2pr-transport-ntcp2`).
15. No handshake/interoperability/public support claim is made
    (`support.toml` surface stays experimental,
    `advertised = false`; dossier states the deferrals).
16. Full workspace quality floor passes (see validation record).
17. Routine CI passes on the exact Plan 155 closing commit
    (`33818955817`, success, including the new `Check SSU2
    vectors` step).
18. This record carries exact tests/results and sets
    `next_executable_plan = 156`.

## Handoff

Plan 155 is closed. Execute Plans **156 → 157 → 158 → 159 →
160 → 161** in order under the Plan 154 roadmap authority. Plan
156 owns the Noise XK handshake, header protection,
TokenRequest/Retry flow, bounded one-use token lifecycle,
RouterInfo fragmentation/validation, and replay/deadline state —
still with no UDP socket ownership.
