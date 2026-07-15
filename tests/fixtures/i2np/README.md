# I2NP fixture corpus

These fixtures are locally authored, sanitized protocol bytes. They are not
live captures and contain no router identities, addresses, destinations, or
private material. The adjacent manifest records the positive/negative
classification, expected decoded type or typed error category, pinned
specification revision, generator and deterministic input, license note, SHA-
256 hash, and whether the bytes are local or independently produced.

The `.hex` representation is intentional so review can inspect every byte
without introducing a binary fixture dependency. The fixture-backed tests
decode and re-encode every positive entry, exercise truncation prefixes for
selected compound values, and consume every malformed entry. The corpus is
local structural evidence only; it is not a mixed-router interoperability
claim.
