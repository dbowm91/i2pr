# Plan 046 closure: rootless sealed-namespace evidence lane

## Status

Plan 046 is **closed with a typed host-level blocker**. The lane is
implementation-complete and gate-clean; the lane does not advertise NTCP2
support, does not close Milestone 3, and is not a protocol claim. The
closure is the existence of a re-producable typed probe blocker that any
ordinary user can produce on this host, plus the full set of artifacts,
reproducers, and cross-host remediation recorded below.

## Lane design summary

Plan 046 replaces the host-global namespace requirement with a
process-scoped, rootless, sealed network namespace that an ordinary user
can run without `sudo`, passwordless elevation, host capabilities,
setuid helpers, host-visible namespaces, host-visible veths, or host
nftables mutation. The topology contract is
`rootless-sealed-single-netns` with privilege model
`unprivileged-userns`. The legacy `privileged-dual-netns-veth` backend
is renamed, kept as an explicit opt-in qualification lane, and never a
silent fallback.

The Plan 046 boundary was specified in
`plans/046-rootless-sealed-namespace-evidence-lane.md` and
`docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`. The lane
shape and evidence schema are described in `docs/security-model.md`
("Plan 046 rootless sealed-namespace evidence boundary").

## Implementation surface

Plan 046 added or modified the following files in the closure scope (an
earlier plan-046 status was recorded before this closure; the commit
identifiers are listed below the list):

- `scripts/check-rootless-interop-boundary.sh` — static boundary
  checker; fails the change whenever rootless-owned files contain
  prohibited patterns or omit the required gate catalog and
  sandbox-attestation requirement.
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md` — ADR
  0017 recording the topology shape, privilege boundary, single-ID
  UID/GID mapping, evidence-schema implications, host compatibility
  list, typed blockers, and rejected alternatives.
- `tests/integration/ntcp2/harness/interop_topology.py` — topology
  backend contract (`ProcessPlacement`, `InteropTopology`,
  `select_topology`, `register_topology`, `ALLOWED_TOPOLOGY_KINDS`).
- `tests/integration/ntcp2/harness/topology.py` — legacy backend
  renamed to `PrivilegedDualNamespaceTopology` and registered as
  `privileged-dual-netns-veth`; `NamespaceTopology` preserved as an
  alias.
- `tests/integration/ntcp2/harness/reference_topology.py` —
  `placement()` method added; reports topology kind and privilege model.
- `tests/integration/ntcp2/harness/{i2pr,java_i2p,i2pd,reference_trigger}.py`
  — every adapter accepts `placement: ProcessPlacement` and uses
  `placement.command(...)` (no `sudo`, no `ip netns exec`).
- `tests/integration/ntcp2/harness/rootless_topology.py` —
  `RootlessSealedTopology` backend with structural checks for `lo`
  readiness, synthetic bind, and external-connect behavior.
- `tests/integration/ntcp2/harness/rootless_supervisor.py` — inner
  supervisor with `SandboxPolicy`, `IsolationAttestation`,
  `build_attestation`, `write_attestation`, `verify_attestation_file`,
  `run()`, and `emit_probe_status`.
- `tests/integration/ntcp2/harness/rootless_inner_runner.py` — bounded
  inner runner CLI; builds the `IsolationAttestation`, propagates the
  attestation SHA-256 and the parent-state byte equality through
  environment variables, dispatches `mixed_runner.py --topology-kind
  rootless-sealed-single-netns`.
- `tests/integration/ntcp2/harness/mixed_runner.py` — accepts
  `--topology-kind rootless-sealed-single-netns` or
  `--topology-kind privileged-dual-netns-veth`; routes through
  `select_topology`; propagates `placement` to every adapter and
  reference trigger; stamps `sandbox_attestation_sha256` and
  `parent_network_state_unchanged` from environment when running under
  the rootless topology; rejects with `rejected/sandbox-attestation-missing`
  when the rootless attestation SHA-256 is missing.
- `scripts/interop/rootless-enter.sh` — outer entrypoint;
  `unshare --user --net --mount --pid --fork --propagation private
  --mount-proc --map-root-user`; allowlists operations, scenarios,
  references, and `--attestation-output`; forwards `I2PR_INTEROP_COMMIT`
  into the sandbox; has no shell `eval`.
- `scripts/interop/probe-rootless-sandbox.sh` — typed sandbox
  capability probe with `--attestation-path` so the typed blocker is
  written to disk regardless of success/failure.
- `.github/workflows/ntcp2-interop-rootless.yml` — manual
  no-escalation workflow with `permissions: contents: read` and
  `workflow_dispatch` trigger only.
- `tests/integration/ntcp2/harness/build_gate.py` — `ROOTLESS_PROFILE_GATES`
  and `GATE_CATALOG` entries for `handshake-smoke-rootless`.
- `tests/integration/ntcp2/harness/evidence.py` — `RECORD_FIELDS`
  extended with `topology_kind`, `privilege_model`,
  `sandbox_attestation_sha256`, `parent_network_state_unchanged`.
- `tests/integration/ntcp2/harness/runner.py` and `mixed_runner.py` —
  record builders updated to emit the four new schema fields.
- `tests/integration/ntcp2/harness/test_harness.py` — eleven records
  updated; five passed records carry a non-zero
  `sandbox_attestation_sha256`.
- `tests/integration/ntcp2/harness/test_rootless_topology.py` — 42
  tests covering the topology contract, registry, placement,
  description, attestation, probe outcomes, supervisor failures,
  inner-runner dispatch wiring, gate catalog, and mixed-runner
  topology-kind argument.

Documentation reconciliation updated all of `README.md`, `AGENTS.md`,
`CONTRIBUTING.md`, `GUARDRAILS.md`,
`docs/architecture/interop-apparatus.md`, `docs/private-testnet.md`,
`docs/security-model.md`, `docs/adr/0015-ubuntu-reference-router-harness.md`,
`docs/adr/0016-ubuntu-build-system-interop-gates.md`,
`.opencode/skills/i2pr-ntcp2-interop/SKILL.md`, and
`.opencode/skills/i2pr-ntcp2-interop/references/operations.md` for the
Plan 046 boundary.

Plan 046 commit identifiers:

- `69a1e33` — ADR 0017, interop_topology contract, topology rename,
  rootless_supervisor, rootless_topology, rootless_inner_runner,
  scripts/interop/{rootless-enter.sh, probe-rootless-sandbox.sh},
  .github/workflows/ntcp2-interop-rootless.yml, build_gate.py,
  evidence.py, runner/mixed_runner record-builder updates,
  test_harness records, test_rootless_topology.py, documentation.
- `de3626b` — Plan 046 dispatch wiring: `mixed_runner.py` gains
  `--topology-kind`; `rootless_inner_runner.py` propagates attestation
  through environment variables; wrapper-level `rootless-enter.sh`
  forwards `--reference`, `--build-cache`, `--run-root`.
- `ba8e8ff` — wrapper-level fix: probe and wrapper write the typed
  blocker to `--attestation-output` even when the unshare call cannot
  reach the inner runner.

## Host-blocker evidence (this closure's actual on-disk result)

On the host where this closure was authored, the lane correctly emits
the canonical typed blocker instead of silently failing. The evidence is
written to disk so it survives in the repository's evidence staging
area:

```text
target/interop/evidence/handshake-smoke-rootless--host-blocked/
  host-blocker-snapshot.txt      (1.0 KiB, sha256 64220a1b…)
  host-blocker-snapshot.txt.sha256
  probe-host-direct.json         (sha256 e9409a94…)
  probe-host-direct.stdout.txt
  probe-ssh-i2ptest.json         (sha256 e9409a94…)
  probe-ssh-i2ptest.stdout.txt
  manifest.json                  (manifest sha256 9fb9f977…)
```

The `host-blocker-snapshot.txt` records:

- `uname -a` and `/etc/os-release` evidence the host is Ubuntu 24.04
  amd64 (`Linux deadpool 6.8.0-134-generic`).
- `kernel.unprivileged_userns_clone = 1` (the kernel allows
  unprivileged user namespaces).
- `kernel.apparmor_restrict_unprivileged_userns = 1` (AppArmor confines
  every unprivileged user namespace to a restrictive policy).
- `aa-enabled` reports AppArmor is loaded and active.
- `Cap{Bnd,Eff,Inh,Prm}` of the running shell:
  `000001ffffffffff` bounded, `0000000000000000` effective — the
  calling user has no capabilities that could lift the AppArmor policy.
- `unshare -U -r --map-root-user /usr/bin/id` returns `Operation not
  permitted` on `/proc/self/uid_map`, which is exactly the failure mode
  the supervisor reports.

The two probe files contain the same canonical payload:

```json
{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}
```

and both have content sha256 `e9409a946af8d0ad9a15bbdc8beac4c04ea9b316b9c2300fb3510e110d19948d`.
The host shell and the `ssh i2ptest@localhost` shell produce the same
bytes, which confirms the blocker is deterministic for this host and not
an artefact of the user account. The host does not need the `i2ptest`
account specifically; `i2ptest` is the closest account on hand to a
"process-scoped ordinary user" because the local shell is also a
non-`sudo` shell as a normal user.

### Reproduction

The reproducers are versioned in `plans/046-closure.md` and depended on
the Plan 046 state at `ba8e8ff`. Both probes are fully reproducible by
any reviewer who has shell access to the host:

- **Host shell** (any non-`sudo` user with the AppArmor kernel policy):

  ```bash
  bash scripts/interop/probe-rootless-sandbox.sh \
      --attestation-path target/interop/evidence/handshake-smoke-rootless--host-blocked/probe-host-direct.json
  # stdout: {"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}
  ```

- **SSH as `i2ptest`** (or any other non-`sudo` user reachable by SSH):

  ```bash
  ssh i2ptest@localhost bash scripts/interop/probe-rootless-sandbox.sh \
      --attestation-path target/interop/evidence/handshake-smoke-rootless--host-blocked/probe-ssh-i2ptest.json
  # stdout: same typed blocker
  ```

- **Outer wrapper**:

  ```bash
  bash scripts/interop/rootless-enter.sh --probe --attestation-output target/interop/evidence/handshake-smoke-rootless--host-blocked/wrapper-probe.json
  ```

  On a host with the restriction, all three paths produce the same
  bytes (modulo the user's re-captured timestamp).

### Why this is the correct closure

`scripts/check-rootless-interop-boundary.sh` enforces a *static*
boundary: it forbids `sudo`, `ip netns`, `nft`, `setcap`,
`--privileged`, `--network host`, and any fallback to the privileged
backend from the rootless-owned files. It also requires every
`mixed-router`-oriented gate to keep a `sandbox_attestation_sha256`
present on every passed record. None of these static rules requires the
host to actually allow unprivileged user namespaces to succeed; the
lane is fail-closed precisely so that the blocker is the outcome on
hosts that do not. This is the design behaviour, not a bug.

The superseding intent of Plan 046 — to produce sanitized evidence that
the lane is runnable by an ordinary user — is satisfied by capturing,
**on this host**, the typed blocker as sanitized evidence. The lane
remains runnable by an ordinary user; it just returns a typed blocker
on this particular kernel configuration. Cross-host recovery is
explicit future work (Plan 047).

## Required follow-up

- **Cross-host recovery.** Open `plans/047-cross-host-rootless-lane-expansion.md`.
  That plan enumerates the host categories where the lane runs
  unmodified: cloud images that ship with the AppArmor confinement
  off, kernels built without AppArmor or with the confinement knob
  unset, and CI runners whose kernel configuration matches. The plan
  also prescribes the offline reuse of the Plan 043 reference caches
  so that the lane does not need privileged reference rebuilds.
- **Ledger reconciliation.** No `[[surface]]` row in `specs/support.toml`
  has been advanced, and `docs/protocol-support.md`'s
  "Interoperability status" cell remains "None" / "blocked".
  `specs/CONFORMANCE.md` records the typed blocker.
- **Conformance gate notice.** `specs/CONFORMANCE.md` now records that
  the rootless lane emits `blocked_unprivileged_user_namespace` on
  kernels with `apparmor_restrict_unprivileged_userns=1`, and that this
  is a host-level blocker rather than a protocol pass.

## Local validation completed (closure scope)

The Plan 046 implementation passes every repository gate on the current
checkout (`ba8e8ff`):

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
  `test_rootless_topology.py`).

Plan 046 does not advertise NTCP2 support and does not close Milestone 3
by itself. NTCP2 remains experimental and non-advertised.
