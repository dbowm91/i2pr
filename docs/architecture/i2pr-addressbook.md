# `i2pr-addressbook` — Canonical Naming Owner

Plan 294 canonical `.i2p` address-book owner: four independent
administrative books with fixed lookup precedence, a
subscription-derived table consulted last, typed hostname and
full-Destination validation, the thirteen-key `SetConfig` domain,
bounded subscription ingestion, deterministic versioned generations,
a bounded refresh queue, and a narrow read-only resolver handle.

## Purpose

This crate owns i2pr naming state and its validation. It owns no
sockets, no Tokio tasks, no timers, no filesystem access, no HTTP
client, and no UI behavior. `i2pr-storage` persists opaque
generations; `i2pr-daemon` owns refresh tasks, timers, download
composition, and resolver-handle installation into SAM,
service-tunnel, and control consumers.

## Module layout

| Module | File (lines) | Responsibility | Key public types |
| --- | --- | --- | --- |
| `book` | `book.rs` (459) | Four books, precedence lookup, `AddressBook` reads, `AddressBookControl` mutations | `AddressBook`, `AddressBookControl`, `BookKind`, `EntryMutation`, `EntryOutcome`, `Provenance`, `ResolvedEntry` |
| `hostname` | `hostname.rs` (132) | Canonical `.i2p` validation (lowercase, suffix, labels, lengths) | `Hostname`, `parse_hostname`, `MAX_HOSTNAME_LEN`, `MAX_LABEL_LEN` |
| `destination_text` | `destination_text.rs` (134) | I2P-Base64 text decode + exact `Destination::decode` structural validation | `validate_destination_text`, `MAX_DESTINATION_TEXT_LEN`, `MAX_DESTINATION_BYTES` |
| `config` | `config.rs` (538) | Thirteen-key domain with one disposition per key, typed parsers, atomic whole-map updates | `AddressBookConfig`, `ConfigKey`, `LogLevel`, `parse_config_key` |
| `subscription` | `subscription.rs` (215) | URL validation, strict `hostname=destination` body ingestion with bounds | `SubscriptionSet`, `validate_subscription_url`, `ingest_subscription_body`, `MAX_*` |
| `generation` | `generation.rs` (267) | Deterministic versioned JSON generations with full re-validation on decode | `encode_generation`, `decode_generation`, `GENERATION_VERSION`, `MAX_ENTRIES_PER_BOOK`, `MAX_GENERATION_BYTES` |
| `refresh` | `refresh.rs` (210) | Bounded queue (one active + one coalesced pending) and outcome vocabulary | `RefreshQueue`, `RefreshReason`, `RefreshOutcome`, `RefreshDiagnostic` |
| `resolver` | `resolver.rs` (186) | Immutable snapshots and the read-only shared resolver | `AddressBookSnapshot`, `AddressBookResolver` |
| `error` | `error.rs` (54) | Typed errors carrying no caller material | `AddressBookError` |

## Public surface

`AddressBook`, `AddressBookControl`, `BookKind`, `EntryMutation`,
`EntryOutcome`, `Provenance`, `ResolvedEntry`, `AddressBookConfig`,
`ConfigKey`, `LogLevel`, `parse_config_key`,
`validate_destination_text`, `AddressBookError`,
`encode_generation`, `decode_generation`, `GENERATION_VERSION`,
`MAX_ENTRIES_PER_BOOK`, `MAX_GENERATION_BYTES`, `Hostname`,
`parse_hostname`, `RefreshQueue`, `RefreshReason`,
`RefreshOutcome`, `RefreshDiagnostic`, `AddressBookSnapshot`,
`AddressBookResolver`, `SubscriptionSet`,
`ingest_subscription_body`, `validate_subscription_url`, plus the
`MAX_*` bound constants.

## Key contracts

- Precedence is fixed: private, local, router, published, then
  subscription-derived. The same hostname may live in several books;
  lookup uses the first match.
- Mutations go only through `AddressBookControl`; the resolver
  cannot mutate. Snapshots isolate later commits.
- Entry operations are atomic: hostname and destination validate
  fully before state changes. Delete presence selects deletion;
  delete plus a destination fails (`MixedShapes`); deleting a
  missing hostname fails (`UnknownHostname`).
- Whole-request validation precedes mutation for subscriptions
  (count + every URL) and config (every key + every value).
- Config dispositions: six confined logical artifact paths, hourly
  refresh cadence 1..=720, fetch-path-only proxy host/port,
  inert theme, artifact-only log path/level, fetch-stage lookup
  timeout 1..=300 s, per-book ceiling 1..=1000.
- Generations reject anything but version 1 and re-validate every
  hostname, destination, count, and config value on decode.
- Errors carry no hostnames, destinations, URLs, or values.

## Dependencies

`i2pr-proto` (structural destination validation) + `base64ct`,
`serde`, `serde_json`, `thiserror`. Dev-dependency on
`i2pr-i2pcontrol` for the contract-parity test only (excluded from
the production graph).

## Tests

- 26 in-crate unit rows: hostname matrix, destination
  round-trip/malformed/truncated/oversized, book CRUD/precedence/
  shape errors, config whole-map/path/numeric/proxy validation,
  subscription URL/body/bounds, generation round-trip/hostile
  rejection, refresh coalescing/diagnostics, snapshot isolation.
- `tests/contract.rs` (5 rows): book/config/field inventories,
  ceilings, and path/inert classifications agree with the frozen
  `i2pr-i2pcontrol` contract key-for-key.

## Distinctive design choices

1. The `subscriptions` config key names the staged-download
   artifact (the URL list travels on `SetSubscriptions`).
2. Derived subscription entries live outside the four books and
   replace wholesale, so operator deletions never resurrect.
3. `proxy_host`/`proxy_port`/`lookup_timeout` validate, store,
   and round-trip for the fetch path although no downloader owner
   exists yet (explicitly documented, never a capability claim).
4. The theme key is inert metadata by plan record.
5. I2P-Base64 translates `-`/`~` to RFC 4648 before the reviewed
   `base64ct` decoder (no local codec).
6. Destination text stores verbatim after validation (no silent
   normalization; getters echo committed text).

## Cross-references

- Plan 294 (`plans/implementation/i2pcontrol-proposal-170/294-canonical-addressbook-and-resolver-integration.md`,
  closure `plans/closure/i2pcontrol-proposal-170/294-status.md`).
- Spec dossier `specs/protocols/15-canonical-addressbook.md`.
- Frozen contract: `i2pr-i2pcontrol` `address_book`, `limits`,
  `router_info`, `source_matrix` modules.
- Daemon owner: `docs/architecture/i2pr-daemon.md` (address-book
  manager, refresh worker, consumer wiring).
- Storage adapter: `docs/architecture/i2pr-storage.md`
  (generation file store).
