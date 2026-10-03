# Plan 308 status — HTTP proxy differential qualification

Status: `blocked-controlled-http-reference-topology-not-implemented`

## Plan authority and implementation lineage

Plan 308 was registered by `4b43675a302693c62f43f0c305d08cf29d4f15e0` and became active after Plan 307 passed. It depends on Plan 307, the retained Plan 304 pins/artifacts, and ADR 0030. The Plan 307 dependency and pinned-artifact preflight are satisfied.

Implementation commit: `afa0b99` (`test(anonymity): add fail-closed HTTP capture evidence tools`). It adds a bounded canonicalizer and a fail-closed evidence checker with regression tests. No production HTTP profile was changed because no family captures were obtained.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Exact references and Ubuntu preflight | `bash scripts/interop/anonymity/preflight-ubuntu.sh` passed on Ubuntu 24.04 x86_64. Retained manifest confirms i2pd `635b013a612ff47278ef02acf8580a28e10e26c5` and Java I2P `9134f808337b401e8e53c73734c81fab04280c9d` artifacts. | Passed prerequisite only |
| Bounded canonical capture representation | `tests/integration/anonymity/canonicalize_http_capture.py` retains ordered headers, method/target shape, body length and digest, and whole-request digest; rewrites I2P authorities to a fixed marker and rejects malformed or oversized input. | Implemented; unit-tested |
| Capture evidence checker | `scripts/check-http-anonymity-evidence.sh` requires all three family captures, exact reference pins, matching source-head and corpus hashes, complete corpus rows, and cleanup facts. | Implemented; seeded missing-family and pin-mismatch tests passed |
| Controlled ordinary HTTP Destination/readiness | A bounded i2pd-only probe with a fresh datadir and no peer bootstrap returned the reference's `Host is down` response. No controlled peer topology or ready server Destination was available in that probe. | Not demonstrated; topology work remains |
| i2pr, i2pd, and Java HTTP captures | No family capture was produced. `target/interop/anonymity/http-profile-evidence` is absent; the checker rejects missing evidence. | Blocked |
| Differential matrix and selected profile | No captures exist to support a comparison or production change. | Not attempted; fail-closed |
| Repeated post-change captures and full Plan 307 floor | No production profile was changed. | Not run |

## Commands and outcomes

- `bash scripts/interop/anonymity/preflight-ubuntu.sh` — passed locally; host and exact reference facts recorded under ignored `target/interop/anonymity/`.
- `python3 -m unittest discover -s tests/integration/anonymity -p 'test_http_capture.py'` — passed, 6 tests.
- `bash scripts/check-http-anonymity-evidence.sh target/interop/anonymity/http-profile-evidence` — failed closed with `manifest-missing`, as expected without family evidence.
- `bash -n scripts/check-http-anonymity-evidence.sh` — passed.
- A disposable pinned-i2pd HTTP proxy/server probe returned `500 Host is down`; its raw log, temporary destination key, request data, and datadir were removed with the temporary directory. This is evidence that the no-bootstrap one-router setup is insufficient, not evidence that a properly bootstrapped controlled topology is impossible.
- `git diff --check` — passed before commit.
- Production profile tests and full workspace floor — not run; there were no production changes or qualified captures.

## Compatibility, security, and limitations

No runtime, HTTP profile, listener, dependency, or persistent format changed. The capture tooling is bounded and stores only sanitized request structure, header facts, lengths, and digests; raw capture bytes are expected to remain in a caller-owned private scratch directory. The new checker is deliberately fail-closed and does not turn absent captures into a pass.

| Severity | Finding |
|---|---|
| Critical | None. |
| High | None. |
| Medium | The three-family controlled ordinary HTTP topology and captures remain absent. No family convergence or profile-equivalence claim is supported. |
| Low | None. |

## Dependency audit and disposition

- **Plan 309:** remains independently eligible and active. ADR 0030 and Plan 307 satisfy its dependencies; Plan 308 is not a dependency.
- **Plan 310:** remains blocked on Plan 309.
- **Plans 311–313:** remain blocked on their registered Plan 310/312 dependencies.
- Plans 297–305 remain historical stopped records. No anonymity, HTTP-equivalence, or production-readiness claim changes.

Disposition: blocked pending implementation of an unmodified-reference controlled peer topology, then all three ordinary HTTP captures and the evidence-driven differential. Resume Plan 308 when that topology is available; the capture normalizer and checker are reusable foundations, not qualification evidence.
