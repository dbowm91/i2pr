# Plan 381 — Live ELS2 external driver lane (i2pd direction)

Status: **in progress** — WP1 complete, WP2–WP5 open. See
`plans/closure/i2pcontrol-proposal-170/381-status.md`.

WP1 execution **corrected two facts this plan recorded at registration**; both
corrections are made forward, not by rewriting this plan:

- The plan's stop condition 1 asked whether `i2cp.leaseSetClient.psk` is settable
  from `tunnels.conf`, and the registration claimed the indexed `.nnn` spelling
  was not available. **Both spellings are accepted** — the reader is a prefix
  match (`libi2pd_client/ClientContext.cpp:465-473`). The real trap is that the
  value **must contain a `:`**: `libi2pd/Destination.cpp:1612-1620` keeps only
  the bytes after the first colon and drops a colon-less entry with no
  diagnostic at all. Verified six ways against the pinned binary. The keys
  **are** settable from `tunnels.conf`, so the publisher role keeps its shape.
- The plan's stop condition 3 said the pinned source tree is not retained and a
  source-level claim needs a re-fetch. **The tree is present** at exactly
  `635b013a612ff47278ef02acf8580a28e10e26c5` / `2.61.0`, so every source
  citation in this plan is source-level. What is not retained is
  *reproducibility on a fresh host*, which is recorded as a finding needing its
  own plan-of-record.

WP1.3 (the static evidence checker) is **deliberately deferred to WP5**: its
subject is the WP3 lane and the WP4 rows, and a checker written before them
would either fail or be weakened to pass over something that does not exist.

Subsystem: Proposal 170 / I2PControl, and Red25519 + ELS2 (shared number; see
`plans/global-number-collision-ledger.md` convention — no collision here, 381 is
free and Proposal 170/Red25519-ELS2 already holds 374/375/377/378/380).

Plan of record for the scope that Plan 374's closure named as its remaining work
and could not execute: <https://github.com/…/plans/closure/i2pcontrol-proposal-170/374-status.md>.

Predecessor: **Plan 380 passed**
(`380-status.md`, `passed-authorized-consumer-production-path-closed-with-two-defects-found-and-fixed`).
Plan 380 is this plan's closed hard dependency. Without it this lane cannot write
a single authorized row, because i2pr had no way to present a client credential.

## Why this plan exists, and why now

Plan 374 completed every pre-execution freeze item and verified its substrate:
stock i2pd 2.61.0 builds unmodified at `635b013a612ff47278ef02acf8580a28e10e26c5`,
and the Plan 303/306 controlled mesh was **verified running on this host**. Both
i2pd directions are reference-feasible and all three auth modes are implemented,
so no direction may be marked `reference-not-applicable`. The one remaining item
was a live ELS2 driver.

That driver did not exist and could not be built out of the existing lane:
`tests/integration/floodfill/run-i2pd.sh` starts three stock i2pd processes with
`sam`, `i2cp`, `http`, `httpproxy` and `i2pcontrol` all `enabled = false`, and
generates no `tunnels.conf` at all. So its non-floodfill role can neither host nor
consume a service. `tests/integration/els2/` contains only `reference-freeze.md`.

Plan 380 has now removed the last production-code reason for delay. What remains
is orchestration, and orchestration is exactly what a plan can close.

## What this plan is not

- **Not a second external lane.** This is Plan 374's scope, written as its own
  plan-of-record because Plan 374 cannot be re-executed without the driver. The
  i2pd reference pin is **unchanged**; no new reference version may be introduced.
- **Not Java.** Plan 375 owns the Java I2P direction and its own blocker.
- **Not a convergence plan.** Plan 377 owns convergence. This plan produces rows
  for the i2pd direction only.
- **Not a conformance gate.** Plan 378 owns that, and remains blocked on 377.
- **Not a type-5 advertisement.** Nothing here promotes `advertised`, and type 5
  stays `advertised = false` however many rows pass.

## The substrate, precisely

Established by executed commands, not by assumption:

| Fact | Where it is true today |
|---|---|
| The controlled mesh runs | `tests/integration/floodfill/run-i2pd.sh`, three stock i2pd processes, all `setsid`-ed, with `MAX_ATTEMPTS=1` frozen |
| Readiness is a file+log conjunction | `run-i2pd.sh:145-157` — `router.info` present **and** `Start listening on 127.0.0.1:<port>` in the log, 120 × 0.5 s |
| Fail-closed i2pd pin | `run-i2pd.sh:52-68` — executable, `source-revision.txt` equals the pin, `--version` contains `2.61.0` |
| Every lane subsystem is off | `run-i2pd.sh:115-126` — `sam`, `i2cp`, `http`, `httpproxy`, `i2pcontrol` all `enabled = false` |
| No `tunnels.conf` is generated | `run-i2pd.sh` `write_conf()` only; the stock tunnel set is empty |
| The i2pd pin is cached | `target/interop/cache/ssu2/i2pd/635b013a…/bin/i2pd`, overridable by `I2PR_I2PD_BIN` |
| A `.b33` parses as its own reference kind | `i2pr-service-tunnels/src/destination.rs:112-114`, before the b32 branch |
| …but a `.b33` service target is refused unless `delay_open` | `i2pr-service-tunnels/src/config.rs:1160-1176`, and TOML hardcodes `delay_open: false` (`i2pr-daemon/src/config.rs:2208`) |
| So the consumer service must be created over I2PControl | `i2pcontrol_tunnels.rs:1341` is the only setter — this is Plan 351 Gate 2, unchanged |
| i2pr's `[sam]` / `[i2cp]` default to disabled on loopback | `i2pr-daemon/src/config.rs` `RawSamConfig` (`enabled=false`, `127.0.0.1`, port 7656) and `RawI2cpConfig` (port 7654) |
| The i2pr consumer now has both branches | `service_product.rs:3745`, `begin_authorized` and `begin` (Plan 380) |
| A loopback payload harness already exists | `tests/integration/service-tunnels/fixtures/{http,irc,echo}_fixture.py`, `clients/sam_stream_fixture.py` |
| An i2pd `tunnels.conf` writer already exists | `run-plan214-applications.sh:336-361`, launched with `--tunconf=` at `:543` |
| …and a `.dat` → b32 derivation with an independent recompute check | `tests/integration/service-tunnels/clients/parse_i2pd_destination.py`, consumed at `run-plan214-applications.sh:587-646` |
| The Java lane already hosts a SAM destination | `run-java-floodfill.sh:377-420`, an inline python heredoc speaking the SAM line protocol |
| `els2` is wired to **no** CI workflow | the wired external lanes are sam / ssu2 / i2cp / m11-transit / m6-mixed-router / service-tunnels / three ntcp2 |

## The ELS2 configuration keys, verified at the pin

Read out of the **pinned binary** with `strings`, not from memory:

```text
i2cp.leaseSetType   i2cp.leaseSetAuthType   i2cp.leaseSetEncType
i2cp.leaseSetPrivKey  i2cp.leaseSetClient.psk  i2cp.leaseSetClient.dh
sam.enabled  sam.address  sam.port
```

**Correction to a durable artifact.** `reference-freeze.md:120` records
`i2cp.leaseSetClient.psk[.nnn]`. The 2.61.0 binary contains no indexed group —
only a single `i2cp.leaseSetClient.psk`. This plan corrects that line forward. It
matters: an indexed spelling would have produced a `tunnels.conf` that i2pd
silently ignored, and the lane would have failed with an authentication error that
looks exactly like a crypto defect. The freeze document is corrected here rather
than left to mislead the next executor.

## Work packages

Ordered so that each package ends with something verifiable, and so the cheap
gate comes first.

### WP1 — the cheap gate (no external process)

Everything here is `cargo test` / `bash` and needs no i2pd at all, so it can be
built and proven before the expensive lane exists.

1. A `tunnels.conf` writer/validator for the i2pd ELS2 roles, with the exact
   camelCase keys above and `deny_unknown_fields`-style rejection of anything
   else. Mirrored on `test-plan215-tunnels-conf.sh`.
2. A b33-aware destination extractor: derive the published `.b33` from the i2pd
   `.dat` **and independently recompute** the address↔hash relationship, so a
   wrong derivation fails rather than producing a plausible-looking address. The
   precedent and the recompute check are `run-plan214-applications.sh:640`.
3. The static evidence checker plus its `--self-test` and `--mutation-table`, on
   the `check-outproxy-wire-lane-evidence.py` model: `REQUIRED_ROWS`,
   `DOCUMENTED_ABSENCES` that **fire** if an absence becomes a real row,
   `REQUIRED_IN_PRODUCTION` triples each carrying the defect it would reintroduce,
   per-row rejection of `#[ignore]` / `#[should_panic]` / `cfg(`, and a re-run of
   `check-encrypted-service-consumer-caller.sh` because a green lane over a
   mutated request path proves nothing.
4. The R-side service creation over loopback I2PControl: a `.b33` client with
   `DelayOpen`, no floodfill, and the Plan 380 credential where the mode calls for
   it. Shaped like `i2pcontrol_els2_black_box.rs`.
5. `.github/workflows/els2-external.yml`.

### WP2 — the lane runner

`tests/integration/els2/run-i2pd-els2.sh`, on the `run-i2pd.sh` pattern: the same
fail-closed pin gates, the same `setsid` process groups, the same
`trap cleanup EXIT`, `MAX_ATTEMPTS=1` frozen, and a new lane profile that enables
`sam` on loopback for the consumer role and generates a `tunnels.conf` for the
publisher role. The `R` role is a **non-floodfill** identity; `F` is a distinct
process from `R`.

### WP3 — the driver

`crates/i2pr-daemon/tests/els2_i2pd_external.rs`, driving only through the public
surfaces: start R, create the service over I2PControl, issue a real loopback
application payload, and read a real result. No private bridge, resolver, driver
or pump API, and no decoded-LeaseSet injection.

### WP4 — the rows

- i2pd publishes, i2pr consumes, **application payload crosses both ways**.
- i2pr publishes, i2pd consumes.
- All three auth modes on the i2pr consumer side, which is the only side i2pr
  owns. `NONE` needs no credential and must stay working — that is Plan 380
  invariant 1.
- The negative rows: a wrong credential refused, a wrong lookup secret refused, a
  record for a different day refused.
- A `.b32.i2p` authority row, so the lane also proves the lane has not broken the
  ordinary path.

### WP5 — evidence and closure

`evidence.json` + `evidence.md` from sanitized counts and hashes only. Raw i2pd
logs are never evidence. Then the closure record, the registry, and both roadmaps.

## Invariants that must not regress

1. **No reference modification.** i2pd runs stock at the pin. No patch, no
   vendor, no rebuild.
2. **`MAX_ATTEMPTS = 1`.** No retry-until-green, no budget increase. A lane that
   needs a second try is a lane that found a bug.
3. **Missing environment fails.** If the pin, the cached binary, or a configured
   port is absent, the lane exits non-zero. Never silently skips, never returns
   success early.
4. **No `|| true`, no `continue-on-error`, no filename filtering, no fake peer
   env, no broad exclusions.**
5. **Raw reference logs are never evidence.** Sanitized counts and hashes only.
6. **No advertisement change.** Type 5 stays `advertised = false`;
   `full-proposal-conformant` stays unset; `support.toml` gains no surface.
7. **Every row goes over the real transport.** A row that reaches the result
   without a network round trip proves nothing and is refused by review.
8. **Plan 380's guard stays green.** The lane is not allowed to need a weakened
   consumer path.

## Stop conditions

Stop and register a corrective rather than continuing if any of these occur:

- **`i2cp.leaseSetClient.psk` cannot be set from `tunnels.conf` at all.** If the
  only route is a SAM `SESSION CREATE` option, the publisher role changes shape
  and that is a design decision, not a workaround.
- **i2pd client tunnels cannot be made to work in the controlled mesh.** The Java
  lane needed relay peers seeded mutually for exactly this; if the same turns out
  to be unsolvable here, the direction is blocked on topology, not on code.
- **The pinned source tree cannot be recovered** and a source-level claim is
  needed. The cache retains only `bin/`, `logs/` and build metadata, so re-proving
  `leaseSetAuthType` semantics in-repo needs a re-fetch. A source claim asserted
  from a binary's `strings` output is a weaker claim and must be labelled as one.
- **Any direction can only be satisfied by a partial matrix.** A half-run matrix
  recorded as evidence is worse than an honest block.
- **A row passes only because the reference was modified, a secret was weakened,
  or a decoded LeaseSet was injected.**
- **Correctness requires any change to the deployed type-11 transcript profile.**
  ADR 0032 governs that and this plan may not touch it.

## Verification

Ordered so the cheap gate proves itself before the expensive lane is attempted.

```text
# WP1 — cheap gate, no external process
cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_client_credential -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1
bash tests/integration/els2/test-tunnels-conf.sh
bash tests/integration/els2/run-i2pd-els2.sh --self-test
python3 scripts/check-els2-live-lane-evidence.py --self-test
python3 scripts/check-els2-live-lane-evidence.py --mutation-table

# WP2+WP3 — the lane, only once WP1 is green
bash tests/integration/els2/run-i2pd-els2.sh

# whole-repo floor, unchanged
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-encrypted-service-consumer-caller.sh
bash scripts/check-config-secret-hygiene.sh
python3 scripts/check-tooling-inventory.py
python3 scripts/check-workflow-validity.py
python3 scripts/check-global-plan-number-uniqueness.py
```

## Closure evidence required

- A requirement-to-evidence matrix covering every acceptance criterion, naming
  each row.
- Commands run with outcomes, labelled local or CI truthfully.
- A statement of what remains unproven, naming Plan 375's Java direction,
  Plan 377's convergence and Plan 378's gate explicitly.
- An invariant review against all eight items above.
- Findings by severity, with anything pre-existing attributed forward rather than
  absorbed, and its own plan-of-record named where it needs one.
- The roadmap disposition with an unblock audit.
- **An explicit statement of the i2pd auth-mode matrix actually executed**, since
  all three modes are implemented at the pin and none may be quietly omitted.

## Handoff

Plan 377 cannot converge until **both** Plan 374's rows and Plan 375's rows exist.
This plan delivers only the i2pd half. Plan 375's blocker is separate and is not
touched here.

Two drift findings are recorded at registration and belong to this plan's
follow-on, because both are boundary conditions rather than ELS2 work:
`scripts/check-els2-type11-transcript-boundary.sh` is in the `AGENTS.md` floor
but has **no `ci.yml` line**, and `tests/integration/floodfill/run-i2pd.sh` is in
**no workflow at all**. Neither is weakened or fixed by this plan.