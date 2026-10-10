# Plan 432 status: passed — verified HTTPS SU3 cold-start reseed

Closure token: `passed-verified-https-signed-su3-cold-start-and-cache-restart`

Plan: `plans/implementation/core-router-recovery/432-verified-https-reseed-and-cold-start.md`

## Implementation

- `42360cc` — implement verified HTTPS cold-start reseeding, canonical SU3 parsing, and cache restart coverage.

The implementation adds explicitly configured HTTPS sources to the existing
reseed policy. Acquisition is disabled unless the operator enables reseeding.
It accepts only HTTPS `i2pseeds.su3?netid=2` URLs, rejects redirects and
oversized responses, has per-source and total deadlines, uses TLS server
authentication, and then independently verifies the SU3 signature against the
configured signer-certificate set. Source URLs and signer identities are not
logged. At least two distinct online host authorities and unique signer IDs
are required by configuration.

Cold start first checks the local cache. If the inventory needs reseed, the
daemon fetches without holding the bootstrap mutex, validates the complete SU3
bundle into a disposable bounded store, and only then reruns bootstrap with
the candidate bundle. Bootstrap revalidates each RouterInfo and persists the
accepted records. A subsequent process bootstrap reloads the cache. The test
does not inject an offline bundle into this path.

## Requirement-to-evidence matrix

| Requirement | Result and evidence |
| --- | --- |
| Canonical SU3 envelope and trust semantics | Corrected the previous parser's header layout, byte order, file/content type values, version/signer length fields, exact-consumption rule, and stock reseed entry naming. The values are recorded in `specs/protocols/04-reseed-netdb.md` and linked to the official specification and pinned Java I2P 2.13.0 source. |
| TLS and signed-content verification are independent | Local TLS fixtures prove a configured TLS root is required; the SU3 ingestor separately requires a configured signer certificate with matching ID and valid certificate time. Negative cases reject untrusted TLS, plain HTTP, redirects, oversized responses, wrong signer/content, and malformed SU3. |
| Empty-directory online cold start | `tests::authorized_https_reseed_bootstraps_and_reuses_cold_start_cache` serves an RSA-4096 signed valid RouterInfo bundle over a local trusted HTTPS fixture, runs the production fetch verifier, bootstraps an empty data directory, verifies one validated persisted record, then starts bootstrap again and verifies the record is reused. |
| No consent means no request | `tests::online_reseed_does_not_fetch_without_operator_consent` passes with enabled sources configured but `reseed.enabled = false`. |
| Disabled-by-default and offline compatibility | Configuration defaults and existing offline/cache integration coverage pass; no default, listener, proxy, transport, support inventory, or advertisement was enabled. |
| Plan 433 readiness | Plan 432 is closed. Plan 433 remains blocked on Plan 431, which is stopped pending an authorized independent non-loopback reference topology. |

## Commands and outcomes

All commands below were run locally on Linux; no hosted CI result is claimed.

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo check --locked --workspace --all-targets` — passed.
- `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed before the daemon qualification suite.
- `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` — **4,761 passed, 38 ignored across 186 suites** on the final implementation/test tree.
- `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed.
- `rtk proxy bash -c "RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps"` — passed.
- `rtk cargo test --locked --workspace --doc` — passed (29 suites, no doctests defined).
- `rtk cargo test --locked -p i2pr-daemon --lib tests::authorized_https_reseed_bootstraps_and_reuses_cold_start_cache -- --exact --test-threads=1` — passed.
- `rtk cargo test --locked -p i2pr-daemon --test netdb_integration -- --test-threads=1` — 40 passed.
- `rtk cargo test --locked -p i2pr-netdb --all-targets` — 277 passed.
- `rtk cargo test --locked -p i2pr-su3 --all-targets` — 3 passed.
- `rtk cargo test --locked -p i2pr-daemon --lib addressbook_fetch::tests::direct_https_reseed -- --test-threads=1` — 3 passed.
- `rtk cargo test --locked -p i2pr-daemon --lib config::tests::online_reseed -- --test-threads=1` — 2 passed.
- `rtk cargo test --locked -p i2pr-daemon --lib tests::online_reseed_does_not_fetch_without_operator_consent -- --exact --test-threads=1` — passed.
- The AGENTS routine static floor passed, including dependency direction, runtime and console boundaries, managed-app boundaries and self-tests, service-tunnel guards, Plan M11/M12 checks, vector/evidence/workflow checks, `cargo deny check advisories bans sources`, planning unit tests (51 passed), and `tests/integration/ntcp2/harness/test_execution_lane.py` (18 passed).
- `git diff --check` — passed.

The Plan 193 streaming evidence checker emitted its existing guarded-label
warnings and returned success. The planning unit-test mutation emitted its
expected missing-workflow-directory diagnostic and the full 51-test suite
passed. One intermediate Clippy run caught a manual range check, which was
changed to `RangeInclusive::contains`; the final Clippy run passed. An
intermediate full-suite compile caught an extracted test-helper visibility
error; the helper was corrected, focused integration tests passed, and the
final full suite above passed.

## Security, compatibility, and findings

- **High — prior SU3 parser defect corrected.** The previous Plan 104 parser
  used a non-stock header layout and little-endian lengths, and expected
  non-stock reseed ZIP filename/type values. A valid stock signed reseed bundle
  could not pass that parser. The corrected parser follows the official SU3
  specification and pinned Java I2P 2.13.0 `SU3File.java`; it bounds lengths,
  validates reserved bytes and signature size, and rejects trailing bytes.
- TLS certificate validation and SU3 signer verification are both required;
  a TLS-valid response alone cannot become trusted NetDB state.
- The only networked test is a local HTTPS fixture. No public reseed source,
  public I2P peer, or reference router was contacted.
- `ReadyForNetworkIntegration` in this test means the configured peer-count
  threshold is met. It is not evidence of a live authenticated router link,
  tunnel, or application path. Plan 433 owns those requirements.
- Online reseeding remains explicitly consented and disabled by default.
  `specs/support.toml` and protocol conformance claims are unchanged.

## Unblock audit and roadmap disposition

Plan 432 is passed. Plan 433 remains blocked on Plan 431 and Plan 432 because
the independently addressed non-loopback SSU2 prerequisite is still absent.
Plan 434 remains independently ready. Plans 435–439 remain blocked on their
registered predecessors. No other plan was unblocked by this closure.

Updated `plans/registry.md`, `plans/subsystems/core-router-recovery-roadmap.md`,
and the Plan 432 implementation status above.
