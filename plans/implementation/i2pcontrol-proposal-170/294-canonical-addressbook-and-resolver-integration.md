# Plan 294 — Canonical AddressBook subsystem and Proposal 170 resolver integration

Status: registered-prop170-addressbook-blocked-on-plan287

Classification: capability + new runtime-neutral router service.

Hard dependency: Plan 287 closed. May execute in parallel with Plans 288 and 289.

## Objective

Add one canonical i2pr address-book/naming owner used by ordinary .i2p resolution and expose that same owner through Proposal 170 AddressBook mutations and RouterInfo address-book getters.

No disconnected I2PControl administrative store is acceptable.

## New crate and ownership

Create i2pr-addressbook as a runtime-neutral crate. It owns:
- typed hostname and full Destination entry validation;
- four administrative books: private, local, router, published;
- lookup precedence and result provenance;
- subscription URL/config domain;
- deterministic state/revision model;
- no sockets, Tokio tasks, unrestricted filesystem access, HTTP client, or UI/theme behavior.

i2pr-storage owns or supplies the atomic generation persistence adapter.

i2pr-daemon owns subscription refresh/download tasks, timers, HTTP/I2P proxy composition, and resolver handle installation into SAM/service-tunnel/admin consumers.

## Resolver integration

Identify every current hostname/.i2p resolution path in:
- SAM NAMING LOOKUP;
- HTTP proxy;
- SOCKS;
- IRC/service tunnel destination references;
- any daemon static alias table.

Define the canonical precedence explicitly. Preserve existing static aliases where required, but ordinary address-book lookup must not be bypassed by a separate Proposal-only map.

All consumers receive a narrow read-only resolver handle. Administrative mutation goes only through the AddressBook control handle.

## Persistence

Persist complete versioned generations with:
- independent four-book contents;
- subscription set;
- accepted config metadata;
- deterministic serialization;
- bounded entry/hostname/destination/config sizes;
- restrictive permissions;
- atomic/recoverable publication;
- prior-generation fallback;
- symlink/special-file/path escape rejection.

Full Destination text must be structurally validated before commit. Hostnames are canonicalized under an explicit .i2p policy with duplicate/conflict behavior documented.

## Proposal 170 AddressBook method

Implement exact pinned semantics for:
- Type private/local/router/published;
- Hostname;
- Destination;
- optional Delete;
- SetSubscriptions;
- SetConfig.

Validate a whole request before mutation. Mixed incompatible operation shapes fail rather than applying partial changes.

Delete/add/update semantics must have deterministic behavior for existing/nonexistent entries and exact response shapes.

## SetSubscriptions

Accept only bounded HTTP/HTTPS subscription URLs if that is what the pinned contract expects. Do not accept arbitrary file/command schemes.

The runtime manager uses:
- one bounded replacement command channel;
- at most one active refresh plus one newest pending generation;
- durable subscription commit before success;
- explicit unavailable failure if the downloader owner is absent.

Download bodies, redirect count, decompression, entry count, and aggregate bytes are bounded. Existing network/proxy policy decides how clearnet HTTP is reached; Proposal requests do not open arbitrary sockets.

## SetConfig

Freeze all thirteen Proposal config keys. Each gets one explicit disposition and typed parser.

Path-like values are treated as AddressBook-owned logical paths/config metadata, not unrestricted filesystem selectors. Normalize under an administrative root or reject values that cannot be safely represented. No traversal, symlink escape, special files, or overwrite of router identity/config/runtime state.

Theme is inert metadata only. It does not create frontend scope.

Log, if retained for compatibility, refers only to a bounded AddressBook-owned artifact and does not redirect global tracing.

proxy_host/proxy_port may configure only the bounded subscription fetch path and do not create a general proxy capability.

## Proposal RouterInfo integration

Once the canonical owner exists, implement:
- private/local/router/published list selectors;
- subscriptions object selector;
- config object selector.

Getters read the same committed generation normal lookup uses.

## Feature isolation

When I2PControl is disabled:
- ordinary AddressBook naming may still be available if configured as a router service;
- Proposal-specific administrative state cannot silently override legacy/static resolution unless the AddressBook subsystem is explicitly enabled/configured;
- stale/corrupt control files cannot influence resolution before successful validated activation.

Choose and document the exact startup activation rule.

## Evidence

- four-book independence and precedence;
- add/update/delete/lookup;
- invalid destination/hostname/duplicate/conflict;
- complete-generation crash/recovery;
- path confinement and symlink attacks;
- subscription replacement, failed download, oversized body/list, redirect ceiling;
- concurrent mutation serialization;
- restart persistence;
- SAM/HTTP/SOCKS resolution through the same owner;
- Proposal getters equal resolver owner state;
- disabled I2PControl and disabled AddressBook isolation;
- theme/log cannot affect frontend/global logging.

## Acceptance criteria

Plan 294 closes when one canonical AddressBook owner drives both ordinary i2pr naming and Proposal 170 administrative/getter behavior with atomic persistence and bounded refresh composition. A Proposal-only JSON database does not satisfy this plan.
