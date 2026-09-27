# Plan 256 status — retained; corrective required via Plan 257

Status:
**retained-m11-i2pd-qualification-evidence-topology-corrective-required-via-plan257**

Plan:
plans/implementation/transit-tunnels/256-m11-i2pd-qualification-evidence-topology-corrective.md

Registration baseline:
014f72d3e9c0a4128b0d6cacdb003b55c0943b12

Implementation head reviewed:
bc9b5c6172d0560d3eb8d2ef1ae0d447478ba480

Corrective authority:
plans/implementation/transit-tunnels/257-m11-production-self-reply-and-external-evidence-completion-corrective.md

## Disposition

Plan 256 is not closed as M11 qualification.

Its implementation materially repaired the Plan 255 qualification scaffold and those repairs are
retained. Post-implementation source/evidence review found remaining qualification gaps and also
found that Plan 256 crossed its own production-defect stop condition by adding local-IBGW /
OBEP reply-path production behavior. Plan 257 is therefore the current corrective authority.

Historical Plan 256 implementation evidence is preserved. This status narrows its interpretation;
it does not rewrite that evidence.

## Retained implementation evidence

The bc9b5c6172d0560d3eb8d2ef1ae0d447478ba480 implementation retains useful corrections:

- tunnel-build responder secret comes from the same RouterIdentityBundle encryption key advertised
  by the signed i2pr RouterInfo;
- SSU2 transport key remains distinct;
- i2pd-A and i2pd-B are real exact-pinned reference processes with explicit datadirs;
- public i2pr RouterInfo is written to the source-locked NetDB owner before reference startup;
- Participant topology provisions a real i2pd-B;
- role evidence is typed by TransitHopRoleKind and epoch;
- the Plan 255 generic observed-build fan-out is removed;
- OBEP data exercises real unfragmented and fragmented/reassembled SAM traffic;
- IBGW data exercises genuine gateway ingress and bounded multi-cell emission;
- code-30 rejection has a dedicated rejecting epoch;
- replay/expiry/cancel/session-close/restart have dedicated event shapes rather than generic pass
  rows;
- static checker coverage was substantially strengthened.

These are retained infrastructure/qualification improvements, not a completed external pass.

## Unclosed findings

### High — no complete counted external pass

No M11 transit external workflow execution has closed the corrected Plan 256 matrix. The
mandatory two complete same-SHA passes remain unexecuted.

### High — production reply behavior landed inside a qualification-only corrective

Plan 256 introduced production local-IBGW self-reply / OBEP OTBRM behavior and associated garlic
reply helpers after its own stop condition required a narrow successor for production defects.
That behavior must be source-locked and qualified/corrected by Plan 257 before M11 closure.

### High — Participant far-side receipt is not independently proven

The lane records local Participant forwarding and A-side creator acceptance but does not bind the
counted data cell to an independent i2pd-B endpoint/counter observation for the exact next tunnel.

### High — restart evidence is constructor-only

The restart row constructs a new TransitLiveOwner and checks zero active state. It does not stop
and recreate the controlled i2pr runtime, re-establish authenticated sessions, and prove a fresh
post-restart accepted build.

### High — cancellation/session-close state evidence is incomplete

Cancellation proves active registration count drains but does not directly prove pending
reservations, peer index, and transit-owned queued work all return to zero. Session close proves
A removal but not that unrelated B remains.

### Medium — bandwidth disposition is not decoded evidence

The rejection epoch records that reference options were unobserved rather than deriving m/r/l
absence/presence and b disposition from the typed decoded request/reply transaction.

### Medium — exact registration cardinality is not directly evidenced

Role rows record accepted receive ids but do not yet prove a before/after active-state delta of
exactly +1 for each counted accepted build.

### High — exact-head ordinary CI is not green

Actions run 36332304955 on bc9b5c6172d0560d3eb8d2ef1ae0d447478ba480 had MSRV and dependency policy passing at this audit.
macOS Quality failed in the Clippy step. Ubuntu Quality was still running. Plan 256 therefore
does not satisfy its ordinary exact-head CI closure requirement.

## Requirement-to-evidence disposition

| Plan 256 area | Disposition |
| --- | --- |
| RouterIdentity/build-key coherence | retained |
| exact NetDB owner/bootstrap | retained |
| real i2pd-B provisioning | retained |
| typed role/epoch anti-fan-out | retained |
| OBEP semantic data | retained pending complete counted pass |
| IBGW gateway/multi-cell data | retained pending complete counted pass |
| Participant local forward | retained; far-side proof required |
| code-30 epoch | retained; typed bandwidth evidence required |
| replay/expiry | retained pending complete counted pass |
| cancellation | corrective required |
| session close | corrective required |
| restart | corrective required |
| exact registration cardinality | corrective required |
| two same-SHA complete passes | not executed |
| exact-head ordinary CI | failed/incomplete |

## Security/resource review

No review finding authorizes public transit. Ordinary product construction remains
transit-disabled and public reseed/network are outside the lane. Secret/private material must
remain absent from retained evidence.

Plan 257 must retain bounded state, no per-cell task spawning, fail-closed missing prerequisites,
and exact reference pinning.

## Roadmap disposition

- Plan 256: retained corrective infrastructure; not M11 qualification closure.
- Plan 257: registered ready as corrective/qualification-completion authority.
- M11 experimental progression: open.
- M12 floodfill planning: blocked until Plan 257 closes with two complete same-SHA exact-pinned
  i2pd passes and exact-head ordinary CI green.
