# Plan 336 status — passed: spec-first Red25519 transcript conformance decision

- Plan: [`plans/implementation/i2pcontrol-proposal-170/336-red25519-transcript-conformance-decision-and-deferred-lanes.md`](../../implementation/i2pcontrol-proposal-170/336-red25519-transcript-conformance-decision-and-deferred-lanes.md)
- Status: **`passed-red25519-transcript-conformance-decision-spec-first`**
- Decision date: 2026-10-04
- Decision: **spec-first** — the implementation follows the specification's Red25519 transcript.
- Evidence: [`specs/references/red25519-qualification-freeze.md`](../../../specs/references/red25519-qualification-freeze.md) §4–§5
- Predecessor: [`331-status.md`](331-status.md)

## Decision

The plan offered three options: spec-first, reference-compatible (bare SHA-512), or dual mode.
**Spec-first is chosen**, on evidence rather than preference:

1. The specification's own published corpus requires it. All ten official Red25519 vectors verify
   under the specified transcript; a control experiment re-verified the same ten signatures with the
   bare-SHA-512 transcript and got 0/10.
2. An independent deployed router agrees byte-for-byte. Emissary `6885a945` reproduces i2pr's alpha,
   blinded keys, DHT storage key, and **signature bytes** exactly. The bare-SHA-512 form has one
   supporter that contradicts the specification's own vectors, against one implementation that
   matches both.
3. Dual mode was rejected: it puts two signature formats on one key type, which the specification
   does not describe, and adds an operator-visible failure mode for a problem that the chosen option
   does not have.
4. Reference-compatible mode was rejected: adopting it would require reclassifying the official
   vector corpus as a documented deviation, which trades a verifiable specification for
   interoperability with two implementations that cannot verify their own published vectors.

## Where the divergence landed

It is an **ecosystem finding against i2pd and Java I2P**, not an i2pr defect:

| Property | i2pr | Official vectors | Emissary `6885a94` | i2pd `2c69414` | Java I2P `93eef5d` |
|---|---|---|---|---|---|
| alpha / blinded keys / storage key | reference | — | identical | identical | not executed |
| signature transcript | specified | 10/10 verify | identical | bare SHA-512 | no domain literal in tree |

Reproduction steps, pins, and the quarantine method are in the freeze record §5. The Java lane is
still source-level evidence, which is named as an open gap below.

## Support inventory

- `specs/CONFORMANCE.md` records that the Red25519 primitive is implemented, qualified against the
  official vectors, byte-compatible with Emissary, and **consumed by nothing**; signature type 11 is
  not accepted on the wire, advertised, or used for any database record yet.
- `specs/support.toml` is unchanged: no Encrypted LeaseSet2 or DatabaseStore type 5 support is
  claimed, because no ELS2 owner exists.
- No advertisement, capability, or version change.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Decision recorded with an ADR reference | `docs/adr/0005-crypto-dependency-selection.md`, "Amendment (Plan 336)" | PASS. Spec-first, with the Emissary byte-identity as the interoperability basis. |
| `specs/CONFORMANCE.md` and `specs/support.toml` state the truth of the chosen option | `specs/CONFORMANCE.md` Red25519 entry; `specs/support.toml` unchanged | PASS. |
| Deferred lanes executed or explicitly deferred with the host capability named | Freeze record §4–§5 | PASS with one named deferral: the Java I2P lane is source-level only, because no runnable Java I2P build was provisioned on this host. It is tracked as a live-lane obligation for Plan 335, which needs a controlled Java router regardless. |
| Official-vector corpus preserved as the conformance gate | `tests/red25519_official_vectors.rs` | PASS. No vector was relaxed, edited, or reclassified. |
| Re-freeze re-referenced | Freeze record §1–§3, unchanged; no production Red25519 edit after `75b91b0` | PASS. |

## Unblock audit

| Plan | Pre-closure state | Decision |
|---|---|---|
| 332 type-5 ELS2 foundation | ready (unblocked by Plan 331) | **Stays ready.** It may now start; its own requirements stand, and no ELS2 capability may be claimed without its own evidence. |
| 333, 334, 335 | blocked behind 332/333/334 | Unchanged. |
| 326 encrypted/blinded LeaseSet | historical blocked | Unchanged; successor reclosure belongs to Plan 335. |

## Limitations

- Java I2P's behavior is established by tree-wide absence of the `I2P_Red25519H` literal, not by an
  executed differential. If a future Java build does implement the specified domain, this record's
  classification must be revisited — the executed Emissary evidence would still stand.
- Choosing spec-first means a type-5 record published by i2pr is unverifiable by i2pd and Java I2P
  today. That is acceptable only because nothing publishes type-5 records yet, and it is a named
  constraint on Plan 332: the ELS2 owner must not claim network interoperability.
- Upstream reporting of the i2pd/Java divergence is outside this repository's scope and is not
  claimed as done.
