# Plan 183 — M6 mixed-router destination/Streaming interoperability roadmap

Status: **registered scoped roadmap authority; execution begins with Plan 184**.

Source floor: `f9f2d70ea84e3765cfcc5696c858493d517276d3`.

## 1. Why this program exists

Plan 181 reached its required stop condition after all local M10 application rows passed but the first independent-router service qualification could not leave the local destination fabric:

```text
classification = m6-mixed-router-streaming-blocker
valid independent Destination = yes
remote route                 = absent (`unknown_peer`)
Streaming established        = no
application bytes delivered  = no
```

The missing work belongs to retained Milestone 6 external interoperability, not to the HTTP/SOCKS/IRC application layer. This roadmap closes only the minimum mixed-router destination/Streaming path needed to resume Plan 181's remote HTTP and IRC rows.

## 2. Research conclusion

The repository already contains most protocol machinery required for this program. Do **not** rebuild it:

- Plan 161 proved direct SSU2 v2 sessions and authenticated I2NP exchange against i2pd in both directions.
- `i2pr-runtime::ssu2_runtime` already exposes bounded `send_i2np()` and authenticated inbound `Ssu2InboundI2np` delivery.
- `i2pr-tunnel::short` already implements the Short Tunnel Build state machine.
- `i2pr-tunnel::bridge::ShortBuildI2npBridge` is the canonical short-build-to-I2NP encoder.
- `i2pr-tunnel` already owns exploratory-pool and TunnelData roles.
- `i2pr-daemon::netdb_seam`, `outbound_lookup`, and `inbound_dispatch` contain the Plan 117 production composition seams for NetDB through exploratory tunnels.
- `i2pr-client::routing::DestinationRouting` already owns LeaseSet2 lookup state, lease selection, ECIES/Garlic construction, and outbound tunnel delivery plans.
- `i2pr-client::streaming` remains the single Streaming implementation.

The blocker is primarily **runtime composition**: normal daemon startup still rejects `ssu2.enabled = true`; the Plan 161 SSU2 runtime is not registered in the daemon graph; authenticated transport I2NP is not centrally dispatched into tunnel/NetDB/destination consumers; and the destination route has no live non-local tunnel/NetDB owner.

## 3. Reference implementations and transport lock

First family / primary debugging reference:

```text
i2pd 2.61.0
repository = PurpleI2P/i2pd
commit     = 635b013a612ff47278ef02acf8580a28e10e26c5
```

Second independent family:

```text
Java I2P 2.13.0
repository = i2p/i2p.i2p
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
```

Java I2P and I2P+ count as one family for this program. Final M6 mixed-router closure requires i2pd **and** Java I2P unless a concrete exact-pinned Java build/runtime blocker is captured and the conformance authority is explicitly revised before substitution.

Use **SSU2 v2** for the entire first program. Do not reopen NTCP2 unless a passing Plan 184 transport preflight proves SSU2 itself prevents an upper-layer criterion. SSU2 remains constrained/non-advertised; this program is not authority to publish a general public SSU2 RouterInfo.

## 4. Controlled topology

Keep the lane unprivileged and small. Run one reference family at a time:

```text
reference router (localhost)
  - exact pinned, unmodified
  - SSU2 loopback
  - floodfill enabled for the controlled NetDB dataset
  - transit enabled only so it may participate in i2pr's one-hop tunnels
  - SAM loopback service
  - no public reseed / no public-network dependency

reference-owned service Destination
  - created through the reference router's public SAM/client API
  - inbound.length=0 / outbound.length=0 where supported

i2pr
  - imports the exact signed reference RouterInfo
  - establishes one authenticated SSU2 link
  - builds genuine one-hop inbound + outbound tunnels through the reference router
  - performs NetDB, LeaseSet2, ECIES/Garlic, and Streaming through those tunnels
```

The reference Destination's zero-hop client tunnels are acceptable because the reference router owns them and this is a documented ordinary client profile. **i2pr's side must not use local zero-hop destination tunnels for counted mixed-router evidence.**

This topology intentionally avoids a third router and avoids implementing Milestone 11 transit participation in i2pr.

## 5. Current protocol requirement: tunnel liveness

Current I2P tunnel-routing guidance requires creators to test newly created tunnels; participating routers may delete tunnels that receive no traffic for roughly two minutes. The program therefore treats tunnel liveness as product behavior, not harness polish.

Plan 185 must add a bounded creator-side liveness scheduler:

- first test well before two minutes (target <=30 seconds);
- periodic paired outbound->inbound DeliveryStatus tests (target <=60 seconds while active);
- bounded consecutive-failure threshold;
- remove/replace a failed tunnel through the existing pool policy;
- one central scheduler, never one task/timer per tunnel.

## 6. Architecture lock: authenticated router-I2NP spine

Plan 184 introduces one daemon-owned transport-to-router dispatch capability:

```text
authenticated SSU2 inbound I2NP
  -> bounded decode / expiry validation
  -> classify by I2NP type
  -> Short Tunnel Build / Reply coordinator
  -> TunnelData registry
       -> recovered standard I2NP
       -> NetDB response ingestion OR destination/Garlic path
  -> required direct control messages
```

The authenticated peer identity from `Ssu2InboundI2np` must remain attached to dispatch decisions where relevant. Test-only injection must not enter the counted live route.

Outgoing router delivery uses a narrow transport-neutral capability that resolves an already-established peer link and calls the existing SSU2 `send_i2np()` path. Do not introduce per-message tasks or a second transport manager.

Keep `inbound_dispatch.rs` NetDB-focused where it already is. The new router-I2NP dispatcher sits **above** it rather than turning that helper into an unrestricted catch-all.

## 7. Execution sequence

```text
183 scoped roadmap authority
  -> 184 authenticated SSU2/I2NP runtime + reference preflight
  -> 185 live one-hop exploratory short tunnels + TunnelData + liveness
  -> 186 mixed-router RouterInfo NetDB lookup/publication
  -> 187 remote LeaseSet2 + destination ECIES/Garlic routing
  -> 188 mixed-router Streaming against i2pd
  -> 189 Java-family qualification + final M6 mixed-router closure
  -> 190 resume Plan 181 remote HTTP/IRC rows + final M10 closure
```

Each plan must close independently before the next begins. A concrete protocol defect found against an exact-pinned reference should produce one narrow corrective rather than expanding the next plan.

## 8. Layer boundaries

### Plan 184

Daemon activation and dispatch only. Enable constrained SSU2 runtime in normal supervised composition, establish exact-pinned direct sessions, and prove authenticated I2NP reaches the central dispatcher. No tunnel/NetDB/Streaming claim.

### Plan 185

Use the existing short-build bridge/state machine to build real one-hop inbound and outbound tunnels through i2pd. Prove TunnelData in both directions and current-spec tunnel testing/liveness/replacement behavior.

### Plan 186

Use the real exploratory pair for RouterInfo `DatabaseLookup`/`DatabaseStore`/`DatabaseSearchReply` and publication. Replace any placeholder floodfill-selection store in the live LeaseSet2/NetDB seam with the daemon-authoritative bounded RouterInfo store.

### Plan 187

Wire destination-owned tunnel factories/pools to the live exploratory/transport path. Resolve a reference-owned Standard LeaseSet2, validate/cache it, select a real lease, send ECIES/Garlic through i2pr's outbound tunnel, and receive/decrypt destination traffic through i2pr's inbound tunnel. No Streaming claim yet.

### Plan 188

Run the existing Streaming protocol end to end with i2pd-owned destinations in both directions. Exercise small/large/sibling connections plus ordinary bounded loss/reordering. No application proxy substitution.

### Plan 189

Repeat the required mixed-router path against exact-pinned Java I2P and close the conformance requirement for two independent router families. Keep i2pd evidence retained.

### Plan 190

Resume Plan 181 only after Plan 189 passes. Drive ordinary `curl` HTTP and the selected independent IRC client through i2pr M10 services to independently hosted reference-router destinations; close M10 only when both remote rows pass.

## 9. Global no-go rules

Throughout Plans 184–190:

- no public I2P/reseed dependency for a green result;
- no root, namespace, Docker, VM, or systemd dependency;
- no patched i2pd or Java I2P source;
- no NTCP2 reopening unless SSU2 is specifically proven to be the blocker;
- no direct client/destination payload bypass over transport;
- no fake or empty remote `EstablishedMaterial`;
- no self-composed i2pr substitution for mixed-router rows;
- no private delivery/test queue injection after counted runtime startup;
- no M11 transit implementation or claim in i2pr;
- no raw private keys or application payloads in evidence;
- no weakening the strict Short Tunnel Build decoder to match a reference bug;
- no per-message or per-tunnel task explosion;
- no broad public-router/SSU2 support claim beyond demonstrated controlled behavior.

## 10. Program acceptance

The 183 program completes only when:

1. Plans 184–188 pass against exact-pinned i2pd.
2. Plan 189 passes the same required mixed-router destination/Streaming core against Java I2P as the second independent family.
3. `specs/CONFORMANCE.md` may truthfully mark the bounded M6 mixed-router rows passed with both families named and pinned.
4. Plan 190 then resumes and passes Plan 181's remote HTTP and IRC service rows.
5. M10 final acceptance closes only through Plan 190/updated Plan 181 authority.

Until then:

```text
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## 11. Handoff

Execute **Plan 184** next. Do not start tunnel-build changes in Plan 184 unless required to expose a concrete dispatch defect; Plan 185 owns the first real tunnel build.
