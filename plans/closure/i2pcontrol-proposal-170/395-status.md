# Plan 395 closure — blocked

Status: **blocked-post-admission-reverse-els2-delivery-plan-396**.

Plan: `plans/implementation/i2pcontrol-proposal-170/395-encrypted-type5-publication-coordinator.md`.

## Scope result and verification

The destination publication coordinator now accepts encrypted type-5 records
only when the DatabaseStore key equals the key derived from the record's blinded
public key. Standard LeaseSet2 remains admitted, unrelated DatabaseStore types
remain rejected, and the bounded coordinator lifecycle is unchanged. The new
deterministic type-5 valid-key/wrong-key regression passes alongside the existing
publication tests.

- `rtk cargo fmt --all` — pass.
- Managed-app sibling build — pass.
- `encrypted_type5_publication_requires_its_blinded_storage_key` — pass.
- `destination_tunnel_unit ls2_publication` — 3 passed.

## Exact-pinned NONE result

Run: `target/interop/els2-evidence-plan395-none-type5-20261009`.
Stock i2pd 2.61.0 at exact SHA
`635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.

Reference ELS2 and standard authority mesh controls passed. The authority
payload was returned. The reverse service committed and reached the local
DatabaseStore delivery boundary: publication counters were 2 attempts, 1
accepted, 1 failed, 0 pending, with all begin-rejection counters zero. The stock
SAM client connected to the reverse address but did not receive the fixture
banner within its bounded read window. The coordinator admission mismatch is
fixed; remote visibility, server delivery, or reply dispatch remains unresolved.
No reverse payload was returned, so PSK/DH and the full floor remain gated.

The runner's evidence metadata still labels this run Plan 394 because the
metadata handoff was not updated before execution. The immutable run is retained
as diagnostic evidence, not a passing Plan-395 artifact. Hashes:
`evidence.json` `40846a4266b7aee98befd3e0a2d3dde198f0d4ae9228db8b0f462b9fab986279`,
`results.tsv` `0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv` `608e5f1da807f713f4f8a378fbbbd37750b56b8b8b3fe3885afda6dd108729aa`.

No transcript, wire format, credential, dependency, support, capability, or
advertisement change.

## Unblock audit

Plan 396 is registered to localize the post-admission failure using bounded
publication/connection counters and then correct only the identified owner.
Plan 374 remains blocked on the complete i2pd matrix; Plan 375 independently
remains blocked on Java source-lock/live evidence; Plan 376 passed; Plans 377/378
remain blocked. No future plan is unblocked.
