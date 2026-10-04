# `i2pr-su3` — bounded SU3 envelope verification

`i2pr-su3` is a runtime-neutral library for the common SU3 container
boundary. It validates complete framing under caller-supplied byte
ceilings, parses an operator-pinned DER X.509 certificate into RSA
verification material, and verifies the currently implemented
RSA-SHA512 signature type against an explicit caller-provided key and
verification time. It has no file, network, clock, or ambient
trust-store access.

Content policy remains with the caller. Reseed ZIP entry limits and
RouterInfo validation stay in `i2pr-netdb`; the Proposal 170 NEWS owner
must apply NEWS file/content allowlists, signer configuration, XML/gzip
limits, and cache policy separately after envelope verification.

The crate currently supports signature type 6 (RSA-SHA512). Other SU3
signature types fail closed. Certificate parsing and trust-anchor
construction are caller responsibilities; this crate accepts already
parsed RSA modulus/exponent and caller-validated dates.

## Verification evidence

- Unit tests cover generic file/content metadata, exact signed/content
  slices, configured content ceilings, and trailing-byte rejection.
- `i2pr-netdb` reseed verification delegates RSA-SHA512 signature math to
  this crate while retaining its reseed-specific trust and archive policy.
- Dependency direction is checked by
  `scripts/check-dependency-direction.sh`.
