# Plan 044 implementation and execution record

## Status

Plan 044 is implementation-complete locally: every deterministic corrective
defect, the mixed-router composition, the strict launcher renderer, the
non-echo data-phase oracle, the gate-staging archival design, and the
documentation reconciliation are wired and tested. Privileged external
execution is not closed on this host: the Ubuntu 24.04 amd64 host contract
rejects the current environment before namespace creation or reference-cache
use. No authenticated mixed-router result is claimed and no sanitized mixed
evidence record is committed.

The Plan 044 typed blocker is replaced: `handshake-smoke` no longer returns
`i2pr-mixed-router-profile-not-wired` for the four allowlisted directional
scenarios. That blocker is now reserved for scenario IDs that are not
allowlisted for the active gate. On the current non-Ubuntu host, every
mixed-router execution still terminates at `blocked_host_contract` from
`scripts/interop/ubuntu/check-host.sh --post-install`.

## Corrective defects closed

- Java RouterInfo export now locates `router.info` only inside the pinned
  writable router directory (`reference-data`); symlinks, empty files,
  oversized files, and candidates outside the run root are rejected.
- Schema-1 evidence sanitation uses field-aware allowlists for `expected`,
  `actual_typed_result`, `known_deviation`, and `reproduction`. The fixed
  taxonomy values used by the manifest are accepted while secret/payload/path
  material, free-form strings, and the `record-at-execution` placeholder
  remain rejected.
- `run-gate.sh` now moves evidence through a per-gate staging directory,
  validates each scenario ID against the active gate, archives only staged
  records with the gate prefix, refuses filename collisions, snapshots
  pre-existing record digests and rejects modification or deletion, and always
  cleans the staging directory.
- The GitHub Actions workflow installs `rustfmt` and `clippy` components,
  uses `--locked` on every required `cargo` invocation, and records
  `rustc --version --verbose` and `cargo --version --verbose`. The
  `validate-build-contract.py` validator structurally verifies those
  requirements and rejects unbounded workflow inputs.

## Mixed-run composition

- The four Plan 044 directional scenarios live under
  `tests/integration/ntcp2/mixed-scenarios/` with their own manifest:
  `i2pr-to-java-ipv4`, `java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`,
  `i2pd-to-i2pr-ipv4`. Each carries declared initiator and responder, role,
  address family, padding profile, expected result class, and the launcher
  scenario schema inputs.
- `tests/integration/ntcp2/harness/launcher_renderer.py` renders a confined
  `run-root/scenario.toml` that the strict Rust launcher parser accepts, and
  rejects absolute paths, parent traversal, synthetic-range violations,
  mismatched address families, missing or extra peer data, unsupported
  network IDs, and unknown fields.
- `tests/integration/ntcp2/harness/mixed_runner.py` composes
  `NamespaceTopology`, `I2prAdapter`, the chosen reference adapter, and the
  typed evidence boundary. It enforces the four-step responder-first and
  initiator-first RouterInfo lifecycles, requires the listener-ready status
  before any reference initiator starts, validates RouterInfo structure and
  signature through the strict Python and Rust parsers, and only writes the
  sanitized mixed-router record outside the disposable exchange and run
  roots.
- `tests/integration/ntcp2/harness/data_oracle.py` defines the non-echo
  data-phase oracle: per-reference send-only and receive-only hooks via
  implementation-specific test surfaces (Java SAM v3, i2pd HTTP JSON-RPC)
  and a mixed split send/receive oracle. The prior echo assumption is
  explicitly rejected and recorded as `data_phase_oracle=` sentinel in the
  evidence's deterministic_parameters.
- `tests/integration/ntcp2/harness/reference_trigger.py` provides the
  reference-only control trigger used when automatic reference dialing is
  non-deterministic. Both the Java SAM v3 STREAM `SessionCreate` and i2pd
  HTTP `/jsonrpc` `ConnectPeer` triggers operate only within the disposable
  namespace.
- `scripts/interop/run-matrix.sh` routes the four directional scenarios
  through `mixed_runner.py`; the full profile also runs the original
  environment-smoke matrix through `runner.py` before invoking
  `mixed_runner.py`.
- `tests/integration/ntcp2/harness/build_gate.py` updates the handshake-smoke
  gate scenarios to the four directional scenarios and extends the full
  profile to require both the original matrix and the four mixed-router
  directions.

## Documentation reconciliation

- `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `GUARDRAILS.md`, the NTCP2
  interoperability skill and its `references/operations.md`,
  `docs/architecture/interop-apparatus.md`,
  `docs/architecture/i2pr-runtime.md`, `docs/private-testnet.md`,
  `docs/protocol-support.md`, `docs/security-model.md`, `specs/CONFORMANCE.md`,
  `tests/integration/ntcp2/README.md`, and the evidence README all describe
  the Plan 044 status accurately: runtime-owned NTCP2 wire adapter
  implemented and locally validated; mixed-router harness composition and
  authorized evidence pending; NTCP2 remains experimental and
  non-advertised. Milestone 3 remains open; no claim of proven
  interoperability.

## Local validation

The following checks passed on 2026-07-15:

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace              # all 27 test groups pass
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'   # 104 tests
python3 scripts/interop/validate-scenarios.py
python3 scripts/interop/validate-build-contract.py
python3 scripts/interop/validate-evidence.py
bash -n scripts/check-ntcp2-interoperability.sh scripts/interop/*.sh scripts/interop/lib/*.sh scripts/interop/ubuntu/*.sh
git diff --check
```

The host contract gate returns the typed `blocked_host_contract` for any
privileged execution here because the local host is not Ubuntu 24.04 amd64
and non-interactive sudo is unavailable. No privileged namespace creation,
reference build, or public-network attempt was made.

## Required external closure

On an authorized disposable Ubuntu 24.04 amd64 host, the Plan 044 lane
executes the Plan 043 gate order with the expanded directional scenarios:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo -E bash scripts/interop/reset-lane-state.sh
sudo -E bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
sudo -E bash scripts/interop/verify-clean-host.sh --record-baseline
bash scripts/interop/build-references.sh --force-rebuild
python3 scripts/interop/cache-manifest.py --verify
sudo -E bash scripts/interop/offline-reuse.sh
sudo -E bash scripts/interop/run-gate.sh --profile environment-smoke --offline
sudo -E bash scripts/interop/run-gate.sh --profile reference-crosscheck-ipv4 --offline
sudo -E bash scripts/interop/run-gate.sh --profile handshake-smoke --offline
python3 scripts/interop/validate-evidence.py
python3 scripts/interop/aggregate-evidence.py --profile handshake-smoke
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify
```

Stop on any reference-control failure before introducing i2pr as the
variable under test. Once `handshake-smoke` passes with all four direction
records and the typed non-echo oracle, run the `full` profile and the
fresh-checkout, offline-cache, and post-reboot repetitions before any
separate Milestone 3 evidence review. Plan 044 does not close Milestone 3
itself; that closure depends on a separate evidence review that compares
the retained sanitized records directly against `plans/000-mvp-roadmap.md`
and `specs/CONFORMANCE.md`.