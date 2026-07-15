# i2pd template provenance

The template targets i2pd 2.60.0 revision `f618e41` and is rendered only in a
disposable namespace run root. Names are sourced from the pinned upstream
`contrib/i2pd.conf` sample and daemon configuration implementation; the
adapter asserts the safety-critical values before launch.

The assertion set covers foreground operation, explicit synthetic address and
port, a zero reseed threshold with empty URLs, no UPnP, disabled SSU2, no
floodfill or transit role, and disabled unrelated client services. Any
upstream rewrite or ignored setting is a typed scenario failure. The exact
section/key spellings come from the pinned `contrib/i2pd.conf` sample.
