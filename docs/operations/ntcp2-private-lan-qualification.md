# NTCP2 private-LAN qualification procedure

This procedure exercises the daemon-owned NTCP2 TCP listener and the signed
dual-transport local RouterInfo inside an operator-authorized private subnet,
against a lab peer the operator has authorized.

It is **not** a public transport activation. The profile publishes nothing,
advertises no capability, mints no reachability permit, and changes no entry
in the support inventory. It exists so that an operator with an authorized
two-host private LAN can bring up the NTCP2 transport outside the loopback
clamp and hand the resulting signed record to a lab peer by hand.

The authority for the plan that introduced this profile is
[`plans/registry.md`](../../plans/registry.md). This document is the
operator-facing procedure only; plan state is not restated here.

## 0. The one thing to understand before you run anything

**This profile is a local bind profile, not a publication permit.** Every
refusal in the configuration layer exists to keep one promise: the router may
listen on an operator-chosen RFC1918 endpoint **without** that endpoint ever
becoming a public advertisement.

Concretely, and in the order the refusals are checked:

- The address must be one RFC1918 IPv4 literal (`10.0.0.0/8`, `172.16.0.0/12`,
  `192.168.0.0/16`). Loopback, the unspecified address `0.0.0.0`, public IPv4,
  TEST-NET `192.0.2.0/24`, and every IPv6 literal are refused.
- The port must be explicit, nonzero, and at least 1024.
- The network id must be representable by the handshake width.
- The `[ssu2]` surface must be in the same `private_lan_qualification` profile
  and must itself bind an RFC1918 IPv4 address, because one RouterInfo carries
  both transports' endpoints.

None of those checks consults the network, mints a permit, or grants a
capability. The profile therefore **cannot** publish. The NetDB publication
path still requires an `Active` floodfill role **and** an in-memory
`FloodfillAdvertisementPermit`, and neither is reachable from this profile.
`[ssu2] advertise = true` is refused at parse time regardless of profile.

**If you are looking for a way to make this router reachable on the public
network, this is not it.** The profile is RFC1918-only by construction; a
public bind is refused by name.

## 1. Prepare the two-host inventory

Record, before any process launches:

| Item | Value | Notes |
| --- | --- | --- |
| Router host RFC1918 IPv4 | e.g. `192.168.10.25` | one address, on the private LAN |
| Router host NTCP2 TCP port | e.g. `44321` | at least 1024, explicit |
| Router host SSU2 UDP port | e.g. `44123` | at least 1024, explicit |
| Peer host RFC1918 IPv4 | e.g. `192.168.10.26` | same private LAN |
| Peer host ports | peer-chosen | the peer is a separate test, with its own configuration |
| Network id | e.g. `2` | must be representable by the handshake width |
| Router data directory | dedicated, temporary | the profile persists transport keys here |

The data directory is dedicated to this procedure. The NTCP2 and SSU2
transport keys are generated into it on first start and reused afterwards, so
the signed record stays stable for the life of the lab run.

## 2. The configuration surface

The profile is selected by one key on the NTCP2 surface. The SSU2 surface
must select the same profile, and both must name the same RFC1918 address.
Both keys must be written out: the NTCP2 surface's default is `disabled`, and
the SSU2 surface's default is `controlled_loopback`, so neither defaults into
this procedure.

### 2.1 Complete example

```toml
schema_version = 1

[router]
data_dir = "/absolute/path/to/private-lan-state"

[logging]
filter = "debug"

[ssu2]
enabled = true
qualification_profile = "private_lan_qualification"
bind_ipv4 = "192.168.10.25"
bind_ipv6 = ""
port = 44123
advertise = false
introducer_service = false

[transport.ntcp2]
enabled = true
qualification_profile = "private_lan_qualification"
bind_address = "192.168.10.25"
listen_port = 44321
```

Every key the profile requires is present in the two transport blocks above.
`advertise` and `introducer_service` are shown explicitly as `false` because
this procedure never publishes and never runs the introducer. The remaining
`[transport.ntcp2]` keys (`connect_timeout_ms`, `handshake_timeout_ms`,
`read_idle_timeout_ms`, `write_timeout_ms`, `queue_wait_timeout_ms`,
`drain_timeout_ms`, `max_active_links`, `max_replay_entries`, `ipv4_prefix`,
`ipv6_prefix`) keep their defaults and are not part of the profile.

### 2.2 The profile matrix

| `enabled` | `qualification_profile` | Result |
| --- | --- | --- |
| `false` | `"disabled"` | accepted; the transport stays off |
| `false` | `"controlled_loopback"` | refused — `controlled_loopback requires enabled = true` |
| `false` | `"private_lan_qualification"` | refused — `private_lan_qualification requires enabled = true` |
| `true` | `"disabled"` | refused — `enabled NTCP2 requires the explicit controlled_loopback or private_lan_qualification profile` |
| `true` | `"controlled_loopback"` | accepted; loopback bind only |
| `true` | `"private_lan_qualification"` | accepted; RFC1918 IPv4 bind only, under the rules in §3 |
| `true` | any other value | refused — `enabled NTCP2 requires the explicit controlled_loopback or private_lan_qualification profile` |

**No other profile value is accepted.** A string that is not one of the three
above is refused by name; the profile is never inferred from the bind address,
and a private-LAN bind does not select the private-LAN profile for you.

## 3. Every refusal you can hit, and what to change

Each refusal names the offending field and states the requirement. The field
name is the first half of the message; the requirement is the second.

| Field | Refusal text | What to change |
| --- | --- | --- |
| `transport.ntcp2.qualification_profile` | `controlled_loopback requires enabled = true` | set `enabled = true`, or select the `disabled` profile |
| `transport.ntcp2.qualification_profile` | `private_lan_qualification requires enabled = true` | set `enabled = true`, or select the `disabled` profile |
| `transport.ntcp2.qualification_profile` | `enabled NTCP2 requires the explicit controlled_loopback or private_lan_qualification profile` | name a profile explicitly; there is no default |
| `transport.ntcp2.qualification_profile` | `disabled NTCP2 requires the disabled profile` | use `"disabled"` when the transport is off |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an explicit RFC1918 IPv4 address literal; an empty bind address is not a private-LAN endpoint` | set `bind_address` to the host's RFC1918 literal |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an explicit RFC1918 IPv4 address literal` | the value did not parse as an IP literal; quote it |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address; loopback is not a private-LAN address` | use the host's private address, not `127.0.0.1` |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address; the unspecified address is not a private-LAN address` | use the host's private address, not `0.0.0.0` |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address (10.0.0.0/8, 172.16.0.0/12, or 192.168.0.0/16)` | the address is public IPv4 or TEST-NET; move it into RFC1918 |
| `transport.ntcp2.bind_address` | `private_lan_qualification requires an RFC1918 IPv4 address; IPv6 literals are not supported` | the profile is IPv4-only; use the IPv4 literal |
| `transport.ntcp2.listen_port` | `private_lan_qualification requires an explicit nonzero port` | `listen_port = 0` is refused; name the port |
| `transport.ntcp2.listen_port` | `private_lan_qualification requires a port of at least 1024` | use an unprivileged port |
| `network.network_id` | `private_lan_qualification requires a network ID representable by the handshake` | use a network id of at most 255 |
| `transport.ntcp2.qualification_profile` | `private_lan_qualification requires the ssu2 surface to use the private_lan_qualification profile` | set `[ssu2] qualification_profile = "private_lan_qualification"` |
| `transport.ntcp2.qualification_profile` | `private_lan_qualification requires the ssu2 surface to bind an RFC1918 IPv4 address` | set `[ssu2] bind_ipv4` to the RFC1918 literal, and `bind_ipv6 = ""` |
| `ssu2.qualification_profile` | `private_lan_qualification requires enabled=true, one RFC1918 IPv4 bind, no IPv6 bind, and an explicit nonzero port` | the SSU2 surface is not yet in the authorized shape |
| `ssu2.advertise` | `SSU2 address publication is unavailable in this milestone` | leave `advertise = false`; this profile never publishes |

The SSU2 surface has its own refusals, which are reported when it is not yet
in the authorized shape:

| Field | Refusal text | What to change |
| --- | --- | --- |
| `ssu2.qualification_profile` | `must be controlled_loopback or private_lan_qualification` | name a profile explicitly on the SSU2 surface too — it accepts `controlled_loopback` or `private_lan_qualification`, and has no `disabled` value |
| `ssu2.bind_ipv4` / `ssu2.bind_ipv6` | `must be a valid IP address or empty to disable` | the value did not parse as an IP literal |
| `ssu2.bind_ipv4` / `ssu2.bind_ipv6` | `address family does not match its bind field` | an IPv6 literal in `bind_ipv4`, or an IPv4 literal in `bind_ipv6` |
| `ssu2.bind_ipv4` / `ssu2.bind_ipv6` | `must be loopback, or RFC1918 IPv4 under private_lan_qualification` | the SSU2 bind is neither loopback nor RFC1918 IPv4 |
| `ssu2.enabled` | `SSU2 activation requires a bind address permitted by its qualification profile` | SSU2 is enabled with no usable bind |
| `ssu2.port` | `must be 0 for an ephemeral port or at least 1024` | use an unprivileged port; the profile forbids the ephemeral form |
| `ssu2.introducer_service` | `the SSU2 introducer service requires ssu2.enabled = true` | leave `introducer_service = false` for this procedure |

Note the two refusals that are **not** about the NTCP2 block: the
family-coherence rules are reported against
`transport.ntcp2.qualification_profile` even when the thing to fix is in
`[ssu2]`. Read the requirement text, not just the field name.

The `controlled_loopback` profile keeps its own refusals unchanged
(`controlled_loopback requires a loopback address`,
`controlled_loopback requires an explicit non-privileged port`, and the
network-id floor), and it still refuses a private-LAN bind. Selecting
`controlled_loopback` is not a way to reach the private LAN.

## 4. Run the authorized private profile

Create the identity once, through the CLI, exactly as an operator would:

```text
cargo run --locked -p i2pr-daemon -- identity generate --config <cfg>
cargo run --locked -p i2pr-daemon -- check-config --config <cfg>
cargo run --locked -p i2pr-daemon -- run --config <cfg>
```

`check-config` parses and normalizes without starting anything, so it is the
cheapest way to surface a refusal.

### 4.1 Confirming the router came up

Two sockets must be bound on the operator's RFC1918 endpoint:

- **NTCP2** — a TCP listener on `<bind_address>:<listen_port>`. The service
  reports readiness only after that listener is bound and the local
  RouterInfo is ready, so a running `ntcp2-router` service is the bind
  confirmation.
- **SSU2** — a UDP socket on `<bind_ipv4>:<port>`. There is no separate log
  line for it; confirm it by observing that the port is already in use.

A TCP connect to the NTCP2 port succeeding, and a second bind of the SSU2
UDP port failing with "address already in use", together confirm both
sockets. That is the same pair of observations the repository's own
environment-gated test makes
(`crates/i2pr-daemon/tests/ntcp2_private_lan_profile.rs`).

If the NTCP2 service refuses to start, it names the reason. The same gate is
applied at the composition root, where a refusal **fails daemon startup**
with the named reason; and it is applied again inside the service closure,
where a refusal degrades that one service. The `ntcp2-router` service carries
an optional startup requirement, so a refusal there costs the NTCP2 transport
and nothing else — the rest of the router keeps running. The refusal
vocabulary is the same five shapes the configuration layer already refuses,
reported individually: the transport is disabled, no explicit profile was
selected, a controlled-loopback profile was given a non-loopback bind, a
private-LAN profile was given a non-RFC1918 bind, or the port was privileged.

### 4.2 Running the repository's own check

The shipped executable has a black-box test for this profile. It is
`#[ignore]`-gated because it needs a host property:

```text
cargo test --locked -p i2pr-daemon --test ntcp2_private_lan_profile -- \
  --ignored --exact private_lan_profile_binds_the_operator_endpoint_and_advertises_nothing \
  --test-threads=1
```

On a host with no RFC1918 IPv4 address the row **fails** rather than
skipping: a missing environment must never pass silently.

## 5. Handing the signed RouterInfo to the lab peer

**The signed record is never published to the NetDB.** The operator hands it
to the lab peer **out of band**, through the lab's own private provisioning
method — never through a NetDB insert, a reseed, a file drop into a shared
public location, or a paste into a public channel.

Read the encoded bytes from the daemon's own local inspection path — the
I2PControl `RouterInfo` method's `i2p.router.info` field, over the loopback
control listener. That listener is disabled by default, so enabling
`[i2pcontrol]` and authenticating is part of this procedure. Distribute the
bytes by the lab's authorized private method. Do not copy them into the
repository, into an evidence file, or into a public claim.

The record carries the operator's two private endpoints — one SSU2 address and
one NTCP2 address — and **no capability letter**. It is the handshake
material for the lab, nothing more.

### 5.1 The endpoint disclosure, stated honestly

The private endpoint is not secret from the peers this profile talks to. Three
mechanisms disclose it, and an operator should know all of them before
starting:

1. **We disclose it when we dial.** The NTCP2 initiator's SessionConfirmed —
   its second flight — carries our signed RouterInfo. So **any peer we dial
   learns the operator's private endpoint** from the record itself. This is
   inherent to the transport, not to this profile.
2. **SSU2 discloses it to a peer that dials us.** The SSU2 responder's
   SessionCreated carries the responder's own signed RouterInfo. Because this
   profile composes **one** record carrying **both** transports' endpoints, a
   peer that completes an SSU2 handshake with this router learns the whole
   record — the SSU2 address *and* the NTCP2 address. This is the widest
   disclosure of the three, and it is a property of the SSU2 transport, not
   of the NTCP2 profile. (The NTCP2 responder behaves differently: its
   SessionCreated carries only options, and it never transmits a RouterInfo.)
3. **The peer sees it from the connection.** Any peer that opens a TCP
   connection to the NTCP2 listener sees the operator's private address as
   the connection's source address. Binding a private address is what makes
   the socket reachable on that LAN.

What is **not** disclosed: nothing is published, so no router that is not a
peer in this lab learns the record at all; and over NTCP2 a peer that only
listens learns nothing from the record beyond what the connection's own
addressing already shows.

The profile does not narrow mechanism (1), (2) or (3). It narrows *who can
reach the router at all*: RFC1918 addresses are not routable from the public
network, so the audience for all three disclosures is the private LAN.

## 6. Tear down

Stop the router, confirm both owned sockets have closed, and remove the
temporary configuration and data directory according to the host owner's
retention rules. Before any ordinary daemon run, remove the
`qualification_profile` keys entirely (or select `"disabled"` and set
`enabled = false`), and use the normal data directory. The private profile
never authorizes publication, reachability, a public support claim, or a
floodfill capability.

## 7. What this procedure does not settle

- **This is not a public transport activation.** The profile is RFC1918-only
  and publishes nothing. A private-LAN run is controlled non-loopback
  evidence and nothing more.
- **No capability is advertised.** The signed record carries no capability
  letter, the published RouterInfo capability set is unchanged, and the
  support inventory is unchanged.
- **No reachability permit is minted.** Nothing here grants the `R` bit or
  any other reachability claim.
- **A one-family result is progression evidence, not conformance evidence.**
  A private-LAN pass against a single peer family satisfies no two-family
  claim.
- **A lab result may not be promoted into a public claim.** Retain only the
  sanitized evidence the authority requires, and never retain raw
  RouterInfos, addresses, keys, identities, or peer logs.

## 8. Authority

The plan of record for this profile, its refusals, and its non-publication
posture is in [`plans/registry.md`](../../plans/registry.md). The
architecture description of the configuration surface and the activation gate
is in [`docs/architecture/i2pr-daemon.md`](../architecture/i2pr-daemon.md); the
runtime-side address scope is in
[`docs/architecture/i2pr-runtime.md`](../architecture/i2pr-runtime.md).
