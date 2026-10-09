# Plan 405 closure — blocked on standard authority stream delivery

Status: **blocked-standard-authority-lookup-succeeds-but-payload-does-not-plan-406**.

Plan: `plans/implementation/i2pcontrol-proposal-170/405-final-i2pd-els2-reverse-and-authority-qualification.md`.

## Requirement-to-evidence

| Requirement | Evidence | Result |
|---|---|---|
| Final fresh NONE invocation | First attempt was invalid because the stock reference control process exited and the i2pr driver could not start. Retained at `target/interop/els2-evidence-plan405-none-final-20261009`. | Failed control; not counted |
| Final separate NONE attempt | Run `target/interop/els2-evidence-plan405-none-final-20261009b`: reference ELS2 and standard self-consumer controls passed; i2pr ordinary lookup reported one success and encrypted target resolved, but the application payload did not return. Counters had `remote_stream_connect_started=0`, `remote_stream_established=0`, `failed_connects=0`, activation pending false, and no activation failure. A stock i2pd process reported a segmentation fault during teardown. Hashes: evidence `fb085abe35f1fccbf194091805b129c8d8b4f5952011e72760fe9e20b1facc54`; results `0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`; driver `4cf702f53dd94d759a71c213f13f1a1ddc9f001474832b311d0e0577c98a4d2b`. | Blocked; lookup success is not payload success |
| Prior passing mode evidence | NONE passed in Plan 400; PSK passed in Plan 403; DH passed in Plan 404. Those artifacts remain immutable and available. | Prior scoped evidence passes; Plan 405 fresh NONE remains unresolved |

## Commands and outcomes

- Global plan-number uniqueness — pass before Plan 405 run.
- Final NONE lane, first invocation — failed reference control; retained.
- Final NONE lane, second invocation — controls passed, authority payload row failed after lookup; retained.
- No full routine floor: Plan 405's mandatory fresh NONE authority row did not pass.

## Security, compatibility, disposition

No production code changed in Plan 405. Both attempts used stock i2pd 2.61.0,
the final explicit type-7 requester, separate artifacts, and one runner attempt
per invocation. No capability or support claim is made. The evidence does not
yet prove whether the standard service stream was accepted locally or lost at a
later transport boundary. Plan 406 owns bounded connection-stage attribution,
reference-process health evidence, final matrix execution, and the floor after
the authority row passes.

## Unblock audit

Plan 406 is registered as the corrective successor. Plans 374/384 remain blocked
on this authority row; Plans 375/377/378 retain their independent Java and
convergence dependencies. No future plan is unblocked by this failure.
