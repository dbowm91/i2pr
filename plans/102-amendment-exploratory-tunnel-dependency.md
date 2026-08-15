# Plan 102 authority amendment: exploratory-tunnel dependency

## Status

- Original amendment date: 2026-08-13.
- Updated: 2026-08-15 after the Plan 111 post-closure reference-backed audit.
- Authority: **closed amendment to Plan 102**.
- Closure record: [`plans/102-amendment-status.md`](102-amendment-status.md).
- Reason: the dependency correction is implemented and propagated through the
  closed Plans 107–113 sequence. The next qualified external-delivery
  checkpoint is now unblocked; full Milestone 4B acceptance remains gated on
  real independent-router evidence.

## Original dependency correction

Current I2P RouterInfo `DatabaseLookup` operation uses an outbound exploratory tunnel and requests the response through an inbound exploratory tunnel. Exploratory tunnels are Milestone 5 scope.

Therefore this amendment supersedes any Plan 102 wording that implies Plan 106 can complete a standards-conformant live RouterInfo lookup merely by re-entering NTCP2 or another direct router transport.

A direct `DatabaseLookup` over NTCP2 is not accepted as a substitute for the standard exploratory-tunnel path.

## Current authoritative sequence

Plans 103-107 landed the local NetDB and exploratory-tunnel substrate. Plan 108 landed the runtime-neutral short-build architecture. Plans 109-111 iteratively corrected the short-record, Noise, and multi-record cryptographic core.

A post-Plan-111 audit against the current final I2P specification plus current Java I2P and i2pd source confirmed that the Plan 111 core should be retained but found several remaining pre-delivery surface defects and one inbound standards/reference discrepancy.

The authoritative sequence is now:

```text
Plan 103  RouterInfo validation + bounded local NetDB                 [closed]
   -> Plan 104  persistent cache + SU3 reseed trust/ingestion         [closed]
   -> Plan 105  transport-neutral lookup/store/publication states     [closed]
   -> Plan 106  daemon/bootstrap integration                          [closed]
   -> Plan 107  exploratory tunnel substrate                          [closed]
   -> Plan 108  local short-build architecture                        [superseded]
   -> Plan 109  single-record short-build correction                  [superseded]
   -> Plan 110  multi-record construction/preprocessing               [superseded]
   -> Plan 111  core short-build cryptographic correction             [landed; amended]
-> Plan 112  outbound short-build pre-delivery closure             [CLOSED]
   -> Plan 113  inbound specification/reference reconciliation        [CLOSED; reference-compatible]
      -> narrow qualified external-delivery checkpoint                [UNBLOCKED; NEXT]
   -> full exploratory inbound/outbound acceptance
   -> return to Milestone 4B external NetDB acceptance
```

Current corrective authority:

- `plans/111-post-closure-audit-amendment.md`
- `plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`
- `plans/112-outbound-short-build-pre-delivery-closure.md`
- `plans/112-handoff.md`
- `plans/113-inbound-short-build-spec-reference-reconciliation.md`

Historical corrective context:

- `plans/108-conformance-amendment.md`
- `plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`
- `plans/109-short-build-record-and-noise-conformance-correction.md`
- `plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md`
- `plans/111-short-build-final-local-conformance-correction.md`
- `plans/111-status.md`

## Plan 111 post-closure result

Retain these Plan 111 corrections:

```text
Noise null-prologue MixHash          = corrected
request es single-HKDF split         = corrected
record-slot nonce/IV byte 4          = corrected
OBEP garlic reply tag                = corrected-to-8-bytes
per-hop receive/next tunnel IDs      = explicit
hop responder role                   = decoded-from-authenticated-plaintext
frozen cryptographic vectors         = retained
```

Do not reopen those primitives during Plan 112 absent concrete new evidence.

The Plan 111 closure status is amended because these outer-surface issues remain:

```text
request plaintext random padding     = missing
reply plaintext random padding       = missing
direction/role topology validation   = missing
HopCryptoContext ephemeral accessor  = wrong envelope slice
STBM/OTBRM action-event contract     = count-prefix semantics inconsistent
fixed-vector generator provenance    = stale/non-reproducible
production inbound gate              = not explicit/coherent
```

These are deterministic local issues. They do not justify reopening the Milestone 3 transport-validation program.

## Research-backed inbound correction

The final ECIES tunnel-creation specification says the creator ephemeral public key is included in an inbound plaintext record, but the 154-byte layout does not define a concrete field location or option encoding.

Current independent implementations were inspected at pinned 2026-08-14 commits:

```text
Java I2P master = 498488b0d01d9f59efe906424e56ff5e25f58a4d
i2pd openssl    = dfcb8a8043c0c689e5681c5ae5da89df5643347e
```

Both implement the inbound originator fake as:

```text
creator truncated hash16 || fresh X25519 pub32 || remainder
```

and neither exposes a separate creator-ephemeral field in the real 154-byte short request constructor.

Therefore:

- Plan 112 must make production inbound construction fail closed with an intentional typed error;
- Plan 112 must not invent an offset in request padding;
- Plan 113 owns the discrepancy and may close either reference-aligned or fail-closed;
- Plan 113 does not block the outbound-only external-delivery checkpoint after Plan 112.

## Current Milestone 4A / Milestone 5 dependency state

```text
RouterInfo validation                 = implemented
local NetDB                           = implemented
persistent cache                      = implemented
SU3 reseed verification               = implemented
reseed ingestion                      = implemented
lookup/publication state              = implemented
NetDB daemon integration              = implemented
exploratory tunnel substrate          = implemented
short-build architecture              = implemented
short-build cryptographic core        = Plan111-corrected
outbound short-build pre-delivery      = passed-outbound-pre-delivery-closure
inbound short-build                    = passed-reference-compatible-discrepancy
outbound independent delivery          = eligible-for-qualified-checkpoint
inbound independent delivery           = eligible-for-qualified-checkpoint
live exploratory inbound/outbound pair = blocked-on-qualified-delivery
live RouterInfo lookup                 = blocked-on-live-exploratory-tunnels
live publication verification         = blocked-on-live-exploratory-tunnels-and-qualified-transport
normal daemon NTCP2                   = disabled/unenableable
ntcp2                                 = experimental/non-advertised
```

This remains a successful local foundation state, but it is not `milestone4-passed` and it is not yet ready for the full exploratory-tunnel acceptance checkpoint.

## Plan 112 boundary (closed)

Plan 112 is deliberately local and deterministic. It owns only:

- random request plaintext padding using injected CSPRNG;
- random reply plaintext padding using injected CSPRNG;
- outbound and inbound structural role-topology validation;
- Plan 113 reference-compatible inbound policy with explicit creator identity,
  one originator fake, and integrity verification;
- correcting/removing the bad ephemeral-public accessor;
- exact count-prefixed STBM/OTBRM action/event contracts;
- a small Rust-only reproducible reference-vector artifact;
- status/support corrections necessary to stop stale pre-delivery claims.

Plan 112 must not require:

- normal-daemon NTCP2 activation;
- SSU2;
- public I2P access;
- Java I2P/i2pd/Emissary live validation as a closure gate;
- privileged network namespaces;
- Docker/Multipass/rootless isolation machinery;
- new Python interoperability frameworks;
- generic I2NP dispatch.

Plan 112 passed. The smallest available qualified external-delivery lane may
now be selected for a real mixed-router tunnel-build attempt. This is a
checkpoint, not a claim of interoperability or a normal-daemon transport
activation.

## Plan 113 boundary (closed)

Plan 113 owns only the inbound specification/reference discrepancy and resulting local inbound policy.

It must not invent a private field from random request-padding bytes.

Acceptable closure states are:

1. concrete authoritative encoding found and implemented;
2. current deployed-reference-compatible behavior explicitly documented and implemented without overstating strict final-spec conformance;
3. discrepancy unresolved and inbound remains disabled.

Plan 113 selected the deployed-reference-compatible policy and documented the
remaining final-spec prose discrepancy. It does not regress outbound progress
and does not claim strict final-spec conformance for that one semantic.

## Milestone 4B external acceptance

After Milestone 5 supplies usable exploratory inbound/outbound build paths and a router transport is deliberately qualified, return to the Milestone 4 acceptance checkpoint.

Full acceptance requires:

1. real exploratory tunnel build(s) against an independent router implementation sufficient to provide the outbound and inbound exploratory paths;
2. a real RouterInfo lookup through the exploratory path;
3. a valid matching response;
4. normal Plan 103 validation/insertion/persistence;
5. local RouterInfo publication with independent verification.

An outbound-only Plan 112 successor checkpoint is useful product evidence, but it is not by itself full Milestone 4B acceptance because NetDB lookup also needs an inbound reply path.

Do not substitute deterministic simulation for this later external acceptance, but do not make external acceptance a gate for Plan 112 or Plan 113 local work.

## Transport boundary

Before normal NTCP2 activation/public I2P use, reconcile deferred Plan 079 with the retained Plan 099/100/101 state.

Any new transport work must be narrow and product-driven. A Plan 112-correct outbound STBM payload will be the concrete first consumer. Do not create another generic interoperability harness sequence.

If a transport other than NTCP2 becomes the smallest viable lane for the first independent tunnel-build check, it may be evaluated separately without rewriting Plans 112/113.

## Authority precedence

When documents disagree, use this order for the current line of work:

```text
Plan 102 amendment (this file)
 -> Plan 111 post-closure audit amendment
 -> Plans 112-113 corrective roadmap
 -> Plan 112 plan-of-record + handoff
 -> Plan 113 plan-of-record for inbound semantics
 -> historical Plan 111 status/plan
 -> historical Plan 109/110 corrective roadmap/status
 -> Plan 102 parent roadmap
 -> Plans 099-101 for retained Milestone 3/NTCP2 state
 -> plans/000-mvp-roadmap.md milestone descriptions
 -> older historical Milestone 3 active blocks
```

The next executable action is the narrow qualified external-delivery
checkpoint after Plans 112 and 113. The amendment is closed; the Plan 102
parent remains the active Milestone 4 authority for the subsequent Milestone
4B acceptance decision.

## Closure and future-plan unblock audit

The amendment's dependency correction is complete. Plans 103–107 delivered
the local NetDB and exploratory-tunnel substrate; Plans 108–111 corrected the
short-build implementation; Plan 112 closed the outbound pre-delivery gate;
and Plan 113 closed the inbound specification/reference reconciliation.

The following status changes are authoritative:

```text
plan_102_amendment               = closed
plans_112_113_corrective_roadmap = closed
qualified_external_delivery     = unblocked-next-checkpoint
milestone4b_external_acceptance = still-blocked-on-qualified-evidence
normal_daemon_ntcp2             = disabled-and-unenableable
ntcp2                           = experimental-non-advertised
```

No older deferred NTCP2 development plan is reactivated by this closure.
Plan 079 remains deferred to the separate pre-normal-activation/public-
network checkpoint, and the qualified delivery checkpoint must not be
expanded into a generic interoperability-harness restart.
