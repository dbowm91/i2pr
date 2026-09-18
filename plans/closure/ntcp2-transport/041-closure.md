# Plan 041 implementation and execution record

## Status

The Plan 041 harness implementation is complete locally. Privileged external
execution is not closed on this host: the Ubuntu 24.04 amd64 host contract
rejects the current environment before namespace creation or reference-cache
use. No authenticated reference result is claimed and no sanitized evidence
record is committed.

## Implemented

- Added a strict, separate reference-pair scenario schema and two directional
  Java I2P/i2pd scenarios.
- Added a dedicated two-reference namespace owner with synthetic IPv4
  addressing, one-way initiation policy, exact nftables rules, route checks,
  forwarding checks, and residual-state verification.
- Added rendered Java and i2pd configuration assertions for the shared,
  explicitly non-public network ID 99 and disabled services.
- Added staged RouterInfo exchange with run-root confinement, bounded Python
  structure checks, and the Rust `i2pr-interop ntcp2 inspect` signature/parser
  check.
- Added dual authenticated-state observations, typed counters, schema-2
  sanitized evidence, cleanup precedence, and the dedicated matrix runner.
- Updated the repository, architecture, operations, configuration, support,
  conformance, and interop-skill documentation to keep Plan 041 distinct from
  the eight-scenario Plan 038 manifest and from i2pr interoperability claims.

## Local validation

The following checks passed on 2026-07-15:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
python3 scripts/interop/validate-scenarios.py
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/interop/validate-evidence.py
bash -n scripts/interop/*.sh scripts/interop/lib/*.sh scripts/interop/ubuntu/*.sh
```

The reference-crosscheck matrix was also invoked. It returned the typed
`blocked_host_contract` result for both directional scenarios and performed no
privileged setup. The evidence validator reports that no sanitized mixed-router
records are committed, which is expected while the host prerequisite remains
unmet.

## Required external closure

On an authorized Ubuntu 24.04 amd64 host, rerun the Plan 040 preparation and
Plan 041 sequence from the plan, including offline cache reuse, environment
smoke, both directional reference crosschecks, evidence validation, cleanup,
and the fresh-run/reboot repetitions. Only after those runs produce real hashes,
dual authenticated observations, and zero residual state may this record be
replaced with a successful Plan 041 closure and the result considered a control
for Plan 042.
