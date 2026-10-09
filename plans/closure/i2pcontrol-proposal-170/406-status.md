# Plan 406 closure — i2pd authority and reverse matrix passed

Status: **passed-healthy-i2pd-authority-and-none-psk-dh-matrix-plan384-scope-delivered**.

Plan: `plans/implementation/i2pcontrol-proposal-170/406-authority-stream-stage-and-final-matrix-corrective.md`.

Implementation commit: `2f54014` (`Complete i2pd ELS2 reverse qualification chain`).

## Requirement-to-evidence

| Requirement | Evidence | Result |
|---|---|---|
| Reference process health gates the driver | The runner records named `f/c/n` process health before and after the i2pr driver; all six observations are `alive` in each final lane. The pre-driver gate prevents driver execution on an unhealthy mesh. | Pass, all three lanes |
| Authority connection stage is observable without exposing identities or payloads | `authority-active-connections-peak=1` in NONE, PSK, and DH. The row samples the existing bounded manager accessor; no new production counter was added. | Pass |
| Reverse payload and standard post-start authority payload | The stock i2pd requester and i2pr ordinary authority client both exchange their expected application payloads in each mode. Existing exact-type-7 profile and credential-scrub controls pass. | Pass, all three modes |
| NONE final lane | `target/interop/els2-evidence-plan406-none-connection-stage-20261009`; evidence SHA-256 `667c03d37ca3afc4edb90417f2288e64d8ebfcc1ad0ae061c8594b1a72dc271c`, results `2df1157881ca10cf4044a7d202fcbd0758bb9688fa8f7d06bec1f74beb9352ed`, driver `ada42314915ed0f1debede0d92b1f4516756c967052e47215e3b04c68596cbfe`. | Pass |
| PSK final lane | `target/interop/els2-evidence-plan406-psk-connection-stage-20261009`; evidence SHA-256 `7eb26813b2f81e873fb49d28a1f26fbd5d1a3bda55e32fe1a58522d07b5ef15a`, results `79fb3345009292f0bc3996ad00901d7d81b0727ff032f262271b4fb116cd85f8`, driver `6597ebc74b526188a10d71e8d11374ea0b8c782241c7a5d7363e898e51ef27fa`. | Pass |
| DH final lane | `target/interop/els2-evidence-plan406-dh-connection-stage-20261009`; evidence SHA-256 `8782e2cb9b9b30f3d740b948c82cce0e8dfc198051ec8bf0b77a377a91640c9a`, results `2067baa542607dac9ad1f9a619aa3088f2632f86cc49b4bdf7fc321919f763b8`, driver `128a356e53a9bfd4d1f1b4516756c967052e47215e3b04c68596cbfe`. | Pass |
| Routine floor | Full AGENTS.md floor completed locally. Workspace tests: 4,678 passed, 36 ignored across 183 suites. All remaining floor commands completed with exit status 0. | Pass; command inventory below |

## Live lane and retained history

All three invocations used the exact pinned stock i2pd 2.61.0 mesh, unique evidence
directories, and `MAX_ATTEMPTS=1`. For each mode the reference ELS2 round-trip,
reference standard self-connect, family/gossip controls, ordinary post-start authority
payload, reverse payload, explicit SAM signature type 7, credential scrub, pre/post
process health, and activity sampling passed. Each consumer and authority activity peak
was one accepted connection.

The prior failures remain preserved. Plan 404's first DH attempt failed before resolver
status and coincided with a reference-process crash; its subsequent identical run passed.
Plan 405's first NONE attempt had an unhealthy reference process; the second completed
lookup but failed to return the authority payload and observed no stream-connect start.
Plan 406's health and activity gates were added to distinguish those conditions, and the
final matrix passed on healthy meshes. These earlier failures remain unreproduced and
undiagnosed; no raw reference logs or payloads are committed.

## Commands and outcomes

The complete routine floor from `AGENTS.md` passed locally:

- `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets`, and the
  managed-app sibling binary build passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` passed: 4,678
  passed, 36 ignored, 183 suites, 769.30 seconds.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`, and
  `cargo test --locked --workspace --doc` passed. Clippy initially identified seven
  needless borrows in the new test instrumentation; those were removed, then Clippy
  passed cleanly.
- Dependency direction, global plan-number and ADR-number uniqueness, portable API and
  consumer checks, 51 planning tests, runtime/console/service-tunnel/managed-app/M11
  boundaries, fixture and NTCP2/SSU2/I2CP vector guards, NTCP2 interoperability static
  guard, constrained-host and M11 guards, SAM/SSU2/I2CP/I2PControl/ELS2 evidence guards,
  outproxy and secret hygiene guards, workflow validity, floodfill/service-tunnel/
  exploratory/NetDB/destination/Streaming/M6 evidence guards, M12 self-tests, NTCP2
  harness execution tests, and `cargo deny check advisories bans sources` all passed.
- `cargo test --locked -p i2pr-daemon --test els2_i2pd_external -- --test-threads=1`
  passed as an ordinary environment-gated test run (0 passed, 1 ignored); all three
  exact-pinned external lane runs above were invoked explicitly and passed.
- Plan 406's ELS2 checker, runner self-test, live evidence guard, encrypted-consumer
  caller guard, focused daemon test-target check, formatting, and final planning tests
  passed. `git diff --check` passed.

`cargo deny` reported the existing duplicate `windows-sys` lock entries as warnings;
the advisories, bans, and source checks completed successfully. The M6 mixed-router
evidence checker emitted its existing unbound-label warnings but exited successfully.
No unrelated warning was treated as a Plan 406 product result.

## Compatibility, security, and disposition

No production behavior changed in Plan 406; the final edits are test instrumentation,
health gates, guards, and planning records. The implementation chain 388–406 contains
the product corrections documented in each successive plan. The final profile remains
explicit SAM signature type 7, consistent with ADR 0004. No reference router was
patched, no dependencies or protocol fields were added, and no transcript, support,
capability, conformance, or advertisement state changed. ELS2 type 5 and full Proposal
170 remain unadvertised; `full-proposal-conformant` remains unset.

The live NONE/PSK/DH matrix completes the i2pd remainder of Plan 374 and delivers the
scope of Plan 384. The original Plan 374 and Plan 384 closure records remain immutable
point-in-time blocked records; this closure supersedes those blocked snapshots for the
completed i2pd scope. Plans 385–405 likewise remain historically accurate records of
each intermediate blocker and corrective handoff.

## Unblock audit

- Plan 374's i2pd scope is delivered by this closure; Plan 384's reverse and authority
  scope is complete. No new work is required for those scopes.
- Plan 375 remains blocked on Java pinned-source proof and its Java live matrix.
- Plan 377 remains blocked on Plan 375; Plan 378 remains blocked on Plan 377. The i2pd
  rows do not satisfy Java evidence or the four-direction convergence gate.
- Plan 376 is already passed. No other blocked plan has all hard dependencies closed
  because of Plan 406, so no additional plan is moved to `ready`.

Roadmap disposition: **closed for the scoped i2pd qualification; Java and full Proposal
170 conformance remain open and unclaimed.**
