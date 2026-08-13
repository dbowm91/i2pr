# Plan 102 authority amendment: exploratory-tunnel dependency

## Status

- Date: 2026-08-13.
- Authority: **active amendment to Plan 102**.
- Reason: specification review during Plans 103-106 planning identified a cross-milestone dependency that the first Plan 102 draft described too optimistically.

## Correction

Current I2P RouterInfo `DatabaseLookup` operation uses an outbound exploratory tunnel and requests the response through an inbound exploratory tunnel. Exploratory tunnels are Milestone 5 scope.

Therefore this amendment supersedes any Plan 102 wording that implies Plan 106 can complete a standards-conformant live RouterInfo lookup merely by re-entering NTCP2 or another direct router transport.

The authoritative sequence is:

```text
Plan 103  RouterInfo validation + bounded local NetDB
   -> Plan 104  persistent cache + SU3 reseed trust/ingestion
   -> Plan 105  transport-neutral lookup/store/publication state machines
   -> Plan 106  daemon/bootstrap integration
   -> Milestone 5 exploratory tunnel substrate
   -> return to Milestone 4B external acceptance
```

Plan 106 closes the local/bootstrap implementation phase, not the complete original Milestone 4 exit criteria.

## Milestone 4A expected state after Plan 106

```text
RouterInfo validation          = implemented
local NetDB                    = implemented
persistent cache               = implemented
SU3 reseed verification        = implemented
reseed ingestion               = implemented
lookup/publication state       = implemented
NetDB daemon integration       = implemented
live RouterInfo lookup         = blocked-on-milestone5-exploratory-tunnels
live publication verification = blocked-on-milestone5-and-qualified-transport
normal daemon NTCP2            = disabled/unenableable
next implementation            = Milestone 5 exploratory tunnels
```

This is a successful Milestone 4A implementation state but is not `milestone4-passed`.

## Milestone 4B external acceptance

After Milestone 5 supplies exploratory inbound/outbound paths and a router transport is deliberately qualified, return to the Milestone 4 acceptance checkpoint. Full acceptance requires a real RouterInfo lookup through the exploratory path, a valid matching response, normal Plan 103 validation/insertion/persistence, and local RouterInfo publication with independent verification.

A direct `DatabaseLookup` over NTCP2 is not accepted as a substitute for the standard exploratory-tunnel path.

Before normal NTCP2 activation/public I2P use, reconcile deferred Plan 079 with the retained Plan 099/100 result. Any new transport work must be narrow and defect-driven; do not create another generic interoperability harness sequence.

## Authority precedence

When documents disagree, use this order for the current line of work:

```text
Plan 102 amendment (this file)
 -> active child Plan 103/104/105/106
 -> Plan 102 parent roadmap
 -> Plans 099-101 for retained Milestone 3/NTCP2 state
 -> plans/000-mvp-roadmap.md milestone descriptions
 -> older historical Milestone 3 active blocks
```

The next executable implementation remains **Plan 103**.
