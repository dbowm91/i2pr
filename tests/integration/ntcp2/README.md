# Plan 038 Ubuntu reference-router interoperability harness

This is a manual, opt-in integration path. It is separate from normal
workspace tests and is restricted to Ubuntu 24.04 amd64. The harness is not a
public bootstrap configuration, does not enable `i2pr-daemon`, and does not
advertise NTCP2.

The pinned targets are Java I2P 2.12.0 at revision
`2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd 2.60.0 at revision
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. Their source URLs, build commands, package set, and
verified IzPack 5.2.4 hash are in [`references.lock.toml`](references.lock.toml).
Build hashes are recorded per build; no nondeterministic stable artifact hash
is fabricated.

## Preparation

Preparation is the only phase allowed to install packages or fetch sources.
Run it from the repository root on a disposable Ubuntu host:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
```

The setup script installs only the declared packages, never enables a router
service, and is safe to repeat. The builders clone/fetch only the locked
repositories, detach at the exact revisions, reject dirty or mismatched source
trees, and write cache/build metadata below `target/interop/`. Cache lookup
uses the canonical `java_i2p` and `i2pd` identifiers plus
`target/interop/cache/current-cache.json`; it never scans arbitrary metadata.

Offline repeatability uses only an already prepared cache:

```text
bash scripts/interop/build-references.sh --offline
```

## Isolated execution

Each scenario creates a unique run root, one `i2pr-*` namespace, and one
`ref-*` namespace. Both veth endpoints leave the host namespace. The only
allowed path is the directly connected synthetic peer subnet; default routes,
DNS, host bridges, public egress, reseed, bootstrap, NAT/UPnP, SSU/SSU2, and
unrelated client services are forbidden. Route checks are primary and
namespace-scoped nftables rules are defense in depth.

Run a bounded scenario with the reference cache and optional explicit paths:

```text
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4 --reference java_i2p
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4 --reference i2pd
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full
```

`environment-smoke` validates reference startup, disposable RouterInfo
production, and cleanup only. `reference-crosscheck-ipv4` is reserved for
Plan 041 and currently returns the typed `blocked_missing_driver` result rather
than running an i2pr scenario. The handshake/full profiles remain
`blocked_missing_driver` until the complete runtime-owned wire adapter exists;
this is an explicit blocker, not a skipped success.

The dedicated launcher seam is separate from the normal daemon:

```text
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

It emits typed JSON only and currently reports the missing-driver result for
listen/dial. It must not be used as interoperability evidence by itself.

## Cleanup and evidence

Every runner path stops and drains children, deletes both namespaces and veth
state, removes identities, keys, RouterInfo, configs, raw logs, and run roots,
and treats cleanup failure as scenario failure. Emergency cleanup is:

```text
sudo -E bash scripts/interop/cleanup.sh
```

Only sanitized JSON records containing typed outcomes and hashes may be
retained under `target/interop/evidence/`; secret-bearing run roots are always
deleted. Validate records with:

```text
bash scripts/interop/validate-evidence.py
bash scripts/check-ntcp2-interoperability.sh
```

An empty evidence directory is reported as “no evidence”, never as success.
Local testkit, loopback, vectors, and fuzz results remain useful local
evidence but cannot satisfy the two-reference, two-direction requirement.

## Troubleshooting

- A host or namespace error is fail-closed; run the pre/post checker and fix
  Ubuntu, amd64, UTF-8 locale, `sudo`, `iproute2`, or kernel namespace support.
- `blocked_missing_driver` means the prepared references or complete i2pr wire
  adapter is unavailable. Do not replace it with a self-handshake.
- `blocked_host_contract` means execution did not start and no protocol claim
  may be inferred.
- Inspect only disposable local build metadata. Never retain raw logs, packet
  captures, RouterInfo, identities, keys, endpoint diagnostics, or payloads.
