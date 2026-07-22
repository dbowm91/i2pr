---
name: i2pr-rootless-sandbox
description: Operate, diagnose, or extend the Plan 046 rootless, process-scoped, sealed-namespace sandbox lane for NTCP2 interoperability evidence on the host itself. Use when an agent is asked to run the rootless probe, enter the sandbox, dispatch a bounded scenario inside it, validate the typed blocker taxonomy, or update the static rootless boundary checker.
---

# I2PR Rootless Sandbox (Plan 046)

Use this skill from the repository root for the Plan 046 rootless sealed-namespace
lane. **Plan 046 is the host-side fallback.** On any host where
`kernel.apparmor_restrict_unprivileged_userns=1`, the probe emits the canonical
typed blocker `blocked_unprivileged_user_namespace`; recover on a permissive
host or use the `i2pr-multipass-recovery` skill, which wraps an ordinary Ubuntu
24.04 amd64 Multipass guest whose policy is permissive by design. Never
describe Plan 046 as an interoperability pass when only the host-blocker result
was obtained, and never substitute a self-loopback or testkit result for
evidence.

Read `AGENTS.md`, `plans/046-rootless-sealed-namespace-evidence-lane.md`,
`plans/046-closure.md`, `plans/047-cross-host-rootless-lane-expansion.md`, and
the relevant `docs/adr/` records before changing anything in this lane.

## Topology contract

The single allowed evidence topology is `rootless-sealed-single-netns` with
privilege model `unprivileged-userns`. The legacy `privileged-dual-netns-veth`
topology is reserved for explicit later qualification work and **is never the
default and never a silent fallback**. Code-level gates live in
`tests/integration/ntcp2/harness/interop_topology.py` (registry and
`select_topology`) and `tests/integration/ntcp2/harness/rootless_topology.py`
(the backend adapter).

A passed mixed-router evidence record requires **all** of:

- `topology_kind == "rootless-sealed-single-netns"`
- `privilege_model == "unprivileged-userns"`
- a non-zero `sandbox_attestation_sha256`
- `parent_network_state_unchanged == True`

Missing any one is a typed blocker, not a fallback or a skipped success.

## Authoritative command surface

Run from the repository root:

```text
bash scripts/interop/probe-rootless-sandbox.sh
bash scripts/interop/rootless-enter.sh --probe
bash scripts/interop/rootless-enter.sh \
    --scenario <id> --reference <ref> \
    --build-cache <path> --run-root <path>
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-ntcp2-interoperability.sh
```

### Script responsibilities (read these before changing them)

| Script | Responsibility |
|---|---|
| `scripts/interop/probe-rootless-sandbox.sh` | Typed capability probe. Emits sanitized JSON `{schema, type, outcome}`. Writes a typed blocker when the inner unprivileged user namespace cannot be entered. |
| `scripts/interop/rootless-enter.sh` | Outer no-escalation entrypoint. Creates the sandbox via `unshare --user --net --mount --pid --fork --propagation private --mount-proc --map-root-user`. Allowlists one operation at a time. Never uses `sudo`, `setcap`, `--privileged`, `--network host`, `ip netns`, `nft`, or any privileged fallback. |
| `tests/integration/ntcp2/harness/rootless_supervisor.py` | Inner supervisor. Verifies single-ID UID/GID maps, `no_new_privs`, distinct user/network/mount/PID namespaces, `lo` readiness, synthetic bind, absence of default/external routes, bounded external connect probe. Emits sanitized `IsolationAttestation` whose sha256 is bound to every passed mixed-router record. |
| `tests/integration/ntcp2/harness/rootless_inner_runner.py` | Inner-side process that runs the bounded scenario actions through the sandboxed adapter. |
| `scripts/check-rootless-interop-boundary.sh` | Static checker. Fails the change when rootless-owned files contain prohibited patterns (`sudo`, `ip netns`, `nft`, `setcap`, `--privileged`, `--network host`, fallback to privileged backend), when the gate catalog omits `handshake-smoke-rootless`, or when evidence validation does not require the sandbox attestation. |
| `tests/integration/ntcp2/harness/test_rootless_topology.py` | Unit tests for the topology contract and the supervisor's structural checks (no `multipass`, no host networking). |

## Typed blocker catalogue

The probe, sandbox verifier, and topology backend emit a stable set of typed
outcomes. Treat each as a hard stop; never substitute it for a fallback or a
pass:

| Typed outcome | Meaning |
|---|---|
| `rootless_sandbox_available` | Sandbox entered cleanly. All structural checks passed. Proceed to a bounded scenario. |
| `blocked_unprivileged_user_namespace` | `kernel.apparmor_restrict_unprivileged_userns=1` confines the unprivileged user namespace. The ordinary user has no lever to lift it. Use `i2pr-multipass-recovery` for this host or pick a host where the sysctl is `0`. |
| `blocked_loopback_unconfigured` | `lo` is not usable inside the namespace. The supervisor cannot verify loopback readiness. |
| `blocked_synthetic_bind_failed` | A synthetic IP could not be bound on `lo`. Network plumbing is wrong; do not retry blindly. |
| `blocked_external_connect_succeeded` | An external connect succeeded from inside the namespace. The sandbox does not actually seal; investigate host routes before any router. |
| `blocked_no_new_privs_missing` | `PR_SET_NO_NEW_PRIVS` could not be applied. Sandbox is not fully unprivileged. |
| `blocked_namespace_id_not_single` | UID/GID maps are not single-ID; sandbox surface is too broad. |
| `blocked_namespace_distinct_kind_missing` | User / network / mount / PID namespaces are not all distinct. |

Plan 046 also rejects `i2pr-mixed-router-profile-not-wired` when the active
scenario ID is not allowlisted for `handshake-smoke-rootless`. A missing
`topology_kind`, `privilege_model`, `sandbox_attestation_sha256`, or
`parent_network_state_unchanged` field on a passed record is a hard
reject.

## Result interpretation

A `rootless_sandbox_available` from BOTH `probe-rootless-sandbox.sh` and
`rootless-enter.sh --probe` is required before any bounded scenario dispatch.
A typed blocker on either probe is a hard stop. After a bounded direction run:

1. Confirm the evidence record carries the four required fields listed above.
2. Confirm `sandbox_attestation_sha256` matches the on-disk
   `target/interop/evidence/handshake-smoke-rootless--<date>/attestation.json`.
3. Run `bash scripts/check-ntcp2-interoperability.sh`.
4. Run `bash scripts/check-rootless-interop-boundary.sh`.
5. Run `bash scripts/interop/cleanup.sh` and `sudo -n bash scripts/interop/verify-clean-host.sh --verify --baseline target/interop/build/clean-host-baseline.json`.

A `blocked_host_contract` from Plan 040 is informational here; it does not
stop Plan 046 from running, but Plan 046's own probe result is what's
authoritative for evidence.

## Safety and development rules

- Never edit the kernel, AppArmor, sysctl, or namespace policy on the host to
  make Plan 046 pass. Plan 046 forbids `sudo`, `setcap`, `--privileged`,
  `--network host`, and any privileged fallback. Plan 047 documents cross-host
  recovery.
- Never reuse a self-handshake, loopback run, vector, or testkit result as
  evidence. The sandbox must be entered; the attestation must be on disk.
- Keep the static boundary checker green. If a new operation is required,
  update `scripts/check-rootless-interop-boundary.sh` to allowlist its pattern
  with a comment, not by weakening the gate.
- Add negative-path tests for any new topology, supervisor, or attestation
  behavior.
- Before handoff: `cargo fmt --all --check`, `cargo check --workspace --all-targets`,
  `cargo test --workspace`, `bash scripts/check-dependency-direction.sh`,
  `bash scripts/check-runtime-boundaries.sh`,
  `bash scripts/check-rootless-interop-boundary.sh`,
  `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_rootless_topology.py'`.
- For the recovery lane on a permissive host or a permissive Multipass guest,
  hand off to `i2pr-multipass-recovery`. For the canonical harness workflow,
  hand off to `i2pr-ntcp2-interop`.
