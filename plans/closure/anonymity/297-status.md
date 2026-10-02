# Plan 297 closure — HTTP anonymity-profile convergence

Status: stopped-http-differential-capture-needs-controlled-three-router-topology

Implementation commits: none. The Plan 296 HTTP neutralization remains intact. No Plan 297 production profile or differential claim was introduced.

## Requirement disposition

| Requirement | Result |
| --- | --- |
| Exact Java/i2pd source authority | Java I2P 2.13.0 source verified at `9134f808337b401e8e53c73734c81fab04280c9d`; i2pd 2.61.0 source verified at `635b013a612ff47278ef02acf8580a28e10e26c5`. Source review confirms both implementations contain `MYOB/6.66 (AN/ON)` defaults and resolved `.b32.i2p` Host handling. |
| Build feasibility | i2pd built successfully from the exact pinned checkout on macOS. Java I2P 2.13.0 `ant updater preppkg` completed sufficiently to stage `pkg-temp` artifacts, but the repository fetch helper stopped during metadata generation because macOS `find` does not support GNU `-printf`. Staged files remained under ignored `target/interop/cache`. |
| Controlled black-box fixture | Not executed. The existing Java `ControlledRouter` test launcher deliberately starts only the SAM bridge; it has no HTTP client proxy listener. No exact-pinned i2pd client proxy plus independently controlled remote HTTP Destination fixture was provisioned in this checkout. No Java/i2pd/i2pr sanitized capture matrix exists. |
| Named profile, disposition matrix, convergence, differential checker | Not implemented or claimed. Selecting defaults without executed cross-family captures would violate the plan's reference-first requirement. |
| CONNECT opacity and bounds | Existing behavior and Plan 296 boundary changes remain; no TLS inspection or parser-limit change was made. |

## Commands and outcomes

- Exact source review via `git grep` at both recorded commits: passed for the audited User-Agent and Host behavior.
- `make -C /tmp/i2pd-plan296 -j2 USE_UPNP=no DEBUG=0 HOMEBREW=1`: passed; built exact i2pd source without patching.
- `I2PR_M6_JAVA_SRC=/tmp/java-plan296 bash scripts/interop/fetch-m6-java.sh`: stopped after staging Java artifacts with `find: -printf: unknown primary or operator` during cache metadata generation.
- `bash scripts/check-service-anonymity-boundaries.sh`: passed; this verifies retained Plan 296 boundaries only, not Plan 297 equivalence.
- Plan 297 focused/workspace test floor and new HTTP capture runner: not run/not present; the required reference capture topology was not available.

## Limitations and findings

Critical: none. High: none. Medium: HTTP differential behavior and identifying-header disposition remain unqualified. Low: the Java reference fetch helper's metadata generation is GNU-find-specific and stops on macOS after building/staging artifacts.

No HTTP anonymity-compatible profile, Java/i2pd equivalence, or browser/TLS claim is authorized. The default remains Plan 296's neutral User-Agent and resolved-Destination Host behavior with its existing bounded rewrite semantics.

## Unblock audit and roadmap disposition

Plan 297 is stopped rather than passed because its required three-family executed captures do not exist. Plan 301 therefore remains blocked on a successful Plan 297 closure. Plans 298 and 300 are independently ready: Plan 296 is closed; Plan 298's local M6 and Plan 193 prerequisites are closed; Plan 300's Destination, tunnel, NetDB, and M10 authorities remain closed. Proceed with Plan 298 next under the user's sequential instruction, then take Plan 300 if Plan 299 cannot be executed from qualified Plan 298 evidence. No other registered plan lists Plan 297 as a dependency.
