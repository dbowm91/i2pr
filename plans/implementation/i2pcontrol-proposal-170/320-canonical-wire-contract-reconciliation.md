# Plan 320 — Exact Proposal 170 canonical wire-contract reconciliation

Status: **in-progress-prop170-canonical-wire-reconciliation**

Classification: protocol contract corrective. This plan changes the public control-plane contract but does not add deep router capabilities.

Hard dependency: Plan 319 closed.

Plan 319 closed as `passed-prop170-planning-authority-and-global-number-reconciliation` in [`plans/closure/i2pcontrol-proposal-170/319-status.md`](../../closure/i2pcontrol-proposal-170/319-status.md). Current-source check performed 2026-10-04: Proposal 170 remains Open and lists Last Updated 2026-05-20, matching the research baseline. Canonical reference: <https://www.i2p.net/en/proposals/170-i2pcontrol-expansion/>. The base API reference was checked 2026-10-04 and now reports Updated 2026-07-10 at <https://i2p.net/en/docs/api/i2pcontrol/>; its fourteen RouterInfo keys are recorded separately from the 43 Proposal additions. Pinned Java PR 6, Emissary fork, and i2pd commits remain those in [`docs/provenance/proposal-170-manifest.md`](../../../docs/provenance/proposal-170-manifest.md).

## Objective

Replace the current i2pr-normalized Proposal 170 public contract with the exact pinned Proposal 170 wire vocabulary and shapes while retaining typed internal normalization behind an adapter.

The current implementation is useful internally but is not canonical Proposal 170: RouterInfo uses normalized keys such as router.version rather than i2p.router.*, TunnelManager uses lowercase action/name/type plus a nested options object rather than the Proposal’s capitalized top-level parameters, and AddressBook exposes a different thirteen-key SetConfig set.

Full Proposal 170 work cannot build on those spellings as its conformance authority.

## Frozen references

At execution start re-fetch and pin:
- I2P Proposal 170, current revision; research baseline is Open, updated 2026-05-20.
- Base I2PControl API documentation, API version 1 / JSON-RPC 2.0.
- Java Proposal 170 PR 6 pinned implementation.
- project-owned eggstack/emissary Proposal 170 implementation.
- i2pd only for the base/adopted subset it implements.

If Proposal 170 materially changed from the 2026-05-20 baseline, stop and record the diff before changing code.

The base API reference changed after the original planning baseline: its current page identifies itself as updated 2026-07-10 and lists the fourteen base RouterInfo fields frozen independently in `BASE_ROUTER_INFO_FIELDS`. The Proposal 170 43-addition set remains unchanged.

## Required canonical inventory

### RouterInfo

Freeze all 43 Proposal additions with their exact i2p.router.* spellings and declared return types. The inventory must include router news/id/clockskew/info/logs/log-clear, byte/bandwidth/tunnel metrics, participating/exploratory/client tunnel info, v4/v6 status/error/testing values, success/queue values, NetDB lists/info/stats/limits/bans, and six AddressBook getters.

Selector semantics are presence-based as specified: the request value is not required to be null. Preserve base RouterInfo keys separately; do not alias custom normalized names into canonical keys silently.

### AddressBook

Freeze exactly:
- Type, Hostname, Destination, Delete;
- SetSubscriptions;
- SetConfig with the Proposal’s thirteen common keys: subscriptions, update_delay, published_addressbook, router_addressbook, local_addressbook, private_addressbook, proxy_port, proxy_host, should_publish, etags, last_modified, log, theme.

Internal extra configuration such as max-entry ceilings may continue to exist but is not a Proposal wire key.

### TunnelManager

Freeze exact top-level fields and aliases from Proposal 170:
- Name, Action, All, Type, NewName;
- all common create/edit fields;
- all client proxy, client-management, HTTP-filter, server-policy and LeaseSet fields;
- the twelve tunnel types and seven actions;
- exact ranges and enumerations, including the ten EncryptLeaseSet strings;
- OptionalLookup and LeaseSetClientAuths structured values.

Do not hide these inside a nonstandard nested options object on the canonical endpoint.

### ClientServicesInfo

Freeze the exact six selectors and result types from the proposal/adopted i2pd surface.

## Compatibility transition

The canonical form becomes the default and the conformance authority.

If retaining the existing normalized i2pr form is judged necessary for existing tests/tools, isolate it as an explicitly nonstandard compatibility adapter that:
- is never emitted as canonical output;
- cannot make the canonical parser more permissive;
- has separate tests and documentation;
- may be removed later without changing the internal router owners.

Do not support two ambiguous interpretations in one parameter namespace.

## Typed domain and security

Keep internal typed enums/config objects independent of wire spelling.

Build one explicit wire-to-domain translation layer that:
- validates before allocation;
- preserves secret classification;
- rejects duplicate/contradictory aliases;
- caps lists/maps/strings before allocation;
- preserves JSON scalar/list/object types rather than stringifying everything;
- emits exact Proposal response shapes.

RawConfig must never leak secret options even if reference implementations do.

## Required evidence

- machine-readable exact inventories and cardinality assertions;
- fixture for every official Proposal 170 request/response example;
- Java/Emissary differential fixtures for overlapping exact fields;
- case/spelling/alias/duplicate/type/range max+1 tests;
- selector presence tests with empty string, null and benign non-null values where allowed;
- full secret-redaction regression;
- old normalized-shape compatibility tests only if that adapter is retained;
- no router-runtime behavior change beyond routing canonical input to existing owners.

## Acceptance criteria

Plan 320 passes only when the default I2PControl endpoint accepts and emits the exact pinned Proposal 170 wire contract and no documentation calls the prior normalized inventory canonical.

Closure unblocks Plans 321, 323, and 325.
