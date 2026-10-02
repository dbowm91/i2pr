# Plan 286 — Proposal 170 parallel authority, provenance, and contract foundation

Status: registered-prop170-parallel-authority-provenance-and-contract-foundation

Classification: invariant + infrastructure. No network listener, router mutation, or user-visible Proposal 170 capability is authorized by this plan.

Hard dependencies: none beyond the current repository architecture and accepted ADR 0028. This plan is intentionally independent of M12 and may execute in parallel with router-mainline work.

## Objective

Create the durable contract and provenance foundation for the Proposal 170 workstream so later implementation agents can reuse the project-owned Emissary work without importing Emissary's runtime architecture and can prove exact wire coverage mechanically.

## Frozen references

Use and record exact local copies/hashes or source metadata for:
- Proposal 170 revision 2026-05-20.
- Base I2PControl API version 1 documentation as of the 2026-07-10 documentation update.
- eggstack/emissary master 6885a945d25a5ae61bc68191d27c5816bc3df4c9.
- eepnet/emissary master 9b43484a21d5a1291c4881cdae62a36c527f8c0f.
- Java I2PControl PR 6 head 45bb593000408071dd376b78848fdc246dccd964.
- i2pd openssl head 2d57d3f6783efbfebde6c5b03f29e6c231a84d6b.

If a pin moved before execution, retain these as research provenance and add the newer pin explicitly; do not silently rewrite history.

## Required work

### A. Exact fork provenance manifest

Create a Proposal 170 provenance manifest under docs/provenance or specs/references that enumerates every eggstack/emissary source/test/document path proposed for literal reuse, its blob/commit origin, the applicable notice, and one of:
- reusable protocol/domain/security/test code;
- behavioral reference only;
- Emissary/Yosemite-specific and prohibited from direct import;
- unrelated upstream code.

Verify mechanically that the current eepnet/emissary upstream tree does not contain the fork's i2pcontrol subtree. Do not infer ownership from adjacency: only manifest-listed Proposal 170 files receive the ADR 0028 reuse exception.

### B. New runtime-neutral crate

Add workspace crate i2pr-i2pcontrol with forbid-unsafe and no Tokio/socket/filesystem dependency. It should initially contain only protocol/domain contract modules.

Minimum module split:
- jsonrpc: request ids, single/batch envelope shapes, success/error envelopes, strict named params;
- auth: request/result/error vocabulary only, not token storage or clocks;
- methods: exact method inventory and protected/public classification;
- router_info: exact selector inventory and declared return type metadata;
- client_services: six exact service selectors and result shapes;
- address_book: four book types plus mutation/subscription/config request domain;
- tunnel: seven actions, twelve exact tunnel types, names, lifecycle/status/result domain;
- tunnel_options: exact Proposal option names/types/sensitivity/applicability metadata;
- limits: wire-level string/list/map/request ceilings shared by later daemon code;
- conformance: machine-readable public contract inventory.

Serde/serde_json may be introduced as focused dependencies if the dependency review records purpose, unsafe/transitive impact, and feature set. Keep the crate independent of i2pr-daemon, i2pr-runtime, i2pr-netdb, i2pr-client, and i2pr-service-tunnels.

### C. Exact inventories

Freeze and test:
- Authenticate as the base public method needed to reach protected methods;
- Proposal 170 methods RouterInfo, AddressBook, TunnelManager, ClientServicesInfo;
- all Proposal 170 RouterInfo additions including the address-book selectors;
- ClientServicesInfo keys I2PTunnel, HTTPProxy, SOCKS, SAM, BOB, I2CP;
- AddressBook types private/local/router/published and the complete SetConfig key inventory;
- TunnelManager actions and twelve type spellings;
- exact tunnel option inventory and applicability metadata.

Do not add unrelated base methods. Unknown methods must remain representable as method-not-found later.

### D. Conformance dimensions

Extend specs/CONFORMANCE.md with the Proposal 170 support model:
- wire;
- source;
- runtime effect;
- persistence/atomicity;
- feature isolation;
- security/secret handling;
- evidence.

Add support-inventory rows to specs/support.toml only as unimplemented/registered metadata if the schema supports such rows without implying support. Otherwise defer support.toml changes until the first capability closes.

### E. Boundary enforcement

Extend dependency/boundary scripts so:
- i2pr-i2pcontrol cannot import daemon/runtime/service implementation owners;
- router core crates cannot depend on i2pr-i2pcontrol;
- only daemon/composition-facing crates may adapt the control contract to router state;
- no UI/frontend dependency enters the crate.

## Required tests

- exact literal inventories with cardinality assertions;
- each known method/action/type/selector parses exactly and canonicalizes deterministically;
- case changes/unknown literals fail with typed classification;
- maximum and maximum+1 bounds for names/strings/collections;
- secret-classification table has no unclassified option;
- dependency-direction/static-boundary checks;
- build with all ordinary workspace feature combinations.

## Acceptance criteria

Plan 286 passes only if:
1. ADR 0028, README/GUARDRAILS narrow provenance exception, and the exact per-file donor manifest agree;
2. i2pr-i2pcontrol exists and is runtime-neutral by static check;
3. the public contract inventory is exhaustive and mechanically tested;
4. no listener, token store, TLS code, router-state adapter, persistence mutation, or frontend code is added;
5. the routine workspace verification floor is green;
6. closure records the exact Proposal/base/fork/Java/i2pd pins and any divergence discovered.

On closure, Plan 287 becomes ready. No other Proposal 170 capability may be claimed yet.
