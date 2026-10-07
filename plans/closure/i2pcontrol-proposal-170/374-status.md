# Plan 374 — Stock i2pd bidirectional ELS2 qualification: status

Status: **blocked-reference-freeze-and-substrate-verified-live-els2-driver-not-written**

Plan of record:
[`374-i2pd-live-els2-qualification.md`](../../implementation/i2pcontrol-proposal-170/374-i2pd-live-els2-qualification.md).

Classification: external interoperability + capability evidence.

This plan **did not pass**. No ELS2 direction was executed. It is recorded as
blocked, with the freeze work completed and the blocker named precisely.

## What was completed

Everything the plan's "Reference freeze" section requires **before** execution,
plus the environment verification that de-risks a future pass. The full record
is [`tests/integration/els2/reference-freeze.md`](../../../tests/integration/els2/reference-freeze.md),
which is durable and reusable by Plan 375.

| Freeze item (plan §23–37) | Status | Result |
|---|---|---|
| the pin builds unmodified | pass | i2pd 2.61.0 built clean from `635b013a…`; nothing patched, vendored, or copied in |
| record i2pd version and dependency versions | pass | 2.61.0 (0.9.70); Boost 1.83.0; stock `Makefile`, `cmake`/`g++`/`openssl`/`boost`/`zlib` |
| re-read the exact ELS2/type-5 consumer/publisher/config surfaces | pass | see below |
| record the current i2pr SHA and Cargo.lock hash | pass | `75fd691cfe0e0473ce79f89fa16d66a5e7453649`; `Cargo.lock` SHA-256 `ab1963730134…b9004a` |
| ADR 0032 (Proposal 170) transcript policy unchanged | pass | untouched by this plan |
| controlled topology with no public-network dependency | **pass — verified running** | the Plan 303/306 controlled mesh executed green here |

The mesh verification is the important one, because it removes the most likely
cause of a block:

```text
bash tests/integration/floodfill/run-i2pd.sh
Plan 278 lane passed; sanitized evidence: target/interop/m12-floodfill-evidence
```

Three stock i2pd processes and a controlled i2pr floodfill formed a real
multi-process loopback SSU2 mesh with no reseed. That is the R/F/D topology the
plan names. **The external lane is not blocked by the environment.**

## Source proof (plan §99–101)

Required before a mode may be `reference-not-applicable`. All three modes are
implemented by the reference, so **none may be marked not-applicable**:

| Mode | Stock i2pd 2.61.0 support | Evidence |
|---|---|---|
| no-auth | implemented | `ENCRYPTED_LEASESET_AUTH_TYPE_NONE = 0`, `LeaseSet.h:291` |
| PSK | implemented | `AUTH_TYPE_PSK = 2` → `layer1Flags |= 0x03`, `LeaseSet.cpp:991`; keys `i2cp.leaseSetPrivKey`, `i2cp.leaseSetClient.psk.nnn` |
| DH/X25519 | implemented | `AUTH_TYPE_DH = 1` → `layer1Flags |= 0x01`, `LeaseSet.cpp:990`; type bounded `NONE..PSK` at `Destination.cpp:82` |

Both directions are reference-feasible:

- **A (i2pr → i2pd)** — `Destination.cpp:778`
  `RequestDestinationWithEncryptedLeaseSet` derives the storage key from the
  blinded key and issues a real `RequestLeaseSet`; `:1276` turns the result into
  a stream. `SAM.cpp:1369` documents the b33 addressing rule.
- **B (i2pd → i2pr)** — `Destination.cpp:1548` builds the outer record via
  `LocalEncryptedLeaseSet2`; `I2CP.cpp:867` parses it back.

This is a **material change to the plan's sizing**, discovered by the freeze the
plan itself mandated: Plan 374's matrix is the full one, not a trimmed one.

## The blocker

Named, specific, and not environmental.

The repository has no ELS2 live driver. It has the Plan 303/306 controlled
floodfill mesh (type 1/3/7 only), Plan 350/351's i2pr-internal type-5 path, and
Plan 346's crypto-boundary transcript check. **None of those is a live ELS2
row**, and the plan forbids satisfying one by inserting a decoded LeaseSet into
a consumer, by sharing an in-process NetDB between R and F, by invoking the
resolver with test bytes, or by modifying i2pd.

So the lane must be **written**, not extended:

1. an R/F/D ELS2 mesh driver adding type-5 publication and blinded-key lookup to
   the controlled topology, R and F as distinct processes;
2. an i2pd service/client driver built only from stock configuration and public
   output — create an encrypted service, obtain its b33, set the lookup secret
   / PSK / DH material with stock syntax, expose a loopback application;
3. the matrix: 2 directions × 3 auth modes × 3 credential scenarios, the nine
   negative rows, the persistence and daily-rotation rows;
4. a bounded machine-readable artifact plus a checker that fails when a
   mandatory row lacks `store → lookup → decrypt/validate → application`
   provenance.

A partial lane recorded as evidence would be worse than an honest block. The
repo's external-lane guardrails forbid early-return-success, and a half-run
matrix presented as a pass is precisely the failure the evidence checkers exist
to prevent.

## Findings by severity

- **critical / high: none.** No product defect was found; no product code was
  changed by this plan.
- **medium, planning**: Plan 374's registered matrix size rested on an
  unverified assumption that some auth modes might be
  `reference-not-applicable`. The freeze disproved it. Any future re-sizing of
  this lane must use the full matrix, or re-derive why a mode is inapplicable.
- **low (recorded, not fixed here)**: `scripts/interop/fetch-ssu2-reference.sh`
  cannot fetch the Java reference at its pin — it clones a default branch and
  then fails its own pin check, so a correctly pinned override is required. The
  script's i2pd path supports an override and works. Fixing the Java path is an
  interop-tooling change outside this plan's scope.

## What is explicitly **not** claimed

- No ELS2 interoperability, in either direction, with any reference.
- No live type-5 `DatabaseStore`, blinded-key `DatabaseLookup`, decrypt, inner
  LS2 validation, or application payload across routers.
- Type 5 remains `advertised = false`. `specs/support.toml` is unchanged.

Plan 346's crypto-boundary cross-verification remains valid and remains **not**
interoperability. That distinction is unchanged by this record.

## Roadmap disposition and unblock audit

- **Roadmap disposition: blocked.** Plan 374 stays blocked, now on a named
  work gap rather than on the missing provider/tunnel-path diagnoses of
  historical Plan 347.
- **Unblock audit, executed per `plans/README.md`:**

| Plan | Other hard dependencies | All closed? | Disposition |
|---|---|---|---|
| 375 | Proposal 170/346, /350, /351 all passed | yes | **unblocked by this pass** — it has its own blocker, recorded separately |
| 377 | Plan 374 **not passed**, Plan 375 not passed | no | stays blocked |
| 378 | Plan 376 passed; Plan 377 not passed | no | stays blocked on 377 alone |

No corrective pass is registered: no defect was found, so there is nothing to
correct. The blocker is unwritten work, which is what the plan itself is for.

## Limitations

- The source proof is reading at the pinned commit, which is exactly what the
  plan asks for, but it is not execution. It narrows the matrix; it does not
  pass any row.
- The mesh run proves the Plan 278 lane, not an ELS2 row. It is substrate
  evidence and nothing more.
- No claim is made about i2pd behaviour under load, at scale, or against the
  public network.