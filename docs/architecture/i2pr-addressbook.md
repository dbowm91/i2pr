# `i2pr-addressbook` — Canonical `.i2p` Naming Owner (Plan 294)

Path: `crates/i2pr-addressbook`. Plan 294 canonical runtime-neutral naming
owner: four independent administrative books with fixed lookup precedence, a
subscription-derived table consulted last, typed hostname and full-Destination
validation, the Proposal 170 `SetConfig` domain, bounded subscription
ingestion, deterministic versioned generations, bounded refresh composition,
and a narrow read-only resolver handle. No I/O, no sockets, no Tokio, no
timers, no filesystem, no HTTP client, no UI behavior.

## Purpose

`i2pr-addressbook` owns i2pr `.i2p` naming state and its validation, and
nothing else. It is a leaf contract crate: `#![forbid(unsafe_code)]`
(`src/lib.rs:18`) and ten private modules re-exported through one `pub use`
block.

What the crate does **not** own, verified against source:

- **No filesystem.** No `std::fs`, no file handles, no path construction. The
  only `std::net` import in the whole crate is `IpAddr` for parsing a
  configured loopback proxy literal (`src/config.rs:28`), which is a string
  parse, not a socket. Verified by grep: no `std::fs`, `tokio`, `async fn`,
  `spawn`, `File`, or `reqwest` token anywhere under `src/`.
- **No timers, no Tokio tasks, no executor.** `refresh.rs` is a state
  machine, not a scheduler (see *Refresh is composition, not execution*).
- **No HTTP client and no downloader.** URLs are validated and bodies are
  parsed here; bytes are fetched by the daemon
  (`crates/i2pr-daemon/src/addressbook_fetch.rs`, a loopback-eepProxy-only
  fetcher that imports this crate's bounds).
- **No UI or frontend behavior.** `theme` is inert stored metadata with no
  side effect (`src/config.rs:18-19`).

Ownership division, as stated by the crate doc comment
(`src/lib.rs:10-16`) and confirmed in the source:

- `i2pr-storage` persists **opaque** generation bytes. It never parses them:
  `crates/i2pr-storage/src/address_book_generation.rs:3` — "stores opaque
  generation bytes only: it never parses". Serialization and validation stay
  here (`generation.rs`).
- `i2pr-daemon` owns the refresh worker, the cadence timer, download
  composition, the diagnostic artifact, and resolver-handle installation into
  SAM / service-tunnel / I2PControl consumers. Only `i2pr-daemon` is allowed
  to depend on this crate (`scripts/check-dependency-direction.sh:35,65`;
  `docs/architecture/dependency-graph.md:33`).

Administrative mutation goes only through `AddressBookControl`; ordinary
lookup goes only through `AddressBookResolver`, which cannot mutate.

## Module layout

Line counts from `wc -l` on `crates/i2pr-addressbook/src/`.

| Module | File (lines) | Responsibility | Key public types |
| --- | --- | --- | --- |
| `book` | `book.rs` (591) | Four books, fixed precedence lookup, read surface on `AddressBook`, sole mutation surface on `AddressBookControl`, revision counter, published-book projection | `AddressBook`, `AddressBookControl`, `BookKind`, `Provenance`, `ResolvedEntry`, `EntryMutation`, `EntryOutcome` |
| `config` | `config.rs` (612) | 16-key owner `SetConfig` domain with one disposition per key, typed parsers, confined artifact-name validation, whole-map atomic update | `AddressBookConfig`, `ConfigKey`, `LogLevel`, `parse_config_key` |
| `generation` | `generation.rs` (432) | Deterministic versioned JSON generation encode/decode with full re-validation on load | `encode_generation`, `decode_generation` |
| `subscription` | `subscription.rs` (250) | Bounded URL set, strict `hostname=destination` body ingestion, conditional validators | `SubscriptionSet`, `SubscriptionSource`, `validate_subscription_url`, `validate_subscription_validator`, `ingest_subscription_body` |
| `refresh` | `refresh.rs` (226) | Bounded refresh queue (one active + one coalesced pending) and the diagnostic outcome vocabulary | `RefreshQueue`, `RefreshReason`, `RefreshOutcome`, `RefreshDiagnostic` |
| `resolver` | `resolver.rs` (184) | Immutable snapshot capture and the shared read-only resolver | `AddressBookSnapshot`, `AddressBookResolver` |
| `destination_text` | `destination_text.rs` (134) | I2P-Base64 text decode plus exact `Destination::decode` structural validation | `validate_destination_text` |
| `hostname` | `hostname.rs` (132) | Canonical `.i2p` name validation (lowercase, suffix, labels, lengths) | `Hostname`, `parse_hostname` |
| `error` | `error.rs` (54) | Typed error enum carrying no caller material | `AddressBookError` |
| `lib` | `lib.rs` (54) | Crate doc, `forbid(unsafe_code)`, module declarations, the single `pub use` surface | (re-exports only) |

`tests/contract.rs` (116) is the only integration test file.

## Public surface

The complete `pub use` block, `src/lib.rs:30-54`:

- `book` — `AddressBook`, `AddressBookControl`, `BookKind`, `EntryMutation`,
  `EntryOutcome`, `Provenance`, `ResolvedEntry`
- `config` — `AddressBookConfig`, `ConfigKey`, `LogLevel`, `parse_config_key`,
  `MAX_CONFIG_PATH_LEN`, `MAX_CONFIG_VALUE_LEN`
- `destination_text` — `validate_destination_text`, `MAX_DESTINATION_TEXT_LEN`,
  `MAX_DESTINATION_BYTES`
- `error` — `AddressBookError`
- `generation` — `encode_generation`, `decode_generation`, `GENERATION_VERSION`,
  `MAX_ENTRIES_PER_BOOK`, `MAX_GENERATION_BYTES`
- `hostname` — `Hostname`, `parse_hostname`, `MAX_HOSTNAME_LEN`, `MAX_LABEL_LEN`
- `refresh` — `RefreshQueue`, `RefreshReason`, `RefreshOutcome`, `RefreshDiagnostic`
- `resolver` — `AddressBookResolver`, `AddressBookSnapshot`
- `subscription` — `SubscriptionSet`, `SubscriptionSource`,
  `ingest_subscription_body`, `validate_subscription_url`,
  `validate_subscription_validator`, `MAX_SUBSCRIBED_ENTRIES`,
  `MAX_SUBSCRIPTION_BODY_BYTES`, `MAX_SUBSCRIPTION_LINE_LEN`,
  `MAX_SUBSCRIPTION_URL_LEN`, `MAX_SUBSCRIPTION_URLS`,
  `MAX_SUBSCRIPTION_VALIDATOR_LEN`

Notable methods on the public types:

- `BookKind::name(self) -> &'static str` (`"private" | "local" | "router" |
  "published"`) and `BookKind::parse` (exact spelling, else
  `MalformedField`) — `book.rs:33,53`. `precedence_index` is `pub(crate)`.
- `AddressBook`: `new`, `control`, `lookup`, `list`, `book_len`,
  `subscribed_len`, `subscriptions`, `subscription_sources`, `config`,
  `revision` — all read-only (`&self`) except `control` (`&mut self`).
  `derived_table` and `set_revision` are `pub(crate)`.
- `AddressBookControl`: `apply_entry`, `replace_subscriptions`,
  `replace_derived`, `replace_subscription_sources`,
  `sync_published_from_router`, `apply_config`.
- `AddressBookSnapshot`: `capture`, `revision`, `entry_count`, `book_entries`,
  `subscription_urls`, `config_entries`.
- `AddressBookResolver`: `new`, `lookup`, `revision`, `snapshot`.
- `AddressBookConfig`: `rendered_entries` (canonical key order for
  persistence/generation encode) and `checked_update` (validate without
  mutating).
- `RefreshQueue`: `new`, `push`, `active`, `finish_active`, `take_active`,
  `is_idle`, `last_reason`. `RefreshDiagnostic::line()`.
- `Hostname`: `as_str`, plus `Display`. `SubscriptionSet`: `new`, `checked`,
  `urls`.
- `LogLevel`: `parse`, `name` (`off|error|warn|info|debug`).

Not re-exported (module-private): `MIN_REFRESH_INTERVAL_HOURS`,
`MAX_REFRESH_INTERVAL_HOURS`, `MIN_LOOKUP_TIMEOUT_SECS`,
`MAX_LOOKUP_TIMEOUT_SECS` (`config.rs:38-44`), the `confined_path` /
`parse_ranged_u64` / `validated_proxy_host` helpers, and every
`GenerationShape` / `ConfigShape` / `SubscriptionSourceShape` field.

## Key contracts

### Four books and the fixed precedence

`BookKind` has exactly four variants, declared in precedence order
(`book.rs:20-29`): `Private`, `Local`, `Router`, `Published`, with
`precedence_index` 0..=3 (`book.rs:43-50`). Lookup walks
Private → Local → Router → Published and then falls through to the
subscription-derived table (`book.rs:142-162`, mirrored in
`resolver.rs:92-113`). A `Provenance` of `Book(BookKind)` vs `Subscribed`
tells the caller which layer answered. The same hostname may live in several
books; lookup returns the first match. This exact order is pinned by
`book.rs:511` (`precedence_is_private_local_router_published_subscribed`),
which peels the layers one at a time and asserts each reveal.

### Mutation vs lookup is enforced by the type system, not by convention

Three signatures make the split structural:

- `AddressBook::control(&mut self) -> AddressBookControl<'_>` (`book.rs:134`)
  is the only way to obtain a mutator, and it borrows the book
  **exclusively for the guard's lifetime**. `AddressBookControl` holds
  `inner: &'a mut AddressBook` (`book.rs:242`) and every mutation method
  takes `&mut self` on the guard. There is no `&mut AddressBook` accessor
  outside the guard, so a mutation and a concurrent read cannot coexist
  through this type.
- `AddressBookResolver` holds `snapshot: Arc<AddressBookSnapshot>`
  (`resolver.rs:78`) and exposes only `lookup`, `revision`, and `snapshot()`
  — no mutation method exists on it, and the `AddressBook` it was captured
  from is not retained.
- `AddressBookSnapshot` fields are private with no mutating accessors
  (`resolver.rs:19-25`); it can only be produced by
  `AddressBookSnapshot::capture(&AddressBook)`, which takes `&AddressBook`.

So a consumer holding only a resolver handle is structurally incapable of
administrative change. Snapshots also isolate readers from later commits:
`resolver.rs:148` proves an outstanding handle still resolves a hostname
deleted afterwards, and that a fresh capture has a higher revision.

### Entry mutation is atomic

`apply_entry` (`book.rs:256`) validates fully before touching state:

1. `delete` plus a `destination` → `MixedShapes` (refuses to guess).
2. Otherwise `parse_hostname` runs first (`InvalidHostname` on failure).
3. `delete` removes from the target book only; absent hostname →
   `UnknownHostname`.
4. A non-delete without a destination → `MalformedField`; with a
   destination, `validate_destination_text` runs before the insert.
5. A **new** hostname is admitted only while `book.len() < ceiling`, where
   `ceiling = config.max_entries.clamp(1, MAX_ENTRIES_PER_BOOK)`; otherwise
   `BookFull`. Updating an existing entry is not capacity-checked.
6. Every committed mutation bumps the revision with `saturating_add`
   (`book.rs:213`).

Destination text is stored **verbatim** after validation — the decoded bytes
from `validate_destination_text` are bound and dropped (`book.rs:276`), so
there is no silent normalization and getters echo the committed text.

### Hostname validation (`hostname.rs`)

`parse_hostname` (`hostname.rs:35`), in order: non-empty and
`len <= MAX_HOSTNAME_LEN` (255) → ASCII-only → `to_ascii_lowercase` →
strip one optional trailing `.` (re-check empty and length) → require the
`.i2p` suffix with a non-empty body → every label non-empty,
`len <= MAX_LABEL_LEN` (63), no leading or trailing `-`, and only
`a-z 0-9 -`. The stored form is `{body}.i2p`, lowercase, no trailing dot.
`UPPER.i2P.` therefore canonicalizes to `upper.i2p` (case is folded, not
rejected). Non-`.i2p` names — operator aliases, `localhost`, IP literals,
`example.com` — are rejected here; they are not address-book names.

### Full-Destination validation (`destination_text.rs`)

`validate_destination_text` (`destination_text.rs:21`) checks the
**I2P-Base64** alphabet `A-Z a-z 0-9 - ~` with `=`: non-empty, within
`MAX_DESTINATION_TEXT_LEN` (4096) bytes, total length a multiple of four,
padding confined to the final two characters with no data after it. It then
translates I2P-Base64 `-`→`+` and `~`→`/` to RFC 4648 and hands the text to
the reviewed `base64ct` decoder — no local codec. The decoded bytes must be
non-empty, within `MAX_DESTINATION_BYTES` (4096), and accepted by
`i2pr_proto::Destination::decode(&decoded, MAX_DESTINATION_BYTES)`
(`destination_text.rs:63`), which is an exact-consumption decode under the
workspace algorithm policy, so trailing garbage fails. Success returns the
decoded bytes; the caller stores the original text.

### The `SetConfig` domain (`config.rs`)

`ConfigKey` has **16** variants (`config.rs:48-81`), each with an exact
`name()` and a `parse_config_key` arm; unknown keys are
`UnknownConfigKey`. The Proposal 170 wire has 13 keys — the owner has three
more: `should_publish`, `etags`, `last_modified`. The thirteen-line
`key_inventory_matches_contract_order` unit test (`config.rs:591`) still
asserts the 13-wire-key subset, while `tests/contract.rs:29-47` pins all 16
rendered owner keys.

Dispositions, one per key:

| Key | Disposition / parser |
| --- | --- |
| `private_book`, `local_book`, `router_book`, `published_book`, `subscriptions`, `log_file`, `etags`, `last_modified` | Confined logical artifact name (`ConfigKey::is_path_like`, `config.rs:107`) |
| `refresh_interval` | integer hours, `MIN..=MAX_REFRESH_INTERVAL_HOURS` = 1..=720 |
| `proxy_host` | loopback **IP literal** only (IPv4 or IPv6), no DNS, non-empty clears to `None` |
| `proxy_port` | `1..=65535`, empty clears to `None` |
| `theme` | inert stored string, round-trip only |
| `log_level` | `LogLevel` (`off`/`error`/`warn`/`info`/`debug`), artifact verbosity only |
| `lookup_timeout` | integer seconds, `1..=300` |
| `max_entries` | per-book ceiling, `1..=MAX_ENTRIES_PER_BOOK` (1000) |
| `should_publish` | exact `"true"` / `"false"` only |

`checked_update` (`config.rs:294`) clones `self`, validates every key and
value, and returns the new configuration without mutating — so
`apply_config` (`book.rs:400`) can swap atomically. Two extra whole-map
rules beyond per-key parsing:

- All eight artifact names must be **mutually distinct** (`BTreeSet` check,
  `config.rs:369-379`); a collision is `InvalidConfigValue`.
- A tightened `max_entries` that is below the live entry count of any book is
  rejected with `InvalidConfigValue` (`book.rs:406-415`).

`confined_path` (`config.rs:388`) rejects: empty, `> MAX_CONFIG_PATH_LEN`
(255), any byte `< 0x20`, `0x7f`, `/`, `\\`, or NUL (flat namespace, so
absolute, drive, and UNC forms are all excluded), `.` and `..`
(`PathEscape`), any leading `.` (hidden-file impersonation), and the two
reserved generation filenames `addressbook.current.json` /
`addressbook.backup.json` (owned exclusively by the storage adapter). Any
value longer than `MAX_CONFIG_VALUE_LEN` (1024) is rejected before the key is
even parsed.

`parse_ranged_u64` is ASCII-digit-only, so `12h`, `" 12"`, `+12`, `-1` all
fail.

### Bounded subscription ingestion (`subscription.rs`)

- `SubscriptionSet::checked` (`subscription.rs:68`) rejects more than
  `MAX_SUBSCRIPTION_URLS` (16) URLs with `TooManySubscriptions`, validates
  every URL, and deduplicates while preserving request order.
- `validate_subscription_url` (`subscription.rs:93`) requires an
  `http://` or `https://` prefix matched case-insensitively, rejects any
  byte `<= 0x20` or `0x7f`, rejects empty host, `@` (userinfo), and any host
  that does not end in `.i2p` after an optional all-digit port is stripped.
  Requiring an I2P host is deliberate: it prevents an eepProxy outproxy
  from becoming a clearnet subscription capability.
- `ingest_subscription_body` (`subscription.rs:123`) rejects a body over
  `MAX_SUBSCRIPTION_BODY_BYTES` (1 MiB) and a single line over
  `MAX_SUBSCRIPTION_LINE_LEN` (8192) with `BodyOverBound`; splits on `\n`
  and strips a trailing `\r`; skips empty lines and `#` comments; requires
  exactly one `=` per line via `split_once('='`) (else `MalformedField`);
  bounds the name by `MAX_HOSTNAME_LEN` and the value by
  `MAX_DESTINATION_TEXT_LEN`; validates both. **Any** invalid line rejects
  the whole body. Duplicates resolve last-wins, deterministically, and the
  entry count is capped at `MAX_SUBSCRIBED_ENTRIES` (= 1000) with
  `ListOverBound` — an existing key may still be updated at the cap.
- `validate_subscription_validator` (`subscription.rs:51`) bounds an ETag or
  Last-Modified value at `MAX_SUBSCRIPTION_VALIDATOR_LEN` (1024) and rejects
  control bytes, so no raw header value can cross into persisted state.

### Derived entries are replaced wholesale, never merged

`replace_derived` (`book.rs:317`) swaps the whole derived table and clears
the per-URL sources, so operator books are untouched and an operator
deletion cannot be resurrected by a later download.
`replace_subscription_sources` (`book.rs:338`) requires the source map to
cover exactly the configured URL set (else `InvalidSubscription`), revalidates
every entry, both validators, per-source and merged counts, then re-derives
the table in **URL order — later sources win** (`merge_subscription_sources`,
`book.rs:224`). `replace_subscriptions` (`book.rs:296`) swaps the URL set,
prunes sources for URLs that left the set, and re-derives.

`sync_published_from_router` (`book.rs:383`) is a no-op unless
`should_publish` is enabled; when enabled it copies the router book over the
published book wholesale. Local and private are never published.

### Refresh is composition, not execution

This is the honest reading of `refresh.rs`. The crate owns only the capacity
discipline and the outcome vocabulary:

- `RefreshQueue` (`refresh.rs:44`) holds at most one **active** set plus one
  **coalesced pending** set. `push` returns `true` when the caller should
  start fetching (queue was idle) and `false` when the set coalesced behind
  an active fetch; a newer commit overwrites the pending slot, so the queue
  can never grow. `take_active` / `finish_active` are the worker's drain
  primitives; `finish_active` promotes the coalesced pending set and returns
  it. `is_idle` and `last_reason` are the observable state.
- `RefreshReason` is `SubscriptionsReplaced | IntervalElapsed | Manual`.
- `RefreshOutcome` is `Committed { ingested, changed } | DownloaderUnavailable
  | IngestRejected | FetchFailed`. `DownloaderUnavailable` exists because no
  downloader owner is wired in this crate — the daemon reports it.
- `RefreshDiagnostic { reason, outcome, url_count }` renders a bounded
  artifact line via `line()` (`refresh.rs:130`). It carries a **URL count,
  never the URLs**, so no URL or body reaches the artifact by construction
  (`refresh.rs:206`).

Timers, the cadence sleep, the fetch, the ingestion commit, and the
timestamped artifact line all belong to the daemon's
`addressbook-refresh` service (`crates/i2pr-daemon/src/lib.rs:700-745`), which
drives this queue.

### Deterministic versioned generations (`generation.rs`)

A generation is one complete bounded state: `version`, `revision`, the four
books, the subscription URL list, the derived table, the per-URL
`subscription_sources`, and the typed config. Determinism comes from three
things: a fixed struct field order (`GenerationShape`, `generation.rs:29`),
`BTreeMap`/`BTreeSet` ordering throughout, and `serde_json::to_vec` of
in-memory state only. `generation.rs:327` asserts
`encode(book) == encode(book)`, and `generation.rs:337` asserts
`encode(decode(bytes)) == bytes`.

`GENERATION_VERSION` is `1` and `decode_generation` rejects anything else
fail-closed (`generation.rs:157`). Decoding is **full re-validation, not
trust**: config is applied first (so the stored `max_entries` governs book
admission), then every entry is re-admitted through `apply_entry` — the same
validators a live mutation runs — then subscriptions, then sources. Any
failure maps to `InvalidGeneration` and rejects the whole generation, so a
corrupt or hostile file can never activate partially. The byte ceiling is
`MAX_GENERATION_BYTES` (24 MB) and the per-book count ceiling is
`MAX_ENTRIES_PER_BOOK` (1000). The persisted `revision` is restored exactly at
the end via the `pub(crate) set_revision` so later mutations bump from it.

There is a consistency check worth naming: when `subscription_sources` is
present, the re-derived table must equal the stored `subscribed` map exactly
(`generation.rs:267-274`) — entries and conditional validators cannot
disagree with the flat table. When it is absent, the stored `subscribed` map
is ingested as a whole via `replace_derived`.

### Error variants (`error.rs`)

`AddressBookError` is `Clone + Copy + Debug + Eq + PartialEq + Error`, with
exactly 14 variants and **no payload** — no hostname, destination, URL, or
config value ever enters an error, so a control surface can render one
verbatim with no redaction pass (`error.rs:1-5`):

`InvalidHostname`, `InvalidDestination`, `UnknownHostname`, `BookFull`,
`MixedShapes`, `MalformedField`, `InvalidSubscription`,
`TooManySubscriptions`, `BodyOverBound`, `ListOverBound`,
`UnknownConfigKey`, `InvalidConfigValue`, `PathEscape`, `InvalidGeneration`.

No `anyhow` anywhere in the crate.

### Bound constants (all values verified in source)

| Constant | Value | Declared at |
| --- | --- | --- |
| `MAX_HOSTNAME_LEN` | 255 | `hostname.rs:13` |
| `MAX_LABEL_LEN` | 63 | `hostname.rs:15` |
| `MAX_DESTINATION_TEXT_LEN` | 4096 | `destination_text.rs:15` |
| `MAX_DESTINATION_BYTES` | 4096 | `destination_text.rs:17` |
| `MAX_CONFIG_VALUE_LEN` | 1024 | `config.rs:34` |
| `MAX_CONFIG_PATH_LEN` | 255 | `config.rs:36` |
| `MIN_REFRESH_INTERVAL_HOURS` | 1 (crate-private) | `config.rs:38` |
| `MAX_REFRESH_INTERVAL_HOURS` | 720 (crate-private) | `config.rs:40` |
| `MIN_LOOKUP_TIMEOUT_SECS` | 1 (crate-private) | `config.rs:42` |
| `MAX_LOOKUP_TIMEOUT_SECS` | 300 (crate-private) | `config.rs:44` |
| `GENERATION_VERSION` | 1 | `generation.rs:21` |
| `MAX_ENTRIES_PER_BOOK` | 1000 | `generation.rs:23` |
| `MAX_GENERATION_BYTES` | 24_000_000 | `generation.rs:25` |
| `MAX_SUBSCRIPTION_URLS` | 16 | `subscription.rs:18` |
| `MAX_SUBSCRIPTION_URL_LEN` | 2048 | `subscription.rs:20` |
| `MAX_SUBSCRIPTION_BODY_BYTES` | 1_048_576 | `subscription.rs:22` |
| `MAX_SUBSCRIBED_ENTRIES` | `MAX_ENTRIES_PER_BOOK` (1000) | `subscription.rs:24` |
| `MAX_SUBSCRIPTION_LINE_LEN` | 8192 | `subscription.rs:26` |
| `MAX_SUBSCRIPTION_VALIDATOR_LEN` | 1024 | `subscription.rs:48` |

Defaults: refresh 24 h, lookup timeout 30 s, `max_entries` 1000,
`should_publish` false, `log_level` `Warn`, artifacts
`private.json` / `local.json` / `router.json` / `published.json` /
`subscriptions.body` / `addressbook.log` / `subscriptions.etags` /
`subscriptions.last-modified` (`config.rs:217-240`).

## Dependencies

Production dependencies (`crates/i2pr-addressbook/Cargo.toml`): `i2pr-proto`
(path), plus workspace `base64ct`, `serde`, `serde_json`, `thiserror`.
Exactly one workspace dependency: `i2pr-proto`.

Why `i2pr-proto` is needed: destination validation is not re-implemented
here. `validate_destination_text` defers the structural half to
`i2pr_proto::Destination::decode` (`crates/i2pr-proto/src/common/identity.rs:314`),
which is the single exact-consumption decoder under the workspace algorithm
policy. Duplicating it here would create a second, weaker Destination parser.
`base64ct` supplies the reviewed Base64 decoder for the alphabet translation;
`serde`/`serde_json` are the generation codec; `thiserror` derives the
material-free error enum.

Dev-dependency: `i2pr-i2pcontrol` (path) only. It is used by
`tests/contract.rs` to pin contract parity against the frozen Proposal 170
inventory — `BOOK_TYPES`, `SET_CONFIG_KEYS`, `ADDRESS_BOOK_FIELDS`, and the
shared ceilings (`MAX_HOSTNAME_LEN`, `MAX_DESTINATION_LEN`,
`MAX_SUBSCRIPTION_URL_LEN`, `MAX_SUBSCRIPTION_URLS`) plus the wire
`is_path_like_config_key` / `is_inert_config_key` classifiers. A dev-dependency
keeps the frozen contract crate out of the production graph, so this crate
never depends on the wire contract it exists to back.

Boundary enforcement: `scripts/check-dependency-direction.sh:65` allows
`i2pr-addressbook` exactly `{"i2pr-proto"}`, and
`docs/architecture/dependency-graph.md:33,84` mirrors it. Only `i2pr-daemon`
appears as a consumer (`check-dependency-direction.sh:35`). Reverse
consumers in the tree are `crates/i2pr-daemon/{src,tests}` only; no other
crate depends on it.

## Tests

`cargo test --locked -p i2pr-addressbook -- --test-threads=1` on this checkout:
**29 in-crate unit tests passed, 5 integration tests passed, 0 doc tests.**

In-crate units by module: `book` 6, `config` 6, `destination_text` 4,
`generation` 3, `hostname` 3, `refresh` 3, `resolver` 1, `subscription` 3.
No RNG, no clock, no network, no filesystem: every test is deterministic and
builds its own destination fixture in-crate (384 zero bytes plus a 7-byte
type-5 Ed25519/X25519 key certificate, re-mapped to I2P-Base64).

What they prove:

- Hostname matrix: six canonical forms pass, eleven non-names rejected (a
  twelfth case, `UPPER.i2P.`, is folded to `upper.i2p`), label-63 accepted /
  label-64 rejected, 255-byte ceiling enforced.
- Destination: minimal 391-byte destination round-trips; eight malformed
  texts rejected; short-but-decodable and truncated-certificate payloads
  rejected; oversized text rejected.
- Book CRUD, `Created`/`Updated`/`Deleted` outcomes, delete-plus-destination
  → `MixedShapes`, second delete → `UnknownHostname`, the full
  layer-by-layer precedence reveal, duplicate names across books, revision
  unchanged after two invalid requests, and exact book-name parsing.
- Config: whole-map validation before mutation (one bad value fails the
  receiver), `should_publish`/`etags`/`last_modified` typing, twelve
  rejected artifact names, numeric bounds per key, loopback-only proxy hosts
  with clear-on-empty, 13-wire-key inventory parse.
- Subscription: scheme/host matrix, `MAX_SUBSCRIPTION_URLS + 1` rejection,
  order-preserving dedup, four rejected bodies, body and line ceilings.
- Generation: round-trip with byte-for-byte re-encode, entries plus
  validators in one generation, and five hostile generations (wrong version,
  truncated, non-JSON, over the byte ceiling, smuggled `evil..i2p` entry).
- Refresh: newest-pending coalescing, `take_active`/`finish_active` pairing,
  and a diagnostic line asserted not to contain `http`.
- Resolver: snapshot isolation from later commits plus a higher revision on
  fresh capture.

`tests/contract.rs` (5 tests) is the cross-crate parity gate: book inventory
and order, all 16 rendered owner keys against the frozen wire key set,
four shared ceilings, path/inert classification agreement, and the six frozen
`ADDRESS_BOOK_FIELDS`.

Bounded negative paths are covered at each bound: URL count, URL length, body
bytes, line length, entry count, hostname length, label length, destination
text length, destination bytes, generation bytes, config value length,
artifact path length, and every numeric range endpoint (`0`, `1`, `720`,
`721`, `300`, `301`, `1000`, `1001`, `65535`, `65536`).

## Distinctive design choices

1. Mutation and lookup are separated by ownership, not convention:
   `AddressBook::control(&mut self)` is the only route to a mutator and the
   `Arc<AddressBookSnapshot>` in the resolver admits no mutation at all.
2. Generation decode re-admits every entry through the live mutation path
   (`apply_entry`) rather than trusting parsed JSON, so a hostile file cannot
   activate partially.
3. Destination text is stored verbatim after validation; the decoded bytes
   are dropped, so getters echo exactly what was committed.
4. Derived subscription entries are replaced wholesale and live outside the
   four books, so operator deletions never resurrect from a later download.
5. The refresh queue is capacity-only: one active set plus one coalesced
   pending set, with `DownloaderUnavailable` as a first-class outcome because
   no downloader owner exists in this crate.
6. Refresh diagnostics carry a URL **count**, never URLs or bodies, so
   nothing sensitive reaches the artifact by construction.
7. Logical artifact names form a flat, distinct, reserved namespace: no
   separators, no dotfiles, no `..`, and `addressbook.current.json` /
   `addressbook.backup.json` are refused so a config key can never address
   durable state.
8. The owner has 16 config keys to the wire's 13: `should_publish`, `etags`,
   and `last_modified` are owner-side additions, and the contract test pins
   the mapping so the difference stays deliberate.
9. Subscription URLs must resolve to an `.i2p` host, so an eepProxy outproxy
   can never silently become a clearnet subscription capability.
10. `proxy_host` accepts only a loopback IP literal — no DNS, no general
    proxy capability — and the value validates, stores, and round-trips even
    though no in-crate consumer exists; that is a stored policy value, not a
    capability claim.

## Cross-references

ADRs:

- [ADR 0001 — Modular monolith and crate-boundary strategy](../adr/0001-modular-monolith.md)
- [ADR 0002 — Tokio at runtime-facing boundaries](../adr/0002-tokio-runtime-boundary.md) — why this crate has no runtime
- [ADR 0028 — Proposal 170 as the parallel router control-plane contract](../adr/0028-i2pcontrol-proposal-170-control-plane.md) — decides that one canonical address-book owner backs both ordinary naming and Proposal 170
- [ADR 0029 — Anonymity boundaries and profile convergence](../adr/0029-anonymity-boundaries-and-profile-convergence.md) — §2 canonical remote naming uses the resolved Destination; local aliases stay local state

ADR 0003 (bounded supervised services) concerns the daemon's supervised
services, not this crate; the refresh service that drives this queue is a
consumer, not a part of it.

Plans and specs:

- Plan 294 — `plans/implementation/i2pcontrol-proposal-170/294-canonical-addressbook-and-resolver-integration.md`, closure `plans/closure/i2pcontrol-proposal-170/294-status.md` (status token `passed-prop170-canonical-addressbook-and-resolver-integration`)
- Plan 295 — `plans/closure/i2pcontrol-proposal-170/295-status.md` (source completion; the 13-vs-16 `SetConfig` set difference is frozen for this adjudication)
- Plan 321 — `plans/closure/i2pcontrol-proposal-170/321-status.md` (exact canonical 13-key wire projection onto the typed owner config)
- Spec dossier — `specs/protocols/15-canonical-addressbook.md`
- Support surface — `specs/support.toml` surface `prop170.canonical-addressbook-and-resolver-integration` (`advertised = false`)

Related deep dives:

- [Overview](overview.md)
- [Dependency graph](dependency-graph.md)
- [i2pr-proto](i2pr-proto.md) — owns `Destination::decode`, the single exact-consumption decoder this crate validates against
- [i2pr-i2pcontrol](i2pr-i2pcontrol.md) — the frozen Proposal 170 wire contract this crate backs (dev-only dependency, parity-pinned by `tests/contract.rs`)
- [i2pr-storage](i2pr-storage.md) — the opaque generation byte store; it never parses what this crate encodes
- [i2pr-daemon](i2pr-daemon.md) — the address-book manager, refresh worker, download composition, and consumer wiring
