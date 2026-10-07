# Plan 377 — ELS2 external evidence convergence and successor closure

Status: **registered-els2-external-convergence-blocked-on-plans374-and375**

Classification: external-evidence convergence + support-claim gate.

Hard dependencies:
- Plan 373 passed.
- Plan 374 passed.
- Plan 375 passed.

## Objective

Converge the independent i2pd and Java evidence into one authoritative ELS2 external-qualification
closure, superseding historical Proposal 170/347 for forward execution and satisfying the external
portion of historical Proposal 170/326.

This plan should not add protocol features. Any newly discovered code defect gets its own corrective.

## Evidence import

Import by exact SHA/hash:
- Plan-374 i2pd artifact + checker output;
- Plan-375 Java artifact + checker output;
- Proposal 170/346 crypto/profile evidence;
- Proposal 170/350 floodfill type-5 serve evidence;
- Proposal 170/351 production consumer evidence.

Fail if an imported artifact was produced on an implementation SHA incompatible with the current
tree and cannot be reproduced.

## Required four-direction matrix

The converged matrix must show pass for:

| Publisher/service | Consumer/client |
|---|---|
| i2pr | i2pd |
| i2pd | i2pr |
| i2pr | Java I2P |
| Java I2P | i2pr |

For every row require:
- real DatabaseStore type 5;
- storage under blinded key;
- real blinded-key DatabaseLookup;
- outer deployed-profile verification;
- authorization/decrypt;
- inner LS2 validation;
- streaming/application payload.

No crypto-only row can satisfy this table.

## Auth-mode convergence

Produce a single capability matrix for:
- no-auth;
- lookup secret;
- PSK;
- DH/X25519.

Classify each reference/mode as:
- passed both directions;
- passed one applicable direction;
- reference-not-applicable with source proof.

Do not extrapolate from Emissary or one reference family to the other.

## Security/regression convergence

Re-run:
- strict Proposal-146 official vectors;
- generic verifier rejects deployed transcript;
- ELS2-only verifier accepts the correct deployed/strict compatibility set and fails ambiguous;
- type-5 floodfill remains opaque;
- B33 malformed/wrong-secret/wrong-key failures;
- secret redaction;
- bounded consumer in-flight table;
- service failure isolation for the Plan-351 DelayOpen gate, now through at least one live
  multi-service composition.

That last row closes Plan-351's recorded partial criterion 4.

## Support consequences

On pass, update support/conformance truth to state the exact externally qualified ELS2 subset.

Permitted promotion:
- type-5 publication/store/serve/consume can move from "local only" to externally qualified for the
  exact Java/i2pd overlap proven here;
- B33/deployed type-11/no-auth and individually proven auth modes may be claimed.

Do **not** automatically set the whole I2PControl subsystem to `full-proposal-conformant`; Plan 378
owns that claim.

Historical Proposal 170/326, /335, /347 closure records remain immutable. Registry/roadmap status
must say they are superseded for forward execution by 377.

## Acceptance criteria

Plan 377 passes only when all four mandatory directions pass end to end and the resulting support
inventory precisely distinguishes qualified modes from reference-not-applicable ones.

Passing Plan 377 is the ELS2 prerequisite for Plan 378.
