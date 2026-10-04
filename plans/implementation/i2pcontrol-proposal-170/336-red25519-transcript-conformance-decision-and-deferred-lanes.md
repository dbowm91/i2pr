# Plan 336 — Red25519 transcript conformance decision and deferred reference lanes

Status: **passed-red25519-transcript-conformance-decision-spec-first**

Closure record: `plans/closure/i2pcontrol-proposal-170/336-closure.md`.

Classification: architectural decision + external-evidence corrective. No production crypto code in
this plan.

Hard dependencies: Plan 330 closed; Plan 331 closure record
(`plans/closure/i2pcontrol-proposal-170/331-status.md`) for the frozen evidence.

## Why this plan exists

Plan 331 qualified the Plan 330 Red25519 implementation against the specification's own published
vector corpus and against the pinned i2pd reference, and stopped short of passing for two named
reasons:

1. **Reference signature-transcript divergence (high, unresolved).** i2pr implements the
   specification's transcript, `SHA-512("I2P_Red25519H(x)" ‖ prefix1 ‖ prefix2 ‖ len_u16le(m) ‖ m)`,
   and all ten official vectors verify under it while none verify under a bare-SHA-512 transcript.
   The pinned i2pd revision, and i2pd master as of this registration, sign type 11 with
   `SHA-512(T ‖ A ‖ M)` and verify with the plain Ed25519 equation; the string
   `I2P_Red25519H` appears nowhere in the pinned Java I2P tree. Blinding, alpha derivation, and the
   DHT storage key agree across all three, so only the signature transcript diverges.
2. **Reference oracle lanes unexecuted (blocking evidence gap).** The post-freeze Emissary
   black-box differential, and an executed Java I2P differential, could not run on the Plan 331 host.

Because the divergence is a conformance decision with interoperability consequences — not an i2pr
defect — it needs an owner decision, not another agent pass.

## Decision required

The repository owner must choose, and record as an ADR amendment, one of:

- **Spec-first, non-interoperable (current i2pr state).** Keep the specification's transcript. Type-5
  encrypted LeaseSets published by i2pr are unverifiable by Java I2P and i2pd today, so the branch
  delivers a self-consistent, specification-conformant primitive and a *deferred* ELS2 capability,
  and the ELS2 work (Plans 332–334) proceeds only with that limitation stated in
  `specs/CONFORMANCE.md` and never as a supported interop capability.
- **Reference-compatible, non-conformant.** Adopt the bare-SHA-512 transcript that both references
  use, record a documented deviation from the specification text *and* from its published vectors,
  and accept that the official vector corpus cannot be used as a conformance gate. This requires an
  explicit ADR deviation record; it must not be achieved by quietly relaxing the vectors.
- **Dual mode.** Carry the transcript choice as an explicit, operator-visible mode with no
  autodetection and no silent fallback, plus a migration story. Cost: two signature formats on the
  same key type, which the specification does not describe.

No option may be adopted by editing the implementation to match a reference without recording the
deviation.

## Required work

1. Record the decision as an ADR 0005/0028 amendment, with the executed evidence from the Plan 331
   freeze record and the official-vector result.
2. Update `specs/CONFORMANCE.md` and `specs/support.toml` to state exactly what Red25519 support
   means under the chosen option, including that DatabaseStore type 5 remains unimplemented.
3. Execute the deferred lanes on a host that can run them, and record results as evidence:
   - **Java I2P** (needs a buildable I2P 2.13.0/head checkout or a controlled router): alpha with
     and without a lookup secret, type-7 and type-11 public/private blinding, storage hash, signature
     acceptance in both directions, UTC day rollover.
   - **Emissary black-box** (needs a running Emissary instance and network reachability): alpha,
     blinded keys, storage hash, b33, cross-signature verification. Emissary Red25519/ELS2 source
     stays unread (Plan 329 §5).
   - **i2pd live** (needs two routers or a controlled pair): publication/lookup of a type-5 record.
4. Re-run the Plan 331 qualification after any implementation change; the freeze is invalidated by
   any production Red25519 edit.
5. Only after the decision and the deferred lanes: unblock Plan 332, and keep Plans 333–335 behind
   it in order.

## Evidence required for closure

- the recorded decision with its ADR reference;
- executed Java and Emissary differential results, or an explicit owner deferral of each lane with
  the host capability named;
- `specs/CONFORMANCE.md` and `specs/support.toml` updated to the truth of the chosen option;
- the Plan 331 freeze record re-referenced, with any invalidated runs named.

## Stop conditions

- If the owner selects reference-compatible mode, the official-vector corpus must be re-classified
  as a documented deviation rather than deleted, and the deviation must be visible in the support
  inventory.
- If neither Java nor Emissary can be executed in the available environment, this plan closes as
  retained/blocked again with the capability gap named. It must not close as passed.
- If the decision makes the ELS2 branch non-interoperable by construction, Plans 332–335 stay
  blocked and the roadmap records the branch as specification-conformant but unclaimed.
