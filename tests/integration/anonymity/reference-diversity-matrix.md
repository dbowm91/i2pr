# Plan 305 reference diversity source matrix

Source revisions are frozen by `references.lock.toml`:

- Java I2P 2.13.0: `9134f808337b401e8e53c73734c81fab04280c9d`.
- i2pd 2.61.0: `635b013a612ff47278ef02acf8580a28e10e26c5`.

This is source review, not a claim that i2pr enforces these behaviors.

| Dimension | Java I2P 2.13.0 | i2pd 2.61.0 | Plan 305 disposition |
| --- | --- | --- | --- |
| Same RouterHash in one path | `ClientPeerSelector.selectPeers` carries selected hashes in an exclusion set; endpoint role wrappers also exclude existing tunnel endpoints. | `NetDb::GetRandomRouter(compatibleWith, ...)` excludes the compatible RouterInfo itself; path assembly carries prior peers. | Retain i2pr's structural repeated-router guard. The actual i2pr service build path still only accepts one peer today. |
| Family | `MaskedIPSet` adds RouterInfo `family` option text; source comments explicitly call verification a TODO. | `RouterInfo::IsSameFamily` compares the parsed `FamilyID`; the family subsystem verifies certificates. | Java client profile is the selected coherent target because the service profile was aligned to Java's three-hop client defaults. Family semantics and their trust difference from i2pd must remain explicit; no i2pr selection owner is wired yet. |
| Address proximity | `TunnelPoolSettings.DEFAULT_IP_RESTRICTION = 2`; `MaskedIPSet` uses the first two IPv4 bytes (/16) and first four IPv6 bytes (/32), plus advertised address ports. Client selection carries the accumulated set across hop selection when the path length is over one and local-address mode is off. | With `reservedrange` enabled by default, `NetDb::GetRandomRouter` rejects same-family or same-subnet candidates relative to `compatibleWith`. `RouterInfo::IsSameSubnet` compares IPv4 /24 and IPv6 /56 on a shared supported transport. | The masks differ. Do not synthesize a hybrid. Java's /16 and /32 rule is the selected profile; i2pd's /24 and /56 behavior is recorded as a divergence. |
| Endpoint reuse across pools | `ClientPeerSelector` uses `IBGWExcluder`, `OBEPExcluder`, and `ClosestHopExcluder` for role and pool-specific exclusions. | `TunnelPool::CreateTunnels` delegates candidate selection to NetDB and can install a custom selector; the reviewed default NetDB overload applies compatibility checks against one prior RouterInfo. | No cross-pool endpoint-reuse rule is claimed for i2pr until the candidate provider and service pool owner are connected. |
| Scarcity | `selectPeers` returns `null` when selected peers do not satisfy the configured minimum; no shorter path is returned by that branch. | Random-router selection returns no candidate when bounded searches fail their predicate. | i2pr must report typed exhaustion and never reduce the configured path length. This Plan 305 owner was not implemented. |
| Candidate ordering | Fast-pool/profile selection uses a per-pool random key and role-specific slices. | `GetRandomRouter` uses random indices and bounded nearby attempts. | Production selection must use cryptographic randomness, with injected deterministic randomness only in tests. |

Primary source locations, relative to each exact source checkout:

- Java `router/java/src/net/i2p/router/tunnel/pool/ClientPeerSelector.java` (`selectPeers`, `IBGWExcluder`, `OBEPExcluder`).
- Java `router/java/src/net/i2p/router/TunnelPoolSettings.java` (`DEFAULT_IP_RESTRICTION`).
- Java `router/java/src/net/i2p/router/util/MaskedIPSet.java` (masked addresses, ports, and family label).
- Java `router/java/src/net/i2p/router/tunnel/pool/TunnelPeerSelector.java` (`getExclude`, closest-hop exclusion).
- i2pd `libi2pd/NetDb.cpp` (`NetDb::GetRandomRouter` overload with `compatibleWith`).
- i2pd `libi2pd/RouterInfo.h` (`Address::IsSameSubnet`, `RouterInfo::IsSameFamily`).
- i2pd `libi2pd/RouterInfo.cpp` (`RouterInfo::IsSameSubnet`).
- i2pd `libi2pd/Config.cpp` (`reservedrange`, default enabled).
- i2pd `libi2pd/TunnelPool.cpp` (`TunnelPool::CreateTunnels`, default/custom selector dispatch).
