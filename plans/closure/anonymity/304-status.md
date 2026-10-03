# Plan 304 closure — Ubuntu controlled reference topology and capture foundation

Status: stopped-hostile-reference-packet-boundary-unavailable

Implementation commit: `e01db5b5c09aebf9654c07caa3b50913272d419c` (preflight, source
artifact metadata, topology contract, and synthetic HTTP corpus). This closes the bounded
attempt with retained foundation evidence. It does not pass the controlled topology or
authorize Plans 297/298 successors.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Ubuntu preflight and host/tool record | Passed locally on Ubuntu 24.04.5 LTS, x86_64, kernel `6.8.0-142-generic`; sanitized record: `tests/integration/anonymity/evidence/ubuntu-host-manifest.json`. Preflight fails before capture on non-Linux/non-Ubuntu hosts, missing tools, invalid pins, insufficient free disk, low descriptor ceiling, or a tracked dirty tree. |
| Exact i2pd and Java artifacts | Passed locally. i2pd `2.61.0` source `635b013a612ff47278ef02acf8580a28e10e26c5`, binary SHA-256 `107ad4cc0bd5263205d66bcf7322b6d52457cb2b15d1cccf11ad11d302388ccf`. Java I2P `2.13.0` source `9134f808337b401e8e53c73734c81fab04280c9d`, `router.jar` SHA-256 `f79d44472c5ddbefb87f745a0c19a811e4915f20f9c5a119195b337dd4171ef4`, staged tree SHA-256 `3699b7b35cdcc9aa09ce26743fe69b04ec2ceec5c0aac6a5f9f3a2ca80e9e77e`. See `tests/integration/anonymity/evidence/reference-manifest.json`. Neither reference source was patched. |
| Machine-readable shared topology and HTTP corpus | Foundation added at `tests/integration/anonymity/topology.toml` and `http-corpus.toml`; no runtime topology was launched. |
| Three-family controlled fixture reachability | Not demonstrated. No fresh i2pr/i2pd/Java family topology, controlled fixture RouterInfo/bootstrap, readiness contract, or family-specific runner was implemented. |
| HTTP smoke captures | Not produced for any family. |
| Hostile-Destination Streaming adapter and smoke traces | Not implemented or executed. Stock Java and i2pd application-facing Streaming APIs terminate at their own server Streaming implementation; this lane has no unpatched reference API that lets the external harness schedule packet-level ACK/loss responses at the required pre-manager Destination boundary. A real adapter for those families therefore needs a newly designed raw-delivery/test interface; an ordinary server socket would not satisfy the plan. |
| Sanitized evidence checker and seeded-negative proof | The existing `scripts/check-streaming-fingerprint-evidence.sh` passed its local contract/negative checks. The new family smoke evidence checker, seeded pin-mismatch case, and six-row smoke evidence set do not exist. |
| Owned-process cleanup proof | No family processes were launched, so no topology cleanup claim is made. |
| Plan 296 boundary regression and production scope | `scripts/check-service-anonymity-boundaries.sh` passed. No production crate or profile was changed. |
| Routine workspace/security floor | Not run; acceptance stopped at the missing raw hostile-reference destination boundary. No workspace-wide green claim is made. |

## Commands and outcomes

- `bash scripts/interop/anonymity/preflight-ubuntu.sh` — passed locally; host facts recorded.
- `bash scripts/interop/fetch-ssu2-reference.sh --rebuild --with-java` — passed locally; built exact-pinned i2pd and verified Java source. The script explicitly deferred Java runtime staging.
- `I2PR_M6_JAVA_SRC=<exact-pinned checkout> bash scripts/interop/fetch-m6-java.sh --rebuild` — passed locally; staged the exact-pinned Java headless router artifacts.
- `python3 scripts/interop/anonymity/record-reference-manifest.py "$PWD" target/interop/anonymity` — passed locally; artifact hashes and relative cache paths recorded.
- `bash -n scripts/interop/anonymity/preflight-ubuntu.sh` — passed.
- Python `compile()` of `record-reference-manifest.py` — passed.
- `bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `bash scripts/check-streaming-fingerprint-evidence.sh` — passed; validates Plan 298's retained schema/fixtures, not family captures.
- `cargo fmt/check/test/clippy/doc`, topology smoke runners, and the full Plan 304 evidence checker — not run/not available because controlled family runners and a hostile raw Destination interface were not implemented.

## Compatibility, security, and findings

No production behavior, HTTP/Streaming policy, listener, persistent schema, or dependency
changed. Captured evidence contains host/tool versions, exact public source revisions, hashes,
and relative cache paths only. Raw reference logs, identities, private keys, payloads, and
process command lines were not copied to the repository.

| Severity | Finding |
| --- | --- |
| Critical | None. |
| High | None. |
| Medium | Required three-family topology and hostile-Destination capture remain unavailable; Plans 297/298 cannot resume from this evidence. |
| Low | None. |

## Unblock audit and roadmap disposition

- Plan 297 remains stopped; its fresh HTTP corrective successor is not ready because no family HTTP captures exist.
- Plan 298 remains stopped; its fresh hostile-Destination corrective successor is not ready because no reference packet-control adapter or captures exist.
- Plan 299 remains stopped on missing Plan 298 differential evidence.
- Plan 301 remains stopped on missing successful HTTP, Streaming, and Plan 300 successor qualifications.
- Plan 305 is independent and remains ready; it is the next eligible plan in the requested sequence.
- No downstream qualification status changed and no anonymity or reference-equivalence claim is authorized.

Disposition: retained-stopped with the exact hostile-reference packet-boundary gap above.
