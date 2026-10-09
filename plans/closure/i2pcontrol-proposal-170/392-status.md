# Plan 392 closure — blocked

Status: **blocked-publication-coordinator-retry-leak-plan-393**.

Plan: `plans/implementation/i2pcontrol-proposal-170/392-bridge-inbound-projection-corrective.md`.

## Scope result

The staged-state transition is implemented: when a committed router-backed
generation includes a Destination with no router projection, the product
installs an identity-bound staged `RouterDestinationNetworkState`. It accepts
real inbound/outbound role projections before the first usable LS2, while
`router_ls2_for_publication` remains `None` until a real signed LS2 is
installed. Bridge append failures now have typed coarse reasons (router state
missing or duplicate receive projection), and the Plan 392 lane checker
guards this transition.

## Exact-pinned evidence

Run: `target/interop/els2-evidence-plan392-none-staged-router-state-20261009`.
Stock i2pd 2.61.0 at pinned SHA
`635b013a612ff47278ef02acf8580a28e10e26c5`, `MAX_ATTEMPTS=1`.

- Authority payload passed and reverse server committed.
- The pinned reference process segfaulted during the run; the runner continued
  with already-captured private mesh evidence. This is recorded as an
  environment event, not a pass.
- Reverse destination snapshot: 2 inbound and 2 outbound registrations, 2
  usable inbound leases, minimum 1, no pending builds, no build or registration
  failures, 2 successful post-activation inbound installations, and LS2
  present.
- Bridge typed rejection counters: zero router-state-missing and zero duplicate
  receive projections. The earlier Plan 392 NONE attempt had 37 router-state-
  missing rejections, confirming why staged state was required.
- Local publication still failed: 35 attempts, 0 admitted, 1 pending, last
  failure `PublicationCoordination`. Reverse DatabaseStore delivery/payload
  acceptance was not reached.
- SHA-256: `evidence.json`
  `e3767e66d26d2e260058668d9626d69ada1237dd6cf4b760238396e30c1483a2`,
  `results.tsv`
  `0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`,
  `driver-evidence.tsv`
  `3cfe58d8f0647041624747da2fd6e374367bc7418d8d49928d734a5d8e04e4d6`.

The final publication stage means `begin_ls2_publication` is failing after
repeated prior attempts. `publish_service_ls2_for_service` creates a pending
coordinator entry before composing/delivering, but error exits after creation
do not cancel it. The eight-entry publication bound can therefore be consumed
by retries. Plan 393 owns exact rollback and bounded retry regression.

## Verification

- Formatting, managed-app sibling build, and focused coordinator capacity test
  — pass.
- ELS2 runner and live evidence checker self-tests — pass.
- Evidence checker mutation table — 21 detected, 0 missed, 2 controls.
- Live evidence checker and encrypted-consumer caller guard — pass.
- Exact-pinned NONE — reverse LS2 created but publication did not reach
  delivery acceptance or payload.

The NONE gate blocks PSK/DH and full floor. No dependency, wire, support,
conformance, capability, or advertisement change.

## Successor and unblock audit

Plan 393 owns cancellation of the active local publication intent on every
failure after `begin_ls2_publication`, preserving bounded capacity and retry
semantics. Plans 374 and 375 remain blocked; Plan 376 remains passed; Plans 377
and 378 remain blocked on their existing hard dependencies. No future plan is
unblocked.
