# Plan 183 — M6 mixed-router destination/Streaming interop program (registration)

Status: **registered; not yet scoped for execution**.

## 1. Why this plan exists

Plan 181 §6.1/§6.3 requires, for Milestone 10 closure, ordinary
destination/Streaming traffic between an i2pr M10 client service
and a service hosted by an independently implemented I2P router
under a controlled environment. The mandatory qualification
attempt (exact-pinned i2pd 2.61.0, SAM `DEST GENERATE` public
destination, one M10 generic-client connect; see
`plans/181-status.md` and
`crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs`)
demonstrates the retained gap with command/log provenance:

```text
classification = m6-mixed-router-streaming-blocker
mechanism      = valid independent peer destination parses,
                 zero routes (unknown_peer), bounded timeout,
                 nothing delivered, nothing established
```

i2pr has no tunnel/netdb/transport path from a local
destination to a non-local destination by design of the local
product (Plan 149 fabric + Plan 182 driver route only to
co-owned bridges). Closing that gap is Milestone 6
mixed-router work, explicitly out of scope for Plans 174–182
and for Plan 181 itself.

## 2. Program entry criteria

No 183-series execution plan may start until it states, with
command-derived evidence:

1. which independent implementations are in scope (at least
   two router families per `specs/CONFORMANCE.md` §7; Java I2P
   or I2P+ plus i2pd preferred);
2. which transport carries the first interop (NTCP2 stays
   non-advertised and disabled; SSU2 v2 has the Plan 161
   loopback session precedent);
3. the minimal destination/Streaming path the M10 HTTP/IRC rows
   need (tunnel build, NetDB lookup/publication, Streaming
   handshake/data over the chosen transport);
4. the controlled environment contract (unprivileged,
   loopback-or-private-testnet, no public-network dependence
   for green results).

## 3. Non-goals for the program registration

- No implementation in this plan (no `crates/` changes).
- No M10 closure claim (Milestone 10 stays open until Plan 181
  §6 rows pass against the program's output).
- No reopening of the NTCP2 historical harness unless the
  scoped execution proves transport is the blocker (Plan 181
  §6.3).
- No weakening of the Plan 181 remote-row criterion and no
  substitution of self-composed rows for mixed-router rows.

## 4. Handoff

```text
plan_183 = registered-m6-mixed-router-streaming-interop-program
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 183 (scoping first)
```

Plan 181 resumes only after the program it registers produces
passing remote rows.
