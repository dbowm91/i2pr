# Plan 040 corrective apparatus closure record

- Date: 2026-07-15
- Scope: executable apparatus corrections only
- Host: local execution intentionally blocked; this host is not Ubuntu 24.04 amd64
- Interoperability claim: none

## Implemented

- Resolved and locked the exact Java I2P object
  `2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd object
  `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Standardized the machine identifiers to `java_i2p` and `i2pd`.
- Added strict schema-2 cache metadata parsing, full-tree re-hashing,
  current-cache summary lookup, cache-key contract checks, read-only cache
  finalization, and offline source/dependency failure paths.
- Strengthened Ubuntu pre/post checks, JSON host metadata, namespace/nftables
  probes, short collision-resistant veth names, exact topology verification,
  destination-port firewall rules, and the disposable canary self-test.
- Corrected Java and i2pd address rendering, data/configuration confinement,
  network-ID settings, launcher/version probes, and RouterInfo NetDB filename
  derivation.
- Added atomic PID ownership, namespace-PID emergency cleanup, sanitized
  evidence finalization under `target/interop/evidence/`, and complete run-root
  deletion. Cleanup failure overrides a protocol result.
- Updated README, AGENTS.md, architecture/ADR documentation, configuration
  provenance, and the NTCP2 interoperability skill guidance.

## Local validation

All of the following passed from the repository root:

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
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'  # 9 passed
bash -n scripts/check-ntcp2-interoperability.sh scripts/interop/*.sh scripts/interop/lib/*.sh scripts/interop/ubuntu/*.sh
git diff --check
```

The negative-path runner check emitted `blocked_host_contract` and left no run
root. `scripts/interop/cleanup.sh` then reported zero started, terminated,
forced, residual namespace, interface, or failure counters. The offline Java
builder also failed before source access when its required cached source was
absent.

## External host gate

The following commands remain mandatory on an authorized disposable Ubuntu
24.04 amd64 host and were not run here because the host checker correctly
rejects the current host before modification:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh --force-rebuild
bash scripts/interop/build-references.sh --offline
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
python3 scripts/interop/validate-evidence.py
sudo -E bash scripts/interop/cleanup.sh
```

This record closes the implementation pass without closing Plan 038, Plan
041, or Milestone 3. It does not promote support rows or advertise NTCP2.
