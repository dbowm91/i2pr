# Protocol support matrix

This matrix is intentionally explicit: every row describes the exact evidence
available, not just code presence. “Experimental structural subset” means
bounded codecs exist and are tested locally, but no mixed-router interoperability
or capability claim exists.

The fine-grained, machine-readable inventory for Milestone 1 is
[`specs/support.toml`](../specs/support.toml). Structural entries may be marked
`experimental` with repository evidence, but remain `advertised = false`; the
ledger does not itself publish protocol capabilities.

| Protocol area | Status | Planned milestone | Specification/source starting point | Test-vector status | Interoperability status |
| --- | --- | --- | --- | --- | --- |
| Common identity, keys, and certificates | Experimental structural subset | 1 | `specs/protocols/01-common-identity-crypto.md`, pinned source in `specs/SOURCES.md` | Locally authored fixed bytes and malformed/boundary tests; no independent router vectors | None |
| I2NP message envelope and message types | Not implemented | 1 | `specs/protocols/02-i2np.md` | None imported | None |
| NTCP2 | Not implemented | 3 | `specs/protocols/03-ntcp2.md` | None imported | None |
| Reseed and RouterInfo publication | Not implemented | 4 | `specs/protocols/04-reseed-netdb.md` | None imported | None |
| Network tunnels and transit participation | Not implemented | 5 | `specs/protocols/05-tunnels.md` | None imported | None |
| Garlic, ECIES, and LeaseSets | Classic LeaseSet structural subset only; LeaseSet2-family deferred | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | Local Lease/LeaseSet vectors and negative tests; no independent router vectors | None |
| I2P streaming | Not implemented | 7 | `specs/protocols/07-streaming.md` | None imported | None |
| SAM | Not implemented | 8 | `specs/protocols/08-sam.md` | None imported | None |
| SSU2 | Not implemented | 9 | `specs/protocols/09-ssu2.md` | None imported | None |
| I2CP and service tunnels | Not implemented | 10 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |

The four-crate bootstrap may name the `common` and `i2np` namespaces, but only
the exact structural common subset described above has codecs. No signature or
encryption operation, transport support, network compatibility, or capability
advertisement follows from those codecs. Legacy NTCP and SSU1 are outside the
MVP target unless a later plan explicitly changes scope.

Each future protocol row must be updated with exact targeted proposal/spec
revisions, limits, malformed-input behavior, vectors, and mixed-router evidence
before its status changes.
