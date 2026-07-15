# i2pd template provenance

The template targets i2pd 2.60.0 revision
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` and is rendered only in a
disposable namespace run root. Names are sourced from the pinned upstream
`contrib/i2pd.conf` sample and daemon configuration implementation; the
adapter asserts the safety-critical values before launch.

The adapter passes `--datadir run-root/reference-data` and
`--conf run-root/config/i2pd.conf`. The pinned source writes `router.info` in
the data directory and persists imported peers under `reference-data/netDb`
as `routerInfo-<I2P-base64-identity-hash>.dat`; arbitrary copied filenames are
rejected. `netid = 99` is rendered explicitly for the synthetic network.
The foreground `daemon = false` process is started in the reference namespace,
readiness is taken from bounded stdout, and SIGTERM is joined with a bounded
kill fallback.

The assertion set covers foreground operation, explicit synthetic address and
port, a zero reseed threshold with empty URLs, no UPnP, disabled SSU2, no
floodfill or transit role, and disabled unrelated client services. Any
upstream rewrite or ignored setting is a typed scenario failure. The exact
section/key spellings come from the pinned `contrib/i2pd.conf` sample.
