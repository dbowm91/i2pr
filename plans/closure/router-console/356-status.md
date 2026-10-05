# Plan 356 closure — EggServe/Axum self-contained console foundation

Status: **passed-eggserve-axum-self-contained-console-foundation**.

Classification: **invariant + infrastructure**. This closure establishes an
HTTP/application boundary and an embedded asset surface. It establishes no
router capability, no control-plane read path (that is Plan 358), no browser
security claim (that is Plan 357), and no anonymity or privacy property.

## Commits

- `73506b38` — the whole Plans 356–358 implementation (crate, daemon wiring,
  checkers, docs). Plans 356/357/358 landed together because each is only
  meaningful with the others present, and because Plan 358's `ControlDispatcher`
  extraction is a precondition for the console's data path. This record
  distinguishes what each plan actually contributed.
- ADR 0034 (`docs/adr/0034-eggserve-axum-router-console-http-substrate.md`) was
  already frozen and committed in `f6036a9e`'s parent lane before implementation
  began.

The closure/registry/roadmap status transition is in the commit that adds this
record.

## Dependency review

| Dependency | Resolved | Declaration | Why |
|---|---|---|---|
| `eggserve-server` | `0.4.0` | `default-features = false, features = ["tower"]` | Direct H1 transport, connection lifecycle, admission, shutdown. The `tower` feature is the only one needed; `eggserve-core` was deliberately **not** taken, per Plan 356 §1. |
| `eggserve-primitives` | `0.2.2` | transitive | Required by `eggserve-server`. |
| `axum` | `0.8.9` | `default-features = false, features = ["form", "json", "query"]` | Application routing/extraction only. **`tokio` is deliberately excluded** so `i2pr-console` stays outside the runtime-boundary rule and never acquires a runtime. |
| `argon2` | `0.6.0` | `i2pr-console` | Plan 357 credential verification. |
| `tower` | `0.5.3` | `i2pr-console` dev-dependency | Drives the synchronous router in tests. |
| `futures-executor` | `0.3.34` | `i2pr-daemon` dev-dependency | `block_on` for the tower adapter in the loopback suite. |
| `i2pr-console` | workspace | `i2pr-daemon` | The console substrate. |

**MSRV moved `1.88` → `1.89`.** Every published `eggserve-server` release
(0.2.0–0.4.0) and `eggserve-primitives` 0.2.2 declares `rust-version = "1.89"`.
The previous locked graph topped out at exactly 1.88.0, so `cargo check`
hard-failed on the old floor once EggServe entered the graph. This is recorded
in `Cargo.toml`, `.github/workflows/ci.yml`, and `AGENTS.md`. It was chosen over
abandoning EggServe because ADR 0034's substrate decision still holds and 1.89
is the exact threshold — no larger jump than the dependency actually requires.

`Cargo.lock` is **purely additive**: 30 version lines added, **0 removed**. No
previously-resolved dependency changed version.

## Requirement-to-evidence matrix

| # | Acceptance criterion | Evidence |
|---|---|---|
| 1 | `i2pr-console` exists as a distinct browser/application crate | `crates/i2pr-console/`, registered as a workspace member. `check-dependency-direction.sh` pins it to `"i2pr-console": set()`. |
| 2 | Daemon owns a disabled-by-default loopback-only console listener | `Config::default()` has `console.enabled = false`; `console_defaults_to_disabled_on_loopback`. Non-loopback is refused at parse time by `console_rejects_non_loopback_bind_address`. |
| 3 | EggServe direct H1 is the sole console server runtime | `console.rs` builds an `eggserve_server::Server` from the bound listener. `check-console-boundaries.sh` forbids `axum::serve` and a second Hyper server anywhere in the console. |
| 4 | Axum routing executes through `TowerToEggserve` | `console.rs` calls `start_with_service(TowerToEggserve::new(router))`. `console_loopback.rs` drives real HTTP through it. |
| 5 | No `axum::serve` / second Hyper server | Checked by `check-console-boundaries.sh`; no occurrence in either crate. |
| 6 | Shell and all browser assets compiled into the package | `assets.rs` is a compile-time table with no filesystem access. `compiled_assets_are_served_with_declared_types`. |
| 7 | Normal rendering makes zero external-resource requests | Scan of `console.css`, `console.js`, `logo.svg`: no remote URL, no `<script src>`, no `@font-face`, no `@import`. The only `url(` in `logo.svg` are same-document `url(#g)` gradient fragments; its only `http://` is the SVG XML namespace identifier, which is never dereferenced. |
| 8 | No custom font dependency | No `@font-face`; platform font stacks only. |
| 9 | Theme adapter is bounded and deterministic | 64 KiB/document, depth 8, 256 keys, 9-byte colour literals; `CSS_VARIABLE_ORDER` is the only property list. `bounds_reject_oversized_deep_and_wide_documents`, `deterministic_output_for_identical_input`. |
| 10 | Imported EggPool themes have exact provenance/license evidence | **The import was not performed.** Halloy is GPL and EggPool's 50-theme licence could not be established, so it is recorded as a blocked follow-up in `assets/themes/PROVENANCE.md`. Plan 356 §7 explicitly permits this branch. Three original i2pr palettes ship instead: `i2pr-default`, `i2pr-midnight`, `i2pr-daylight`. |
| 11 | Theme CSS cannot be arbitrary source-controlled injection | Values pass through `Rgb::to_css_hex`, which can only emit `#rrggbb`; property names are compile-time constants. `malicious_theme_text_cannot_emit_arbitrary_css`, `theme_stylesheet_is_pure_custom_properties`. |
| 12 | Console and future eepsite boundaries are statically/documentarily distinct | `check-console-boundaries.sh` enforces the console side; the eepsite non-reuse rule is recorded in ADR 0034 and `docs/architecture/i2pr-console.md`. |
| 13 | Startup/shutdown/admission are supervised and bounded | Bind runs under a bounded timeout; serving runs until cancellation; EggServe `RuntimeConfig` bounds connections and read sizes. `listener_shuts_down_and_stops_accepting`. |
| 14 | No router state/control access introduced yet | Plan 356 attaches no `ControlClient`; `UnavailableControlClient` is the only implementation at this stage. Plan 358 adds the real one. |
| 15 | No router support claim changes | `specs/support.toml` is untouched. Verified by `git diff --stat 73506b38~1 73506b38 -- specs/`. |
| 16 | Full routine floor passes | See **Verification** below. |

## Asset inventory and external-resource scan

| Asset | Bytes | Notes |
|---|---|---|
| `console.css` | 5 504 | Theme custom properties consumed from `/theme.css`; no remote reference. |
| `console.js` | 3 716 | Dependency-free; bounded polling, `AbortController`, single in-flight. |
| `logo.svg` | 742 | Inline SVG; no external reference. |
| `themes/i2pr-default.toml` | 1 388 | Original i2pr palette. |
| `themes/i2pr-midnight.toml` | 1 388 | Original i2pr palette. |
| `themes/i2pr-daylight.toml` | 1 388 | Original i2pr palette. |
| `themes/PROVENANCE.md` | 2 423 | Records the blocked 50-theme import and its licence blocker. |

## Console bind validation matrix

| Input | Result |
|---|---|
| `bind = "127.0.0.1:7070"` | accepted |
| `bind = "localhost:7070"` | accepted |
| `bind = "[::1]:7070"` | accepted |
| any non-loopback literal | **rejected at parse time** — `console_rejects_non_loopback_bind_address` |
| unknown theme identifier | rejected — `console_rejects_unknown_theme_identifiers` |
| unknown keys under `[console]` | rejected — `console_rejects_unknown_keys` |
| `auth = true` with no password and no hash | rejected — `console_auth_requires_real_credential_material` |
| credential material with `auth = false` | rejected — `console_credential_without_auth_is_a_configuration_error` |
| PHC hash outside the console's own ceilings | rejected at parse time — `console_prehashed_verifier_is_validated_at_parse_time` |
| session/throttle bounds out of range | rejected — `console_session_and_throttle_bounds_are_validated`, `console_rejects_out_of_range_connection_ceilings` |
| `max_connections` exceeding the router task budget | rejected — `console_connection_ceiling_respects_the_router_task_budget` |

Runtime ceilings are applied **only when the console is enabled**, so a disabled
console can never shadow another subsystem's budget error. That was a real
regression found and fixed during the work.

## Theme parse / contrast / fallback evidence

- `every_bundled_theme_parses_and_is_readable` — all three shipped palettes parse
  and clear the WCAG AA floor.
- `unreadable_pairs_are_repaired_not_rendered` — a below-threshold pairing is
  deterministically repaired, never rendered.
- `invalid_colors_fail_closed_rather_than_defaulting_silently`.
- `malformed_theme_configuration_falls_back_to_the_default_palette`.
- `theme_name_resolution_rejects_unknown_and_oversized`.

**Defect found and fixed:** `ensure_contrast`'s fallback branch measured its
result against the wrong colour, so the repair could return a value that did not
actually clear the threshold. The test `repair must not reduce contrast` now
covers the corrected behaviour.

## `i2pr-console` dependency graph

Zero `i2pr-*` production dependencies. External production dependencies are
`argon2`, `axum` (no `tokio`), `rand_core`, `serde`, `serde_json`, `toml`,
`zeroize`. There is no HTTP client crate and no TLS crate: the console makes no
outbound request and terminates no TLS.

## Trust-zone assertion

The console owns no socket. The daemon owns the listener and passes an
`AppRouter` behind `TowerToEggserve`. Code sharing with any future eepsite
server must stop **below** the application/listener boundary; ADR 0034 records
that a shared listener or route tree is forbidden even on localhost.

## Unblock audit for Plan 357

Plan 357 was registered `blocked on 356` and required "Plan 356's real
EggServe/Axum request path". That path exists and is exercised over a real
loopback socket by `console_loopback.rs`. Plan 357's dependency is satisfied;
nothing else in the registry was blocked on 356.

## Verification

All evidence below is from the local worktree on implementation head
`73506b38`. **No hosted CI result is claimed by this record.**

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS — **4 251 passed, 0 failed, 35 ignored, 152 suites** |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS |
| `cargo test --locked --workspace --doc` | PASS — 21 suites |
| `cargo test -p i2pr-console --all-targets` | PASS — 118 unit + 23 browser-security + 9 route |
| `cargo test --locked -p i2pr-daemon --test console_loopback -- --test-threads=1` | PASS — 6 |
| `bash scripts/check-console-boundaries.sh` | PASS |
| `bash scripts/check-console-browser-security.sh` | PASS |
| `bash scripts/check-dependency-direction.sh` | PASS |
| `bash scripts/check-runtime-boundaries.sh` | PASS |
| `bash scripts/check-service-tunnel-boundaries.sh` | PASS |
| `bash scripts/check-service-anonymity-boundaries.sh` | PASS |
| `bash scripts/check-m11-transit-boundaries.sh` | PASS |
| `bash scripts/check-m11-transit-qualification-evidence.sh` | PASS |
| `bash scripts/check-m11-per-epoch-composition.sh` | PASS |
| `bash scripts/check-ntcp2-interoperability.sh` | PASS |
| `bash scripts/check-constrained-host-lane-boundary.sh` | PASS |
| `bash scripts/check-sam-acceptance-evidence.sh` | PASS |
| `bash scripts/check-ssu2-acceptance-evidence.sh` | PASS |
| `bash scripts/check-i2cp-acceptance-evidence.sh` | PASS |
| `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS |
| `bash scripts/check-exploratory-tunnel-evidence.sh` | PASS |
| `bash scripts/check-netdb-tunnel-evidence.sh` | PASS |
| `bash scripts/check-destination-tunnel-evidence.sh` | PASS |
| `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS — existing historical-label warnings remain diagnostic only |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-portable-service-tunnel-api.py` | PASS — 678 declarations |
| `bash scripts/check-portable-service-tunnel-consumer.sh` | PASS |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS — expected negative probes plus final integrity pass |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 7 tests |
| `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS — 18 tests |
| `cargo deny check advisories bans sources` | PASS — `advisories ok, bans ok, sources ok` |

### Not run locally (bash 4+ required; macOS ships bash 3.2.57)

These seven floor checkers exit 2 on this host because they use `declare -A`,
`mapfile`, or a construct bash 3.2 mis-parses. Per `AGENTS.md` they were **not**
run, are **not** reported as passing, and were **not** "fixed":

`check-fixture-manifest.sh`, `check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`, `check-streaming-tunnel-evidence.sh`,
`check-java-source-lock-gating.sh`, `check-service-tunnel-acceptance-evidence.sh`.

`check-streaming-tunnel-evidence.sh` was found to hit this same trap during this
closure and had been missing from the `AGENTS.md` list; the list now records
seven scripts, not six.

## Defects found and fixed during this work

1. `ensure_contrast` fallback measured against the wrong colour — a real
   correctness bug in the readability gate.
2. Uniform colour grammar across all theme fields (previously mixed
   `String`/`TextStyle`).
3. The supervisor aborted a healthy console listener after one second. Binding
   is now bounded; serving runs to cancellation.
4. EggServe rejected an empty `server_header`; it is dropped entirely, so **no
   `Server` header is sent**, matching "advertise nothing beyond the tested
   subset".
5. Console config defaults shadowed the SAM budget error attribution.
6. A module-path typo `i2prcontrol` → `i2pcontrol`.

## Known limitations

- The 50-theme EggPool import is **not performed**; licensing could not be
  established. Three original palettes ship instead. This is Plan 356 §7's
  explicit alternative branch, not a silent scope reduction.
- **The console has no product-reachable path.** `i2pr run` does not open any
  listener (the pre-existing Essential-`lifecycle` readiness defect recorded in
  `README.md`), so the console is currently reachable only through tests and
  direct `ConsoleServiceState` use.
- The console is experimental, loopback-only, disabled by default, and
  non-advertised. It is absent from `specs/support.toml` by design.

## Unresolved findings

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low:** the 50-theme provenance follow-up remains open (deliberate, see Known
  limitations); the console's unreachability through `i2pr run` is a pre-existing
  daemon defect, not introduced here.

## Roadmap disposition

Plan 356 is **closed** as infrastructure plus invariants. Its registration
blocked Plan 357; that block is released and Plan 357 is closed in
[`357-status.md`](357-status.md).