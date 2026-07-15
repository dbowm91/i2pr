# ADR 0016: Ubuntu build-system interoperability gates

- Status: accepted for Plan 043
- Date: 2026-07-15
- Decision owners: repository maintainers

## Context

The Plan 038/040/041 harness separates source preparation from isolated
reference execution, but a build-system lane needs a stronger promotion
boundary. A green preparation job, a reference-only crosscheck, or a local
launcher result must not be mistaken for i2pr interoperability. Privileged
execution also needs a terminal cleanup decision independent of protocol
results.

## Decision

Plan 043 uses one ordered, fail-closed lane:

```text
contract
  -> reference-build
  -> reference-offline-reuse
  -> environment-smoke
  -> reference-crosscheck-ipv4
  -> i2pr-handshake-smoke-ipv4
  -> full-matrix
  -> evidence-validation
  -> cleanup-verification
```

Preparation is a network-enabled trust domain. It is limited to the exact
Ubuntu package contract, locked source revisions, verified external artifacts,
and declared build dependencies. Execution is a separate offline trust domain:
it consumes only verified caches, uses disposable namespace-local veth links,
and has no default route, DNS, reseed/bootstrap, or public egress.

Every gate consumes explicit, hashed artifacts. Cache reuse requires the
canonical reference identifier, full source revision, lock digest, host
contract, build-command version, and recorded tool/ABI inputs to match. The
complete runtime tree is re-hashed before execution. A missing or mismatched
cache is a hard failure and never permits a fetch.

Evidence is an allowlisted, typed, sanitized product. The aggregate manifest
records the expected scenarios, actual record filenames and SHA-256 digests,
gate dispositions, host/lock/cache digests, workflow run metadata, and cleanup
verification. Raw logs, paths, endpoints, identities, keys, RouterInfo, I2NP,
packet captures, and mutable run state are not retained or uploaded.

Cleanup runs after every privileged phase and at the end regardless of earlier
results. An independent clean-host verifier must reject residual interop
namespaces, veths, processes, secret-bearing run roots, forbidden retained
files, or attributable global nftables/routes/forwarding changes. Cleanup or
verification failure makes the lane fail even when protocol scenarios passed.

Promotion is staged: manual dispatch first, scheduled control only after
repeated clean-checkout and cache-reuse success, a current successful run at
Milestone 3 closure, and only then a separate decision about any reduced
trusted-pull-request lane. Privileged execution is never automatically exposed
to forked or untrusted pull-request code.

## Consequences

- Ordinary CI remains unprivileged and continues to run static, unit,
  deterministic, manifest, and boundary checks.
- The Ubuntu lane is manual and opt-in until stability, cost, and cleanup
  recovery are demonstrated.
- Environment smoke and the Java-I2P/i2pd reference crosscheck are harness
  controls, not i2pr evidence.
- Four independent authenticated i2pr/reference IPv4 directions, bounded I2NP
  exchange, adversarial coverage, valid sanitized records, and clean-host
  verification are required before NTCP2 support or advertisement changes.
- The current checkout has not satisfied these gates; no NTCP2 claim follows
  from this ADR or from workflow scaffolding.

## Rejected alternatives

- A single privileged job with an undifferentiated pass/fail result hides which
  trust-domain or evidence gate failed.
- Reusing mutable caches or fetching on an offline miss makes the result
  non-reproducible and weakens the execution boundary.
- Treating cleanup as best effort permits a protocol result to coexist with
  leaked namespaces, processes, keys, or routes.
- Running privileged interop on arbitrary fork pull requests creates an
  inappropriate trust path from unreviewed code to the host.
