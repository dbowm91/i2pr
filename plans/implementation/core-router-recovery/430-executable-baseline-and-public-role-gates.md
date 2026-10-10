# Plan 430 — Core-router executable baseline and public-role safety gates

Status: **active**. Date: 2026-10-10. Registered baseline: `main@8fd5eb10824909ddebbd99c8b3800dac71eae211`; execution branch begins at `02b428ebc4c83ce4c979b182fe91782c61f29b7a`. Re-census before production changes. Roadmap: `plans/subsystems/core-router-recovery-roadmap.md`.

## Objective and readiness

Establish one executable, source-verified baseline and explicit capability/readiness contracts shared by SSU2, reseed, NTCP2, NetDB, transit and floodfill. It is the **only immediately executable recovery plan**; no transport/public role is activated by this plan. Existing Plans 099/101, 161, 268, 303/306 and 360 are closed baseline evidence; no new prerequisite is assumed.

## Current implementation evidence

`crates/i2pr-daemon/src/config.rs::normalize_ssu2` currently rejects non-loopback SSU2, `advertise=true` and introducer service; `normalize_ntcp2` rejects enabled NTCP2. `bootstrap_daemon` uses offline reseed only. `specs/support.toml` retains experimental transport status; M11 transit off; M12 normal floodfill off. Confirm actual code versus stale prose (`plans/registry.md` and older deep dives can lag closure records). Reference exact pins in `specs/SOURCES.md`.

## Invariants, scope, and exclusions

**Invariant:** all unsupported normal profiles remain forbidden and unadvertised; one signed RouterInfo owner, durable identity separation, no public/clearnet traffic in tests, no default-on network capability. **Capability:** none; this pass is evidence infrastructure. **Infrastructure:** typed service readiness/advertisement decision contracts and a read-only, versioned capability/gate ledger with deterministic inputs. **Polish:** operator diagnostics/plan authority consistency. Out: reimplement transport, change NTCP2/SSU2 cryptography, activate transit or floodfill, edit prior closure records, manufacture claims from a test runner.

## Ordered work packages

1. **Source census.** On an exact SHA, locate and document concrete source entry points for identity loading, SSU2/NTCP2 config normalization/listener creation, RouterInfo construction/publication/withdrawal, reseed ingestion, NetDB client/store, tunnel admission, M11 and M12 role selectors. Compare test claims vs source behavior; record all unresolved contradictions with file and line evidence.
2. **Frozen profile matrix.** Distinguish `isolated_test` (non-I2P network ID, controlled peer endpoints), `controlled_interop` (isolated exact-reference peers), `normal_router` (real I2P network ID, qualified authenticated transports) and `optional_network_role` (transit/floodfill opt-in). Define readiness stages `ProcessServing`, `PeerDatabaseUsable`, `TransportUsable`, `TunnelPoolsUsable`, `ApplicationRoutingUsable` and `Degraded`. No serving-state bit can impersonate network readiness.
3. **No-false-claim gate.** Introduce a small runtime-neutral eligibility/claim contract consumed only at daemon/router-info composition: verified owner health, address reachability, independently qualified protocol/profile, operator authorization, withdrawal generation, and explicit typed reason for rejection. Avoid hardcoded trust based on config or one loopback probe. Keep old strict posture for every unsupported profile; never allow a new flag to bypass conformance.
4. **Executable negative controls.** Add integration tests and a mutation-tested static checker proving the baseline rejects all unauthorized NTCP2/SSU2/transit/floodfill public exposures, false readiness, forged eligibility evidence and unverified RouterInfo records; demonstrate that a test actually exercises each guard.
5. **Planning handoff.** Record exact contracts needed by 431/432/434; reconcile only changed *planning* and scope text, not legacy closure/history.

## Failure, restart, and contention policy

On restart, unverified reachability/health and prior in-memory test qualification expire; persisted cryptographic identity is not itself an advertisement permit. All transitions are monotone per owner generation, fail-closed during incomplete start/reconfigure and require bounded resource accounting. No network side effects in dry-run or disabled profiles.

## Compatibility/migration

Existing TOML and disabled defaults remain accepted. Any future normal/public configuration additions must be opt-in, schema-validated before binding; this plan must not normalize `enabled=true` into `supported`. No on-disk migrations.

## Verification commands and evidence

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 scripts/check-router-readiness-contract.py
python3 scripts/check-router-readiness-contract.py --self-test
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-m12-floodfill-boundaries.sh
# plus the full AGENTS.md floor, new anti-vacuity checker, and CI
```

## Execution audit — 2026-10-10

The working source was re-censused at `02b428eb` before implementation. The
registered source description is partly historical; the following are current
entry points and boundaries:

| Concern | Current source authority | Observed behavior |
| --- | --- | --- |
| Persistent router identity | `crates/i2pr-storage/src/lib.rs::IdentityStore::{load,create}`; `crates/i2pr-daemon/src/lib.rs::bootstrap_daemon` | Router signing identity is loaded from the protected data directory before bootstrap. The controlled SSU2 service still generates separate ephemeral SSU2 transport material. |
| SSU2 config and socket | `crates/i2pr-daemon/src/config.rs::{normalize_ssu2,parse_ssu2_bind}`; `crates/i2pr-daemon/src/router_i2np.rs::{ssu2_socket_config_from,Ssu2DaemonService}`; `crates/i2pr-runtime/src/ssu2_runtime.rs::Ssu2RuntimeService::start` | Config refuses advertisement and introducer service, accepts only loopback bind literals, and runtime binds the validated socket config. No public profile is activated. |
| NTCP2 config and listener | `crates/i2pr-daemon/src/config.rs::normalize_ntcp2`; `crates/i2pr-runtime/src/ntcp2_runtime.rs` | `enabled=true` is rejected before listener composition. Existing runtime handshake support is not normal-daemon activation authority. |
| Local RouterInfo construction and publication | `crates/i2pr-netdb/src/local.rs::LocalRouterInfoBuilder`; `crates/i2pr-daemon/src/bootstrap.rs`; `crates/i2pr-daemon/src/lib.rs::run_daemon` | The ordinary builder signs a validated zero-address record. The SSU2 runtime's replacement path verifies signature, freshness, local identity, `netId`, SSU2 style/key, and exact bound endpoint before installation. |
| Reseed and cache | `crates/i2pr-daemon/src/bootstrap.rs::{Bootstrap::run,run_offline_reseed}`; `crates/i2pr-netdb-persist/src/reseed_ingest.rs` | The bootstrap pipeline may load cache and optionally ingest a caller-supplied offline SU3. It has no HTTPS acquisition path. |
| NetDB client/store and tunnel composition | `crates/i2pr-daemon/src/netdb_tunnels.rs::NetDbTunnelCoordinator`; `crates/i2pr-daemon/src/service_product.rs` | These provide bounded validated store/lookup/publication and local controlled product composition. They do not establish independent public-router reachability. |
| Transit | `crates/i2pr-daemon/src/transit_volume.rs::TransitParticipation`; `crates/i2pr-daemon/src/lib.rs` composition | A real controlled transit owner exists, but ordinary daemon inspection publishes `TransitParticipation::Disabled`; no normal public opt-in is enabled by this plan. |
| Floodfill | `crates/i2pr-daemon/src/config.rs::{default_floodfill_enabled,normalize_floodfill}`; `crates/i2pr-daemon/src/floodfill.rs::{evaluate_normal_eligibility,prepare_normal_activation}`; `crates/i2pr-netdb/src/local.rs::LocalRouterInfoBuilder::build_floodfill` | A guarded opt-in state machine already exists and defaults off. It requires measured reachable SSU2 material and an opaque role permit. This is infrastructure, not proof of two-family floodfill capability. |

The `i2pr-core::router_readiness` contract now represents the four deployment
profiles, five ordered service/network stages plus `Degraded`, and independent
health, reachability, protocol qualification, operator authorization and owner
generation facts. It is a decision result, not a capability token; the existing
owner-specific RouterInfo permits remain mandatory. The new source checker
mutation-tests each decision fact and checks current activation, identity and
publication restrictions. Plan 431 must wire fresh owner-generation evidence
into the contract before any normal SSU2 address can be considered.

One source comment is inconsistent with its closure authority: the normal
floodfill eligibility comment in `crates/i2pr-daemon/src/floodfill.rs` describes
the surface as having passed two-family qualification via Plan 279, while
`plans/closure/floodfill/279-status.md` stopped before any matrix row and
`306-status.md` retained the bandwidth-selection boundary. No support claim is
derived from that comment. Plan 438 owns reconciliation before any normal
`caps=f` promotion; the prior closure records remain unchanged.

Acceptance: one checked-in truthful source/evidence inventory, reviewed profile/readiness/eligibility contract, negative tests that detect deliberate forbidden activation and false-ready mutation, and deterministic status projection with no current `specs/support.toml` promotion. Stop if branch HEAD changed incompatible owner contracts or the proposed claim gate requires unapproved architectural changes; file a new ADR or corrective. Closure `plans/closure/core-router-recovery/430-status.md` must report exact commits, command outcomes, tests not run, guard-mutation results, unresolved risk severities, and explicit 431/432/434 readiness disposition. Handoff lists source files, migrations (none expected), and deviations.
