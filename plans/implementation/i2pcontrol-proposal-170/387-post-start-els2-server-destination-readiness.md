# Plan 387 — post-start ELS2 server destination readiness corrective

Status: **registered-post-start-els2-server-destination-readiness-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Corrects the local publication-stage finding from Plan 386. Its exact-pinned live
runs established that control-created type-5 service material and encrypted
address commit, but the post-start server Destination never has a router-backed
LeaseSet: 36 publication attempts produced 0 accepted handoffs and left one
server pending. The observed failure stage is `missing-leaseset`, before
DatabaseStore construction, floodfill selection, or remote lookup. Plan 386
remains a blocked evidence record; no reverse auth payload row passed.

## Objective

Make a server Destination added through the running Proposal 170 control path
reach usable inbound/outbound pool state, install its real signed LeaseSet2, and
schedule the matching type-5 DatabaseStore through the existing bounded
publication owner. Prove the local readiness transition and exact-pinned i2pd
reverse NONE/PSK/DH payload matrix with one lookup attempt per run.

This is evidence for only the rows executed. It does not promote ELS2 support or
advertisement.

## Dependencies and evidence

Hard/interface inputs: Plans 380 and 381; Plan 385's post-start authority
regression; Plan 386's live `MissingLeaseSet` stage evidence and blocked closure.
The service product, destination pool, control reconciliation, and type-5
publication seams already have stable written contracts. The remaining work is
the post-start server-runtime provisioning lifecycle.

## Invariants

1. Use unmodified stock i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`.
2. Keep `MAX_ATTEMPTS=1`; no remote lookup retries may hide publication delay.
3. Every payload crosses loopback SAM and the live I2P mesh.
4. Keep the Plan 346 type-11 transcript, Plan 380 credential seam, Proposal 170
   inventory, support inventory, and advertisement posture unchanged.
5. Do not treat a committed control definition, encrypted address, or queue
   insertion as LeaseSet or publication success.
6. Keep pool/build/publication queues bounded, owner-tracked, cancellation-safe,
   and retry-limited by existing policy.
7. Evidence contains only sanitized row outcomes, coarse counters, and hashes.

## Ordered work packages

1. Reproduce the control-created server transition with a redacted snapshot of
   runtime registration, inbound/outbound pool counts, usable inbound leases,
   minimum readiness, and LS2 presence. Identify the exact reconcile/activation
   transition that leaves the post-start Destination without its router state.
2. Fix that transition at the owner. Ensure an added server group is provisioned
   on the live product and is retried/coalesced through existing bounded pool
   ownership. Do not block unrelated router readiness on an optional service.
3. Add regression coverage for control-created server readiness, LS2 install,
   cancellation/reconcile, and publication scheduling; failure evidence must
   distinguish pool-not-ready, LS2-missing, and handoff failure.
4. Run the exact pinned live reverse matrix NONE/PSK/DH, requiring local
   publication handoff and returning payload in each mode, plus the post-start
   authority row and mesh controls.
5. Run relevant guards and the full routine floor only after every live row
   passes; close with requirement matrix, hashes, lifecycle/security review, and
   unblock audit.

## Verification

Before focused daemon tests, run the AGENTS.md managed-app sibling build. Run
`cargo fmt --all --check`, the focused product/service tunnel regressions, the
ELS2 runner self-test and evidence-checker self-test/mutation table, the
encrypted-consumer caller guard, and the exact live NONE/PSK/DH lane. Use a
separate ignored evidence directory per mode. Run the complete AGENTS.md floor
only after the reverse matrix passes.

## Acceptance and stop conditions

Pass requires a control-created server to reach its configured pool threshold,
install a real LeaseSet2, cross the local DatabaseStore delivery boundary, and
return the stock i2pd payload for NONE, PSK, and DH. The ordinary post-start
authority row, three-peer gossip controls, checker mutations, relevant ELS2
guards, and routine floor must pass.

Stop and register a further corrective if the fix requires transcript,
inventory, credential-seam, or advertisement changes; if the service cannot
reach the pool threshold under the frozen topology; or if the exact live gate
cannot be established.

## Lifecycle and compatibility

The server listener may remain locally bound while router-backed pool
provisioning is pending, but no publication or remote reachability success may
be reported before the real LeaseSet exists and outbound publication is
accepted. Reconcile and shutdown must cancel owned builds and clear pending
publication state. No persistent format, control schema, or public support
claim is expected to change.

## Closure evidence required

Record the Plan 386 local `MissingLeaseSet` evidence and its result hash; the
exact runtime transition found; tests/guards and exact live result hashes;
commands and outcomes; bounded lifecycle, failure, migration, and secret
review; findings; and the registry/roadmap unblock audit.
