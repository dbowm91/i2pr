# Plan 036 closure: interoperability, adversarial validation, and evidence boundary

Date: 2026-07-15

Status: **blocked for milestone closure; local validation and evidence
infrastructure complete**.

Plan 036 is an evidence and integration plan. This checkout completed the
repository-side validation work without changing production activation or
support claims, but the required mixed-router runs were not executed. The
daemon still keeps live `run` activation disabled, and the pure Plan 033/034
handshake/data owners have not been composed with the Plan 035 runtime socket
owner into a complete wire-level adapter. No Java I2P or i2pd result is
therefore claimed.

## Scope and changed files

The implementation/evidence change adds:

- `tests/integration/ntcp2/manifest.toml`, a sanitized private-testnet
  manifest with pinned Java I2P and i2pd source revisions and a complete
  eight-scenario reference matrix;
- `tests/integration/ntcp2/README.md` and
  `tests/integration/ntcp2/evidence/README.md`, which define manual execution,
  teardown, artifact hashes, typed outcomes, and prohibited evidence;
- `scripts/check-ntcp2-interoperability.sh`, a fail-closed manifest and
  evidence-boundary preflight used by Linux CI;
- a fixed-seed `0..=255` testkit campaign in
  `crates/i2pr-testkit/tests/milestone_3.rs`;
- synchronized guidance in `AGENTS.md`, `README.md`, `docs/architecture.md`,
  `docs/private-testnet.md`, `docs/protocol-support.md`,
  `docs/security-model.md`, and `fuzz/README.md`;
  `specs/protocols/03-ntcp2.md`, `specs/CONFORMANCE.md`,
  `specs/support.toml`, and `.github/workflows/ci.yml`;
- normal-CI execution of the NTCP2 vector and interoperability-boundary
  checks; and
- this plan-specific record and the aggregate Milestone 3 record.

The exact changed-file set is the files named above plus
`tests/integration/ntcp2/manifest.toml`,
`tests/integration/ntcp2/README.md`,
`tests/integration/ntcp2/evidence/README.md`,
`plans/036-closure.md`, and `plans/030-milestone-3-closure.md`.

No production crate dependency, public API, socket activation default,
RouterInfo publication path, NetDB behavior, or capability advertisement was
added. No prior Plan 032–035 correction was required by the local evidence.

## Controlled reference pins

| Reference | Released version | Source revision | Execution here | Evidence status |
| --- | --- | --- | --- | --- |
| Java I2P | 2.12.0 | `2800040` | Not run | Pin recorded; binary/configuration hashes must be recorded per run |
| i2pd | 2.60.0 | `f618e41` | Not run | Pin recorded; binary/configuration hashes must be recorded per run |

The manifest uses network ID `synthetic-private-036`, loopback IPv4/IPv6
binds, disabled reseed/bootstrap, disposable identities and static keys, and
fixed scenario clocks. It explicitly prohibits operational identities,
public addresses, peer lists, payload captures, raw logs, and secret-bearing
artifacts. The preflight checks those repository-side invariants but does not
start either reference router.

## Required interoperability matrix

| Reference | Scenario group | Directions | Address families | Expected result | Actual result |
| --- | --- | --- | --- | --- | --- |
| Java I2P 2.12.0 | handshake/data | i2pr initiator and reference initiator | IPv4; IPv6 where available | authenticated handshake and bounded I2NP exchange | **blocked: complete wire-level i2pr adapter unavailable** |
| Java I2P 2.12.0 | padding/skew/replay/identity/network failures | both | IPv4; IPv6 where available | typed rejection within bounds | **blocked: authorized runner unavailable** |
| Java I2P 2.12.0 | duplicate-link race | both | IPv4 | deterministic winner, loser drain, no churn | **blocked: authorized runner unavailable** |
| i2pd 2.60.0 | handshake/data | i2pr initiator and reference initiator | IPv4; IPv6 where available | authenticated handshake and bounded I2NP exchange | **blocked: complete wire-level i2pr adapter unavailable** |
| i2pd 2.60.0 | padding/skew/replay/identity/network failures | both | IPv4; IPv6 where available | typed rejection within bounds | **blocked: authorized runner unavailable** |
| i2pd 2.60.0 | duplicate-link race | both | IPv4 | deterministic winner, loser drain, no churn | **blocked: authorized runner unavailable** |

No successful handshake, I2NP delivery, duplicate-link result, or reference
router log has been imported. The sanitized evidence format is defined but
contains no run records; this is intentional and prevents a missing external
lane from being represented as a skip-success.

## Local adversarial and resource evidence

The local evidence is bounded and does not contact a network:

| Boundary | Local evidence | Claim |
| --- | --- | --- |
| Handshake codecs/state sequences | Plan 033 tests, `ntcp2_handshake` fuzz target, partial/truncated/mutation tests | pure local rejection and ownership evidence |
| Authenticated blocks/frames | Plan 034 tests, `ntcp2_blocks`/`ntcp2_frames` targets, tag/length/order/terminal tests | pure local authentication and parser bounds |
| Replay/skew/address policy | Plan 033/035 tests and `ntcp2_handshake` inputs | deterministic policy evidence |
| Admission/queue/backoff/children | Plan 035 runtime tests and `i2pr-testkit` resource tests | bounded local cleanup evidence |
| Scheduling variation | fixed-seed testkit matrix for seeds `0..=255`, bounded stream delay schedules | 256 deterministic local runs; no router interoperability claim |
| Artifact sanitation | `scripts/check-ntcp2-interoperability.sh` | committed evidence boundary only |

The full slowloris, malformed, stress, and fault-injection matrix remains an
authorized private-testnet responsibility. It must not be directed at the
public I2P network.

## Security and privacy review

The review found no new production secret owner. Existing owners remain:

- router identity and NTCP2 static-key records use the storage boundaries and
  zeroizing wrappers established by Plans 013 and 032;
- handshake transcript/cipher and directional frame owners remain consuming,
  non-debuggable where secret-bearing, and terminal on authentication failure;
- runtime owns sockets, tasks, timers, replay entries, admission leases,
  queues, and child joins;
- default snapshots/events retain only typed categories, synthetic IDs,
  coarse address family, and bounded counters.

The committed integration path contains no key, identity, payload, capture,
or operational endpoint. Its preflight rejects private-key PEM markers,
captures, identity files, and static-key files. The operator lane must delete
all disposable secrets and processes after each scenario and retain only typed
outcomes plus artifact/configuration/evidence hashes.

## Fuzz and deterministic campaigns

The repository has pure fuzz targets for handshake parsers/state commands,
authenticated blocks, frames/counter transitions, transcript/KDF behavior,
and storage records. The required commands and final run results are recorded
in the aggregate closure. On 2026-07-15, `scripts/fuzz-smoke.sh` completed 22
targets at 32 runs each (704 bounded executions), and the four critical NTCP2
targets completed 1,000 runs each with seed `36`:

```text
RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir fuzz ntcp2_handshake -- -runs=1000 -seed=36
RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir fuzz ntcp2_blocks -- -runs=1000 -seed=36
RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir fuzz ntcp2_frames -- -runs=1000 -seed=36
RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir fuzz ntcp2_transcript -- -runs=1000 -seed=36
```

The campaigns used `cargo-fuzz 0.13.2` and `rustc 1.97.0-nightly
(f964de49b, 2026-05-07)`, with `LSAN_OPTIONS=detect_leaks=0` for the managed
ptrace environment. The first handshake run completed its 1,000 inputs but
then hit the known LeakSanitizer/ptrace shutdown failure; the rerun with the
documented setting passed. No parser crash, timeout, or OOM was observed and
all generated corpus/artifact files were removed before handoff. The fixed-
seed integrated campaign is committed as one bounded Rust test and covers
exactly 256 seeds with no wall-clock sleep or network access.

## Support ledger and activation decision

All NTCP2 rows remain `status = "experimental"` and `advertised = false`.
Evidence paths now include the Plan 036 closure and the pinned integration
manifest, but the absence of actual reference results prevents a status
transition. This is synchronized in `docs/protocol-support.md` and the NTCP2
dossier.

The operator activation decision is option 1 from Plan 036: keep live
`run`/listener activation disabled after this plan. The integration path is
the only socket-entry point for a future explicitly authorized private lane;
it is not a public-listener or publication feature.

## Blocker and handoff

Milestone 3 cannot close until a later authorized execution supplies:

1. a complete bounded runtime adapter that translates the Plan 033/034 action
   and frame owners through the Plan 035 socket/task owners;
2. disposable private-testnet configurations and exact binary/image hashes;
3. Java I2P and i2pd inbound/outbound runs for the manifest matrix;
4. sanitized typed results showing I2NP exchange, duplicate-link stability,
   adversarial rejection, and zero/expected cleanup counters; and
5. the exact CI/manual run identifiers and reproduction commands.

Until then, the aggregate record marks Milestone 4 readiness **not ready**.
