# Protocol support matrix

This matrix is intentionally explicit: every row describes planned work, not
current interoperability. “Not implemented” means the repository provides no
usable implementation for that area.

The fine-grained, machine-readable inventory for Milestone 1 is
[`specs/support.toml`](../specs/support.toml). Its initial entries are
`not-implemented`, carry no evidence, and set `advertised = false`; the ledger
does not itself publish protocol capabilities.

| Protocol area | Status | Planned milestone | Specification/source starting point | Test-vector status | Interoperability status |
| --- | --- | --- | --- | --- | --- |
| Common identity, keys, and certificates | Not implemented | 1 | `specs/protocols/01-common-identity-crypto.md` | None imported | None |
| I2NP message envelope and message types | Not implemented | 1 | `specs/protocols/02-i2np.md` | None imported | None |
| NTCP2 | Not implemented | 3 | `specs/protocols/03-ntcp2.md` | None imported | None |
| Reseed and RouterInfo publication | Not implemented | 4 | `specs/protocols/04-reseed-netdb.md` | None imported | None |
| Network tunnels and transit participation | Not implemented | 5 | `specs/protocols/05-tunnels.md` | None imported | None |
| Garlic, ECIES, and LeaseSets | Not implemented | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | None imported | None |
| I2P streaming | Not implemented | 7 | `specs/protocols/07-streaming.md` | None imported | None |
| SAM | Not implemented | 8 | `specs/protocols/08-sam.md` | None imported | None |
| SSU2 | Not implemented | 9 | `specs/protocols/09-ssu2.md` | None imported | None |
| I2CP and service tunnels | Not implemented | 10 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |

The four-crate bootstrap may name the `common` and `i2np` namespaces, but that
does not constitute parsing, serialization, transport support, or network
compatibility. Legacy NTCP and SSU1 are outside the MVP target unless a later
plan explicitly changes scope.

Each future protocol row must be updated with exact targeted proposal/spec
revisions, limits, malformed-input behavior, vectors, and mixed-router evidence
before its status changes.
