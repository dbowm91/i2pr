# i2pr-netdb

Runtime-neutral RouterInfo validation, bounded in-memory NetDB store,
SU3/reseed verification, peer-selection primitives, local signed
RouterInfo construction, transport-neutral query state machines, and
RouterInfo publication coordinator.

> Status: experimental. Not production-ready. See `README.md` and
> `GUARDRAILS.md`.

## Purpose

`i2pr-netdb` is the first stateful router-information subsystem in
`i2pr`. It owns:

- cryptographic, freshness, and key-binding validation for `RouterInfo`
  records (`ValidatedRouterInfo`);
- a bounded in-memory store with deterministic
  replacement/conflict/expiry semantics (`RouterInfoStore`);
- I2P Base64 codec for reseed filenames (`base64`);
- SU3 reseed container parser and RSA-SHA512-4096 signature verifier
  (`reseed`);
- pure routing primitives (`xor_distance`, `nearest`,
  `nearest_floodfill`);
- local signed RouterInfo construction without advertising unqualified
  transports (`LocalRouterInfoBuilder`);
- the daily routing-key derivation and bounded nearest-floodfill
  selection (`routing`, `lookup_policy`);
- the typed lookup identity, exploratory-tunnel reply-path token, and
  waiter-set/coalescing primitives (`lookup_id`);
- the iterative `RouterInfo` lookup state machine and bounded
  `DatabaseStore` decompression (`lookup_action`, `databaselookup`,
  `lookup_engine`);
- the local RouterInfo publication coordinator that emits one
  bounded `DatabaseStore` per nearest floodfill without becoming a
  live effects service (`publication`);
- the bounded unsolicited `DatabaseStore` handler outside an active
  lookup (`store_message`).

## Module layout

| Module | Purpose |
| --- | --- |
| `router_info` | `RouterHash` derivation, `ValidatedRouterInfo` boundary, `ValidationContext` |
| `store` | `RouterInfoStore` with checked arithmetic, byte quotas, replacement/conflict/expiry |
| `base64` | I2P Base64 alphabet codec (RFC 4648 with `~` for `=` and `-?` for `+/`) |
| `reseed` | SU3 parser, RSA signature verification, ZIP ingestion, `ReseedSignerTrustSet` |
| `routing` | XOR distance, daily routing-key derivation, deterministic nearest-N floodfill selection |
| `local` | `LocalRouterInfoBuilder`, self-validation, capability policy |
| `lookup_policy` | Bounded `LookupPolicy`, candidate eligibility, floodfill selection |
| `lookup_id` | `LookupId`, `LookupKind`, exploratory-tunnel `ReplyPath`, bounded `WaiterSet` |
| `lookup_action` | Typed `LookupAction` vocabulary, bounded gzip decompression |
| `databaselookup` | Standards-conformant `DatabaseLookup` builder |
| `lookup_engine` | Iterative `RouterInfo` lookup state machine |
| `publication` | Local RouterInfo publication coordinator |
| `store_message` | Unsolicited `DatabaseStore` ingestion handler |

## Dependency boundary

```text
i2pr-netdb -> i2pr-crypto, i2pr-proto + thiserror, base64ct, sha2, x509-parser, zip, sad-rsa, flate2
```

Runtime-neutral: no `tokio`, no `std::net`, no `std::fs`, no sockets,
no DNS. Filesystem I/O belongs to `i2pr-storage` (raw-byte seam) and
`i2pr-netdb-persist` (composition owner).

## Plan 104 surfaces

- `base64::decode` / `base64::encode` — I2P Base64 codec for
  reseed filenames.
- `reseed::parse_su3` / `verify_su3` / `verify_su3_with_signers` /
  `verify_su3_archive` — full SU3 trust + signature verification
  pipeline. Plan 105 migrates the underlying RSA primitive to
  `sad-rsa 0.2` (Marvin-Attack hardened pure-Rust fork of the
  RustCrypto `rsa` crate) so the Plan 104 SU3 trust path stays
  inside the `cargo deny` advisory budget.
- `reseed::ReseedSignerTrustSet` — typed signer trust store with
  certificate validity enforcement.
- `reseed::ReseedLimits` — tuned scan budgets for the reseed pipeline.
- `reseed::TrustedSigner` — parsed certificate with modulus, exponent,
  and validity interval.

## Plan 105 surfaces

- `routing::daily_routing_key` / `format_daily_key` — bounded
  `SHA256(search_key || UTC_yyyyMMdd)` derivation; no wall-clock
  reads; deterministic for any caller-supplied `Date`.
- `lookup_policy::LookupPolicy` — bounded peer budget, per-attempt
  deadline, total deadline, suggested-hash limit and total ceiling.
- `lookup_policy::select_floodfill_candidates` — nearest-N floodfill
  selection that respects the candidate ceiling, the policy peer
  budget, and a caller-supplied exclusion set.
- `lookup_id::LookupId`, `lookup_id::LookupKind`, `lookup_id::ReplyPath`
  — typed lookup identity and the explicit exploratory-tunnel handoff
  token. The lookup state machine refuses to emit any
  `SendDatabaselookup` action until the call site supplies a
  `ReplyPath`.
- `lookup_id::WaiterSet` / `lookup_id::CoalescedTargets` — bounded
  waiter and target coalescing buffers.
- `lookup_action::decompress_router_info` — bounded gzip
  decompression with explicit compressed and decompressed length
  ceilings.
- `lookup_action::LookupAction` — the small action vocabulary the
  state machine emits (`SendDatabaselookup`, `NeedExploratoryReplyPath`,
  `Complete`).
- `databaselookup::build_databaselookup` — standards-conformant
  `DatabaseLookupMessage` builder; the on-wire key is the raw
  RouterHash (not the daily routing key), the `from`/reply-tunnel
  fields come from the supplied reply path, and excluded peers are
  bounded by `LOOKUP_EXCLUDED_PEER_BUDGET`.
- `lookup_engine::RouterInfoLookup` — the iterative state machine;
  handles `DatabaseStore`, `DatabaseSearchReply`, and bounded
  delivery outcomes; never signs or re-signs RouterInfos.
- `lookup_engine::CoalescedRouterInfoLookup` — bounded coalescing
  across multiple local requests for the same target.
- `publication::PublicationCoordinator` — local RouterInfo
  publication coordinator that emits one bounded `DatabaseStore` per
  nearest floodfill with a `reply_token=0` store-and-forget
  semantics; tracks acknowledgement tokens, never re-signs the
  local RouterInfo, and surfaces a `needs_verification_lookup()`
  signal for the future Milestone 5 verification path.
- `store_message::handle_unsolicited_databasestore` — bounded
  ingestion handler for `DatabaseStore` messages that arrive outside
  an active lookup; rejects non-RouterInfo payloads and enforces the
  Plan 103 validator after decompression.

## Key contracts

- `ValidatedRouterInfo::from_router_info` is the only constructor;
  there is no unchecked insertion path.
- `RouterInfoStore::insert` is the only public store entry point; it
  enforces byte quotas and replacement/conflict/expiry semantics
  deterministically.
- SU3 signature verification delegates to the `sad-rsa` crate's
  PKCS#1 v1.5 + SHA-512 implementation; the repository does not
  implement RSA primitives locally. `sad-rsa` is the Marvin-Attack
  hardened pure-Rust fork of `rsa` that removes RUSTSEC-2023-0071.
- `ReseedSignerTrustSet` maps signer identifiers to parsed certificates;
  the verifier requires an exact match before signature verification
  proceeds.
- ZIP ingestion is bounded by `ReseedLimits`; one archive exceeding
  any limit fails closed with zero records accepted.
- Daily routing-key derivation never reads the wall clock; the
  caller supplies the UTC `Date`.
- The lookup state machine refuses to emit a standards-conformant
  `DatabaseLookup` until a `ReplyPath` is supplied; a direct peer
  link alone is not equivalent to a complete reply path.
- Decompression enforces compressed and decompressed byte ceilings
  before allocating any buffer.
- Publication never re-signs the local RouterInfo; retries reuse
  the originally encoded bytes and the originally minted reply
  token.

## Tests

All tests run locally. No root, namespaces, Java I2P, i2pd, or
Internet connection is required. The SU3 end-to-end test uses a
deterministic 2048-bit RSA test signer and verifies the full SU3
header + signature + ZIP archive + RouterInfo validation path. The
Plan 105 test suite covers routing-key derivation, candidate
selection, lookup identity/coalescing, action emission, decompressor
bounds, lookup state-machine progression, publication correlation,
and unsolicited store ingestion.
