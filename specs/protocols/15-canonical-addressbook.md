# Canonical AddressBook (Plan 294)

Status: Plan 294 implementation record. One canonical
`i2pr-addressbook` owner drives ordinary i2pr naming (SAM, HTTP,
SOCKS, IRC/service-tunnel references) and Proposal 170
administrative/getter behavior with atomic persistence and bounded
refresh composition.

## Precedence

`private`, then `local`, then `router`, then `published`, then
subscription-derived entries. The same hostname may intentionally
appear in multiple books; lookup uses the first match. Static
operator aliases always precede the address book; session-registry
(`ME`) and Base32 decoding precede it on the SAM path.

## Names and values

Hostnames canonicalize to lowercase ASCII with one optional
trailing dot stripped and a mandatory `.i2p` suffix; labels are
`a-z 0-9 -` without empties or edge hyphens, total within 255
bytes. Non-`.i2p` names are not address-book names. Values are
full-Destination text: I2P-Base64 (`-`/`~`, `=` padding only in the
final quantum, multiple of four, within 4096 bytes) decoding to
bytes that `Destination::decode` accepts exactly under the
workspace algorithm policy. Validation never logs or echoes
material; errors are opaque by construction.

## Entry operations

One mode per request: entry operation, subscription replacement, or
config replacement. Mixed modes fail whole with no partial effect.
Without `Delete`, the entry is added or replaced (both validated
first). `Delete` presence selects deletion regardless of its
value; combining delete with a destination fails the request, and
deleting a missing hostname fails deterministically. Every
mutation bumps a saturating revision.

## Subscriptions

`SetSubscriptions` replaces the complete bounded set (at most 16
HTTP/HTTPS URLs with hosts, 2048 bytes each, deduplicated,
order-preserving). Bodies ingest as `hostname=destination` lines
(`#` comments and blanks skipped, 1 MiB body cap, 8192-byte line
cap, 1000-entry cap): every line validates, any invalid line fails
the whole body, duplicates resolve last-wins. Derived entries live
outside the four books and replace wholesale, so operator
deletions never resurrect from a later download. No downloader
owner exists yet: refresh attempts report unavailable (never a
socket, never history-rewriting); ingestion stays reachable
through the tested seam for bodies obtained by other means.

## Configuration (thirteen keys)

`private_book`, `local_book`, `router_book`, `published_book`,
`subscriptions`, `log_file` are confined logical artifact names
(flat namespace: no separators, no absolute forms, no parent or
hidden-dot tricks, no control bytes); they select
AddressBook-owned snapshots, the staged-download artifact, and the
diagnostic artifact — never arbitrary filesystem paths.
`refresh_interval` is integer hours 1..=720 and drives the worker
cadence. `proxy_host`/`proxy_port` configure only the bounded
subscription fetch path (validated, stored, round-tripped for the
path to consume when a downloader owner exists; no general proxy
capability). `theme` is inert metadata (durable round-trip only).
`log_level` (`off`/`error`/`warn`/`info`/`debug`) gates only the
diagnostic artifact, never global tracing. `lookup_timeout` is
integer seconds 1..=300 bounding the fetch stages once a
downloader exists. `max_entries` (1..=1000) is enforced on
mutation, import, and configuration tightening. Whole-map
validation precedes every swap; unknown keys fail untouched.

## Refresh composition

At most one active refresh plus one newest pending set; commits
coalesce. The cadence worker wakes per committed interval and
drains the chain (attempt, diagnostic, promote). The diagnostic
artifact holds bounded outcome lines (URL counts, never URLs;
64 KiB ceiling with tail retention).

## Persistence

One state directory with fixed filenames:
`addressbook.current.json` (latest) and `addressbook.backup.json`
(previous). Publication writes, syncs, rotates current over
backup, and installs atomically; failure leaves the prior current
untouched. Loads reject symlinks, non-regular files, permissive
modes, and over-ceiling sizes. Generations are deterministic JSON
(version 1 only) re-validating everything on decode.

## Activation rule

Disabled (default): the subsystem never touches the filesystem
and publishes nothing; SAM naming, static aliases, and getters
behave exactly as before. Enabled: load current, then backup (a
present-but-invalid current falls back to the backup); with
neither file, bounded per-book snapshot artifacts import when
present (any invalid artifact fails activation) and the result
publishes. Corrupt current and backup fail into an inactive
sticky-error state: getters report unavailable-with-reason and
resolvers stay unpublished (legacy naming), never partial or
fabricated state.

## First-activation import

Per-book snapshot artifacts are JSON objects mapping hostname to
destination text, read once when no generation exists. Missing
files mean empty books. There are no legacy sources to migrate:
i2pr has no pre-existing address-book files.

## Getters

All six selectors return maps over the published snapshot:
books as hostname-to-destination maps (empty object when empty),
subscriptions as a URL-list object, config as the thirteen-key
map. Collection ceilings re-check defensively at serialization
(violation gaps, never truncation). Selectors read the same
committed generation that lookup uses.

## Isolation

When I2PControl runs without the subsystem (standalone
construction or disabled/inactive manager), the `AddressBook`
method fails explicitly with the Plan 294 marker and getters gap
with the 294 owner — no silent override, no stale-file influence
(the disabled manager never reads control files).

## Carried to Plan 295

The frozen thirteen (`private_book`, `local_book`,
`router_book`, `published_book`, `subscriptions`,
`refresh_interval`, `proxy_host`, `proxy_port`, `theme`,
`log_file`, `log_level`, `lookup_timeout`, `max_entries`) differ
as a set from the reference fork's M096 thirteen (see Plan 293
spec 14 §Items-carried). Plan 294 builds against the frozen
inventory; Plan 295 adjudicates the divergence against the pinned
Proposal before any final support claim.
