# Plan 102 authority amendment: exploratory-tunnel dependency

## Status

- Original amendment date: 2026-08-13.
- Updated: 2026-08-15 after Plan 111 post-Plan-110 conformance review.
- Authority: **active amendment to Plan 102**.
- Reason: standards-conformant external NetDB acceptance depends on exploratory tunnels, and the current short-build implementation requires one final bounded local protocol-conformance correction before live tunnel construction can proceed.

## Original dependency correction

Current I2P RouterInfo `DatabaseLookup` operation uses an outbound exploratory tunnel and requests the response through an inbound exploratory tunnel. Exploratory tunnels are Milestone 5 scope.

Therefore this amendment supersedes any Plan 102 wording that implies Plan 106 can complete a standards-conformant live RouterInfo lookup merely by re-entering NTCP2 or another direct router transport.

A direct `DatabaseLookup` over NTCP2 is not accepted as a substitute for the standard exploratory-tunnel path.

## Current authoritative sequence

Plans 103–107 landed the local NetDB and exploratory-tunnel substrate. Plan 108 landed the runtime-neutral short-build architecture. Plans 109 and 110 then corrected most of the short-record, Noise, and multi-record construction surface, but a post-Plan-110 audit found a remaining bounded set of deterministic protocol defects.

The authoritative sequence is now:

```text
Plan 103  RouterInfo validation + bounded local NetDB                 [closed]
   -> Plan 104  persistent cache + SU3 reseed trust/ingestion         [closed]
   -> Plan 105  transport-neutral lookup/store/publication states     [closed]
   -> Plan 106  daemon/bootstrap integration                          [closed]
   -> Plan 107  exploratory tunnel substrate                          [closed]
   -> Plan 108  local short-build architecture                        [landed; superseded]
   -> Plan 109  single-record short-build correction                  [landed; conformance reopened]
   -> Plan 110  multi-record construction/preprocessing               [landed; conformance reopened]
   -> Plan 111  final local short-build conformance correction        [next executable]
   -> narrow qualified external-delivery checkpoint
   -> return to Milestone 4B external acceptance
```

Current corrective authority:

- `plans/111-short-build-final-local-conformance-correction.md`
- `plans/111-handoff.md`

Historical corrective context:

- `plans/108-conformance-amendment.md`
- `plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`
- `plans/109-short-build-record-and-noise-conformance-correction.md`
- `plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md`

## Why Plan 111 precedes external delivery

The current implementation has useful structure but still requires these local corrections before its STBM bytes can be treated as standards-conformant evidence:

```text
Noise null-prologue MixHash          = missing
request es AEAD-key derivation       = incorrect-second-HKDF
record-slot nonce/IV                 = byte11-instead-of-byte4
OBEP garlic reply tag                = 16-bytes-instead-of-8
inbound creator ephemeral plaintext  = missing
per-hop receive/next tunnel IDs      = missing-or-synthesized
hop responder role                   = flattened-to-participant
independent fixed-vector oracle      = insufficient
```

These are deterministic local defects. They do not justify reopening the Milestone 3 transport-validation program.

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
short-build local conformance         = reopened-on-plan111
live exploratory tunnel               = blocked-on-plan111-and-qualified-delivery
live RouterInfo lookup                = blocked-on-live-exploratory-tunnel
live publication verification         = blocked-on-live-exploratory-tunnel-and-qualified-transport
normal daemon NTCP2                   = disabled/unenableable
ntcp2                                 = experimental/non-advertised
```

This remains a successful local foundation state, but it is not `milestone4-passed` and the short-build path must not yet be called locally conformant.

## Plan 111 corrective boundary

Plan 111 is deliberately local and deterministic.

It owns only:

- literal Noise-N null-prologue and single-HKDF request `es` correction;
- record number at nonce/IV byte 4;
- 8-byte OBEP garlic reply tag representation;
- explicit inbound creator-ephemeral plaintext semantics;
- explicit independent per-hop receive/next tunnel IDs;
- role-aware responder KDF behavior;
- independently frozen cryptographic intermediate vectors;
- status/support corrections necessary to stop stale conformance claims.

It must not require:

- normal-daemon NTCP2 activation;
- SSU2;
- public I2P access;
- Java I2P/i2pd/Emissary live validation as a closure gate;
- privileged network namespaces;
- Docker/Multipass/rootless isolation machinery;
- new Python interoperability frameworks;
- generic I2NP dispatch.

One narrow exception is allowed: if the final public specification remains ambiguous about the exact placement/order of the inbound creator ephemeral public key inside the 154-byte plaintext's variable region, the implementer must inspect a current independent reference-router implementation to disambiguate that wire layout. This is source inspection only, not live-router execution.

Only after Plan 111 passes may the project select the smallest available external delivery lane for one real mixed-router tunnel-build attempt.

## Milestone 4B external acceptance

After Milestone 5 supplies a locally conformant exploratory inbound/outbound build path and a router transport is deliberately qualified, return to the Milestone 4 acceptance checkpoint.

Full acceptance requires:

1. a real exploratory tunnel build against an independent router implementation;
2. a real RouterInfo lookup through the exploratory path;
3. a valid matching response;
4. normal Plan 103 validation/insertion/persistence;
5. local RouterInfo publication with independent verification.

Do not substitute deterministic simulation for this later external acceptance, but do not make external acceptance a gate for Plan 111's local protocol correction.

## Transport boundary

Before normal NTCP2 activation/public I2P use, reconcile deferred Plan 079 with the retained Plan 099/100/101 state.

Any new transport work must be narrow and product-driven. A Plan 111-correct STBM payload will be the concrete consumer. Do not create another generic interoperability harness sequence.

If a transport other than NTCP2 becomes the smallest viable lane for the first independent tunnel-build check, it may be evaluated separately without rewriting Plan 111.

## Authority precedence

When documents disagree, use this order for the current line of work:

```text
Plan 102 amendment (this file)
 -> Plan 111 plan-of-record
 -> Plan 111 handoff
 -> Plan 109/110 status amendments
 -> historical Plan 109/110 corrective roadmap
 -> Plan 102 parent roadmap
 -> Plans 099-101 for retained Milestone 3/NTCP2 state
 -> plans/000-mvp-roadmap.md milestone descriptions
 -> older historical Milestone 3 active blocks
```

The next executable implementation is **Plan 111**.
