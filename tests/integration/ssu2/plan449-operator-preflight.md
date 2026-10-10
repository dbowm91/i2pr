# Plan 449: private SSU2 host preflight

This is an **offline readiness check only**. It does not start i2pr or i2pd,
send UDP, test remote reachability, or qualify SSU2. The two-host run requires
separate explicit authorization and a later command that this preflight does
not provide.

## Requirements

- Two owner-approved hosts on the same isolated private IPv4 subnet.
- An unprivileged account on each host, a distinct local interface/address,
  distinct UDP test ports, and separate private data directories.
- Stock i2pd 2.61.0 at commit
  `635b013a612ff47278ef02acf8580a28e10e26c5`, built without source changes.
- A temporary JSON inventory outside the repository, readable only by its
  owner (`chmod 600 inventory.json` on POSIX). Do not commit the inventory.

The inventory schema is strict; replace every placeholder with local values:

```json
{
  "schema": "i2pr-plan449-host-inventory-v1",
  "reference_revision": "635b013a612ff47278ef02acf8580a28e10e26c5",
  "consent": {
    "owner_authorized": true,
    "scope": "isolated-private-subnet-only"
  },
  "attempt_budget": 1,
  "timeout_seconds": 120,
  "hosts": [
    {
      "role": "i2pr",
      "host_id": "<unique-32-character-random-hex-id>",
      "interface": "<local-interface-name>",
      "address": "<private-rfc1918-ipv4>",
      "prefix": 24,
      "udp_port": 19001,
      "data_dir": "/absolute/private/i2pr-plan449"
    },
    {
      "role": "i2pd",
      "host_id": "<different-32-character-random-hex-id>",
      "interface": "<local-interface-name>",
      "address": "<private-rfc1918-ipv4>",
      "prefix": 24,
      "udp_port": 19002,
      "data_dir": "/absolute/private/i2pd-plan449"
    }
  ]
}
```

The same transient inventory is used on both hosts, with the corresponding
`--role`. Host-specific values remain local and are never placed in evidence.
Use the exact reference checkout and binary paths for that host:

```sh
python3 scripts/plan449-ssu2-preflight.py \
  --manifest /private/path/inventory.json \
  --role i2pr \
  --reference-source /private/path/i2pd-source \
  --i2pd-binary /private/path/i2pd
```

Run with `--role i2pd` on the reference host. A successful result means only
that the local interface/address, route source, UDP bind, data-directory
permissions, owner consent, and exact reference source/binary checks passed.
The UDP route probe uses a connected UDP socket to ask the local kernel for a
route and sends no datagram. A missing peer, invalid host, or unsupported
interface probe fails closed with a sanitized reason code.

No live SSU2 attempt is authorized by this checklist. The independent two-host
lane must separately prove authenticated bidirectional sessions, I2NP delivery,
identity persistence, withdrawal/restart behavior, negative controls, and
bounded cleanup before Plan 431 can advance.
