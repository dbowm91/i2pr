# Plan 191 — M6 inbound destination delivery boundary (Plan 190 §6 stop boundary E)

Status: **registered executable corrective**. The corrected reply
path landed in Plan 190 succeeded locally and against exact-pinned
i2pd 2.61.0; the next demonstrated boundary is the inbound
delivery of the destination message to the i2pd-owned SAM bridge.

## 1. Goal

Resolve the Plan 190 §6 stop boundary E by ensuring the destination
message emitted on the real outbound destination tunnel (Plan 190
`external-destination-outbound = passed`) actually reaches the
i2pd-owned SAM bridge as a `DATAGRAM RECEIVED` line on the loopback
SAM socket, and the corresponding reply (DATAGRAM SEND i2pd → i2pr
through the published i2pr Standard LeaseSet2) actually reaches
i2pr's ECIES decrypt path on the real inbound destination tunnel.

This plan passes only when the four remaining destination rows
flip from `blocked` to `passed` in a fresh `run-destination.sh`
without weakening authentication, acceptance correlation, evidence
hygiene, or wire semantics.

## 2. Starting authority and observed failure

Plan 190 closed the outbound reply-path correction (the reply
path advertised on the tunneled `DatabaseLookup` is now the remote
gateway receive tunnel id, not the local endpoint id). With that
correction:

```text
installed_ob = 1
installed_ib = 1
outbound-lookup-via-tunnel cells = 1
lease-lookup-completed leases=3 published=… expires=…
ls2-publication-tunnel cells=1
destination-outbound-delivered cells=1 payload_len=27
inbound-reply-path gateway_matches_reference=true gateway_tunnel=38401 local_receive=38402 ids_distinct=true
```

The corrected lane reaches the destination outbound delivery at
i2pr and then panics in `wait_for_datagram` at
`crates/i2pr-daemon/tests/destination_tunnel_external.rs:158`
("SAM read timeout: Elapsed(())"). The reference SAM bridge never
produces a `DATAGRAM RECEIVED` line, so the test never reaches
`append_evidence(... "reference-received")` and never reaches the
`external-direct-rejected` / `external-liveness-first-test` checks
(the driver test panics first).

Per Plan 190 §6 stop conditions:

```text
A. TunnelGateway reaches i2pd but no TunnelData reaches i2pr
B. TunnelData reaches i2pr but tunnel decrypt/reassembly fails
C. I2NP DatabaseStore is recovered but LeaseSet2 parse/validation fails
D. LeaseSet2 validates/caches but destination ECIES/Garlic send fails
E. outbound destination message passes but inbound response fails   <- we stop here
```

The current boundary is E. The passing reply-path correction is
preserved; only the inbound-delivery layer is in scope here.

## 3. Root-cause classes (any of which Plan 191 may resolve)

a. **i2pd-side delivery model**: exact-pinned i2pd 2.61.0 routes
   SAM DATAGRAM deliveries through the destination's inbound tunnel
   pool. i2pr wraps the destination message in a `TunnelGateway`
   addressed to the lease's `(gateway_router, tunnel_id)` and forwards
   it through the real outbound tunnel. i2pd must unwrap the gateway
   at the inbound-tunnel endpoint and dispatch the inner Garlic
   clove to its local destination. Whether the published lease
   points at a zero-hop inbound tunnel (which i2pd routes via
   `HandleI2NPMessage`) or a real one-hop inbound tunnel (which i2pd
   routes through its `InboundTunnel::HandleTunnelDataMsg` path)
   determines whether i2pd actually delivers to the SAM bridge.

b. **i2pr-side composition**: `compose_outbound_delivery` rejects
   `tunnel_id == 0` leases and wraps everything else in a
   `TunnelGateway`. If i2pd publishes zero-hop leases for the
   SAM `inbound.length=0 outbound.length=0` session
   configuration, i2pr currently fails closed via
   `LeaseSelectionError::ZeroTunnelId` rather than sending a
   direct-encrypted payload to the gateway router. Either the test
   must use a SAM session config that produces non-zero leases, or
   i2pr must support direct delivery for `tunnel_id == 0` leases.

c. **i2pr-side inbound dispatch**: even if i2pd delivers the
   message to its SAM bridge and i2pd `DATAGRAM SEND`s a reply, i2pr
   must recover the inbound TunnelData on the corrected
   `(gateway_router, gateway_receive_tunnel)` and decrypt through
   the existing ECIES path. The Plan 188/Plan 190 inbound tunnel
   material gives i2pr a real receive id; the test must exercise
   the ECIES dispatch under that material.

This plan owns exactly one of these layers — or a documented
combination — whichever the boundary evidence first proves
necessary.

## 4. Constraints

- No authentication weakening, no unsupported crypto, no
  `LocalZeroHop`, no fake LeaseSet, no direct-transport
  substitution for any counted row.
- No production wire-format change to make a test convenient.
- The Plan 190 `InboundGatewayRoute` + `reply_path_for_inbound_route`
  surfaces stay unchanged unless the boundary evidence proves a
  defect specifically in them.
- Evidence hygiene: no parse-raw-i2pd-log, no
  `LocalZeroHop`-tagged row flipped, no timeout relaxation.
- Creator-known tunnel keys are never installed without a consumed
  reply; the Plan 187/Plan 188 install path stays.

## 5. Acceptance criteria

Plan 191 passes only when:

1. `external-reference-received` flips from `blocked` to `passed`
   via command-derived evidence; i2pd's SAM bridge observes a
   digest-matched message on the loopback SAM socket.
2. `external-destination-inbound` flips from `blocked` to `passed`
   via command-derived evidence; i2pr recovers the inbound TunnelData
   on the real inbound tunnel and decrypts through the existing
   ECIES path with the destination-payload digest equality asserted.
3. `external-direct-rejected` flips to `passed` via direct attempt
   rejection as a counted path (no synthesis).
4. `external-liveness-first-test` flips to `passed` via the
   creator-side liveness scheduler's first test succeeding during
   destination activity.
5. Existing Plan 187 / Plan 188 / Plan 190 `passed` rows remain
   `passed` after the same run; no regression on
   `external-outbound-tunnel`, `external-inbound-tunnel`,
   `external-lease-lookup-tunnel`, `external-ls2-publication-tunnel`,
   or `external-destination-outbound`.
6. Workspace/static quality floor and exact-head routine CI pass.
7. `plans/191-status.md` records exact implementation SHA, exact
   hosted CI run, exact external lane result, and the first
   newly demonstrated layer if the boundary shifts (per Plan 190
   §6 stop conditions).

If the corrected run makes more rows pass than the four listed
above, this plan may close and hand off to the deferred
`plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming pass.
If the corrected run reaches yet another layer, this plan stops
without claiming closure and the next demonstrated boundary is
recorded as a follow-up plan.

## 6. Validation commands

Focused local floor:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-tunnel -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
```

External first-family gate:

```bash
bash tests/integration/m6-interop/run-destination.sh
```

Then the repository floor used by current authority:

```bash
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

Run the existing static evidence/boundary scripts required by CI;
do not weaken them.

## 7. Stop conditions

Stop and register a narrower follow-up if after the corrected run:

- the i2pd SAM bridge observes the message but i2pr fails to
  decrypt it on the corrected inbound-gateway tuple;
- i2pr decrypts the inbound message but a later protocol step
  fails (Streaming, publication, ECIES ratchet);
- a reference-specific quirk is identified (e.g., i2pd only
  delivers via direct delivery for zero-hop SAM sessions) and a
  topology/configuration change is the documented corrective.

In each case preserve the passing reply-path correction and the
inbound-gateway tuple, and escalate only the newly demonstrated
layer.

## 8. Handoff

Plan 191 is the next executable plan. If it passes, the deferred
`plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming pass
becomes executable; Plan 189 Java I2P second-family qualification
depends on Plan 188 + Plan 191 + Streaming.
