# Plan 398 closure — blocked

Status: **blocked-reverse-garlic-binding-rejection-plan-399**.

Plan: `plans/implementation/i2pcontrol-proposal-170/398-reverse-i2pd-tunnel-ingress-corrective.md`.

## Scope result and verification

Added transaction-scoped, saturating counters from SSU2 TunnelData through completed
Garlic envelopes, receive-owner lookup, typed dispatch, queued payloads, and server SYN
acceptance. The bridge now preserves a privacy-safe rejection class instead of collapsing
every dispatcher rejection to a boolean. No raw error strings, IDs, peer addresses,
ciphertext, or keys enter evidence.

- `cargo fmt --all` — pass.
- Managed-app sibling build — pass.
- `inbound_traffic_delta_is_saturating_and_fieldwise` — pass.
- `check-els2-live-lane-evidence.py`, runner `--self-test`, evidence guard, encrypted
  consumer caller guard — pass.

## Exact-pinned evidence

Healthy-mesh run: `target/interop/els2-evidence-plan398-none-rejection-categories-20261009`.
The authority controls and reference consumer payload passed. Reverse type-5 publication
was admitted, but the fixture payload did not return. Transaction delta:

- 14 TunnelData messages, 7 accepted cells, 7 complete Garlic envelopes;
- all 7 resolved to the correct service owner; zero owner misses and dispatch errors;
- zero authenticated dispatches, 7 rejected Garlic envelopes, zero payloads dequeued;
- zero AEAD-authentication or session-tag rejections; the then-current bounded
  classification placed all 7 in the typed binding family;
- zero server SYNs accepted and zero target dials.

Hashes: `evidence.json`
`c9647191f0bbc9ae1b7459557da449c9a497f1f5be6843c65d335330f3d8add1`, `results.tsv`
`0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv`
`03840f7a800cf2e8dc2d8d0acc9f446a19da468c9cf23397bdc55028f5dcaaab`.

The attempt after splitting the binding category failed the standard reference
self-connect control (`CANT_REACH_PEER / LeaseSet not found`) before reverse evidence;
another attempt had a pinned i2pd process segfault during setup. These are retained as
failed lane attempts, not product conclusions. Finer counters now distinguish missing
sender LS2, LS2 validation, sender-key mismatch, and unknown destination, but still need
a healthy exact-pinned run.

## Disposition and unblock audit

Plan 398 completed its ingress-boundary localization but did not meet the live reverse
payload acceptance criterion. Plan 399 owns the remaining typed binding outcome, a
healthy exact-pinned attribution run, and the first demonstrated corrective. No wire,
transcript, credential, support, capability, or advertisement change was made. Plans
374/375 and downstream 377/378 remain blocked on independent evidence; Plan 376 passed.
No future plan is unblocked.
