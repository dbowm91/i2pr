# Plan 046 status: rootless sealed-namespace evidence lane

## Status

Plan 046 implementation is complete and the lane is closed with a typed
host-blocker. The closure record is `plans/046-closure.md`. This file
remains the short implementation-and-status note.

The current host (Ubuntu 24.04 amd64 with
`kernel.apparmor_restrict_unprivileged_userns=1`) cannot establish an
unprivileged user namespace from an ordinary user account, so
`unshare -U -r --map-root-user` returns `Operation not permitted` on
`/proc/self/uid_map`. Both the host shell and an `ssh i2ptest@localhost`
shell produce the canonical typed blocker
`blocked_unprivileged_user_namespace`. The probe and the wrapper emit this
result to the attestation path on disk, so the result is captured in
sanitized evidence rather than being raised as a non-zero exit alone.

The lane is runnable by an ordinary user on hosts that permit
unprivileged user namespaces (kernels with the AppArmor restriction off
or no AppArmor driver). Cross-host portability is deferred to
`plans/047-cross-host-rootless-lane-expansion.md`.

## Implementation surface (unchanged since commit `ba8e8ff`)

The Plan 046 implementation surface is present and passes every
repository gate. The complete file list is enumerated in the closure
record; this file retains only the policy-relevant pieces.

- Plan 046 topology backend contract:
  `tests/integration/ntcp2/harness/interop_topology.py` adds
  `ProcessPlacement`, `InteropTopology`, `select_topology`,
  `register_topology`, and `ALLOWED_TOPOLOGY_KINDS`.
- Plan 046 rootless backend:
  `tests/integration/ntcp2/harness/rootless_topology.py` plus the renamed
  `PrivilegedDualNamespaceTopology` in
  `tests/integration/ntcp2/harness/topology.py`.
- Plan 046 inner supervisor:
  `tests/integration/ntcp2/harness/rootless_supervisor.py` writes a
  sanitized `IsolationAttestation`.
- Plan 046 inner runner dispatch:
  `tests/integration/ntcp2/harness/rootless_inner_runner.py` invokes
  `mixed_runner.py --topology-kind rootless-sealed-single-netns` and
  propagates `I2PR_INTEROP_ROOTLESS_ATTESTATION_SHA256` and
  `I2PR_INTEROP_ROOTLESS_PARENT_STATE_UNCHANGED`.
- Plan 046 mixed-runner wire-up:
  `tests/integration/ntcp2/harness/mixed_runner.py` accepts
  `--topology-kind`, routes through `select_topology`, propagates
  `placement` to every adapter and reference trigger, and stamps the
  record with the attestation SHA-256 and parent-network state digest.
- Plan 046 outer entrypoint: `scripts/interop/rootless-enter.sh` and
  `scripts/interop/probe-rootless-sandbox.sh` both honour
  `--attestation-output` (`--attestation-path` on the probe) so that
  the typed blocker is written to disk regardless of which side of the
  success/failure boundary the run lands on.
- Plan 046 static boundary checker:
  `scripts/check-rootless-interop-boundary.sh` enforces the gate catalog
  and sandbox-attestation requirement without any privileged tool.
- Plan 046 ADR: `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`.
- Plan 046 manual no-escalation workflow:
  `.github/workflows/ntcp2-interop-rootless.yml`.

## Local validation completed

The Plan 046 implementation passes the repository gates on the current
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
  `test_rootless_topology.py`).

## Host-blocker evidence (closed as a typed blocker)

Both probe surfaces produce the canonical typed blocker on this host:

```json
{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}
```

Evidence files (sanitized, with content hashes):

- `target/interop/evidence/handshake-smoke-rootless--host-blocked/probe-host-direct.json`
- `target/interop/evidence/handshake-smoke-rootless--host-blocked/probe-ssh-i2ptest.json`
- `target/interop/evidence/handshake-smoke-rootless--host-blocked/host-blocker-snapshot.txt`
- `target/interop/evidence/handshake-smoke-rootless--host-blocked/manifest.json`
  (and content hashes for each retained file)

Both probe files have identical sha256 (`e9409a94…`), so this is a
deterministic host-level result, not an environment artefact.

Plan 046 does not advertise NTCP2 support and does not close Milestone 3
by itself. NTCP2 remains experimental and non-advertised.
