# SSU2 private-LAN qualification procedure

This procedure exercises the daemon-owned SSU2 socket and its signed local
RouterInfo inside an operator-authorized private subnet. It does not qualify
public reachability, production peer selection, RouterInfo publication, or
`R`/`f` capability claims. Plan 431 remains the authority for independent
two-host interoperation.

## Prepare the two-host inventory

Obtain authorization from both host owners for one isolated RFC1918 subnet,
one bounded attempt, and a timeout no longer than five minutes. Assign distinct
`i2pr` and pinned-i2pd roles, each with an interface-local IPv4 address, prefix,
unused UDP port, distinct host identifier, and a new private data directory
outside the repository. Keep the inventory outside the repository with mode
`0600`; the preflight refuses repository paths and group/world-readable files.

Use the schema and exact reference revision accepted by
`python3 scripts/plan449-ssu2-preflight.py --help`. Run the preflight on each
host with the same inventory. It checks the assigned local address, interface,
route-selected source address, UDP bind availability, distinct endpoints,
data-directory properties, and the unmodified i2pd 2.61.0 pin. It sends no
datagram and does not start either router. Do not share host addresses or
identity-bearing logs in repository evidence.

## Run only the authorized private profile

On the i2pr host, use a temporary config with:

```toml
[ssu2]
enabled = true
qualification_profile = "private_lan_qualification"
bind_ipv4 = "<this-host RFC1918 address>"
bind_ipv6 = ""
port = <this-host's assigned UDP port>
advertise = false
introducer_service = false
```

Use the inventory's dedicated data directory. The profile rejects wildcard,
non-RFC1918, nonlocal, IPv6, missing/privileged/ephemeral-port, and disabled
binds. The daemon signs the endpoint reported by the bound socket before
readiness and preserves the router and transport identities in that data
directory. Verify the resulting local RouterInfo only through the daemon's
sanitized local inspection path; distribute its encoded bytes to the other
host only through the separately authorized lab's private provisioning method.
Do not publish it to public NetDB.

Run the exact-pinned i2pd peer with its own inventory data directory and
private endpoint. Collect only the sanitized stage/count evidence required by
Plan 431: matching authenticated sessions, bidirectional I2NP delivery,
shutdown, and restart with stable identity. A successful local bind or signed
RouterInfo alone is not an interoperation result. Stop on the first failed
stage; do not retry an unchanged fixture.

## Tear down

Stop both processes, confirm their owned listeners have closed, and retain only
the sanitized evidence required by the plan. Remove the temporary config and
inventory according to the host owners' retention rules. Before any ordinary
daemon run, set `qualification_profile = "controlled_loopback"` (or remove the
key), disable SSU2 unless explicitly needed for a separate controlled test,
and use the normal data directory. The private profile never authorizes
publication, reachability, public support claims, or floodfill capability.
