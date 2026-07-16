# Plan 047: cross-host rootless lane expansion

## Objective

Recover the rootless sealed-namespace mixed-router evidence path on a
host where it can actually run as an ordinary user, and document the
host categories where the Plan 046 lane is portable. This plan does not
modify the Plan 046 surface, the existing rootless lane implementation,
or the static boundary checker; it carries the cross-host evidence
recovery and a *negative-baseline* set of host classifications so future
runs are predictable.

Plan 046 emitted the canonical typed blocker
`blocked_unprivileged_user_namespace` on this host because
`kernel.apparmor_restrict_unprivileged_userns=1` confines every
unprivileged user namespace to a restrictive AppArmor policy. The
ordinary invoking user cannot lift that policy from inside the
sandbox. Plan 046 closed with that blocker on disk; Plan 047 takes on
recovery.

## Starting repository state

The Plan 047 implementation starts from main commit `ba8e8ff` (the
Plan 046 closure), with these directly relevant files already in
place:

- `plans/046-rootless-sealed-namespace-evidence-lane.md`
- `plans/046-status.md`
- `plans/046-closure.md`
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`
- `scripts/check-rootless-interop-boundary.sh`
- `scripts/interop/rootless-enter.sh`
- `scripts/interop/probe-rootless-sandbox.sh`
- `tests/integration/ntcp2/harness/{rootless_topology.py,
  rootless_supervisor.py, rootless_inner_runner.py, interop_topology.py,
  topology.py, mixed_runner.py}`
- `tests/integration/ntcp2/manifest.toml`

Plan 047 inherits the Plan 046 blocker-baseline evidence directory
verbatim:

```text
target/interop/evidence/handshake-smoke-rootless--host-blocked/
  host-blocker-snapshot.txt      # kernel, sysctl, capability snapshot
  host-blocker-snapshot.txt.sha256
  probe-host-direct.json         # typed blocker, sha256 e9409a94…
  probe-host-direct.stdout.txt
  probe-ssh-i2ptest.json         # typed blocker, sha256 e9409a94…
  probe-ssh-i2ptest.stdout.txt
  manifest.json                  # content-addressed manifest
```

## Non-negotiable boundaries

1. Do not modify `scripts/check-rootless-interop-boundary.sh` or any of
   the Plan 046 surface files to weaken the static rules.
2. Do not introduce `sudo`, `ip netns`, `nft`, `setcap`, `--privileged`,
   or `--network host` in any Plan 047 artefact. The cross-host lane
   must remain runnable by an ordinary user.
3. Do not consume the Plan 046 host-blocker evidence as a protocol pass
   on this checkout. Plan 047 is recovery, not reframing.
4. The Plan 046 closure record is the plan-of-record for this checkout.
   Plan 047 adds to it; it does not replace it.
5. Where the Plan 046 host-blocker evidence already exists, it is the
   *negative baseline* for that host category. Plan 047 runs must
   produce either:
   - `rootless_sandbox_available`, **or**
   - a different typed blocker (e.g. `blocked_rootless_cleanup`,
     `blocked_external_route_present`, …).
6. Always re-run the typed probe on the new host under the same
   `--attestation-output` convention. Capturing the typed blocker to
   disk remains mandatory.

## Host compatibility taxonomy

Plan 046 worked on hosts of the form **"kernel permits unprivileged
user namespaces and AppArmor is either absent or its confinement
default is `0`."** Plan 047 enumerates the categories concretely:

| Category | Description | Probe outcome | Plan 047 lane status |
| --- | --- | --- | --- |
| `host.no-apparmor-and-userns-allowed` | Kernel built without AppArmor or with the driver unloaded, `unprivileged_userns_clone=1` | `rootless_sandbox_available` | Lane runs as designed |
| `host.apparmor-restrict-off` | `apparmor_restrict_unprivileged_userns=0`, `unprivileged_userns_clone=1` | `rootless_sandbox_available` | Lane runs as designed |
| `host.apparmor-restrict-on` | `apparmor_restrict_unprivileged_userns=1`, `unprivileged_userns_clone=1` (this host) | `blocked_unprivileged_user_namespace` | Lane is a typed blocker; Plan 046 closure applies |
| `host.userns-disabled` | `unprivileged_userns_clone=0` regardless of AppArmor | `blocked_unprivileged_user_namespace` | Lane is a typed blocker; needs `sysctl` (privileged) to run |
| `host.rootless-container-with-host-cap` | Container host denies user namespaces (e.g. container without `--privileged` and without `--userns=host`) | `blocked_unprivileged_user_namespace` (typically) | Lane is a typed blocker; needs a different launcher container |
| `host.in-vm-no-userns` | Guest kernel without `CONFIG_USER_NS=y` | `blocked_unprivileged_user_namespace` | Lane is a typed blocker |

The Plan 046 closure belongs to the `host.apparmor-restrict-on` row.
The first three rows are the recovery target.

## Recovery

### Recovery lattice

Plan 047 produces the Plan 046 lane on **any host in
`host.no-apparmor-and-userns-allowed` or `host.apparmor-restrict-off`**,
without touching the Plan 046 surface. The recovery is just running
the existing scripts on a host that satisfies one of those two
categories:

```text
bash scripts/interop/probe-rootless-sandbox.sh --attestation-path <att>
# expect: {"schema":1,"type":"rootless-sandbox-probe","outcome":"rootless_sandbox_available"}
bash scripts/interop/rootless-enter.sh --probe --attestation-output <att>
# expect: same outcome; also writes IsolationAttestation to disk
for d in i2pr-to-java-ipv4 java-to-i2pr-ipv4 i2pr-to-i2pd-ipv4 i2pd-to-i2pr-ipv4; do
  ref=$(case "$d" in *java*) echo java_i2p ;; *) echo i2pd ;; esac)
  bash scripts/interop/rootless-enter.sh \
    --scenario "$d" --reference "$ref" \
    --build-cache target/interop/cache --run-root target/interop/runs \
    --attestation-output "target/interop/evidence/$d--attestation.json"
done
```

Each direction's `attestation-output` is the per-direction copy of
`isolation attestation` and the `mixed_runner.py` record (without
secret-bearing material). All four must reference the same SHA-256
attestation under parent-network-state pre/post digest byte-equality.

The reference caches built by Plan 043 can be reused offline: the
existing `scripts/interop/offline-reuse.sh` flow does not require
privilege at execution time once the offline caches exist.

### Reference-cache reuse

Plan 043 produced the offline reference build cache path:

```text
target/interop/build/
  reference-build-summary.json    # overall build summary
  cache/                          # pinned reference tree, hash-indexed
```

Plan 047 *reuses* these caches where possible. The reuse is purely an
offline step and does not require any privilege. The reference-build
cache can be moved from a privileged builder host to the recovery host
without ever touching the public network. The Plan 043
`references.lock.toml` carries the locked source and build inputs so
that Plan 047 can verify the cache by re-hash.

### Evidence retention

Plan 047 records follow the same sanitized rules as Plan 046:

- evidence root: `target/interop/evidence/handshake-smoke-rootless/`
- `manifest.json` carries the content sha-256 of every retained file.
- Forbidden material is exactly the Plan 043 list: raw RouterInfo,
  identities, NTCP2 static keys, I2NP bodies, frame plaintext,
  transcripts, endpoint text, raw logs, packet captures, private paths.
- Cleanup verification is part of every direction: the
  `sandbox_attestation_sha256` is non-zero, the
  `parent_network_state_unchanged` byte-digest pre/post is byte-equal,
  and the namespace disappears with the process tree (which is the
  recovery host's `unshare`-level guarantee).

### Failure-mode capture

Plan 047 also recognizes that on some hosts the probe can succeed but a
later step can fail. The expected typed blockers are recorded in
`tests/integration/ntcp2/harness/rootless_supervisor.py` as
`ALLOWED_PROBE_OUTCOMES`. Plan 047 implementations must leave every
typed blocker on disk rather than swallowing it.

## Deliverables

1. **Compatibility taxonomy table** — recorded in this plan and mirrored
   into `specs/CONFORMANCE.md`.
2. **Recovery reproducer script** — `scripts/interop/recover-rootless.sh`,
   a thin wrapper that exercises the Plan 046 surface on a recovery
   host, verifies the typed probe, runs the four directional scenarios,
   and produces the sanitized manifest. The wrapper must call the
   existing Plan 046 scripts; no new privileges.
3. **Reference-cache reuse helper** —
   `scripts/interop/rootless-reuse-cache.sh` that consumes the Plan 043
   `target/interop/build/` cache on a recovery host and refuses to fetch
   on a miss.
4. **Cross-host evidence directory** — under
   `target/interop/evidence/handshake-smoke-rootless/`, with one
   `manifest.json` per host category under test. The current
   `handshake-smoke-rootless--host-blocked/` directory remains the
   `host.apparmor-restrict-on` baseline.
5. **Documented cross-host test matrix** — a table inside this plan
   that records host category → probe outcome → record outcome →
   blocker reason.
6. **Updated README / AGENTS.md / docs** reflecting that the rootless
   lane is runnable on `host.no-apparmor-and-userns-allowed` and
   `host.apparmor-restrict-off`, and that the `host.apparmor-restrict-on`
   row is the typed-blocker baseline already captured.

## Required external closure

On a recovery host in `host.no-apparmor-and-userns-allowed` or
`host.apparmor-restrict-off`:

```text
bash scripts/interop/recover-rootless.sh --host-category <cat>
# produces target/interop/evidence/handshake-smoke-rootless--<cat>-<host>/
```

The script must produce exactly one `manifest.json` per host category,
and the per-host manifest must contain:

- The probe attestation (typed `rootless_sandbox_available` with a
  non-zero sha256 of the IsolationAttestation payload).
- The four directional sanitized mixed-router records, each with
  `topology_kind = rootless-sealed-single-netns`, `privilege_model =
  unprivileged-userns`, the same `sandbox_attestation_sha256`, and
  `parent_network_state_unchanged = true`.

If a recovery host fails to produce any single one of those four records,
Plan 047 does not advance Milestone 3; it adds another typed blocker
under a new directory and records the cause. The negative baseline in
`handshake-smoke-rootless--host-blocked/` is not consumed.

## Plan 047 non-claims

- Plan 047 does not modify Milestone 3 closure policy.
- Plan 047 does not modify the Plan 044 / Plan 045 mixed-runner logic.
- Plan 047 does not advertise NTCP2 support.
- Plan 047 does not consume the Plan 046 host-blocker evidence as a
  protocol pass.
- Plan 047 does not introduce any privileged tool.

## Plan 047 deliverable checklist

- [ ] `scripts/interop/recover-rootless.sh`
- [ ] `scripts/interop/rootless-reuse-cache.sh`
- [ ] `target/interop/evidence/handshake-smoke-rootless/` directory on
      at least one recovery host
- [ ] `docs/architecture/interop-apparatus.md` cross-host matrix
- [ ] `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `GUARDRAILS.md`
      compatibility-taxonomy update
- [ ] `docs/security-model.md` Plan 046 + Plan 047 boundary
      reconciliation
- [ ] `specs/CONFORMANCE.md` Plan 047 cross-host section
- [ ] `specs/support.toml` unchanged (no row advances)

The deliverable checklist is not a milestone closure check; it is a
running inventory. Plan 047 closes when at least one recovery host has
produced a sanitized `handshake-smoke-rootless` manifest that
`scripts/check-ntcp2-interoperability.sh` accepts.
