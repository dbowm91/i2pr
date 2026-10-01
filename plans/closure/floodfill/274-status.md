# Plan 274 status — passed bounded lookup serving and ECIES replies

Status: **passed-m12-lookup-serving-bounded-dsrm-and-ecies-replies**

Implementation commit: recorded in Git history for this closure.

## Requirement-to-evidence

| Requirement | Evidence |
|---|---|
| RouterInfo and LeaseSet-family lookup hits use only main-router server-authority records | `crates/i2pr-netdb/src/floodfill_service.rs::lookup_body`; `crates/i2pr-netdb/src/server_store.rs::database_store_for_answer` |
| ANY precedence, LeaseSet and RouterInfo type constraints, exploration exact-hit DSRM policy | `lookup_hits_misses_and_exploration_follow_adr_policy`; policy authority is ADR 0027 §5 |
| DSRM candidates exclude requester/local/explicit hashes, filter hidden and wrong-family records, and sort deterministically | `ServerNetDb::router_info_candidates`; `dsrm_candidates_are_excluded_type_filtered_and_hard_limited`; `hidden_routerinfo_is_neither_returned_as_a_hit_nor_a_search_candidate` |
| Default three and hard sixteen suggestion limits | `FloodfillStorePolicy::default`; DSRM tests with both default and oversized policy count |
| Lookup request target/global throttling reuses bounded Plan 273 policy | `handle_lookup`; `repeated_lookup_target_is_throttled_by_the_store_service_policy` |
| Tunnel reply route and exact one-tag supplied-key ECIES, no downgrade | `requested_tunnel_reply_uses_ecies_and_rejects_plaintext_downgrade`; `ReplyProtection`; `LookupFailure` |
| Correct wire framing includes cleartext session tag followed by ciphertext; wrong key/tag rejected | `crates/i2pr-crypto/src/lib.rs::seal_netdb_ecies_reply`; service test checks tag prefix; crypto test exercises wrong key and wrong associated-data tag |
| Independent ECIES known-answer vector | `netdb_ecies_reply_uses_supplied_tag_as_associated_data`; vector was independently computed with Python `cryptography` ChaCha20Poly1305, key `44×32`, tag `55×8`, zero nonce, plaintext `bounded reply body` |
| RouterInfo answer encoding follows canonical gzip header | Service test asserts the first ten compressed bytes are `1F 8B 08 00 00 00 00 00 02 FF` |
| Reply payload and response candidate counts are bounded; direct/tunnel destination is explicit | `max_reply_bytes`, `MAX_DATABASE_SEARCH_REPLY_PEERS`, and typed `FloodfillReplyIntent` |
| No runtime, destination session manager, daemon composition, or advertisement coupling | `scripts/check-m12-floodfill-boundaries.sh`; support matrix remains non-advertised |

## Verification

All commands were run locally on the implementation checkout:

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-crypto --all-targets` — 53 passed.
- `rtk cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` — 162 passed.
- `rtk cargo test --locked -p i2pr-proto --all-targets` — 154 passed.
- `rtk cargo clippy --locked -p i2pr-crypto -p i2pr-netdb --all-targets -- -D warnings` — passed.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- `rtk bash scripts/check-m12-floodfill-boundaries.sh` — passed.
- `rtk git diff --check` — passed.

Protocol source used for the supplied-key Existing Session shape: [official I2NP specification,
DatabaseLookup reply encryption](https://www.i2p.net/en/docs/specs/i2np/), which specifies the
clear 8-byte session tag, ChaCha/Poly1305, nonce zero, and the tag as associated data. ADR 0027
freezes that supported mode and the exploration-hit policy for this repository.

## Security, operational limits, and findings

Reply keys and tags remain borrowed from zeroizing/redacted protocol owners and are not retained
in service state. Only the explicitly supported ECIES mode with exactly one tag is accepted for
tunnel replies. Encryption failure, an unsupported mode, an invalid tunnel route, or a size
ceiling violation yields a typed no-response outcome. Direct replies are unencrypted only when
the request did not request encryption. The service emits effects; it does not dispatch network
messages or create tasks.

No dependency was added. No critical/high findings remain open. Medium/low findings: none
recorded. Type 5 remains deferred. Replication transport, persistence, daemon lifecycle, live
qualification, and `caps=f` remain outside this plan.

## Unblock audit and roadmap disposition

Plan 275 lists Plan 274 as its hard dependency and now has stable bounded selectors over validated
main-router RouterInfos plus `ReplicationCandidate` and daily-routing-key primitives. Move Plan
275 to ready. Plans 276–279 remain blocked in sequence. Plan 280 remains stopped pending a vetted
I2P-compatible Red25519 provider; Plan 281's type-5 deferral remains authoritative. No other
blocked work is unblocked by this closure.
