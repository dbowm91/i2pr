# Plan 431 status: stopped at the non-loopback qualification topology gate

Closure token: `stopped-ssu2-independent-non-loopback-reference-topology-unavailable`

Plan: `plans/implementation/core-router-recovery/431-public-capable-ssu2-runtime-and-routerinfo.md`

## Source baseline and scope

The read-only source census was performed on `9a7e667619ff89d5705936dd3a9bcde3069f9ab3` (the Plan 430 closure head). No production changes or capability claims were made for Plan 431.

The implementation still has the protections this plan is intended to evolve: `crates/i2pr-daemon/src/config.rs::parse_ssu2_bind` rejects non-loopback binds, `normalize_ssu2` rejects advertisement and introducer service, and `crates/i2pr-runtime/src/ssu2_runtime.rs` owns the UDP bind lifecycle. Existing Plan 161 evidence is explicitly loopback-only. This is a real source limitation, not evidence of non-loopback capability.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Preserve strict default-off and controlled-loopback behavior | Preserved; no files or configuration were changed. |
| Establish independently addressed non-loopback SSU2 sessions and I2NP in both directions | Not run. No qualified independent private-address peers or prepared isolated topology were supplied. The routine-work rules prohibit creating a namespace, container, VM, or public-I2P test topology. Loopback evidence cannot satisfy this requirement. |
| Persistent key identity, bound RouterInfo, and false-publication controls | Not implemented or claimed. Their migration and lifecycle behavior must be qualified together with the required non-loopback owner evidence. |
| Two-family evidence and advertisement permissions | Not run; no SSU2 advertisement permission is added. |
| Plan 433 readiness | Still blocked on Plan 431 and Plan 432. |

## Commands and outcomes

This stop was based on source and plan inspection only. The following were read; no test or external integration command was run because none can supply the missing topology:

- `plans/implementation/core-router-recovery/431-public-capable-ssu2-runtime-and-routerinfo.md`
- `crates/i2pr-daemon/src/config.rs` (`parse_ssu2_bind`, `normalize_ssu2`)
- `crates/i2pr-runtime/src/ssu2_runtime.rs` (SSU2 socket owner)
- `plans/closure/ssu2/161-status.md` (loopback-only historical evidence)
- `AGENTS.md` routine-work restrictions on namespaces, containers, VMs, and public-I2P testing

No hosted CI or external reference result is claimed.

## Security, compatibility, findings, and disposition

- Existing non-loopback rejection, default-off behavior, and non-advertisement remain unchanged.
- No key format, migration, address publication, or runtime ownership changed; there is no migration evidence to report.
- **High — qualification prerequisite unavailable.** Enabling the public-capable path without independent non-loopback sessions, persistent-identity restart proof, and truthful RouterInfo evidence would bypass the plan's explicit acceptance gate. Do not relax the gate or infer reachability from bind/configuration state.
- The narrowly bounded continuation is the already registered Plan 431 work itself, resumed only when an authorized isolated topology supplies independently addressed i2pd peers and permits restart/fault testing. No production corrective plan is warranted before that prerequisite exists.
- Plan 431 is stopped, not passed. Plan 433 remains blocked; Plans 432 and 434 are independent and may proceed.

Roadmap disposition: keep SSU2 default-off for external exposure, loopback-only, and non-advertised. No `specs/support.toml` or conformance promotion.
