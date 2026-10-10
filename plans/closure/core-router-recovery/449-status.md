# Plan 449 status: portable preflight infrastructure passed; live LAN gate not run

Closure token: `passed-portable-preflight-infrastructure-only`

Plan: [`449-portable-ssu2-independent-lan-qualification-lane.md`](../../implementation/core-router-recovery/449-portable-ssu2-independent-lan-qualification-lane.md)

## Scope and outcome

The infrastructure portion is complete: Plan 449 now has a strict, owner-consent
inventory validator; a local, no-packet interface/address/UDP-bind/route-source
preflight; pinned reference source and binary checks; deterministic positive and
negative self-tests; and an operator checklist. Host-specific inventory values
must remain in an owner-only file outside the repository. The preflight never
starts i2pr or i2pd, sends a packet, or declares remote reachability.

The authorized two-host qualification was **not run**. No owner-approved host
pair or invocation was supplied, and no addresses or remote endpoint were
contacted. Plan 449 is closed only at this explicitly bounded infrastructure
scope. Plan 431's non-loopback qualification remains stopped, and Plan 433 does
not become ready from preflight infrastructure.

## Implementation

- Added `scripts/plan449-ssu2-preflight.py`. It rejects an invalid schema,
  missing consent, changed i2pd pin, non-RFC1918 IPv4, mismatched subnet,
  duplicate endpoint/host identity, invalid port or budget, and non-distinct
  host identity before probing local state. Real mode reports only sanitized
  reason codes and a binary digest; it does not print addresses, interface
  names, router identities, or data paths.
- Added `tests/integration/ssu2/plan449-operator-preflight.md` with the
  owner-authorized, unprivileged operator procedure and transient inventory
  format.
- Added the deterministic self-test to the routine floor and Linux/macOS
  quality workflow.
- Implementation commit: `deb1060` (`feat(ssu2): add plan 449 offline preflight`).

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Strict two-role inventory, exact reference pin, owner consent, finite budget | Pass; covered by positive and negative self-test cases. |
| Private IPv4 pair, shared subnet, distinct host IDs/endpoints and data roots | Pass at inventory-validation scope; no host addresses are persisted. |
| Local interface/address, port bind and route-source preflight | Implemented; not run against an external host pair. The connected UDP socket is used only for local route selection and sends no datagram. |
| Exact pinned i2pd source and binary identity | Pass locally: source revision `635b013a612ff47278ef02acf8580a28e10e26c5`, tracked tree clean, executable version 2.61.0, SHA-256 `107ad4cc0bd5263205d66bcf7322b6d52457cb2b15d1cccf11ad11d302388ccf`. |
| Sanitized output and transient inventory outside repository | Pass by code review and strict path/permission checks; no real inventory was created or committed. |
| Authenticated independently addressed SSU2 in both directions, I2NP delivery, restart/withdrawal and negative controls | Not run; requires a separately authorized two-host private LAN and explicit live qualification. |
| No public `R`/`f`/introducer or support promotion | Preserved; no production, RouterInfo, or support inventory change. |

## Commands and outcomes

Executed locally:

- `python3 scripts/plan449-ssu2-preflight.py --self-test` — passed (8 deterministic positive/negative cases; no process or socket).
- `python3 -m py_compile scripts/plan449-ssu2-preflight.py` — passed.
- `python3 -c 'import runpy; from pathlib import Path; ns=runpy.run_path("scripts/plan449-ssu2-preflight.py"); print(ns["verify_reference"](Path("target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5"), Path("target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/bin/i2pd")))'` — passed; exact pin, clean tracked tree, 2.61.0 version, and binary hash above.
- `python3 scripts/check-tooling-inventory.py` — passed; every published inventory figure matches the tree.
- `python3 scripts/check-workflow-validity.py` — passed; all 11 workflow files parse and are structurally valid.
- `git diff --check` — passed before closure.
- The two-host invocation was not run. No hosted CI result is claimed.

## Security, compatibility, and findings

- **Medium — live topology unavailable.** The preflight does not prove remote
  UDP reachability or protocol interoperability. Keep Plan 431 stopped until
  the separately authorized live run yields authenticated bidirectional
  sessions and I2NP evidence.
- The local preflight validates only the selected host's interface, configured
  address, local port bind, route source, and data-directory permissions. It
  does not inspect or alter the peer host.
- Inventory values are never printed and the sample uses placeholders. No
  key, payload, raw log, or host identity was written to the repository.
- No dependency was added. Public support, reachability, transit, and floodfill
  claims are unchanged.

## Unblock audit and disposition

Plan 449's portable infrastructure scope is passed. Its live qualification
column remains environment-blocked and must be recorded separately if the
owner authorizes and supplies the private two-host pair. Plan 431 remains
stopped; Plan 442 remains blocked on that proof; Plan 433 remains gated on 431
and its own multihop product evidence. No plan is automatically unblocked by
this infrastructure closure. Plans 446–448 are independent.

The result is **not** public SSU2 qualification, a two-family conformance result,
or evidence for normal public router participation.
