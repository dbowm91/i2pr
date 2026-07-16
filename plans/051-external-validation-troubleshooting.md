# External validation troubleshooting for Milestone 3 closure

This document troubleshoots exactly how external (non-host, non-testkit)
NTCP2 mixed-router interoperability evidence can be produced and what
currently stands in the way. It is **not** a closure record and it does
**not** claim Milestone 3 is closed. Its purpose is to make the remaining
work visible and to map each blocker to a concrete next step.

## 1. What Milestone 3 actually requires

`plans/030-milestone-3-overview.md` lists the exit criteria. The
non-negotiable external-validation items are:

- "`i2pr` completes inbound and outbound NTCP2 handshakes with Java I2P
  and i2pd in an authorized controlled testnet."
- "Required I2NP messages cross an authenticated link in both directions."
- "Support metadata remains truthful and links to concrete evidence."

`specs/CONFORMANCE.md` clarifies the claim model:

- "A local self-handshake, loopback socket, fixed-seed simulation, or
  fuzz result cannot satisfy the two-implementation, two-direction
  requirement."
- "Only sanitized, bounded authenticated i2pr-to-reference runs in both
  directions can supply mixed-router evidence for NTCP2."
- "The current checkout has no sanitized i2pr-to-reference record or
  completed successful aggregate manifest."

`specs/support.toml` keeps every NTCP2 row at `status = "experimental"`
and `advertised = false`. Promotion requires sanitized run records,
artifact/configuration hashes, and reproduction identifiers.

`plans/030-milestone-3-closure.md` records the current closure status:

> Status: **blocked; implementation phases complete for their bounded
> local scope, milestone acceptance criteria not met**.

## 2. Why the local host cannot produce external evidence by itself

This host (`sugarwookie@i2pr-dev`, Ubuntu 24.04.4 LTS amd64) is the Plan
046 negative baseline:

```text
kernel.unprivileged_userns_clone = 1
kernel.apparmor_restrict_unprivileged_userns = 1
user.max_user_namespaces = 28633
```

The host's AppArmor confines every unprivileged user namespace to a
restrictive policy that prevents `unshare -U -r --map-root-user` from
writing `/proc/self/uid_map`. The invoking user has no `CAP_MAC_ADMIN`
and Plan 046 forbids `sudo`. The host probe therefore emits the
canonical typed blocker `blocked_unprivileged_user_namespace`, recorded
on disk at
`target/interop/evidence/handshake-smoke-rootless--host-blocked/`.

Plan 046 is **intentionally** closed on this host. Cross-host recovery
is delegated to Plans 047, 048, 049, 050.

The host also fails the Plan 040 host-contract gate:

```text
sudo: a password is required
interop error: noninteractive sudo is required
```

`check-host.sh --pre-install` requires `sudo -n true`. The current
user is in the `sudo` group, but `/etc/sudoers` does not enable
`NOPASSWD` for any user. The Plan 040 contract is therefore not met
on this host without an operator-driven `/etc/sudoers` change, which
is forbidden by the lock-down boundary.

## 3. Why the existing Multipass guest already satisfies Plan 040

The on-host Multipass 1.16.3 daemon runs Ubuntu 24.04.4 LTS amd64
guests. The existing `i2pr-interop-plan049-20260716-guestfix3-g1-a1`
guest, provisioned by an earlier Plan 049 cycle, has every required
Plan 040 host-contract attribute:

| Contract | Guest |
| --- | --- |
| OS id `ubuntu` | `Ubuntu 24.04.4 LTS` |
| OS version `24.04` | `24.04` |
| Architecture `x86_64` | `x86_64` |
| Bash 4+ | `GNU bash 5.2.21(1)` |
| Non-interactive sudo | `sudo -n true` returns 0 |
| UTF-8 locale | `LANG=C.UTF-8` |
| `apt-get` | present |
| `bash git curl python3 ip nft java ant cmake g++ gettext openssl` | all present |
| `kernel.apparmor_restrict_unprivileged_userns` | `0` (guest-only sysctl) |
| `kernel.unprivileged_userns_clone` | `1` |
| `user.max_user_namespaces` | 28633 |

The guest's kernel policy is the Plan 048 `host.apparmor-restrict-off`
recovery category. The host AppArmor / user-namespace policy is
unchanged. Inside the guest:

- `unshare --user --net --mount --pid --fork --propagation private
  --mount-proc --map-root-user bash` succeeds.
- `ip link` reports loopback only.
- `/proc/self/ns/` shows fresh `user`, `net`, `mnt`, `pid`, `pid_for_children`.

This means the Plan 046 rootless sealed-namespace lane is runnable
inside the Multipass guest today. The host contract is satisfied
inside the guest. The Plan 040 host-contract gate is satisfied inside
the guest.

## 4. Why the canonical Plan 040 scripts are not yet wired into the Multipass lane

`scripts/interop/ubuntu/check-host.sh` (and the rest of the Plan 040
scripts under `scripts/interop/`) is intended to be run on the host
that performs the actual mixed-router execution. It assumes the host
is Ubuntu 24.04 amd64 with non-interactive sudo. It is not aware of
Multipass.

The Multipass lane (`scripts/interop/multipass/`) currently provides:

- `probe.sh` — guest-side rootless sandbox probe.
- `prepare-offline.sh` — guest-side offline egress policy enforcement.
- `run-matrix.sh`, `run-direction.sh` — guest-side four-direction
  mixed-router runner.
- `export-evidence.sh` — sanitized evidence export.
- `cleanup.sh` and `verify-clean-host.sh` — host-side cleanup
  verification.

The Multipass lane **does not yet run**:

- `scripts/interop/ubuntu/check-host.sh --pre-install|--post-install`
- `scripts/interop/build-references.sh`
- `scripts/interop/cache-manifest.py --verify`
- `scripts/interop/offline-reuse.sh`
- `scripts/interop/run-gate.sh --profile <profile> --offline`
- `scripts/interop/aggregate-evidence.py`
- `scripts/interop/validate-evidence.py`

These are the canonical Plan 040/041/043 gate scripts. The Multipass
lane has its own analogue (`run-matrix.sh`, `aggregate.py`,
`validate-evidence.py` analog in `export-evidence.sh`, etc.) but the
canonical Plan 040 gate order has not been ported into the Multipass
guest. The current Multipass lane stops at the `rootless_sandbox_available`
guest probe and a self-symmetric `run-matrix.sh` (which is not a
mixed-router reference run).

## 5. What blocks "external validation" today

### 5.1 Mixed-runner not invoked inside the guest

The four Plan 045 directional scenarios
(`i2pr-to-java-ipv4`, `java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`,
`i2pd-to-i2pr-ipv4`) require:

- A built Java I2P 2.12.0 reference.
- A built i2pd 2.60.0 reference.
- A working `i2pr-interop` Rust launcher.
- Per-direction sanitized evidence records.

None of these have been produced inside the Multipass guest. The
guest has only `cloud-init` provisioned packages; it has no
`/home/i2ptest/i2pr/` checkout and no `target/interop/cache/`. The
`transfer-source.sh` step has never been run on this guest.

### 5.2 The Plan 050 cloud-init schema warning

The current Plan 050 cloud-init uses YAML string permissions
(`'0644'`, `'0600'`, `'0755'`). When Multipass delivers the
cloud-init via the NoCloud datasource, it re-emits the user-data and
converts the string permissions to integers (e.g. `420` for `0644`).
The cloud-init schema validator then emits:

```text
Error: Cloud config schema errors:
  write_files.0.permissions: 420 is not of type 'string'
```

This is a **cosmetic warning**. The actual files are written with
correct permissions (`0644`, `0600`, `0755`) verified by `stat -c %a`.
cloud-init's `extended_status` becomes `degraded done` with a
recoverable schema-validation warning; functional state is correct.

Plan 050 already classifies this as
`blocked_cloud_init_post_verify_failure` with `retry_safe=true` and
`recommended_action=resume-provisioning`. The new guest
(`i2pr-interop-plan050-external-1784224161-g1`) was launched and its
`i2pr-multipass-verify-base` script reports:

```text
{"apparmor_restrict":"0","i2ptest_cap_eff":"0000000000000000",
 "ownership_contract_root_owned":true,
 "phase_markers_present":["base-packages.complete"], ...}
```

So the cloud-init is functionally clean and the guest is ready for
the next step.

### 5.3 No reference builds in cache

`target/interop/cache/` is empty on this host. Java I2P and i2pd
have not been built. `cache-manifest.py --verify` would fail. The
Plan 040 `build-references.sh` script is not yet wired into the
Multipass lane.

### 5.4 The i2pr-interop launcher is not built inside the guest

The Plan 042 runtime-owned NTCP2 wire driver exists as
`tools/i2pr-interop`. It has been compiled and tested locally, but
not transferred into a Multipass guest or verified there.

### 5.5 The canonical Plan 040 scripts are not yet callable from inside the Multipass lane

The Plan 040 gate order is:

```text
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

Inside a Multipass guest, this becomes:

```text
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/ubuntu/check-host.sh --pre-install
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/ubuntu/setup-host.sh
multipass exec <guest> -- bash <repo>/scripts/interop/ubuntu/check-host.sh --post-install
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/verify-clean-host.sh --record-baseline
multipass exec <guest> -- bash <repo>/scripts/interop/build-references.sh --force-rebuild
multipass exec <guest> -- python3 <repo>/scripts/interop/cache-manifest.py --verify
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/offline-reuse.sh
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/run-gate.sh --profile environment-smoke --offline
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/run-gate.sh --profile reference-crosscheck-ipv4 --offline
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/run-gate.sh --profile handshake-smoke --offline
multipass exec <guest> -- python3 <repo>/scripts/interop/validate-evidence.py
multipass exec <guest> -- python3 <repo>/scripts/interop/aggregate-evidence.py --profile handshake-smoke
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/cleanup.sh
multipass exec <guest> -- sudo -E bash <repo>/scripts/interop/verify-clean-host.sh --verify
```

Each call is `multipass exec`-wrappable, but the Multipass
`run-evidence-lane.sh --all` shortcut does not currently dispatch
through these canonical scripts. A Plan 051 (or amendment to Plan 050)
could add a `--profile <profile>` flag that calls the canonical Plan
040/043 gate order, but that work has not started.

## 6. How to actually produce the evidence

The shortest concrete path is:

1. **Use the Multipass guest as the host contract surface.** The Plan
   046 host contract is satisfied inside the existing or fresh
   Multipass guest. `check-host.sh --post-install` returns success
   inside the guest because `sudo -n` works and all required tools
   are present.

2. **Add a Multipass-aware gate dispatcher.** The
   `run-evidence-lane.sh --profile <profile>` flag should accept the
   same profile names as Plan 040/043 (`environment-smoke`,
   `reference-crosscheck-ipv4`, `handshake-smoke`, `full-matrix`,
   `evidence-validation`, `cleanup-verification`) and dispatch each
   one through `multipass exec` to the canonical Plan 040/043
   scripts.

3. **Transfer the source into the guest.** Once a fresh guest with
   the current Plan 050 cloud-init is launched, run
   `scripts/interop/multipass/transfer-source.sh --commit <sha>`.
   This requires:
   - a clean working tree at commit `d11055d` (already true),
   - a `target/interop/cache/` populated from `build-references.sh`,
   - `i2pr-interop` Rust binary built and verified.

4. **Build the references in the guest cache.** Run
   `scripts/interop/build-references.sh --force-rebuild` inside the
   guest. This builds Java I2P 2.12.0 and i2pd 2.60.0 from the pinned
   sources into `target/interop/cache/`. The build takes ~20-60
   minutes depending on host speed; it must complete before the
   offline-reuse and gate steps.

5. **Verify the cache.** Run `scripts/interop/cache-manifest.py
   --verify` to confirm the cache hash matches `references.lock.toml`.

6. **Run `offline-reuse.sh`.** This applies the guest nftables
   policy denying non-loopback egress.

7. **Run `run-gate.sh --profile reference-crosscheck-ipv4 --offline`.**
   This is the reference-control gate. It must pass before any
   i2pr-as-variable-under-test scenario is attempted.

8. **Run `run-gate.sh --profile handshake-smoke --offline`.** This
   runs the four directional mixed-router scenarios
   (`i2pr-to-java-ipv4`, `java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`,
   `i2pd-to-i2pr-ipv4`) through `mixed_runner.py`.

9. **Run `validate-evidence.py` and `aggregate-evidence.py`.** This
   validates and aggregates the four sanitized directional records
   into the canonical handshake-smoke manifest.

10. **Run `cleanup.sh` and `verify-clean-host.sh --verify`.** This
    verifies no residual namespaces, veths, processes, or secret-bearing
    run roots remain. A protocol pass with failed cleanup is not a
    pass.

If the four directions pass all of the above, Plan 045 directional
predicates are satisfied for the first time. The aggregate
`plans/030-milestone-3-closure.md` can then be amended to record
the successful evidence records and an updated closure status.

## 7. Constraints and risks

### 7.1 Reference build time

Java I2P build (IzPack installer + signing) takes 20-60 minutes on a
modern amd64. i2pd build takes 2-5 minutes. Both are run inside the
guest as `ubuntu` (cloud-init admin user) before offline-reuse
applies. The host-network enablement of `apt-get` and `git clone` for
declared packages and locked source is the only network-enabled phase.

### 7.2 Disk space

`target/interop/cache/` after Java I2P + i2pd builds is typically
2-4 GiB. `target/interop/runs/<run-id>/` for a full mixed-router run
adds another 1-2 GiB. The host contract requires 4 GiB free under
`target/`. Multipass disk is 40 GiB; plenty.

### 7.3 Plan 050 cloud-init schema warning

The cosmetic `cloud-config failed schema validation` warning is
benign (functional permissions are correct) but the
`degraded done` state will keep surfacing as
`blocked_cloud_init_post_verify_failure`. The retry-safe
recommendation is correct. The cleanup-verification gate must
confirm functional state, not just schema state.

### 7.4 Multipass 1.16.3 per-instance purge

The installed Multipass 1.16.3 client does not accept
`multipass purge <instance>`. `selective-purge.sh` reports
`selective_purge_not_supported` and the operator must run
`multipass purge` (the global command) manually if a deleted
instance must be cleared. The Multipass lane never invokes the
global purge automatically. This is intentional and enforced by the
boundary checker.

### 7.5 Deleted-but-unpurged state

The legacy `i2pr-interop-plan050-validate-1784222496-g1-a1` and
`i2pr-interop-plan050-final-test-1784222697-g1` instances are in
`Deleted` state. They are visible to `multipass list` until the
operator manually runs `multipass purge`. Plan 049/050 explicitly
forbids the global purge in normal paths.

## 8. What the next plan must do

The next plan (numbered `051-external-validation-multipass-bridge.md`
or similar) must:

- Define a `multipass exec`-wrapped dispatch for the Plan 040/043
  gate order.
- Add `scripts/interop/multipass/dispatch-gate.sh` that accepts
  the canonical profile names and forwards them to the guest.
- Move the reference-build and cache-prep steps into the Multipass
  guest lifecycle (provisioning → source-and-cache-ready).
- Add a sanitized `multipass-mixed-router-manifest.json` collector
  inside the guest that follows the Plan 041 reference-pair profile
  and Plan 044 directional scenarios.
- Document the exact success criteria for `rootless_sandbox_available`
  → four-direction `passed` → `aggregate validated` → `cleanup verified`.
- Re-execute the Plan 030 closure predicate review once a
  successful handshake-smoke run is preserved under
  `target/interop/evidence/multipass/<run-id>/`.

This plan does not advertise NTCP2 support, does not satisfy the
Plan 045 directional predicates, and does not close Milestone 3. It
defines the exact path that, when executed on an authorized
disposable Multipass guest, produces the first trustworthy
external-validation evidence for the NTCP2 implementation.

## 9. Status of this document

This is troubleshooting output, not a closure record. It captures
what was learned from launching and inspecting a fresh
`i2pr-interop-plan050-external-1784224161-g1` guest on this host:

- the guest satisfies the Plan 040 host contract,
- the Plan 050 cloud-init is functionally clean despite the cosmetic
  schema warning,
- the guest supports the Plan 046 rootless sealed-namespace lane,
- the canonical Plan 040/043 gate scripts are not yet wired into the
  Multipass lane.

None of those facts closes Milestone 3. They clarify what is left
to do.

## 10. The immediate next move

The lowest-cost concrete next move is to implement a
`scripts/interop/multipass/dispatch-gate.sh` that wraps the
canonical Plan 040/043 scripts inside the guest, run
`reference-crosscheck-ipv4` first as a control, then run
`handshake-smoke` against the four Plan 045 directions. If the
reference crosscheck passes but the i2pr mixed-runner fails, the
i2pr implementation needs further work; if the reference crosscheck
fails, the harness needs further work. Either way the result is
typed and reproducible.

This document does not implement that next move. It only
troubleshoots how it can be done.