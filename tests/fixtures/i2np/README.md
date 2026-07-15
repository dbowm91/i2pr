# I2NP fixture corpus

These fixtures are locally authored, sanitized protocol bytes. They are not
live captures and contain no router identities, addresses, destinations, or
private material. The adjacent manifest records the pinned specification
revision, generation method, expected result, license note, and file hash.

The `.hex` representation is intentional so review can inspect every byte
without introducing a binary fixture dependency. Tests decode the hex after
loading it and retain the exact bytes as the ordinary regression corpus.
