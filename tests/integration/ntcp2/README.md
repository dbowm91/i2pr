# Plan 038/040/041 Ubuntu reference-router interoperability harness

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

The Plan 038/040 i2pr/reference scenarios create one `i2pr-*` namespace and
one `ref-*` namespace. Plan 041 uses a separate reference-pair owner and
creates `java-<short-run-id>` and `i2pd-<short-run-id>` namespaces. Both veth
endpoints leave the host namespace. The only allowed path is the directly
connected synthetic peer subnet; default routes, DNS, host bridges, public
egress, reseed, bootstrap, NAT/UPnP, SSU/SSU2, and unrelated client services
are forbidden. Route checks are primary and namespace-scoped nftables rules
are defense in depth.

Run a bounded scenario with the reference cache and optional explicit paths:

```text
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4 --reference java_i2p
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4 --reference i2pd
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full
```

`environment-smoke` validates reference startup, disposable RouterInfo
production, and cleanup only. `reference-crosscheck-ipv4` runs the two dedicated
Plan 041 scenarios, `reference-java-i2pd-ipv4` and
`reference-i2pd-java-ipv4`. It requires both offline caches, the explicit
private network ID 99, strict RouterInfo validation, one-way firewall policy,
and authoritative authenticated observations from both routers. It is a
reference-only control and is not i2pr evidence. The handshake/full profiles remain
`blocked` with reason `i2pr-mixed-router-profile-not-wired` while the reference
runner's i2pr/reference topology is incomplete; this is an explicit blocker,
not a skipped success. The launcher itself now has a bounded local
listener/dial, handshake, authenticated-link, and DeliveryStatus path.

The dedicated launcher seam is separate from the normal daemon:

```text
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

It emits versioned typed JSON only. `listen` emits listener readiness followed
by one terminal typed result, and `dial` emits one terminal typed result.
`inspect` delegates
RouterInfo structural, signature, and NTCP2-address validation to the
repository's strict Rust parser. The reference-pair runner uses this
inspection only inside a deleted run root and never treats it as mixed-router
i2pr evidence.

## Cleanup and evidence

Every runner path stops and drains children, deletes both namespaces and veth
state, removes identities, keys, RouterInfo, configs, raw logs, and run roots,
and treats cleanup failure as scenario failure. Emergency cleanup is:

```text
sudo -E bash scripts/interop/cleanup.sh
```

Plan 041 reference-pair runs hold a host-local lock so directional runs cannot
overlap. Emergency cleanup also owns their `java-*`/`i2pd-*` namespaces and
short `jv…`/`iv…` veth names.

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

Plan 041 pair records use schema 2 and retain only both reference revisions,
artifact/tree/configuration hashes, a topology hash, typed RouterInfo and
authenticated-link observations, bounded counters, direction policy, cleanup
result, digest, and reproduction command. They never retain raw RouterInfo,
identities, keys, endpoints, or logs.

## Troubleshooting

- A host or namespace error is fail-closed; run the pre/post checker and fix
  Ubuntu, amd64, UTF-8 locale, `sudo`, `iproute2`, or kernel namespace support.
- `blocked` with reason `i2pr-mixed-router-profile-not-wired` means the
  reference runner is not yet connected to the i2pr launcher. Do not replace it
  with a self-handshake or treat local launcher success as reference evidence.
- `blocked_host_contract` means execution did not start and no protocol claim
  may be inferred.
- Inspect only disposable local build metadata. Never retain raw logs, packet
  captures, RouterInfo, identities, keys, endpoint diagnostics, or payloads.
