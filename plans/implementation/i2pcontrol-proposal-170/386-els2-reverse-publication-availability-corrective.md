# Plan 386 — ELS2 reverse publication availability corrective

Status: **registered-i2pr-els2-reverse-publication-availability-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Corrects the unresolved live publication result from Plans 384–385. Plan 385's
ordinary post-start authority payload now passes, but its reverse NONE and PSK
attempts reach the stock i2pd consumer and return `CANT_REACH_PEER` / `LeaseSet
not found`. Its DH run fails earlier in the existing reference-publishes
consumer row. Plan 385 is retained as a blocked implementation/evidence record;
this plan owns the remaining live matrix and publication diagnosis.

## Objective

Trace the control-created i2pr type-5 service from committed tunnel config
through local record creation, storage-key selection, publication, router
database handling, and retrieval by stock i2pd 2.61.0. Fix any demonstrated
product defect at its owner, then pass the complete reverse NONE/PSK/DH payload
matrix and retain the already-passing standard post-start authority row.

This is a live interoperability result for only the rows executed. It does not
promote Proposal 170 support or advertisement.

## Dependencies and evidence

Hard/interface dependencies: Plans 346, 350, 351, 380, and 381. Plan 385 is a
blocked evidence input: it closes the ordinary standard-lookup defect and
passes the post-start authority payload, while leaving reverse publication
unproven. The exact-pinned controlled mesh, runner, driver, sanitized evidence
schema, and gossip gate are reusable.

Current failure observations:

- With stock i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`, the
  NONE reverse request returns `CANT_REACH_PEER` / `LeaseSet not found`.
- The PSK reverse request returns the same lookup failure after the control
  create commits.
- The DH reverse request also returns `CANT_REACH_PEER` / `LeaseSet not found`
  after its consumer and authority payload controls pass.
- Plan 385's standard post-start authority payload succeeds with the same
  stock mesh and one-attempt lookup gate.

These observations locate the open boundary at live reverse publication or
its DHT visibility, but do not yet distinguish an absent publish, wrong
storage key, rejected store, missing floodfill response, or an earlier
reference consumer step. The diagnosis must name the observed transition; do
not infer from the final SAM status alone.

## Invariants

1. Use unmodified stock i2pd `2.61.0` at
   `635b013a612ff47278ef02acf8580a28e10e26c5`.
2. Keep `MAX_ATTEMPTS=1`; do not mask publication/lookup failures with retries.
3. Keep the type-11 transcript profile, Proposal 170 inventory, Plan 380
   consumer credential seam, support inventory, and advertisement posture
   unchanged.
4. Keep type 5 non-advertised; do not set `full-proposal-conformant`.
5. Require the existing three-peer standard-LS2 gossip gate and mesh controls.
   Missing reference inputs fail closed.
6. Evidence contains sanitized row outcomes and hashes only. Never retain
   credentials, raw reference logs, or unsanitized control responses.
7. Every payload crosses the real loopback SAM/I2P path and the pinned
   reference. No injected LeaseSet, private resolver, or direct test-only data
   path.
8. Do not weaken lifecycle, bounded queue, cancellation, or advertisement
   invariants to make a row pass.

## Ordered work packages

1. **Reproduce and instrument:** preserve separate sanitized NONE, PSK, and DH
   runs. Add only bounded stage evidence at the actual publication and remote
   database boundaries. Establish whether the record is published, which key
   is used, and whether a floodfill stores/returns it. Keep raw logs outside
   evidence and scrub every credential.
2. **Correct:** fix the demonstrated publication, key, storage, or retrieval
   defect at its owner. Add regression evidence for the failed transition and
   cancellation/restart behavior. If the fix requires transcript, inventory,
   credential-seam, or advertisement changes, stop and register that separately.
3. **Requalify NONE:** run the exact pinned one-attempt lane and require the
   control-created type-5 payload to return through i2pd.
4. **Requalify PSK and DH:** use fresh keys per run and pass each payload row;
   the consumer must supply the matching PSK or DH private key. Retain
   authority and mesh controls in every run.
5. **Close or stop:** run checker self-tests/mutations, relevant ELS2 guards,
   the full routine floor after all live gates pass, then close with a
   requirement/evidence matrix and unblock audit. If any mode remains
   unproven, keep dependent plans blocked and register the next corrective.

## Verification

Run focused regression and lane guards first. Before any focused daemon test,
build `i2pr-app-fixture`, `i2pr-apphost`, `i2pr-appd`, and `i2pr-appctl` as
required by `AGENTS.md`. Use separate `I2PR_ELS2_EVIDENCE_DIR` locations for
each auth mode. Preserve the exact commands, result hashes, and distinction
between failed and unrun rows. Run the AGENTS.md routine floor only after all
required live rows pass.

## Acceptance and stop conditions

Pass requires post-start authority plus reverse NONE/PSK/DH payload rows, the
gossip gate and both mesh controls, all checker mutations, relevant ELS2
guards, and the routine floor. The evidence checker must fail closed on a
missing row, pin/attempt drift, unsanitized credential, or failed lane marked
green.

Stop and register a further corrective if the diagnosis points to transcript,
inventory, credential-seam, or advertisement changes; if a mode cannot pass
under the frozen reference and bounded lifecycle; or if the exact live gate
cannot be established.

## Lifecycle and compatibility

Publication and database work stays bounded by existing runtime ownership and
cancellation. No persistent-format or public control-schema change is
expected. A service that cannot publish or retrieve its LeaseSet must remain a
typed failed/unresolved service; do not report success based only on a local
encrypted address projection.
