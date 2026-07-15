# Plan 036 controlled NTCP2 interoperability lane

This is a manual, opt-in integration path. It is intentionally separate from
the normal workspace tests because malformed, slow, stress, and fault-injected
traffic is permitted only in an authorized isolated testnet.

The pinned reference targets are Java I2P 2.12.0 at source revision
`2800040` and i2pd 2.60.0 at source revision `f618e41`. The release pages and
source revisions are recorded in `manifest.toml`; each built binary or image
must also be recorded by SHA-256 in the per-run evidence record. These values
are pins, not claims that the versions were executed here.

The environment must use the synthetic network ID in the manifest, disposable
identities and NTCP2 static keys, loopback/private namespaces only, disabled
reseed/bootstrap, fixed clocks, explicit timeouts, and teardown that removes
all secret-bearing artifacts. Public I2P addresses, operational identities,
peer lists, and traffic captures are prohibited.

Run the repository-side preflight from the workspace root:

```text
bash scripts/check-ntcp2-interoperability.sh
```

The preflight validates the pinned manifest and scans the committed evidence
directory for secret/capture artifacts. It does not start a router or claim
interoperability. An authorized external runner must supply the complete
wire-level `i2pr` adapter, reference binaries/images, exact artifact hashes,
configuration hashes, isolated namespace, and sanitized result writer before
executing the matrix.

The required matrix includes both directions for Java I2P and i2pd, IPv4 and
available IPv6, padding boundaries, skew/replay/identity/network failures,
authenticated I2NP exchange, partial/coalesced I/O, duplicate-link races,
slow/oversized/mutated inputs, queue/resource saturation, and cleanup. The
full scenario list is in `manifest.toml`; the evidence format is in
`evidence/README.md`.

Local substitutes are deliberately labeled separately: `cargo test -p
i2pr-testkit --all-targets` runs the fixed-seed 0..255 simulation matrix, and
the NTCP2 unit/fuzz lanes exercise pure parsers and state owners. They do not
replace mixed-router evidence.
