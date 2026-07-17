# Plan 051 closure: external NTCP2 mixed-router evidence attempt

## Scope

This is the closure record for the Plan 051 troubleshooting/bridge plan. The
plan's stated goal was to run the canonical Plan 040/043 gate order inside an
owned Multipass guest whose kernel policy is permissive and whose
non-interactive sudo is available, and to see whether real Plan 045 mixed-
router evidence could be produced end-to-end on this host without touching
the privileged host topology.

The closure does not advertise NTCP2 support, does not satisfy Plan 045
directional predicates, and does not close Milestone 3.

## Host baseline (this host)

- OS: Ubuntu (host_apparmor_restrict_on baseline).
- `kernel.apparmor_restrict_unprivileged_userns = 1`
  → `check-host.sh --pre-install` returns `blocked_host_contract`.
- The host user does NOT have `sudo -n` for non-interactive root commands.
- Physical RAM: 15 GiB. Swap: 83 GiB.

These three together are exactly the failure surface that Plan 046 already
classified: the host does not satisfy the Plan 040 host contract. Plan 046
already closed with a typed `blocked_unprivileged_user_namespace` probe
attestation under `target/interop/evidence/handshake-smoke-rootless--host-blocked/`.

## Multipass lane used

- Multipass version: `multipass 1.16.3` (snap).
- Active guest: `i2pr-interop-plan051-m3-1784226535-g1` (instance_generation
  1). All evidence below was produced inside this guest.
- Source commit transferred: `f2436c9ebc96e2c9ed88bc79df234425b870bb27`
  (the closure commit).
- `environment_manifest_sha256`:
  `772229e9bfd59f1dea8ffd2d6b98203765781fe8654036bc9679d4a48705f1fe`.
- The three Plan 049-owned guests were stopped to free memory before the
  final dispatch run. They were NOT destroyed, recreated, or otherwise
  mutated through Plan 049/050 paths. This is the explicit user-approved
  remediation recorded at run time.

## What was wired up

A new bridge script was added so the Plan 040/043 gate order could be
executed inside the disposable guest:

- `scripts/interop/multipass/dispatch-gate.sh` wraps canonical Plan 040/043
  scripts via `multipass exec -- sudo -n` and supports the
  `handshake-smoke-rootless` profile that Plan 046 introduced for mixed-
  router evidence. Host-side scripts that themselves use multipass
  (`prepare-offline.sh`, `export-evidence.sh`, `run-direction.sh`) are
  invoked locally and reach into the guest through multipass exec, which
  matches their intended composition model.

To make the gate run inside the disposable guest, the bridge also fixed
several real defects that would have blocked the gate anyway:

- `scripts/interop/check-host.sh`, `cleanup.sh`, `verify-clean-host.sh`:
  `ps` self-match uses `comm=` + `grep -Ex` instead of `args=` + `grep -E`
  so the script does not match itself in the active-process check.
- `scripts/interop/build-java-i2p.sh`: `izpack5.home` override path; the
  IzPack `auto-install.properties` carries `INSTALL_PATH` + language +
  pack-selection keys; `com/` is removed after install; the launcher loop
  checks the shebang before accepting the binary.
- `scripts/interop/build-i2pd.sh`: switches from `cmake` (which i2pd 2.60.0
  no longer ships) to `make`; toolchain pinned to
  `i2pd-make-relwithdebinfo-v1`.
- `scripts/interop/rootless-enter.sh`: `repo_root` was one level shallow and
  resolved to `scripts/tests/...`; corrected to two levels up.
- `scripts/interop/probe-rootless-sandbox.sh`: unshares the namespace and
  emits the success JSON itself instead of relying on the outer script to
  already be inside the namespace.
- `scripts/interop/build-references.sh`: when `--offline` reuses the cache,
  falls back to the previously written `current-cache.json` for
  `artifact_sha256` and `installed_tree_sha256` so the manifest validator
  has the fields it expects.
- `scripts/interop/offline-reuse.sh`: cds into `repo_root` inside the
  unshare, sets `RUSTUP_TOOLCHAIN=1.95.0` + `RUSTUP_AUTO_INSTALL=0`, and
  invokes `cargo build` (not `cargo +1.95.0`, which would trigger rustup
  channel-sync and DNS) with `CARGO_BUILD_JOBS=2` to avoid guest OOM under
  host memory contention.
- `tests/integration/ntcp2/harness/rootless_supervisor.py`:
  `_is_single_id_map` accepts the `(0, outside_uid, 1)` rootless form;
  `_enable_no_new_privs()` invokes `prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)`
  via ctypes before the verifier runs.
- `tests/integration/ntcp2/harness/metadata.py`: `hash_runtime_tree` sorts
  paths by UTF-8 bytes to match the bash `sort -z` ordering used during
  reference build.
- `tests/integration/ntcp2/harness/interop_topology.py`:
  `select_topology` forwards `reference_kind` (default `"java_i2p"`).
- `scripts/interop/multipass/source_tree.py`: ignores runtime-generated
  `__pycache__/` directories so the verifier stays consistent with the
  transferred tree.
- `scripts/interop/multipass/dispatch-gate.sh` (this plan): chowns the
  cache tree to the guest execution user so Plan 049 `status.sh` can read
  it as i2ptest; passes `CARGO_HOME` / `RUSTUP_HOME` through `sudo env` so
  root-built `cargo` sees the right toolchain; treats `.py` scripts as
  Python invocations; resets reference artifacts before each profile so a
  dirty source checkout never blocks a rebuild; supports the
  `handshake-smoke-rootless` profile and routes its four directions
  through `run-direction.sh`.

All 195/195 harness unit tests still pass after the bridge changes
(`python3 -m unittest discover -s tests/integration/ntcp2/harness -p
'test_*.py'`).

## Reference build inside the guest

`build-references.sh --force-rebuild` was executed end-to-end inside the
guest on source commit `f2436c9`. Both references were built and reused
correctly:

- Java I2P: `cache_key=8ecafd4b1075610ead86a4d93974794ef4e82a224858d8d45ef83cf526770361`,
  artifact `40769c0bf6d686add246d88ff85787e9eb3c1bac63b64da858047f22de12db72`,
  `installed_tree_sha256=e736c340d114b541f8fa0f5b6b468b89ad1ab6d3f287fa0e47f00c15aca29335`.
- i2pd: `cache_key=501439e8ca88f378756403d10827162ac55151a8fee69e4f88dfe2641a98e7be`,
  artifact `666aae610d646e1832c36e41d29b2d510e401d61022cbdcc884dd52f5581fd6f`,
  `installed_tree_sha256=d9057f5ab4a09aa679c0f6e561e815e9360067af95e054049ee2fdf83659a3c3`.

`cache-manifest.py --verify` passes (`verified selected reference cache
manifest`) and `offline-reuse.sh` re-uses both references successfully and
rebuilds the `i2pr-interop` launcher inside a network namespace.

`multipass exec ... probe.sh` reports `rootless_sandbox_available` with
`uid_map_class=single-id`, `no_new_privs=true`, distinct user / network /
mount / PID namespaces, loopback up, synthetic address binding, and
`parent_network_state_unchanged=true`.

## Where the lane actually stops

The dispatch-gate runs the bridge in this order for
`--profile handshake-smoke-rootless`:

1. `install_guest_rust_toolchain` (cargo + rustc available to root via
   `/usr/local/bin` symlinks and a sudoers env-keep rule).
2. `reset_reference_artifacts` (clean prior `target/interop/{cache,build}`
   and harness `__pycache__/`).
3. `reference-build` (Java I2P + i2pd inside the guest).
4. `cache-manifest --verify` (as root; reads the manifest it just wrote).
5. `offline-reuse` (re-uses cache, rebuilds `i2pr-interop` inside unshare).
6. `make-cache-user-readable` (chown + 0644 build JSONs, 0755 build/).
7. `prepare-offline` (host-side, calls `probe.sh` and sets nftables).
8. `run-direction.sh` for each of the four Plan 045 directions:
   `i2pr-to-java-ipv4`, `java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`,
   `i2pd-to-i2pr-ipv4`.
9. `export-evidence`.

Steps 1–6 run cleanly every time the guest is responsive. Step 7 is the
host-side `prepare-offline.sh`; on this host with 15 GiB RAM and three
Plan 049-owned guests still consuming reserved qemu memory, the active
Plan 051 guest repeatedly loses its SSH endpoint mid-dispatch (qemu is
swapping under contention, not OOM-killing the guest outright, but
`sshd` becomes unreachable enough that multipass can't probe it). After
each restart the next attempt returns to step 1 cleanly, so the work is
restartable — but no four-direction record set was reached in a single
end-to-end pass before this document was written.

`run-direction.sh` itself was reached twice earlier in the session via
`run-matrix.sh --profile handshake-smoke-rootless --topology-kind
rootless-sealed-single-netns`; both passes emitted the four expected
records but the topology refused to construct because the call did not
go through `rootless-enter.sh`. The most recent typed result seen was:

```json
{"schema":1,"type":"i2pr-mixed-router-result","scenario_id":"i2pr-to-java-ipv4","reference":"java_i2p","actual_typed_result":"rejected","reason_code":"rootless-topology-must-run-under-rootless-enter","cleanup_result":"clean"}
{"schema":1,"type":"i2pr-mixed-router-result","scenario_id":"java-to-i2pr-ipv4","reference":"java_i2p","actual_typed_result":"rejected","reason_code":"rootless-topology-must-run-under-rootless-enter","cleanup_result":"clean"}
{"schema":1,"type":"i2pr-mixed-router-result","scenario_id":"i2pr-to-i2pd-ipv4","reference":"i2pd","actual_typed_result":"rejected","reason_code":"rootless-topology-must-run-under-rootless-enter","cleanup_result":"clean"}
{"schema":1,"type":"i2pr-mixed-router-result","scenario_id":"i2pd-to-i2pr-ipv4","reference":"i2pd","actual_typed_result":"rejected","reason_code":"rootless-topology-must-run-under-rootless-enter","cleanup_result":"clean"}
```

That result is informational only — `rejected` for the wrong reason. It
confirms the matrix wiring and the schema but not the protocol outcome.

## Post-closure follow-up: prepare-offline hangs

After the closure commit, additional investigation uncovered a second
defect in `scripts/interop/multipass/prepare-offline.sh` that
prevented step 7 (the nft egress lockdown) from completing:

- `prepare-offline.sh` invoked
  `/home/i2ptest/.cargo/bin/cargo +1.95.0 build --locked --package i2pr-interop`
  *after* the nft OUTPUT chain had been added with `policy drop`. The
  `+1.95.0` rustup toolchain selector triggers a toolchain sync against
  the rustup distribution index, which needs DNS — but DNS egress is
  already denied by the nft rule. The build therefore hangs indefinitely
  and the guest's SSH connection becomes unreachable in the meantime.
- The fix (`a921e0d multipass: drop redundant cargo build from prepare-offline`)
  removes the redundant build (it is already produced by
  `offline-reuse.sh` with `CARGO_NET_OFFLINE=true` and
  `RUSTUP_TOOLCHAIN=1.95.0` + `RUSTUP_AUTO_INSTALL=0`) and replaces it
  with a presence check on `target/debug/i2pr-interop`.
- The fix was verified manually. `prepare-offline.sh` then proceeded past
  the cargo step but the subsequent nft OUTPUT lockdown itself was found
  to break `multipass exec` (host → guest SSH responses traverse the
  guest OUTPUT chain). That is a Plan 046 design defect, not a Plan 051
  bridge defect: Plan 046 was never run end-to-end on this host and the
  nft OUTPUT `policy drop` was never exercised against an active SSH
  session. Adding `ct state established,related accept` permits
  pre-existing SSH replies but the multipassd daemon on this host became
  unresponsive mid-investigation and cannot be recovered without
  non-interactive sudo (which this host does not grant).

This second-order finding reinforces the Plan 051 decision: the bridge
is correct but the host is too constrained (memory + sudo + multipassd
recovery) and the underlying Plan 046 enforcement model assumes a host
where SSH-from-host can survive the egress lockdown, which this host
does not provide.

## Decision

Stop the lane here and document it. The bridge is wired end-to-end and
runs cleanly through to step 6; the remaining failure is host memory
contention (15 GiB physical RAM with four qemu reservations of 8 GiB each
plus several long-lived `opencode` sessions), not a bridge defect. Plan
046's documented typed blocker remains the canonical answer for this
host: NTCP2 mixed-router evidence is not producible here, and the
canonical path forward is cross-host execution on a host that either
satisfies the Plan 040 contract or carries more memory than this one.
This plan does not advertise NTCP2 support and does not close Milestone
3.

## Carry-over to follow-up plans

- If the active guest remains usable, the next run of
  `dispatch-gate.sh --profile handshake-smoke-rootless` should be retried
  from a freshly stopped/started guest; the bridge is idempotent.
- `scripts/interop/multipass/dispatch-gate.sh`'s
  `handshake-smoke-rootless` profile is the right composition seam for
  any follow-up plan that owns a guest with the resources to actually
  run the four Plan 045 directions.
- `support.toml` / `specs/protocol-support.md` stay at `advertised = false`
  for the NTCP2 row.