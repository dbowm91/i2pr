# Plan 309 status — Destination linkability domains and service-group composition

Status: `passed-destination-linkability-domain-service-group-composition`

## Plan authority and implementation lineage

Plan 309 was registered after Plan 307 passed. The required architecture inputs are
ADR 0030 and the retained Plan 305 owner/reference audit. Plan 308 is independent and
remains blocked; its status does not gate this closure.

Implementation commits:

- `2def9f6` — compose shared Destination-group runtime ownership, persistence,
  provisioning, documentation, and regression coverage.
- `e24a548` — cover multiple same-kind shared client services and ephemeral-group
  restart rotation.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Explicit groups are the linkability domain; dedicated services retain unique groups | `DestinationGroupKey` and `DestinationGroupSpec` feed one staged `DestinationGroupRuntime` per group. Runtime tests prove HTTP+SOCKS and multiple same-kind members share one ID while a dedicated HTTP service gets a distinct ID. | Passed |
| One group owns one identity, Streaming bridge, and registry entry | Group identity and bridge are shared by member runtimes. Generation prepare/reconcile deduplicate `SamDestinations`, `DestinationRegistry`, router provisioning, inbound owner registration, and server publication by Destination ID. Snapshot counts distinct Destinations. | Passed |
| Client-only groups are ephemeral; persistent groups include any server | Client group test verifies no persisted group file and that a restart rotates its ID. Two-server group test verifies one ID across members and restart persistence. | Passed |
| Existing dedicated server keys migrate without changing bytes | `ServiceDestinationStore::for_group` uses a separate `groups/<id>` namespace. `migrate_from` atomically installs the same encoded key before removing the legacy file; storage regression compares exact encoded bytes. The existing generic-server persistence/corruption test now checks the canonical group path. | Passed |
| Multiple server services share one Destination on distinct inbound ports; duplicate group/port is rejected | Runtime test prepares two server members at ports 8080 and 6667, confirms one persistent ID, and restarts it. Runtime-neutral config test `shared_server_destination_ports_must_be_unique` rejects duplicate ports. | Passed |
| Group membership changes reconcile atomically | `diff_sets` marks every existing member for destination replacement when the group key or membership changes. Daemon reconcile regression adds a member and verifies all committed members retain one group ID. | Passed |
| Persistent mixed groups disclose intentional linkage locally | Startup and reconcile emit a warning for client members of persistent server groups. Architecture documentation describes this relationship and gives shared-client and multi-port server examples. | Passed |
| Resource, dependency, router-identity, and protocol boundaries remain intact | Existing ceilings remain authoritative; no dependency, wire format, RouterInfo behavior, listener exposure, or router identity reuse was added. Boundary scripts passed. | Passed |

## Commands and outcomes (local)

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed,
  3,282 tests passed, 34 ignored, 110 suites. The first run found a stale test path;
  the fixture was updated to the group namespace, its focused suite passed, and the
  workspace run then passed.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` —
  passed after replacing one boolean equality assertion flagged by Clippy. The final
  ephemeral-restart test addition also passed focused daemon Clippy.
- `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed, 16 suites.
- `cargo test --locked -p i2pr-service-tunnels --lib -- --test-threads=1` — 228 passed.
- `cargo test --locked -p i2pr-storage --lib -- --test-threads=1` — 20 passed.
- `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` — 302 passed.
- `cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1` — 9 passed.
- `cargo test --locked -p i2pr-daemon --lib http_and_socks_services_can_share_one_ephemeral_group -- --test-threads=1` — passed after the final restart/multiple-SOCKS fixture update.
- `bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `git diff --check` — passed.

## Compatibility, security, and limitations

Dedicated server identities migrate to the stable group namespace without rotating
their key bytes. Client-only group identities remain ephemeral. Sharing occurs only
when explicitly configured; no capacity-triggered sharing or fallback was added.
Persistent groups containing clients produce a local warning because those clients
are intentionally linkable to the server Destination. Server port uniqueness is
validated before daemon mutation. No dependency or production wire change was made.

Plan 309 establishes composition and ownership only. It does not implement real
three-hop group pools, peer selection, service lifecycle smoothing, or Streaming
profile convergence; those remain Plan 310 and later work. It makes no public
anonymity or production-readiness claim. Plan 308's ordinary-HTTP topology and
three-family evidence remain independently blocked.

| Severity | Finding |
|---|---|
| Critical | None. |
| High | None. |
| Medium | Plan 308 remains blocked on a controlled ordinary-HTTP reference topology and all three family captures; this is independent of Plan 309. |
| Low | None. |

## Unblock audit and roadmap disposition

- **Plan 308:** remains blocked on the controlled ordinary HTTP topology and three-family captures; it does not depend on Plan 309.
- **Plan 310:** ready. Plan 309 is its only hard plan dependency; ADR 0030 and the retained Plan 305 reference-diversity matrix are available.
- **Plans 311 and 312:** remain blocked on Plan 310.
- **Plan 313:** remains blocked on Plan 312.
- Plans 297–305 remain immutable historical stopped records. No M12/mainline or production-anonymity status changes.

Disposition: passed. Destination groups now provide the ownership/composition contract
required by Plan 310, which may proceed independently of the still-blocked Plan 308.
