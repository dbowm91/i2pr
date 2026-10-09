# Plan 399 closure — blocked

Status: **blocked-default-i2pd-signature-type-outside-selected-profile-plan-400**.

Plan: `plans/implementation/i2pcontrol-proposal-170/399-reverse-els2-garlic-binding-corrective.md`.

## Scope result and verification

Preserved the typed `LeaseSet2ValidationError` through inbound dispatch rather than
stringifying it, and added privacy-safe rejection categories. The live sample shows the
exact unsupported public algorithm code without retaining keys, identities, addresses,
or payloads. No DSA implementation or dependency was added.

- `cargo fmt --all` — pass.
- `cargo check --locked -p i2pr-client -p i2pr-daemon --all-targets` — pass.
- `cargo test --locked -p i2pr-client --test plan127_trajectory -- --test-threads=1` —
  16 passed.
- `inbound_traffic_delta_is_saturating_and_fieldwise` — pass.
- ELS2 checker and runner self-test — pass.

## Exact-pinned result

Run: `target/interop/els2-evidence-plan399-none-sigtype-20261009b`.
Reference client tunnel-pool readiness and its own ELS2 self-connect passed. The reverse
connect delivered 14 TunnelData, 7 complete Garlic envelopes, all 7 to the correct
service owner, and no service-dispatch errors. All 7 failed sender LeaseSet2 validation
because the unsupported signature algorithm code was **0**. No payload was queued and no
server SYN was accepted. The separate i2pr ordinary authority payload row failed, so this
artifact is diagnostic evidence only and does not satisfy the full live acceptance gate.

Hashes: `evidence.json`
`a58859a849fcde9ae95be515f937d0bf42a99742ec74d134f250c7cd5ed54881`, `results.tsv`
`0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv`
`5b061d44b878b7868eb136df3566c7e0d7060923a166ff262f6001808b178722`.

## Disposition and unblock audit

This is outside ADR 0004's selected type-7 profile: the inbound sender uses legacy
DSA-SHA1 type 0. Plan 399's stop condition forbids silently widening that profile. Plan
400 will keep the crypto boundary unchanged and configure the unmodified stock i2pd SAM
test destination to use the supported type 7, then execute the reverse auth matrix and
authority controls. No future plan is unblocked: Plans 374/375 and downstream 377/378
remain blocked on their independent evidence; Plan 376 passed.
