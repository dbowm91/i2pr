# Plan 214 status — M10 HTTP/IRC product-only external requalification and final closure

Status: **`passed-m10-product-only-remote-http-and-irc-application-closure`**.

Plan of record: [`214-m10-http-irc-product-only-external-requalification-evidence-hardening-and-final-closure.md`](214-m10-http-irc-product-only-external-requalification-evidence-hardening-and-final-closure.md).

Active corrective: [`215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md`](../../implementation/service-tunnels/215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md).

## What Plan 214 has already established

Plan 214 remains the final M10 HTTP/IRC application-profile qualification authority after Plan 213's generic router-backed product proof. Its source-side work is retained:

- black-box counted driver through `ServiceProduct` only;
- concurrently pumped external curl/jaraco subprocesses;
- target-derived HTTP facts;
- target-derived IRC registration/PING/PRIVMSG/ACTION/DCC/privacy facts;
- independent production-counter windows;
- exact i2pd/jaraco pin verification;
- product-only remote HTTP and IRC aggregate rows;
- exactly one terminal `P214-*` classification;
- no lower-layer shadow router/Streaming stack;
- transparent i2pd `type = server` tunnel for the IRC transport endpoint;
- local `P214-N-passed` proof on the Plan 214 source tree.

Plan 213 remains passed and is not reopened by the hosted Plan 214 failure.

## Plan 215 hosted double-pass on `1992d67` (initial closure)

Per Plan 215 §15 the hosted `full` gate was satisfied on the
corrective verification SHA `1992d67ffe1d37c1d5c225fff494c5bf02ba00b3`:

```text
hosted_pass_1 = P214-N-passed (run 35309158441 on 1992d67)
hosted_pass_2 = P214-N-passed (run 35309655867 on 1992d67)
```

Each run produced non-empty `plan213-generic-evidence-<run-id>` (with
`P213-N-passed`) and `plan214-applications-evidence-<run-id>` (with
`P214-N-passed`) artifacts.

## Plan 215 hosted double-pass on `a71e0193` (re-closure on dependabot-merged head)

After four safe dependabot PRs (`#18` tokio 1.53.1, `#21` flate2 1.1.10,
`#22` thiserror 2.0.20, `#23` dtolnay/rust-toolchain 1.120.0) landed on
top of `1992d67`, the workspace head advanced to
`a71e0193c420c8d8464fd05ff67d5323515ed4dd`. Plan 215 §15 requires the
two-consecutive-pass gate to be re-proven on the new immutable SHA:

```text
hosted_pass_1_reclosure = P214-N-passed (run 35347780246 on a71e0193)
hosted_pass_2_reclosure = P214-N-passed (run 35349313549 on a71e0193)
```

Both runs again concluded with `success`, with `P213-N-passed` and
`P214-N-passed` recorded in the non-empty
`plan213-generic-evidence-<run-id>` +
`plan214-applications-evidence-<run-id>` artifacts and
`source-head.txt = a71e0193c420c8d8464fd05ff67d5323515ed4dd` in both.
No executable qualification source changed between the two passes.

Plan 214 final-acceptance authority is inherited by the re-closure
SHA; the §18 transition already fired on `1992d67` and the
`plan214 = passed-m10-product-only-remote-http-and-irc-application-closure`
label applies to both SHAs.

## Hosted requalification attempt on `b08d4977dbd43605f47bdea01ef8bd142f8b1c7f`

Two hosted `full` workflow runs were attempted on the same Plan 214 source head:

```text
35245848091 = failure
35245869000 = failure
```

Both runs passed Plan 213 generic Direction A+B and reached the delegated Plan 214 lane. Neither run reached the counted HTTP or IRC application operation.

The runner failed before public destination extraction because its i2pd `tunnels.conf` writer used an unquoted heredoc containing shell-active Markdown backticks in explanatory comments. The shell interpreted that prose as command substitution while constructing the config. i2pd therefore did not generate the intended HTTP/IRC destination `.dat` files, and the lane correctly failed closed at:

```text
P214-C-public-destination-extraction
```

This is a hosted qualification-runner/config-generation defect, not evidence of a product regression. Plan 215 owns the narrow correction and exact-head re-verification.

## Current execution graph

```text
Plan 212 source closure
  -> Plan 213 generic Direction A+B external proof (passed)
  -> Plan 214 source/local application proof (locally passed)
  -> Plan 215 hosted config-generation corrective
  -> local exact-head Plan 214 P214-N re-verification
  -> hosted full pass #1 on one immutable corrective SHA
  -> hosted full pass #2 on the same SHA
  -> Plan 214 / M10 final authority transition
```

## Current authority

```text
plan_211 = retained-source-harness-superseded-for-final-evidence-by-plan214
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = passed-m10-product-only-remote-http-and-irc-application-closure
plan_215 = passed-m10-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = passed-hosted-double-pass-on-1992d67
milestone10_remote_service_interop = passed-via-plans-213-214-215
milestone10_final_acceptance = closed-on-1992d67-pending-plan204-convergence
```

## Plan 215 hosted double-pass ledger

Both Plan 215 hosted `full` runs landed on the identical immutable
SHA `1992d67ffe1d37c1d5c225fff494c5bf02ba00b3`:

```text
hosted_pass_1 = P214-N-passed (run 35309158441 on 1992d67)
hosted_pass_2 = P214-N-passed (run 35309655867 on 1992d67)
```

Each run produced non-empty
`plan213-generic-evidence-<run-id>` (with `P213-N-passed`) and
`plan214-applications-evidence-<run-id>` (with `P214-N-passed`)
artifacts. No executable qualification source changed between the
two passes. Plan 215 §15 is satisfied and the §18 authority
transition fires on this documentation-only commit.

### Plan 215 re-closure double-pass ledger on `a71e0193`

After four safe dependabot PRs landed on top of `1992d67`, the
workspace head advanced to `a71e0193c420c8d8464fd05ff67d5323515ed4dd`.
Plan 215 §15 requires the two-consecutive-pass gate to be re-proven
on the new immutable SHA, which it was:

```text
hosted_pass_1_reclosure = P214-N-passed (run 35347780246 on a71e0193)
hosted_pass_2_reclosure = P214-N-passed (run 35349313549 on a71e0193)
```

Both runs recorded `P213-N-passed` + `P214-N-passed` with
`source-head.txt = a71e0193c420c8d8464fd05ff67d5323515ed4dd` in both.
No executable qualification source changed between the two passes;
only the pre-existing dependabot Cargo.lock + Cargo.toml + workflow
toolchain pin mutations. The re-closure ledger is recorded in
`plans/closure/service-tunnels/215-status.md` under the "Plan 215 re-closure on dependabot-merged head `a71e0193`" section.

## Post-closure scope

Plan 214 final acceptance remains bounded to the i2pd first-family
qualification: the Plan 201 Java-side LeaseSet2 publication gap is
still the active M6-Java blocker and is owned by the Plan 204 docs /
CI normalization pass. The M10 closure transition recorded here does
not imply `milestone6_interoperable = passed`; that claim stays
gated on Plan 204 convergence over the Plan 201 terminal
`P200-{A..H}` classification.

## What remains

Java M6 second-family interoperability remains a separate authority and is not closed by Plan 214 or Plan 215.
