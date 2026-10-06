# Plan 352 — daemon config secret hygiene: status

Status: **passed-structural-toml-error-redaction-and-conditional-at-rest-mode-gate**

Plan of record:
[`352-config-secret-hygiene.md`](../../implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md).
Origin: found while scoping Plan 351's Gate 3; **pre-existing**, not caused by any ELS2 work.
Landed on `main` on top of `2416c30`.

## The headline

Both defects are closed, and the closure is **structural** rather than cosmetic.

The important discovery is that **message filtering was never going to work**. A temporary probe
(`zz_probe_toml.rs`, since deleted) rendered all three leak shapes before any fix and showed:

| Shape | Leaks the secret through `Display`? | Leaks through `message()`? |
|---|---|---|
| TOML syntax error on the password line | yes | yes |
| type error (`String` where `u64` expected) | yes | yes |
| `deny_unknown_fields` (`bogus_<secret>`) | yes | **yes** |

The third row is the decisive one. `toml`'s unknown-field rejection embeds the *offending key* in the
message, so `deny_unknown_fields` leaks through `message()` even if `Display` were already redacted.
Any design that filtered rendered strings would have shipped a guard that passes while the secret
still reaches stderr. The fix therefore replaces what the error **carries**, not what it says.

## D1 — structural redaction

`RedactedTomlError` (`crates/i2pr-daemon/src/config.rs:3223`) holds a resolved `(line, column)` pair
and the upstream `toml::de::Error`, and holds **no content**:

- `new()` resolves `error.span()` through `position_of` (`config.rs:3261`), which counts newlines in
  the supplied `&str` and never retains it. Out-of-range offsets degrade to `(None, None)` rather
  than panicking on a slice.
- `Display` (`config.rs:3274`) emits `TOML parse error at line L, column C [source content
  redacted]`. It has no access to the retained error, so it *cannot* render it.
- `Debug` delegates to `Display` — this closes the `{:?}` path too, since a `thiserror` derive would
  otherwise have reintroduced the leak through `Debug`.
- `Error::source` (`config.rs:3294`) returns the retained error for programmatic callers, so no
  diagnostic is lost.
- `ConfigError::Parse(#[source] RedactedTomlError)` (`config.rs:3364`).

The `Display`/`Debug` split is load-bearing: `DaemonError::Config` is `#[error(transparent)]`, so
`main.rs:51`'s `eprintln!("error: {error}")` formats the top-level `Display`. If `Debug` had kept a
derived form, any caller unwrapping with `{:?}` — including test harnesses and log adapters — would
have re-leaked. The guard asserts no chain-walking printer is added to this path.

### Citation correction

The plan of record cites `toml-1.1.6+spec-1.1.0/src/de/error.rs:138` as the line that does
`writeln!(f, "{content}")`. The verified location is **line 140**; line 138 is
`write!(f, "{line_num} | ")?;`. The substance is unchanged — the whole source line is printed
verbatim — but the line number was wrong in both the plan and the guard header, and both are
corrected here. The plan's other citation, `serde-1.0.228/src/core/de/mod.rs:410`
(`Str(s) => write!(formatter, "string {:?}", s)`), is **exact** and was re-verified.

## D2 — the at-rest permission gate

The config was the only secret-bearing file in the daemon with no mode check; `i2pr-storage`,
`i2pcontrol_tunnels.rs`, and `addressbook.rs` all already enforce `& 0o077 != 0`.

`secret_file_permission_verdict(path, mode)` (`config.rs:3310`) is the **policy**, deliberately
platform-independent: `None` → refuse, `Some(mode) if mode & 0o077 != 0` → refuse, otherwise accept.
`check_secret_file_permissions` is the thin platform `cfg` that supplies the mode — `#[cfg(unix)]`
reads `PermissionsExt::mode() & 0o7777` (`config.rs:3331`), `#[cfg(not(unix))]` passes `None`
(`config.rs:3353`).

Two deliberate properties:

1. **The gate runs after parsing and only when a password exists** (`config.rs:1694`). A secret-free
   config is never rejected, which is what keeps "no production behaviour change for a
   secret-free config" true rather than aspirational.
2. **The decision is factored out so the non-POSIX refusal is testable on Linux.** Had the refusal
   lived inside the `cfg(not(unix))` arm it would be code no CI run on this host could execute — a
   row that has never failed is a comment, not a test.

The two new `ConfigError` variants carry a `PathBuf` and a `u32`, never content:
`InsecureConfigPermissions` and `InsecureConfigPermissionsUnsupported`.

### Platform behaviour, stated explicitly

**On Windows this is a deliberate behaviour change.** Enabling `[i2pcontrol]` with a password now
**fails closed** instead of loading, because there is no POSIX mode to verify. The plan's stop
condition forbids a silent pass, and accepting on an unexamined platform is the failure mode that
matters for a credential. The consequence is recorded rather than smoothed over: a Windows operator
must move the secret out of the config (`outbound_secret.rs` holds the sealing mechanism) or the
platform gate must be designed. **No Windows runtime evidence exists** — the `#[cfg(not(unix))]`
rows compile only on non-POSIX, and only the policy function is exercised here. This is recorded as
unverified, not as passing.

## D3 — recorded, not resolved

`Config` keeps `#[derive(Clone, Debug, Eq, PartialEq)]` and `CommandOutcome` keeps
`Debug + PartialEq` while embedding it. The plan puts derive removal out of scope as a wide
breaking change and requires the residual be recorded with a named follow-on. This section is that
record.

**`I2pControlPassword` keeps its `Clone` derive.** The plan permits removing it (preferred, and
consistent with `OutboundSecretKey`) *or* recording why not. Reason: the only production clone site
is a service-spec closure at `crates/i2pr-daemon/src/lib.rs:775`, so removing the derive is a
production refactor rather than a local edit, and the derive is not itself an exposure — `Debug` is
already hand-written and redacting (`config.rs:323`, rendering `I2pControlPassword([redacted])`).
The guard asserts the **redaction** rather than pretending the `Clone` is gone.

**Residual risk, unchanged and named:** `CommandOutcome` derives `PartialEq`, so a future
`assert_eq!` on it panics through `{:?}` and dumps the whole config. Current tests use `matches!`, so
nothing prints today. A future *inline secret* config field would be dangerous even with D1 and D2
closed. Follow-on: remove `PartialEq` from `CommandOutcome`, or drop the embedded `Config` from it.

## Evidence

Base commit `2416c30`. Working tree: 5 files changed (+394/−45) plus one new 312-line test file.

### Requirement → evidence

| Plan requirement | Evidence | Result |
|---|---|---|
| In-scope 1: redacting wrapper, line/column preserved | `config.rs:3223-3297`; rows 1, 2, 3, 5 | met |
| In-scope 2: `& 0o077` gate, fail-closed, Windows decision documented | `config.rs:3310-3355`, `:1694`; rows 6, 7, 8, 10, 11 | met |
| In-scope 3: negative test proving no secret reaches stderr | row 3 (the `message()` leak) | met |
| In-scope 4: negative-tested static guard | §4/§5/§6; 11/11 N-rows | met |
| Required evidence: syntax error | row 1 — no secret, keeps `line 3`, marker present | met |
| Required evidence: type error | row 2 — no secret, keeps `line 2` | met |
| Required evidence: `deny_unknown_fields` | row 3 — no secret **through the message** | met |
| Required evidence: `0644` + password refused, names path and mode | row 6 — names `644` and `600`, no secret | met |
| Required evidence: `0600` + password accepted | row 7 — password survives | met |
| Required evidence: no secret accepted at `0644` | row 8 | met |
| Required evidence: no-secret error still reports line **and** column | row 5 | met |
| Required evidence: guard negative-tested | 11/11 mutations detected | met |
| AC1–AC3, AC5, AC7 | as above | met |
| AC4 (`Config`/`CommandOutcome` never `Serialize`d) | zero `Serialize` derives in `config.rs`; guard assertion 2 | met |
| AC6 (residual recorded with named follow-on) | D3 above | met |
| **AC8 (exact-head routine CI green)** | **no CI in this environment** | **UNPROVEN** |

### Commands (all local; no CI available in this environment)

```text
cargo test --locked -p i2pr-daemon --test config_secret_hygiene -- --test-threads=1   11 passed
bash scripts/check-config-secret-hygiene.sh                                            passed
bash scripts/check-dependency-direction.sh                                            passed
bash scripts/check-runtime-boundaries.sh                                              passed
AGENTS.md routine floor (46 steps, re-parsed from the merged file)                    46 pass / 0 fail
```

The full floor includes `cargo fmt --all --check`, `cargo check --locked --workspace
--all-targets`, `cargo test --locked --workspace --all-targets -- --test-threads=1`,
`cargo clippy -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `cargo test --doc`, and all
`check-*.sh` / `check-*.py` guard scripts. Steps 3 and 6 (the workspace test and doctest runs)
dominate the wall clock; see "Why the floor is slow" below.

### Row-level mutation testing (7/7)

Each row family was broken deliberately and the suite re-run.

| # | Mutation | Rows failing |
|---|---|---|
| M1 | revert redaction, render the raw `toml` error | 4 |
| M2 | remove the permission gate | 1 |
| M3 | make the non-POSIX branch silently pass | 1 |
| M4 | make the gate unconditional (blanket restriction) | 1 |
| M5 | drop the redaction marker | 2 |
| M6 | destroy line/column (`[..0]`) | 5 |
| M7 | pin the line to 1 | 5 |
| C | control, restored | 11 pass |

Two mutations initially **escaped** and forced design changes rather than test edits: M3 (the
non-POSIX row passed because the policy had been inlined in the `cfg` arm, which is what motivated
factoring `secret_file_permission_verdict` out) and M6 (the first attempt was a no-op because the
mutation anchor did not match the real code).

### Guard negative testing (11/11)

`scripts/check-config-secret-hygiene.sh` was inverted from "assert the hazard is present" to "assert
the fixed state", as its own header originally specified, so reverting the fix now fails the guard.

| # | Mutation | Detected |
|---|---|---|
| N1 | `ConfigError::Parse` reverts to a bare `toml::de::Error` | yes |
| N2 | `Display` renders the retained source | yes |
| N3 | redaction destroys line/column | yes |
| N4 | `Error::source` stops retaining | yes |
| N5 | chain-walking printer added to the config path | yes |
| N6 | `& 0o077` weakened to `& 0o007` | yes |
| N7 | non-POSIX silently passes | yes |
| N8 | gate made unconditional | yes |
| N9 | gate call replaced by a bare reference | yes |
| N10 | `I2pControlPassword` `Debug` stops redacting | yes |
| N11 | `Config` gains `Serialize` | yes |
| C | control | pass |

**Three real guard defects and two harness errors were found by this exercise and fixed:**

1. **N5 escaped.** The regex searched for a literal `{:#}` while the code emits `{error:#}`, so the
   assertion matched nothing and would have passed against the hazard it forbids. Now
   `\{[A-Za-z_][A-Za-z0-9_]*:[^}"]*\}` — catches both `#` and `?`; verified against 3 positives and
   2 negatives.
2. **N9 escaped.** The assertion accepted a bare *mention* of `check_secret_file_permissions`. Now
   pins the call: `check_secret_file_permissions\(path\)\?`.
3. **N11 needed targeted mutation.** The derive capture read only `line - 1`, but a doc comment sits
   between the derive and the struct; it now walks **up** to the nearest `#[derive`. Separately, the
   harness replaced the first of **15** identical derives rather than `Config`'s.

Each fix is recorded inline in the guard.

## Findings by severity

| ID | Sev | Finding | Disposition |
|---|---|---|---|
| F1 | **high** | `message()` leaks the secret through `deny_unknown_fields`, so message filtering is insufficient | fixed structurally (D1) |
| F2 | **high** | Whole offending source line printed on any TOML syntax error | fixed (D1) |
| F3 | **high** | Config was the only secret-bearing daemon file with no mode gate | fixed (D2) |
| F4 | medium | Windows silently accepted a password-bearing config | fixed by refusing (D2); behaviour change disclosed |
| F5 | medium | Guard's chain-walking assertion matched nothing (N5) | fixed |
| F6 | medium | Guard accepted a bare mention of the gate (N9) | fixed |
| F7 | low | Plan + guard cited `toml` line 138; verified 140 | corrected |
| F8 | low | One observed flake, unrelated to this plan | recorded below, not fixed |
| F9 | info | `I2pControlPassword` retains `Clone` | named follow-on (D3) |

## Known limitations

1. **AC8 is unproven.** No CI is reachable from this environment. The floor was run locally at
   `2416c30` + working tree and is 46/46, which is strong but is not the acceptance criterion as
   written. **Recorded as unproven, not claimed.**
2. **Windows runtime behaviour is unexercised.** The `#[cfg(not(unix))]` rows exist and compile only
   on non-POSIX. Only the platform-independent policy function is exercised on this host.
3. **`Config` still derives `Debug` and `PartialEq` reaches it via `CommandOutcome`.** Unchanged and
   named above.
4. **`I2pControlPassword` still derives `Clone`.** Named above.
5. **Sealed-at-rest storage is out of scope.** `RouterBoundOutboundSecrets` still has no production
   caller. A stolen config file is still a stolen credential *in the clear*; this plan only stops it
   being world-readable and stops it being printed.
6. **One observed flake, not attributable to this plan.**
   `tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets`
   (`crates/i2pr-daemon/tests/i2pcontrol_tunnels.rs:578`) failed **once** in a full-workspace run,
   asserting all statuses `success` after a bulk `restart`/`stop`. Evidence gathered: pre-change tree
   ×2 `cargo test -p i2pr-daemon --all-targets` clean; this tree ×2 same command clean; the isolated
   row ×3 clean; the whole `i2pcontrol_tunnels` binary ×3 clean (51 tests); full-workspace ×1
   failure. No mechanistic path exists — that test uses `Config::parse`, which is pure text and so
   never reaches D2's gate, and it produces no parse errors, so D1 is never reached either. It is
   timing-sensitive and uses `distinct_port()`, the port-guessing helper behind F6/F7 in Plan 342's
   closure record. **Ten clean runs is not proof of absence.** Stated plainly as a load-sensitive
   flake with one observed failure, not claimed fixed and not claimed unrelated with certainty.

## Why the floor is slow

Recorded because it was investigated and the answer is not "something hung". `cargo test --locked
--workspace --all-targets -- --test-threads=1` serializes every test in the workspace, and the
daemon/runtime suites carry **~11 004 s (183 min) of `tokio::time::sleep`** wall-clock waits in total.
Largest contributors: `crates/i2pr-runtime/tests/ssu2_local.rs` (1300 s),
`service_tunnels_local_roundtrip.rs` (1100 s), `service_tunnel_irc_client_product.rs` (766 s, which
includes a single 35 s stall-wait at `:403` used to observe a per-direction idle timeout),
`service_tunnels_plan212_router_backed_product.rs` (400 s), `floodfill_controlled_lifecycle.rs`
(350 s). These are **socket-backed** product tests, so `#[tokio::test(start_paused = true)]` is not
a drop-in fix: a paused clock cannot be relied on to advance correctly while real loopback socket
I/O is pending. The floor is slow by construction, the run was healthy, and no step had failed at
the point of diagnosis. **No test timing was changed by this plan** — shortening a timeout-wait to
make the suite faster would weaken the very assertions those rows exist to make.

## Roadmap disposition

`plans/subsystems/i2pcontrol-proposal-170-roadmap.md`: Plan 352 moves `ready` → `passed`. It was
the last registered plan in the Proposal 170 line with work that does not require the external
Java/i2pd lane. What remains in that line is blocked on external evidence, not on local work:

- Plan 347 — `stopped` at a classified boundary (Java requires a bandwidth tier ADR 0030 forbids
  inventing; the type-5 consumer half closed via 349 → 351).
- Plan 348 — blocked on 347 alone; its §1 re-freeze is already executed and clean.
- Plans 325/326/327/328 — blocked, remainder external.

**No Encrypted LeaseSet2 interoperability or full-Proposal claim is promoted. Type 5 stays
non-advertised.** This plan hardened the configuration path; it changed no protocol behaviour, no
capability advertisement, and no support surface.