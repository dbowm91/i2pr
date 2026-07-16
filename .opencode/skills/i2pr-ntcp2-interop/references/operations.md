# Plans 038–046 operations reference

Run commands from the repository root. The authoritative harness instructions
are in `tests/integration/ntcp2/README.md`; this reference is a compact routing
guide for an agent.

Plan 043 gate order is:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

Preparation is network-enabled only for the lock-listed Ubuntu packages,
sources, verified IzPack artifact, and declared dependencies. Restore-only
offline reuse must re-hash the complete runtime tree and fail on a cache miss;
execution has no public egress. Cleanup and the independent clean-host check
run with an always-run policy and override protocol success on failure.

## Files to inspect

- `tests/integration/ntcp2/references.lock.toml`: Ubuntu contract, source pins,
  build commands, and the exact IzPack SHA-256.
- `tests/integration/ntcp2/scenarios/*.toml`: the eight bounded i2pr/reference
  scenario definitions. Keep their IDs synchronized with
  `tests/integration/ntcp2/manifest.toml`.
- `tests/integration/ntcp2/reference-scenarios/`: the separate Plan 041 pair
  schema and the two directional Java I2P/i2pd control scenarios.
- `tests/integration/ntcp2/mixed-scenarios/`: the four Plan 044 directional
  i2pr/reference scenarios with their own manifest. Each direction has a
  unique execution ID, declared initiator/responder, and launcher role.
- `tests/integration/ntcp2/harness/`: Python topology, adapters, process
  bounds, runner, evidence, mixed-runner, launcher renderer, data-phase
  oracle, and reference-trigger code.
- `scripts/interop/`: host setup, builders, isolation, matrix, gate staging,
  aggregate, and cleanup.
- `tools/i2pr-interop/`: non-production launcher seam; the current checkout
  composes bounded state preparation, listener/dial, handshake, authenticated
  link, and DeliveryStatus smoke through the Plan 044 mixed-runner. Its
  success is local driver validation only.
- `target/interop/evidence/`: sanitized records only; gate-prefixed files
  live alongside `run-manifest.json`. `target/interop/runs/` is secret-bearing
  and is deleted after every run.

## Host and build gates

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
```

Use `build-references.sh --offline` only with a complete prepared cache. The
builders reject dirty or mismatched source trees and record per-build hashes.
Do not substitute packaged routers or floating revisions.

The only reference identifiers are `java_i2p` and `i2pd`. Cache resolution uses
`target/interop/cache/current-cache.json` and a strict metadata schema; it does
not recursively search for a matching text substring.

## Profiles

```text
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full
bash scripts/interop/probe-rootless-sandbox.sh
bash scripts/interop/rootless-enter.sh --probe
bash scripts/interop/run-matrix.sh --profile handshake-smoke-rootless
```

`environment-smoke` checks reference startup, disposable RouterInfo production,
and cleanup. `reference-crosscheck-ipv4` runs both Plan 041 reference-pair
scenarios, validates the explicit private network ID and RouterInfo exchange,
and requires authoritative authenticated observations from both references; it
does not make an i2pr claim. `handshake-smoke` and `full` now invoke the bounded runtime-owned i2pr
launcher path composed with the Plan 044 mixed-router runner. Plan 044
expands each primary IPv4 scenario into four independently attributable
directional executions (`i2pr-to-java-ipv4`, `java-to-i2pr-ipv4`,
`i2pr-to-i2pd-ipv4`, `i2pd-to-i2pr-ipv4`) and renders each launcher
scenario through the strict renderer; the data-phase proof uses a typed
non-echo oracle (split send/receive per direction) rather than an assumed
echo. A successful launcher result is local driver validation only; the
reference profile still requires authenticated data exchange and cleanup,
not TCP or listener readiness alone. The runner emits
`i2pr-mixed-router-profile-not-wired` only for scenario IDs that are not
allowlisted for the active gate.

The data-phase scope remains DeliveryStatus (I2NP type 10): a 12-byte body,
21-byte NTCP2/SSU2 short transport message, and 24-byte NTCP2 block before
frame overhead and padding. Plan 044 replaces the unsupported echo
assumption with a typed oracle probe and split send/receive observations;
no reference echo behavior is currently proven, so this remains a bounded
plan scope rather than interoperability evidence.

## Plan 046 rootless sealed-namespace lane

```text
bash scripts/interop/probe-rootless-sandbox.sh --attestation-path <att>
bash scripts/interop/rootless-enter.sh --probe --attestation-output <att>
bash scripts/interop/rootless-enter.sh --scenario <id> --reference <ref> \
    --build-cache <path> --run-root <path> --attestation-output <att>
bash scripts/check-rootless-interop-boundary.sh
```

The probe emits a typed outcome; the outer wrapper writes the typed
blocker to `--attestation-output` even when the `unshare` call cannot
reach the inner runner (host-level blocker). The lane forbids `sudo`,
`setcap`, `--privileged`, `--network host`, `ip netns`, and any fallback
to the privileged backend. The static boundary checker
`scripts/check-rootless-interop-boundary.sh` enforces the privilege
surface independently of the host-level kernel policy.

The Plan 046 closure on this checkout is a typed host-level blocker.
The cause is `kernel.apparmor_restrict_unprivileged_userns=1`, which
confines every unprivileged user namespace to a restrictive AppArmor
policy even when `kernel.unprivileged_userns_clone=1`. The probe
records the canonical typed blocker
`blocked_unprivileged_user_namespace` on disk. Cross-host recovery is
recorded in `plans/047-cross-host-rootless-lane-expansion.md`.

The launcher status meanings are fixed: schema-1 `i2pr-interop-status` records
use fixed phase, result, reason-code, and aggregate counters; `listen` readiness
is separate from a later authenticated terminal result, `dial` has one
terminal result, and `inspect` returns only redacted metadata. Typed state,
authentication, data-phase, timeout, and cleanup failures are terminal results,
never readiness or evidence.

The Plan 041 runner serializes reference-pair executions with a host-local
lock. Its emergency cleanup owns the dedicated `java-*`/`i2pd-*` namespaces and
their short `jv…`/`iv…` veth names.

For one bounded run, use:

```text
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4 --reference java_i2p
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4 --reference i2pd
```

## Result interpretation and cleanup

`blocked_host_contract` means no router process or protocol claim was made.
`i2pr-mixed-router-profile-not-wired` means the active scenario ID is not
allowlisted for the current mixed-router gate. Rejected
configuration/state, authentication, timeout, cleanup, and evidence-validation failures remain
typed and visible. Never convert them to pass or omit them from the closure
record. An empty evidence directory is not success; Plan 041 reference-pair
records are harness controls, not i2pr mixed-router evidence.

```text
bash scripts/interop/validate-evidence.py
python3 scripts/interop/aggregate-evidence.py --profile <profile>
bash scripts/check-ntcp2-interoperability.sh
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify --baseline target/interop/build/clean-host-baseline.json
```

Retain only sanitized typed JSON records and approved hashes. If cleanup is
uncertain, stop and investigate the disposable host before any later run.

The workflow now exposes the ordered manual Plan 043 lane, but the checkout has
no completed successful aggregate manifest. Plan 044 closes the deterministic
defects (Java RouterInfo export, schema-1 sanitation, gate-specific staging,
locked Cargo), wires the mixed-runner composition, the strict launcher
renderer, the non-echo data-phase oracle, and the gate-staging archival
design, but the authorized Plan 044 mixed-router execution has not been
performed. Do not describe the gate chain as passing, do not present
reference-only control records as i2pr mixed-router evidence, and do not
advertise NTCP2 or close Milestone 3.

## Plan 046 rootless operations

The primary evidence topology for the Plan 046 lane is
`rootless-sealed-single-netns` with privilege model `unprivileged-userns`.
The legacy `privileged-dual-netns-veth` topology is reserved for explicit
later qualification work and is never the default and never a silent
fallback. Run the probe before any mixed-router run on a candidate host:

```text
bash scripts/interop/probe-rootless-sandbox.sh
```

A typed blocker such as `blocked_unprivileged_user_namespace`,
`blocked_loopback_unconfigured`, `blocked_synthetic_bind_failed`, or
`blocked_external_connect_succeeded` is a hard stop, not a fallback. The
sandbox-only verify path is:

```text
bash scripts/interop/rootless-enter.sh --probe
```

A bounded direction run from the inner runner is:

```text
bash scripts/interop/rootless-enter.sh --scenario i2pr-to-java-ipv4 \
    --reference java_i2p --build-cache <path> --run-root <path>
```

Outer entrypoint invariants:

- Creates the sandbox via
  `unshare --user --net --mount --pid --fork --propagation private --mount-proc --map-root-user`.
- Sets `I2PR_INTEROP_ROOTLESS_INNER=1` before `exec`.
- Forbids `sudo`, `ip netns`, `nft`, `setcap`, `--privileged`,
  `--network host`, and any fallback to the privileged backend.
- Allowlists the active operation and scenario kind; rejects everything else
  before exec.

A passed mixed-router record requires all of:

- `topology_kind == "rootless-sealed-single-netns"`.
- `privilege_model == "unprivileged-userns"`.
- non-zero `sandbox_attestation_sha256`.
- `parent_network_state_unchanged == True`.

The static rootless boundary checker is:

```text
bash scripts/check-rootless-interop-boundary.sh
```

It fails when rootless-owned files contain prohibited patterns, when the
gate catalog omits `handshake-smoke-rootless`, or when the evidence
validation does not require the sandbox attestation. Plan 046 does not
advertise NTCP2 support and does not close Milestone 3 by itself.

## Plan 048/049/050 Multipass recovery operations

The host-level `blocked_unprivileged_user_namespace` result remains the
negative baseline. The recovery lane is a disposable Multipass guest. Its
reviewed environment ID is distinct from the safe run ID, concrete instance
name, and generation. The default path reserves host lifecycle state
atomically before launch and uses a fresh bounded instance name; the legacy
`i2pr-interop-rootless` name is not authoritative. Use the checked-in manifest
and lifecycle-owned wrapper:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --create
bash scripts/interop/multipass/run-evidence-lane.sh --prepare
bash scripts/interop/multipass/run-evidence-lane.sh --probe
bash scripts/interop/multipass/run-evidence-lane.sh --run
bash scripts/interop/multipass/run-evidence-lane.sh --export --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh --destroy-owned \
  --run-id <safe-id>
```

The one-command lane is:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --all \
  --run-id <safe-id> --destroy-after-export
```

Read-only inspection and recovery are explicit:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --inspect --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --resume-owned \
  --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --adopt-owned \
  --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --recreate-owned \
  --run-id <safe-id>
bash scripts/interop/multipass/run-evidence-lane.sh --destroy-owned \
  --run-id <safe-id>
```

`--adopt-owned`, `--resume-owned`, `--recreate-owned`, and `--destroy-owned`
require matching host/guest ownership records, ownership-token hash,
environment/cloud-init/source/cache digests, generation, guest policy,
execution-user privileges, mounts, snapshots, and process state. A name-only
match is not ownership. Normal execution never silently adopts, recreates,
deletes, stops, or purges an existing resource; global `multipass purge` is
forbidden. Unknown, ambiguous, and deleted-but-unpurged states are typed
blockers. Per-run/per-instance locks serialize state transitions.

Cloud-init is the administrative preparation phase. Source transfer requires
a clean exact commit and deterministic archive; cache transfer requires the
verified canonical `target/interop/cache` and its build sidecar manifest.
After `prepare-offline.sh`, guest nftables denies non-loopback egress and all
scenario commands run as `i2ptest`. The host baseline probe is informational;
the guest probe must pass after ownership/policy verification and again before
any router. Snapshot names are only `provisioned` and `source-and-cache-ready`
and are bound to the environment contract and generation; host mounts,
arbitrary guest commands, and privileged fallback are forbidden.

Export accepts only the sanitized bundle and atomically installs it at
`target/interop/evidence/multipass/<run-id>/`. Preserve that directory before
owned destruction. Environment and directional records must share the
environment ID, run ID, generation, ownership/contract digests, separate host
and guest probe results, and environment evidence hash. Missing Multipass,
guest policy, source/cache, probe, offline, cleanup, or evidence requirements
are typed blockers; pre-router blockers are never protocol passes and no
support row or Milestone 3 claim changes automatically.

### Plan 050 cloud-init recovery and guest-probe pass

Cloud-init failure classification is sanitized and typed:

```text
bash scripts/interop/multipass/cloud-init-status.sh --instance-name <name>
```

It captures `cloud-init status --long`, the four canonical services, and
the boot-finished marker, classifies via
`scripts/interop/multipass/cloud_init_status.py`, and emits sanitized
JSON. Failure classes are
`blocked_cloud_init_post_verify_failure`,
`blocked_cloud_init_service_failure`,
`blocked_cloud_init_boot_timeout`,
`blocked_cloud_init_status_unparseable`,
`blocked_cloud_init_user_incomplete`, and
`blocked_cloud_init_phase_missing`. Each record carries `retry_safe`
and `recommended_action` fields. The compatibility alias
`blocked_cloud_init_failed` is retained only for transition consumers.

The base cloud-init no longer installs `rustup` or any host toolchain
inside the guest. After provisioning, post-verify the base environment:

```text
bash scripts/interop/multipass/verify-base.sh \
  --run-id <safe-id> --instance-name <name> \
  --output <evidence-output.json>
```

`verify-base.sh` runs `/usr/local/sbin/i2pr-multipass-verify-base` via
`multipass exec`, parses the JSON, writes a sanitized
`multipass-base-verify` record, and verifies the ownership contract file
ownership/mode.

The minimum probe-only flow runs create-adopt + cloud-init-status +
verify-base + probe and writes a single `multipass-guest-probe-only`
record:

```text
bash scripts/interop/multipass/run-evidence-lane.sh \
  --guest-probe-only --run-id <safe-id>
```

`--guest-probe-only` is mutually exclusive with `--create`,
`--prepare`, `--probe`, `--run`, `--export`, `--all`, `--inspect`,
`--adopt-owned`, `--resume-owned`, `--recreate-owned`, and
`--destroy-owned`. The flag forbids router launch, cache transfer, and
`run-matrix.sh` execution.

Deleted-but-unpurged instances owned by the active contract can be
cleaned with:

```text
bash scripts/interop/multipass/selective-purge.sh \
  --run-id <safe-id> --instance-name <name>
```

`selective-purge.sh` validates the ownership contract against
`environment_manifest_sha256`, confirms the instance is in `Deleted`
state via `multipass list --format json`, and only invokes
`multipass purge <instance>` (per-instance) when supported by the
installed client. Unowned collisions, unsupported client versions, or
missing manifests return typed blockers
(`ownership_not_proven`, `selective_purge_not_supported`,
`resource_already_absent`) without mutating global Multipass state.
The static boundary check
`bash scripts/check-multipass-interop-boundary.sh` enforces the new
artifacts, sanitized taxonomy, phase markers, absence of `rustup` in
cloud-init, absence of `eval`, and absence of any global `multipass
purge` form in normal paths.
