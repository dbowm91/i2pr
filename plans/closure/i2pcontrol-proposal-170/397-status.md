# Plan 397 closure — blocked

Status: **blocked-no-inbound-streaming-arrives-after-owner-fix-plan-398**.

Plan: `plans/implementation/i2pcontrol-proposal-170/397-inbound-type5-service-dispatch-corrective.md`.

## Scope result and verification

Corrected the post-start inbound role owner projection. `TunnelRegistration::tunnel_id`
is the creator tunnel id, while the data plane and service receive-owner map need
the activated `EstablishedTunnel::local_inbound_receive()` endpoint id. Startup
and post-start registration now carry that endpoint id explicitly. Added bounded
cumulative server SYN observed/accepted and successful loopback target-dial
counters, plus transaction-scoped orphan deltas in the live driver.

- Format and managed-app sibling build — pass.
- `encrypted_type5_publication_requires_its_blinded_storage_key` — pass.
- Evidence checker mutation suite — pass.
- Runner self-test — pass.
- ELS2 live guard and encrypted-consumer caller guard — pass.

## Exact-pinned NONE result

Run: `target/interop/els2-evidence-plan397-none-owner-id-fix-20261009`.
Stock i2pd 2.61.0 at exact SHA
`635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.

Reference ELS2 and ordinary authority controls passed. The reverse service
committed, type-5 publication reached local DatabaseStore admission, and the
stock client connected to the advertised encrypted address. Its payload did not
return. The before/after inbound orphan delta was zero, server SYN observed and
accepted totals were zero, successful loopback target dials were zero, and
`remote_inbound_dispatched` did not advance. Provisioning still reported two
registered inbound roles, two usable leases, two bridge projections, and no
owner/activation failures. The receive-owner mismatch is fixed; no reverse
request reached the service Streaming accept path in this run.

Hashes: `evidence.json`
`a22bb364513f052b21231f283e447e5c7ce8c5d60c6e25971f69173e42d82bc8`,
`results.tsv`
`0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv`
`e216b0f02a49573d9fdee93d802b287b1d035b31d825787bd33699eb02444070`.

No wire, transcript, credentials, dependency, support, capability, or
advertisement changes.

## Unblock audit

Plan 398 owns the missing boundary between stock SAM Streaming connect and
service-side inbound TunnelData/SYN, with evidence that distinguishes no
reference send, tunnel ingress without completed Garlic, and server accept.
Plans 374/375 and downstream 377/378 remain blocked on independent pinned
evidence; Plan 376 passed. No future plan is unblocked.
