# Plan 102 authority amendment: exploratory-tunnel dependency

## Status

- Original amendment date: 2026-08-13.
- Updated: 2026-08-15 after Plan 108 protocol-conformance review.
- Authority: **active amendment to Plan 102**.
- Reason: standards-conformant external NetDB acceptance depends on exploratory tunnels, and the first Plan 108 tunnel-build implementation requires a bounded protocol-conformance correction before live tunnel construction can proceed.

## Original dependency correction

Current I2P RouterInfo `DatabaseLookup` operation uses an outbound exploratory tunnel and requests the response through an inbound exploratory tunnel. Exploratory tunnels are Milestone 5 scope.

Therefore this amendment supersedes any Plan 102 wording that implies Plan 106 can complete a standards-conformant live RouterInfo lookup merely by re-entering NTCP2 or another direct router transport.

A direct `DatabaseLookup` over NTCP2 is not accepted as a substitute for the standard exploratory-tunnel path.

## Current authoritative sequence

Plans 103–107 landed the local NetDB and exploratory-tunnel substrate. Plan 108 then landed a useful runtime-neutral short-build architecture, but its initial short-record wire/cryptographic semantics were subsequently found to diverge from the current official I2P Tunnel Creation Specification.

The authoritative sequence is now:

```text
Plan 103  RouterInfo validation + bounded local NetDB                 [closed]
   -> Plan 104  persistent cache + SU3 reseed trust/ingestion         [closed]
   -> Plan 105  transport-neutral lookup/store/publication states     [closed]
   -> Plan 106  daemon/bootstrap integration                          [closed]
   -> Plan 107  exploratory tunnel substrate                          [closed]
   -> Plan 108  local short-build architecture                        [landed; conformance reopened]
   -> Plan 109  exact short-record + Noise-N/KDF correction           [next executable]
   -> Plan 110  multi-record preprocessing + local conformance close  [blocked on 109]
   -> narrow qualified external-delivery checkpoint
   -> return to Milestone 4B external acceptance
```

See:

- `plans/108-conformance-amendment.md`
- `plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`
- `plans/109-short-build-record-and-noise-conformance-correction.md`
- `plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md`

## Current Milestone 4A / Milestone 5 dependency state

```text
RouterInfo validation               = implemented
local NetDB                         = implemented
persistent cache                    = implemented
SU3 reseed verification             = implemented
reseed ingestion                    = implemented
lookup/publication state            = implemented
NetDB daemon integration            = implemented
exploratory tunnel substrate        = implemented
short-build local architecture      = implemented-needs-protocol-correction
short-build record/Noise conformance = blocked-on-plan109
short-build multi-record conformance = blocked-on-plan110
live exploratory tunnel             = blocked-on-plan109-plan110-and-qualified-delivery
live RouterInfo lookup              = blocked-on-live-exploratory-tunnel
live publication verification       = blocked-on-live-exploratory-tunnel-and-qualified-transport
normal daemon NTCP2                 = disabled/unenableable
ntcp2                               = experimental/non-advertised
```

This remains a successful local foundation state, but it is not `milestone4-passed` and is not yet a locally conformant tunnel-build state.

## Plan 109–110 corrective boundary

Plans 109 and 110 are deliberately local and deterministic.

They must not require:

- normal-daemon NTCP2 activation;
- SSU2;
- public I2P access;
- Java I2P/i2pd/Emissary live validation as a closure gate;
- privileged network namespaces;
- Docker/Multipass/rootless isolation machinery;
- new Python interoperability frameworks.

Plan 109 corrects exact single-record wire and Noise-N semantics. Plan 110 corrects randomized record slots, fake records, raw-ChaCha20 preprocessing/postprocessing, exact one-byte-count STBM/OTBRM payload framing, and independent multi-hop local conformance evidence.

Only after Plan 110 passes may the project select the smallest available external delivery lane for one real mixed-router tunnel-build attempt.

## Milestone 4B external acceptance

After Milestone 5 supplies a locally conformant exploratory inbound/outbound build path and a router transport is deliberately qualified, return to the Milestone 4 acceptance checkpoint.

Full acceptance requires:

1. a real exploratory tunnel build against an independent router implementation;
2. a real RouterInfo lookup through the exploratory path;
3. a valid matching response;
4. normal Plan 103 validation/insertion/persistence;
5. local RouterInfo publication with independent verification.

Do not substitute deterministic simulation for this later external acceptance, but do not make external acceptance a gate for the local Plan 109/110 protocol corrections.

## Transport boundary

Before normal NTCP2 activation/public I2P use, reconcile deferred Plan 079 with the retained Plan 099/100/101 state.

Any new transport work must be narrow and product-driven. The existence of a correct STBM payload after Plan 110 will be the concrete consumer. Do not create another generic interoperability harness sequence.

If a transport other than NTCP2 becomes the smallest viable lane for the first independent tunnel-build check, it may be evaluated separately without rewriting Plans 109/110.

## Authority precedence

When documents disagree, use this order for the current line of work:

```text
Plan 102 amendment (this file)
 -> Plans 109-110 corrective roadmap
 -> Plan 108 conformance amendment
 -> active child Plan 109 / Plan 110
 -> Plan 102 parent roadmap
 -> Plans 099-101 for retained Milestone 3/NTCP2 state
 -> plans/000-mvp-roadmap.md milestone descriptions
 -> older historical Milestone 3 active blocks
```

The next executable implementation is **Plan 109**.