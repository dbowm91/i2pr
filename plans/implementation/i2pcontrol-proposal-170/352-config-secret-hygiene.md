# Plan 352 — daemon config secret hygiene: parse-error echo and file-mode check

Status: **registered-preexisting-config-secret-leak-paths; independent-of-351**

Classification: invariant (a static-guard-backed security property that must always hold) +
security corrective. Origin: found while scoping Plan 351's Gate 3; **pre-existing** and
not caused by any ELS2 work.

This is a corrective pass. It has no relationship to Plan 349's capability work and must not
wait for it — the leak it closes is live today for `I2pControlPassword`.

## Why this is a separate plan

Plan 351 found that a secret-bearing config field leaks to stderr on any TOML error, and
worked around it by keeping its secret out of the config entirely. That is the right
response for a *new* field, but it leaves a live defect in an existing field. Filing this
separately keeps Plan 351's scope honest (it explicitly does not own these defects) and lets
this small, self-contained fix land without waiting on a capability plan.

Subsystem: `i2pcontrol-proposal-170` (it is the same config surface and the same guard
family).

## Hard dependencies

None. The current tree reproduces both defects. No interface dependency on Plan 351 or 351's
outcome.

## The defects, verified against vendored crate sources

### D1 — a TOML parse error prints the entire offending source line

`toml-1.1.6+spec-1.1.0/src/de/error.rs:138`:

```rust
// 1 | 00:32:00.a999999
write!(f, "{line_num} | ")?;
writeln!(f, "{content}")?;
```

`content` is the whole source line, verbatim. Independently, a type mismatch prints the
value: `serde-1.0.228/src/core/de/mod.rs:410`:

```rust
Str(s) => write!(formatter, "string {:?}", s),
```

Both reach the terminal. The chain is:

```text
toml::de::Error
  -> ConfigError::Parse(#[source] toml::de::Error)
       #[error("configuration parse failed: {0}")]        config.rs:2853-2854
  -> DaemonError::Config(#[from] ConfigError)
       #[error(transparent)]                                error.rs:70-71
  -> eprintln!("error: {error}")                            main.rs:51
```

So **any TOML syntax error on the line holding a secret, or any type mismatch on that
field, prints the secret to stderr.** The operator's most likely reaction to a typo in a
password is to paste the whole terminal output into a bug report.

`I2pControlPassword` is exposed today: `RawI2pControlConfig.password`
(`config.rs:343`), normalised into `I2pControlConfig.password` (`:1154`). `main.rs:45` and
`main.rs:51` both print the error.

`ConfigError::Semantic { field: &'static str, reason: &'static str }` (`config.rs:2860-2865`)
is already safe and is the shape any new rejection must use: two `&'static str` fields mean
the variant **physically cannot** carry a value.

### D2 — the config file is the only unhardened secret-bearing file in the daemon

`Config::load` (`config.rs:1485-1491`) is a bare `fs::read_to_string`. There is no
`Permissions`, no `0o600`, and no `metadata().permissions().mode()` anywhere in `config.rs`;
the single `fs::metadata` at `:2312` only checks `is_dir()` for the data directory.

The repo already enforces this elsewhere and should not be inconsistent with itself:

- `crates/i2pr-storage/src/lib.rs:1021` `options.mode(0o600)`, `:1054` `builder.mode(0o700)`,
  `:1083` `if metadata.permissions().mode() & 0o077 != 0` (reject group/world readable);
- `crates/i2pr-daemon/src/i2pcontrol_tunnels.rs:1022-1023` — the same `& 0o077` rejection
  for a secret file;
- `crates/i2pr-daemon/src/addressbook.rs:808,879` `options.mode(0o600)`.

A config file holding a plaintext secret is therefore the one secret file with no mode gate.

### D3 — `Config` is `Clone + Debug` and is embedded in a `Debug + PartialEq` public type

`config.rs:1399`: `#[derive(Clone, Debug, Eq, PartialEq)] pub struct Config`.
`lib.rs:76-84`: `#[derive(Debug, Eq, PartialEq)] pub enum CommandOutcome`, whose `Validated`
variant embeds the whole `Config`.

No production site formats either today — `main.rs:14-20` prints a fixed string and
`initialize_logging(&config.logging)` is the only use. But `CommandOutcome` derives
`PartialEq`, so any `assert_eq!` on it panics through `{:?}` and dumps the entire config.
Current tests use `matches!` (`lib.rs:1649`), so nothing prints **yet**. This is a latent
leak one assert away, and it is the reason a future inline secret field would be dangerous
even with D1 and D2 fixed.

`Config` is not `Serialize` and is never written back — confirmed: zero `Serialize` derives in
`config.rs` and no `toml::to_string` / `serde_json::to_string` on a config anywhere. That one
is already correct and must stay correct.

## Objective

No secret byte reaches a terminal, a log, or an error string through the config path, and
the config file carrying a secret is refused when it is group- or world-readable. Prove it
with a static guard that fails if any of it regresses.

## In scope

1. **A redacting wrapper for TOML parse errors.** Route `ConfigError::Parse` through a
   redacting representation before it can reach `main.rs`, so the rendered message carries
   the line and column but never the source content. Preserve the diagnostic value: an
   operator still learns *which* line failed.
   - Prefer keeping the underlying `toml::de::Error` as `#[source]` for programmatic use and
     controlling only the **rendered** message, so no information is lost to callers that
     legitimately want the source.
   - This is a change to how a `thiserror` `Display` is produced, so it likely needs a
     hand-written `Display`/`Debug` for `ConfigError` (or a newtype around the parse error
     with a redacting `Display`). Choose whichever keeps `error.rs:70-71`'s transparent
     `DaemonError::Config` chain working.
2. **A mode check in `Config::load`**, using the existing `i2pr-storage` idiom
   (`& 0o077 != 0`), gated so it is **fail-closed when a secret is present** and does not
   break a config that holds no secret. Decide and document the Windows behaviour, where
   `PermissionsExt::mode` is not meaningful — the repo must not silently pass there.
3. **A negative test proving a secret cannot reach stderr.** Assert that a config whose
   password line has a type error produces a rendered error that does **not** contain the
   secret bytes. This is the row that makes D1 non-hypothetical.
4. **`scripts/check-config-secret-hygiene.sh`** — the static guard, negative-tested.

## Out of scope

- **Removing `Debug` from `Config` or `CommandOutcome`.** Both are public API and the derive
  removal is a wide, breaking change well beyond this plan. Instead, the guard asserts the
  *known* leak paths stay closed and that no new secret field is added to a `Debug` struct
  (see below). Record the residual `Debug` risk as a limitation with a named follow-on.
- **Any new secret-bearing config field.** That is Plan 351's decision, and Plan 351's Gate 3
  keeps its secret out of config. This plan only closes the paths that already exist.
- **Sealed-at-rest secret storage.** `outbound_secret.rs` already has the mechanism
  (HKDF from the router signing seed, `$i2pr1o$` framing, and the property *"A stolen
  configuration file is not a stolen credential"* at `:22-24`), but
  `RouterBoundOutboundSecrets` has **no production caller**, its label is purpose-bound to
  the outproxy (`:53-55`), and sealing needs the signing seed, which is unavailable at
  config-parse time. Sealing `I2pControlPassword` is a separate plan with its own migration
  and compatibility questions.
- **Plan 351's capability work.** No dependency in either direction.

## Invariants

- No secret byte in any `Debug`, `Display`, `Serialize`, `log`, or `eprintln!` output. This
  is the repo rule from `docs/security-model.md:80-81`: *"Secret-bearing types must avoid
  accidental `Debug`, `Display`, unrestricted serialization, and unnecessary cloning."*
- A config file with a secret is refused when group- or world-readable, and the refusal
  names the path and the required mode without echoing content.
- The redaction must not destroy diagnostics: line and column survive; content does not.
- No production behaviour changes for a config that contains no secret.
- `Config` stays non-`Serialize` and is never written back.
- No new dependency: the fix uses `std::os::unix::fs::PermissionsExt` behind a cfg and
  `thiserror`, both already in the tree.

## Required evidence

- A secret-bearing config line with a **syntax** error renders an error that does not
  contain the secret bytes, and does contain the line number.
- A secret-bearing config line with a **type** error renders an error that does not contain
  the secret bytes.
- The same for a `deny_unknown_fields` rejection on a secret's line.
- A `0644` config holding a password is refused with a message naming the path and the
  required mode, and the message contains no password bytes.
- A `0600` config holding a password is accepted.
- A config with **no** secret is accepted at `0644`, so the check is not a blanket
  regression.
- A no-secret config error still reports the correct line and column.
- `check-config-secret-hygiene.sh` is **negative-tested**: each assertion is shown to fail
  when its condition is deliberately broken.

## Production changes

`crates/i2pr-daemon/src/config.rs` (redacting parse-error representation, mode check in
`Config::load`), `crates/i2pr-daemon/src/error.rs` only if the transparent
`DaemonError::Config` chain requires it, and a new
`scripts/check-config-secret-hygiene.sh` added to the `AGENTS.md` routine floor. No new
dependency, no wire change, no advertisement change.

## Guard design

`scripts/check-config-secret-hygiene.sh`, following the skeleton in
`scripts/check-els2-type11-transcript-boundary.sh` (`set -euo pipefail` at `:19`, `fail()` at
`:28-31`, `rg -q '…' file || fail "…"` assertions, the `awk '/^impl<.a> Name<.a> \{/{flag=1}…/'`
impl-block extraction idiom at `:96-100`, terminal success line at `:130`).

Assertions, each name-anchored so a rename **fails closed** via
`rg -q 'pub struct <Name>' || fail "<Name> is missing"`:

1. No secret-bearing config type derives `Debug` or `Clone`; each must have a hand-written
   redacting `impl Debug` containing a `write_str(…redacted…)` call. Currently the only
   named type is `I2pControlPassword`; it derives `Clone` today, so either the derive is
   removed (preferred, and consistent with `OutboundSecretKey` at
   `outbound_secret.rs:68-72`) or the plan records why not.
2. `rg -n 'Serialize' "$config"` produces nothing.
3. No `println!` / `eprintln!` / `tracing::…!` / `log::…!` / `dbg!` in
   `crates/i2pr-daemon/src/` binds `config`, `CommandOutcome`, or a secret field name.
4. No `{:?}` / `{:#?}` of `Config` or `CommandOutcome` in a non-test path; the one existing
   redaction assertion is the only permitted hit.
5. The redaction exists: assert `ConfigError`'s rendered form does not embed the raw
   `toml::de::Error` `Display`, and assert a redaction marker is present.
6. The mode check exists and is a real gate: assert `Config::load` contains the
   `& 0o077` idiom, not merely a comment.
7. No `fs::read_to_string` on the config path is left unguarded by that check.

## Verification

The full `AGENTS.md` routine floor, plus:

```text
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --bin i2pr -- --test-threads=1
bash scripts/check-config-secret-hygiene.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Add `bash scripts/check-config-secret-hygiene.sh` to the `AGENTS.md` routine floor, and note
that Plan 351's own floor already lists it — the two plans converge on one script, so
whichever lands second must confirm the entry rather than duplicate it.

## Documentation updates

`docs/security-model.md` — state the config-path rule explicitly, next to the existing
`:80-81` line. `docs/architecture/i2pr-daemon.md` — the config load and redaction
behaviour. There is **no** `docs/operations.md` in this tree (verified at Plan 352
registration); record that as a documentation gap for config file permissions rather than
creating a new top-level doc in this plan.

## Acceptance criteria

Plan 352 passes only when:

1. a TOML syntax error, a type error, and a `deny_unknown_fields` rejection on a
   secret-bearing line all render an error containing **no** secret byte, proven by rows;
2. those errors still report the correct line and column, proven by rows;
3. a group- or world-readable config holding a secret is refused, and a `0600` one is
   accepted, and a no-secret config at `0644` is still accepted;
4. `Config` and `CommandOutcome` are never `Serialize`d and are not written back;
5. `scripts/check-config-secret-hygiene.sh` exists, is in the `AGENTS.md` routine floor, and
   is negative-tested with recorded transcripts;
6. the residual `Debug`-derive risk is recorded as a limitation with a named follow-on
   rather than being papered over;
7. no config with no secret changes behaviour, proven by the existing config suite green;
8. exact-head routine CI is green.

## Stop conditions

Stop and record a classified boundary if redaction cannot be achieved without losing the
line/column diagnostic, or if the mode check cannot be made to fail closed on a
platform-gated basis without silently passing on Windows.

## Closure evidence required

Commits; a requirement-to-evidence matrix; exact commands with local/CI outcomes labelled
truthfully; the security review; the negative-test transcripts for the guard; the
platform-behaviour decision for the mode check stated explicitly; known limitations,
including the residual `Debug`-derive exposure; findings by severity; and the roadmap
disposition.
