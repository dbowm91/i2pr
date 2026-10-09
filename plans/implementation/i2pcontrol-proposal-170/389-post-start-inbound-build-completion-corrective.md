# Plan 389 — post-start inbound build completion and owner attribution corrective

Status: **blocked-destination-role-registry-capacity-plan-390**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Corrects Plan 388's remaining boundary. Plan 388 fixed outbound-first
concurrency starvation; the exact-pinned NONE lane then observed two pending
inbound builds, zero registrations, and no LS2 after 180 seconds. The product
coordinator had 43 inbound replies routed and 11 builds installed overall, but
its existing counters are not scoped to a Destination. Do not infer that the
control-created server received or completed any of those replies.

## Objective

Localize and correct why the post-start server's submitted inbound builds do
not become registered inbound tunnels and a usable signed Standard LS2. Make
the bounded observation identify whether replies correlate to those attempts,
the build state machine establishes them, owner registration succeeds, or
lease derivation/installation fails. Preserve the Plan 388 fair scheduler.

## Dependencies and evidence

Hard/interface inputs: Plans 380/381 for the credential and pinned live-lane
contracts; Plans 385–388 for post-start authority, publication, readiness, and
build scheduling; Plan 388's clean pinned NONE counters. The existing product
coordinator, Destination registration, generation, and publication contracts
are stable.

## Invariants

1. Use unmodified stock i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5` and `MAX_ATTEMPTS=1`.
2. Keep all payload traffic on loopback SAM and the live private mesh.
3. Keep diagnostics bounded, numeric/coarse, and destination-owner scoped;
   never record hashes, addresses, attempt IDs, keys, credentials, LS2 bytes,
   payloads, or raw reference logs.
4. Keep one coordinator, preserve fair inbound/outbound scheduling, pending
   limits, retry/failure limits, generation cancellation, and shutdown
   ownership.
5. Do not change Plan 346 transcript semantics, Plan 380 credential seam,
   Proposal 170 inventory, support inventory, or advertisement posture.

## Ordered work packages

1. Add the smallest bounded per-Destination stage accounting needed to separate
   submitted inbound builds, correlated replies, completed build material,
   failed/cancelled attempts, Destination registration, and usable lease/LS2
   installation. Clear retired-generation counters with their owner; do not
   create an unbounded history map.
2. Run a clean exact-pinned NONE diagnostic and use those counters to name the
   first failing transition. If a clean run cannot be obtained, stop and record
   the reference process failure without claiming a product result.
3. Trace the source owner at the identified transition and add a deterministic
   regression that fails before the fix. Include server creation after startup,
   a subsequent generation replacement cancelling old pending builds, and
   scheduling after the new generation commits.
4. Correct only the identified transition. Keep ordinary service and
   post-start authority behavior passing.
5. Run exact-pinned reverse NONE first. Only after NONE returns its payload,
   run PSK and DH, the authority payload, and three-peer controls. Then run the
   ELS2 guards and routine floor.

## Verification

Before focused daemon tests, run the AGENTS.md managed-app sibling build. Run
formatting, focused product/coordinator tests, the runner self-test, evidence
checker self-test and mutation table, encrypted-consumer caller guard, and the
clean exact-pinned NONE diagnostic. PSK/DH and the full routine floor are
conditional on successful NONE payload return.

## Acceptance and stop conditions

Pass requires the control-created server to register at least the configured
minimum usable inbound leases, install its signed LS2, cross local
DatabaseStore admission, and return the stock i2pd NONE/PSK/DH payloads. The
post-start authority row, three-peer controls, bounded cancellation/generation
regression, checker mutations, ELS2 guards, and routine floor must pass.

Register a further corrective if the fix needs transcript, inventory,
credential-seam, or advertisement changes; the pinned topology cannot satisfy
the existing threshold; or a clean exact-pinned diagnostic cannot be obtained.
Reference process crashes are environment failures, not product passes or
failures.

## Lifecycle, compatibility, and security

All new work remains owned by the existing product coordinator and is cancelled
on generation replacement or shutdown. A committed server may remain locally
bound while provisioning runs, but cannot be reported published or remotely
reachable before a real LS2 exists and local delivery admission succeeds. No
persistent format, control schema, or public support claim changes. Diagnostics
must not expose identifiers or secret-bearing material.

## Closure evidence required

Record Plan 388's clean scheduler diagnostic hash, destination-scoped
transition counters, source root cause and regression, generation cancellation
evidence, exact commands/results, hashes for every executed live row, bounded
lifecycle and secret review, findings, and the registry/roadmap unblock audit.
