# Shared bandwidth governor

The daemon's `[bandwidth]` section is disabled by default. Enabling it creates
one process-wide governor and passes that handle to the enabled SSU2 owner.
Rates are directional bytes per second; `burst_bytes` bounds the token bucket
and every single reservation. The default burst is 72_819 bytes: the smallest
value that fits one maximum NTCP2 data frame (65_537 wire bytes) above the
default 10 percent control reserve, `ceil(65_537 * 100 / 90)` (Plan 484).
Raising `control_reserve_percent` can push the required minimum above this
default; that configuration stays fail-closed at NTCP2 construction time
rather than clamping. An explicitly configured smaller burst keeps exact
semantics and is likewise rejected by the construction gate — set
`burst_bytes` explicitly to override the default. `share_percent` is retained for capacity-class
qualification only when the same governor remains healthy. It does not change
the configured rate or describe observed utilization.

The governor bounds the process waiter queue and each peer's pending requests.
Control traffic may use the reserved burst and is limited to four consecutive
reservations while data is queued. Client, transit, and floodfill traffic share
the remaining burst with rotating class service. A caller's cancellation token
owns its budget wait; stopping an owner cancels its waiters and releases their
queue slots. Queue exhaustion, oversized reservations, and cancellation are
reported as typed errors to the owning transport.

SSU2 outbound reservations cover complete datagrams immediately before socket
submission. Inbound reservations cover accepted datagram bytes before protocol
processing; they cannot throttle bytes already received by the host network
stack. The authenticated NTCP2 reader/writer adapter also accounts for framed
wire bytes, but normal-daemon NTCP2 remains disabled by its independent support
guard.

The current daemon does not publish a bandwidth tier from this configuration.
Capacity eligibility is scoped to the issuing healthy governor and is rechecked
before the private RouterInfo construction path can consume it. Floodfill role,
qualified address, and reachability proofs remain separate requirements. A
missing, stopped, or replaced shaper withdraws the eligibility value; no
`f`, `R`, tier letter, or reachability claim is inferred from idle byte
counters.
