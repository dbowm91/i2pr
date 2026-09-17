# Plan 214 status — M10 HTTP/IRC product-only external requalification and final closure

Status: **`local-pass-proven-hosted-requalification-blocked-by-plan215`**.

Plan of record: [`214-m10-http-irc-product-only-external-requalification-evidence-hardening-and-final-closure.md`](214-m10-http-irc-product-only-external-requalification-evidence-hardening-and-final-closure.md).

Active corrective: [`215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md`](215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md).

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
plan_214 = local-pass-proven-hosted-requalification-blocked-by-plan215
plan_215 = registered-executable-hosted-plan214-config-generation-corrective

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = locally-passed-only-hosted-proof-pending
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## What remains

Plan 214 does not need a new product implementation pass at this point. The required sequence is Plan 215:

1. make the `tunnels.conf` generation shell-inert and deterministic;
2. add pre-launch config validation and a static regression guard;
3. preserve all existing Plan 214 application/evidence assertions;
4. produce one local exact-head `P214-N-passed` after the patch;
5. run the hosted `full` lane twice sequentially on the identical corrective SHA;
6. require both runs to finish with Plan 213 `P213-N-passed`, Plan 214 `P214-N-passed`, both remote application aggregates passed, final checker green, and all expected artifacts present.

Only after those two hosted passes may Plan 214 become `passed-m10-product-only-remote-http-and-irc-application-closure` and Milestone 10 final acceptance close.

Java M6 second-family interoperability remains a separate authority and is not closed by Plan 214 or Plan 215.
