# Tooling — Deep Dive

Every script, fixture corpus, integration lane, fuzz target, and CI job
that wraps the workspace. The boundary contract is enforced here; if a
script rejects, fix the boundary, do not suppress the script.

Paths are relative to the workspace root. The boundary this document
describes is the same one summarized in
[overview.md](overview.md) and
[dependency-graph.md](dependency-graph.md); the harness-level contract
lives in [interop-apparatus.md](interop-apparatus.md).

**Inventory at a glance** (recomputed 2026-10-07 by Plan 372).

Every count here is **derived from the tree and checked by
`scripts/check-tooling-inventory.py`**, which is in the routine floor. If a
guard, lane, or crate is added without updating this table, that guard
fails. The two rows that can be inflated by build artifacts — the
`scripts/` file counts — are `git ls-files`-scoped, because an unfiltered
`find` also matches gitignored `__pycache__/*.pyc`.

| Surface | Count | Where |
| --- | --- | --- |
| Top-level `scripts/` files | 57 | 55 `check-*`, `fuzz-smoke.sh`, `run-java-source-lock-tests.sh` |
| `scripts/interop/` files | 69 | 30 top level, 33 `multipass/`, plus `anonymity/`, `lib/`, `ubuntu/` |
| `check-*` on disk (all classes) | 58 | 55 top level + 3 under `scripts/interop/` |
| Checker invocations in `ci.yml` | 39 | 37 `check-*` + 2 `python3` test discoveries |
| Integration lane directories | 10 | under `tests/integration/` |
| Fixture corpora | 4 | `tests/fixtures/{i2np,ntcp2,ssu2,i2cp}` |
| Fuzz targets | 25 | `[[bin]]` entries in `fuzz/Cargo.toml` (+1 shared `support.rs`) |
| CI workflows | 10 | 1 ordinary gate + 9 `workflow_dispatch` lanes |
| Workspace members | 29 | 28 `crates/*` + `tools/i2pr-interop` |

Corrected by Plan 372, each against the plan that made the figure false:
the `scripts/`, `check-*`, `ci.yml`, fuzz-target and workspace-member
figures had all been left at their pre-Plan-369 values — the member count
said 20 while `cargo metadata` reported 26, so six crates, including the
whole managed-application runtime and the operator console, were missing
from this document's roster.
| Skill bundles | 6 | `.opencode/skills/` |

## `scripts/` — guardrail shells

54 `check-*` files exist on disk in `scripts/` (57 counting `scripts/interop/`),
grouped below by what they catch. The
`Floor` and `CI` columns say whether the script appears in the
[`AGENTS.md` routine floor](../../AGENTS.md) and in
`.github/workflows/ci.yml` respectively. The full floor/CI matrix is in
[Floor and CI coverage](#floor-and-ci-coverage), together with the
counting method both numbers come from.

### Boundary checkers (static, source-scanning)

| Script | What it catches | Floor | CI |
| --- | --- | --- | --- |
| `scripts/check-dependency-direction.sh` | Crate-layer DAG violations, including the runtime-neutral SU3 verifier layer and its NetDB consumer. Uses `cargo metadata` piped to a Python 3 JSON reader with an explicit allowlist map. | yes | yes |
| `scripts/check-managed-app-gateway-boundary.py` | Plan 355 static guard for trusted-only authorization, exact private SAM/I2CP seam use, canonical SAM address-book injection, disabled listener fallback, and daemon-only app-proto ownership. | yes | yes |
| `scripts/check-managed-app-manager-boundary.py` | Plan 368 static guard for one-way protocol ownership (daemon-only consumer), contract-crate runtime/OS purity, no host socket/listener/loopback path in the bridge, no admin/package/config/process vocabulary, unrepresentable `control_scoped`, no application-declaration authority input, bounded accounting, and session-local stream isolation. Normalises `use` trees so grouped imports cannot evade it. | yes | yes |
| `scripts/check-managed-app-process-boundary.py` | Plans 369/374 static guard for **which process execs what**. Rule 1 pins the three blessed spawn sites (daemon→manager, appd→apphost, apphost→application); rule 1b requires `current_exe()` sibling resolution for the two distribution-owned edges and forbids shell launch and `PATH` lookup; rule 2 forbids production code naming or depending on the fixture; rule 3 requires the shipped manager to refuse argv and use the persistent policy catalog; rule 4 keeps the manager test seam definition-shaped, failing closed on any production caller. Strips comments and `#[cfg(test)]` regions before scanning, so test-only qualification can exercise the real catalog without adding a production fixture edge. `--self-test` applies each mutation in memory and requires rejection, with negative controls. | yes | yes |
| `scripts/check-managed-app-package-boundary.py` | Plan 373 package trust-zone guard: confines package code to the app protocol contract and rejects router/runtime, network, process execution, and launch-authority seams. `--self-test` proves each source/dependency rule with in-memory mutations and positive controls. | yes | yes |
| `scripts/check-managed-app-policy-boundary.py` | Plan 374 policy/CLI/appd guard: constrains the app-state/appctl dependencies and keeps `LaunchAuthority` construction in appd, verifies persistent-catalog wiring, and checks the daemon's state-root handoff. `--self-test` proves the seam rules with source mutations. | yes | yes |
| `scripts/check-portable-service-tunnel-api.py` | Reviewed source declaration snapshot for the reusable service-tunnel public API; signature and semver review remains required for changes. | yes | yes |
| `scripts/check-license-metadata.py` | Plan 379 repository license-metadata drift guard. Asserts the MIT `LICENSE` shape, the root `[workspace.package] license`, and that **every** member manifest inherits or names MIT — then cross-checks the resolved `cargo metadata` values, so a `[workspace.package]` key nobody inherits cannot read as a declaration again. Also rejects any member `license-file` (a second, disagreeing source of truth) and keeps the README's clean-room/provenance clause. Regex-parsed rather than `tomllib` so it runs on macOS system Python; `--self-test` gives every rule a rejected mutation plus accepted controls for the strict direction. | yes | yes |
| `scripts/check-portable-service-tunnel-consumer.sh` | Plan 351 standalone Git-pinned consumer proof: compiles/tests the public-only fixture outside the workspace and checks its resolved dependency graph. | yes | yes |
| `scripts/check-runtime-boundaries.sh` | Grep-based audit: unbounded channels, wall-clock sleeps, raw `JoinHandle`s, ownerless `tokio::spawn`, `async fn` in transport contracts, Tokio deps in wrong crates, `std::net`/`std::fs` in transport, `i2pr-testkit` referenced by a production crate. **Plan 362** adds (a) an `i2pr-api` section — 7 source rules (`tokio::`, `async fn`/`async_trait`, socket types, `std::fs`/`OpenOptions`/`File::`, unbounded channels, `JoinHandle`, `spawn(`) plus a manifest rule banning `i2pr-daemon\|runtime\|testkit\|console\|service-tunnels` — and (b) **brace normalisation**: a Python pass blanks comment bodies and literal contents, rewrites every `use` tree into flat leaves, and then re-applies the *same* alternations to the normalised text, so a grouped `use std::{fs, net};` is detected exactly like a flat import. The normalised scan is **additional** to the raw greps, which are untouched. It fails closed on unparseable `use` syntax. Positive controls cover grouped, nested, multi-line and glob groups. `std::net` address *values* remain permitted for `i2pr-api` (Plan 345 precedent). | yes | yes |
| `scripts/check-console-boundaries.sh` | Router-console boundary guard (Plan 356): rule 1 manifest purity, rule 2 source purity, rule 3 daemon EggServe adapter, rule 4 no `eepsite` tree, rule 5 self-contained assets, rule 6 single HTTP substrate, rule 7 dependency-map covers every member, rule 6b Plan 358 local principal. **Rule 2 (Plan 366) was repaired and is now load-bearing.** It previously ran one `awk` whose `in_tests` flag was set on the first `#[cfg(test)]` and never reset, so it scanned `theme.rs` lines 1–1139 and **nothing else — 12 of 13 console source files were never examined**. It is now a single Python scanner (`console_source_scan`) that (a) normalises `use` trees before matching (Plan 362), (b) scopes each test region to the item it annotates instead of latching it, and (c) enforces a **socket-keyed** ban — `TcpListener`, `TcpStream`, `UdpSocket`, `UnixListener`, `UnixStream`, `Server::bind`, `axum::serve`, `spawn(`, `std::fs`, `fs::`, `tokio::` — plus an **enumerated** allow-set of exactly four `std::net` address values (`IpAddr`, `SocketAddr`, `Ipv4Addr`, `Ipv6Addr`). A bare `use std::net;`, a glob, or the `ToSocketAddrs` resolver stays forbidden. This is *more* enforcement, not less: the old rule rejected 0 socket types because it read nothing after `theme.rs:1139`, and its blanket `std::net` ban was unsatisfiable for a correct console. A zero-file scan fails rather than passing. `--trace` prints the per-file `production_lines_scanned` proof; `--self-test` drives the same scanner over fixture trees (every forbidden category probed in all 13 files, positive controls for the allow-set, allow-set-widening negatives, zero-file and fail-closed checks) rather than re-implementing the rule. | yes | yes |
| `scripts/check-console-browser-security.sh` | Plan 357 strengthening guard for the browser boundary: exact `Host` authority, Origin validation on unsafe methods, `HttpOnly`/`SameSite=Strict` session cookies, CSRF, and a centralized no-`unsafe-inline` CSP. Where a rule cannot be expressed as a grep, the named test must exist and be present in the suite. | yes | yes |
| `scripts/check-config-secret-hygiene.sh` | Plan 352 guard for two pre-existing config-secret leak paths. Inverted from "assert the hazard is present" to "assert the fixed state", so reverting the fix fails here: no secret content in `RedactedTomlError` output, and a password-bearing config is refused when group/world-readable. | yes | **no** |
| `scripts/check-els2-type11-transcript-boundary.sh` | Plan 346 guard keeping the deployed ELS2 type-11 transcript confined to the encrypted-LeaseSet2 type-5 owner. That correction is only safe while the confinement holds; three changes would silently undo it. | yes | **no** |
| `scripts/check-encrypted-service-consumer-caller.sh` | Plan 351 guard on the encrypted-service (`.b33`) consumer wiring. Plan 349 landed `EncryptedServiceResolver` with no production caller, and adding one was only acceptable because of a containment this checks; three things would silently undo it. | yes | **no** |
| `scripts/check-outproxy-request-path.sh` | Plan 342 guard for the I2P-routed outproxy request-path boundary: no direct-clearnet capability on any of the four client request paths, **including failure paths**. Wrapper; execs the `.py` beside it because the checks resolve named function bodies by brace matching. | yes | **no** |
| `scripts/check-outproxy-request-path.py` | Implementation of `check-outproxy-request-path.sh`, holding the brace-matched function-body checks. A distinct tracked script, so it carries its own row; invoked only through the wrapper. | **no** | **no** |
| `scripts/check-outproxy-wire-lane-evidence.sh` | Plan 342 evidence-integrity check for the self-composed loopback outproxy wire lane. Wrapper; execs the `.py` beside it. | yes | **no** |
| `scripts/check-outproxy-wire-lane-evidence.py` | Implementation of `check-outproxy-wire-lane-evidence.sh`. A distinct tracked script, so it carries its own row; invoked only through the wrapper. | **no** | **no** |
| `scripts/check-managed-app-private-client-seams.py` | Plan 354 guard for the listener-independent SAM/I2CP connection seams: managed-profile host-target denial, and that both loopback listeners and trusted private connections drive one protocol driver. | yes | yes |
| `scripts/check-floodfill-type5-serve.sh` | Plan 350 guard for the floodfill's servable record types. The bug it prevents is silent and data-only: `database_store_for_answer` and `lookup_body` each need a type-5 arm, and omitting either stores records nobody can fetch. | yes | **no** |
| `scripts/check-ntcp2-interoperability.sh` | Plan 099 NTCP2 interoperability static boundary check, enforcing the durable invariants the retained development interop surface relies on. It enforces **no** behaviour and does not make NTCP2 an advertised transport. | yes | yes |
| `scripts/check-tooling-inventory.py` | **Plan 372.** Derives every published inventory figure in this document from the tree — floor-step counts, `check-*` files, `ci.yml` invocations, workspace members, `rust-version`, the MSRV toolchain, fuzz targets, and the root `tests/` `.rs` count — and **fails closed** when one stops matching. Rule 3 requires every `check-*` script to have a row here, so adding a guard without documenting it is a failure. `--self-test` proves each rule rejects the violation it claims to detect. | yes | yes |
| `scripts/check-service-tunnel-boundaries.sh` | Plan 180 M10 runtime-neutral invariants: no Tokio/sockets in `i2pr-service-tunnels`, no Garlic/I2NP construction, single shared `run_stream_pump`, no unbounded Tokio channels, exactly one `register_service_tunnel_manager` entry point. **Plans 349–350** add positive-controlled bans on runtime/I/O ownership and all direct workspace-crate dependencies. **Rules 9–11 (Plan 343)**: the outproxy policy/route-owner pair (`i2pr-service-tunnels/src/outproxy.rs` + `i2pr-daemon/src/outproxy_route.rs`) must both exist; neither may name a clearnet socket type, resolver, or TLS-to-clearnet client (`TcpStream\|TcpListener\|UdpSocket\|to_socket_addrs\|lookup_host\|TcpSocket\|openssl\|native_tls\|reqwest\|hyper`) — `std::net::IpAddr` is deliberately *not* matched, because parsing an address is how the target grammar refuses IP literals; neither may load a plugin or spawn a process (`libloading\|dlopen\|Library::new\|Command::new\|std::process`). **Rule 10 is a positive control** requiring a local listener reference in `service_tunnels_http.rs`. | yes | yes |
| `scripts/check-m11-transit-boundaries.sh` | Plan 264/265 M11 transit runtime-neutrality and static boundary invariants. | yes | yes |
| `scripts/check-m12-floodfill-boundaries.sh` | M12 floodfill runtime-neutrality: `i2pr-netdb` must not import `i2pr-daemon`/`i2pr-runtime` effects, no `tokio::`/`std::{net,fs}::`/sockets/`JoinHandle`/`tokio::spawn` under `crates/i2pr-netdb/src`, and the type-5 floor is asserted positively. **Plan 364** replaced the stale Plan 281 "type 5 is deferred" grep — which legitimately matched once Plans 332/333/334 populated type-5 NetDB storage, and which made the script exit 1 — with 9 positive assertions traced to the Plans 332/333/334/346 closure records. Do not weaken the script and do not remove type-5 from NetDB to make it pass. | yes | yes |
| `scripts/check-m11-per-epoch-composition.sh` | Plan 264 §work-package-A per-epoch fresh-mesh composition gate, extended by Plan 265 into the fixed-budget opportunity-qualified closure composer (three frozen scenario families, exactly eight retained fresh-mesh attempts). | yes | **no** |
| `scripts/check-service-anonymity-boundaries.sh` | Plan 307 service-boundary matrix and leak regression checker: rejects `i2pr/<version>`-style product sentinels (`Proxy-Agent: i2pr`, `DEFAULT_QUIT_REASON`, product/build/router/transport/hostname/IP/path/destination-alias/raw-error sentinels) across the service-tunnel HTTP/SOCKS/IRC source set. | yes | **no** |
| `scripts/check-rootless-interop-boundary.sh` | Plan 046 rootless sealed-namespace lane boundary. Forbids `sudo`/`ip netns`/`nft`/`setcap`/`--privileged`/`--network host` and silent fallback to the privileged backend. | **no** | no — `ntcp2-interop-rootless.yml` only |
| `scripts/check-multipass-interop-boundary.sh` | Plan 048/049/050/051 Multipass recovery lane boundary. Forbids host-policy mutations and global `multipass purge` outside an atomic reservation. | **no** | **no** |
| `scripts/check-constrained-host-lane-boundary.sh` | Plan 077 constrained-host selection-order boundary (rootful Docker `--network none` → QEMU TCG `-nic none` → reduced inherited descriptors + seccomp → manual remote Linux → typed `no-full-runtime-lane` result). | yes | yes |
| `scripts/interop/check-m6-java-response-source-lock.sh` | Plan 236 §6 read-only source lock over the exact-pinned Java I2P checkout, binding the `Connection.sendPacket` → `PacketQueue.enqueue` → `I2PSession.sendMessage` response path. Writes only sanitized class/method facts; never patches or builds the reference tree. Consumed by the M6 Java lane. | **no** | **no** |
| `scripts/interop/check-p243-host-qualified.sh` | Plan 243 §4 host qualification gate for the M6 Java Streaming hosted stock-client-build lane: proves the execution host carries every artifact the frozen Plan 242 lane depends on *before* any counted attempt consumes the three-attempt budget. | **no** | **no** |
| `scripts/interop/ubuntu/check-host.sh` | Plan 038 Ubuntu host preflight/postflight contract check. Invoked as `check-host.sh --pre-install` before setup and `--post-install` after. It is a host *contract* checker, not a boundary guardrail. | **no** | **no** |

### Plan 362 findings that still need a plan-of-record

Plan 362 closed the grouped-import evasion and added an `i2pr-api` section to
`check-runtime-boundaries.sh`. Doing so surfaced three further gaps. None is a
licence to add the forbidden edge, and none was absorbed by relaxing a rule.

1. ~~**`check-console-boundaries.sh` rule 2 checks almost nothing**~~ — **CLOSED
   by Plan 366.** The awk block set `in_tests = 1` on the first `#[cfg(test)]`
   and never reset it, so the remaining **12 of 13** console source files were
   never examined: `use std::net::TcpStream;` at line 2 of `theme.rs` failed
   the script while the identical line in `security/mod.rs` passed. Rule 2 is
   now a per-file scanner; `--trace` reports `production_lines_scanned` for
   every file (**13/13 files, 4416 production lines**, versus `theme.rs`
   1–1139 alone before). **Corrected by Plan 367:** this figure was recorded as
   4430; the measured value is **4416**, which is what
   `bash scripts/check-console-boundaries.sh --trace` prints in its
   `TRACE … production_lines_scanned=` summary.

2. ~~**Console rule 2's `std::net` alternative is unsatisfiable**~~ — **CLOSED
   by Plan 366.** The per-file reset surfaced two real hits,
   `crates/i2pr-console/src/security/authority.rs:20`
   (`use std::net::{IpAddr, SocketAddr};`) and `security/mod.rs:27`. Both are
   address **values**, used to satisfy the AGENTS.md requirement that "requests
   must match an exact `Host` authority including the port". Applying Plan 345's
   precedent, the blanket `std::net` ban became a socket-keyed ban plus an
   enumerated four-type address-value allow-set. The console crate is
   **unchanged**; only the enforcement was repaired.

3. **The transport `std::net` ban is already evaded** (medium, **OPEN**). The
   transport rules forbid the string `std::net`, but
   `crates/i2pr-transport-ntcp2/src/address.rs:9`,
   `crates/i2pr-transport-ssu2/src/address.rs:21`, `block.rs:20`,
   `state_machine.rs:20` and `token.rs:16` import `std::net::IpAddr` /
   `std::net::SocketAddr` through a grouped `use`. Plan 362 deliberately did
   **not** extend the normalised scan to the transport crates, and Plan 366 kept
   that boundary in scope: doing so would fail the floor on a rule whose intent
   ("no sockets") differs from its literal text. Resolving it needs a
   transport-scoped plan, not a tooling fix.

A fourth, benign observation: `crates/i2pr-api/src/sam/limits.rs:178,206` name
`tokio::` in comments only. That is why the normalised scan masks comment bodies
before matching.

### Fixture and vector checkers

| Script | Corpus | What it catches | Floor | CI |
| --- | --- | --- | --- | --- |
| `scripts/check-fixture-manifest.sh` | `tests/fixtures/i2np/` | Manifest IDs, classification (`positive`/`negative`), provenance (`locally-authored`/`independently-produced`), all metadata fields, on-disk file existence, SHA-256 hash match; rejects orphan `.hex` files. Requires bash 4 `declare -A`. | yes | yes |
| `scripts/check-ntcp2-vectors.sh` | `tests/fixtures/ntcp2/crypto/` | Duplicate-free manifest, `positive`/`malformed` categories, 64-char hex hashes, path containment, file existence, SHA-256 match, and the 13 required NTCP2 crypto vector IDs in `vectors.tsv`. Requires bash 4 `declare -A`. | yes | yes |
| `scripts/check-ssu2-vectors.sh` | `tests/fixtures/ssu2/` | Duplicate-free manifest, `positive`/`malformed` categories, 64-char hex hashes, path containment, file existence, SHA-256 match, the 5 required Plan 155 fixture IDs, the 6 Plan 156 handshake vectors, and the 2 Plan 157 data-phase vectors. Requires bash 4 `declare -A`. | yes | yes |
| `scripts/check-i2cp-vectors.sh` | `tests/fixtures/i2cp/` | Duplicate-free manifest, `positive`/`malformed` categories, 64-char hex hashes, path containment, file existence, SHA-256 match, the required Plan 164 fixture IDs, and the narrow `i2pr-api --test i2cp_vectors` suite. Requires bash 4 `declare -A`. | yes | yes |

### Evidence-integrity checkers

Every checker in this class rejects literal unconditional `passed` rows
and binds each required row to an executed command's exit code and
evidence key.

| Script | Lane | What it catches | Floor | CI |
| --- | --- | --- | --- | --- |
| `scripts/check-sam-acceptance-evidence.sh` | SAM | Plan 151 SAM evidence integrity: no literal unconditional `passed` rows; every required row flows through the exit-code-gated helpers. | yes | yes |
| `scripts/check-ssu2-acceptance-evidence.sh` | SSU2 | Plan 161 SSU2 evidence integrity: no literal unconditional `passed` rows; every required row flows through the exit-code/evidence-key-gated helpers with explicit `--ignored --exact` external selection. | yes | yes |
| `scripts/check-i2cp-acceptance-evidence.sh` | I2CP | Plan 170/172 I2CP evidence integrity (Plan 170 9-row lane retained; Plan 172 adds lifecycle rows, raw-driver substitution rejection, zero-lease rejection, and LeaseSet2-install gating): no literal unconditional `passed` rows; every required row flows through the exit-code-gated `record_guarded` helper with digest-equality + strong-parse-path gates and explicit Java/Go pins. | yes | yes |
| `scripts/check-i2pcontrol-acceptance-evidence.sh` | I2PControl | Plan 295 I2PControl differential evidence integrity: the local 28/8/4 corpus derives from the executed Rust corpus with a sanitized shape hash; external rows stay env-gated (`blocked-env-absent` until both target env vars are set); no literal pass rows, no forgiveness, no fake env, no secret-carrying evidence. | yes | yes |
| `scripts/check-service-tunnel-acceptance-evidence.sh` | service tunnels | Plan 181/199 service-tunnel evidence integrity: 29 command-derived local rows plus two fail-closed remote qualification rows; no literal passes; pin/head/cleanliness and curl/SOCKS/jaraco gates. | yes | yes |
| `scripts/check-exploratory-tunnel-evidence.sh` | M6 tunnels | Exploratory tunnel evidence integrity: guarded local build/liveness rows flow through exit-code-gated helpers. | yes | yes |
| `scripts/check-netdb-tunnel-evidence.sh` | M6 NetDB | NetDB-over-tunnel evidence integrity: guarded lookup/publication rows flow through exit-code-gated helpers. | yes | yes |
| `scripts/check-destination-tunnel-evidence.sh` | M6 destination | Destination-tunnel evidence integrity: 21 guarded labels across local and mixed-router harnesses. | yes | yes |
| `scripts/check-streaming-tunnel-evidence.sh` | M6 Streaming | Streaming-tunnel evidence integrity: 33 guarded labels across i2pd + Java harnesses. | yes | yes |
| `scripts/check-m6-mixed-router-acceptance-evidence.sh` | M6 cross-family | Plan 189 §8 / Plan 194 / Plan 196 / Plan 197 cross-family M6 mixed-router evidence integrity: per-layer harnesses + static checkers both pin i2pd 2.61.0 and Java I2P 2.13.0; the cross-family aggregator reuses the four per-layer harnesses and binds every guarded row to a family + a per-layer exit code. Plan 196 requires the out-of-tree `ControlledRouter.java` source as a required artifact and rejects `i2p.vmCommSystem=true`, the obsolete Plan 194 keys (`i2np.reseed.enable`, `router.isFloodfill`, `i2np.ntcp2.enabled`), `sed` mutations of the verified Java cache's `clients.config` / `clients.config.d`, non-loopback `i2p.reseedURL` URLs, and `\|\| true` forgiveness of `cargo test` / `cargo fmt` / `cargo check` / `bash [[ ]]` / `javac -cp` / `java -D` calls in the lane. Plan 197 requires `Ssu2RouterAddress::parse` to accept `pq=4,3` (positive `parses_java_high_mtu_pq_options`/`parses_java_low_mtu_pq_option` regressions present), a `pq_capabilities()` accessor, `Ssu2PqKem`/`PqCapabilities`/`MAX_SSU2_PQ_SCHEMES` re-exports from `crates/i2pr-transport-ssu2/src/lib.rs`, and no `pq` key in any `props.setProperty(...)`-style push or matching branch in `crates/i2pr-transport-ssu2/src/publication.rs` (a literal in a comment is fine). Second-family Java rows stay `failed` with stop provenance until Plan 236 external execution closes `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`. | yes | yes |
| `scripts/check-m6-final-closure-evidence.sh` | M6 closure | Plan 199 evidence-consuming final gate: requires exact-head workflow provenance, exact i2pd/Java pins, all mandatory cross-family rows passed, and exactly one Java public-client ledger with no blocked, failed, or missing mandatory rows. Manual external-workflow only; it is **not** a routine-CI substitute. | **no** | no — `m6-mixed-router-external.yml` only |
| `scripts/check-m11-transit-qualification-evidence.sh` | M11 transit | Plan 264/265 M11 qualification evidence integrity for per-epoch and frozen-scenario-family rows. | yes | no — `m11-transit-external.yml` only |
| `scripts/check-m12-floodfill-qualification-evidence.sh` | M12 floodfill | Plan 279 §9 M12 floodfill qualification evidence integrity: guarded i2pd (10 rows, budget 1) and Java (12 rows, budget 3) matrix rows flow through the exit-code-gated `record_guarded`/`driver_row` helpers with per-row evidence-key gates; exact reference pins, loopback bind, frozen per-lane attempt budgets, `--ignored --exact` selection, no `\|\| true` forgiveness, no reference patching. `--self-test` proves the gates against synthetic fixtures. | yes | yes (`--self-test`) |
| `scripts/check-http-anonymity-evidence.sh` | anonymity | Thin bash wrapper that resolves the evidence root (default `target/interop/anonymity/http-profile-evidence`) and delegates to the Python checker. | **no** | **no** |
| `scripts/check-http-anonymity-evidence.py` | anonymity | HTTP anonymity profile evidence integrity; takes repo root + evidence root. Invoked by the wrapper above, and runnable directly. | **no** | **no** |
| `scripts/check-streaming-fingerprint-evidence.sh` | anonymity | Validates a Plan 312 Streaming-fingerprint evidence directory (takes the evidence dir as its single argument; parses CSV rows). | **no** | **no** |

### Source-lock and planning hygiene

| Script | What it catches | Floor | CI |
| --- | --- | --- | --- |
| `scripts/check-java-source-lock-gating.sh` | Verifies the Plan 246 Java source-lock test set in `crates/i2pr-daemon/tests/java_tunnel_external.rs`, its driver `scripts/run-java-source-lock-tests.sh`, and the `ci.yml` wiring. Requires bash 4 `mapfile`. | **no** | yes |
| `scripts/check-global-plan-number-uniqueness.py` | Global plan-number ownership across `plans/` — no duplicate plan numbers across the planning tree. | yes | yes |
| `scripts/check-adr-number-uniqueness.py` | **Plan 361.** Fails closed on duplicate ADR numbers under `docs/adr/`, with an explicit, ledger-linked tolerated set for the three known duplicate pairs (`0030`, `0032`, `0033`). **No ADR was renumbered.** It was added to the floor because a merge once produced an unparseable workflow file, so unguarded CI-adjacent structure had no automated check at all. | yes | yes |
| `scripts/check-workflow-validity.py` | **Plan 365.** Parses `.github/workflows/*.yml` as YAML and fails closed on an invalid file, so a bad indentation cannot silently disable the `quality`, `msrv`, and `dependency-policy` jobs. Not hypothetical: merge `0d50319` de-indented a step line in `ci.yml` at `2416c30` and the whole file stopped parsing, so those jobs never ran. | yes | yes |
| `tests/planning/test_global_plan_number_uniqueness.py` | unittest coverage for the plan-number checker; run via `python3 -m unittest discover -s tests/planning -p 'test_*.py'`. | yes | yes |
| `tests/planning/test_adr_number_uniqueness.py` | unittest coverage for the Plan 361 ADR-number checker, including the tolerated-duplicate set; same `unittest discover` invocation. | yes | yes |
| `tests/planning/test_workflow_validity.py` | unittest coverage for the Plan 365 workflow-validity checker; same `unittest discover` invocation. | yes | yes |

### Opt-in runners (not in the floor, not in CI by design)

| Script | Posture |
| --- | --- |
| `scripts/fuzz-smoke.sh` | Opt-in smoke run of **all 25 fuzz targets** for 32 iterations each at seed=1 (`-runs=32 -seed=1`). Requires `cargo-fuzz` + nightly. Disables LeakSanitizer (`LSAN_OPTIONS=detect_leaks=0`) for managed environments. |
| `scripts/run-java-source-lock-tests.sh` | Java source-lock test driver. Fails closed unless `I2PR_M6_JAVA_SOURCE_ROOT` names a Git checkout at the exact Java I2P 2.13.0 pin `9134f808337b401e8e53c73734c81fab04280c9d`. Driven by `scripts/check-java-source-lock-gating.sh`. |

### Pruned / historical (not on disk)

| Script | Status |
| --- | --- |
| `scripts/check-plan095-workflow.sh` | **Pruned.** Removed by the Plan 099 harness reduction; the file is not on disk. Only `plans/`, `docs/`, and skill text still mention it, as audit context. It is **not** a live command — do not add it to the floor or CI. |

### `scripts/interop/` — harness apparatus (69 files)

The interop subtree is the Plan 038/040/041/043/045–053 apparatus plus
the M6/SAM/SSU2/I2CP fetch and evidence helpers. It is documented in
detail by [interop-apparatus.md](interop-apparatus.md); this is the
file-level inventory.

- **Reference build and host contract** (10): `build-references.sh`,
  `build-i2pd.sh`, `build-java-i2p.sh`, `ubuntu/check-host.sh`,
  `ubuntu/setup-host.sh`, `java-prepare-template.py`,
  `cache-manifest.py`, `validate-build-contract.py`,
  `validate-scenarios.py`, `validate-evidence.py`.
- **Reference fetch (exact pins)** (5): `fetch-i2cp-clients.sh`,
  `fetch-m6-java.sh`, `fetch-sam-clients.sh`,
  `fetch-service-tunnel-clients.sh`, `fetch-ssu2-reference.sh`.
- **Scenario execution and gates** (10): `run-scenario.sh`,
  `run-matrix.sh`, `run-gate.sh`, `run-ntcp2-loopback-smoke.sh`,
  `run-minimal-i2pd-host-loopback-probe.py`, `reset-lane-state.sh`,
  `cleanup.sh`, `verify-clean-host.sh`, `verify-isolation.sh`,
  `offline-reuse.sh`.
- **Evidence assembly** (2): `aggregate-evidence.py`,
  `plan056_drive_bundles.py`.
- **Lane-local checkers** (2): `check-m6-java-response-source-lock.sh`,
  `check-p243-host-qualified.sh` (both documented above).
- **Host probes and shared shell** (6): `probe-constrained-host-lanes.sh`,
  `probe-rootless-sandbox.sh`, `rootless-enter.sh`, `lib/common.sh`,
  `lib/namespaces.sh`, `aggregate-evidence.py`'s sibling
  `anonymity/` pair below.
- **Anonymity lane** (2): `anonymity/preflight-ubuntu.sh`,
  `anonymity/record-reference-manifest.py`.
- **Multipass recovery lane** (33 files under `interop/multipass/`):
  lifecycle (`lifecycle.py`, `create.sh`, `destroy.sh`, `status.sh`,
  `snapshot.sh`, `restore.sh`, `config.py`, `environment.toml`),
  cloud-init (`cloud-init.yaml`, `cloud-init-status.sh`,
  `cloud_init_status.py`), dispatch and direction
  (`dispatch-gate.sh`, `run-direction.sh`, `run-matrix.sh`,
  `run-evidence-lane.sh`), evidence (`collect.py`, `aggregate.py`,
  `export.py`, `export-evidence.sh`, `records.py`, `host_state.py`,
  `sidecars.py`, `source_tree.py`, `common.sh`), offline and transfer
  (`prepare-offline.sh`, `offline-enforcement.json`,
  `transfer-cache.sh`, `transfer-source.sh`, `selective-purge.sh`),
  verification (`probe.sh`, `verify-base.sh`, `verify-clean-host.sh`),
  and `README.md`.

**How they work**: `check-dependency-direction.sh` uses
`cargo metadata` + Python. The static boundary and evidence checkers
use `rg` (ripgrep) for pattern scanning; the fixture/vector checkers
use `sha256sum` / `find` / bash 4 associative arrays for manifest
integrity. `fuzz-smoke.sh` delegates to `cargo fuzz run`.

## Floor and CI coverage

Every `AGENTS.md` floor command still maps to a script that exists —
no floor entry is missing. The gaps run in the other direction.

### Counting method

Two conventions are in use in this file, and they answer different
questions. Both are stated here so the numbers can be recomputed rather
than trusted.

**Method A — "invocation": what the floor and CI actually execute.**
Counted from the command text, not from the tables below.

```sh
# floor steps in the AGENTS.md routine-floor block that invoke a checker
sed -n '/^## Routine floor/,/^```$/p' AGENTS.md | grep -c 'scripts/check-'
# distinct checkers executed by CI
grep -oE '(bash|python3) scripts/check-[A-Za-z0-9._-]+' .github/workflows/ci.yml \
  | grep -oE 'scripts/check-[A-Za-z0-9._-]+' | sort -u | wc -l
# checkers on disk (non-recursive; add `find scripts -name 'check-*'` for the
# scripts/interop/ ones)
ls scripts/check-* | wc -l
```

**Method B — "inventory row": this document's own checker tables.**
Count the rows in *Boundary checkers*, *Fixture and vector checkers*,
*Evidence-integrity checkers*, and *Source-lock and planning hygiene*
whose `Floor` / `CI` column reads `yes`. This is the convention the
**gap lists** below use.

Recomputed 2026-10-07 (Plan 372; Plan 367's figures were already stale
by one commit):

| Figure | Method A | Method B |
| --- | ---: | ---: |
| Floor steps invoking a checker | 47 | 47 rows marked `Floor: yes` |
| Total routine-floor steps | 57 | — |
| Checkers executed by `ci.yml` | 37 | 40 rows marked `CI: yes` |
| `check-*` files on disk | 55 (58 with `scripts/interop/`) | 57 checker rows |

Method B counts the `tests/planning/` rows too, because those are floor
steps in their own right.

**What the previous figures counted, and why they were replaced.** The
2026-10-05 line read *"25 floor checkers, 22 in `ci.yml`, 33
`check-*` files on disk"*. That triple is **not reproducible under
either method, even at its own commit** (`52c38bf`, the parent of the
Plans 360–366 batch): Method A gives 37 / 29 / 47 and Method B gives
28 / 26 / 39 rows. So the old absolute was already stale and undocumented
when written — it used a narrower filter that this file never stated.
It is not silently replaced with a different convention; both are
recorded above and the gap lists below are Method B, as before.

What did change, and is the real delta from Plans 360–366, is:

| Figure | before (52c38bf) | now | change |
| --- | ---: | ---: | --- |
| Floor steps invoking a checker (A) | 37 | 40 | +3 |
| Checkers executed by `ci.yml` (A) | 29 | 30 | +1 |
| `check-*` files on disk (A) | 47 | 49 | +2 |
| Checker rows in the four tables (B) | 39 | 44 | +5 |
| Rows marked `Floor: yes` (B) | 28 | 34 | +6 |

The +3 floor steps are the Plan 361 ADR-number guard, the Plan 365
workflow-validity guard, and the Plan 364 m12 boundary guard entering
the floor; the +2 files are the first two. Method B moves further than
Method A because two guards were missing inventory rows entirely —
the Plan 361 and Plan 365 checkers had **no row at all** despite being
in the floor, which is the A3 finding this section now records — and
because Plan 367 added their `tests/planning/` companions. The m12 row
also moved from `**no**`/`**no**` to `yes`/`yes` in Plan 364.

**In the floor but not in `ci.yml` (11):**
(the previous "(4)" list was itself stale — it omitted `check-service-tunnel-boundaries.sh`, which *is* in `ci.yml`, and seven floor checkers that never were)

- `bash scripts/check-adr-number-uniqueness.py` (Plan 361)
- `bash scripts/check-config-secret-hygiene.sh`
- `bash scripts/check-els2-type11-transcript-boundary.sh`
- `bash scripts/check-encrypted-service-consumer-caller.sh`
- `bash scripts/check-floodfill-type5-serve.sh`
- `bash scripts/check-m11-per-epoch-composition.sh`
- `bash scripts/check-m11-transit-qualification-evidence.sh`
- `bash scripts/check-outproxy-request-path.sh`
- `bash scripts/check-outproxy-wire-lane-evidence.sh`
- `bash scripts/check-service-anonymity-boundaries.sh`
- `bash scripts/check-workflow-validity.py` (Plan 365)

**In `ci.yml` but not in the floor (1):**

- `bash scripts/check-java-source-lock-gating.sh`

**On disk but in neither the floor nor `ci.yml` (8 in `scripts/`, 11
including `scripts/interop/`):**

- `scripts/check-streaming-fingerprint-evidence.sh` — takes a plan argument.
- `scripts/check-http-anonymity-evidence.sh` and
  `scripts/check-http-anonymity-evidence.py` — need a Plan 308 manifest that
  does not exist (Plan 308 blocked).
- `scripts/check-outproxy-request-path.py` and
  `scripts/check-outproxy-wire-lane-evidence.py` — the Python halves of two
  guards whose shell halves *are* in the floor.
- `scripts/check-m6-final-closure-evidence.sh` — manual external workflow only.
- `scripts/check-rootless-interop-boundary.sh`
- `scripts/check-multipass-interop-boundary.sh` — both historical-lane
  checkers, green; they police closed Plans 046/048 lanes.
- `scripts/interop/check-m6-java-response-source-lock.sh`
- `scripts/interop/check-p243-host-qualified.sh`

`scripts/check-m12-floodfill-boundaries.sh` has left this list: it is now
in **both** the floor and `ci.yml`, green, per Plan 364.

**In a manual external workflow only (3):**
`check-rootless-interop-boundary.sh`
(`ntcp2-interop-rootless.yml`), `check-m6-final-closure-evidence.sh`
(`m6-mixed-router-external.yml`), `check-m11-transit-qualification-evidence.sh`
(`m11-transit-external.yml`).

A new checker must be added here with its class, its floor/CI
disposition, and its lane. Absence from both the floor and CI is a
deliberate decision that needs a plan-of-record, not an oversight.

## Fixture and vector corpora

Four corpora live under `tests/fixtures/`. Each uses a TSV manifest with
SHA-256 hashes, classification, and provenance; no corpus carries
secrets, tokens, or operational keys.

| Corpus | Manifest | Rows | `.hex` files | Classification split | Checker |
| --- | --- | --- | --- | --- | --- |
| `tests/fixtures/i2np/` | `manifest.tsv` (pipe-delimited) | 31 | 31 | 15 positive / 16 negative | `scripts/check-fixture-manifest.sh` |
| `tests/fixtures/ntcp2/crypto/` | `manifest.tsv` + `vectors.tsv` | 5 + 13 | 4 | 4 positive / 1 malformed | `scripts/check-ntcp2-vectors.sh` |
| `tests/fixtures/ssu2/` | `manifest.tsv` | 13 | 13 | 12 positive / 1 malformed | `scripts/check-ssu2-vectors.sh` |
| `tests/fixtures/i2cp/` | `manifest.tsv` | 29 | 29 | 22 positive / 7 malformed | `scripts/check-i2cp-vectors.sh` |

### `tests/fixtures/i2np/` — I2NP wire fixture corpus

- `manifest.tsv` — 31 entries (15 positive, 16 negative) with id, path,
  classification, SHA-256, source (official I2NP specification),
  revision, generator, deterministic input description, expected
  decode/error, license (CC-BY), and independence
  (`locally-authored`). The separator is `|`, not a tab.
- **31 `.hex` files** — hand-crafted binary fixtures exercising
  DeliveryStatus, DatabaseLookup (none/legacy/ecies),
  DatabaseSearchReply, DatabaseStore (classic LeaseSet / compressed
  RouterInfo), TunnelData, TunnelGateway (nested),
  VariableTunnelBuild, ShortTunnelBuild, Garlic/Data
  deferred-length, plus 16 malformed variants (bad checksum,
  truncated header, oversized payload, trailing bytes, unknown
  type, invalid flags, excessive counts, zero IDs).
- **Verified by** `scripts/check-fixture-manifest.sh` in routine CI
  (Linux only).

### `tests/fixtures/ntcp2/crypto/` — NTCP2 cryptographic vector corpus

- `manifest.tsv` — 5 rows: the 4 `.hex` vectors plus `vectors.tsv`
  itself, with SHA-256 integrity and provenance
  (`independent-python-cryptography-41.0.7`, `local-format-seed-25`,
  `local-plan-034-*`).
- `vectors.tsv` — 13 named deterministic vectors (after two comment
  header lines) covering X25519 key exchange, protocol name hash,
  transcript initial/final hash, SessionRequest/SessionCreated/
  SessionConfirmed AEAD, ChaCha20-Poly1305 seal, AES-CBC ephemeral,
  and Split-KDF outputs.
- Hex files: `storage-static-key.hex`, `data-phase-frame.hex`,
  `data-phase-blocks.hex`, `data-phase-malformed.hex`.
- **Verified by** `scripts/check-ntcp2-vectors.sh` in routine CI
  (Linux only).

### `tests/fixtures/ssu2/` — SSU2 v2 vector corpus (Plans 155–157)

- `manifest.tsv` — 13 rows (12 positive, 1 malformed) with SHA-256
  integrity and explicit provenance
  (`spec-derived-constructed-vector` for the Plan 155 set,
  `locally-authored-deterministic-vector` for Plans 156/157).
- Hex files: `long-header.hex`, `short-header-data.hex`,
  `short-header-confirmed.hex`, `blocks-positive.hex`,
  `blocks-malformed.hex` (Plan 155 spec-derived constructed
  vectors), plus `handshake-initial.hex`,
  `header-protection-request.hex`, `token-request.hex`,
  `token-retry.hex`, `session-created-full.hex`,
  `session-confirmed-frag.hex` (Plan 156 locally-authored
  deterministic handshake vectors), plus `data-phase-first.hex`
  and `data-phase-ack.hex` (Plan 157 locally-authored deterministic
  data-phase vectors, reproduced byte-for-byte through the session
  path). No private keys, tokens, or operational secrets.
- **Verified by** `scripts/check-ssu2-vectors.sh` in routine CI
  (Linux only).

### `tests/fixtures/i2cp/` — I2CP wire fixture corpus (Plan 164)

- `manifest.tsv` — 29 entries (22 positive, 7 malformed) with id,
  path, category, SHA-256, and provenance
  (`locally-authored-deterministic-vector`).
- Hex files: `protocol-byte`, framed `get-date`/`set-date`,
  `create-session`/`reconfigure-session`/`destroy-session`,
  `session-status-created`, `request-variable-leaseset`,
  `create-leaseset2` (Standard LeaseSet2 + test-only decryption
  key), `send-message`/`send-message-expires`,
  `message-payload`/`message-status-accepted`,
  `get-bandwidth-limits`/`bandwidth-limits`, `dest-lookup`,
  `dest-reply-destination`/`dest-reply-hash`, `disconnect`,
  `host-lookup-hostname`, `host-reply-success`/`host-reply-failure`,
  plus malformed unknown/deprecated/oversize/truncated/trailing/
  bad-destination/short-signature negatives. Fixed test-only
  keys/inputs are documented in the corpus README; no secrets present.
- **Verified by** `scripts/check-i2cp-vectors.sh` in routine CI
  (Linux only).

## Integration and interop lanes

Ten lane directories live under `tests/integration/`. Each lists its
real driver scripts and its matching evidence checker.

| Lane | Drivers | Evidence checker(s) | Manual workflow |
| --- | --- | --- | --- |
| `tests/integration/anonymity/` | `run-plan312-streaming.sh`, plus `test_streaming_fingerprint.py`, `test_http_capture.py`, `canonicalize_http_capture.py` (unittest), data files `http-corpus.toml`, `streaming-scenarios.toml`, `topology.toml`, `references.lock.toml`, `reference-diversity-matrix.md`, `README.md` | `check-http-anonymity-evidence.sh` / `.py`, `check-streaming-fingerprint-evidence.sh`, `check-service-anonymity-boundaries.sh` | none |
| `tests/integration/floodfill/` | `run-i2pd.sh`, `run-java-floodfill.sh` | `check-m12-floodfill-qualification-evidence.sh`, `check-m12-floodfill-boundaries.sh` | none |
| `tests/integration/i2cp/` | `run-independent.sh` (plus `external/`) | `check-i2cp-acceptance-evidence.sh` | `i2cp-external.yml` |
| `tests/integration/i2pcontrol/` | `run-differential.sh` (plus `evidence/`) | `check-i2pcontrol-acceptance-evidence.sh` | none |
| `tests/integration/m11-transit/` | `run-i2pd.sh` | `check-m11-transit-boundaries.sh`, `check-m11-transit-qualification-evidence.sh`, `check-m11-per-epoch-composition.sh` | `m11-transit-external.yml` |
| `tests/integration/m6-interop/` | `run-preflight.sh`, `run-tunnels.sh`, `run-netdb.sh`, `run-destination.sh`, `run-streaming.sh`, `run-java.sh`, `run-m6-mixed-router.sh` (plus `java/`) | `check-m6-mixed-router-acceptance-evidence.sh`, `check-exploratory-tunnel-evidence.sh`, `check-netdb-tunnel-evidence.sh`, `check-destination-tunnel-evidence.sh`, `check-streaming-tunnel-evidence.sh`, `check-m6-final-closure-evidence.sh`, `interop/check-m6-java-response-source-lock.sh`, `interop/check-p243-host-qualified.sh` | `m6-mixed-router-external.yml` |
| `tests/integration/ntcp2/` | `manifest.toml` + Python `harness/` (23 modules, incl. `test_execution_lane.py`); subtrees `config/`, `evidence/`, `evidence-receipts/`, `mixed-scenarios/`, `qualification/`, `reference-drivers/`, `reference-observation-qualification/`, `reference-scenarios/`, `scenarios/` | `check-ntcp2-interoperability.sh`, `check-constrained-host-lane-boundary.sh`, `check-rootless-interop-boundary.sh`, `check-multipass-interop-boundary.sh`, `interop/check-p243-host-qualified.sh` | `ntcp2-interop-ubuntu.yml`, `ntcp2-interop-rootless.yml`, `ntcp2-interop-host-loopback-development.yml` |
| `tests/integration/sam/` | `run-independent.sh` (plus `clients/build.sh`, `reference/`, `clients/`, `evidence.md`, `README.md`) | `check-sam-acceptance-evidence.sh` | `sam-external.yml` |
| `tests/integration/service-tunnels/` | `run-independent.sh`, `run-plan213-generic.sh`, `run-plan214-applications.sh`, `test-plan215-tunnels-conf.sh`, `hold_sam_session.py` (plus `clients/`, `fixtures/`) | `check-service-tunnel-acceptance-evidence.sh`, `check-service-tunnel-boundaries.sh` | `service-tunnels-external.yml` |
| `tests/integration/ssu2/` | `run-independent.sh` | `check-ssu2-acceptance-evidence.sh` | `ssu2-external.yml` |

`tests/integration/ntcp2/harness/test_execution_lane.py` is also run in
routine CI (`ci.yml`, Linux) via
`python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`.

### `tests/integration/ntcp2/` — synthetic interoperability lane (Plan 036)

- `manifest.toml` — defines a synthetic test network
  (`network_id = "synthetic-private-036"`), loopback-only, fixed
  clocks, disposable identities. Pins reference implementations at the
  exact full revisions recorded in `references.lock.toml`
  (`lock_version = "plan-040-v1"`): **Java I2P 2.12.0**
  (`2800040deee9bb376567b671ef2e9c34cf3e30b6`) and **i2pd 2.60.0**
  (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`). These are the frozen
  NTCP2-era synthetic-lane pins; the current SSU2/M6 lanes pin Java I2P
  2.13.0 and i2pd 2.61.0 per [`AGENTS.md`](../../AGENTS.md). The lock
  also pins IzPack 5.2.4. Specifies exactly **8** `[[scenario]]`
  entries:
  1. `java-ipv4-inbound-outbound` — authenticated handshake + I2NP
     exchange.
  2. `java-ipv6-inbound-outbound` — same, IPv6.
  3. `java-adversarial-and-resource` — boundary / oversized padding
     rejection.
  4. `java-duplicate-link-race` — deterministic winner/loser drain.
  5. `i2pd-ipv4-inbound-outbound` — same as java-ipv4.
  6. `i2pd-ipv6-inbound-outbound` — same as java-ipv6.
  7. `i2pd-adversarial-and-resource` — same as java-adversarial.
  8. `i2pd-duplicate-link-race` — same as java-duplicate-link.
- `evidence/` — contains only `README.md`. The README states that
  `i2pr` daemon activation is disabled; Java I2P and i2pd lanes are
  "recorded blockers, not skipped successes."
- `README.md` — explains this is manual / opt-in, requires an
  authorized external runner, and that
  `cargo test -p i2pr-testkit --all-targets` is the local substitute.
- **Verified by** `scripts/check-ntcp2-interoperability.sh` in routine
  CI (Linux only): required disclaimer lines (`network_id`,
  `public_network = false`, `reseed = false`, …), exactly 8
  `[[scenario]]` entries, and a scan of the committed `evidence/`
  directory for forbidden artifacts (`.pcap`, `.pcapng`,
  `router.identity`, `ntcp2.static.key`, private key headers).

### Plan 038 Ubuntu harness (implemented foundation, opt-in)

Plan 038 extends the manual lane with an Ubuntu-only, amd64-only
harness. The existing manifest and evidence preflight remain a
repository boundary; they do not install or launch reference routers.
The host and build commands are:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
```

Reference builds are source-pinned and hashed during preparation. The
runner then uses a separate execution phase for each scenario. It
creates one i2pr namespace and one reference namespace, moves both ends
of a veth pair out of the host namespace, permits only the expected
directly connected routes, and rejects default routes, DNS, host
bridges, and public egress before launch. Route checks are primary;
namespace-scoped nftables rules are defense in depth. Execution has no
dependency downloads, reseed, bootstrap, RouterInfo publication, NetDB
mutation, or public endpoint.

The scenario and launcher interfaces are:

```text
bash scripts/interop/run-scenario.sh --scenario <id> --reference java_i2p --build-cache <path> --run-root <path>
bash scripts/interop/run-scenario.sh --scenario <id> --reference i2pd --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh --profile environment-smoke
bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

The launcher uses `i2pr-runtime` as its only Tokio owner. A valid
disposable scenario creates or reloads private identity/static-key
state, verifies the published RouterInfo endpoint, runs the selected
listener or dial handshake, promotes the authenticated frame owner,
and exchanges DeliveryStatus before cleanup. Its status protocol is
versioned and redacted; state, handshake, data-phase, timeout, and
cleanup failures are typed rejections. A successful launcher run is
local driver validation only, never mixed-router evidence.

The evidence taxonomy is strict: environment smoke validates reference
startup/RouterInfo generation and cleanup only; Plan 041's
`reference-crosscheck-ipv4` profile runs two dedicated directional
Java-I2P/i2pd control scenarios in `reference-scenarios/`, with a
separate `java-*`/`i2pd-*` topology, explicit network ID 99, staged
RouterInfo validation/import, and dual authenticated observations. It
remains harness control evidence; i2pr mixed-router evidence requires
bounded authenticated runs between i2pr and each reference in both
directions. The full eight-scenario manifest remains gated on the
positive smoke profiles. Sanitize before retention and keep only typed
outcomes, bounded run metadata, and artifact/configuration hashes. Delete
raw addresses, peer identities, RouterInfo, I2NP, keys, transcripts,
logs, and arbitrary remote error text. A missing host, cache, strict
parser, or authoritative observation remains a typed blocker; NTCP2
remains experimental/non-advertised.

### Plan 040 corrective apparatus

Plan 040 hardens that foundation. The machine identifiers are exactly
`java_i2p` and `i2pd`; the exact source revisions, cache metadata
schema, topology token rules, firewall semantics, implementation-specific
runtime paths, and evidence finalization order are recorded in
[interop-apparatus.md](interop-apparatus.md). The cache summary is
`target/interop/cache/current-cache.json`; sanitized run results are
written to `target/interop/evidence/`, while `target/interop/runs/` is
always deleted after cleanup. A successful environment smoke result
remains harness validation only and cannot advertise NTCP2.

### Plan 043 build-system gates

Plan 043 owns the build-system promotion boundary. The semantic gate
order is `contract` → `reference-build` → `reference-offline-reuse` →
`environment-smoke` → `reference-crosscheck-ipv4` →
`i2pr-handshake-smoke-ipv4` → `full-matrix` → `evidence-validation` →
`cleanup-verification`. The contract gate is unprivileged and does not
start routers. Preparation is the only network-enabled phase; offline
reuse and all scenario profiles consume verified caches and
namespace-local synthetic links.

The exact host is Ubuntu 24.04 amd64/x86_64 with Bash 4+, UTF-8 locale,
non-interactive `sudo` when needed, Linux namespace/nftables capability,
and at least 4 GiB free under `target/`. The setup package list, full
source pins, IzPack digest, cache schema, and build-command versions are
authoritative in `tests/integration/ntcp2/references.lock.toml`. Host
evidence records Ubuntu, kernel, architecture, Rust/Cargo, Java/Ant,
compiler/CMake, Python, iproute2, and nftables; the aggregate manifest
adds workflow run and attempt metadata.

The offline gate restores only a cache selected through
`target/interop/cache/current-cache.json`, validates strict schema-2
metadata, and re-hashes the complete runtime tree. Its key includes the
canonical reference (`java_i2p` or `i2pd`), full source revision, lock
digest, `ubuntu-24.04-amd64`, build-command version, and relevant
tool/ABI versions. It must not fetch, clone, install, resolve DNS, or
fall back on a cache miss. Runtime configs, identities, keys, RouterInfo,
NetDB state, run roots, raw logs, namespace state, and evidence records
are never cache inputs.

The reference crosscheck is a control, not an i2pr result; it must pass
before the four independent i2pr/reference IPv4 directions can run. The
full profile adds bounded malformed, replay, timeout, resource, race,
cancellation, and failure-cleanup scenarios, but not unbounded fuzzing.
The evidence gate validates an aggregate manifest and a narrow
sanitized upload allowlist. Cleanup runs with an always-run policy, and
Plan 043 requires an independent `verify-clean-host.sh` check for
residual namespaces, veths, processes, secret-bearing run roots,
forbidden files, and attributable host firewall or route changes. The
workflow and helper apparatus expose this manual lane, but documentation
of the contract is not a claim that the lane has passed.

### Rust integration test files under `tests/`

**Corrected by Plan 372.** This section previously asserted "there are
**zero** `.rs` files under `tests/`", and repeated the claim under
*Distinctive design choices*. It was false, and false in the direction
that hides a real dependency edge:
`tests/portable-service-tunnel-consumer/tests/conformance.rs` is tracked,
so the portable service-tunnel consumer is exercised through an
integration test outside its own crate rather than an in-crate
`#[cfg(test)]` module.

That is **1 tracked `.rs` file** under the root `tests/` tree, out of 145
`.rs` files across `crates/*/tests/` plus in-crate modules. The
separation this section describes is otherwise intact: decode/encode
verification does live inside the crates, and the root `tests/` tree is
still fixture data, lane drivers, and the interoperability manifest.
`scripts/check-tooling-inventory.py` rule 10 now derives this count.

## `fuzz/` — opt-in fuzz workspace

- **Separate workspace** (`fuzz/Cargo.toml` declares an empty
  `[workspace]` — standalone).
- **Package**: `i2pr-proto-fuzz`, edition 2021, `publish = false`,
  `[package.metadata] cargo-fuzz = true`.
- **Dependencies**: `i2pr-crypto`, `i2pr-proto`, `i2pr-storage`,
  `i2pr-transport-ntcp2`, `libfuzzer-sys 0.4`.
- **25 fuzz targets** declared as `[[bin]]` in `fuzz/Cargo.toml`, each
  with a matching `fuzz/fuzz_targets/*.rs`, plus one shared
  `support.rs` module (26 `.rs` files in the directory).

| Area | Targets |
| --- | --- |
| Primitive codecs | `date`, `date32`, `hash`, `mapping` |
| Identity / certificate | `certificate`, `key_certificate`, `key_and_cert`, `router_identity`, `destination`, `router_address`, `router_info` |
| Lease structures | `lease`, `lease_set`, `leaseset2`, `metaleaseset` |
| I2NP | `i2np_standard`, `i2np_bodies`, `i2np_short_ssu`, `i2np_short_transport` |
| NTCP2 | `ntcp2_transcript`, `ntcp2_storage`, `ntcp2_handshake`, `ntcp2_blocks`, `ntcp2_frames` |
| Shared support (not a target) | `support.rs` — defines `COMMON_MAX` (1 MiB) and `I2NP_MAX` (62,724) with a `within()` guard |

**Scope correction**: fuzzing covers datatypes, the LeaseSet/LeaseSet2
datastore structures, I2NP message/framing decode, and NTCP2 crypto and
framing. There are **no** fuzz targets for Streaming packets, SAM or
I2CP framing, or tunnel records/fragmentation. Do not claim fuzz coverage
for subsystems that have no target on disk.

### How to drive

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir fuzz ntcp2_handshake -- -runs=10000
bash scripts/fuzz-smoke.sh   # all 25 targets, 32 iterations each
```

`fuzz-smoke.sh` is opt-in: it is **not** in the AGENTS.md floor and
**not** in CI, because it requires `cargo-fuzz` + nightly.

### Corpus

- `fuzz/corpus/<target>/` directories with seed files; most include
  a `seed-oversized-shape`.
- `fuzz/corpus/metadata.toml` records provenance: all seeds locally
  authored, no third-party bytes.

## `.cargo/config.toml`

```toml
[term]
color = "auto"
```

Minimal. No rustflags, no target-dir overrides, no custom
subcommands, no hidden `-Z` flags. Deliberately clean.

## Plan 161 SSU2 external lane (passed, retained)

The independent SSU2 driver is
`crates/i2pr-runtime/tests/ssu2_independent.rs`. It is compiled by
`cargo check --workspace --all-targets` but its single
environment-dependent test is explicitly ignored during ordinary
workspace and direct-executable test runs. The dedicated loopback lane
must provision the exact-pinned i2pd 2.61.0 reference and select it
with:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

The driver remains fail-closed when `I2PD_ROUTER_INFO`,
`I2PD_SSU2_ENDPOINT`, `I2PR_SSU2_BIND`, `I2PR_SSU2_FLOODFILL`, or
`EVIDENCE_DIR` is absent. Routine CI does not provision an external
peer; Plan 162 established this as a test-lane boundary, not as a
protocol or interoperability skip.

The full Plan 161 lane derives all 15 required rows from executed
commands (local focused suites plus the single explicit driver
invocation) with no literal `passed` bookkeeping:

```text
bash tests/integration/ssu2/run-independent.sh
bash scripts/check-ssu2-acceptance-evidence.sh
```

The checker is enforced in routine Linux CI and the manual
`.github/workflows/ssu2-external.yml` lane. Do not weaken it to make
CI pass.

## `.github/` — CI

Ten workflows exist. One is the ordinary gate; the other nine are
`workflow_dispatch`-only manual external lanes.

### `.github/workflows/ci.yml` (ordinary gate, three jobs)

| Job | OS | Steps |
| --- | --- | --- |
| **Quality** | ubuntu-latest + macos-latest (matrix, `fail-fast: false`) | Checkout → ripgrep install (Linux only) → Rust 1.95.0 + rustfmt + clippy → Cargo cache → `cargo fmt --all --check` → `cargo check --locked --workspace` → `cargo check --locked --workspace --all-targets` → tests → `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` → `cargo doc --locked --workspace --no-deps` (with `RUSTDOCFLAGS: -D warnings`) → 24 checker invocations |
| **MSRV** | ubuntu-latest | Rust **1.89.0** → `cargo check --locked --workspace --all-targets` |
| **Dependency policy** | ubuntu-latest | `cargo deny check advisories bans sources` (via `EmbarkStudios/cargo-deny-action@v2`) |

Triggers: `on: push`, `on: pull_request` (all branches).
`permissions: contents: read`.

**The 24 checker invocations, exactly** (all bare `if: runner.os ==
'Linux'` unless noted):

1. `bash scripts/check-dependency-direction.sh` (both OS)
2. `python3 scripts/check-global-plan-number-uniqueness.py` (both OS)
3. `python3 -m unittest discover -s tests/planning -p 'test_*.py'` (both OS)
4. `bash scripts/check-runtime-boundaries.sh`
5. `bash scripts/check-fixture-manifest.sh`
6. `bash scripts/check-ntcp2-vectors.sh`
7. `bash scripts/check-ssu2-vectors.sh`
8. `bash scripts/check-i2cp-vectors.sh`
9. `bash scripts/check-ntcp2-interoperability.sh`
10. `bash scripts/check-constrained-host-lane-boundary.sh`
11. `bash scripts/check-sam-acceptance-evidence.sh`
12. `bash scripts/check-ssu2-acceptance-evidence.sh`
13. `bash scripts/check-i2cp-acceptance-evidence.sh`
14. `bash scripts/check-i2pcontrol-acceptance-evidence.sh`
15. `bash scripts/check-service-tunnel-acceptance-evidence.sh`
16. `bash scripts/check-exploratory-tunnel-evidence.sh`
17. `bash scripts/check-netdb-tunnel-evidence.sh`
18. `bash scripts/check-destination-tunnel-evidence.sh`
19. `bash scripts/check-streaming-tunnel-evidence.sh`
20. `bash scripts/check-m6-mixed-router-acceptance-evidence.sh`
21. `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test`
22. `bash scripts/check-m11-transit-boundaries.sh`
23. `bash scripts/check-java-source-lock-gating.sh`
24. `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`

**macOS test pattern.** The workspace includes several real loopback
listener suites, and macOS runners become flaky when Cargo launches
those test binaries together. macOS therefore builds the complete test
set once with `cargo test --locked --workspace --no-run
--message-format=json-render-diagnostics`, extracts every
`compiler-artifact` executable with `.profile.test == true` via `jq`,
and then runs each executable serially with a single libtest worker
(`"$executable" --test-threads=1`), finishing with `cargo test --locked
--workspace --doc`. Non-macOS runners use the single
`cargo test --locked --workspace -- --test-threads=1` invocation. Use
`--test-threads=1` locally for the `i2pr-daemon` and `i2pr-runtime`
loopback suites.

### Manual external lanes (all `workflow_dispatch`-only)

| Workflow | Lane |
| --- | --- |
| `ssu2-external.yml` | Plan 161: fetch/verify the exact i2pd 2.61.0 reference → run `scripts/check-ssu2-acceptance-evidence.sh` → run `tests/integration/ssu2/run-independent.sh` → upload sanitized evidence even on failure. Bounded 45-minute timeout; no public-I2P participation beyond the GitHub source fetch. |
| `sam-external.yml` | SAM external clients: `bash scripts/interop/fetch-sam-clients.sh --rebuild` → `bash tests/integration/sam/clients/build.sh` → `scripts/check-sam-acceptance-evidence.sh` → `tests/integration/sam/run-independent.sh` → upload sanitized evidence. Ubuntu 24.04, bounded 30-minute timeout. |
| `i2cp-external.yml` | Plan 172 (Plan 170 retained): install ant/JDK/Go → fetch/verify the exact Java I2P 2.13.0 + go-i2cp pins → `scripts/check-i2cp-acceptance-evidence.sh` → `tests/integration/i2cp/run-independent.sh` (24 fail-closed rows: 9 retained Plan 170 wire/data-plane rows + 15 counted Plan 172 lifecycle rows) → upload sanitized evidence. Bounded 45-minute timeout; loopback-only. |
| `service-tunnels-external.yml` | Plan 181 (local rows green, remote rows `blocked` per §6.3): install i2pd build deps + netcat → fetch/verify the exact jaraco/irc pin and i2pd 2.61.0 → `scripts/check-service-tunnel-acceptance-evidence.sh` → `tests/integration/service-tunnels/run-independent.sh` (which delegates the remote section to `run-plan214-applications.sh`; `run-plan213-generic.sh` covers the generic rows) → upload sanitized evidence. The full lane exits nonzero while the remote rows stay blocked (fail-closed by design); `--local-only` skips only the i2pd section and still records the remote rows as blocked. Bounded 45-minute timeout; loopback-only. |
| `m6-mixed-router-external.yml` | Plan 189 §8: install i2pd build deps + ant/JDK/gettext-base → fetch/verify the exact i2pd 2.61.0 reference → run the per-layer checkers (`check-exploratory-`, `check-netdb-`, `check-destination-`, `check-streaming-tunnel-evidence.sh`) and the cross-family checker `scripts/check-m6-mixed-router-acceptance-evidence.sh` → `tests/integration/m6-interop/run-m6-mixed-router.sh` → `scripts/check-m6-final-closure-evidence.sh` → upload sanitized evidence. Java family rows record `failed` with stop provenance until Plan 236 external execution closes the response-emission observability gap. Bounded 60-minute timeout; loopback-only. |
| `m11-transit-external.yml` | Plan 264/265 M11 exact-pinned i2pd controlled transit. Takes `epoch`, `epoch_pass`, and `scenario` dispatch inputs and runs a frozen 8-attempt matrix. Runs `scripts/check-m11-transit-boundaries.sh`, `scripts/check-m11-transit-qualification-evidence.sh`, and `tests/integration/m11-transit/run-i2pd.sh`. Bounded 90-minute timeout. |
| `ntcp2-interop-ubuntu.yml` | Plan 038 Ubuntu harness. Runs `check-dependency-direction.sh`, `check-ntcp2-interoperability.sh`, `check-runtime-boundaries.sh`, and the `ntcp2/harness` tests. Historical lane. |
| `ntcp2-interop-rootless.yml` | Plan 046 rootless sealed-namespace lane. Runs `check-dependency-direction.sh`, `check-ntcp2-interoperability.sh`, `check-rootless-interop-boundary.sh` (twice — pre- and post-dispatch), `check-runtime-boundaries.sh`, and the `ntcp2/harness` tests. Historical lane. |
| `ntcp2-interop-host-loopback-development.yml` | Historical host-loopback development lane. Intentionally has **no** `pull_request` trigger. Runs the `ntcp2/harness` tests and `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`. |

The three `ntcp2-interop-*` workflows are retained historical
apparatus. NTCP2 is experimental and non-advertised, and normal-daemon
NTCP2 is disabled per Plan 101; do not extend those lanes without a new
plan-of-record.

### `.github/dependabot.yml`

- Cargo ecosystem: weekly, max 5 open PRs.
- GitHub Actions: weekly, max 5 open PRs.

## Reference pins

**Do not change a pin without a new plan.** Full rationale lives in
[`specs/SOURCES.md`](../../specs/SOURCES.md) and
[`docs/provenance/`](../../docs/provenance/).

| Reference | Pin | Role |
| --- | --- | --- |
| i2pd | `2.61.0` @ `635b013a612ff47278ef02acf8580a28e10e26c5` | **mandatory** — primary C++ interoperability reference |
| Java I2P | `2.13.0` @ `9134f808337b401e8e53c73734c81fab04280c9d` | **secondary** — primary Java family |
| go-i2cp | `b529ee1c10a6011558b4d69fc9436a4afc489eac` | mandatory for the I2CP counted lane |
| i2psam | `b80ecd48…` | counted SAM client |
| i2plib | `6edf51cd…` | counted SAM client |
| Java I2P / i2pd (NTCP2 synthetic lane) | Java I2P `2.12.0` @ `2800040deee9bb376567b671ef2e9c34cf3e30b6`; i2pd `2.60.0` @ `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` | frozen NTCP2-era synthetic-lane pins in `tests/integration/ntcp2/references.lock.toml` |
| IzPack | `5.2.4` | Java I2P build installer digest, in the same lock file |

### Proposal 170 provenance pins (Plan 286)

Frozen for the Proposal 170 / I2PControl workstream. Only
manifest-listed Proposal 170 files receive the ADR 0028 reuse
exception. Source: `docs/provenance/proposal-170-manifest.md`.

| Reference | Pin |
| --- | --- |
| Proposal 170 (I2PControl Expansion, Open) | revision `2026-05-20`, source text SHA-256 `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |
| eggstack/emissary fork master | `6885a945d25a5ae61bc68191d27c5816bc3df4c9` |
| eepnet/emissary upstream master | `9b43484a21d5a1291c4881cdae62a36c527f8c0f` |
| Java I2PControl Proposal 170 PR 6 head | `45bb593000408071dd376b78848fdc246dccd964` |

## Environment-gated test discipline

Lanes that need a reference router, a captured corpus, or an external
client are `#[ignore]`-gated. This is a fail-closed contract, not a
formality:

- An ordinary run **compiles** the gated test and **skips** it.
- An explicit run requires `--ignored --exact` plus the single exact
  test name.
- **Missing environment must fail, never silently pass.** A driver with
  absent env must exit nonzero, not record a skip.
- Forbidden: `|| true`, `continue-on-error`, filename filtering, fake
  peer env, broad exclusions, early-return-success, and production wire
  changes made to go green.
- Raw reference logs are never evidence. Only sanitized counts and
  hashes reach evidence files.
- The environment-gated driver is the single
  `crates/i2pr-runtime/tests/ssu2_independent.rs` SSU2 lane; it stays
  fail-closed on `I2PD_ROUTER_INFO`, `I2PD_SSU2_ENDPOINT`,
  `I2PR_SSU2_BIND`, `I2PR_SSU2_FLOODFILL`, and `EVIDENCE_DIR`.

## Skills inventory

Six skill bundles ship under `.opencode/skills/`. `.agents/skills` is a
**symlink to `../.opencode/skills`** — the same directory, not a second
copy. There is no separate `.skills/` directory, and the historical
references to `.codex/` are stale.

| Skill | Status | Use |
| --- | --- | --- |
| `i2pr-architecture` | active | ADR/plan navigation, per-crate deep-dive ownership, doc-vs-source audits |
| `i2pr-local-dev` | active | the local product path — destinations/garlic/LeaseSet2/Streaming, SAM 3.1, SSU2, I2CP, service tunnels |
| `i2pr-planning` | active | registering plans, closure records, `registry.md`, subsystem roadmaps, unblock audit |
| `i2pr-ntcp2-interop` | **historical / read-only** | archaeology over the closed Plan 038–100 NTCP2 lane; never for routine work |
| `i2pr-rootless-sandbox` | **historical / read-only** | the closed Plan 046 rootless lane |
| `i2pr-multipass-recovery` | **historical / read-only** | the closed Plan 048–053 Multipass lane |

Load the matching skill before touching a lane; the skills are the
source of truth for harness detail.

## `tools/`

- `tools/i2pr-interop/` — the non-production interop launcher crate
  (`Cargo.toml` + `src/`). It is the 20th workspace member and is
  `publish = false`; no production crate may depend on it.
- `tools/generate-els2-independent-fixture.py` — independent
  EncryptedLeaseSet2 fixture producer.
- `tools/generate-red25519-independent-fixture.py` — independent
  Red25519 fixture producer.
- `tools/i2pd-red25519-oracle.cpp` — the i2pd-side Red25519 oracle
  used when producing the independent fixture.

## Top-level `Cargo.toml` — workspace configuration

### Members (28 crates + 1 non-production binary = 29)

Recomputed 2026-10-07 by Plan 372 from `cargo metadata --no-deps`; the roster
below was six crates short, missing the whole operator console and the entire
managed-application runtime (`i2pr-app-proto`, `i2pr-app-manager-proto`,
`i2pr-app-package`, `i2pr-app-state`, `i2pr-appctl`, `i2pr-appd`,
`i2pr-apphost`, and the `i2pr-app-fixture` evidence crate).

```text
crates/i2pr-addressbook, crates/i2pr-api, crates/i2pr-crypto,
crates/i2pr-proto, crates/i2pr-client, crates/i2pr-console,
crates/i2pr-core, crates/i2pr-daemon, crates/i2pr-i2pcontrol,
crates/i2pr-netdb, crates/i2pr-netdb-persist, crates/i2pr-runtime,
crates/i2pr-service-tunnels, crates/i2pr-storage, crates/i2pr-su3,
crates/i2pr-testkit, crates/i2pr-transport,
crates/i2pr-transport-ntcp2, crates/i2pr-transport-ssu2,
crates/i2pr-tunnel,
crates/i2pr-app-proto, crates/i2pr-app-manager-proto,
crates/i2pr-app-package, crates/i2pr-app-state, crates/i2pr-appctl,
crates/i2pr-appd, crates/i2pr-apphost, crates/i2pr-app-fixture,
tools/i2pr-interop
```

`resolver = "2"`, `edition = "2024"`, `rust-version = "1.89"`,
the MSRV raised from `1.88` by Plan 357 because every published
`eggserve-server` release declares `rust-version = "1.89"`,
workspace version `0.1.0`. `crates/i2pr-testkit` and
`tools/i2pr-interop` are non-production crates (`publish = false`), and
no production crate may depend on the testkit.

### Workspace lints

```toml
[workspace.lints.rust]
unsafe_code = "deny"
unexpected_cfgs = "deny"
unused_must_use = "warn"

[workspace.lints.clippy]
dbg_macro = "deny"
todo = "deny"
unimplemented = "deny"
```

### Profile overrides

```toml
[profile.dev]
overflow-checks = true

[profile.test]
overflow-checks = true

[profile.release]
panic = "unwind"
lto = false
```

Overflow checks are enabled in dev and test profiles. Release uses
unwinding panics and no LTO (fast builds over binary size).

## The routine floor

The `AGENTS.md` handoff floor, cross-checked against disk. **Every entry
resolves to a script that exists.** `scripts/fuzz-smoke.sh` is opt-in and
is listed separately in [Opt-in runners](#opt-in-runners-not-in-the-floor-not-in-ci-by-design).

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appctl
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
python3 scripts/check-managed-app-gateway-boundary.py
python3 scripts/check-managed-app-manager-boundary.py
python3 scripts/check-managed-app-process-boundary.py
python3 scripts/check-managed-app-process-boundary.py --self-test
python3 scripts/check-portable-service-tunnel-api.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m11-transit-qualification-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-i2pcontrol-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
cargo deny check advisories bans sources
```

### Host limitation: macOS bash 3.2

The four fixture/vector checkers
(`check-fixture-manifest.sh`, `check-ntcp2-vectors.sh`,
`check-ssu2-vectors.sh`, `check-i2cp-vectors.sh`) use bash 4
`declare -A` and `check-java-source-lock-gating.sh` uses bash 4
`mapfile`. macOS ships bash 3.2, so those five exit early on this host
with a shell-syntax/`command not found` error. This is a **host
limitation, not a fixture fault** — the same scripts pass in Linux CI.
Do not "fix" the scripts to accommodate a local shell; run them under
bash 4 (e.g. `brew install bash` and invoke with an explicit path) or
rely on CI.

## Distinctive design choices

1. **Almost no Rust integration test files under `tests/`.** Exactly one
   is tracked — `tests/portable-service-tunnel-consumer/tests/conformance.rs`,
   added for the Plan 350 portable-core boundary. Decode/encode
   verification otherwise lives inside the crates.
2. **Dual-toolchain CI.** Production builds use 1.95.0; MSRV
   verification runs 1.89.0 separately. The toolchain is pinned in
   `rust-toolchain.toml`. (Plan 372 corrected this row: it read 1.88.0,
   which was the floor before Plan 357 raised it for `eggserve-server`.)
3. **Python in several checkers.** `check-dependency-direction.sh` and
   the planning checker use Python 3 stdlib for JSON/YAML-free
   parsing, alongside the bash-4 vector checkers.
4. **`unsafe_code = "deny"` workspace-wide** combined with clippy
   denies on `dbg!`, `todo!`, `unimplemented!`. Very strict lint
   posture.
5. **Fuzz workspace fully isolated.** Declares its own `[workspace]`
   with no members, depends on production crates by path, and uses
   edition 2021 (not 2024).
6. **Interoperability is a documented blocker, not a skip.** The
   evidence directory README explicitly says Java I2P and i2pd
   lanes are "recorded blockers, not skipped successes."
7. **`LSAN_OPTIONS=detect_leaks=0` in fuzz-smoke.** Disabled
   because managed CI runs sanitizer binaries under ptrace, which
   triggers false LeakSanitizer aborts.
8. **Manifest-driven fixture integrity.** The I2NP, NTCP2,
   SSU2, and I2CP corpora use TSV manifests with SHA-256 hashes,
   classification, provenance, and independence tracking.
9. **Edition 2024 in production, 2021 in fuzz.** Likely because
   `libfuzzer-sys` / `cargo-fuzz` aren't edition-2024-compatible
   yet.
10. **Top-level [`AGENTS.md`](../../AGENTS.md)** is the canonical
    developer guide — read it before changing code, alongside
    `README.md`, `GUARDRAILS.md`, the applicable `plans/NNN-*.md`, and
    relevant `docs/adr/` records.

## Cross-references

- [Overview](overview.md)
- [Dependency graph](dependency-graph.md)
- [Interop apparatus](interop-apparatus.md)
- [`AGENTS.md`](../../AGENTS.md)
- [`CONTRIBUTING.md`](../../CONTRIBUTING.md)
- [`GUARDRAILS.md`](../../GUARDRAILS.md)
- [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md)
- [`specs/SOURCES.md`](../../specs/SOURCES.md)
- [`specs/support.toml`](../../specs/support.toml)
- [`docs/provenance/`](../../docs/provenance/)
- Plan-of-record: latest active `plans/NNN-*.md`
