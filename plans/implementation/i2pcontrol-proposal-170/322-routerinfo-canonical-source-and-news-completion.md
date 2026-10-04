# Plan 322 — Canonical RouterInfo source completion and signed-news owner

Status: **in-progress-prop170-routerinfo-canonical-source-completion**

Classification: capability + observability + signed-content integration.

Hard dependencies: Plans 320 and 321 closed.

## Objective

Implement the exact 43 Proposal 170 RouterInfo additions with their specified return shapes, including logs.clear and a truthful i2p.router.news source. Replace the current normalized 30-selector source matrix as conformance authority.

## Source matrix

Create one row per canonical selector containing:
- exact wire key and return type;
- authoritative owner;
- read cost and cardinality/byte ceiling;
- freshness semantics;
- privacy/sensitivity;
- available/gated/unsupported state;
- test/evidence id.

No request-time whole-router scan is allowed where a bounded maintained snapshot/counter can exist.

## Existing-source remapping

Project existing Plan 295 owners into the canonical fields:
- router identity and serialized RouterInfo;
- cumulative transport byte counters;
- transit byte/bandwidth counters;
- tunnel share ratio;
- participating tunnel info;
- I2PTunnel quick summaries;
- exploratory/client counts and info lists;
- v4/v6 status/error/testing;
- recent/total tunnel success and both queue depths;
- NetDB peers, active peers, serialized RouterInfo lists, limits, bans and active-peer stats;
- AddressBook rows from Plan 321.

Where an owner does not currently maintain enough detail, add a bounded snapshot at the owning subsystem rather than fabricating values.

## Logs and logs.clear

The bounded redacted log ring remains the source for i2p.router.logs.

Add i2p.router.logs.clear as an authenticated control mutation with:
- atomic clear;
- monotonic clear/drop diagnostics if useful internally;
- no effect on ordinary stdout/file logging;
- deterministic concurrent-read/clear tests.

Return the exact Proposal string result.

## Signed news

i2p.router.news must become a real source.

Reuse the bounded fetch capability from Plan 321, but give news its own configured source/trust policy and cache. I2P news is signed SU3 content; the current reseed verifier already proves the workspace can verify bounded SU3 containers but is specialized to reseed content.

Extract only genuinely generic SU3 framing/signature verification into a neutral reusable component. Keep reseed-specific ZIP/RouterInfo policy in NetDB.

News requirements:
- SU3 content type NEWS;
- file type XML or XML.GZ;
- trusted signer set;
- certificate validity;
- signature verification before parse/use;
- bounded gzip/XML input and depth/text/entity handling;
- Atom feed updated field validation;
- strict/sanitized extraction suitable for the Proposal’s String result;
- ETag/Last-Modified conditional acquisition;
- old verified news remains authoritative on transient fetch failure, with freshness/status tracked separately.

Do not add router update/install behavior.

## Exact return shapes

Correct current normalized shape differences. In particular counts that the Proposal declares int/long/double must not remain one-element lists merely because the old contract did so; banned peers must use the declared nested map; nullable id/clock/info fields remain nullable where specified.

## Evidence

- exact 43-row matrix cardinality;
- one live/source test per row;
- null/gated state tests;
- logs read/clear race tests;
- signed-news positive fixture plus wrong signer, expired cert, wrong content/file type, invalid signature, gzip/XML bombs and malformed feed;
- conditional-fetch/no-change/restart cache tests;
- response encoded-byte ceilings;
- differential static fixtures against Java PR6 and i2pd where overlapping.

## Acceptance criteria

Plan 322 closes only when all 43 canonical Proposal additions have exact wire shapes and truthful bounded owners, including a real signed-news source and logs.clear.

## Current implementation progress

- `i2p.router.logs` now returns the Proposal's exact bounded `List<String>` shape from the redacted log ring. `i2p.router.logs.clear` is an authenticated mutation on the same owner: it clears retained entries atomically, preserves cumulative eviction diagnostics and ordinary tracing output, returns the exact Proposal string `"success"`, and defers the clear until other selected fields have resolved.
- Focused evidence: `cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection authenticated_router_info_logs_clear_clears_ring_and_returns_success -- --test-threads=1` and `cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection failed_mixed_router_info_selection_does_not_clear_logs -- --test-threads=1`.
- The canonical 43-field source ownership matrix, remaining metrics, and signed SU3 NEWS fetch/cache/verifier are still open. Plan 322 stays in progress; Plan 328 remains blocked on 322, 326, and 327.

No neutral placeholder is accepted merely to obtain a green matrix.
