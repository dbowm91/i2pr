# Plan 321 — Canonical AddressBook configuration, subscription fetch, and publication semantics

Status: **registered-prop170-addressbook-operational-completion-blocked-on-plan320**

Classification: capability + daemon I/O composition.

Hard dependency: Plan 320 closed.

## Objective

Turn the existing canonical i2pr-addressbook owner into the full operational AddressBook surface expected by Proposal 170: exact SetConfig semantics, live bounded subscription acquisition, conditional refresh state, and should_publish behavior.

Plan 294’s books/resolver/persistence remain authoritative. Do not create a second naming database.

## Current gap

The existing owner validates and stores subscription/config state but no downloader consumes it. proxy_host, proxy_port and lookup_timeout therefore have no live fetch effect. The existing SetConfig key set also differs from Proposal 170.

## Required work

### 1. Exact SetConfig projection

Map the thirteen canonical Proposal keys from Plan 320 onto typed internal state:
- subscriptions;
- update_delay;
- published_addressbook;
- router_addressbook;
- local_addressbook;
- private_addressbook;
- proxy_port;
- proxy_host;
- should_publish;
- etags;
- last_modified;
- log;
- theme.

Paths are logical AddressBook-owned artifacts under a confined root, never arbitrary filesystem capabilities.

theme remains inert metadata. log is an AddressBook diagnostic artifact only.

### 2. Bounded content-fetch owner

Add a daemon-owned fetch service with a narrow trait/capability reusable by Plan 322 news acquisition.

It must support the AddressBook subscription transport actually configured by the canonical proxy fields without giving AddressBook arbitrary sockets. Prefer the router’s I2P HTTP path or an explicitly configured local eepProxy. Direct clearnet fetching is not implicitly authorized.

Bounds:
- URL count/length/scheme;
- redirects;
- DNS/name resolution;
- connect/read/total deadlines;
- response-header bytes;
- compressed and decoded body bytes;
- concurrent fetches;
- retries/backoff;
- aggregate work per refresh generation.

Cancellation releases every permit/task.

### 3. Conditional acquisition

Implement ETag and Last-Modified state as real owned artifacts. Send conditional requests on subsequent refreshes and treat 304 as a successful no-change outcome.

Commit validator metadata atomically with the generation it validates so crashes cannot pair stale content with newer validators.

### 4. Subscription parsing and commit

Run the existing bounded whole-body parser over fetched bodies. One corrupt or over-limit fetch cannot partially mutate the canonical book.

Define deterministic precedence across multiple subscription URLs and deterministic duplicate handling.

Commit the derived subscription layer atomically and publish a new resolver snapshot only after the complete refresh generation validates.

### 5. update_delay and replacement

update_delay governs daemon cadence in hours with hard bounds and deterministic injected-time tests.

SetSubscriptions replaces the configured list atomically and triggers at most one coalesced refresh according to the existing one-active/one-pending queue contract.

### 6. should_publish

Give should_publish a real owner:
- false: no published artifact regeneration from router-book state;
- true: generate/update the confined published-addressbook artifact atomically from the canonical eligible set.

Do not publish private/local-only entries accidentally. Document exact source/precedence policy and test it.

### 7. Canonical getters

The six Proposal RouterInfo AddressBook selectors must return exact list/map shapes from the same committed generation, including configured artifact path metadata where the Proposal requires it.

## Security

Treat subscription content and URLs as untrusted. No credentials in logs. No arbitrary file:// or command schemes. Reject path traversal/symlink/special-file escape. Fetch output cannot inject config keys.

## Evidence

Use a deterministic local HTTP/I2P-proxy fixture to prove:
- 200 initial load;
- ETag and Last-Modified conditional 304;
- changed content replacement;
- failure leaves old generation authoritative;
- body/count/redirect/time ceilings;
- two subscriptions deterministic merge;
- SetSubscriptions concurrent replacement/coalescing;
- should_publish on/off;
- restart recovery;
- SAM/service tunnel/control getters all observe the same snapshot.

## Acceptance criteria

Plan 321 closes when every canonical AddressBook config field has its intended real or explicitly inert effect, live subscriptions refresh through a bounded owner, and ordinary .i2p resolution and Proposal getters remain one coherent state.

Closure unblocks Plan 322.