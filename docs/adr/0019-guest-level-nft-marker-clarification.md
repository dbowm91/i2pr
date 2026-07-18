# ADR 0019: guest-level nft marker in the rootless Multipass lane

- Status: accepted for Plan 051 closure / Milestone 3 mixed-router evidence
- Date: 2026-07-18
- Decision owners: repository maintainers
- Supersedes: the nft OUTPUT `policy drop` clause implicitly carried over
  from Plan 046 into Plan 049/050; reconciles `prepare-offline.sh` with the
  Plan 046 namespace-only enforcement intent.

## Context

Plan 046 introduced the rootless sealed-namespace mixed-router evidence
lane. The lane is the primary NTCP2 evidence path on hosts that do not
grant non-interactive sudo, do not allow mutating host network state, or
do not carry the explicit Podman/privileged dual-netns backend. The
isolation guarantee in that lane is the network namespace created by
`unshare --user --net --mount --pid --fork --propagation private
--mount-proc --map-root-user` inside `scripts/interop/rootless-enter.sh`,
followed by `tests/integration/ntcp2/harness/rootless_supervisor.py`
asserting single-ID UID/GID maps, `no_new_privs`, distinct user/network
mount/PID namespaces, loopback readiness, synthetic address binding, and
the absence of external routes.

To make the offline state observable before launching the per-direction
run, `scripts/interop/multipass/prepare-offline.sh` (introduced under
Plan 046) installs a guest-level `nft` table:

```text
table inet i2pr_interop_offline {
    chain output { type filter hook output priority -100; policy drop; }
    rule output oifname lo accept
    rule output ip daddr 127.0.0.0/8 accept
    rule output ip6 daddr ::1 accept
}
```

The runner scripts (`run-direction.sh`, `run-matrix.sh`,
`run-evidence-lane.sh`, the older `profiler`) detect the offline state by
checking `nft list table inet i2pr_interop_offline` and emit the typed
blocker `blocked_execution_not_offline` if the table is absent.

The original Plan 046 intent of the guest-level chain was an additional
defence in depth: even outside the namespace, the guest itself would
deny non-loopback egress. On this host the chain is unreachable because
`multipassd` bridges guest commands through guest `sshd`; once the
`policy drop` is applied on the OUTPUT chain, every TCP segment the
guest `sshd` sends back to `multipassd` is dropped and the next
`multipass exec` call hangs until the daemon's SSH timeout fires.

The Plan 046 / Plan 049 evidence shows:

- `scripts/interop/multipass/prepare-offline.sh` originally invoked
  `cargo +1.95.0 build` against the just-applied `nft` rules. The
  `+1.95.0` selector triggers a rustup toolchain sync that needs DNS,
  which the `policy drop` denies, so the build hangs and the guest
  becomes unreachable. This was fixed by removing the redundant build
  (the binary is already produced by `offline-reuse.sh` with
  `CARGO_NET_OFFLINE=true` and `RUSTUP_TOOLCHAIN=1.95.0` plus
  `RUSTUP_AUTO_INSTALL=0`).
- With the build removed, the next `multipass exec` command after the
  `policy drop` is installed still hangs: `sshd`'s SYN-ACK reply from
  the guest to the `multipassd` bridge is an OUTPUT packet and is
  dropped. Adding `ct state established,related accept` does not recover
  the case because the SYN-ACK is in the SYN_RECV conntrack state, not
  ESTABLISHED, so nftables classifies it as NEW and drops it.
- `multipassd` then enters an unresponsive state because its own
  in-flight SSH query against the guest never finishes. Recovery from
  this state requires `sudo systemctl restart snap.multipass.multipassd`
  (admin-only) — a recovery cost Plan 049 does not tolerate.
- The Plan 046 design was never run end-to-end on the canonical host.
  Plan 046 closed with the typed `blocked_unprivileged_user_namespace`
  probe, which short-circuited the lane before the nft rule was ever
  applied against an active SSH session. The Plan 049/050/051 Multipass
  bridge is the first attempt to exercise the full design, and it is the
  first attempt that uncovers the nft-vs-SSH conflict.

## Decision

The guest-level `nft` OUTPUT chain installed by
`scripts/interop/multipass/prepare-offline.sh` is **a state marker, not
an enforcement boundary**. Its sole purpose is to make the offline state
observable to `run-direction.sh`, `run-matrix.sh`, and
`run-evidence-lane.sh` via `nft list table inet i2pr_interop_offline`.
The chain is installed with `policy accept` and the existing loopback
`accept` rules. It does **not** drop OUTPUT, and it does not otherwise
constrain guest egress.

The real isolation guarantee for the per-direction run is the
process-scoped namespace created inside
`scripts/interop/rootless-enter.sh`, supervised by
`tests/integration/ntcp2/harness/rootless_supervisor.py`. That namespace
has only a loopback interface, no default route, and no outside-network
connectivity by construction. Every router process that runs an NTCP2
handshake (i2pr, java_i2p, i2pd) is exec'd from inside that namespace
and never sees the host network.

Concretely:

- `prepare-offline.sh` keeps creating the `inet i2pr_interop_offline`
  table and chain (with `policy accept`) so the existing runner checks
  continue to function.
- `prepare-offline.sh` writes the same bridge state files
  (`offline-transition.json`, lifecycle `offline_ready`) so the bridge
  state machine is unchanged.
- The `offline-transition` receipt gains two new fields:
  `offline_enforcement = "namespace-only"` and `guest_nft_role =
  "marker"`, replacing the previous
  `offline_enforcement = "guest-nft-egress-deny"`.
- The lifecycle `last_typed_outcome` becomes
  `namespace-only-marker`, replacing `guest-nft-egress-deny`. Existing
  keys stay valid for forensic comparison.
- `scripts/check-multipass-interop-boundary.sh`, the static no-sudo /
  no-`ip netns` / no-`nft` checks, and the no-`eval` checks are
  unchanged. The new shape still does not introduce any host-namespace
  mutation, any privileged operation, or any host-firewall mutation;
  the existing rules continue to catch regressions.
- The Plan 046 design (Plan 046 closure document,
  `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`) keeps
  declaring the namespace-only policy as the canonical enforcement; this
  ADR reconciles the bridge layer (`prepare-offline.sh`) with that
  declared policy.

The Multipass guest itself continues to have a real multipass-managed
network (`mpqemubr0`/tap interface) and an SSHD that resolves addresses
from `dnsmasq`. The Multipass guest is the disposable execution
environment the Plan 049 contract describes; it is not the trusted
boundary. Its purpose is to provide a permissive kernel, a permissive
`unprivileged_userns_clone`, an unprivileged `i2ptest` account, and the
pinned reference routers — not to be a hardened sandbox for the host.

## Consequences

Positive:

- `multipass exec` continues to work between bridge steps, so the
  bridge order (`prepare-offline` → `run-direction.sh` × N →
  `export-evidence`) is reachable in a single guest session without
  external recovery.
- The runner scripts (`run-direction.sh`, `run-matrix.sh`,
  `run-evidence-lane.sh`) continue to function without modification:
  the `nft list table inet i2pr_interop_offline` check still passes and
  the `blocked_execution_not_offline` typed blocker remains in scope for
  any future guest that loses the marker.
- The typed evidence record (`offline-transition.json`) carries an
  honest description of the actual enforcement surface, so a reader can
  tell that the isolation guarantee comes from the namespace rather
  than from the guest-level chain.

Negative / acknowledged:

- The guest-level table is no longer a defence in depth against a
  scenario step that tries to talk to the public network from inside
  the guest. The Plan 046 / Plan 049 boundary checks forbid such a
  regression in the runner, but the table itself can no longer catch
  it. The namespace-only enforcement is the single point of truth.
- `blocked_offline_enforcement_unavailable` now reflects an nft setup
  failure (the table could not be created at all) rather than a
  guarantee failure. Callers that treated that blocker as a hard stop
  continue to be safe; callers that relied on the chain actually
  denying packets lose that property by design.

## Compatibility notes

- The `prepare-offline.sh` script previously required the `policy drop`
  rule to exist for the receipt to be valid. It now requires only the
  table and the `output` chain with `policy accept`. The
  `nft list table inet i2pr_interop_offline` check still passes.
- Existing typed blockers (`blocked_offline_enforcement_unavailable`,
  `blocked_execution_not_offline`, `blocked_source_tree_hash_mismatch`,
  `blocked_reference_cache_offline_reuse_failed`) all keep their
  semantics.
- The `last_typed_outcome` string changed from `guest-nft-egress-deny`
  to `namespace-only-marker`. The `Plan 051 closure` document and any
  consumer that hard-coded the old outcome should switch to the new
  one; no other call sites exist in the repository.

## Counterfactual

The alternative — keeping the `policy drop` and accepting that
`multipass exec` breaks between bridge steps — is rejected because:

1. It is unrecoverable by an ordinary user (the daemon hangs and only
   `sudo systemctl restart` can recover it), which Plan 049 explicitly
   forbids.
2. It does not provide any guarantee that the namespace-only
   enforcement does not already provide, given that every router
   process in the per-direction run is exec'd from inside the
   namespace.
3. It was never end-to-end exercised against an active SSH session in
   any prior closure. Plan 046's closure is a typed host blocker, not a
   successful lane.

This ADR is the design fix that lets Plan 051 attempt the four
directional mixed-router records end-to-end on the Multipass guest.
