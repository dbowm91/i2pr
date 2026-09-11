# Plan 186 status — M6 mixed-router RouterInfo NetDB lookup and publication

Status: **`passed-m6-mixed-router-netdb-lookup-and-publication`**.

Plan of record:
[`plans/186-m6-mixed-router-netdb-lookup-and-publication.md`](186-m6-mixed-router-netdb-lookup-and-publication.md).

## Current authority

```text
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_183 = registered-m6-mixed-router-streaming-interop-program
m6_authenticated_i2np_preflight = passed-via-plan184
m6_exploratory_one_hop_tunnels = passed-via-plan185
m6_netdb_lookup_publication = passed-via-plan186
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 187
```

## What landed

Strict Plan 186 §3–§8 delivery, no M6 wire change.

```text
crates/i2pr-daemon/src/netdb_tunnels.rs (new)
  §3 authoritative store: one bounded RouterInfoStore populated only
  by validated signed records; live lookup never consults the Plan 122
  placeholder empty store. `bootstrap_reference_router_info` traverses
  the same RouterInfo::decode + ValidatedRouterInfo::from_router_info
  parser/signature/freshness path as ordinary records before
  eligibility. No public reseed.
  §4 floodfill profile: `is_floodfill` / `floodfill_count` /
  `candidate_hashes` verify the effective RouterInfo capability before
  dispatch; `begin_tunnel_lookup` fails closed with
  NoEligibleCandidates when the authoritative store carries no
  eligible candidate (honest termination, never spin).
  §5 lookup trajectory: `begin_tunnel_lookup` (authoritative store +
  real inbound reply path -> existing LookupAction::SendDatabaseLookup)
  + `compose_lookup_via_tunnel` (existing outbound composition ->
  TunnelData cells over the Plan 185 path, first-hop + floodfill +
  key proof) + `ingest_tunnel_store` (TunnelData recovery ->
  inbound_dispatch / NetDbSeam -> signed validation -> ordinary store
  -> terminal success). `note_direct_transport_attempt` always rejects
  so direct SSU2 DatabaseLookup is never a counted row.
  §6 publication trajectory: `register_local` (normal i2pr identity
  code, never re-signed per retry) + `begin_publication` (existing
  floodfill selection/routing-key logic, peer must be eligible) +
  `compose_publication_via_tunnel` (existing outbound composition,
  store key must remain local) + `correlate_publication_status`
  (protocol-derived DeliveryStatus ack) + mark/retry/cancel bounded
  surface. Duplicate/expired/invalid publication failure stays bounded.
  §7 retries/search: `ingest_search_reply` merges through the bounded
  policy (unknown peers never cause unbounded work);
  mismatch/stale/invalid -> Continue (bounded counters);
  malformed -> typed Malformed; duplicate after success -> bounded
  UnknownLookup (no spin); deadline via `expire_lookups`; cancellation
  via `cancel_lookup`.
  §8 resources: MAX_CONCURRENT_LOOKUPS=8, MAX_RETAINED_REPLY_PATHS=8,
  MAX_LOOKUP_RETRIES=3, store entries/bytes via RouterInfoStoreConfig,
  retries/deadlines bounded. `note_tunnel_loss` returns typed
  retry/failure and never falls back to direct transport.
  Runtime-neutral: no sockets, tasks, DNS, or filesystem; forbid unsafe.

crates/i2pr-daemon/tests/netdb_tunnel_unit.rs (new)
  22 rows: authoritative bootstrap/eligibility, tamper rejection,
  empty-store honest termination, send after bootstrap, non-floodfill
  exclusion, tunnel-path proof + direct rejection, mismatch, stale,
  invalid signature, malformed, duplicate bounded, search-reply bounded,
  deadline, cancellation, concurrent ceiling, tunnel-loss bounded,
  publication selection/path/unknown-token/retry-no-resign, registry
  composition, store install + direct rejection.

crates/i2pr-daemon/tests/netdb_tunnel_live.rs (new)
  9 two-role rows through real TunnelData cells: outbound composition
  to selected floodfill, inbound DatabaseStore recovery to terminal
  success + ordinary install, publication + DeliveryStatus correlation,
  direct rejection, inbound search-reply bounded, malformed bounded,
  tunnel-loss typed, reference-unavailable honest, liveness green
  during NetDB activity.

crates/i2pr-daemon/tests/netdb_tunnel_external.rs (new)
  Single fail-closed driver against exact-pinned i2pd 2.61.0
  (`635b013a612ff47278ef02acf8580a28e10e26c5`). Profile:
  `notransit = false, floodfill = true` (Plan 186 only). The driver
  verifies the reference RouterInfo, bootstraps the authoritative
  store through the ordinary path, verifies floodfill capability,
  dials the reference, drains warmup, submits one real one-hop
  outbound + inbound build (i2pd `endpoint/gateway` logs), composes
  one lookup + one publication through the tunnel seam bound to the
  real reference identity, admits every cell over the real session,
  rejects direct transport, and drives the liveness first test.

tests/integration/m6-interop/run-netdb.sh (new)
  Local rows: `local-netdb-tunnel-unit`, `local-netdb-tunnel-live`,
  `local-tunnel-liveness`. External rows: strict profile, reference
  verified, floodfill, session, outbound-lookup-tunnel,
  publication-tunnel, direct-rejected, liveness-first-test, plus the
  Plan 184 daemon + reference + session rows. Workspace gates row uses
  `cargo fmt --all --check` + `cargo check --locked --workspace
  --all-targets` + the standard static boundary scripts.

scripts/check-netdb-tunnel-evidence.sh (new)
  Static evidence-integrity checker. The 12 guarded labels (3 local +
  8 external + workspace-gates) are referenced from `run-netdb.sh`
  through `record_guarded` or `m6_row` only; a literal
  `record "<label>" passed` line anywhere in the harness fails this
  check.
```

## Evidence (Plan 186)

Implementation head: current local commit. All counters below
are command-derived; no synthetic `passed` rows exist in the
harness.

NetDB lane (exact-pinned i2pd, loopback, unmodified):

```text
bash tests/integration/m6-interop/run-netdb.sh
# passed; sanitized evidence:
#   target/interop/m6-netdb-evidence/evidence.md
#   target/interop/m6-netdb-evidence/driver/driver-evidence.tsv
```

Lane rows (12/12 passed):

```text
local-netdb-tunnel-unit            = passed
local-netdb-tunnel-live            = passed
local-tunnel-liveness              = passed
external-daemon-strict-profile     = passed
external-reference-verified        = passed
external-reference-floodfill       = passed
external-session-established       = passed
external-outbound-lookup-tunnel    = passed
external-publication-tunnel        = passed
external-direct-rejected           = passed
external-liveness-first-test       = passed
workspace-gates                    = passed
```

Driver evidence keys (sanitized; no secrets):

```text
daemon-strict-profile        = true
reference-routerinfo-verified = true
reference-bootstrap-store    = 1
reference-floodfill-capable  = true
i2pr-routerinfo-len           = 651
session-established          = 1
outbound-build-emitted       = true
outbound-installed            = true
inbound-build-emitted        = true
inbound-installed            = true
outbound-lookup-via-tunnel   = cells=1
outbound-lookup-delivered    = true
publication-via-tunnel       = cells=1
publication-delivered        = true
direct-rejected              = true
liveness-first-test          = passed
shutdown-baseline            = true
```

Reference (unmodified):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
profile = notransit = false, floodfill = true (Plan 186 only)
bind = 127.0.0.1 ephemeral loopback only
advertise = false (i2pr); i2pd published only inside the isolated RouterInfo
```

Focused suites (same tree):

```text
cargo test --locked -p i2pr-daemon --test netdb_tunnel_unit -- --test-threads=1
# 22 passed
cargo test --locked -p i2pr-daemon --test netdb_tunnel_live -- --test-threads=1
# 9 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
# 7 passed
cargo test --locked -p i2pr-daemon --test netdb_tunnel_external \
  netdb_tunnels_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: fail-closed (missing required env)
# with lane env: passed (see driver-evidence.tsv)
bash tests/integration/m6-interop/run-netdb.sh
bash scripts/check-netdb-tunnel-evidence.sh
# NetDB evidence check passed (12 guarded labels)
```

Full floor (same tree):

```text
cargo fmt --all --check
# ok
cargo check --locked --workspace --all-targets
# ok
cargo test --locked --workspace --all-targets -- --test-threads=1
# 2218 passed, 5 ignored (89 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# ok
cargo test --locked --workspace --doc
# 0 passed (no doc tests)
cargo deny check advisories bans sources
# ok
```

Retained regressions: Plan 185 exploratory suites, Plan 184 preflight,
Plan 161 direct SSU2 suite, M9 local suites, M10 local suites remain
green inside the full workspace run.

Static boundary scripts (same tree):

```text
bash scripts/check-dependency-direction.sh
# dependency direction: ok
bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed
bash scripts/check-fixture-manifest.sh
# ok
bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh
# SSU2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh
# Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh
# Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh
# SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh
# SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh
# I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-service-tunnel-acceptance-evidence.sh
# service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records
bash scripts/check-service-tunnel-boundaries.sh
# service-tunnel boundary checks passed
bash scripts/check-i2cp-vectors.sh
# I2CP vector manifest is complete and hashes match.
bash scripts/check-exploratory-tunnel-evidence.sh
# exploratory tunnel evidence check passed (12 guarded labels)
bash scripts/check-netdb-tunnel-evidence.sh
# NetDB evidence check passed (12 guarded labels)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# 153 passed
```

## Stop conditions

None of the Plan 186 §10 stops fired:

- no second lookup engine, direct-transport shortcut, or test-only
  reply injection for counted rows; the coordinator reuses the
  existing `RouterInfoLookup` / `PublicationCoordinator` /
  `outbound_lookup` / `inbound_dispatch` / `ExploratoryPool` /
  `DataPlaneRegistry` / central dispatcher seams;
- the reference floodfill behavior did not expose a NetDB
  encoding/validation mismatch; the ordinary validator accepts the
  exact-pinned reference record and the synthetic controlled records;
- tunnel loss never bypasses the exploratory-path requirement
  (typed `TunnelLost` / retry, direct always rejected);
- no Plan 184/185/M8/M9/M10 regression; the full workspace run +
  every static boundary script above is green.

## Known limitations

- `notransit = false, floodfill = true` is required for i2pd to accept
  one-hop builds and act as the controlled floodfill. Both settings
  are recorded in the lane-generated i2pd config and stay loopback +
  unpublished; no public I2P claim is made.
- i2pd encrypts the endpoint OTBRM with the creator's ECIES-X25519
  key. The external driver verifies build acceptance through i2pd's
  structured log evidence and tunnel composition/admission through the
  real session; the local two-role suite exercises the full
  TunnelData-to-installed pipeline in isolation (same limitation
  recorded in Plan 185).
- One-hop exploratory tunnels only; no multi-hop tunnel build.
- No destination LeaseSet2 / Streaming claim; Plan 187 owns the
  Standard LeaseSet2 and destination-layer ECIES/Garlic program.

## Handoff

Plan 187 reuses this real NetDB substrate for Standard LeaseSet2 and
destination-layer ECIES/Garlic routing. Do not start Streaming until
the destination message plane is independently proven.

```text
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
next_executable_plan = 187
```
