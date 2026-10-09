# Plan 393 closure — blocked

Status: **blocked-publication-begin-rejection-cause-plan-394**.

Plan: `plans/implementation/i2pcontrol-proposal-170/393-publication-coordinator-retry-rollback-corrective.md`.

## Scope result and verification

Added post-begin cleanup in `publish_service_ls2_for_service`: coordinator
intent is cancelled when tunnel composition returns no dispatch, outbound
request construction fails, or any transport delivery is rejected. Successfully
admitted messages retain their request for response/ACK correlation. Added a
max+1/cancel/replacement regression for the coordinator's eight-entry bound.

- `rtk cargo fmt --all --check` — pass.
- Managed-app sibling build — pass.
- `failed_publication_cancellation_releases_bounded_capacity` — pass.
- Coordinator service-role capacity regression — pass.
- Runner and checker self-tests — pass.
- Evidence checker mutation table — 22 detected, 0 missed, 2 controls.
- Live evidence checker and encrypted-consumer caller guard — pass.

## Exact-pinned NONE result

Run: `target/interop/els2-evidence-plan393-none-publication-cancel-20261009`.
Stock i2pd 2.61.0 at exact SHA
`635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.

The authority payload passed. The reverse service had 2 inbound and 2 outbound
registrations, 2 usable inbound leases, 2 successfully installed bridge
inbound projections, and a signed LS2. Nevertheless, local publication failed
36 times with last stage `PublicationCoordination`, 0 accepted and 1 pending;
reverse DatabaseStore delivery and payload were not reached. This is the
`begin_ls2_publication` stage, so post-begin cancellation does not address the
current rejection. The remaining cause may be the bounded publication-cap
condition or a non-cap begin validation result. Plan 394 adds bounded typed
attribution before changing publication policy.

Hashes: `evidence.json`
`ecbe1b580d75a9a2fa39ac999b5bf37155cac98f1242bc868e20aaf8b8d6b959`,
`results.tsv`
`0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv`
`4254d53144ac2bbd28ddc3744e4100c74f0577af4ac98f94f8f80c71d17477a5`.

NONE did not return the reverse payload; PSK/DH and full floor remain gated.
No wire, dependency, credential, support, capability, or advertisement change.

## Unblock audit

Plan 394 owns identifying the exact `begin_ls2_publication` rejection without
exposing errors or identifiers, then correcting only that cause. Plan 374
remains blocked on full i2pd rows, Plan 375 on Java source-lock/live evidence,
Plan 376 remains passed, and Plans 377/378 remain blocked. No future plan is
unblocked.
