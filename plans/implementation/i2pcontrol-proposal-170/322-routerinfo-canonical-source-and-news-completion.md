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
- Canonical RouterInfo selection now returns Proposal-permitted null for unavailable `clockskew`, local `info`, and unpublished router `id`; the latter uses the real RouterHash inspection owner when published. The four AddressBook lists, subscriptions map, and config map now serialize from Plan 321's committed AddressBook snapshot with bounded output and canonical names. Their ownership remains the Plan 321 manager and resolver. Focused evidence: `cargo test --locked -p i2pr-daemon --lib plan294_addressbook_method_drives_the_canonical_owner -- --test-threads=1`, `cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection router_info_hash_gated_then_published_over_wire -- --test-threads=1`, and `cargo test --locked -p i2pr-daemon --test i2pcontrol_differential differential_corpus_against_production_composition -- --test-threads=1`.
- Canonical received/sent byte selectors now read the existing cumulative `ControlMetrics` source and fail while it is unobserved. Its documented coverage is the SSU2 I2NP byte counters; loopback/destination traffic and any transport without a registered counter are not included. Focused evidence: `cargo test --locked -p i2pr-daemon --lib proposal_transport_totals_require_and_read_authoritative_sample -- --test-threads=1`, `cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection canonical_transport_totals_are_served_from_published_metrics -- --test-threads=1`, and the production-composition differential corpus checks that these selectors stay unavailable before a transport sample.
- `i2p.router.netdb.peers` and `i2p.router.netdb.peers.list` both use the same bounded known-peer snapshot owner and return the declared string-list shape. The production-composition differential corpus checks both canonical names against that owner.
- Added a separate canonical source matrix derived in Proposal order from all 43 wire fields; the historical normalized Plan 288 matrix remains intact for its original contract. Rows record the declared JSON type, owner/snapshot, per-value cardinality and byte ceilings, sensitivity, freshness, availability, and whether source-specific evidence exists. The current census contains 23 explicit Plan 322 gaps; nullable fields, logs, sampled SSU2 counters, known/active peers, bounded I2PTunnel summaries, the attested build queue, sampled recent/cumulative tunnel success ratios, and AddressBook getters have their actual current owners represented. Inventory tests verify all 43 unique keys and fail if a gap is mislabeled as having source evidence. This is a current-state matrix; source-specific tests and resolved owners remain required before closure.
- `i2p.router.net.tunnels.i2ptunnel` now returns a bounded, sorted object-list projection of the existing startup service inventory and live manager overlay. Each row exposes only name, side, kind, enabled/running state, and loopback bind. The existing `ClientServicesInfo.I2PTunnel` output shape is unchanged. Focused evidence: `proposal_i2ptunnel_summaries_are_bounded_and_canonical` plus the production-composition differential corpus.
- RouterInfo dispatch now validates each canonical result against its frozen Proposal JSON type before returning it; `null` is accepted only for `id`, `clockskew`, and `info`. The `logs.clear` result shape is also validated before the deferred mutation executes. Focused evidence: `proposal_router_info_shape_guard_matches_every_declared_json_type`.
- `i2p.router.net.tunnels.successrate` and `.totalsuccessrate` now project the existing `ControlMetrics` interval and cumulative succeeded/attempted ratios as JSON doubles. Both remain unavailable when no attempt has been reported, since 0/0 is undefined. The source rows are published-gated and the production differential corpus checks the default gaps; `proposal_success_rates_require_attempts_and_read_metrics` covers both gaps and the 3/4 sample.
- `i2p.router.net.tunnels.queue` now projects the attested Plan 295 build-queue snapshot as the Proposal's scalar integer; the old internal selector retains its legacy one-element list. Composition currently attests depth zero because no build coordinator is installed. `proposal_tunnel_queue_depth_uses_attested_snapshot` checks a nonzero sample and the production differential corpus checks the canonical scalar result.
- Exploratory/client directional counts and the three canonical tunnel-info lists now use the attested aggregate tunnel snapshot conditionally: zero aggregate proves zero counts and empty lists; any nonzero aggregate fails closed because no direction/detail snapshot exists. `proposal_empty_tunnel_projection_requires_zero_aggregate` checks both cases, and the production differential corpus exercises all seven selectors against the default zero snapshot. These rows move from unavailable to published-gated.
- `i2p.router.net.tunnels.tbmqueue` now has a separately published scalar owner instead of being conflated with the tunnel-request queue. The current composition attests zero because it has no tunnel-build coordinator; the source unit test distinguishes unpublished from an attested nonzero sample, and the production differential corpus checks the canonical scalar result. Fifteen canonical fields remain explicit gaps.
- `i2p.router.netdb.activepeers.info` and `.peers.info` now return empty serialized lists only when the corresponding attested peer-hash snapshot is empty. Nonempty hashes still fail closed without serialized RouterInfo data. The source test checks both states, the source matrix marks these rows published-gated, and the production differential corpus checks the default empty lists. Thirteen canonical fields remain explicit gaps.
- `i2p.router.netdb.activepeers.stats` and `i2p.router.netdb.bannedpeers` now likewise return empty list/map values only when their respective active-peer and ban owners attest empty sets. Populated sources fail closed because stats or ban reason/expiry details are absent. The source test covers these empty and nonempty states; eleven canonical fields remain explicit gaps.
- `i2p.router.info` now publishes the bootstrap owner's real signed local RouterInfo in bounded I2P-base64 form. The canonical getter returns null only before publication, as permitted by the Proposal. Its publication cap bounds the encoded output, and the inspection unit plus production-composition differential corpus cover published and absent states.
- The daemon-owned bounded content-fetch capability now accepts an explicit per-consumer body ceiling while retaining its fixed response-header cap, loopback-proxy-only routing, redirect rejection, and identity-encoding policy. The existing AddressBook path still selects its original 1 MiB ceiling; this provides the correctly bounded acquisition seam for the separately configured signed-news fetcher. Focused evidence: `cargo test --locked -p i2pr-daemon --lib addressbook_fetch::tests -- --test-threads=1` (5 passed), plus `cargo check --locked -p i2pr-daemon --all-targets`.
- Common SU3 framing and explicit-key RSA-SHA512 verification now live in runtime-neutral `i2pr-su3`; NetDB delegates signature verification there and retains certificate parsing/trust lookup plus reseed ZIP and RouterInfo policy. The crate rejects unsupported signature types and applies caller-owned file/content ceilings. X.509 RSA modulus/exponent extraction now removes DER integer sign padding so signature lengths match the actual RSA key size. Focused evidence: `cargo test --locked -p i2pr-su3`, `cargo test --locked -p i2pr-netdb reseed -- --test-threads=1`, NEWS certificate-signature fixtures, and `bash scripts/check-dependency-direction.sh`.
- Signed NEWS has a separately configured loopback-proxy fetcher, signer certificate pin, NEWS SU3 content/file-type gate, RSA-SHA512 signature-before-parse path, bounded XML/GZIP parser, conditional validators, retained prior verified feed on fetch/validation failure, and opaque current/backup persistence that is re-verified at startup. Feed output is a sanitized plain string. Parser checks reject non-UTF-8 declarations, empty or duplicate required elements, namespace override/prefix ambiguity, DTD/processing instructions, invalid timestamps, excessive gzip expansion, and HTML delimiters in rendered text. Focused evidence: `cargo test --locked -p i2pr-daemon --lib news::tests -- --test-threads=1` (11 passed, including signed XML and GZIP feeds, wrong signer, expired pinned cert, invalid signature, wrong content/file type, ETag/Last-Modified conditional 304, restart re-verification, and last-known-good retention), `cargo test --locked -p i2pr-storage verified_content_cache -- --test-threads=1` (2 passed), `cargo check --locked -p i2pr-daemon --all-targets`, and the source-matrix contract test. Only RSA-SHA512 type 6 is supported by the extracted verifier; alternate NEWS signature algorithms remain unsupported pending a provider/format-specific plan decision.
- The canonical 43-field source ownership matrix, remaining metrics, signed-news acceptance evidence, and remaining owner-backed values are still open. Ten fields remain explicit gaps. Plan 322 stays in progress; Plan 328 remains blocked on 322, 326, and 327.

No neutral placeholder is accepted merely to obtain a green matrix.
