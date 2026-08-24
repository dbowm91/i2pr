# Plan 124 status — passed Plan 122 corrective closure

## Authority

- Status: **`passed-plan122-corrective-closure`**.
- Closed: **2026-08-24**.
- Plan of record: [`124-m6-plan122-destination-routing-corrective-closure.md`](124-m6-plan122-destination-routing-corrective-closure.md).
- Predecessor implementation: Plan 122 (Plan 124 reopens its closure and corrects the composition defect).
- Successor: [`125-m6-streaming-corrective-and-local-closure.md`](125-m6-streaming-corrective-and-local-closure.md).

## What landed

Plan 124 corrected the concrete Plan 122 composition defect where
`i2pr_client::routing::compose_outbound_delivery()` successfully
constructed an ECIES Garlic envelope and retained it in
`OutboundDeliveryPlan.encrypted_message`, but the outbound tunnel
call was fed the plaintext standard-encoded inner I2NP `Data`
envelope. The corrected composition now:

1. Builds the inner I2NP `Data` envelope the local creator emits.
2. Builds the Garlic payload sequence (Data clove plus optional
   bundled LeaseSet2 DatabaseStore clove).
3. Hands the payload bytes to `EciesSessionManager`, which returns
   the ECIES-protected envelope (New Session or Existing).
4. Wraps the encrypted envelope in an `I2npBody::Garlic` message
   and standard-encodes the carrier. The carrier is the only byte
   stream the outbound tunnel data plane observes.
5. Forwards the encoded Garlic message through the outbound tunnel
   role with `DeliveryInstruction::Tunnel` targeting the selected
   lease's gateway and tunnel id.

`OutboundDeliveryPlan` now exposes `garlic_i2np_bytes: Vec<u8>` as
the canonical carrier the tunnel data plane must observe;
`inner_envelope_bytes` and `encrypted_message` are retained as
diagnostic evidence only.

## Code surface

- `crates/i2pr-client/src/routing.rs::compose_outbound_delivery` —
  the corrected composition.
- `crates/i2pr-client/src/routing.rs::OutboundDeliveryPlan` — adds
  `garlic_i2np_bytes: Vec<u8>` as the canonical carrier.
- `crates/i2pr-client/src/identity.rs::DestinationIdentity::static_secret_bytes`
  — accessor for the ECIES static secret; consumed by the
  dispatcher when accepting inbound New Session messages.
- `crates/i2pr-client/src/routing.rs::DestinationRouting::register_resolved_remote`
  — direct composition seam that registers a validated LeaseSet2
  into the active-remotes cache without driving the lookup state
  machine.
- `crates/i2pr-client/src/dispatch.rs::DestinationDispatcher::bind_destination_hash`
  — binds `DestinationId` to `DestinationHash`. The dispatcher
  fails closed on `UnknownDestination` without trial-decryption
  across all registered destinations.
- `crates/i2pr-client/src/dispatch.rs::DestinationDispatcher::unregister_destination`
  — atomically removes the destination and every matching hash
  binding.
- `crates/i2pr-client/src/dispatch.rs::DestinationDispatcher::lookup_local_destination`
  — looks up the owning destination through the bound hash; never
  trial-decrypts across all registered destinations.
- `crates/i2pr-client/tests/plan124_trajectory.rs` — eleven
  deterministic tests covering Phases A, B, C, D (existing-session
  carrier), E (ciphertext isolation, unregister atomically drops
  ownership), F (stale lease), and G (tampered / malformed /
  non-Garlic fault paths).
- `crates/i2pr-client/tests/plan122_trajectory.rs::plan_122_phase_f_outbound_composition_produces_delivery_plan`
  — strengthened to drive the actual composition path and assert
  the canonical carrier.

## Plan 124 master trajectory

The master Plan 124 trajectory is
`plan_124_trajectory_a_to_b_carries_garlic_through_obep`. It
drives two destination identities A and B through:

```text
A selects a non-expired B Lease2
 A compose_outbound_delivery() emits encoded I2NP Garlic through outbound tunnel
 OBEP delivers TunnelGateway to B lease gateway/id
 authenticated-router-link-bypassed-local-seam (explicit local seam)
 B inbound gateway / inbound participant / local endpoint recovers exact I2NP Garlic
 B DestinationDispatcher authenticates + decrypts the Garlic envelope
 B application queue receives exactly "hello" once
```

The only network omission is the explicit local seam after the
outbound endpoint. The local seam does not decrypt / re-encrypt /
rewrite destination payloads or tunnel identity.

## Required acceptance criteria

- [x] `compose_outbound_delivery()` feeds encoded I2NP Garlic bytes,
      not plaintext inner I2NP Data bytes, into the outbound tunnel
      role.
- [x] The ECIES message bytes are carried inside the I2NP Garlic
      body that traverses the tunnel.
- [x] The byte-identity regression
      `plan_124_phase_a_b_compose_emits_garlic_through_obep` proves
      the OBEP-recovered bytes equal the composed I2NP Garlic
      carrier and differ from the plaintext inner Data envelope.
- [x] The selected Lease2 gateway and tunnel id survive to the OBEP
      TUNNEL delivery unchanged
      (`plan_124_phase_b_obep_target_router_matches_selected_lease`).
- [x] A successful A → B New Session trajectory uses real
      destination-owned outbound and inbound tunnel roles
      (`plan_124_trajectory_a_to_b_carries_garlic_through_obep`).
- [x] The only network omission is the explicit local seam after
      outbound endpoint processing and before remote router ingress.
- [x] The local seam does not decrypt / re-encrypt / rewrite
      destination payloads or tunnel identity.
- [x] B dispatches the recovered ciphertext to B's destination
      context only.
- [x] B authenticates / decrypts before delivering exact application
      payload bytes.
- [x] Plan 119 / Plan 120 / Plan 121 functionality remains green.
- [x] No NTCP2 / SSU2 activation, SAM, I2CP socket API, HTTP / SOCKS
      proxy, Python harness, Docker, namespace, VM, or public-I2P
      work is introduced.
- [x] Workspace tests, clippy, docs, and boundary scripts are
      green.
- [x] Status authority is synchronized and Plan 123 remains
      `provisional-awaiting-plan125-correction`.

## Cross-plan status update

```text
plan_118 = closed
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-ecies-destination-session-layer
plan_122 = passed-corrected-local-destination-routing
plan_123 = provisional-awaiting-plan125-correction
plan_124 = passed-plan122-corrective-closure
plan_122_transport_boundary = authenticated-router-link-bypassed-local-seam
plan_122_external_interop = not-claimed
milestone6_local_product = not-yet-closed
next = plans/125-m6-streaming-corrective-and-local-closure.md
```

NTCP2 stays experimental and non-advertised.

## Handoff on success

Plan 124 closes. The next executable plan is
[`125-m6-streaming-corrective-and-local-closure.md`](125-m6-streaming-corrective-and-local-closure.md),
which restores the Streaming protocol-6 framing (canonical RFC 1952
gzip, no SHA-256 prefix, no custom compressed-length prefix) and the
connection-establishment state machine (no optimistic local
Established before peer SYN response), then exercises the canonical
two-destination streaming trajectory through the corrected Plan 122
destination routing.
