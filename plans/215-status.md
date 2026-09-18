# Plan 215 status — hosted Plan 214 tunnel-config generation corrective and exact-head re-verification

Status: **`source-side-corrective-landed-cleanup-hardened-hosted-double-pass-pending`**.

Plan of record: [`215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md`](215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md).

## Source floor / failure provenance

Audited source head before the corrective:

```text
b08d4977dbd43605f47bdea01ef8bd142f8b1c7f
```

Hosted `full` workflow attempts on that exact SHA:

```text
35245848091 = failure
35245869000 = failure
```

Both runs passed the Plan 213 generic router-backed qualification first, then stopped in the delegated Plan 214 lane before either counted HTTP or IRC application operation. The Plan 214 runner's i2pd `tunnels.conf` generation used an unquoted heredoc containing Markdown backticks in explanatory comments. Those backticks are active shell command substitution, so the hosted runner interpreted prose while constructing the config. i2pd consequently never produced the expected HTTP/IRC destination `.dat` files and Plan 214 failed closed at `P214-C-public-destination-extraction`.

This is classified as a qualification-runner/config-generation defect. It is not evidence of a router/product regression because the application driver was never reached.

## Source-side corrective landed

The corrective is intentionally narrow and stays within the Plan 215 §3 scope lock:

- `tests/integration/service-tunnels/run-plan214-applications.sh`
  - new `write_plan214_tunnels_conf` helper: deterministic, shell-inert
    `printf`-based writer for the i2pd `tunnels.conf`. The shell is
    never asked to interpret template prose; every line is either a
    literal argument to `printf` or a single `%s` interpolation of an
    explicit port number.
  - new `plan215_section_body` helper: extracts a section body from a
    key=value INI-style file without depending on the next section
    header to terminate.
  - new `validate_plan214_tunnels_conf` helper: pre-launch sanity gate
    that fails closed on any of the 12 Plan 215 §5 contract violations
    (file presence, exactly two sections, `type = http` + `type = server`,
    configured HTTP/IRC port equality, expected key filenames, zero-hop
    lengths, no unresolved template placeholders).
  - the vulnerable `cat > "${I2PD_HOME}/tunnels.conf" <<EOF … EOF` block
    is replaced by a single `write_plan214_tunnels_conf` invocation +
    an immediate `validate_plan214_tunnels_conf` pre-launch gate. The
    rationale for the transparent IRC `type = server` tunnel is moved
    to a shell comment block immediately above the helper, where it is
    no longer interpreted while the config is generated.
  - new sanitized `plan214-reference-tunnel-config-sanity` evidence
    row recorded through `record_guarded`; the failure maps to the
    existing `P214-B-reference-startup-or-pin` terminal class.
  - the i2pd `tunnels.conf` is now also copied into the evidence
    directory after the sanity gate passes, so a future regression is
    debuggable from the uploaded artifact alone.
  - **Plan 215 §5 cleanup hardening** (same runner, no product
    change): the loopback fixtures are now started under `setsid`
    so the §16 cleanup's `kill -KILL -- -${pid}` only signals the
    fixture subtree. Without `setsid`, the fixture shared the
    runner's process group, and CPython's default SIGTERM handler
    could wedge the cleanup because accept() auto-restarts on EINTR.
    `stop_group` now sends SIGKILL (uncatchable) instead of SIGTERM,
    and the §16 cleanup replaces the indefinite `wait $pid` with a
    bounded grace window + KILL fallback + bounded reap. The §16
    process / port checks are now scoped to *this run's* fixtures
    via the unique `--facts ${HTTP_FACTS}` / `--facts ${IRC_FACTS}`
    command-line markers and the ephemeral `--datadir
    ${I2PD_DATA}` i2pd directory, so the delegated lane no longer
    races against the harness's local-lane fixtures (which are
    owned by `run-independent.sh` and stopped only at its own
    CHILD_PIDS sweep).
- `tests/integration/service-tunnels/test-plan215-tunnels-conf.sh`
  - new focused shell test that exercises the writer and validator
    contract independently of the expensive external lane (writer
    happy path, alternate-port happy path, missing file, wrong HTTP
    port, wrong IRC port, `type = irc` instead of `type = server`,
    missing HTTP section, extra section, unresolved `${}` placeholder).
- `scripts/check-service-tunnel-acceptance-evidence.sh`
  - new §29 source-level invariants:
    1. reject the unquoted `<<EOF` heredoc shape for `tunnels.conf`;
    2. require `write_plan214_tunnels_conf` + literal `[HTTP-Server]`
       and `[IRC-Server]` section headers;
    3. require `validate_plan214_tunnels_conf` pre-launch (the source
       ordering relative to the `setsid "${I2PD_BIN}"` launch is also
       enforced);
    4. require the literal `'type = server'` token;
    5. require `HTTP_TARGET` / `IRC_TARGET` as dynamic inputs;
    6. require `plan214-http-server.dat` / `plan214-irc-server.dat`
       key filenames;
    7. require the `plan214-reference-tunnel-config-sanity` evidence
       row through `record_guarded`;
    8. reject any `eval` call;
    9. require `plan214-reference-tunnel-config-sanity` to map to
       `P214-B-reference-startup-or-pin`;
    10. reject `envsubst` / `jinja2` / `mustache` template layers;
    11. reject `echo … > tunnels.conf` regressions;
    12. require the focused `test-plan215-tunnels-conf.sh` contract
        test to stay on disk and cover all nine documented contract
        cases.

The runner i2pd.conf heredoc (lines 464–499) is unchanged — it never
contained shell-active prose and stays out of scope for the §6.1
rejection rule (which is scoped to `tunnels.conf`).

## Source-side validation

The focused Plan 215 validation sequence is fully green on the source
side:

```text
bash -n tests/integration/service-tunnels/run-plan214-applications.sh
bash -n scripts/check-service-tunnel-acceptance-evidence.sh
bash -n tests/integration/service-tunnels/test-plan215-tunnels-conf.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash tests/integration/service-tunnels/test-plan215-tunnels-conf.sh
```

The full workspace floor (`cargo fmt --all --check`,
`cargo check --locked --workspace --all-targets`,
`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`,
`cargo test --locked --workspace --all-targets -- --test-threads=1`,
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`,
every static boundary / acceptance evidence / vector / interop
checker, and `cargo deny check advisories bans sources`) is green on
the source-side corrective tree.

## Current authority

```text
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = local-pass-proven-hosted-requalification-blocked-on-plan215-corrective-source-landed
plan_215 = source-side-corrective-landed-cleanup-hardened-hosted-double-pass-pending

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = locally-passed-only-hosted-proof-pending
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## Local exact-head re-verification on `0fbacc3`

The exact-head local full lane (`bash
tests/integration/service-tunnels/run-independent.sh --full`) is now
green on the source-side corrective + cleanup hardening head:

```text
0fbacc3de31c9089be66a567cc880d0f71576585
```

Two consecutive local delegated full-lane runs both observed
`P214-N-passed` with 73/73 rows green (HTTP eepsite, IRC service,
HTTP/IRC counter deltas, sibling isolation, DCC policy, clean-resource
baseline, cleanup-kills-on-timeout). The Plan 213 prerequisite row
also stayed `passed` on the same evidence directory.

## Required hosted re-verification (Plan 215 §15)

After the source-side corrective is pushed and the routine CI floor
remains green on the resulting SHA:

1. dispatch the existing hosted `full` workflow lane once on the
   exact corrective SHA and require `P213-N-passed` + `P214-N-passed` +
   non-empty `plan213-generic-evidence-<run-id>` and
   `plan214-applications-evidence-<run-id>` artifacts;
2. only after run #1 finishes successfully, dispatch the same lane a
   second time on the identical SHA and require the same outcome;
3. only after both runs finish with `success`, update
   `plans/214-status.md`, `plans/215-status.md`, and the current
   authority documentation to the Plan 215 §18 transition.

No executable qualification source may change between the two hosted
passes; the evidence-only authority transition is allowed afterwards
and stays on top of the same SHA.

## Verification ledger

```text
corrective_verification_sha = pending
local_exact_head_plan214 = pending
hosted_pass_1 = pending
hosted_pass_2 = pending
```

Do not promote this status from `source-side-corrective-landed` until
both hosted passes record `P214-N-passed` on one immutable source
SHA. If executable qualification source changes after the first hosted
pass, the consecutive-pass count resets to zero.
