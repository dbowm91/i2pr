# Plan 274 — M12 DatabaseLookup service, bounded search replies, and ECIES reply protection

Status: **passed-m12-lookup-serving-bounded-dsrm-and-ecies-replies**

Classification: capability / cryptographic wrapper.

Hard dependency: Plan 273 passed.

## 1. Objective

Implement runtime-neutral floodfill lookup serving for the ADR-authorized lookup types, including
namespace-safe record hits, bounded DatabaseSearchReply misses/exploration, excluded-peer policy,
and current ECIES supplied-key protected replies.

## 2. Current evidence

i2pr already parses DatabaseLookup reply-encryption material and has client-side iterative search
handling. i2pr-crypto has ChaCha20-Poly1305 and ECIES Existing Session machinery, but the NetDB
reply key/tag is a one-shot caller-supplied context and must not be installed into destination
session state.

## 3. Invariants

- Lookup key/excluded peers on the wire are real hashes; daily routing key is local-only selection.
- Router-level replies read only server-eligible main namespace material.
- A lookup hit returns only the requested ADR-authorized record family.
- A miss returns a bounded DSRM: default 3 peers, never more than the ADR hard maximum (<=16).
- Exploration returns only eligible non-floodfill routers according to the frozen ADR policy.
- Excluded peers, requester/local router, invalid/unreachable candidates, and namespace-ineligible
  peers are filtered.
- DSRM from field is treated as unauthenticated by consumers; server emits local hash as specified
  but never uses it as security proof.
- ECIES reply key material is non-Debug, consumed narrowly, zeroized, and never retained as a
  destination session.
- Unsupported encryption modes fail closed according to ADR 0027; no downgrade from requested
  protected reply to plaintext.
- Reply plaintext/ciphertext size and crypto work are explicitly bounded.

## 4. Required production changes

A. Add a pure lookup server method over FloodfillService.

B. Add nearest eligible floodfill/non-floodfill selectors over provenance-safe main NetDB views,
using daily_routing_key() and deterministic tie-breaking.

C. Implement DatabaseStore hit and DatabaseSearchReply miss action construction.

D. Add a narrow crypto API equivalent to seal_netdb_ecies_reply(reply_key, reply_tag, plaintext):
ChaCha20-Poly1305, nonce zero, 8-byte tag as AD, Existing Session wire shape. Do not expose generic
raw-key AEAD outside the protocol wrapper.

E. Model direct versus reply-tunnel delivery as typed reply intents for the daemon; no network I/O.

## 5. Scope / non-goals

No replication, persistence, daemon queues, floodfill config, caps=f, ElGamal floodfill-router
reply implementation unless ADR 0027 explicitly requires it.

## 6. Work packages

1. Lookup-type dispatch and hit selection.
2. DSRM candidate selection and excluded-peer handling.
3. Exploration behavior locked to ADR decision.
4. ECIES one-shot reply seal/open test helper and zeroization/redaction tests.
5. Amplification and malformed lookup tests.
6. Static guards preventing destination EciesSessionManager from becoming the NetDB reply owner.

## 7. Failure / cancellation / restart / contention

Synchronous. A lookup may return NoResponse/Unsupported/Throttled when policy requires; it never
allocates a task. Encryption failure returns a typed terminal outcome and must not fall back to
plaintext.

## 8. Compatibility and migration

Client-side RouterInfoLookup remains unchanged. New server selectors may reuse routing primitives
but not its active-query state. No wire/config/storage migration.

## 9. Required tests

- RI hit/miss;
- LeaseSet-family hit/miss per ADR lookup type;
- deprecated ANY behavior exactly as ADR decided;
- exploration returns only non-floodfill candidates and handles known-key ambiguity per ADR;
- excluded set up to wire maximum with bounded internal work;
- default 3 / hard <=16 DSRM hashes;
- deterministic daily-key selection;
- no main/client namespace leakage;
- ECIES known vector, tag as AD, nonce zero, wrong key/tag failure, no plaintext downgrade;
- reply key/tag Debug redaction and drop zeroization;
- response-size/amplification ceilings;
- repeated lookup throttle integration from Plan 273 policy.

## 10. Exact verification

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-crypto --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo clippy --locked -p i2pr-crypto -p i2pr-netdb --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
~~~

## 11. Documentation

Update NetDB/I2NP support documentation with exact lookup and reply-encryption subset. Keep
floodfill role non-advertised.

## 12. Acceptance criteria

- Hit/miss/exploration semantics are deterministic and source-backed.
- DSRM amplification is bounded.
- ECIES requested protection is correct and cannot silently downgrade.
- Server logic remains runtime-neutral and namespace-safe.
- No replication/daemon/config/advertisement.
- No critical/high finding remains.

## 13. Stop conditions

Stop if a current reference requires an encryption mode ADR 0027 excluded, if the existing parser
cannot distinguish requested modes safely, or if exact reply bytes disagree across normative
vectors and both independent references.

## 14. Closure evidence

Record vectors, lookup matrices, amplification bounds, secret review, exact commands, and unblock
audit. On pass, move Plan 275 to ready.

## 15. Handoff

Plan 275 consumes ReplicationCandidate from Plan 273 and selection primitives from this plan; it
does not alter lookup semantics.
