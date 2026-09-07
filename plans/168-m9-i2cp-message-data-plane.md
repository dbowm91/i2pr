# Plan 168 — Milestone 9 I2CP message data plane

Status: **blocked until Plan 167 passes**.

## 1. Goal

Connect the real I2CP session product to the existing destination routing/data plane for bounded application-message send/receive, status reporting, destination lookup, and bandwidth queries.

This pass does not implement Streaming-over-I2CP as a separate stack. It carries I2CP application payloads using the existing destination/garlic routing path.

Expected closure:

```text
plan_168 = passed-m9-i2cp-message-data-plane
next_executable_plan = 169
```

## 2. Outbound SendMessage / SendMessageExpires

Implement the selected profile's exact structural/semantic handling for both message families.

Validation before routing:

- owning SessionID is active and belongs to the TCP connection;
- destination decodes/validates strictly;
- payload/body stays within I2CP and destination payload ceilings;
- expiration is within sane protocol/current-time bounds and not already expired;
- flags contain only supported/defined-ignore bits;
- protocol/source/destination ports are validated as the payload format requires;
- nonce/message ID fields are bounded and tracked without unbounded maps.

Then adapt to the existing `i2pr-client::DestinationRouting` / outbound ECIES/garlic path. Do not duplicate remote LeaseSet lookup, lease selection, garlic construction, or delivery queues.

## 3. Payload format

Implement the Plan 164-pinned I2CP payload/GZIP metadata format with strict bounds.

Requirements:

- source port, destination port, xflags, and I2P protocol round-trip exactly;
- CRC/integrity handling follows the pinned profile;
- compressed input cannot expand beyond `MAX_DESTINATION_PAYLOAD_BYTES` or the smaller M9 ceiling;
- malformed gzip/metadata fails before routing;
- encoding is deterministic for fields under i2pr control where the spec requires it;
- no arbitrary decompression allocation based on advertised size.

For M9 independent evidence, use a message protocol directly supported by the clients (raw/datagram-style application messages are preferred). Do not make external-client closure depend on I2P Streaming interoperability debt from M6.

## 4. MessageStatus semantics

Create one documented mapping from actual router outcomes to I2CP status codes.

Distinguish at least:

```text
rejected-before-queue
accepted/queued-locally
routing/lookup failure
destination unavailable/expired
resource/backpressure failure
expired-before-send
stronger delivery status only if actually observed by existing destination machinery
```

Do not translate “accepted into local queue” into end-to-end success.

Nonce correlation:

- bounded pending-status table per session;
- explicit count/byte/time ceiling;
- terminal status removes state;
- session teardown clears state;
- duplicate/late internal events are idempotent and cannot resurrect an entry.

`i2cp.messageReliability` from Plan 165 controls whether/which statuses are promised. Reject unsupported stronger semantics instead of lying.

## 5. Inbound MessagePayload

Route authenticated/decrypted inbound destination payloads to the owning client-owned I2CP session.

Requirements:

- destination ownership lookup is exact;
- payload is accepted only after existing ECIES/garlic authentication;
- decode/validate I2CP payload metadata within bounds;
- enqueue into the owning connection's bounded output queue;
- no cross-session/cross-connection leakage;
- slow/nonreading client triggers bounded backpressure policy;
- payload bytes are not logged or stored in acceptance artifacts.

M9 baseline uses fast receive / `MessagePayload`. If deprecated ReceiveMessageBegin/End is not supported, the behavior must match the profile declared in Plan 164/165 rather than silently switching modes.

## 6. Local cross-session delivery

Where both destination hashes are owned by active i2pr client destinations, preserve the existing local destination routing shortcut if it is part of the authoritative product path. The I2CP adapter should not special-case bytes around the destination layer.

Tests must prove local shortcut and normal destination-routing interfaces produce the same I2CP-visible payload/status semantics.

## 7. Destination lookup

Implement `DestLookup` / `DestReply` through existing local destination/NetDB lookup capabilities.

Policy:

- a complete Destination lookup is bounded and typed;
- locally owned destinations may resolve through the local registry;
- remote destination lookup uses existing NetDB/destination lookup seams only where currently available;
- no system DNS;
- no new address book;
- not-found is protocol-correct and time-bounded;
- malformed hashes/Destinations fail explicitly.

If selected clients require HostLookup/HostReply, add the smallest equivalent adapter over the same safe lookup surface; human-readable address-book resolution remains unsupported unless an existing router component already owns it.

## 8. Bandwidth limits

Implement `GetBandwidthLimits` / `BandwidthLimits` from existing configured/resource-governor information.

Do not invent measured bandwidth values.

Where the protocol defines a field that i2pr does not currently know, return the spec-defined neutral/zero value and document it. Values that are configuration ceilings should be derived from actual router configuration snapshots.

This is informational; client input must not mutate router-wide bandwidth policy in M9.

## 9. Send flags and Proposal 171

Validate standard SendMessageExpires flags according to the pinned profile.

- obsolete tag-count/threshold fields must follow current ECIES-ratchet/reference behavior;
- `NO_LEASESET` or equivalent supported flags must map only if the existing garlic path can honor them correctly;
- Proposal 171 outbound tunnel-switch bit remains draft/deferred; ignore it only if the current draft/spec requires routers that do not implement the feature to ignore it;
- unknown reserved bits are rejected or ignored exactly according to normative semantics, not ad hoc.

No outbound tunnel-switch feature is implemented here.

## 10. Backpressure and resource policy

Name explicit ceilings for:

- pending outbound I2CP messages per session;
- pending status correlations;
- pending inbound MessagePayload frames;
- aggregate pending bytes per connection/session;
- concurrent destination lookups;
- lookup deadline;
- message expiration horizon.

These must compose with, never exceed, existing destination/router ceilings.

A client cannot use many small I2CP frames to bypass `MAX_PENDING_DESTINATION_MESSAGES` or aggregate-byte accounting.

## 11. Tests

Real loopback I2CP tests must cover:

- two active client-owned sessions exchange one small payload each direction;
- near-maximum accepted payload each direction;
- max+1 rejected without routing;
- source/destination port and protocol metadata round-trip;
- valid SendMessage and SendMessageExpires;
- expired message rejection;
- unsupported flag/reliability behavior;
- local queue acceptance vs actual failure status mapping;
- duplicate/late status events;
- pending-status max/max+1 and cleanup;
- inbound payload delivered only to owning connection;
- slow reader boundedness with sibling session unaffected;
- destination lookup local hit, remote/not-found, malformed, timeout;
- bandwidth reply derives documented real/neutral values;
- malformed gzip/CRC/metadata and decompression-bomb ceiling;
- disconnect with pending sends/status/lookups restores baseline.

After listener/session setup, behavior-driving interactions in black-box tests remain TCP/I2CP only.

Focused commands:

```text
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
```

plus M6/SAM regression seams and the Plan 163 workspace floor.

## 12. Documentation

Update:

- I2CP dossier with payload/status/lookup/bandwidth semantics;
- `docs/architecture/i2pr-client.md` and daemon/API docs for the adapter path;
- CONFORMANCE/support matrix with exact message families now implemented;
- known limitations for deprecated receive, address-book/HostLookup, flags, and delivery-status strength.

## 13. Acceptance criteria

Plan 168 closes only when:

1. SendMessage/Expires validates session, payload, expiration, and flags before routing;
2. outbound traffic reuses existing destination routing/ECIES/garlic paths;
3. I2CP payload encoding/decoding is bounded against expansion and malformed metadata;
4. status mapping distinguishes local acceptance from stronger delivery and is documented;
5. pending status/lookup/message state has count/byte/time ceilings and deterministic cleanup;
6. authenticated inbound destination payload reaches only the owning I2CP session;
7. slow clients cannot create unbounded output retention or harm a sibling session;
8. destination lookup uses existing local/NetDB seams and never system DNS/new address book;
9. bandwidth replies use real configuration/neutral spec values, not fabricated metrics;
10. unsupported/draft flags are handled exactly per declared profile;
11. bidirectional small and near-limit real-TCP local message exchange passes;
12. M6 client and M7 SAM regressions remain green;
13. workspace floor and exact-head routine CI pass;
14. no independent-client or public-network claim is made yet;
15. `plans/168-status.md` records exact evidence.

## 14. Stop conditions

Stop if message delivery would require bypassing the existing destination routing layer, if status semantics cannot honestly satisfy a selected client reliability mode, or if independent lookup requires adding an address-book subsystem. Record the limitation/corrective rather than expanding M9 silently.

## 15. Handoff

After Plan 168 passes, execute Plan **169**.