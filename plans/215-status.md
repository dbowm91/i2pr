# Plan 215 status — hosted Plan 214 tunnel-config generation corrective and exact-head re-verification

Status: **`registered-executable-hosted-plan214-config-generation-corrective`**.

Plan of record: [`215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md`](215-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification.md).

## Source floor / failure provenance

Audited source head:

```text
b08d4977dbd43605f47bdea01ef8bd142f8b1c7f
```

Hosted `full` workflow attempts on that exact SHA:

```text
35245848091 = failure
35245869000 = failure
```

Both runs passed the Plan 213 generic router-backed qualification first, then stopped in the delegated Plan 214 lane before either counted HTTP or IRC application operation. The Plan 214 runner's i2pd `tunnels.conf` generation uses an unquoted heredoc containing Markdown backticks in explanatory comments. Those backticks are active shell command substitution, so the hosted runner interprets prose while constructing the config. i2pd consequently never produces the expected HTTP/IRC destination `.dat` files and Plan 214 fails closed at `P214-C-public-destination-extraction`.

This is classified as a qualification-runner/config-generation defect. It is not evidence of a router/product regression because the application driver was never reached.

## Current authority

```text
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

## Required execution

1. Replace the vulnerable Plan 214 unquoted `tunnels.conf` heredoc with deterministic shell-inert generation, preferably a small `printf`-based writer.
2. Move the transparent-IRC rationale out of generated config text while retaining `type = server`.
3. Add a pre-i2pd generated-config sanity gate and bounded evidence row.
4. Extend `check-service-tunnel-acceptance-evidence.sh` to reject reintroduction of the vulnerable heredoc shape and require the sanity gate.
5. Run focused shell/checker/unit validation and the normal source floor.
6. Re-run the local exact-head full lane and require `P214-N-passed`.
7. Designate the resulting executable SHA as `corrective_verification_sha`.
8. Run hosted `full` sequentially twice on that identical SHA. Each run must finish successfully with Plan 213 `P213-N-passed`, Plan 214 `P214-N-passed`, both remote application aggregates passed, and non-empty Plan 213/Plan 214/service-tunnel evidence artifacts.
9. Only after both hosted passes, land an evidence-only authority update closing Plan 215 and Plan 214/M10.

## Verification ledger

```text
corrective_verification_sha = pending
local_exact_head_plan214 = pending
hosted_pass_1 = pending
hosted_pass_2 = pending
```

Do not promote this status from registered/pending until the exact evidence above exists. If executable qualification source changes after the first hosted pass, the consecutive-pass count resets to zero.
