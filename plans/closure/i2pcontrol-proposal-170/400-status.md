# Plan 400 closure — blocked

Status: **blocked-reverse-authorized-els2-lookup-fails-after-type7-plan-401**.

Plan: `plans/implementation/i2pcontrol-proposal-170/400-i2pd-reverse-signature-profile-qualification.md`.

## Scope result and verification

The stock reference requester in the Rust driver now creates its transient SAM
destination with `SIGNATURE_TYPE=7`, matching ADR 0004. The standalone reference controls
use the same setting. The runner records the selected type, and the evidence checker
mutates both the Rust request and evidence marker to ensure the setting cannot silently
revert to the legacy default.

- `cargo fmt --all` — pass.
- managed-app sibling build and focused `els2_i2pd_external` check — pass.
- ELS2 source checker, runner self-test, evidence guard, encrypted-consumer caller guard —
  pass.
- Plan 400 NONE exact-pinned lane — pass.

## Exact-pinned NONE result

Run: `target/interop/els2-evidence-plan400-none-type7-complete-20261009`.
All runner rows passed, including reference ELS2 self-connect, reference standard-LS2
self-connect, i2pr ordinary authority, and reverse NONE payload. Reverse transition delta:
one complete Garlic envelope reached the correct service owner, authenticated, dequeued
one payload, and one canonical Streaming packet was accepted. The sanitized driver row
records signature type 7.

Hashes: `evidence.json`
`810b270b87e6fed7f4293303b173e292c39e8a6505a268269b84db2e933e46c3`, `results.tsv`
`78cf607e4be173ffecb3abda69b7de945135aa7bf4c82442902ed7f090461e0f`,
`driver-evidence.tsv`
`b90f93c8ab73d040d0cc256b31fd2f1be1f41ec8ff0658747457b29b354322d3`.

## PSK result and disposition

Run: `target/interop/els2-evidence-plan400-psk-type7-20261009`. Reference mesh and
standard-LS2 controls passed; the i2pr authority payload passed. The reverse publication
was accepted locally, but the stock i2pd client received
`CANT_REACH_PEER / LeaseSet not found` before a reverse payload or inbound transaction
delta. This run does not establish whether the authorized type-5 record was absent from
the selected floodfill, failed client authorization, or hit another reference-side
lookup condition. The attempt is retained as a failure. Plan 401 owns PSK/DH source and
transaction attribution; no production behavior was changed for authorization.

No DSA-SHA1 support, new dependency, wire/transcript change, support, capability, or
advertisement change. Type 0 remains outside the selected profile; the NONE row is
qualified only with the explicit type-7 requester profile.

## Unblock audit

Plan 401 is registered immediately for the failed authorized reverse boundary. Plan 400
does not pass its full NONE/PSK/DH acceptance. Plans 374/375 and downstream 377/378 remain
blocked on independent evidence; Plan 376 passed. No future plan is unblocked.
