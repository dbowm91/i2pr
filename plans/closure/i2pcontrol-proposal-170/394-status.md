# Plan 394 closure — blocked

Status: **blocked-encrypted-publication-record-rejection-plan-395**.

Plan: `plans/implementation/i2pcontrol-proposal-170/394-publication-begin-rejection-attribution-corrective.md`.

## Scope result and verification

Added bounded begin rejection counters without exposing identifiers or raw errors. The exact-pinned reverse run reached publication coordination and attributed all 35 observed begin rejections to `InvalidRecord`; capacity, no-floodfill, and other counters remained zero, with no pending coordinator publications. Source inspection identified the record mismatch: encrypted services construct a DatabaseStore type-5 `EncryptedLeaseSet`, while `begin_ls2_publication` accepts only `DatabaseStoreData::LeaseSet2`. This is a contract mismatch, not a capacity leak. Plan 395 owns a bounded encrypted-record publication contract consistent with Plan 346 and the existing type-5 NetDB path.

- `rtk cargo fmt --all --check` — pass.
- Managed-app sibling build — pass.
- `failed_publication_cancellation_releases_bounded_capacity` — pass.
- Runner/checker self-tests and mutation table — pass (23 mutations detected, 0 missed, 2 controls).
- ELS2 evidence checker and encrypted-consumer caller guard — pass.

## Exact-pinned NONE result

Run: `target/interop/els2-evidence-plan394-none-publication-begin-third-20261009`.
Stock i2pd 2.61.0 at exact SHA
`635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.

The ordinary authority payload passed. Reverse registration, usable inbound leases,
bridge projection, and signed LS2 were present. Publication recorded 36 attempts,
0 accepted, 36 failed, and 1 pending snapshot; the bounded begin counters were
capacity 0, no floodfill 0, invalid record 35, other 0, coordinator pending 0.
The implementation builds an encrypted type-5 record when ELS2 material exists,
but the coordinator validator admits only Standard LeaseSet2. No reverse payload
was returned; PSK/DH and the full floor remain gated.

Hashes: `evidence.json`
`fd52560c7d12871f8c8148f16e965fab158bcf7fdfc442acf5aee48cc6d13707`,
`results.tsv`
`0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv`
`4d691b2811a7f6b85c80a1c6109b48b9c3655b1046e29be031f613c610e1c7ae`.

No wire, credential, dependency, support, capability, or advertisement change.

## Unblock audit

Plan 395 is registered to allow the coordinator to retain and publish the existing
encrypted type-5 payload while preserving destination-derived blinded storage keys,
floodfill routing, ACK correlation, and bounded cancellation. Plan 374 remains
blocked on full i2pd rows, Plan 375 on Java source-lock/live evidence, Plan 376
passed, and Plans 377/378 remain blocked. No unrelated future plan is unblocked.
