# Plan 042 status: runtime-owned NTCP2 wire driver

## Status

Plan 042 remains open. This is a status record, not a closure record: the
current checkout has the bounded runtime-owned handshake action executor,
listener/dial promotion, authenticated frame/link owner, and local
handshake-to-link/I2NP smoke path. No authorized i2pr-to-reference execution
has been run.

The existing `i2pr-runtime` surface supplies bounded TCP lifecycle, admission,
replay, backoff, deadline, queue, supervised link-child primitives, and the
new action executor for exact I/O, cancellation, clock, replay, padding, and
RouterInfo actions. The launcher now owns the disposable state preparation and
invokes listener/dial policy, the handshake-to-link handoff, and the I2NP smoke
exchange. That end-to-end composition remains in `i2pr-runtime`;
`i2pr-transport-ntcp2` remains free of sockets,
Tokio, filesystem access, and runtime policy.

The action executor keeps protocol boundaries explicit: exact reads are
deadline- and cancellation-aware, I/O failures stay in fixed categories, and
an ambiguous unframed `ReadBounded` action is rejected as a typed invalid
action rather than being inferred from an arbitrary TCP read.

## Launcher and evidence boundary

`tools/i2pr-interop` is a non-production composition seam, not the normal
daemon. The completed Plan 042 status protocol is expected to provide:

- `listen`: a flushed listener-readiness record followed by a separate
  authenticated terminal result;
- `dial`: one terminal typed result; and
- `inspect`: bounded, redacted state metadata only.

In the current checkout, after bounded scenario-file and disposable-state
validation, `listen` emits readiness followed by a terminal result and `dial`
emits one terminal result. A local success has the form:

```json
{"schema":1,"type":"i2pr-interop-status","scenario_id":"<scenario_id>","phase":"terminal","result":"passed","reason_code":"i2np_exchange_complete","counters":{"listener_ready":1,"authenticated":1,"frames_sent":1,"frames_received":1,"i2np_sent":1,"i2np_received":1}}
```

This is local driver validation, not mixed-router evidence. Typed state,
authentication, data-phase, timeout, cleanup, and unsupported-profile results
remain terminal failures. The current `inspect` operation only validates disposable RouterInfo structure,
signature, and NTCP2 address presence; it does not establish a session.

Retained records may contain only sanitized typed outcomes, bounded counters,
and approved artifact/configuration/topology hashes under
`target/interop/evidence/`. Secret-bearing run roots under
`target/interop/runs/<run-id>/` must be deleted. Plan 041's Java I2P/i2pd
reference-pair records are harness-control evidence and do not count as i2pr
mixed-router evidence.

## Selected smoke-message scope

The initial Plan 042 data smoke is the existing fixed-size DeliveryStatus I2NP
message, type 10. Its body is 12 bytes. The NTCP2/SSU2 short transport encoding
is 21 bytes (9-byte short header plus body), and the NTCP2 I2NP block is 24 bytes
before encrypted-frame overhead and padding.

The intended positive gate is one valid outbound and one valid inbound
DeliveryStatus per direction, in addition to a complete authenticated
handshake and orderly cleanup. The message is a bounded status exchange only;
it does not authorize NetDB publication, tunnel construction, garlic/client
behavior, or public routing. Reference acceptance or echo behavior has not been
verified, so this selection is scope—not interoperability evidence. Padding,
TCP connection, listener readiness, loopback, vectors, and testkit results do
not satisfy the smoke gate.

## Typed blockers and remaining gates

| Typed outcome | Meaning at this status point |
| --- | --- |
| launcher `result=blocked` / `reason_code=i2pr-mixed-router-profile-not-wired` | The reference harness has not connected the launcher to a reference adapter; local driver validation remains separate. |
| `blocked_host_contract` | The Ubuntu 24.04 amd64, privilege, namespace, route, firewall, or cleanup prerequisite failed before a protocol claim. |
| `rejected` | Scenario, disposable state, or RouterInfo input failed bounded validation. |
| typed timeout/authentication/cleanup failure | A real execution reached that phase but failed; it must remain visible and cannot be promoted to `passed`. |

The current Plan 041 record reports `blocked_host_contract` on this host before
privileged reference setup. No authenticated Java I2P/i2pd result is claimed,
and no Plan 042 positive or negative profile has been run. Plan 042 cannot close
until the runtime driver, the DeliveryStatus exchange, all four primary IPv4
directions, typed negative/resource outcomes, sanitized evidence, and zero
residual runtime/namespace/secret state are proven. Plan 043 build-system
reproducibility and the separate Milestone 3 review remain later gates.

## Local validation completed

The bounded local implementation and documentation pass the repository gates:
workspace formatting/check/tests (216 tests), clippy with warnings denied,
rustdoc with warnings denied, dependency and runtime-boundary checks,
fixture/vector manifests, NTCP2 evidence-manifest validation, Python interop
harness tests (15 tests), and the `i2pr-interop` build/tests (6 tests). The
focused runtime suite also passes (44 tests). The Ubuntu post-install host gate
remains a typed `blocked_host_contract` here because non-interactive sudo is
unavailable; no privileged namespace or public-network attempt was made.
