# Plan 391 closure — blocked

Status: **blocked-bridge-inbound-projection-rejection-plan-392**.

Plan: `plans/implementation/i2pcontrol-proposal-170/391-post-activation-inbound-ownership-corrective.md`.

## Evidence

The Plan 391 exact-pinned NONE diagnostic is
`target/interop/els2-evidence-plan391-none-postactivation-20261009`.
Stock i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5` ran with
`MAX_ATTEMPTS=1`. Authority payload passed; reverse server creation committed.
After 187.10 seconds, the lane failed at local DatabaseStore delivery because
the server had no LS2.

Destination-scoped counters identify the transition:

- 46 inbound submissions; 40 material completions; 4 build failures.
- 40 inbound pool material registrations and 3 outbound registrations; no
  material/pool/tunnel/lifetime/role-activation failures.
- 40 inbound receive-owner registrations succeeded; zero pool-registration
  lookup and owner failures.
- 40 bridge inbound receive projections were rejected; zero successful
  inbound installations.
- Current pool registrations and usable leases were zero; LS2 absent.
- Lease publication: 35 attempts, 0 accepted, 35 failed, 1 pending,
  `MissingLeaseSet`.

SHA-256: `evidence.json`
`c8efc39ac537e2f2cf1cba885936bbad8d46a6381a974ce881e58ae8e74147bb`,
`results.tsv`
`0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
`driver-evidence.tsv`
`1185366a49d3ae3130dbc8b5b629feb490165de28f06df7418f2152eb78d039b`.

The remaining rejection is inside `DestinationBridge::append_router_inbound_receive`:
it returns an error if router network state is absent or if the receive ID is
already present. The current evidence intentionally does not distinguish these
two reasons. Plan 392 owns typed/coarse attribution and correction.

## Verification

- Formatting and managed-app sibling build — pass.
- Focused aggregate-capacity and inbound registry release/max+1 tests — pass.
- ELS2 runner and live evidence checker self-tests — pass.
- Evidence checker mutation table — 20 detected, 0 missed, 2 controls.
- Live evidence checker and encrypted consumer caller guard — pass.
- Exact-pinned NONE — did not reach LS2 or return reverse payload.

The NONE failure gates PSK/DH and the full floor. No dependency, wire,
credential, inventory, support, capability, or advertisement change. No
unbounded diagnostic state or sensitive identifiers were added.

## Successor and unblock audit

Plan 392 owns distinguishing absent bridge router state from duplicate receive
projection, then correcting the failing bridge lifecycle transition. Plans 374
and 375 remain blocked; 376 remains passed; Plans 377 and 378 remain blocked on
their hard dependencies. No downstream plan is unblocked.
