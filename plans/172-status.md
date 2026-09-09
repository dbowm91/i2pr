# Plan 172 status — Milestone 9 independent LeaseSet2 lifecycle corrective

Status: **`passed-m9-i2cp-independent-leaseset2-lifecycle-corrective`
(closed on exact head `299d17a` — routine + external CI green, see §8).**

Registered: **2026-09-08**.
Closed: **2026-09-09**.

Plan of record:
[`plans/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md`](172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md).

## Current authority

```text
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective-retained
plan_170_external_wire_data_plane = retained-passed
plan_170_final_acceptance = superseded-by-plan172
plan_172 = passed-m9-i2cp-independent-leaseset2-lifecycle-corrective

milestone9_i2cp_local_product = passed-via-plan169
milestone9_i2cp_independent_wire_data_plane = passed-via-plan170
milestone9_i2cp_independent_clients = passed-via-plan170-and-plan172
milestone9_i2cp_independent_leaseset2 = passed-via-plan172
milestone9_final_acceptance = closed-via-plan172
next_executable_plan = none (milestone10-planning next)
next_product_layer = milestone10-planning
```

Milestone 9 I2CP final acceptance is **closed via Plan 172** (experimental,
loopback-only). Do not implement Milestone 10 service tunnels/HTTP/SOCKS/IRC
without a new plan-of-record.

## What landed

### Reference-compat wire corrections (all required for counted lifecycle)

`crates/i2pr-api/src/i2cp/message.rs`:

- `RequestedLease` carries the 44-byte I2CP `Lease` layout (32-byte
  gateway, 4-byte BE tunnel id, 8-byte BE ms end date) so exact-pinned
  Java `Lease.readBytes` and go-i2cp `NewLeaseFromStream` parse without
  modification. The prior 36-byte form (no end date) is rejected as a
  protocol defect; empty requests remain structurally valid but never
  satisfy the counted path.

`crates/i2pr-proto/src/common/lease2.rs`:

- `LeaseSet2` key count is a single byte (`u8`) per Java
  `LeaseSet2.readBytes` (`in.read()`), not `u16`. The prior `u16`
  misaligned every reference LS2 by one byte (observed as key count 256
  for Java's single X25519 key).
- The unpublished flag (`0x0002`) is accepted per Plan 172 §10:
  `dontPublishLeaseSet=true` installs locally without NetDB publication.
  Offline/blinded/reserved remain rejected; leased remains accepted
  (no M9 profile sets it).

`crates/i2pr-client/src/registry.rs`:

- ElGamal-slot destinations skip the early capability-vs-destination
  pre-check (the 256-byte slot has been unused since 2005); X25519
  enforcement lives in `install_external` as the LS2-key-vs-capability
  match plus lease ownership/expiry/signature. X25519-slot destinations
  keep the strict pre-check. Foreign/empty-pool LS2 stays fail-closed
  via pool validation (existing Plan 166 checks unchanged).

`crates/i2pr-daemon/src/i2cp.rs`:

- Counted zero-hop `RequestVariableLeaseSet` derives end dates from
  `advertised_expires_seconds * 1000` (ms) with sanitized
  `I2CP_LEASE_REQUEST` logging (session, count, gateway hex, tunnel,
  end_ms; no private bytes).
- `handle_create_lease_set2` logs sanitized `I2CP_LS2_INSTALLED` on
  success and `I2CP_LS2_REJECTED` / `I2CP_LS2_DECODE_FAILED` (bytes +
  reason, no private bytes, no payload) on failure.
- Data-plane gating on `Usable` for zero-hop preserved; remote
  best-effort path unchanged (empty follow-up, never counted).

### Counted drivers (non-production, unmodified libraries only)

- `tests/integration/i2cp/external/java/i2cp_java_session_driver.java`:
  public `I2PClient` / `I2PSession` + `connect()` only (no `Socket`,
  no manual framing, no `I2CPMessageHandler.readMessage`, no manual
  `CreateLeaseSet2Message`; checker enforces). Zero-hop options
  (length 0, quantity 1, backup 0, allowZeroHop true, dontPublish true,
  type 3/enc 4, fastReceive true, BestEffort). Deterministic
  ElGamal-zeroed destination (zeroed 256-byte slot + 96-byte zero padding)
  so go-i2cp hashes identically to Java/daemon; X25519 LS2 keys are
  generated fresh by Java's normal handler. `lifecycle`, `send-to-go`
  (high-level `sendMessage` with proto 6/ports 7/8), `send-to-java`
  (muxed listener + `receiveMessage` with digest + `delivery_path`).
- `tests/integration/i2cp/external/go/i2cp_go_driver.go`:
  public API only. New `configureZeroHop` (length 0, quantity 1, backup 0,
  allowZeroHop true, dontPublish true, enc 4, fastReceive true, none) plus
  `lifecycle-zero-hop` via async `CreateSession` + single persistent
  public `ProcessIO` loop (no `CreateSessionSync` cancel race, no manual
  LS2). `send-to-go` / `send-to-java` branch on `ZERO_HOP=1` to the same
  async single-loop shape with an 8 s LS2 window before sends; retained
  Plan 170 remote paths use `CreateSessionSync` unchanged.

### Lane, checker, fixtures

- `tests/integration/i2cp/run-independent.sh`: 24 fail-closed rows
  (9 retained Plan 170 + 15 counted Plan 172). Counted rows gate on
  Java `connect_returned`, Go `session_created` + `public_processio_alive`,
  router `leases=1` with no `leases=0` and no `DECODE_FAILED`/`REJECTED`,
  stable non-zero gateway, distinct non-zero non-sentinel tunnels,
  non-zero end dates, digest equality + protocol 6 + strong
  `client_parsed_digest` for all four post-LS2 directions (ports recorded
  as observed per §13 BE/LE exposure), plus `i2cp_zero_hop_lifecycle` in
  regressions. Evidence titles updated to Plan 172 (Plan 170 retained).
- `scripts/check-i2cp-acceptance-evidence.sh`: 24-row integrity gate
  (no literal passes, exit-code gating, pins, loopback-only, digest +
  strong-parse requirements, counted Java high-level + Go zero-hop
  invocations, non-empty/zero-lease fail-closed gates, LS2-install
  observations, lifecycle-before-traffic ordering, counted-driver
  manual-framing rejection, `connect()` requirement), enforced in routine
  Linux CI.
- Fixtures: `request-variable-leaseset.hex` regenerated for 44-byte Leases
  (session 7, 2 leases with end dates 1786000000000/1786000060000);
  `create-leaseset2.hex` patched for u8 key count (frame 589 vs 590);
  `manifest.tsv` hashes updated; `i2cp_vectors` asserts end dates.
- `plan166_trajectory` legacy test updated for ElGamal-skip semantics
  (foreign/empty-pool fails via pool validation, not pre-check mismatch).
- `i2cp_zero_hop_lifecycle` asserts non-zero future end dates.
- Arch deep-dives (`i2pr-api`, `i2pr-client`, `i2pr-daemon`, `tooling`)
  document 44B compat, u8 key count, unpublished acceptance, ElGamal skip,
  async Go lifecycle, and the 24-row lane.

## Evidence (local, two consecutive runs, same tree)

Pins: Java I2P 2.13.0 `9134f808337b401e8e53c73734c81fab04280c9d`,
go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac`.
Bind: `127.0.0.1` only. No root/namespaces/Docker/VM/systemd.

```text
bash tests/integration/i2cp/run-independent.sh   # passed twice
bash scripts/check-i2cp-acceptance-evidence.sh   # 24 rows command-derived
```

Final run rows (all `passed`, all attempt 1/3):

```text
external-java-to-go-small    digest f5cd0143d29d... (25 B each side, retained)
external-java-to-go-large    digest d8690a426100... (32768 B each side, retained)
external-go-to-java-small    digest f5cd0143d29d... (25 B each side, retained)
external-go-to-java-large    digest d8690a426100... (32768 B each side, retained)
external-message-status-semantics
external-bandwidth-query
java-high-level-connect
go-public-session-lifecycle
java-nonempty-lease-request / go-nonempty-lease-request (leases=1, 44B)
java-client-generated-leaseset2 / go-client-generated-leaseset2
java-leaseset2-installed / go-leaseset2-installed (Usable)
zero-hop-lease-gateway-owned / zero-hop-lease-tunnel-id-owned
sessions-usable-after-ls2-only
java-to-go-small-after-ls2    digest 078ab1785258... (28 B? 25 B payload "plan172-payload-small-001")
java-to-go-large-after-ls2    digest d8690a426100... (32768 B)
go-to-java-small-after-ls2    digest 078ab1785258... (25 B)
go-to-java-large-after-ls2    digest d8690a426100... (32768 B)
plan167-169-focused-regressions (6 suites incl. zero-hop-lifecycle)
workspace-gates
external-clean-resource-baseline
```

Digest proof (after LS2, both directions identical for small):

```text
java outbound sha = go inbound sha = 078ab17852588b42512d7d2fa203eae44bed52ce80f60ec612d4057476baad7e
go outbound sha = java inbound sha = 078ab17852588b42512d7d2fa203eae44bed52ce80f60ec612d4057476baad7e
```

Large after LS2 matches retained large (`d8690a426100...`, 32768 B).
Ports `7/8`, protocol `6` on counted rows (Java->Go Go-side ports
observed as 1792/2048 BE/LE exposure per §13; digest + protocol mandatory).
Router observed `I2CP_LEASE_REQUEST` (leases=1, stable non-zero gateway,
distinct tunnels, non-zero end_ms) and `I2CP_LS2_INSTALLED` (leases=1,
usable=true) for every counted session with zero `DECODE_FAILED`/`REJECTED`.

Full floor on the closing tree: `cargo fmt --check`, `cargo check
--locked --workspace --all-targets`, `cargo test --locked --workspace
--all-targets` (1746 passed, 1 ignored, 69 suites), `cargo clippy
--all-targets --all-features -D warnings`, `cargo doc -D warnings`,
doc tests, all static boundary/vector/evidence scripts,
`python3 -m unittest discover` (153 OK), `cargo deny check
advisories bans sources` — all green.

## Known limitations (explicit, non-blocking)

- No public-I2P, remote-I2CP, non-zero-hop destination-tunnel, PQ,
  encrypted/meta-LS, or M6 mixed-router claim. I2CP stays experimental,
  disabled by default, loopback-only.
- `HostLookup`/`HostReply` stays `Failure` (no resolution); `DestLookup`
  resolves only local registry (retained).
- i2pd I2CP reciprocity deferred as nonblocking narrow-orchestration debt
  (retained from Plan 170).
- Java->Go high-level ports observed as 1792/2048 (BE/LE exposure
  difference); digest + protocol 6 mandatory per §13, ports recorded as
  observed.

## §19 closure (exact head)

```text
CLOSING_SHA            = 299d17a1996af4b78850822d3eb782d508c051c6
ROUTINE_CI_RUN         = 34318789357
Quality (ubuntu-latest) = success
Quality (macos-latest)  = success
MSRV (Ubuntu)           = success
Dependency policy       = success
EXTERNAL_RUN_1         = 34319840286 (.github/workflows/i2cp-external.yml)
I2CP independent clients (Ubuntu) = success
EXTERNAL_RUN_2         = 34320329109 (.github/workflows/i2cp-external.yml)
I2CP independent clients (Ubuntu) = success
```

Implementation commit `e3ceb08` went green on routine CI
(`34316200716`, all four jobs success) and proved the 24-row lane
twice locally plus twice hosted (`34317406711`, `34317801182`, all
attempt 1/3). Closure head `299d17a` (docs/authority only, no `crates/`
delta) re-verified routine CI (`34318789357`, all four jobs success)
and the 24-row lane twice hosted on the exact closing tree (all
attempt 1/3, digests match local: retained `f5cd0143d29d...` small,
`d8690a426100...` large; after-LS2 `078ab1785258...` small,
`d8690a426100...` large). Evidence artifacts upload on all external
runs; hosted `results.tsv` matches local row-for-row. No Milestone 10
work is implemented in this plan.

## Handoff

Milestone 9 I2CP final acceptance is **closed via Plan 172**.
Next product layer is `milestone10-planning` (service tunnels/HTTP/SOCKS/IRC
require a new plan-of-record). Retain Plan 170 wire/data-plane rows as
regression evidence; do not claim public-I2P, NetDB publication of local
LS2, non-zero-hop interop, or M6 mixed-router interop from these rows.
