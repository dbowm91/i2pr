# Plan 038 closure: Ubuntu reference-router harness foundation

## Disposition

Plan 038 is closed as an environment/harness foundation only. The checked-in
workflow, builders, namespace runner, adapters, sanitation boundary, and
non-production launcher seam are present and fail closed. Mixed-router i2pr
evidence remains blocked because the complete runtime-owned wire-level NTCP2
adapter and an authorized Ubuntu run are not available. Milestone 3 remains
open; NTCP2 support rows remain experimental and non-advertised.

## Commits and changed files

- `2620ff4` — `docs: document Plan 038 interoperability harness`; updated
  `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `GUARDRAILS.md`, the architecture
  narratives/tooling, private-testnet and security documents, conformance, and
  protocol-support documentation.
- `6aa0528` — `interop: add Ubuntu reference-router harness foundation`;
  added/updated the lock manifest, Ubuntu setup/check scripts, source-pinned
  builders, namespace/isolation helpers, Python harness/configuration/scenario
  files, evidence validators, CI workflow, ADR 0015, the launcher crate, and
  the NTCP2 preflight/evidence README.

The complete file list is authoritative in `git show --name-only` for those
commits. No generated source tree, binary, identity, key, RouterInfo, log,
packet capture, or raw result is committed.

## Host and preparation evidence

The target contract is Ubuntu 24.04, amd64/x86_64, Bash 4+, Python 3,
`iproute2`, `nftables`, `sudo`, and a UTF-8 locale. In this managed execution
environment the pre-install checker reported Ubuntu 24.04, x86_64, kernel
`6.8.0-134-generic`, Python 3.12.3, and UTF-8. The post-install checker could
not obtain namespace privilege because the container's `sudo` is subject to
`no new privileges`; no package installation or router process was attempted.

The declared package set and preparation commands are locked in
`tests/integration/ntcp2/references.lock.toml`. The Java I2P source is pinned
to `2800040`, i2pd to `f618e41`, and the IzPack 5.2.4 installer is pinned to
Maven Central SHA-256
`a3f2c85afea82e04ebca5ebb1b9b5c95ea770c4d35a7635de312370e14a44d43`.
No reference cache, artifact hash, or installed-tree hash was claimed because
the required host lane was unavailable here.

## Isolation, configuration, and lifecycle design

The implementation creates `i2pr-<run>` and `ref-<run>` namespaces, moves both
veth endpoints out of the host namespace, assigns only the synthetic peer
addresses, rejects IPv4/IPv6 default/public routes, and installs namespace
default-deny nftables rules. `verify-isolation.sh` runs before any reference
process. Cleanup stops/drains children, deletes namespaces, and removes the
secret-bearing run root; cleanup failure is typed as a scenario failure.

The Java template is traced to the pinned `installer/resources/router.testnet.config`,
`router/java` NTCP sources, and `apps/routerconsole` update settings. The i2pd
template is traced to the pinned `contrib/i2pd.conf` sample. Adapters assert
reseed/bootstrap suppression, fixed literal NTCP endpoints, no UPnP/NAT,
disabled SSU2, no transit/floodfill role, bounded services, and disposable
configuration before launch.

The `tools/i2pr-interop` binary depends on the runtime and protocol owners but
does not depend on or activate `i2pr-daemon`. Its `listen`/`dial` commands
validate the scenario boundary and emit `blocked_missing_driver` until the
complete authenticated handshake/data driver is implemented.

## Scenario and evidence results

- Environment smoke: `blocked_host_contract` in this environment; no router
  process was started.
- Java I2P/i2pd reference crosscheck: blocked; no prepared artifacts or safe
  one-peer RouterInfo import run was available.
- i2pr handshake/data smoke: blocked by the explicit missing-driver result.
- Full eight-scenario matrix: not started; adversarial profiles remain gated
  on positive handshake/data smoke.
- Offline repeatability: static/offline paths are implemented, but no cache
  existed and the host contract blocked execution before build use.
- Committed sanitized evidence: none. The validator explicitly reports that
  absence is not success.
- Manual workflow: `.github/workflows/ntcp2-interop-ubuntu.yml` added with
  `workflow_dispatch`, explicit `ubuntu-24.04`, bounded timeout, always-cleanup,
  and sanitized-only artifact upload. No run ID exists for this checkout.

## Validation

The following completed successfully from the repository root:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace                 # 205 passed
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
bash -n scripts/interop/lib/common.sh scripts/interop/lib/namespaces.sh scripts/interop/ubuntu/check-host.sh scripts/interop/ubuntu/setup-host.sh scripts/interop/verify-isolation.sh scripts/interop/build-java-i2p.sh scripts/interop/build-i2pd.sh scripts/interop/build-references.sh scripts/interop/run-scenario.sh scripts/interop/run-matrix.sh scripts/interop/cleanup.sh scripts/check-ntcp2-interoperability.sh
git diff --check
```

The opt-in nightly fuzz lane was not run. No public-network traffic, DNS,
namespace run, or reference-router process was used for these checks.

## Remaining stop conditions

Before this plan can provide mixed-router evidence, an authorized Ubuntu host
must prove namespace privilege, build both exact reference revisions, validate
the implementation-specific RouterInfo exchange paths, run environment smoke
and Java/i2pd crosscheck, then complete the runtime-owned i2pr NTCP2 adapter.
Only sanitized authenticated results in both directions against both
references may advance the Milestone 3 gate or any support advertisement.
