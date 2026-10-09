# Plan 396 closure — blocked

Status: **blocked-inbound-receive-owner-transition-plan-397**.

Plan: `plans/implementation/i2pcontrol-proposal-170/396-reverse-type5-post-admission-delivery-corrective.md`.

## Scope result and verification

Added bounded snapshots after reverse payload timeout for publication state,
service connection counts, routing counters, provisioning state, and inbound
orphan receive totals. Plan 395's local DatabaseStore admission remains intact.

- `rtk cargo fmt --all --check` — pass.
- Managed-app sibling build — pass.
- Type-5 blinded-key regression — pass.
- ELS2 checker (including mutation table) — pass.
- Runner self-test — pass.
- ELS2 live guard and encrypted-consumer caller guard — pass.

## Exact-pinned NONE evidence

Healthy-mesh run:
`target/interop/els2-evidence-plan396-none-postadmission-retry-20261009`.
Stock i2pd 2.61.0 at exact SHA
`635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.

The stock ELS2 mesh and standard authority payload passed. Reverse publication
reached local admission (2 attempts, 1 accepted, 1 failed, 0 pending), all begin
rejection counters were zero, and the stock client connected to the reverse
address. The fixture banner did not return. Post-read diagnostics showed zero
active server connections, zero accepted inbound Streaming payloads, and seven
cumulative orphan receive observations. Service provisioning still reported two
registered inbound roles, two usable leases, two installed bridge projections,
and no activation or owner failures. This locates the missing transition after
address resolution and before an accepted service payload, but the cumulative
orphan count was not isolated to the reverse transaction.

Hashes: `evidence.json`
`154de0db586729f9ccdeee7d06f5fd0a633ed448d26b745e9e38ebf4d0e0dc87`,
`results.tsv`
`0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv`
`2240bb3e5cb5c7b1bd476606010a53251e58599f3a8b0224e8d4a8ec492212f`.

A follow-up run with the before/after orphan delta failed earlier at ordinary
authority lookup (`activation_pending=true`, three attempts exhausted) and did
not reach the reverse row; it is retained as an environmental/lookup failure,
not reverse evidence. No wire, transcript, credential, support, capability, or
advertisement change.

## Unblock audit

Plan 397 owns isolating the inbound receive-owner/dispatch boundary with a
transaction-scoped delta and correcting the evidenced transition. Plans 374,
375, 377, and 378 remain blocked on their independent live and source evidence;
Plan 376 passed. No future plan is unblocked.
