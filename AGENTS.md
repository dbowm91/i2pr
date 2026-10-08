# AGENTS.md

`i2pr` is an experimental Rust I2P router. **Not production-ready.** No anonymity/privacy claim. NTCP2 is experimental and non-advertised; SAM/I2CP/service-tunnels/console are loopback-only, disabled by default, non-advertised.

## Read first

1. `README.md` — status snapshot (may lag).
2. `GUARDRAILS.md` — non-negotiable security/architecture constraints.
3. `CONTRIBUTING.md` — conventions.
4. `plans/README.md` (planning system guide: registry, roadmaps, closure records) + load `i2pr-planning` when registering or closing out a plan.
5. `specs/support.toml` + `specs/CONFORMANCE.md` before claiming any protocol support.

Authority order: closure records > executable tests/scripts > ADRs > prose (see `plans/README.md`).

## Toolchain

Pinned Rust `1.95.0` (`rust-toolchain.toml`); MSRV `1.89` (`cargo check --locked --workspace --all-targets` must pass on both). Workspace `edition = "2024"`, `resolver = "2"`. MSRV was raised from `1.88` for the router console: every published `eggserve-server` release (0.2.0–0.4.0) declares `rust-version = "1.89"`, and 1.89 is the exact threshold — the rest of the locked graph tops out at 1.88.0. Lints deny `unsafe_code`, `clippy::dbg_macro`, `clippy::todo`, `clippy::unimplemented` — protocol/client/API/service crates stay `#![forbid(unsafe_code)]` unless separately reviewed.

## Workspace boundaries

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-app-proto` — runtime-neutral managed-app contract for identity, capabilities, framing, manifests, policy, and attestation; no OS/runtime ownership or production workspace dependencies.
- `i2pr-app-package` — signed `.i2prapp` verification and immutable local store; no trust/grant/catalog/launch authority.
- `i2pr-app-state` — persistent offline policy generations and package-verified launch decisions; cannot construct `LaunchAuthority`.
- `i2pr-appctl` — offline package and policy administrator CLI; no router listeners, runtime, or network clients.
- `i2pr-crypto` — protocol crypto wrappers (no local primitives).
- `i2pr-storage` — identity/key persistence.
- `i2pr-core` — runtime-neutral contracts/budgets/health.
- `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` — runtime-neutral, no Tokio/sockets/`async fn`.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo/LeaseSet2/ELS2 validation/store.
- `i2pr-su3` — bounded, runtime-neutral SU3 framing and signature verification.
- `i2pr-tunnel` — runtime-neutral exploratory/transit pool, short-build, data plane.
- `i2pr-client` — destination lifecycle, ECIES session/routing, Streaming.
- `i2pr-api` — runtime-neutral SAM 3.1 + I2CP wire/state (no sockets).
- `i2pr-addressbook` — canonical `.i2p` naming owner, precedence resolver, versioned generations (no I/O).
- `i2pr-i2pcontrol` — Proposal 170 JSON-RPC 2.0 wire/domain contract (no I/O).
- `i2pr-service-tunnels` — runtime-neutral tunnel config/policy (no sockets; daemon owns listeners).
- `i2pr-console` — loopback router console substrate (HTML/CSS/JS assets, themes, browser security, read-only overview). Owns **no socket** and has **zero** workspace dependencies; `axum` is built without `tokio`, so the crate stays runtime-neutral. Daemon owns the listener.
- `i2pr-runtime` — sole production owner of Tokio, sockets, timers, channels, cancellation **for the router's own services**. Two exceptions are deliberate and are separate *process* trust zones, not router libraries: `i2pr-appd` and `i2pr-apphost` each own a runtime for their own process lifecycle (Plan 369), and `i2pr-app-fixture` is evidence tooling. They own no router state, no listener, and no route into the router — see the dependency map. Note that `check-runtime-boundaries.sh`'s manifest rule for Tokio is keyed on `^tokio[[:space:]]*=`, which does **not** match this repo's `tokio.workspace = true` style, so it currently cannot fire; the "Tokio dependencies are confined to approved runtime/testkit manifests" allowlist is consequently stale (it names only `i2pr-runtime` and `i2pr-testkit`, while `i2pr-daemon` predates even that). Recorded as an open finding in `plans/closure/managed-native-app-runtime/369-status.md`; closing it needs its own plan-of-record, and the rule must be fixed rather than relaxed.
- `i2pr-daemon` — CLI/config/composition root; owns SAM/I2CP/I2PControl/service-tunnel/console listeners.
- `i2pr-testkit` — deterministic fixtures only; no production crate may depend on it.
- `tools/i2pr-interop` — non-production test launcher.

Enforced by `scripts/check-dependency-direction.sh`, `scripts/check-runtime-boundaries.sh`, `scripts/check-console-boundaries.sh`, and `scripts/check-console-browser-security.sh`. Details: `docs/architecture/overview.md`.

## Managed application runtime (Plans 368–371 and 382–383; experimental, disabled by default)

`i2pr-appd` is the supervised manager process and `i2pr-apphost` is the **only**
component that execs an application. Both are separate process trust zones that
reach nothing but the two wire contracts, are spoken to over **inherited
anonymous pipes** (no listener, no port, no discovery endpoint), and resolve
their own siblings via `current_exe()` — never configuration, never a shell,
never `PATH`. `i2pr-app-fixture` is **evidence tooling**: no production crate may
name it or depend on it.

`[app_runtime]` is `deny_unknown_fields` and defaults to disabled. Enabling it
starts the production catalog, which can launch only explicitly trusted,
selected, granted, profile-configured applications with autostart enabled in
the offline policy store. Appd receives only the canonical managed-app root
from the daemon after `env_clear`; the private protocol has no
manager-receivable launch request. `UnsafeDirect` means ordinary host
networking without a sandbox. `Secured` is **refused before exec** — there is
no qualified sandbox backend, so there is no containment claim of any kind,
including for grandchildren. Policy changes require app runtime restart. A
broken manager degrades the app runtime and nothing else.

Do not describe managed-app v1 as released, stable, supported, or advertised.
Do not run a focused `cargo test -p i2pr-daemon` qualification run without
`cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` first
(see the floor).

Guards: `scripts/check-managed-app-process-boundary.py` — **run it with
`--self-test` as well** — `scripts/check-managed-app-package-boundary.py`
(`--self-test` too), `scripts/check-managed-app-policy-boundary.py`
(`--self-test` too), plus the Plan-368 gateway, manager, and private-client
seam checkers.

## Skills and architecture index

- Skill bundles live in `.opencode/skills/` (canonical); `.agents/skills` is a symlink to the same directory — there is no separate `.skills/` directory. Load `i2pr-architecture` for ADR/plan navigation and doc-vs-source audits, `i2pr-local-dev` before touching product/SSU2/SAM/I2CP/I2PControl/tunnel/transit/floodfill code, `i2pr-planning` when registering or closing out an implementation plan (roadmap/registry/closure mechanics). The NTCP2/rootless/Multipass skills are historical (closed Plans 038–100/046/048 lanes) — read-only for archaeology, never for routine work.
- Architecture entry points: `docs/architecture/overview.md` (crate index, data flow, capability snapshot); `docs/architecture/dependency-graph.md` (dependency allowlist, mirrors `check-dependency-direction.sh`); `docs/architecture/tooling.md` (scripts, fixtures, lanes, CI); `docs/architecture/i2pr-<crate>.md` (per-crate deep-dives, one per workspace member); `docs/architecture/interop-apparatus.md` (closed NTCP2 apparatus, archaeology only); `docs/adr/` (decisions 0000–0035; note the duplicate `0030-*`, `0032-*` and `0033-*` pairs); `specs/CONFORMANCE.md` (what counts as evidence); `specs/support.toml` (machine-readable support inventory). Latest drift audit: `docs/architecture/audit/`.

## Hard boundaries (CI-enforced — fix code, never weaken scripts)

- Preserve dependency direction; no prod dep on `i2pr-testkit`.
- No unbounded channels/queues; no `tokio::*`/`std::net`/`std::fs`/raw `JoinHandle`/ownerless `spawn` in transport/API/service crates.
- Every spawned task has explicit ownership/cancellation; channel/socket close is a lifecycle event, not retried blindly.
- Listeners bind loopback by default; non-loopback needs explicit config + auth design. SAM/I2CP/service-tunnels/console stay disabled by default.
- Router console (Plans 356–358): experimental, loopback-only, disabled by default,
  non-advertised. `i2pr-console` may not depend on any `i2pr-*` crate, open a
  socket, or name `tokio`/`std::net`; it reaches router state only through its
  `ControlClient` trait. The console is **read-only**: the
  `LocalConsolePrincipal` allow-set is `RouterInfo` + `ClientServices`, and it
  must never gain `TunnelManager`, `AddressBook`, a mutating method, or an
  external I2PControl connection. `[console]` rejects a non-loopback bind at
  parse time; requests must match an exact `Host` authority including the port.
  Do not vendor Halloy or any other third-party GPL theme files — the bundled
  palettes are original i2pr work. `check-console-boundaries.sh` and
  `check-console-browser-security.sh` enforce this; never weaken them to pass.
- Secrets: no `Debug`/`Display`/unrestricted serialization on secret types; avoid `Clone` on secrets; zeroize where supported; never log `PRIV`, signing seeds, SSU2 static/session secrets, tokens, or raw payloads. Do not make `DestinationIdentity: Clone` or mint a second private identity for a bridge.
- Treat all network/config/disk bytes as hostile and bounded: checked arithmetic, caller-visible alloc caps, exact-consumption decodes, typed errors (no `anyhow` in library crates; no swallowed codec results).
- No patching/vendoring external routers/clients; no root/sudo/namespaces/containers/VM/systemd/public-I2P for routine acceptance.
- Outbound proxy (Proposal 170): the runtime-neutral provider policy
  (`i2pr-service-tunnels/src/outproxy.rs`) and the daemon route owner
  (`i2pr-daemon/src/outproxy_route.rs`) may only route through an I2P
  Streaming connection — never a direct clearnet socket, resolver, TLS
  client, plugin load, or process spawn. `check-service-tunnel-boundaries.sh`
  rules 9–11 enforce this, with `std::net::IpAddr` deliberately allowed so
  the target grammar can refuse IP literals. Policy and route owner exist;
  there is currently **no reachable request path** (`open_via_outproxy` has
  zero callers) and no direct clearnet fallback. Do not describe this as a
  working outproxy.
- No capability/version/RouterInfo/SAM/I2CP behavior advertisement beyond tested subset (`specs/CONFORMANCE.md`).

## Routine floor (from repo root, before handoff)

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 scripts/check-portable-service-tunnel-api.py
bash scripts/check-portable-service-tunnel-consumer.sh
python3 -m unittest discover -s tests/planning -p 'test_*.py'
bash scripts/check-runtime-boundaries.sh
bash scripts/check-console-boundaries.sh
bash scripts/check-console-browser-security.sh
python3 scripts/check-tooling-inventory.py
python3 scripts/check-license-metadata.py
python3 scripts/check-managed-app-private-client-seams.py
python3 scripts/check-managed-app-package-boundary.py
python3 scripts/check-managed-app-package-boundary.py --self-test
python3 scripts/check-managed-app-policy-boundary.py
python3 scripts/check-managed-app-policy-boundary.py --self-test
bash scripts/check-service-tunnel-boundaries.sh
python3 scripts/check-managed-app-gateway-boundary.py
python3 scripts/check-managed-app-manager-boundary.py
bash scripts/check-m11-per-epoch-composition.sh
bash scripts/check-service-anonymity-boundaries.sh
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
bash scripts/check-els2-type11-transcript-boundary.sh
bash scripts/check-encrypted-service-consumer-caller.sh
bash scripts/check-outproxy-request-path.sh
bash scripts/check-outproxy-wire-lane-evidence.sh
bash scripts/check-config-secret-hygiene.sh
python3 scripts/check-workflow-validity.py
bash scripts/check-floodfill-type5-serve.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
bash scripts/check-m12-floodfill-boundaries.sh --self-test
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
cargo deny check advisories bans sources
```

macOS CI builds all test executables once then runs each with `--test-threads=1` (loopback suites flake under parallel Cargo). Use `--test-threads=1` locally for `i2pr-daemon`/`i2pr-runtime` suites. After changing committed fixture bytes, also run `bash scripts/check-fixture-manifest.sh`; after NTCP2/SSU2/I2CP fixture changes run the matching `check-*-vectors.sh`.

**Why the floor builds the managed-app sibling binaries explicitly.** Every
other floor line emits only test harnesses under `target/debug/deps`;
`cargo check`, `cargo test --all-targets` and `cargo clippy` never produce the
plain `target/debug/<name>` binaries. Plans 369/383's black-box qualification in
`crates/i2pr-daemon/src/app_runtime_qualification.rs` execs **real sibling
executables** (`i2pr-app-fixture-manager`, the production `i2pr-appd`, and
`i2pr-apphost`), so without that build line it could run binaries left over
from the last build. Its
`assert_fresh` staleness guard catches exactly this and fails closed — verified
on 2026-10-07, when `cargo test --workspace --all-targets --no-run` left
`target/debug/i2pr-app-fixture-manager` at its pre-existing mtime. Run the same
build before any *focused* `-p i2pr-daemon` run, not just the full floor.

**macOS/bash-3.2 trap.** Seven floor/evidence checkers need **bash 4+** and do not
declare it. macOS ships bash 3.2.57, so on this host they exit 2 and the failure
looks like content drift:

- `check-fixture-manifest.sh`, `check-ntcp2-vectors.sh`,
  `check-ssu2-vectors.sh`, `check-i2cp-vectors.sh` — `declare: -A: invalid option`.
- `check-streaming-tunnel-evidence.sh` — the same `declare -A` construct, at
  line 66 (`HELPER_USAGE`). Re-confirmed on this host 2026-10-06; it was
  missing from this list until then, and its exit 2 is **not** content drift.
- `check-java-source-lock-gating.sh` — needs `mapfile` (exit 127).
- `check-service-tunnel-acceptance-evidence.sh` — a parse error, because line
  ~1667 puts a `<<'PY'` heredoc inside a `$( )` command substitution. Bash 3.2
  mis-parses that and reports the failure ~287 lines later, at line 1954, which
  is a red herring. Line 1954 is where the error actually surfaces; do not go
  looking for a defect there.

Run these under a Homebrew bash 5 or on CI. Do not report them as passing
locally, and do not "fix" the scripts to work around it.


Focused examples (same `--locked` + `--test-threads=1` pattern):

```text
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-console --all-targets
cargo test --locked -p i2pr-daemon --test console_loopback -- --test-threads=1
```

### Known checker gaps (a green floor is not full coverage)

Recorded 2026-10-05; re-audited 2026-10-06. Treat these as real coverage
holes, not as licence to add the forbidden edge. A plan-of-record is required
to close each one; the rule is still to fix the boundary, never to weaken a
script.

- ~~`scripts/check-dependency-direction.sh` had 18 expected-map keys for 20
  workspace members~~ — **CLOSED by Plan 356.** `i2pr-tunnel` and
  `tools/i2pr-interop` now have explicit allowlist entries.
  `check-console-boundaries.sh` rule 7 now asserts that every `i2pr-*`
  workspace member appears in the map, so the gap cannot silently reopen when
  a crate is added. **The count is now enforced, not asserted here**: after
  Plans 382–383 added three managed-app crates. Do not re-state a count in
  this file — verify the exact set instead:
  `cargo metadata --no-deps` set-membership against the map's keys.
- ~~`tools/i2pr-interop` is unpoliced by the direction script~~ — **CLOSED by
  Plan 356** for production edges (see the entry above). It is still outside
  `check-runtime-boundaries.sh`, whose globs cover `crates/` only.
- ~~`scripts/check-runtime-boundaries.sh` has **no `i2pr-api` section**~~ —
  **CLOSED by Plan 362.** The crate now has an 8-rule section with grouped-import
  positive controls, and the script normalises `use … { … }` groups into flat
  leaves before scanning, so `use std::{fs, net};` can no longer evade.
- ~~a grouped `use std::{…}` evades the `std::net` scan~~ — **CLOSED for
  `check-runtime-boundaries.sh` by Plan 362** (a `use`-tree parser, validated
  over 493/493 repo `.rs` files with zero fail-open cases). The option-(b) regex
  alternative was explicitly rejected: it is a small diff that leaves nested
  groups working.
- ~~`check-console-boundaries.sh` rule 2 enforces almost nothing~~ — **CLOSED by
  Plan 366**, and it was the worst of these. Its awk latched `in_tests = 1` on
  the first `#[cfg(test)]` and **never reset per file**; `find` returns
  `theme.rs` first and its `#[cfg(test)]` is at line 1139, so **only
  `theme.rs:1–1139` was ever scanned and 12 of 13 console files were never
  examined**. A `TcpListener::bind` injected into `security/auth.rs` passed.
  Rule 2 is now a per-file, brace-scoped scanner: 13/13 files, 4 416 production
  lines (was 1 138). The blanket `std::net` string ban was replaced by a
  socket-keyed ban plus an explicit 4-type address-value allow-set, because
  `AGENTS.md`'s exact-`Host`-with-port rule requires `IpAddr`/`SocketAddr`.
- ~~`scripts/check-m12-floodfill-boundaries.sh` exits 1~~ — **CLOSED by Plan
  364.** The stale Plan-281 "type 5 is deferred" rule was replaced by 9 positive
  assertions traced to the Plans 332/333/334/346 closure records, each
  negative-tested. It is now in this floor and in `ci.yml`.
- ~~ADR numbers are not uniqueness-checked~~ — **CLOSED by Plan 361.**
  `scripts/check-adr-number-uniqueness.py` fails closed, and the three existing
  duplicate pairs (`0030`, `0032`, `0033`) are an explicit, ledger-linked
  tolerated set. No ADR was renumbered.
- **CI workflow validity was unchecked, and a merge broke it.** Plan 365 adds
  `scripts/check-workflow-validity.py` to the floor. It was not hypothetical:
  `.github/workflows/ci.yml` did not parse as YAML at `2416c30`, because the
  merge `0d50319` de-indented one step line — so the whole file was rejected and
  the `quality`, `msrv`, and `dependency-policy` jobs never ran.
- **Open: the transport crates already import `std::net` address values via
  grouped `use`** (`i2pr-transport-ntcp2/src/address.rs` and four ssu2 files),
  and the transport scan is deliberately excluded from the Plan 362 normaliser
  for that reason. A transport-scoped plan should settle the socket-keyed rule
  there the way Plan 366 did for the console.

Separately, a product-path defect found 2026-10-05 was **closed by Plan 360**
(see `plans/closure/workspace-foundation/360-status.md`): `i2pr run` used to
exit `ReadinessTimeout` with no listener, because the Essential `lifecycle`
service awaited cancellation and never signalled initial readiness. `i2pr run`
now starts, binds its configured loopback listeners, and shuts down cleanly.
The readiness contract is *"this service is running"* — anchors signal
immediately, listener services signal **after** the bind succeeds, and periodic
workers signal once their cadence loop exists. Readiness signals are
owner-tracked; a listener service must never report ready before it has bound.

A second startup defect was **closed by Plan 371**
(see `plans/closure/managed-native-app-runtime/371-status.md`): the supervisor
awaited initial readiness for *every* service and failed router startup for any
that never signalled, **regardless of `ServiceClassification`**, and honoured
`RestartExhaustion::Degrade` only after startup. So an optional subsystem could
not be made non-blocking, and Plan 369 invariant 1 was unimplementable. The fix
is a `StartupRequirement::{Required, Optional}` policy orthogonal to
classification:

- An optional subsystem the router must survive being broken registers
  `.startup_requirement(StartupRequirement::Optional)`. It then degrades its own
  feature instead of aborting startup, and is excluded from
  `SupervisorSnapshot::ready` so a usable router is not reported as unready.
- **`RestartExhaustion::Degrade` alone is not enough.** A `Restartable` service
  that degraded still gates `ready`; degrading a service and releasing router
  readiness are separate decisions, which is why `app-runtime` carries both.
- `Required` is the default, and `Optional` + `Essential` is refused at graph
  build. Never weaken that check to make a service register.
- A dependency edge constrains start *order*, not *availability*: a service that
  degraded at startup still lets its dependents start.

## Testing quirks agents miss

- Runtime tests: prefer `#[tokio::test(start_paused = true)]` / manual clock + explicit bounded deadlines; never wall-clock sleeps for overload/state-machine tests. Socket tests use `127.0.0.1:0` (OS port) and loopback only.
- Queue/resource tests must cover capacity 1, exact load, and max+1, and verify lease release on every drop path (receive/drop/timeout/cancel/panic/teardown).
- Black-box product tests (e.g. `sam_stream_self_composed.rs`) drive behavior only through TCP/SAM after listener startup — do not call private bridge/LeaseSet2/driver/pump APIs from them.
- I2CP pre-session reject path must `stream.shutdown()` before `teardown_connection`/`drop_connection` (`crates/i2pr-daemon/src/i2cp.rs`); `wrong_protocol_byte_is_closed` is strict (timeout = failure, 24-iteration baselines + non-paused companion). Never revert to drop-timing.
- SSU2 Java `pq=4,3` option is parser-tolerance only (`Ssu2PqKem`/`PqCapabilities`, `MAX_SSU2_PQ_SCHEMES = 8`); session stays classical X25519, publication stays pq-free, no ML-KEM.
- New deps need review (purpose, transitive impact, `unsafe` exposure, features, license); keep workspace versions centralized, default features narrow.

## External / interop lanes (fail-closed)

- Environment-gated tests are `#[ignore]`-gated. Ordinary run compiles but skips them; explicit run requires `--ignored --exact`, and missing env must **fail**, never silently pass. Forbidden: `|| true`, `continue-on-error`, filename filtering, fake peer env, broad exclusions, early-return-success, production wire changes to go green.
- Reference pins (do not change without a new plan): i2pd `2.61.0` (`635b013a612ff47278ef02acf8580a28e10e26c5`, mandatory); Java I2P `2.13.0` (`9134f808337b401e8e53c73734c81fab04280c9d`, secondary); go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac`; counted SAM clients i2psam `b80ecd48…`, i2plib `6edf51cd…`.
- Lane pattern: `bash tests/integration/<area>/run-*.sh` + `bash scripts/check-*-evidence.sh`. Examples: `tests/integration/ssu2/run-independent.sh`, `tests/integration/m6-interop/run-{preflight,tunnels,netdb,destination,streaming,java}.sh`, `tests/integration/service-tunnels/run-independent.sh` (delegates remote to `run-plan214-applications.sh`), `tests/integration/i2cp/run-independent.sh`. Manual lanes live in `.github/workflows/*-external.yml`.
- M6 Java Plan 236 is a bounded diagnostic, not a Java-family pass: `run-java.sh` source-locks the exact Java 2.13.0 Streaming response path through `Connection.sendPacket` → `PacketQueue.enqueue` → `I2PSession.sendMessage` and must stop at `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` when stock response emission is unobservable. Do not infer Router-A/i2pr behavior or change production code; see `plans/closure/mixed-router-interop/236-status.md`.
- Raw reference logs are never evidence; only sanitized counts/hashes reach evidence files.

## Commits and handoff

Focused commits only; no git config changes, no `--no-verify`, no force-push, no amending others. Handoff lists: files changed, behavior + tests run (exact commands/results), tests not run + why, dep changes, security-relevant decisions, deviations, remaining risks.
