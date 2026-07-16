# Plan 046 status: rootless sealed-namespace evidence lane

## Status

Plan 046 remains open. This is a status record, not a closure record: the
current checkout contains the complete implementation surface for the
rootless sealed-namespace lane. No authorized i2pr-to-reference execution
has been run on a host that permits unprivileged user namespaces, so the
lane is not yet externally proven. Milestone 3 remains open.

## Implementation completion

The Plan 046 implementation surface is present and passes the repository
gates. The added and changed files are:

- `scripts/check-rootless-interop-boundary.sh` — static boundary checker
  that fails the change whenever rootless-owned files contain prohibited
  patterns (sudo, ip netns, nft, setcap, --privileged, --network host,
  fallback to the privileged backend) or omit the required gate catalog
  entries and sandbox attestation requirement.
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md` — ADR
  0017 recording the topology shape, privilege boundary, single-ID
  UID/GID mapping requirement, evidence-schema implications, host
  compatibility list, typed blockers, and rejected alternatives.
- `tests/integration/ntcp2/harness/interop_topology.py` — topology
  backend contract (`ProcessPlacement`, `InteropTopology`,
  `select_topology`, `register_topology`, `ALLOWED_TOPOLOGY_KINDS`).
- `tests/integration/ntcp2/harness/topology.py` — the legacy dual-namespace
  backend was renamed to `PrivilegedDualNamespaceTopology` and registered
  as `privileged-dual-netns-veth`; `NamespaceTopology` is preserved as an
  alias.
- `tests/integration/ntcp2/harness/reference_topology.py` — the reference
  pair topology owner now exposes `placement()` and reports its topology
  kind and privilege model.
- `tests/integration/ntcp2/harness/i2pr.py`,
  `tests/integration/ntcp2/harness/java_i2p.py`,
  `tests/integration/ntcp2/harness/i2pd.py`,
  `tests/integration/ntcp2/harness/reference_trigger.py` — every
  reference adapter accepts an optional `placement: ProcessPlacement`
  parameter and uses `placement.command(...)` instead of constructing
  `sudo` or `ip netns exec` prefixes.
- `tests/integration/ntcp2/harness/rootless_topology.py` —
  `RootlessSealedTopology` backend (`topology_kind =
  "rootless-sealed-single-netns"`, `privilege_model =
  "unprivileged-userns"`) with structural checks for `lo` readiness,
  synthetic bind, and external connect behavior. Adapter `placement()`
  returns an empty prefix and requires `I2PR_INTEROP_ROOTLESS_INNER=1`
  on the inner process.
- `tests/integration/ntcp2/harness/rootless_supervisor.py` — inner
  supervisor that verifies the single-ID UID/GID mapping, `no_new_privs`,
  distinct user/network/mount/PID namespaces, `lo` readiness, exact
  synthetic bind and connect behavior, the absence of default or external
  routes, and a bounded external connect probe. Writes a sanitized
  `IsolationAttestation` whose sha256 is bound to every passed mixed-
  router evidence record and whose parent-network state pre/post digests
  must be byte-equal for a passed run.
- `tests/integration/ntcp2/harness/rootless_inner_runner.py` — bounded
  inner runner CLI. Plan 046 dispatch: builds the `IsolationAttestation`,
  propagates `I2PR_INTEROP_ROOTLESS_ATTESTATION_SHA256` and
  `I2PR_INTEROP_ROOTLESS_PARENT_STATE_UNCHANGED` through the environment,
  and invokes `mixed_runner.py --topology-kind rootless-sealed-single-netns`
  so the resulting evidence record is bound to the sandbox.
- `tests/integration/ntcp2/harness/mixed_runner.py` — extended with
  `--topology-kind` (`rootless-sealed-single-netns` or
  `privileged-dual-netns-veth`); routes through `select_topology` and
  propagates `placement` to every adapter and reference trigger; populates
  `sandbox_attestation_sha256` and `parent_network_state_unchanged` from
  the environment when running under the rootless topology.
- `scripts/interop/rootless-enter.sh` — outer entrypoint; uses
  `unshare --user --net --mount --pid --fork --propagation private
  --mount-proc --map-root-user`; allowlists operations, scenarios, and
  references; forwards `I2PR_INTEROP_COMMIT` into the sandbox; has no
  shell `eval`.
- `scripts/interop/probe-rootless-sandbox.sh` — typed sandbox capability
  probe.
- `.github/workflows/ntcp2-interop-rootless.yml` — manual no-escalation
  workflow with `permissions: contents: read` and `workflow_dispatch`
  trigger only.
- `tests/integration/ntcp2/harness/build_gate.py` — extended with the
  `ROOTLESS_PROFILE_GATES` and `GATE_CATALOG` entries for
  `handshake-smoke-rootless` and the explicit `privileged-dual-netns-veth`
  qualification gate.
- `tests/integration/ntcp2/harness/evidence.py` — `RECORD_FIELDS`
  extended with `topology_kind`, `privilege_model`,
  `sandbox_attestation_sha256`, and `parent_network_state_unchanged`;
  rootless-only validation rules added.
- `tests/integration/ntcp2/harness/runner.py` and `mixed_runner.py` —
  record builders updated to emit the four new schema fields.
- `tests/integration/ntcp2/harness/test_harness.py` — eleven test
  records updated to include the new fields; five passed records carry
  a non-zero `sandbox_attestation_sha256`.
- `tests/integration/ntcp2/harness/test_rootless_topology.py` — forty new
  tests covering the topology contract, registry, placement, description,
  attestation, probe outcomes, supervisor failures, and the gate catalog.

The Plan 046 boundary was reconciled in `AGENTS.md`, `README.md`,
`GUARDRAILS.md`, `CONTRIBUTING.md`, `docs/architecture/interop-apparatus.md`,
`docs/private-testnet.md`, `docs/security-model.md`,
`docs/adr/0015-ubuntu-reference-router-harness.md`,
`docs/adr/0016-ubuntu-build-system-interop-gates.md`,
`.opencode/skills/i2pr-ntcp2-interop/SKILL.md`, and
`.opencode/skills/i2pr-ntcp2-interop/references/operations.md`.

## Local validation completed

The Plan 046 implementation passes the repository gates. On the current
checkout:

- `cargo fmt --all --check` passes.
- `cargo check --workspace --all-targets` passes.
- `cargo test --workspace` passes (219 Rust tests).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  passes.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` passes.
- `bash scripts/check-dependency-direction.sh` passes.
- `bash scripts/check-runtime-boundaries.sh` passes.
- `bash scripts/check-fixture-manifest.sh` passes.
- `bash scripts/check-ntcp2-vectors.sh` passes.
- `bash scripts/check-ntcp2-interoperability.sh` passes.
- `bash scripts/check-rootless-interop-boundary.sh` passes.
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p
  'test_*.py'` passes (146 tests, including 42 in
  `test_rootless_topology.py` covering the dispatch wiring).

## Host-blocked evidence completion

The sandbox capability probe correctly emits a typed blocker on this
host. `unshare --user --map-root-user` fails with `Operation not
permitted` on `/proc/self/uid_map`, which is the canonical
`blocked_unprivileged_user_namespace` outcome. The probe returns rc=1
with the following typed JSON:

```json
{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace","reason":"uid_map write denied"}
```

No protocol claim follows from this status record. The Plan 046 lane
is runnable by an ordinary user on Ubuntu 24.04 amd64 hosts that permit
unprivileged user namespaces; that environment is not the host where
this checkout was authored.

## Evidence-completion requirements

External evidence-completion requires:

1. `bash scripts/interop/probe-rootless-sandbox.sh` returns
   `rootless_sandbox_available` on the candidate host.
2. `bash scripts/interop/rootless-enter.sh --probe` succeeds with a
   matching `IsolationAttestation` written under the evidence staging
   area.
3. `bash scripts/interop/run-matrix.sh --profile handshake-smoke-rootless`
   runs the four Plan 044 directional scenarios through the
   `RootlessSealedTopology` and produces four passed mixed-router records
   that all reference the same gate attestation, with a non-zero
   `sandbox_attestation_sha256` and `parent_network_state_unchanged=True`.
4. `bash scripts/check-ntcp2-interoperability.sh` and
   `bash scripts/check-rootless-interop-boundary.sh` both pass on the
   resulting evidence.
5. Independent review against `specs/CONFORMANCE.md` confirms that the
   retained claim is narrower than the privileged topology (protocol
   compatibility, not separate-stack behavior).

Plan 046 does not advertise NTCP2 support and does not close Milestone 3
by itself. NTCP2 remains experimental and non-advertised.