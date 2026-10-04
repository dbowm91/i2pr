# Plan 328 — Live external differential and full Proposal 170 conformance gate

Status: **blocked-prop170-full-conformance-gate-awaiting-322-326-327**

Classification: external evidence + final closure gate.

Hard dependencies: Plans 322, 326, and 327 closed; their transitive dependencies include Plans 319–325.

## Objective

Execute, not merely statically analyze, the canonical Proposal 170 corpus against i2pr and independent reference implementations, then decide whether i2pr may claim full-proposal-conformant.

This plan does not implement a frontend.

## Spec re-freeze

At execution start:
- fetch current Proposal 170 and base I2PControl docs;
- compare with the Plan 320 pin;
- record exact hashes/dates;
- if normative wire semantics changed, stop and register a spec-reconciliation corrective before qualification.

## Reference lanes

Provision reproducible exact-pinned local/controlled targets:

### eggstack/emissary

Use the project-owned mature Proposal 170 implementation as the broadest behavioral oracle. Compare every overlapping method, selector, action, type, option and security edge.

### Java Proposal 170 PR

Build the exact pinned PR 6 head in a controlled Java router environment. Compare canonical capitalized TunnelManager/AddressBook behavior, RouterInfo shapes and the deep LeaseSet modes it actually supports.

The PR is proposed behavior, not normative authority; Proposal text wins on conflicts.

### i2pd

Use an exact pin only for the base/adopted RouterInfo and ClientServicesInfo subset it implements. Absence of new AddressBook/TunnelManager methods is not a failure.

## Corpus

Generate one canonical corpus from the machine-readable Plan 320 inventory:
- Authenticate and auth errors;
- every RouterInfo selector individually and meaningful grouped requests;
- logs.clear side effect;
- AddressBook add/update/delete, subscriptions and every SetConfig key;
- ClientServicesInfo all selectors;
- all seven TunnelManager actions;
- All start/stop/restart;
- every one of twelve tunnel types;
- every applicable canonical option cell;
- all ten EncryptLeaseSet modes;
- DH/PSK auth and lookup secret;
- outproxy provider fields;
- malformed/type/range/duplicate/case/oversize cases;
- restart/persistence/cancellation/security cases.

Sanitize retained evidence: no passwords, tokens, private keys, client-auth secrets, live peer addresses or sensitive timing identities.

## Differential taxonomy

Every mismatch must be one of:
- normative Proposal violation in i2pr — fail;
- normative Proposal violation in reference — record reference divergence;
- permitted implementation-dependent capability difference — document exact basis;
- stricter i2pr security behavior compatible with Proposal — document;
- unresolved ambiguity — stop and register a narrow reconciliation, do not waive.

“No unexplained mismatch” requires actually executed external rows.

## Full-conformance claim gate

The final support claim requires:
- canonical wire contract at default endpoint;
- every Proposal-required selector/action/type/field covered;
- all twelve types with live backends;
- no applicable option accepted inertly;
- exact AddressBook operational behavior;
- signed news owner;
- encrypted/blinded LeaseSet + client-auth modes operational;
- I2P-routed outproxy semantics operational;
- no unresolved high/medium security finding;
- exact-head routine CI green;
- live external differential evidence retained and integrity-checked.

Historical 286–297 evidence remains valid for the capabilities it proved but cannot substitute for the canonical/live requirements above.

## Acceptance criteria

If every gate passes, update registry/roadmap/support docs to full-proposal-conformant with the exact Proposal revision.

If any gate fails, close blocked or retained with the exact row(s); never use “full support” as shorthand for a qualified profile.
