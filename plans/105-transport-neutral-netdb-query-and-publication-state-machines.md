# Plan 105: transport-neutral NetDB query and publication state machines

## Status and authority

- Status: **planned; blocked until Plan 104 closes**.
- Date: 2026-08-13.
- Parent authority: Plan 102.
- Prerequisites: Plans 103 and 104.
- Successor: Plan 106.
- Protocol authority: `specs/protocols/02-i2np.md`, `specs/protocols/04-reseed-netdb.md`, and current official I2P NetDB/I2NP documentation.

## Critical roadmap correction discovered during planning

A standard I2P RouterInfo lookup is **not** simply a DatabaseLookup sent over a direct NTCP2 connection.

Current I2P NetDB behavior sends lookups through an outbound exploratory tunnel and requests replies through an inbound exploratory tunnel. `DatabaseStore` or `DatabaseSearchReply` returns through that reply path. Milestone 5 owns exploratory tunnel construction.

Therefore:

```text
Plan 105 can fully implement and deterministically test NetDB query state machines.
Plan 106 can compose them and prove bootstrap/readiness locally.
A standards-conformant live RouterInfo lookup cannot close before Milestone 5 supplies exploratory tunnels.
```

Do not route around this dependency by inventing a direct-link lookup mode and calling it interoperability evidence. Direct transport traffic may still carry protocol-defined RouterInfo exchange/DatabaseStore behavior where the transport or NetDB specification permits it, but that is not equivalent to a normal lookup.

This plan makes the tunnel dependency explicit so the project does not recreate the Milestone 3 mistake of building elaborate evidence for the wrong execution topology.

## Objective

Implement runtime-neutral NetDB client logic that consumes the validated RouterInfo store from Plans 103/104 and produces typed actions for an effects owner.

The state machines must cover the RouterInfo-focused Milestone 4 client path:

- daily routing-key calculation;
- floodfill candidate selection;
- iterative RouterInfo lookup progression;
- bounded handling of DatabaseSearchReply suggestions;
- DatabaseStore RouterInfo validation/insertion;
- query deadlines, peer budgets, duplicate coalescing, cancellation, and completion;
- local RouterInfo publication/store bookkeeping and acknowledgement/verification intent;
- transport/tunnel-neutral outbound I2NP delivery actions.

Plan 105 does not spawn Tokio tasks, open sockets, construct tunnels, or activate a transport.

## Existing surfaces to reuse

The repository already provides structural I2NP messages including:

```text
DatabaseLookupMessage
DatabaseStoreMessage
DatabaseSearchReplyMessage
DeliveryStatus
```

`i2pr-transport` already provides transport-neutral peer and encoded-I2NP delivery types. Plan 105 should reuse appropriate `PeerId`, `EncodedI2npMessage`, delivery IDs/outcomes, and deadlines rather than creating transport-specific send contracts.

Plans 103/104 provide the only eligible RouterInfo store/validation/persistence boundary.

## Hard scope lock

Expected implementation surfaces:

```text
crates/i2pr-netdb/src/routing.rs
crates/i2pr-netdb/src/lookup.rs
crates/i2pr-netdb/src/store_message.rs      or equivalent
crates/i2pr-netdb/src/publication.rs
crates/i2pr-netdb/src/lib.rs
crates/i2pr-proto/src/i2np/...              only missing codec/accessor corrections
crates/i2pr-transport/...                   only minimal transport-neutral contract corrections
crates/i2pr-testkit/...                     only if an existing deterministic seam needs extension
Cargo.toml / Cargo.lock                     if bounded gzip support is required
specs/support.toml/docs                     only after support changes
```

Do not add Tokio to `i2pr-netdb`. Do not depend on `i2pr-transport-ntcp2`, `i2pr-runtime`, or `i2pr-daemon` from `i2pr-netdb`.

Do not implement exploratory tunnels, tunnel build messages, garlic routing, LeaseSet client semantics, or floodfill server behavior.

## Work package 1 — routing-key and distance primitives

### 1.1 Daily routing key

Implement the current I2P routing-key transform exactly:

```text
routing_key = SHA256(search_key[32] || UTC_yyyyMMdd[8 ASCII bytes])
```

The 32-byte search key is the RouterHash for RouterInfo lookup. The daily routing key is used only for local closeness calculations and is never serialized into I2NP lookup messages.

Do not call the wall clock internally. Supply the UTC date/day key explicitly or derive it from an injected current time at the boundary.

Required vectors must cover:

- exact 8-byte date formatting including leading zeroes;
- UTC day boundary;
- deterministic known hash/date result;
- previous/next day produce different routing keys;
- raw RouterHash remains the on-wire lookup key.

### 1.2 XOR distance

Order candidate floodfill routers by XOR distance between:

```text
routing_key(target)
router_hash(candidate floodfill)
```

Use full 256-bit lexicographic comparison of XOR bytes. Do not truncate to machine integers.

Use RouterHash as a deterministic final tie-break where needed for stable tests.

## Work package 2 — candidate eligibility and selection

A lookup candidate is eligible only when the local store has a currently valid RouterInfo for it and that RouterInfo advertises floodfill capability `f`.

Signed capability text is still self-asserted. The selector may also consume bounded local outcome metadata later, but Plan 105 must not create a large peer-reputation framework.

Define a policy object with explicit bounds such as:

```text
max candidates considered
max peers queried per lookup
max search-reply hashes accepted per response
max total suggested hashes retained
lookup total deadline
per-attempt deadline
```

Do not encode current network population counts as fixed protocol constants.

Selection must be deterministic for deterministic inputs, except where an injected RNG is deliberately part of policy.

## Work package 3 — lookup identity and duplicate coalescing

Define a bounded query owner keyed by the target RouterHash and lookup kind. For Plan 105, the active kind is RouterInfo.

Suggested concepts:

```text
LookupId
LookupTarget / LookupKind::RouterInfo
LookupState
LookupOutcome
LookupFailure
```

When multiple local consumers request the same RouterInfo concurrently, coalesce them onto one active network lookup where their policy/deadline compatibility permits. Keep a bounded waiter count.

Cancellation of one waiter must not cancel the shared query while other waiters remain. Cancellation of the final waiter may cancel pending work.

No unbounded arbitrary callback list. Use bounded IDs/results/events.

## Work package 4 — explicit network-path requirement

The pure state machine must represent the fact that a standards-conformant lookup needs an exploratory-tunnel reply path.

Do not make the query engine depend on a concrete tunnel crate that does not exist yet. Instead define a narrow path token/value supplied by the future Milestone 5 owner, sufficient to populate the current `DatabaseLookupMessage` fields:

```text
reply gateway RouterHash
reply tunnel ID
```

Preferred semantics:

```text
start RouterInfo lookup without reply path
    -> NeedsExploratoryReplyPath / PendingPath

supply valid reply path
    -> state machine may emit DatabaseLookup action
```

A direct peer link alone must not satisfy this requirement.

This seam is the explicit handoff to Milestone 5.

## Work package 5 — query action model

The state machine should produce actions, not effects. A representative action vocabulary:

```text
SendI2np {
    peer: PeerId/RouterHash,
    message: EncodedI2npMessage or typed encode request,
    deadline: ...,
}
ScheduleDeadline { ... }
NeedExploratoryReplyPath { lookup_id }
NeedRouterInfo { hash }                  // only if recursive peer resolution is supported
PersistAcceptedRouterInfo { hash }       // optional event to effects owner
Complete { outcome }
```

Keep the vocabulary as small as possible. Do not build a generic actor/event framework.

The runtime adapter in Plan 106/Milestone 5 will translate these actions to actual transport/tunnel delivery.

## Work package 6 — DatabaseLookup construction

For RouterInfo lookup:

- on-wire key is the original RouterHash, not daily routing key;
- lookup type must request RouterInfo according to the existing codec;
- `from`/reply gateway and tunnel delivery fields come from the supplied exploratory reply path;
- excluded peers are bounded and deduplicated;
- reply-encryption behavior must match what current RouterInfo lookup interoperability requires; do not pull LeaseSet-specific encrypted lookup semantics into this plan unless the official RouterInfo path requires them.

Use the existing `DatabaseLookupMessage` constructor/encoder where possible. Correct structural codec gaps rather than hand-assembling bytes in NetDB code.

## Work package 7 — response correlation and acceptance

### 7.1 DatabaseStore success response

A DatabaseStore received as a candidate response is not automatically success.

For RouterInfo data:

```text
bounded gzip/decompression if required by DatabaseStore RouterInfo encoding
 -> bounded RouterInfo decode
 -> Plan 103 validate(expected lookup key)
 -> check record answers the active lookup
 -> normal store replacement policy
 -> persistence event/adapter
 -> lookup success
```

The existing I2NP model currently retains compressed RouterInfo payload as deferred bytes. Plan 105 should add the smallest bounded decompression owner required to turn type-0 DatabaseStore data into a RouterInfo. Use a maintained decompression library; enforce compressed and decompressed byte ceilings before/while allocating.

Do not let a valid but unrelated RouterInfo complete a lookup for another hash.

### 7.2 DatabaseSearchReply

Treat every returned peer hash as an unauthenticated suggestion. The `from` field itself is unauthenticated and must not establish response identity.

For suggested hashes:

- enforce response count bound already present in codecs plus local policy;
- deduplicate against queried/excluded/pending peers;
- compare against target routing-key distance locally;
- prefer known validated floodfill RouterInfos;
- never mark a suggestion trusted merely because it appeared in a response;
- retain only a bounded candidate set.

A malicious response that repeats the same peers or suggests no useful progress must not create an infinite loop.

## Work package 8 — iterative lookup progression

Implement bounded iterative behavior:

```text
select closest eligible unqueried floodfill
 -> emit lookup attempt when reply path available
 -> await bounded response/delivery outcome
 -> on valid target DatabaseStore: complete success
 -> on valid SearchReply: merge bounded suggestions, select next eligible peer
 -> on timeout/delivery failure/non-progress: mark attempt outcome, select next candidate
 -> stop on total deadline, max peers, cancellation, or candidate exhaustion
```

The algorithm must continue to the next known candidate even if a SearchReply fails to provide a strictly closer peer, consistent with current I2P robustness behavior.

Do not recursively explode unknown suggested peer resolution. If a SearchReply names a peer for which no RouterInfo is known, represent it as an unresolved hint. Plan 105 may emit a bounded `NeedRouterInfo` prerequisite action, but the parent query must enforce a total sublookup/depth/work budget. A simpler initial implementation may ignore unresolved hints while continuing known candidates, provided this limitation is explicit and tests prove bounded failure rather than deadlock.

## Work package 9 — timeout, cancellation, and delivery outcomes

All clocks/deadlines are explicit inputs. Tests must not depend on wall-clock sleeps.

Handle at least:

```text
delivery accepted/queued
transport/tunnel delivery failure
action deadline
lookup total deadline
caller cancellation
router shutdown cancellation
peer exhaustion
path unavailable
```

A late response after terminal completion must not resurrect or mutate query state, except that a separately valid unsolicited RouterInfo may be offered to the general store path under its own policy if the runtime owner supports that later.

## Work package 10 — local RouterInfo publication state machine

Implement publication as a pure coordinator, not a live effects service.

Inputs:

```text
current ValidatedRouterInfo local snapshot
eligible floodfill candidates
publication policy
transport/tunnel/path capabilities supplied by owner
```

Responsibilities:

- select a bounded closest floodfill set using routing-key logic;
- construct DatabaseStore for the local RouterInfo using existing I2NP encoding;
- generate/track a nonzero reply token when acknowledgement is requested by the protocol path;
- correlate DeliveryStatus token to the pending store attempt;
- bound concurrent publication attempts;
- schedule republish/verification intent without reading wall clock internally;
- never sign a new RouterInfo merely to retry transport delivery.

Do not implement floodfill replication. i2pr is a client publisher in Milestone 4.

### Verification boundary

Current I2P publication verification may require a subsequent NetDB lookup through exploratory tunnels. Represent this as a `NeedsVerificationLookup` state/action. Plan 105 can test the coordinator end-to-end with deterministic simulated replies, but live verification remains blocked until exploratory tunnels exist.

## Work package 11 — DatabaseStore ingestion outside an active lookup

Add one bounded semantic handler that can accept a RouterInfo DatabaseStore presented by an authenticated/effects owner outside a specific active lookup, if such stores are allowed by current role/context.

The handler must:

- decode/decompress boundedly;
- verify the contained key matches the DatabaseStore key;
- use Plan 103 signature/freshness policy;
- use normal replacement/capacity policy;
- trigger persistence only after validation;
- return typed accepted/stale/conflict/invalid/capacity outcomes.

Do not implement unsolicited LeaseSet handling or floodfill forwarding.

## Work package 12 — deterministic tests

Required routing tests:

1. known routing-key vector;
2. UTC day rollover;
3. XOR ordering across full 256 bits;
4. only valid floodfill-advertising RouterInfos are candidates;
5. deterministic nearest ordering and peer budget.

Required lookup tests:

1. lookup without exploratory reply path requests/awaits path rather than sending directly;
2. successful target DatabaseStore completes query;
3. unrelated DatabaseStore does not complete target lookup;
4. invalid signature/hash/stale RouterInfo cannot complete lookup;
5. SearchReply merges bounded new candidates;
6. duplicate/non-progressing SearchReply cannot loop indefinitely;
7. delivery failure advances to next candidate;
8. per-attempt timeout advances;
9. total deadline terminates;
10. max-peer budget terminates;
11. caller cancellation and shutdown cancellation terminate cleanly;
12. duplicate local requests coalesce with bounded waiter behavior;
13. late response cannot revive terminal query;
14. unresolved suggested hashes remain bounded.

Required publication tests:

1. closest floodfill selection from current local store;
2. DatabaseStore uses the current signed local RouterInfo bytes;
3. acknowledgement token correlation;
4. wrong/duplicate DeliveryStatus token ignored/rejected safely;
5. retry does not resign RouterInfo;
6. verification state requests a standard lookup path rather than direct-link shortcut;
7. cancellation clears pending publication state.

Required decompression tests:

- valid compressed RouterInfo;
- truncated stream;
- malformed gzip;
- decompressed-size limit;
- compressed-size limit;
- decompressed record still requires Plan 103 validation.

Use virtual/deterministic time from existing test infrastructure where suitable. No live sockets are required.

## Work package 13 — support/documentation state

After Plan 105, truthful support should be:

```text
RouterInfo NetDB validation/store       = implemented
persistent cache/reseed ingestion       = implemented
routing-key/floodfill selection         = implemented
RouterInfo lookup state machine         = implemented, runtime/path not live
DatabaseStore RouterInfo ingestion      = implemented
RouterInfo publication coordinator      = implemented, not live-verified
exploratory tunnels                     = not implemented (Milestone 5)
live standards-conformant NetDB lookup  = blocked on Milestone 5 + transport
NTCP2                                   = experimental-non-advertised
```

Do not mark live NetDB interoperability passed.

## Validation

Run at minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p i2pr-netdb
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Run dependency checks if decompression or other dependencies change. No new live CI workflow is required.

## Explicit non-goals

Plan 105 does not implement or activate NTCP2/SSU2, build exploratory tunnels, transmit a real DatabaseLookup, implement LeaseSet lookup/encrypted destination semantics, implement floodfill server behavior, create peer reputation, implement transit tunnels, or use the public I2P network.

## Closure criteria

Plan 105 closes only when:

1. daily routing-key derivation and full-width XOR ordering match the current I2P rules;
2. candidate selection uses only current validated RouterInfos advertising floodfill capability and remains bounded;
3. RouterInfo lookup has a consuming/terminal state machine with explicit path, peer, attempt, time, and cancellation budgets;
4. a lookup cannot emit a standards-conformant send action without an exploratory reply-path token;
5. DatabaseLookup construction uses the original target hash on wire and the daily routing key only locally;
6. DatabaseStore RouterInfo payloads are boundedly decompressed and then validated through Plan 103 with exact key binding;
7. DatabaseSearchReply suggestions are treated as untrusted bounded hints and cannot force loops or unbounded recursion;
8. duplicate lookup coalescing is bounded and cancellation-correct;
9. publication state selects floodfills, tracks DatabaseStore/DeliveryStatus attempts, and represents later verification without live-network shortcuts;
10. no Tokio/socket/transport-specific dependency enters `i2pr-netdb`;
11. deterministic tests cover success, malformed responses, non-progress, timeout, cancellation, peer exhaustion, publication correlation, and decompression limits;
12. documentation explicitly records the exploratory-tunnel dependency and does not claim live lookup support;
13. workspace validation passes;
14. Plan 106 can adapt state-machine actions to runtime services without changing NetDB protocol semantics.

## Handoff to Plan 106

The closure note must identify:

```text
lookup start/input API
exploratory reply-path token API
outbound action/event API
response ingestion API
publication coordinator API
store/persistence event seam
readiness conditions for a reseed-populated but not-yet-networked daemon
```

Plan 106 must preserve the rule that direct transport availability is not equivalent to exploratory NetDB path availability.
