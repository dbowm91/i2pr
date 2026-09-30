# Plan 266 — M11 receipt-family reference-completion opportunity generation corrective

Status at registration:
**registered-m11-receipt-family-opportunity-generation-corrective-ready**

Corrects:

- `plans/closure/transit-tunnels/265-status.md`: the `receipt` scenario family
  closed 1/8 against a required 2 on qualification SHA
  `4682920eb0b28b0b590aaabddb8666253c26f081`, with **zero** i2pr semantic
  failures in all 24 Plan 265 attempts.
- `plans/implementation/transit-tunnels/265-m11-per-epoch-emission-sustainability-corrective.md`
  §14: the "family ends attempt 8 with fewer than two required successes"
  stop condition, whose stated disposition is that the successor owns
  *qualification opportunity generation only*.

Retains in full:

- the Plan 265 fixed-budget contract: manifest v5, the closed 16-token
  terminal vocabulary, the three input-side opportunity predicates, exactly
  eight retained fresh-mesh attempts per family on one qualification SHA, no
  success-based early stop, no attempt nine, and the composition gate with its
  negative fixtures;
- every Plan 265 execution: `ibgw-data` 4/8 and `participant-lifecycle` 5/8
  are **closed** and are not re-executed;
- Plan 262 production semantics and authority: dedicated `TransitGatewayData`,
  exact IBGW receive-id ownership, source-neutral `route_ibgw_gateway`,
  self-targeted OBEP TUNNEL loopback, `LocalIbgwDelivered` /
  `LocalIbgwDropped`, no synthetic peer/index mutation, unchanged
  Participant/OBEP previous-peer locks, and a byte-identical
  `crates/*/src` tree at `514bf1237e86fde21e17fc98c743eb52852edd99`;
- the Plan 264 five-epoch retained evidence and its integrity index;
- exact-pinned, unmodified i2pd 2.61.0 at
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- loopback-only controlled qualification, fresh datadirs/ports/evidence
  roots, no public reseed/network fallback, no reference patching.

## 1. Objective

Close the one remaining M11 experimental-qualification row: two
tuple-bound exact-once creator-owned receipt completions, without changing
production routing, without relaxing any frozen counting rule, and without
converting reference nondeterminism into retry-until-green.

Plan 265 localized the shortfall precisely. Across its eight `receipt`
attempts:

- 5 never produced a B-originated self-targeted TUNNEL action on a live
  counted creator-A IBGW receive id (`receipt-no-live-counted-ibgw-target`);
- 1 stopped before the family's input-side boundary
  (`receipt-setup-stop`);
- 2 produced the action, and **i2pr passed both**: multicell emission
  (`nested` 1487–1488, `emitted=2`, `failures=0`), the correct next router
  (creator A), and a bound six-field receipt tuple (`full-tuple-bound=true`);
- 1 of those 2 completed at the creator receiver SAM socket
  (`gateway-receipt=1`, `gateway-receipt-once=true`); 1 did not
  (`receipt-reference-completion-miss`).

So the only missing facts are *reference-side*: the B-originated
self-targeted action, and the creator's own socket completion. Neither is an
i2pr semantic question, and Plan 262 production routing must not be reopened.

## 2. Why the action was absent on 5 of 8 attempts

The retained evidence points at a single upstream window, not at three
independent windows:

- the same reference topology produced the action twice in eight fresh
  meshes, i.e. the opportunity is intermittent in the same way Plan 264
  localized for fragmentation, and Plan 265's fixed budget was sized for
  roughly that rate;
- but the *creator-side* inbound pool is what the action depends on. A B
  sender can only self-target a live creator-A IBGW receive id if the
  creator's LeaseSet still advertises that id and the relay B still resolves
  it. Plan 265 already records both prerequisites
  (`b-leaseset-resolved`, `b-self-targeted-actions`); what it does **not**
  record is *why* they failed, only that the conjunction did not hold.

The corrective therefore does not add budget. It **adds an
input-side opportunity ladder for the receipt family** that names which
upstream precondition was missing, so the successor can act on the actual
window instead of re-rolling all three.

## 3. Frozen invariants

### 3.1 Product

Identical to Plan 265 §3.1: no production `crates/*/src` changes; ordinary
i2pr transit stays disabled; no RouterInfo capability or `router.version`
change; no public transit config option; no wire-format change; no
task/channel/queue addition; no quota, timeout, message-size, retry-round, or
admission-ceiling inflation; no M12 implementation.

### 3.2 Reference/network

Identical to Plan 265 §3.2. The reference stays **unmodified**. In
particular: no source patch, no vendoring, no synthetic LeaseSet, no injected
tunnel, no fake peer, no larger stimulus to widen the window, and no public
network as a fallback for a missing loopback opportunity.

### 3.3 Evidence and budget

- The Plan 265 contract is inherited unchanged, including the eight-attempt
  budget and the "no post-result adjustment of the counting rule" rule.
- The `ibgw-data` and `participant-lifecycle` families are **not**
  re-executed. Their Plan 265 evidence stays the authority for those rows.
- Every dispatched `receipt` attempt is retained in the denominator, with the
  same vocabulary, the same input-side-only opportunity rule, and the same
  composition gate.
- The frozen Plan 265 terminal vocabulary gains **only** declared
  no-opportunity tokens for the new opportunity ladder. It loses none. No
  existing terminal's meaning changes.
- Opportunity classification remains decided from an input-side predicate,
  never from the downstream success result. The static
  `--check-input-side` guard gains the new predicates in its forbidden-field
  lists on the same terms as the existing three.
- Raw reference logs remain diagnostic only.

## 4. Opportunity ladder for the receipt family

Replace the single boolean `b_self_targeted` with a finite, ordered,
input-side ladder. Every rung is an input-side fact observed before any
local loop or emission is evaluated, and every rung maps to exactly one
declared terminal. The ladder is evaluated top-down; the first unsatisfied
precondition names the attempt.

| rung | input-side precondition | terminal when this is the first unsatisfied rung |
|---|---|---|
| 1 | the reference creator's counted receiver session reported ready and its creator-A IBGW receive id was accepted | `receipt-setup-stop` |
| 2 | B's outbound session reached `[i2pr]` and the typed OBEP accept replied to B | `receipt-setup-stop` |
| 3 | B-side LeaseSet resolution is not contradicted by the reference log | `receipt-setup-stop` |
| 4 | B originated at least one self-targeted TUNNEL action during the attempt | `receipt-no-b-originated-self-action` |
| 5 | the creator's LeaseSet still advertised the counted IBGW receive id when B resolved it | `receipt-creator-leaseset-lost-ibgw-id` (new) |
| 6 | the self-targeted action addressed a live counted creator-A IBGW receive id | `receipt-no-live-counted-ibgw-target` |
| 7 | the nested input exceeded the one-cell capacity and the registration was provably live | opportunity PRESENT |
| 8 | the local source-neutral IBGW seam delivered it with zero gateway/forward failures, a correct next router and a nonzero committed next tunnel | semantic half |
| 9 | the six-field receipt tuple validated | semantic half |
| 10 | the creator receiver SAM socket delivered the exact 1400-byte 0xA5 payload exactly once | external completion |

Rung 5 is the one genuinely new fact. It is input-side and observable without
production change: the creator publishes its LeaseSet through its own
reference, so the driver can compare the **IBGW receive id set the creator
advertised** against the **set B addressed**. A B-originated self-targeted
action naming an id the creator no longer advertises is a creator-side pool
churn fact, not an i2pr fact, and must not be counted as an i2pr opportunity
or an i2pr failure.

New terminal tokens (declared additions to the closed vocabulary; total
becomes 17):

- `receipt-creator-leaseset-lost-ibgw-id`

No token is repurposed. `receipt-no-b-originated-self-action` and
`receipt-no-live-counted-ibgw-target` keep their exact Plan 265 meaning.

## 5. Work packages

### WP A — the opportunity ladder (harness only)

- Split the receipt family's input-side evaluation into the ten rungs above,
  each a pure function over typed observations plus reference-log facts, with
  the rung index and the first-unsatisfied rung recorded as sanitized rows.
- Add `receipt-creator-leaseset-lost-ibgw-id` to the closed vocabulary and to
  the no-opportunity set; add the new predicates to the static
  `--check-input-side` forbidden-field lists.
- Keep the existing per-input `plan265-large-input` trace rows.

### WP B — the creator-advertised-id comparison (harness only)

- Read the creator's advertised inbound gateway receive ids from the
  reference's own NetDB/LeaseSet publication path (the same source-locked
  path the lane already installs into), and compare them against the
  addressed set. Sanitized counts and id-presence facts only; no payload
  bytes, no keys.
- If the advertisement is not observable from the harness, record the rung as
  **unobservable** and fall back to the Plan 265 rung-6 terminal unchanged.
  The ladder must never *require* a fact the lane cannot see.

### WP C — fixed-budget execution and composition

- Freeze one new qualification SHA after the local floor.
- Execute exactly eight `receipt` attempts, ordinals 1..8, fresh mesh each,
  in the frozen order, on that SHA.
- Extend `--compose-265` with a `--receipt-only` mode that composes the new
  family against the retained Plan 265 `ibgw-data` and
  `participant-lifecycle` evidence and the Plan 264 five-epoch index. It must
  refuse to accept a fresh `ibgw-data` or `participant-lifecycle` set, so the
  closed families cannot be re-rolled.
- Add negative fixtures: a receipt-only composition that tries to substitute
  a Plan 265 reference-completion miss for a socket completion; one that
  tries to compose the new rung-5 terminal as an opportunity; one that
  presents nine attempts; one that mixes qualification SHAs.

### WP D — closure

- Close only if the `receipt` family reaches ≥2 qualified successes with zero
  semantic failures, all eight attempts retained and classified, the retained
  Plan 265/264 evidence still integrity-checked, and the local floor green.
- If it does not, record the rungs that were first-unsatisfied across the eight
  retained attempts and name the next narrow corrective's scope from that
  distribution rather than from a guess.

## 6. Required focused tests

At minimum:

1. all 20 retained `plan265_*` tests still pass unchanged;
2. each of the ten rungs maps to exactly one declared terminal;
3. an opportunity-present receipt attempt with a correct local seam and a
   socket delivery classifies as `receipt-tuple-bound-socket-verified`;
4. a correct local seam with no socket delivery classifies as
   `receipt-reference-completion-miss`, never as a semantic failure;
5. a B self-targeted action naming an id the creator no longer advertises
   classifies as `receipt-creator-leaseset-lost-ibgw-id`;
6. the same action naming an advertised, live, counted id is an opportunity;
7. an unobservable creator advertisement falls back to the Plan 265 rung-6
   terminal rather than failing closed on an invisible fact;
8. the new predicates are rejected by `--check-input-side` when they read an
   output-side field;
9. the receipt-only composer refuses a fresh `ibgw-data` or
   `participant-lifecycle` set;
10. the receipt-only composer rejects nine attempts, mixed qualification SHAs,
    a changed budget, a duplicate ordinal, an unclassified terminal, and a
    semantic failure;
11. the production-diff guard still reports an empty `crates/*/src` diff;
12. ordinary product transit remains disabled.

## 7. Exact verification floor

Plan 265 §13 unchanged, plus:

```text
bash scripts/check-m11-per-epoch-composition.sh --self-test
bash scripts/check-m11-per-epoch-composition.sh --compose-265 --receipt-only <evidence>
```

Every new focused test must be run by exact name.

## 8. Stop conditions

Stop and register a further narrow corrective only if:

- a `receipt` opportunity-present attempt produces any i2pr semantic
  contradiction;
- the creator-advertised-id comparison would require a production change to
  become observable;
- a new rung-5 terminal cannot be derived from a sanitized, non-secret fact;
- an attempt cannot be classified by the frozen vocabulary;
- reference patching, public fallback, timeout/quota/message-size
  enlargement, or a new dependency would be required.

If fewer than two qualified successes occur within eight attempts and every
opportunity-present input still passes i2pr's semantics, this plan does not
license a second budget increase. The distribution of first-unsatisfied
rungs across the eight retained attempts is then the evidence, and the next
scope must be derived from it.

## 9. Acceptance criteria

1. `receipt` has ≥2 tuple-bound exact-once SAM completions on one
   qualification SHA.
2. Exactly eight retained attempts, ordinals 1..8, all classified from the
   frozen vocabulary.
3. Zero i2pr semantic failures.
4. The `ibgw-data` and `participant-lifecycle` Plan 265 evidence is still
   integrity-checked and is not re-executed.
5. `git diff --name-only 514bf1237e86fde21e17fc98c743eb52852edd99..HEAD -- 'crates/*/src'`
   is empty.
6. Complete local verification passes, including every new focused test.
7. Exact-head ordinary CI passes all four jobs.
8. Registry, roadmap, support inventory, conformance and the tunnel dossier
   agree on the final M11 disposition.
9. No product default, capability, version, or public-network change.
10. No critical/high finding remains open.

Only then may the unblock audit mark ADR 0026's one-family M11 experimental
qualification passed and make M12 planning dependency-ready. Public transit
participation remains a separate future decision.

## 10. Handoff order

1. implement the opportunity ladder and the one new declared terminal;
2. implement the creator-advertised-id comparison with its unobservable
   fallback;
3. add the receipt-only composition mode and its negative fixtures;
4. run the complete local floor;
5. freeze one qualification SHA;
6. execute exactly eight `receipt` attempts;
7. compose once;
8. obtain exact-head ordinary CI;
9. close only if §9 passes exactly.

No production routing changes and no post-result adjustment of the counting
rule.
