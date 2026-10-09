# Plan 384 — i2pr ELS2 reverse publication + post-start authority successor lane (i2pd direction)

Status: **blocked-post-start-authority-lookup-and-reverse-publication-corrective-plan-385**.

Subsystem: Proposal 170 / I2PControl, and Red25519 + ELS2 (shared number — no
collision: 384 is free in every subsystem index, the ledger, and the registry;
see `plans/global-number-collision-ledger.md` convention).

Predecessors, all closed:

- **Plan 381 passed**
  (`plans/closure/i2pcontrol-proposal-170/381-status.md`,
  `passed-i2pd-consumer-direction-live-reverse-and-authority-parked-for-successor`).
  Plan 381 delivered the i2pd→i2pr consumer direction completely (NONE/PSK/DH
  payload rows, both live negatives, different-day unit, mesh authority control,
  evidence checker, packaged evidence) and parked exactly two capabilities with
  firing absence guards: the **reverse direction** (i2pr publishes, reference
  consumes — 4 live attempts, all `CANT_REACH_PEER / LeaseSet not found`, no
  floodfill holding i2pr's record) and the **i2pr-side `.b32` authority row**
  (three compositions tried, each exposing a real product boundary).
- **Plan 380 passed** (the authorized consumer path + typed `CustomOptions`
  credential seam). This plan's publisher-side credential ingress reuses that
  seam; no new Proposal 170 field is proposed or permitted.
- Plans 346 (deployed transcript profile, ADR 0032), 350 (floodfill type-5
  store/serve), 351 (consumer production caller), 337/338 (one shared service
  manager, one identity-store owner) are closed interface/production deps.

Plan of record for the scope that Plan 381's closure parked and could not
execute: `plans/closure/i2pcontrol-proposal-170/381-status.md` §§ "WP4 parked
rows", "Findings, attributed forward" (10–15), "Closure outcome".

## Why this plan exists, and why now

Plan 381's unblock audit is explicit: **Plans 374, 375, 377, 378 stay blocked**;
the successor plan unblocks 374's remainder alongside 375. The consumer
direction is done and repeat-green; what is missing is not another consumer
matrix but two separate capabilities the parked rows proved absent:

1. **Reverse publication.** i2pr's encrypted server creates over I2PControl,
   material installs synchronously, the `.b32.i2p` address encodes, the
   reference SAM connects reach the mesh — and then every consume attempt
   answers `LeaseSet not found`. Open questions, recorded with evidence: whether
   i2pr's publication ever leaves the router, and whether stock i2pd accepts
   i2pr's deployed type-11 profile live (Plan 346 is crypto-boundary only; Plan
   335 measured rejection of the pre-346 form).
2. **Post-start authority.** Finding 10: a control-created ordinary client is
   never provisioned post-start — no remote-LS2 fetch after the startup pass —
   so any operator-created ordinary client post-start is dead on arrival.
   Finding 11: product-`ServiceTunnelSet` specs never reach the shared
   manager's committed generations (the manager builds from daemon config
   only), silently absent with no error. Finding 12: the startup ordinary
   lookup (3 attempts) can exhaust against ungossiped floodfills, and ordinary
   `DatabaseLookup` interop against stock references is itself unqualified.

The i2pr-side authority row needs all three fixed first — which is why it is
one plan, not three: each fix is load-bearing for the row, and the row is the
only proof any of them worked.

## What this plan is not

- **Not Java.** Plan 375 owns the Java I2P direction and its own blocker.
  This plan does not unblock 375; it unblocks 374's remainder *alongside* 375.
- **Not convergence, not the final gate.** Plans 377/378 own those and stay
  blocked until 374's remainder (this plan) *and* 375 both pass.
- **Not a second consumer matrix.** The NONE/PSK/DH consumer rows are 381's
  and stay green unmodified; this plan may run them as controls but claims no
  consumer row.
- **Not a transcript change.** ADR 0032 governs the deployed type-11 profile
  and this plan may not touch it. If the reverse direction fails inside the
  signature profile, that is a stop condition with a corrective, not a patch.
- **Not an advertisement.** Type 5 stays `advertised = false`;
  `full-proposal-conformant` stays unset; `support.toml` gains no surface.
- **Not fresh-host reproducibility.** That is 381's standing finding with its
  own plan-of-record, untouched here.
- **Not finding 15.** The `router.info` rewrite/read race is deferred unless
  it recurs, in which case the handoff is snapshot-and-verify.

## Work packages

Ordered so the cheap, local gates come first and the lane never runs over a
foundation the unit rows have not pinned.

### WP1 — the local gates (no external process)

1. **Control `get` projection unit test (finding 14).** Pin that a control
   `get` projects `encrypted_address` once the material is installed. The 381
   draft polled for 120 s while the material sat installed; the question
   (projection lag vs poll bug) gets a unit test, not another lane. The
   reverse driver (WP2) reads installed material directly until this test
   passes, and the bypass is removed only after it does.
2. **Post-start ordinary provisioning (finding 10).** A control-created
   ordinary client must either be provisioned post-start (remote-LS2 fetch
   after the startup pass) or be refused loudly at create time. Silently
   parking on an unresolved target is the defect; either behavior is an
   acceptable close, decided in code review, not worked around in the lane.
   Unit + black-box rows over loopback I2PControl, no reference needed.
3. **Committed-generations surface-or-refuse (finding 11).** A
   product-`ServiceTunnelSet` spec that never reaches the shared manager's
   committed generations must surface or refuse — never sit silently absent
   with no error. Same test shape as (2).
4. **Gossip-convergence gate + selection audit (finding 12).** The lane must
   prove its floodfills gossiped i2pr's record *before* a consume attempt
   counts: a `LeaseSet not found` against an ungossiped floodfill is
   unattributable and fails the gate, not the row. Audits which floodfills
   the lane selects and why; the 3-attempt startup lookup stays as-is unless
   the audit proves it is the exhausting party, in which case the retry
   budget change is its own reviewed production diff with a regression row.

### WP2 — the reverse rows (stock i2pd 2.61.0 @ `635b013a…`, `MAX_ATTEMPTS=1`)

On the 381 lane substrate unchanged (same mesh, same `setsid` groups, same
fail-closed pin gates, same key hygiene: fresh 32-byte publisher credential
per authorized run, mode/presence recorded never values, hex + both-base64
scrubs): i2pr publishes an encrypted server, stock i2pd consumes, an
application payload crosses.

- Reverse payload, auth NONE.
- Reverse payload, auth PSK (publisher `leaseset_password` via the 380 seam;
  reference-side key in `tunnels.conf` — both PSK spellings are accepted, the
  value **must contain a `:`** per 381 WP1).
- Reverse payload, auth DH.
- Mesh-side authority control every run (reference consumes reference's
  standard LS2 — proves the mesh, not i2pr).
- The 381 consumer NONE row re-run as a control (proves the lane did not
  regress the delivered direction).

The auth-mode matrix actually executed is stated explicitly at closure; all
three modes are implemented at the pin and none may be quietly omitted. A
partial matrix recorded as evidence is worse than an honest block (381 stop
condition, inherited).

### WP3 — the i2pr-side authority row

An ordinary (non-encrypted) i2pr client, created post-start over loopback
I2PControl, resolves a reference publisher's standard LS2 and carries a
payload — through the WP1-fixed provisioning path, against floodfills the
WP1 gossip gate attests. This row is 377's i2pr-side authority input; it
passes only if findings 10–12 stayed fixed under a live reference.

### WP4 — evidence and closure

- Extend `scripts/check-els2-live-lane-evidence.py` (+ `.sh` wrapper):
  new `REQUIRED_ROWS` for WP2/WP3, gossip-gate section, WP1 unit-row
  markers. The 381 parked-row absences become rows **only after they pass** —
  until then the absence guards keep firing (the 342/376 pattern: absences
  are removed by passing replacements, never waived).
- `evidence.json` + `evidence.md` from sanitized counts/hashes only. Raw
  reference logs are never evidence.
- Closure record with requirement-to-evidence matrix, local-vs-CI labelled
  commands, invariant review, findings by severity attributed forward, and
  the unblock audit: on pass, 374's remainder is executable-complete pending
  375; 377 still needs 375; 378 still needs 377. Registry + both roadmaps.

## Invariants that must not regress (inherited from 381, all binding)

1. **No reference modification.** Stock i2pd at the pin. No patch, no vendor,
   no rebuild.
2. **`MAX_ATTEMPTS = 1`.** A lane that needs a second try found a bug.
3. **Missing environment fails.** Absent pin/binary/port exits non-zero.
   Never silently skips, never early-success.
4. **No `|| true`, no `continue-on-error`, no filename filtering, no fake
   peer env, no broad exclusions.**
5. **Raw reference logs are never evidence.** Sanitized counts/hashes only.
6. **No advertisement change.** Type 5 `advertised = false`;
   `full-proposal-conformant` unset; no `support.toml` surface.
7. **Every row goes over the real transport.** No decoded-LeaseSet
   injection, no private bridge/resolver/driver/pump API in the driver.
8. **381's consumer rows and 380's guard stay green unmodified.**
   (`check-encrypted-service-consumer-caller.sh` pins the caller spelling;
   fix code, never the script.)

## Stop conditions

Stop and register a corrective rather than continuing if any of these occur:

- **The reverse direction fails inside the deployed signature profile.**
  If stock i2pd rejects i2pr's type-11 material the way Plan 335 measured
  against the pre-346 form, that is a transcript-profile question owned by
  a corrective under ADR 0032 — not a lane retry, not a local patch.
- **i2pr's publication never leaves the router.** If the gossip gate keeps
  failing with material installed and address encoded, the publication path
  (not the lane) is the defect; the corrective owns the sweep from
  `publish_service_ls2_for_service` to the wire.
- **Post-start provisioning can only be satisfied by a startup semantic
  change.** If findings 10/11 need more than activation policy
  (e.g. lifecycle or supervision changes), that scope is a new plan, not a
  WP1 stretch.
- **Any direction can only be satisfied by a partial matrix.**
- **A row passes only because the reference was modified, a secret was
  weakened, or a decoded LeaseSet was injected.**
- **Correctness requires any change to the deployed type-11 transcript
  profile, the frozen 75-name Proposal inventory, or 380's credential seam.**

## Verification

```text
# WP1 — local gates, no external process
cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1
cargo test --locked -p i2pr-daemon --test <post-start-provisioning-rows> -- --test-threads=1
bash tests/integration/els2/run-i2pd-els2.sh --self-test
python3 scripts/check-els2-live-lane-evidence.py --self-test
python3 scripts/check-els2-live-lane-evidence.py --mutation-table

# WP2+WP3 — the lane, only once WP1 is green
I2PR_ELS2_AUTH_MODE=none bash tests/integration/els2/run-i2pd-els2.sh
I2PR_ELS2_AUTH_MODE=psk bash tests/integration/els2/run-i2pd-els2.sh
I2PR_ELS2_AUTH_MODE=dh bash tests/integration/els2/run-i2pd-els2.sh

# whole-repo floor, unchanged (verbatim from AGENTS.md at execution time)
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
bash scripts/check-els2-live-lane-evidence.sh
bash scripts/check-encrypted-service-consumer-caller.sh
bash scripts/check-config-secret-hygiene.sh
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
```

Reference pins are frozen (i2pd `2.61.0` @
`635b013a612ff47278ef02acf8580a28e10e26c5`). Environment-gated rows are
`#[ignore]`-gated; explicit runs require `--ignored --exact`; missing env
fails, never silently passes.

## Closure evidence required

- Requirement-to-evidence matrix naming every acceptance row (WP2 reverse
  NONE/PSK/DH, WP3 authority, WP1 gates, mesh control, consumer control).
- Commands run with outcomes, labelled local vs CI truthfully.
- Explicit statement of the reverse auth-mode matrix actually executed.
- Invariant review against all eight items above.
- Findings by severity, pre-existing items attributed forward with their own
  plan-of-record named where needed.
- Roadmap disposition with the unblock audit: what 374's remainder still
  needs (375's Java half), what 377/378 still need, nothing claimed early.
- The 381 parked-row absences retired by passing rows, with the exact
  checker diff; any absence still firing named as still-open.

## Handoff

On pass, Plan 374's i2pd remainder is delivered: consumer direction (381) +
reverse + authority (this plan). Plan 377 still needs Plan 375's Java half,
which this plan does not touch; Plan 378 still needs 377. No support,
advertisement, or conformance claim moves until those close.
